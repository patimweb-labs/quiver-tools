// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Anmelde-Routine **ohne** Browser (Aufgabe 06, Punkt 4).
//!
//! Der User hat Benutzer und Passwort im quiver-Fenster eingetragen; damit
//! läuft der Authorize-Ablauf wie in einem Browser, nur über HTTP mit eigenem
//! Cookie-Speicher:
//!
//! 1. Authorize-Adresse laden (Login-Formular),
//! 2. Formular auslesen (action, versteckte Felder, CSRF) und mit Benutzer und
//!    Passwort absenden,
//! 3. Zustimmungsseite („OK“) bestätigen,
//! 4. die Weiterleitung auf `127.0.0.1` **abfangen**, nicht verfolgen,
//! 5. Code und PKCE-Verifier gegen Token tauschen.
//!
//! Den Aufbau der Seiten kennen wir nicht. Darum liest [`crate::html`] die
//! Formulare, und diese Steuerung meldet Schritt für Schritt, was sie sieht –
//! Formular- und Feldnamen, **nie** Werte. Eine Seite, die sie nicht einordnen
//! kann, beendet den Ablauf mit einer Meldung.

use std::time::Duration;

use url::Url;

use crate::cookies::Kekse;
use crate::html;
use crate::metadaten::Metadaten;
use crate::oauth::{self, TokenAntwort, UMLEITUNG};
use crate::pkce::{self, Pkce};

/// So viele Schritte darf ein Ablauf brauchen; danach bricht er ab.
pub const MAX_SCHRITTE: usize = 16;

/// Wie lange eine einzelne Anfrage höchstens dauert.
pub const ZEITLIMIT: Duration = Duration::from_secs(25);

/// Warum die Routine nicht zu einem Token geführt hat.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RoutineFehler {
    /// Die Seite passt zu keinem bekannten Schritt. Der Text nennt Formulare
    /// und Feldnamen, aber keine Werte.
    #[error("Unbekannte Seite – abgebrochen. {0}")]
    UnklareSeite(String),
    /// Nach dem Absenden des Anmeldeformulars kam wieder ein Anmeldeformular:
    /// Benutzer oder Passwort stimmt nicht.
    #[error("Benutzer oder Passwort wurde abgelehnt – die Anmeldung kommt nicht weiter.")]
    Abgelehnt,
    /// Nach dem Absenden kam eine Weiterleitung (die Anmeldung wurde
    /// angenommen), am Ende aber wieder das Anmeldeformular: die Sitzung ist
    /// unterwegs verloren gegangen – kein falsches Passwort.
    #[error(
        "ZEP hat die Anmeldung angenommen und weitergeleitet, danach kam aber wieder das \
         Anmeldeformular – die Sitzung ging unterwegs verloren. Kein falsches Passwort; \
         bitte den Bericht an den Entwickler geben."
    )]
    SitzungVerloren,
    /// Der Anmeldedienst hat einen Fehler zurückgegeben (`error=…`).
    #[error("Der Anmeldedienst meldet: {0}")]
    Dienst(String),
    #[error("{0}")]
    Netz(String),
    #[error("Die Rückleitung kam ohne Code.")]
    KeinCode,
    #[error("Der state der Rückleitung passt nicht – der Vorgang wird verworfen.")]
    StateFalsch,
    #[error("Der Ablauf brauchte mehr als {MAX_SCHRITTE} Schritte – abgebrochen.")]
    ZuVieleSchritte,
    #[error("Die Rückleitung führt nach {0} statt an den Loopback.")]
    FalscheRueckleitung(String),
    #[error("{0}")]
    Oauth(#[from] oauth::OauthFehler),
}

/// Wie eine Anfrage beantwortet wurde.
enum Antwort {
    /// Eine Seite, die gelesen werden kann.
    Seite(String),
    /// Eine Weiterleitung, die wir selbst (oder gar nicht) verfolgen.
    Weiterleitung(Url),
}

/// Was wir gerade schicken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Methode {
    Get,
    Post,
}

/// Der Ablauf mit Bericht.
pub struct Routine<'a> {
    http: &'a reqwest::Client,
    benutzer: &'a str,
    passwort: &'a str,
    bericht: Vec<String>,
    /// Das Gerüst der Seite, an der der Ablauf hängen blieb (ohne Werte).
    unklare_seite: Option<String>,
}

impl<'a> Routine<'a> {
    pub fn neu(http: &'a reqwest::Client, benutzer: &'a str, passwort: &'a str) -> Routine<'a> {
        Routine {
            http,
            benutzer,
            passwort,
            bericht: Vec::new(),
            unklare_seite: None,
        }
    }

    /// Die Schritte, die der Ablauf gesehen hat – ohne Werte, ohne Token.
    pub fn bericht(&self) -> &[String] {
        &self.bericht
    }

    /// Das Gerüst der Seite, an der der Ablauf hängen blieb – ohne Werte.
    pub fn unklare_seite(&self) -> Option<&str> {
        self.unklare_seite.as_deref()
    }

    /// Den Bericht übernehmen.
    pub fn in_bericht(self) -> Vec<String> {
        self.bericht
    }

    /// Den ganzen Ablauf ausführen und die Token zurückgeben.
    pub async fn ausfuehren(
        &mut self,
        metadaten: &Metadaten,
        client_id: &str,
    ) -> Result<TokenAntwort, RoutineFehler> {
        let pkce = Pkce::neu();
        let state = pkce::state();
        let authorize = oauth::authorize_url(
            &metadaten.authorization_endpoint(),
            client_id,
            UMLEITUNG,
            &state,
            &pkce,
            &metadaten.scope,
        )?;
        let Some(token_endpunkt) = metadaten.token_endpoint() else {
            return Err(RoutineFehler::Oauth(oauth::OauthFehler::KeinTokenEndpunkt));
        };
        let rueckleitung = Url::parse(UMLEITUNG)
            .map_err(|_| RoutineFehler::FalscheRueckleitung(UMLEITUNG.to_string()))?;
        // Benutzer und Passwort gehen nur an den Authorization-Host.
        let erlaubter_host = metadaten
            .authorization_endpoint()
            .host_str()
            .map(|host| host.to_ascii_lowercase());

        self.melde(format!("1. Authorize-Seite laden: {}", kurz(&authorize)));

        let mut kekse = Kekse::default();
        let mut ziel = authorize;
        let mut methode = Methode::Get;
        let mut rumpf: Option<Vec<(String, String)>> = None;
        let mut anmeldung_gesendet = false;
        // Wurde nach dem Absenden weitergeleitet? Dann hat der Dienst die
        // Anmeldung angenommen.
        let mut nach_anmeldung_weitergeleitet = false;

        for nummer in 2..=MAX_SCHRITTE {
            let gesendet = kekse.namen_fuer(&ziel);
            let antwort = self.sende(methode, &ziel, rumpf.take(), &mut kekse).await?;
            // Nur Namen, Domain und Pfad – nie Werte.
            self.melde(format!(
                "{nummer}. Kekse gesendet: {gesendet}; danach gespeichert: {}",
                kekse.uebersicht()
            ));

            match antwort {
                Antwort::Weiterleitung(ort) => {
                    self.melde(format!(
                        "{nummer}. {} → Weiterleitung nach {}{}",
                        kurz(&ziel),
                        kurz(&ort),
                        parameternamen(&ort)
                    ));
                    if ist_rueckleitung(&ort, &rueckleitung) {
                        let (code, zurueck_state, fehler) = aus_query(&ort);
                        if let Some(fehler) = fehler {
                            return Err(RoutineFehler::Dienst(fehler));
                        }
                        if zurueck_state.as_deref() != Some(state.as_str()) {
                            return Err(RoutineFehler::StateFalsch);
                        }
                        let Some(code) = code else {
                            return Err(RoutineFehler::KeinCode);
                        };
                        self.melde(
                            "Rückleitung an den Loopback abgefangen (Code erhalten).".to_string(),
                        );
                        let antwort = oauth::code_tauschen(
                            self.http,
                            &token_endpunkt,
                            client_id,
                            &code,
                            pkce.verifier(),
                            UMLEITUNG,
                        )
                        .await?;
                        self.melde("Code gegen Token getauscht – angemeldet.".to_string());
                        return Ok(antwort);
                    }
                    if anmeldung_gesendet {
                        nach_anmeldung_weitergeleitet = true;
                    }
                    ziel = ort;
                    methode = Methode::Get;
                }

                Antwort::Seite(html_text) => {
                    let seite = html::Seite::lesen(&html_text);
                    self.melde(format!(
                        "{nummer}. Seite {}: {}",
                        kurz(&ziel),
                        seite.kurzfassung()
                    ));

                    if let Some(formular) = seite.anmeldeformular() {
                        if anmeldung_gesendet {
                            return Err(if nach_anmeldung_weitergeleitet {
                                RoutineFehler::SitzungVerloren
                            } else {
                                RoutineFehler::Abgelehnt
                            });
                        }
                        let paare = formular.anmeldepaar(self.benutzer, self.passwort).map_err(
                            |fehler| {
                                RoutineFehler::UnklareSeite(format!(
                                    "Das Anmeldeformular taugt nicht: {fehler} Felder: {}",
                                    namen(&seite.feldnamen())
                                ))
                            },
                        )?;
                        self.melde(format!(
                            "{nummer}. Anmeldeformular absenden (Felder: {})",
                            namen(
                                &paare
                                    .iter()
                                    .map(|(name, _)| name.clone())
                                    .collect::<Vec<_>>()
                            )
                        ));
                        let formularziel = ziel_aufloesen(&ziel, &formular.action)?;
                        if !gleicher_host(&formularziel, erlaubter_host.as_deref()) {
                            return Err(RoutineFehler::UnklareSeite(format!(
                                "Das Anmeldeformular zeigt auf {}, also einen anderen Host als der \
                                 Authorization-Server ({}). Benutzer und Passwort werden dort nicht \
                                 gesendet.",
                                kurz(&formularziel),
                                erlaubter_host.as_deref().unwrap_or("unbekannt")
                            )));
                        }
                        ziel = formularziel;
                        methode = methode_von(&formular.methode);
                        rumpf = Some(paare);
                        anmeldung_gesendet = true;
                        continue;
                    }

                    if let Some(formular) = seite.zustimmungsformular() {
                        let knopf = formular
                            .absendeknopf()
                            .map(|knopf| knopf.text.clone())
                            .unwrap_or_else(|| "OK".to_string());
                        self.melde(format!(
                            "{nummer}. Zustimmungsseite: Knopf „{knopf}“ bestätigen (Felder: {})",
                            namen(
                                &formular
                                    .felder
                                    .iter()
                                    .map(|feld| feld.name.clone())
                                    .collect::<Vec<_>>()
                            )
                        ));
                        ziel = ziel_aufloesen(&ziel, &formular.action)?;
                        methode = methode_von(&formular.methode);
                        rumpf = Some(formular.rumpf());
                        continue;
                    }

                    // ZEP: Laravel Passport mit Inertia – kein <form>, die
                    // Angaben für „OK“ stehen in den Seitendaten.
                    if let Some(zustimmung) = crate::inertia::zustimmung(&html_text) {
                        if zustimmung.client_id != client_id {
                            return Err(RoutineFehler::UnklareSeite(format!(
                                "Die Zustimmungsseite gilt einem anderen Client ({}) als \
                                 unserer Registrierung – nicht bestätigt.",
                                zustimmung.client_name
                            )));
                        }
                        let bestaetigen = ziel_aufloesen(&ziel, &zustimmung.approve_url)?;
                        if !gleicher_host(&bestaetigen, erlaubter_host.as_deref()) {
                            return Err(RoutineFehler::UnklareSeite(format!(
                                "Die Zustimmung zeigt auf {}, einen anderen Host als der \
                                 Authorization-Server – nicht bestätigt.",
                                kurz(&bestaetigen)
                            )));
                        }
                        self.melde(format!(
                            "{nummer}. Zustimmungsseite (Inertia): „{}“ mit Scope {} bestätigen \
                             (Felder: _token, state, client_id, auth_token)",
                            zustimmung.client_name,
                            zustimmung.scopes.join(", ")
                        ));
                        ziel = bestaetigen;
                        methode = Methode::Post;
                        rumpf = Some(zustimmung.rumpf());
                        continue;
                    }

                    self.unklare_seite = Some(format!(
                        "Seite {}\n\n{}",
                        kurz(&ziel),
                        crate::geruest::geruest(&html_text)
                    ));
                    return Err(RoutineFehler::UnklareSeite(format!(
                        "Seite {}. {} Formular(e); Felder: {}. \
                         Erwartet war ein Anmelde- oder ein Zustimmungsformular.",
                        kurz(&ziel),
                        seite.formulare.len(),
                        namen(&seite.feldnamen())
                    )));
                }
            }
        }

        Err(RoutineFehler::ZuVieleSchritte)
    }

    /// Eine Anfrage schicken und die Antwort einordnen.
    async fn sende(
        &self,
        methode: Methode,
        ziel: &Url,
        rumpf: Option<Vec<(String, String)>>,
        kekse: &mut Kekse,
    ) -> Result<Antwort, RoutineFehler> {
        let mut anfrage = match methode {
            Methode::Get => self.http.get(ziel.as_str()),
            Methode::Post => self.http.post(ziel.as_str()),
        };
        if let Some(kopf) = kekse.kopf(ziel) {
            anfrage = anfrage.header("Cookie", kopf);
        }
        if let Some(paare) = rumpf {
            anfrage = anfrage.form(&paare);
        }

        let antwort = anfrage.send().await.map_err(|fehler| {
            RoutineFehler::Netz(format!(
                "{} ist nicht erreichbar: {}",
                kurz(ziel),
                fehler.without_url()
            ))
        })?;

        for kopf in antwort.headers().get_all("set-cookie") {
            if let Ok(text) = kopf.to_str() {
                kekse.merken(ziel, text);
            }
        }

        let status = antwort.status();
        if status.is_redirection() {
            let Some(ort) = antwort
                .headers()
                .get("location")
                .and_then(|wert| wert.to_str().ok())
            else {
                return Err(RoutineFehler::Netz(format!(
                    "{} leitet weiter, nennt aber kein Ziel.",
                    kurz(ziel)
                )));
            };
            let weiter = ziel_aufloesen(ziel, ort)?;
            return Ok(Antwort::Weiterleitung(weiter));
        }

        if !status.is_success() {
            return Err(RoutineFehler::Netz(format!(
                "{} antwortet mit Status {}.",
                kurz(ziel),
                status.as_u16()
            )));
        }

        let text = antwort.text().await.map_err(|fehler| {
            RoutineFehler::Netz(format!(
                "Die Antwort von {} ist nicht lesbar: {}",
                kurz(ziel),
                fehler.without_url()
            ))
        })?;
        Ok(Antwort::Seite(text))
    }

    fn melde(&mut self, text: String) {
        tracing::debug!(schritt = %text, "Anmelde-Routine");
        self.bericht.push(text);
    }
}

/// `action` eines Formulars gegen die aktuelle Adresse auflösen.
fn ziel_aufloesen(aktuell: &Url, action: &str) -> Result<Url, RoutineFehler> {
    let action = action.trim();
    if action.is_empty() {
        return Ok(aktuell.clone());
    }
    aktuell.join(action).map_err(|_| {
        RoutineFehler::Netz(format!("„{action}“ ist keine gültige Adresse im Formular."))
    })
}

fn methode_von(methode: &str) -> Methode {
    if methode.eq_ignore_ascii_case("post") {
        Methode::Post
    } else {
        Methode::Get
    }
}

/// Ist diese Weiterleitung die Rückleitung an den Loopback?
///
/// Nur die Adresse aus `redirect_uri` zählt – gleiches Schema, gleicher Host,
/// gleicher Port, gleicher Pfad. Ein Anmeldedienst, der selbst auf dem Loopback
/// läuft (Tests), leitet sonst bei jedem Zwischenschritt „zurück“.
fn ist_rueckleitung(ort: &Url, rueckleitung: &Url) -> bool {
    ort.scheme() == rueckleitung.scheme()
        && ort.host_str() == rueckleitung.host_str()
        && ort.port() == rueckleitung.port()
        && pfad(ort.path()) == pfad(rueckleitung.path())
}

/// Die Namen der Query-Parameter einer Adresse – ohne Werte (der Code und
/// Sitzungskennungen gehören nicht in einen Bericht).
fn parameternamen(url: &Url) -> String {
    let namen: Vec<String> = url
        .query_pairs()
        .map(|(name, _)| name.into_owned())
        .collect();
    if namen.is_empty() {
        String::new()
    } else {
        format!(" (Parameter: {})", namen.join(", "))
    }
}

/// Liegt die Adresse auf dem erlaubten Host?
///
/// Ein vorangestelltes `www.` zählt als derselbe Host: ZEP nennt im
/// Authorization-Server `zep-online.de`, das Anmeldeformular liegt aber auf
/// `www.zep-online.de`. Jeder andere Host – auch eine andere Subdomain – ist
/// fremd.
///
/// Ohne bekannten Host (die Metadaten nannten keinen) bleibt der Ablauf wie er
/// ist – es gibt dann nichts, wogegen zu prüfen wäre.
fn gleicher_host(ziel: &Url, erlaubt: Option<&str>) -> bool {
    fn ohne_www(host: &str) -> String {
        let klein = host.to_ascii_lowercase();
        match klein.strip_prefix("www.") {
            Some(rest) if !rest.is_empty() => rest.to_string(),
            _ => klein,
        }
    }
    match erlaubt {
        None => true,
        Some(host) => ziel
            .host_str()
            .is_some_and(|eigener| ohne_www(eigener) == ohne_www(host)),
    }
}

fn pfad(pfad: &str) -> &str {
    if pfad.is_empty() {
        "/"
    } else {
        pfad
    }
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

/// Eine Adresse ohne Query für Meldungen: der Code gehört nicht in einen Bericht.
fn kurz(url: &Url) -> String {
    let mut ohne_query = url.clone();
    ohne_query.set_query(None);
    ohne_query.set_fragment(None);
    ohne_query.to_string()
}

/// Eine Namensliste für Meldungen.
fn namen(liste: &[String]) -> String {
    if liste.is_empty() {
        "keine".to_string()
    } else {
        liste.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn die_rueckleitung_wird_erkannt_und_gelesen() {
        let rueck = Url::parse(UMLEITUNG).unwrap();
        let url = Url::parse("http://127.0.0.1/?code=abc&state=xyz").unwrap();
        assert!(ist_rueckleitung(&url, &rueck));
        let (code, state, fehler) = aus_query(&url);
        assert_eq!(code.as_deref(), Some("abc"));
        assert_eq!(state.as_deref(), Some("xyz"));
        assert_eq!(fehler, None);

        let fehler_url =
            Url::parse("http://127.0.0.1?error=access_denied&error_description=Nein").unwrap();
        assert!(ist_rueckleitung(&fehler_url, &rueck));
        let (code, _, fehler) = aus_query(&fehler_url);
        assert_eq!(code, None);
        assert_eq!(fehler.as_deref(), Some("access_denied (Nein)"));
    }

    #[test]
    fn zwischenseiten_und_fremde_hosts_sind_keine_rueckleitung() {
        let rueck = Url::parse(UMLEITUNG).unwrap();
        for fremd in [
            // Ein Zwischenschritt auf demselben Host, aber mit Port und Pfad.
            "http://127.0.0.1:4711/mandant/next/oauth/zustimmen",
            // Ein anderer Host.
            "https://boese.example.test/zurueck",
            // Gleicher Host, anderer Pfad.
            "http://127.0.0.1/zurueck",
        ] {
            assert!(
                !ist_rueckleitung(&Url::parse(fremd).unwrap(), &rueck),
                "{fremd}"
            );
        }
        // Mit Port in der Umleitung gilt genau dieser Port.
        let mit_port = Url::parse("http://127.0.0.1:4711").unwrap();
        assert!(ist_rueckleitung(
            &Url::parse("http://127.0.0.1:4711/?code=x").unwrap(),
            &mit_port
        ));
        assert!(!ist_rueckleitung(
            &Url::parse("http://127.0.0.1:4712/?code=x").unwrap(),
            &mit_port
        ));
    }

    #[test]
    fn zugangsdaten_gehen_nur_an_den_authorization_host() {
        let zep = Url::parse("https://www.zep-online.de/zep/next/oauth/anmelden").unwrap();
        assert!(gleicher_host(&zep, Some("www.zep-online.de")));
        assert!(gleicher_host(&zep, Some("WWW.ZEP-ONLINE.DE")));
        assert!(!gleicher_host(&zep, Some("boese.example.test")));
        // ZEP: Authorization-Server ohne www, Anmeldeformular mit www.
        assert!(gleicher_host(&zep, Some("zep-online.de")));
        let ohne = Url::parse("https://zep-online.de/zep/view/login.php").unwrap();
        assert!(gleicher_host(&ohne, Some("www.zep-online.de")));
        // Andere Subdomains oder angehängte Domains bleiben fremd.
        let fremd = Url::parse("https://login.zep-online.de/x").unwrap();
        assert!(!gleicher_host(&fremd, Some("zep-online.de")));
        let angehaengt = Url::parse("https://www.zep-online.de.boese.test/x").unwrap();
        assert!(!gleicher_host(&angehaengt, Some("zep-online.de")));
        let nur_www = Url::parse("https://www./x");
        if let Ok(nur_www) = nur_www {
            assert!(!gleicher_host(&nur_www, Some("zep-online.de")));
        }
        // Ohne bekannten Host wird nicht geprüft.
        assert!(gleicher_host(&zep, None));
        // Eine Adresse ohne Host taugt nie.
        assert!(!gleicher_host(
            &Url::parse("mailto:jemand@example.test").unwrap(),
            Some("www.zep-online.de")
        ));
    }

    #[test]
    fn ein_formularziel_wird_gegen_die_aktuelle_adresse_aufgeloest() {
        let aktuell = Url::parse("https://example.test/a/next/login?x=1").unwrap();
        assert_eq!(
            ziel_aufloesen(&aktuell, "/sitzung").unwrap().as_str(),
            "https://example.test/sitzung"
        );
        assert_eq!(
            ziel_aufloesen(&aktuell, "weiter?y=2").unwrap().as_str(),
            "https://example.test/a/next/weiter?y=2"
        );
        assert_eq!(ziel_aufloesen(&aktuell, "").unwrap(), aktuell);
        // Eine absolute Adresse im Formular bleibt absolut.
        assert_eq!(
            ziel_aufloesen(&aktuell, "https://example.test/login")
                .unwrap()
                .as_str(),
            "https://example.test/login"
        );
    }

    #[test]
    fn meldungen_lassen_die_query_weg() {
        let url = Url::parse("https://example.test/login?csrf=geheim#oben").unwrap();
        assert_eq!(kurz(&url), "https://example.test/login");
    }

    #[test]
    fn methoden_werden_kleingeschrieben_gelesen() {
        assert_eq!(methode_von("POST"), Methode::Post);
        assert_eq!(methode_von("post"), Methode::Post);
        assert_eq!(methode_von("get"), Methode::Get);
        assert_eq!(methode_von(""), Methode::Get);
    }
}
