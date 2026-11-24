// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der DevOps-Client: ein HTTP-Gespräch je Organisation (Aufgabe 09).
//!
//! In Rust neu gebaut, abgelesen an der REST-Dokumentation (Azure DevOps 7.1)
//! und an der Vorlage `pi-azure-devops` (dort über `azure-devops-node-api`):
//!
//! - **Anmeldung** mit HTTP Basic, Benutzer leer und der PAT als Passwort:
//!   `Authorization: Basic base64(":" + PAT)`. Der PAT steht nur in dieser
//!   Kopfzeile.
//! - **`api-version`** hängt an jeder Anfrage (`7.1`; für die Kommentar-Endpunkte
//!   `7.1-preview.4`, weil sie in DevOps noch Vorschau sind).
//! - **401 und 203 mit HTML** heißen „PAT gilt nicht“ (DevOps leitet zur
//!   Anmeldung um) und werden **nicht** wiederholt.
//! - **429** wird nur bei **lesenden** Aufrufen wiederholt, höchstens zweimal,
//!   mit `Retry-After`. Ein Anlegen oder Ändern wird nie wiederholt: ob es ankam,
//!   weiß nur der Server.
//! - **JSON-Patch** (`PATCH`/`POST` mit `application/json-patch+json`) für
//!   Arbeitsitems.

use std::time::Duration;

use reqwest::header::{HeaderMap, ACCEPT, CONTENT_TYPE, RETRY_AFTER, USER_AGENT};
use reqwest::Method;

use crate::fehler::{ist_html, DevopsFehler};
use crate::organisationen::Organisation;

/// Die API-Version von Azure DevOps, die dieses Werkzeug spricht.
pub const API: &str = "7.1";

/// Die Vorschau-Version der Kommentar-Endpunkte (Arbeitsitem-Kommentare).
pub const API_KOMMENTARE: &str = "7.1-preview.4";

/// Wie lange ein einzelner Aufruf höchstens dauern darf.
const ZEITLIMIT: Duration = Duration::from_secs(60);

/// So oft ein lesender Aufruf bei `429` wiederholt wird.
const WIEDERHOLUNGEN: u32 = 2;

/// So lange wird bei `429` höchstens gewartet.
const WARTEN_HOECHSTENS: Duration = Duration::from_secs(10);

/// Was ein Aufruf vom Server will.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Methode {
    Get,
    Post,
    Patch,
    Put,
}

impl Methode {
    fn reqwest(self) -> Method {
        match self {
            Methode::Get => Method::GET,
            Methode::Post => Method::POST,
            Methode::Patch => Method::PATCH,
            Methode::Put => Method::PUT,
        }
    }

    /// Lesend heißt: darf bei `429` wiederholt werden.
    pub fn ist_lesend(self) -> bool {
        self == Methode::Get
    }
}

/// Ein Aufruf an Azure DevOps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anfrage {
    pub methode: Methode,
    /// Pfad ab der Organisationsadresse, z. B. `SCODA/_apis/wit/wiql`.
    pub pfad: String,
    /// Zusätzliche Abfrageparameter; `api-version` kommt automatisch dazu.
    pub abfrage: Vec<(String, String)>,
    /// Rumpf als Text.
    pub inhalt: Option<String>,
    /// Der `Accept`-Kopf; ohne Angabe `application/json`.
    pub annahme: Option<String>,
    /// Weitere Kopfzeilen, etwa ein `Content-Type`.
    pub kopfzeilen: Vec<(String, String)>,
}

impl Anfrage {
    /// Ein lesender Aufruf.
    pub fn holen(pfad: impl Into<String>) -> Anfrage {
        Anfrage::neu(Methode::Get, pfad)
    }

    /// Ein Aufruf mit Rumpf (`POST`).
    pub fn post(pfad: impl Into<String>, inhalt: impl Into<String>) -> Anfrage {
        Anfrage::neu(Methode::Post, pfad).mit_inhalt(inhalt)
    }

    /// Ein JSON-Patch (`PATCH`).
    pub fn patch(pfad: impl Into<String>, inhalt: impl Into<String>) -> Anfrage {
        Anfrage::neu(Methode::Patch, pfad)
            .mit_inhalt(inhalt)
            .mit_kopf("Content-Type", JSON_PATCH)
    }

    /// Ein Aufruf mit Rumpf und eigener Methode.
    pub fn neu(methode: Methode, pfad: impl Into<String>) -> Anfrage {
        Anfrage {
            methode,
            pfad: pfad.into(),
            abfrage: Vec::new(),
            inhalt: None,
            annahme: None,
            kopfzeilen: Vec::new(),
        }
    }

    /// Der Rumpf.
    pub fn mit_inhalt(mut self, inhalt: impl Into<String>) -> Anfrage {
        self.inhalt = Some(inhalt.into());
        self
    }

    /// Ein Abfrageparameter.
    pub fn mit_abfrage(mut self, name: impl Into<String>, wert: impl Into<String>) -> Anfrage {
        self.abfrage.push((name.into(), wert.into()));
        self
    }

    /// Der `Accept`-Kopf, wenn JSON nicht passt (etwa bei einem Log als Text).
    pub fn mit_annahme(mut self, annahme: impl Into<String>) -> Anfrage {
        self.annahme = Some(annahme.into());
        self
    }

    /// Eine weitere Kopfzeile.
    pub fn mit_kopf(mut self, name: impl Into<String>, wert: impl Into<String>) -> Anfrage {
        self.kopfzeilen.push((name.into(), wert.into()));
        self
    }

    /// Ist das ein lesender Aufruf?
    pub fn ist_lesend(&self) -> bool {
        self.methode.ist_lesend()
    }

    /// Setzt der Aufruf selbst einen `Content-Type`?
    pub fn setzt_content_type(&self) -> bool {
        self.kopfzeilen
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("content-type"))
    }
}

/// Der `Content-Type` von JSON-Patch (Aufgabe 09).
pub const JSON_PATCH: &str = "application/json-patch+json";

/// Was der Server geantwortet hat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Antwort {
    pub status: u16,
    pub text: String,
}

/// Der Client einer Organisation.
pub struct Client {
    organisation: Organisation,
    http: reqwest::Client,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("organisation", &self.organisation.name)
            .finish()
    }
}

impl Client {
    /// Ein Client für diese Organisation. Der PAT steht nur im Speicher.
    pub fn neu(organisation: &Organisation) -> Result<Client, DevopsFehler> {
        let http = reqwest::Client::builder()
            .timeout(ZEITLIMIT)
            .user_agent(concat!("quiver-devops/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|fehler| DevopsFehler::Netz {
                text: fehler.to_string(),
            })?;
        Ok(Client {
            organisation: organisation.clone(),
            http,
        })
    }

    /// Die Organisation, mit der dieser Client spricht.
    pub fn organisation(&self) -> &Organisation {
        &self.organisation
    }

    /// Die Adresse eines Aufrufs: eigene Adresse oder vollständige Adresse.
    pub fn url(&self, pfad: &str) -> String {
        if pfad.starts_with("http://") || pfad.starts_with("https://") {
            pfad.to_string()
        } else {
            format!("{}/{}", self.organisation.url, pfad.trim_start_matches('/'))
        }
    }

    /// Ein Aufruf: schicken, bei `429` (nur lesend) wiederholen, Fehler deuten.
    pub async fn anfrage(&self, anfrage: &Anfrage) -> Result<Antwort, DevopsFehler> {
        let mut wiederholt = 0;
        loop {
            let mut antwort = self
                .http
                .request(anfrage.methode.reqwest(), self.ziel(anfrage))
                .header(
                    USER_AGENT,
                    concat!("quiver-devops/", env!("CARGO_PKG_VERSION")),
                )
                // Basic mit leerem Benutzer und dem PAT als Passwort.
                .basic_auth("", Some(&self.organisation.pat))
                .header(
                    ACCEPT,
                    anfrage.annahme.as_deref().unwrap_or("application/json"),
                );
            // Ein `Content-Type` nur bei einem Rumpf – und nur, wenn der Aufruf
            // keinen eigenen setzt (JSON-Patch bringt seinen mit).
            if anfrage.inhalt.is_some() && !anfrage.setzt_content_type() {
                antwort = antwort.header(CONTENT_TYPE, "application/json");
            }
            for (name, wert) in &anfrage.kopfzeilen {
                antwort = antwort.header(name.as_str(), wert.as_str());
            }
            if let Some(inhalt) = &anfrage.inhalt {
                antwort = antwort.body(inhalt.clone());
            }
            let antwort = antwort
                .send()
                .await
                .map_err(|fehler| DevopsFehler::netz(&fehler))?;

            let status = antwort.status().as_u16();
            let wartezeit = wartezeit(antwort.headers());
            let rumpf = antwort.text().await.unwrap_or_default();

            // 429: nur lesende Aufrufe werden wiederholt.
            if status == 429 && anfrage.ist_lesend() {
                if wiederholt < WIEDERHOLUNGEN {
                    wiederholt += 1;
                    tokio::time::sleep(wartezeit.min(WARTEN_HOECHSTENS)).await;
                    continue;
                }
                return Err(DevopsFehler::Http {
                    status,
                    text: format!(
                        "Zu viele Anfragen (429) – nach {WIEDERHOLUNGEN} Wiederholungen \
                         aufgegeben, bitte kurz warten."
                    ),
                });
            }

            if status == 401 || (status == 203 && ist_html(&rumpf)) {
                // Kein Wiederholen: ein falscher PAT wird davon nicht besser.
                return Err(DevopsFehler::Anmeldung {
                    verbindung: self.organisation.name.clone(),
                });
            }

            if (200..300).contains(&status) {
                return Ok(Antwort {
                    status,
                    text: rumpf,
                });
            }
            return Err(DevopsFehler::aus_antwort(status, &rumpf));
        }
    }

    /// Ein lesender Aufruf, der JSON zurückgibt.
    pub async fn json(&self, anfrage: &Anfrage) -> Result<serde_json::Value, DevopsFehler> {
        let antwort = self.anfrage(anfrage).await?;
        lesen(&antwort.text)
    }

    // ------------------------------------------------------------- innen

    /// Die vollständige Adresse inklusive `api-version`.
    fn ziel(&self, anfrage: &Anfrage) -> String {
        let mut abfrage = anfrage.abfrage.clone();
        if !abfrage.iter().any(|(name, _)| name == "api-version") {
            abfrage.push(("api-version".to_string(), API.to_string()));
        }
        let url = self.url(&anfrage.pfad);
        if abfrage.is_empty() {
            return url;
        }
        let teil: Vec<String> = abfrage
            .iter()
            .map(|(name, wert)| format!("{}={}", kodieren(name), kodieren(wert)))
            .collect();
        format!("{url}?{}", teil.join("&"))
    }
}

/// Text als JSON lesen; ein kaputter Rumpf wird ein kurzer Fehler.
pub fn lesen(text: &str) -> Result<serde_json::Value, DevopsFehler> {
    serde_json::from_str(text).map_err(|fehler| DevopsFehler::Json {
        text: format!("{} ({})", fehler, kurz(text)),
    })
}

/// Ein Pfadsegment kodieren (`SCODA`, `PI Agent Reviewer` → `PI%20Agent%20Reviewer`).
pub fn kodieren(wert: &str) -> String {
    let mut ergebnis = String::with_capacity(wert.len());
    for byte in wert.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'$' => {
                ergebnis.push(byte as char)
            }
            _ => ergebnis.push_str(&format!("%{byte:02X}")),
        }
    }
    ergebnis
}

/// Wie lange nach `429` gewartet wird: `Retry-After` in Sekunden, sonst 1 s.
fn wartezeit(kopfzeilen: &HeaderMap) -> Duration {
    kopfzeilen
        .get(RETRY_AFTER)
        .and_then(|wert| wert.to_str().ok())
        .and_then(|wert| wert.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(1))
}

/// Die ersten Zeichen eines Rumpfs, für Fehlertexte.
fn kurz(text: &str) -> String {
    let sauber = text.trim();
    let kurz: String = sauber.chars().take(120).collect();
    if sauber.chars().count() > 120 {
        format!("{kurz} …")
    } else {
        kurz
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn organisation() -> Organisation {
        Organisation {
            name: "neoimpulse".into(),
            url: "https://dev.azure.com/neoimpulse".into(),
            projekt: Some("SCODA".into()),
            pat: "pat-4711".into(),
            schreiben: true,
        }
    }

    #[test]
    fn die_adresse_wird_aus_der_organisation_gebaut() {
        let client = Client::neu(&organisation()).unwrap();
        assert_eq!(
            client.url("SCODA/_apis/wit/wiql"),
            "https://dev.azure.com/neoimpulse/SCODA/_apis/wit/wiql"
        );
        assert_eq!(
            client.url("/_apis/projects"),
            "https://dev.azure.com/neoimpulse/_apis/projects"
        );
        assert_eq!(
            client.url("https://andere.example.test/y"),
            "https://andere.example.test/y"
        );
    }

    #[test]
    fn jede_anfrage_traegt_die_api_version() {
        let client = Client::neu(&organisation()).unwrap();
        assert_eq!(
            client.ziel(&Anfrage::holen("_apis/projects")),
            "https://dev.azure.com/neoimpulse/_apis/projects?api-version=7.1"
        );
        // Eine eigene Version (Kommentare) wird nicht verdoppelt.
        let kommentare = Anfrage::holen("SCODA/_apis/wit/workItems/1/comments")
            .mit_abfrage("api-version", API_KOMMENTARE);
        let ziel = client.ziel(&kommentare);
        assert!(ziel.contains("api-version=7.1-preview.4"), "{ziel}");
        assert!(!ziel.contains("api-version=7.1&"), "{ziel}");
        assert_eq!(ziel.matches("api-version").count(), 1, "{ziel}");
    }

    #[test]
    fn segmente_werden_kodiert() {
        assert_eq!(kodieren("SCODA"), "SCODA");
        assert_eq!(kodieren("PI Agent Reviewer"), "PI%20Agent%20Reviewer");
        assert_eq!(kodieren("a/b"), "a%2Fb");
    }

    #[test]
    fn debug_zeigt_keinen_pat() {
        let client = Client::neu(&organisation()).unwrap();
        let text = format!("{client:?}");
        assert!(!text.contains("pat-4711"), "{text}");
        assert!(text.contains("neoimpulse"), "{text}");
    }

    #[test]
    fn der_json_patch_traegt_den_richtigen_content_type() {
        let anfrage = Anfrage::patch("SCODA/_apis/wit/workitems/1", "[]");
        assert_eq!(anfrage.methode, Methode::Patch);
        assert_eq!(
            anfrage.kopfzeilen,
            vec![("Content-Type".to_string(), JSON_PATCH.to_string())]
        );
        assert!(!anfrage.ist_lesend());
        assert!(Anfrage::holen("x").ist_lesend());
        assert!(!Anfrage::post("x", "{}").ist_lesend());
    }

    #[test]
    fn der_accept_kopf_ist_ersetzbar() {
        assert_eq!(Anfrage::holen("x").annahme, None);
        assert_eq!(
            Anfrage::holen("x")
                .mit_annahme("text/plain")
                .annahme
                .as_deref(),
            Some("text/plain")
        );
    }

    #[test]
    fn nur_lesende_aufrufe_gelten_als_wiederholbar() {
        assert!(Methode::Get.ist_lesend());
        assert!(!Methode::Post.ist_lesend());
        assert!(!Methode::Patch.ist_lesend());
        assert!(!Methode::Put.ist_lesend());
    }
}
