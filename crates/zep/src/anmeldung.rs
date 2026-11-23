// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Angemeldet bleiben (Aufgabe 06, Punkte 1–6).
//!
//! Der eine Gegenstand, den dieses Werkzeug selbst verwaltet: die Anmeldung am
//! ZEP-MCP.
//!
//! - **Client-Registrierung (DCR)** einmal je Installation; die `client_id`
//!   liegt im Tresor.
//! - **Refresh-Token** im Schlüsselbund unter der MCP-Adresse, **Access-Token**
//!   nur im Arbeitsspeicher. Ein rotiertes Refresh-Token wird sofort gesichert.
//! - **Hintergrund-Erneuerung**: solange der Prozess läuft, wird das
//!   Access-Token bei 80 % seiner Laufzeit erneuert – damit das Refresh-Token
//!   nicht aus Untätigkeit verfällt.
//! - **Routine** mit Benutzer und Passwort (kein Browser) als erster Weg;
//!   **Browser** nur als Notfall (`anmelden --browser`).
//! - **Sperrschutz**: je Fehlschlag der Routine genau ein Versuch, danach ist
//!   die Routine gesperrt (in einer Datei, also auch über Neustarts). Frei wird
//!   sie erst durch eine erfolgreiche Anmeldung über den Knopf.
//!
//! Kein Wert von hier erscheint in einer Meldung: weder Passwort noch Token.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::Instant;
use url::Url;

use crate::browser::{Browser, KeinBrowser};
use crate::metadaten::{self, Metadaten};
use crate::oauth::{self, TokenAntwort};
use crate::pkce::{self, Pkce};
use crate::routine::Routine;
use crate::sperre::Sperre;
use crate::tresor::{schluessel_client, schluessel_refresh, Schluesselbund, Tresor};
use crate::werkzeug::ZepKonfig;

/// Der Satz, den jedes Werkzeug sagt, wenn keine Anmeldung möglich ist
/// (Aufgabe 06, Punkt 6).
pub const HINWEIS_ANMELDUNG: &str =
    "ZEP-Anmeldung nötig – bitte im quiver-Fenster unter Tools → ZEP auf „Anmelden“ klicken.";

/// Wie lange eine Anfrage an einen OAuth-Endpunkt höchstens dauert.
pub const ZEITLIMIT: Duration = Duration::from_secs(30);

/// Was bei der Anmeldung schiefgehen kann.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AnmeldeFehler {
    /// Es gibt keine Anmeldung und die Routine darf nicht (mehr) laufen.
    #[error("{HINWEIS_ANMELDUNG}")]
    NichtAngemeldet,
    /// Die Routine ist nach einem Fehlschlag gesperrt.
    #[error("Die Anmelde-Routine ist gesperrt: {0} {HINWEIS_ANMELDUNG}")]
    Gesperrt(String),
    /// Das Refresh-Token wurde abgelehnt (`invalid_grant`): neu anmelden.
    #[error("Das Refresh-Token wurde abgelehnt – eine neue Anmeldung ist nötig.")]
    RefreshAbgelehnt,
    /// Alles andere, als Text ohne Geheimnisse.
    #[error("{0}")]
    Fehlgeschlagen(String),
}

impl AnmeldeFehler {
    /// Ist die Routine gesperrt?
    pub fn ist_gesperrt(&self) -> bool {
        matches!(self, AnmeldeFehler::Gesperrt(_))
    }

    /// Hat der Token-Endpunkt das Refresh-Token abgelehnt (`invalid_grant`)?
    pub fn ist_invalid_grant(&self) -> bool {
        matches!(self, AnmeldeFehler::RefreshAbgelehnt)
    }
}

/// Der Anmeldezustand, wie `quiver-zep status` ihn meldet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Angemeldet { benutzer: String },
    NichtAngemeldet,
    Gesperrt { grund: String },
}

impl Status {
    /// Der Text für die Ausgabe.
    pub fn text(&self) -> String {
        match self {
            Status::Angemeldet { benutzer } => format!("Angemeldet als {benutzer}."),
            Status::NichtAngemeldet => HINWEIS_ANMELDUNG.to_string(),
            Status::Gesperrt { grund } => {
                format!("Die Anmelde-Routine ist gesperrt: {grund} {HINWEIS_ANMELDUNG}")
            }
        }
    }

    /// Ist eine Anmeldung vorhanden?
    pub fn ist_angemeldet(&self) -> bool {
        matches!(self, Status::Angemeldet { .. })
    }
}

/// Das Ergebnis eines Anmeldeversuchs – mit Bericht, auch wenn er scheitert.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutineLauf {
    /// Was der Ablauf Schritt für Schritt gesehen hat (ohne Werte).
    pub bericht: Vec<String>,
    /// Warum es nicht geklappt hat.
    pub fehler: Option<String>,
}

impl RoutineLauf {
    /// Ist der Versuch gelungen?
    pub fn gelungen(&self) -> bool {
        self.fehler.is_none()
    }
}

/// Ein Access-Token samt Ablauf – nur im Arbeitsspeicher. `Debug` schweigt.
#[derive(Clone)]
struct Gespeichertes {
    access: String,
    ablauf: Instant,
    /// Wie lange das Token insgesamt gilt (für die 80-%-Regel).
    laufzeit: Duration,
}

impl std::fmt::Debug for Gespeichertes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gespeichertes")
            .field("access", &"<geheim>")
            .field("ablauf", &self.ablauf)
            .field("laufzeit", &self.laufzeit)
            .finish()
    }
}

impl Gespeichertes {
    /// Gilt das Token noch, ohne die 20 % Reserve?
    fn gilt_noch(&self) -> bool {
        Self::gueltig_bis(self.ablauf, self.laufzeit, Instant::now())
    }

    /// Reine Regel, damit sie ohne echtes Warten prüfbar ist: erneuert wird bei
    /// 80 % der Laufzeit, also wenn weniger als ein Fünftel übrig ist.
    fn gueltig_bis(ablauf: Instant, laufzeit: Duration, jetzt: Instant) -> bool {
        jetzt + laufzeit / 5 < ablauf
    }
}

/// Die Anmeldung eines ZEP-Zugangs. Klonbar; alle Klone teilen sich den Zustand.
#[derive(Clone)]
pub struct Anmeldung {
    inner: Arc<Inner>,
}

struct Inner {
    konfig: ZepKonfig,
    http: reqwest::Client,
    tresor: Arc<dyn Tresor>,
    sperre: Sperre,
    browser: Arc<dyn Browser>,
    /// Einmal gelesen, dann gemerkt.
    metadaten: Mutex<Option<Metadaten>>,
    client_id: Mutex<Option<String>>,
    /// Das gültige Access-Token (nur hier, nie in Tresor oder Datei).
    token: Mutex<Option<Gespeichertes>>,
    /// Nur ein Anmeldevorgang gleichzeitig.
    vorgang: tokio::sync::Mutex<()>,
}

impl std::fmt::Debug for Anmeldung {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Anmeldung")
            .field("url", &self.inner.konfig.url)
            .field("benutzer", &self.inner.konfig.benutzer)
            .field("gesperrt", &self.inner.sperre.grund())
            .finish_non_exhaustive()
    }
}

impl Anmeldung {
    /// Eine Anmeldung über einer Konfiguration.
    pub fn neu(konfig: ZepKonfig, tresor: Arc<dyn Tresor>, browser: Arc<dyn Browser>) -> Anmeldung {
        let http = reqwest::Client::builder()
            // Weiterleitungen selbst verfolgen – die Rückleitung an den
            // Loopback darf uns nicht entgehen.
            .redirect(reqwest::redirect::Policy::none())
            .timeout(ZEITLIMIT)
            .user_agent(concat!("quiver-zep/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("reqwest-Client baut");
        let sperre = Sperre::im_ordner(&konfig.zustand);
        Anmeldung {
            inner: Arc::new(Inner {
                konfig,
                http,
                tresor,
                sperre,
                browser,
                metadaten: Mutex::new(None),
                client_id: Mutex::new(None),
                token: Mutex::new(None),
                vorgang: tokio::sync::Mutex::new(()),
            }),
        }
    }

    /// Eine Anmeldung mit echten Diensten: Schlüsselbund, Standardbrowser aus.
    pub fn aus_konfig(konfig: ZepKonfig) -> Anmeldung {
        Anmeldung::neu(
            konfig,
            Arc::new(Schluesselbund::neu()),
            Arc::new(KeinBrowser::neu()),
        )
    }

    /// Der Benutzer, der im Zugang steht.
    pub fn benutzer(&self) -> &str {
        &self.inner.konfig.benutzer
    }

    /// Die MCP-Adresse des Zugangs.
    pub fn url(&self) -> &str {
        &self.inner.konfig.url
    }

    /// Der Grund, falls die Routine gesperrt ist.
    pub fn sperrgrund(&self) -> Option<String> {
        self.inner.sperre.grund()
    }

    /// Der Anmeldezustand – ohne Netz und ohne Anmeldeversuch.
    pub fn status(&self) -> Status {
        if let Some(grund) = self.inner.sperre.grund() {
            return Status::Gesperrt { grund };
        }
        let hat_token = self
            .inner
            .token
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner())
            .as_ref()
            .is_some_and(|token| token.gilt_noch());
        let hat_refresh = self
            .inner
            .tresor
            .lesen(&schluessel_refresh(&self.inner.konfig.url))
            .ok()
            .flatten()
            .is_some_and(|wert| !wert.trim().is_empty());
        if hat_token || hat_refresh {
            Status::Angemeldet {
                benutzer: self.inner.konfig.benutzer.clone(),
            }
        } else {
            Status::NichtAngemeldet
        }
    }

    /// Ein gültiges Access-Token: aus dem Speicher, per Refresh-Token oder –
    /// wenn beides nicht geht – über die Routine (ein Versuch, dann gesperrt).
    pub async fn access_token(&self) -> Result<String, AnmeldeFehler> {
        if let Some(access) = self.aus_dem_speicher() {
            return Ok(access);
        }

        if self.refresh_token().is_some() {
            match self.erneuern_aus_tresor().await {
                Ok(()) => {
                    if let Some(access) = self.aus_dem_speicher() {
                        return Ok(access);
                    }
                }
                Err(fehler) if fehler.ist_invalid_grant() => {
                    // Widerrufen oder abgelaufen: weg damit, dann die Routine.
                    tracing::info!("Refresh-Token abgelehnt (invalid_grant) – Routine nötig");
                    self.refresh_loeschen();
                }
                Err(fehler) => return Err(fehler),
            }
        }

        // Die Routine läuft nur, wenn kein Fehlversuch sie gesperrt hat.
        if let Some(grund) = self.inner.sperre.grund() {
            return Err(AnmeldeFehler::Gesperrt(grund));
        }

        // Nur eine Routine gleichzeitig; ein zweiter Aufruf übernimmt das
        // Ergebnis des ersten, statt sich ein zweites Mal anzumelden.
        let lauf = self.routine_lauf_intern(true).await;
        if let Some(fehler) = lauf.fehler {
            return Err(AnmeldeFehler::Fehlgeschlagen(fehler));
        }
        self.aus_dem_speicher()
            .ok_or(AnmeldeFehler::NichtAngemeldet)
    }

    /// Das Access-Token verwerfen (nach einem 401); das Refresh-Token bleibt.
    pub fn verwerfen(&self) {
        let mut fach = self
            .inner
            .token
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner());
        *fach = None;
    }

    /// Abmelden: Refresh-Token löschen, Speicher leeren, Sperre aufheben.
    pub fn abmelden(&self) -> anyhow::Result<()> {
        self.inner
            .tresor
            .loeschen(&schluessel_refresh(&self.inner.konfig.url))?;
        self.verwerfen();
        self.inner.sperre.loesen()?;
        tracing::info!("Abgemeldet");
        Ok(())
    }

    /// Die Routine **einmal** ausführen und jeden Schritt berichten.
    ///
    /// Der Weg des Knopfes: er darf auch dann laufen, wenn die Routine gesperrt
    /// ist – eine erfolgreiche Anmeldung hebt die Sperre auf. Ein Fehlschlag
    /// sperrt sie wieder.
    pub async fn routine_lauf(&self) -> RoutineLauf {
        self.routine_lauf_intern(false).await
    }

    /// Die Routine hinter der einen Sperre. `nur_wenn_noetig` gilt für den
    /// automatischen Weg: hat ein anderer Aufruf inzwischen angemeldet, wird
    /// kein zweites Mal angefragt (sonst zwei Logins je Fehlversuch).
    async fn routine_lauf_intern(&self, nur_wenn_noetig: bool) -> RoutineLauf {
        let mut bericht = Vec::new();
        let ergebnis = {
            let _wache = self.inner.vorgang.lock().await;
            if nur_wenn_noetig && self.aus_dem_speicher().is_some() {
                return RoutineLauf {
                    bericht: vec!["Inzwischen angemeldet – keine zweite Anfrage.".to_string()],
                    fehler: None,
                };
            }
            self.routine_innen(&mut bericht).await
        };
        self.verdauen(ergebnis, bericht)
    }

    /// Der Notfall: Anmeldung über den Browser mit Rückleitung an den Loopback.
    pub async fn browser_lauf(&self) -> RoutineLauf {
        let mut bericht = Vec::new();
        let ergebnis = {
            let _wache = self.inner.vorgang.lock().await;
            self.browser_innen(&mut bericht).await
        };
        self.verdauen(ergebnis, bericht)
    }

    /// Erneuern, wenn das Access-Token bei 80 % seiner Laufzeit angekommen ist.
    ///
    /// Meldet, ob erneuert wurde. Ohne Refresh-Token und ohne Token passiert
    /// nichts: die Hintergrund-Erneuerung startet nie eine Anmeldung.
    pub async fn erneuern_wenn_noetig(&self) -> Result<bool, AnmeldeFehler> {
        let faellig = {
            let fach = self
                .inner
                .token
                .lock()
                .unwrap_or_else(|vergiftet| vergiftet.into_inner());
            match fach.as_ref() {
                Some(token) => !token.gilt_noch(),
                None => false,
            }
        };
        if !faellig {
            return Ok(false);
        }
        self.erneuern_aus_tresor().await?;
        Ok(true)
    }

    /// Die Hintergrund-Erneuerung starten: ein Takt, ein Versuch, kein Login.
    pub fn starte_hintergrund(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let inner = Arc::clone(self);
        let takt = inner.inner.konfig.takt;
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(takt).await;
                match inner.erneuern_wenn_noetig().await {
                    Ok(true) => tracing::info!("Access-Token im Hintergrund erneuert"),
                    Ok(false) => {}
                    Err(fehler) => {
                        tracing::debug!(%fehler, "Hintergrund-Erneuerung nicht möglich")
                    }
                }
            }
        })
    }

    // ------------------------------------------------------------ innen

    fn verdauen(
        &self,
        ergebnis: Result<(), AnmeldeFehler>,
        mut bericht: Vec<String>,
    ) -> RoutineLauf {
        match ergebnis {
            Ok(()) => {
                if let Err(fehler) = self.inner.sperre.loesen() {
                    tracing::warn!("Sperrzustand ließ sich nicht löschen: {fehler}");
                }
                bericht.push("Angemeldet – die Sperre der Routine ist aufgehoben.".to_string());
                RoutineLauf {
                    bericht,
                    fehler: None,
                }
            }
            Err(fehler) => {
                let grund = sperrgrund(&fehler);
                if let Err(sperrfehler) = self.inner.sperre.sperren(&grund) {
                    tracing::warn!("Sperrzustand ließ sich nicht schreiben: {sperrfehler}");
                }
                bericht.push(format!("Fehlschlag: {fehler}"));
                bericht.push(
                    "Die Routine ist jetzt gesperrt: je Fehlschlag ein Versuch. Eine erfolgreiche \
                     Anmeldung hebt die Sperre wieder auf."
                        .to_string(),
                );
                RoutineLauf {
                    bericht,
                    fehler: Some(fehler.to_string()),
                }
            }
        }
    }

    async fn routine_innen(&self, bericht: &mut Vec<String>) -> Result<(), AnmeldeFehler> {
        let metadaten = self.metadaten().await?;
        bericht.push(format!(
            "Authorization-Server {} (Scope {})",
            kurz(&metadaten.authorization_endpoint()),
            metadaten.scope
        ));
        let client_id = self.client_id().await?;
        bericht.push(format!("Client-Registrierung: {client_id}"));

        let mut routine = Routine::neu(
            &self.inner.http,
            &self.inner.konfig.benutzer,
            &self.inner.konfig.passwort,
        );
        let ergebnis = routine.ausfuehren(&metadaten, &client_id).await;
        bericht.extend(routine.in_bericht());
        let antwort =
            ergebnis.map_err(|fehler| AnmeldeFehler::Fehlgeschlagen(fehler.to_string()))?;
        self.uebernehmen(antwort);
        Ok(())
    }

    async fn browser_innen(&self, bericht: &mut Vec<String>) -> Result<(), AnmeldeFehler> {
        let metadaten = self.metadaten().await?;
        let client_id = self.client_id().await?;

        let lauscher = TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(|fehler| {
                AnmeldeFehler::Fehlgeschlagen(format!(
                    "Der Loopback-Lauscher startet nicht: {fehler}"
                ))
            })?;
        let port = lauscher
            .local_addr()
            .map_err(|fehler| {
                AnmeldeFehler::Fehlgeschlagen(format!("Der Lauscher hat keine Adresse: {fehler}"))
            })?
            .port();
        let umleitung = oauth::umleitung_mit_port(port);

        let pkce = Pkce::neu();
        let state = pkce::state();
        let ziel = oauth::authorize_url(
            &metadaten.authorization_endpoint(),
            &client_id,
            &umleitung,
            &state,
            &pkce,
            &metadaten.scope,
        )
        .map_err(|fehler| AnmeldeFehler::Fehlgeschlagen(fehler.to_string()))?;

        bericht.push(format!("Browser-Anmeldung, Rückleitung {umleitung}"));
        self.inner
            .browser
            .oeffnen(ziel.as_str())
            .map_err(|fehler| AnmeldeFehler::Fehlgeschlagen(fehler.to_string()))?;
        bericht.push("Browser geöffnet – warte auf die Rückleitung.".to_string());

        let zurueck = warte_auf_rueckleitung(lauscher, self.inner.konfig.wartezeit)
            .await
            .map_err(AnmeldeFehler::Fehlgeschlagen)?;
        let (code, zurueck_state, fehler) = aus_query(&zurueck);
        if let Some(fehler) = fehler {
            return Err(AnmeldeFehler::Fehlgeschlagen(format!(
                "Der Anmeldedienst meldet: {fehler}"
            )));
        }
        if zurueck_state.as_deref() != Some(state.as_str()) {
            return Err(AnmeldeFehler::Fehlgeschlagen(
                "Der state der Rückleitung passt nicht – der Vorgang wird verworfen.".to_string(),
            ));
        }
        let Some(code) = code else {
            return Err(AnmeldeFehler::Fehlgeschlagen(
                "Die Rückleitung kam ohne Code.".to_string(),
            ));
        };
        bericht.push("Rückleitung erhalten (Code da).".to_string());

        let endpunkt = metadaten.token_endpoint().ok_or_else(|| {
            AnmeldeFehler::Fehlgeschlagen("Es gibt keinen Token-Endpunkt.".to_string())
        })?;
        let antwort = oauth::code_tauschen(
            &self.inner.http,
            &endpunkt,
            &client_id,
            &code,
            pkce.verifier(),
            &umleitung,
        )
        .await
        .map_err(|fehler| AnmeldeFehler::Fehlgeschlagen(fehler.to_string()))?;
        self.uebernehmen(antwort);
        bericht.push("Code gegen Token getauscht – angemeldet.".to_string());
        Ok(())
    }

    /// Die Metadaten des ZEP-MCP (einmal holen, dann merken).
    async fn metadaten(&self) -> Result<Metadaten, AnmeldeFehler> {
        if let Some(metadaten) = self
            .inner
            .metadaten
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner())
            .clone()
        {
            return Ok(metadaten);
        }
        let mcp = metadaten::mcp_adresse(&self.inner.konfig.url)
            .map_err(|fehler| AnmeldeFehler::Fehlgeschlagen(fehler.to_string()))?;
        let geholt = metadaten::holen(&self.inner.http, &mcp)
            .await
            .map_err(|fehler| AnmeldeFehler::Fehlgeschlagen(fehler.to_string()))?;
        let mut fach = self
            .inner
            .metadaten
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner());
        *fach = Some(geholt.clone());
        Ok(geholt)
    }

    /// Die `client_id` aus dem Tresor oder aus einer neuen Registrierung (DCR).
    async fn client_id(&self) -> Result<String, AnmeldeFehler> {
        if let Some(id) = self
            .inner
            .client_id
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner())
            .clone()
        {
            return Ok(id);
        }

        let schluessel = schluessel_client(&self.inner.konfig.url);
        if let Some(gespeichert) = self
            .inner
            .tresor
            .lesen(&schluessel)
            .map_err(|fehler| AnmeldeFehler::Fehlgeschlagen(format!("Tresor: {fehler}")))?
            .filter(|wert| !wert.trim().is_empty())
        {
            *self
                .inner
                .client_id
                .lock()
                .unwrap_or_else(|vergiftet| vergiftet.into_inner()) = Some(gespeichert.clone());
            return Ok(gespeichert);
        }

        let metadaten = self.metadaten().await?;
        let endpunkt = metadaten.registrierungs_endpunkt().ok_or_else(|| {
            AnmeldeFehler::Fehlgeschlagen(
                oauth::OauthFehler::KeinRegistrierungsEndpunkt.to_string(),
            )
        })?;
        let id = oauth::registrieren(&self.inner.http, &endpunkt)
            .await
            .map_err(|fehler| AnmeldeFehler::Fehlgeschlagen(fehler.to_string()))?;
        self.inner
            .tresor
            .schreiben(&schluessel, &id)
            .map_err(|fehler| AnmeldeFehler::Fehlgeschlagen(format!("Tresor: {fehler}")))?;
        tracing::info!("Client-Registrierung gespeichert");
        *self
            .inner
            .client_id
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner()) = Some(id.clone());
        Ok(id)
    }

    /// Das gespeicherte Refresh-Token, falls es eines gibt.
    fn refresh_token(&self) -> Option<String> {
        self.inner
            .tresor
            .lesen(&schluessel_refresh(&self.inner.konfig.url))
            .ok()
            .flatten()
            .filter(|wert| !wert.trim().is_empty())
    }

    fn refresh_loeschen(&self) {
        if let Err(fehler) = self
            .inner
            .tresor
            .loeschen(&schluessel_refresh(&self.inner.konfig.url))
        {
            tracing::warn!("Refresh-Token ließ sich nicht löschen: {fehler}");
        }
    }

    /// Ein neues Paar per Refresh-Token holen und übernehmen.
    ///
    /// Hinter der einen Sperre: zwei gleichzeitige Erneuerungen würden sich
    /// gegenseitig das rotierte Refresh-Token wegnehmen.
    async fn erneuern_aus_tresor(&self) -> Result<(), AnmeldeFehler> {
        let _wache = self.inner.vorgang.lock().await;
        if self.aus_dem_speicher().is_some() {
            return Ok(());
        }
        let Some(refresh) = self.refresh_token() else {
            return Err(AnmeldeFehler::NichtAngemeldet);
        };
        let metadaten = self.metadaten().await?;
        let endpunkt = metadaten.token_endpoint().ok_or_else(|| {
            AnmeldeFehler::Fehlgeschlagen("Es gibt keinen Token-Endpunkt.".to_string())
        })?;
        let client_id = self.client_id().await?;
        let antwort = oauth::erneuern(&self.inner.http, &endpunkt, &client_id, &refresh)
            .await
            .map_err(|fehler| {
                if fehler.ist_invalid_grant() {
                    AnmeldeFehler::RefreshAbgelehnt
                } else {
                    AnmeldeFehler::Fehlgeschlagen(fehler.to_string())
                }
            })?;
        self.uebernehmen(antwort);
        Ok(())
    }

    /// Ein Token-Paar übernehmen: Speicher setzen, rotiertes Refresh-Token
    /// sofort sichern.
    fn uebernehmen(&self, antwort: TokenAntwort) {
        if let Some(refresh) = &antwort.refresh {
            if let Err(fehler) = self
                .inner
                .tresor
                .schreiben(&schluessel_refresh(&self.inner.konfig.url), refresh)
            {
                tracing::warn!("Refresh-Token ließ sich nicht sichern: {fehler}");
            }
        }
        let gespeichert = Gespeichertes {
            access: antwort.access,
            ablauf: Instant::now() + antwort.gueltig,
            laufzeit: antwort.gueltig,
        };
        *self
            .inner
            .token
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner()) = Some(gespeichert);
    }

    fn aus_dem_speicher(&self) -> Option<String> {
        let fach = self
            .inner
            .token
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner());
        match fach.as_ref() {
            Some(token) if token.gilt_noch() => Some(token.access.clone()),
            _ => None,
        }
    }
}

/// Warum die Routine gesperrt wird – als Satz, der dem User hilft.
fn sperrgrund(fehler: &AnmeldeFehler) -> String {
    match fehler {
        AnmeldeFehler::Gesperrt(grund) => grund.clone(),
        AnmeldeFehler::NichtAngemeldet => {
            "Ohne Benutzer und Passwort ist keine Anmeldung möglich.".to_string()
        }
        AnmeldeFehler::RefreshAbgelehnt => "Das Refresh-Token wurde abgelehnt.".to_string(),
        AnmeldeFehler::Fehlgeschlagen(text) => text.clone(),
    }
}

/// Eine Meldung ohne Query (der Code gehört nicht in einen Bericht).
fn kurz(url: &Url) -> String {
    let mut ohne_query = url.clone();
    ohne_query.set_query(None);
    ohne_query.set_fragment(None);
    ohne_query.to_string()
}

/// `code`, `state` und `error` aus der Rückleitung.
fn aus_query(url: &Url) -> (Option<String>, Option<String>, Option<String>) {
    let mut code = None;
    let mut state = None;
    let mut fehler = None;
    for (name, wert) in url.query_pairs() {
        match name.as_ref() {
            "code" => code = Some(wert.to_string()),
            "state" => state = Some(wert.to_string()),
            "error" => fehler = Some(wert.to_string()),
            "error_description" if fehler.is_some() => {
                let bisher = fehler.take().unwrap_or_default();
                fehler = Some(format!("{bisher} ({wert})"));
            }
            _ => {}
        }
    }
    (code, state, fehler)
}

/// Warten, bis der Browser auf dem Loopback ankommt, und die Adresse lesen.
async fn warte_auf_rueckleitung(lauscher: TcpListener, geduld: Duration) -> Result<Url, String> {
    let (mut strom, adresse) = tokio::time::timeout(geduld, lauscher.accept())
        .await
        .map_err(|_| {
            format!(
                "Der Browser ist innerhalb von {} Sekunden nicht zurückgekommen.",
                geduld.as_secs()
            )
        })?
        .map_err(|fehler| format!("Der Loopback-Lauscher nimmt nichts an: {fehler}"))?;

    let mut puffer = vec![0u8; 4096];
    let gelesen = tokio::time::timeout(Duration::from_secs(10), strom.read(&mut puffer))
        .await
        .map_err(|_| "Der Browser hat nichts geschickt.".to_string())?
        .map_err(|fehler| format!("Der Browser hat nichts geschickt: {fehler}"))?;
    let anfrage = String::from_utf8_lossy(&puffer[..gelesen]).to_string();

    let antwort = "<!doctype html><html lang=\"de\"><head><meta charset=\"utf-8\">\
                   <title>quiver-zep – Anmeldung</title></head><body>\
                   <p>Angemeldet. Dieses Fenster kann geschlossen werden.</p>\
                   </body></html>";
    let kopf = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{antwort}",
        antwort.len()
    );
    let _ = strom.write_all(kopf.as_bytes()).await;
    let _ = strom.shutdown().await;

    // „GET /?code=…&state=… HTTP/1.1“
    let ziel = anfrage
        .lines()
        .next()
        .and_then(|zeile| zeile.split_whitespace().nth(1))
        .ok_or_else(|| "Die Anfrage des Browsers ist unlesbar.".to_string())?;
    let port = adresse.port();
    Url::parse(&format!("http://127.0.0.1:{port}{ziel}"))
        .map_err(|fehler| format!("Die Rückleitung ist keine gültige Adresse: {fehler}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn das_token_gilt_bis_80_prozent_der_laufzeit() {
        let jetzt = Instant::now();
        let laufzeit = Duration::from_secs(100);
        let ablauf = jetzt + laufzeit;
        // Frisch: gültig.
        assert!(Gespeichertes::gueltig_bis(ablauf, laufzeit, jetzt));
        // 79 Sekunden später: noch gültig (mehr als ein Fünftel übrig).
        assert!(Gespeichertes::gueltig_bis(
            ablauf,
            laufzeit,
            jetzt + Duration::from_secs(79)
        ));
        // 80 Sekunden später: erneuern.
        assert!(!Gespeichertes::gueltig_bis(
            ablauf,
            laufzeit,
            jetzt + Duration::from_secs(80)
        ));
        assert!(!Gespeichertes::gueltig_bis(
            ablauf,
            laufzeit,
            jetzt + Duration::from_secs(99)
        ));
    }

    #[test]
    fn debug_zeigt_kein_token() {
        let token = Gespeichertes {
            access: "at-nur-im-test".to_string(),
            ablauf: Instant::now() + Duration::from_secs(60),
            laufzeit: Duration::from_secs(60),
        };
        let text = format!("{token:?}");
        assert!(!text.contains("at-nur-im-test"));
    }

    #[test]
    fn der_status_ohne_alles_ist_nicht_angemeldet() {
        let ordner = tempfile::tempdir().unwrap();
        let anmeldung = Anmeldung::neu(
            testkonfig(ordner.path().to_path_buf()),
            Arc::new(crate::tresor::SpeicherTresor::neu()),
            Arc::new(crate::browser::KeinBrowser),
        );
        assert_eq!(anmeldung.status(), Status::NichtAngemeldet);
        assert!(!anmeldung.status().ist_angemeldet());
        assert!(anmeldung.status().text().contains("ZEP-Anmeldung nötig"));
        assert!(anmeldung.sperrgrund().is_none());
    }

    #[test]
    fn eine_gesperrte_routine_wird_gemeldet() {
        let ordner = tempfile::tempdir().unwrap();
        let anmeldung = Anmeldung::neu(
            testkonfig(ordner.path().to_path_buf()),
            Arc::new(crate::tresor::SpeicherTresor::neu()),
            Arc::new(crate::browser::KeinBrowser),
        );
        anmeldung
            .inner
            .sperre
            .sperren("Das Passwort wurde abgelehnt.")
            .unwrap();
        match anmeldung.status() {
            Status::Gesperrt { grund } => assert!(grund.contains("abgelehnt")),
            sonst => panic!("kein Sperrzustand: {sonst:?}"),
        }
        assert!(AnmeldeFehler::Gesperrt("x".to_string()).ist_gesperrt());
        assert!(!AnmeldeFehler::NichtAngemeldet.ist_gesperrt());
    }

    fn testkonfig(zustand: std::path::PathBuf) -> ZepKonfig {
        ZepKonfig {
            url: "https://zep.example.test/a/next/mcp".to_string(),
            benutzer: "ada".to_string(),
            passwort: "streng-geheim-4711".to_string(),
            zustand,
            takt: Duration::from_millis(50),
            wartezeit: Duration::from_secs(5),
        }
    }
}
