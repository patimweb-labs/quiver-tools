// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Fassade aus [`crate::Anmeldung`]: Speicher → Refresh-Token → Browser.
//!
//! Regeln (SPEC.md §2): Access-Token nur im Speicher, 60 s vor Ablauf als abgelaufen,
//! Refresh-Token im Tresor (mit Rotation), höchstens **ein** Anmeldevorgang gleichzeitig.

use std::sync::{Arc, Mutex};

use oauth2::basic::BasicErrorResponseType;
use oauth2::{RefreshToken, RequestTokenError};

use crate::fluss::{self, Ergebnis, FlussFehler, OauthClient, Tokenpaar};
use crate::{AnmeldeFehler, Anmeldung, Browser, Konfig, Token, Tresor};

/// Anmeldung an Entra ID über den Browser.
///
/// Klonbar: alle Klone teilen Speicher, Tresor und den laufenden Vorgang.
pub struct EntraAnmeldung<T: Tresor, B: Browser> {
    kern: Arc<Kern<T, B>>,
}

impl<T: Tresor, B: Browser> Clone for EntraAnmeldung<T, B> {
    fn clone(&self) -> Self {
        Self {
            kern: Arc::clone(&self.kern),
        }
    }
}

struct Kern<T: Tresor, B: Browser> {
    konfig: Konfig,
    tresor: T,
    browser: B,
    client: OauthClient,
    http: reqwest::Client,
    /// `<tenant>/<client_id>` – Schlüssel im Tresor.
    schluessel: String,
    /// Von außen gereicht (`M365_ACCESS_TOKEN`, später quiver): nie erneuern.
    festes_token: Option<String>,
    /// Gültiges Access-Token + Ablauf, nur im Arbeitsspeicher.
    zwischen: Mutex<Option<Tokenpaar>>,
    /// Der eine laufende Anmeldevorgang.
    vorgang: Mutex<Option<Vorgang>>,
}

/// Ein laufender oder gerade fertiger Browser-Vorgang. Beliebig viele Aufrufe
/// hängen an demselben `watch`, damit nur ein Browser aufgeht.
struct Vorgang {
    rx: tokio::sync::watch::Receiver<Option<Ergebnis>>,
}

impl<T: Tresor + 'static, B: Browser + 'static> EntraAnmeldung<T, B> {
    /// Wie [`EntraAnmeldung::mit_festem_token`], aber `M365_ACCESS_TOKEN` aus der
    /// Umgebung gilt, wenn gesetzt.
    pub fn neu(konfig: Konfig, tresor: T, browser: B) -> anyhow::Result<Self> {
        let festes_token = std::env::var("M365_ACCESS_TOKEN")
            .ok()
            .map(|roh| roh.trim().to_string())
            .filter(|gekuerzt| !gekuerzt.is_empty());
        Self::mit_festem_token(konfig, tresor, browser, festes_token)
    }

    /// Mit ausdrücklich gereichtem Token: dann immer [`Token::Bereit`], ohne Netz,
    /// ohne Browser, ohne Tresor (SPEC.md §2 „Später“).
    pub fn mit_festem_token(
        konfig: Konfig,
        tresor: T,
        browser: B,
        festes_token: Option<String>,
    ) -> anyhow::Result<Self> {
        let client = fluss::client_bauen(&konfig)?;
        let http = reqwest::Client::builder()
            // Kein Folgen von Weiterleitungen (SSRF-Hinweis von oauth2).
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let schluessel = format!("{}/{}", konfig.tenant, konfig.client_id);

        Ok(Self {
            kern: Arc::new(Kern {
                konfig,
                tresor,
                browser,
                client,
                http,
                schluessel,
                festes_token,
                zwischen: Mutex::new(None),
                vorgang: Mutex::new(None),
            }),
        })
    }
}

impl<T: Tresor + 'static, B: Browser + 'static> Kern<T, B> {
    fn schluessel(&self) -> &str {
        &self.schluessel
    }

    /// Gültiges Token aus dem Speicher, sonst `None`.
    fn aus_dem_speicher(&self) -> Option<String> {
        let fach = self
            .zwischen
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner());
        match fach.as_ref() {
            Some(paar) if paar.gilt_noch() => Some(paar.access.clone()),
            _ => None,
        }
    }

    /// Legt das Paar ab und gibt das Access-Token zurück.
    fn uebernehmen(&self, paar: Tokenpaar) -> Token {
        let access = paar.access.clone();
        {
            let mut fach = self
                .zwischen
                .lock()
                .unwrap_or_else(|vergiftet| vergiftet.into_inner());
            *fach = Some(paar);
        }
        Token::Bereit(access)
    }

    /// Neues Refresh-Token sofort sichern (Rotation!), damit ein Absturz es nicht verliert.
    fn refresh_sichern(&self, paar: &Tokenpaar) {
        if let Some(refresh) = &paar.refresh {
            if let Err(fehler) = self.tresor.schreiben(self.schluessel(), refresh) {
                tracing::warn!("Refresh-Token ließ sich nicht sichern: {fehler}");
            }
        }
    }

    /// Refresh-Token gegen ein neues Paar tauschen (grant_type=refresh_token).
    async fn erneuern(&self, refresh: &str) -> Ergebnis {
        tracing::info!("Token wird erneuert");
        let antwort = self
            .client
            .exchange_refresh_token(&RefreshToken::new(refresh.to_string()))
            .request_async(&self.http)
            .await;

        match antwort {
            Ok(antwort) => {
                let paar = fluss::aus_antwort(&antwort).map_err(|fehler| {
                    FlussFehler::Sonstiges(format!("Token-Antwort unbrauchbar: {fehler}"))
                })?;
                tracing::info!("Token erneuert");
                Ok(paar)
            }
            Err(RequestTokenError::ServerResponse(antwort))
                if matches!(antwort.error(), BasicErrorResponseType::InvalidGrant) =>
            {
                tracing::info!("Refresh-Token abgelehnt (invalid_grant)");
                Err(FlussFehler::RefreshUngueltig)
            }
            Err(fehler) => Err(FlussFehler::Sonstiges(format!("{fehler}"))),
        }
    }

    /// Der Browser-Ablauf, wie er im Hintergrund läuft.
    async fn browser_ablauf(&self) -> Ergebnis {
        let ergebnis =
            fluss::browser_anmeldung(&self.client, &self.http, &self.konfig, &self.browser).await;
        if let Ok(paar) = &ergebnis {
            self.refresh_sichern(paar);
        }
        ergebnis
    }
}

impl<T: Tresor + 'static, B: Browser + 'static> EntraAnmeldung<T, B> {
    /// Anmelden oder auf den laufenden Vorgang warten (höchstens `Konfig::warten`).
    async fn browser_oder_warten(&self) -> Result<Token, AnmeldeFehler> {
        let mut rx = {
            let mut fach = self
                .kern
                .vorgang
                .lock()
                .unwrap_or_else(|vergiftet| vergiftet.into_inner());

            // Ergebnis eines früheren Vorgangs liegt bereit → jetzt verbrauchen.
            let fertig = fach
                .as_ref()
                .and_then(|vorgang| (*vorgang.rx.borrow()).clone());
            if let Some(ergebnis) = fertig {
                *fach = None;
                drop(fach);
                return match ergebnis {
                    Ok(paar) => Ok(self.kern.uebernehmen(paar)),
                    Err(fehler) => Err(fehler.into()),
                };
            }

            match fach.as_ref() {
                Some(vorgang) => vorgang.rx.clone(),
                None => {
                    let (senden, rx) = tokio::sync::watch::channel(None);
                    *fach = Some(Vorgang { rx: rx.clone() });

                    let kern = Arc::clone(&self.kern);
                    tokio::spawn(async move {
                        let ergebnis = kern.browser_ablauf().await;
                        let _ = senden.send(Some(ergebnis));
                    });

                    rx
                }
            }
        };

        let gewartet =
            tokio::time::timeout(self.kern.konfig.warten, rx.wait_for(|wert| wert.is_some())).await;

        let ergebnis = match gewartet {
            Ok(Ok(fach)) => {
                let ergebnis = (*fach).clone();
                drop(fach);
                match ergebnis {
                    Some(ergebnis) => ergebnis,
                    None => {
                        return Err(AnmeldeFehler::Sonstiges(anyhow::anyhow!(
                            "Anmeldevorgang ohne Ergebnis beendet"
                        )))
                    }
                }
            }
            Ok(Err(_)) => {
                // Sender weg, ohne etwas zu melden: Vorgang verworfen.
                let mut fach = self
                    .kern
                    .vorgang
                    .lock()
                    .unwrap_or_else(|vergiftet| vergiftet.into_inner());
                *fach = None;
                return Err(AnmeldeFehler::Sonstiges(anyhow::anyhow!(
                    "Anmeldevorgang abgebrochen"
                )));
            }
            // Zeit um: der Vorgang läuft weiter, der nächste Aufruf erbt das Ergebnis.
            Err(_) => return Ok(Token::WartetAufBrowser),
        };

        {
            let mut fach = self
                .kern
                .vorgang
                .lock()
                .unwrap_or_else(|vergiftet| vergiftet.into_inner());
            *fach = None;
        }

        match ergebnis {
            Ok(paar) => Ok(self.kern.uebernehmen(paar)),
            Err(fehler) => Err(fehler.into()),
        }
    }
}

impl<T: Tresor + 'static, B: Browser + 'static> Anmeldung for EntraAnmeldung<T, B> {
    async fn access_token(&self) -> Result<Token, AnmeldeFehler> {
        // 1. Von außen gereicht: nie erneuern, nie Browser, nie Tresor.
        if let Some(festes) = &self.kern.festes_token {
            return Ok(Token::Bereit(festes.clone()));
        }

        // 2. Aus dem Speicher.
        if let Some(access) = self.kern.aus_dem_speicher() {
            return Ok(Token::Bereit(access));
        }

        // 3. Refresh-Token aus dem Tresor.
        let gespeichert = match self.kern.tresor.lesen(self.kern.schluessel()) {
            Ok(gespeichert) => gespeichert,
            Err(fehler) => {
                tracing::warn!("Tresor nicht lesbar, Anmeldung im Browser: {fehler}");
                None
            }
        };

        if let Some(refresh) = gespeichert {
            match self.kern.erneuern(&refresh).await {
                Ok(paar) => {
                    self.kern.refresh_sichern(&paar);
                    return Ok(self.kern.uebernehmen(paar));
                }
                Err(FlussFehler::RefreshUngueltig) => {
                    // Widerrufen/abgelaufen: weg damit, dann neu anmelden.
                    self.kern.tresor.loeschen(self.kern.schluessel())?;
                }
                Err(fehler) => return Err(fehler.into()),
            }
        }

        // 4. Browser.
        self.browser_oder_warten().await
    }

    /// Nach einem 401 von Graph: nur den Speicher leeren, das Refresh-Token bleibt.
    fn verwerfen(&self) {
        let mut fach = self
            .kern
            .zwischen
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner());
        *fach = None;
    }

    /// Abmelden: Refresh-Token löschen, Speicher leeren.
    fn abmelden(&self) -> anyhow::Result<()> {
        self.kern.tresor.loeschen(self.kern.schluessel())?;
        self.verwerfen();
        tracing::info!("Abgemeldet");
        Ok(())
    }
}

impl From<FlussFehler> for AnmeldeFehler {
    fn from(fehler: FlussFehler) -> Self {
        match fehler {
            FlussFehler::StateFalsch => AnmeldeFehler::StateFalsch,
            FlussFehler::Abgelehnt(text) => AnmeldeFehler::Abgelehnt(text),
            FlussFehler::LauscherAbgelaufen => AnmeldeFehler::Abgelaufen,
            FlussFehler::RefreshUngueltig => {
                AnmeldeFehler::Abgelehnt("Refresh-Token abgelehnt".to_string())
            }
            FlussFehler::Sonstiges(text) => AnmeldeFehler::Sonstiges(anyhow::anyhow!("{text}")),
        }
    }
}
