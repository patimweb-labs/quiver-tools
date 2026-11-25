// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der ADT-Client: ein HTTP-Gespräch je System (Aufgabe 07).
//!
//! Abgelesen bei `abap-adt-api` (`AdtHTTP.js`), in Rust neu gebaut:
//!
//! - **Anmeldung** mit HTTP Basic (`benutzer`/`passwort`) und den
//!   Abfrageparametern `sap-client` (Mandant) und `sap-language` (Sprache).
//! - **CSRF**: der erste Aufruf holt mit `x-csrf-token: fetch` ein Token
//!   (Antwortkopf `x-csrf-token`). Antwortet der Server auf einen Aufruf mit
//!   `403` und `x-csrf-token: Required`, wird das Token neu geholt und der
//!   Aufruf **einmal** wiederholt.
//! - **Cookies** (`SAP_SESSIONID_*`) je System: der Client merkt sie sich und
//!   schickt sie bei jedem Aufruf mit.
//! - **401** heißt „Anmeldung abgelehnt“ und wird **nicht** wiederholt: SAP
//!   sperrt Benutzer nach zu vielen Fehlversuchen.
//!
//! Zwei Betriebsarten teilen sich diesen Client:
//!
//! - **Ohne Sitzung** (Teil 1, alle lesenden Werkzeuge): kein
//!   `X-sap-adt-sessiontype`, Cookies nur innerhalb eines Aufrufs.
//! - **Mit Sitzung** (Teil 2, [`crate::adt::sitzung`]): der Kopf
//!   `X-sap-adt-sessiontype: stateful` hängt an jedem Aufruf, damit SAP alle
//!   Aufrufe **einer** Sitzung zuordnet – nur dort gilt ein Lock-Handle.
//!   In dieser Betriebsart entscheidet die Sitzung, was ein `401` bedeutet.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

use reqwest::header::{HeaderMap, ACCEPT, CONTENT_TYPE, COOKIE, SET_COOKIE};
use reqwest::Method;

use crate::adt::fehler::AdtFehler;
use crate::systeme::System;

/// Wie lange ein einzelner Aufruf höchstens dauern darf.
const ZEITLIMIT: Duration = Duration::from_secs(120);

/// Der Kopf, mit dem ADT das CSRF-Token führt.
const CSRF: &str = "x-csrf-token";

/// Der Kopf, der die Betriebsart der Sitzung festlegt.
const SITZUNGSART: &str = "X-sap-adt-sessiontype";

/// Was ein Aufruf vom Server will.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Methode {
    Get,
    Post,
    Put,
}

impl Methode {
    fn reqwest(self) -> Method {
        match self {
            Methode::Get => Method::GET,
            Methode::Post => Method::POST,
            Methode::Put => Method::PUT,
        }
    }
}

/// Ein Aufruf an den ADT-Server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anfrage {
    pub methode: Methode,
    /// Pfad ab dem Host (`/sap/bc/adt/...`) oder eine vollständige Adresse.
    pub pfad: String,
    /// Zusätzliche Abfrageparameter; `sap-client` und `sap-language` kommen dazu.
    pub abfrage: Vec<(String, String)>,
    /// `Accept`; ohne Angabe `*/*`.
    pub annahme: Option<String>,
    /// Rumpf als Text (Content-Type `text/plain; charset=utf-8`).
    pub inhalt: Option<String>,
    /// Weitere Kopfzeilen, etwa ein anderer `Content-Type`.
    pub kopfzeilen: Vec<(String, String)>,
}

impl Anfrage {
    /// Ein lesender Aufruf.
    pub fn holen(pfad: impl Into<String>) -> Anfrage {
        Anfrage {
            methode: Methode::Get,
            pfad: pfad.into(),
            abfrage: Vec::new(),
            annahme: None,
            inhalt: None,
            kopfzeilen: Vec::new(),
        }
    }

    /// Ein Aufruf mit Rumpf.
    pub fn senden(methode: Methode, pfad: impl Into<String>, inhalt: impl Into<String>) -> Anfrage {
        Anfrage {
            methode,
            pfad: pfad.into(),
            abfrage: Vec::new(),
            annahme: None,
            inhalt: Some(inhalt.into()),
            kopfzeilen: Vec::new(),
        }
    }

    /// Ein Aufruf ohne Rumpf – etwa `POST <objekt-url>?_action=LOCK`.
    pub fn ohne_inhalt(methode: Methode, pfad: impl Into<String>) -> Anfrage {
        Anfrage {
            methode,
            pfad: pfad.into(),
            abfrage: Vec::new(),
            annahme: None,
            inhalt: None,
            kopfzeilen: Vec::new(),
        }
    }

    /// Ein Abfrageparameter.
    pub fn mit_abfrage(mut self, name: impl Into<String>, wert: impl Into<String>) -> Anfrage {
        self.abfrage.push((name.into(), wert.into()));
        self
    }

    /// Der `Accept`-Kopf.
    pub fn mit_annahme(mut self, annahme: impl Into<String>) -> Anfrage {
        self.annahme = Some(annahme.into());
        self
    }

    /// Eine weitere Kopfzeile.
    pub fn mit_kopf(mut self, name: impl Into<String>, wert: impl Into<String>) -> Anfrage {
        self.kopfzeilen.push((name.into(), wert.into()));
        self
    }
}

/// Was der Server geantwortet hat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Antwort {
    pub status: u16,
    pub text: String,
    /// Der `Location`-Kopf, falls der Server einen schickt (neue Objekte).
    pub ort: Option<String>,
}

/// Der Client eines Systems.
pub struct Client {
    system: System,
    http: reqwest::Client,
    /// Das CSRF-Token, sobald es eines gibt.
    token: Mutex<Option<String>>,
    /// `SAP_SESSIONID_*` und was der Server sonst setzt (Name → Wert).
    kekse: Mutex<BTreeMap<String, String>>,
    /// `true` heißt: jeder Aufruf trägt `X-sap-adt-sessiontype: stateful`.
    stateful: bool,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("system", &self.system.name)
            .field("stateful", &self.stateful)
            .field(
                "token",
                &self.token.lock().map(|t| t.is_some()).unwrap_or(false),
            )
            .finish()
    }
}

/// Nur unter diesem Pfad ruft das Werkzeug das System auf.
const ADT_WURZEL: &str = "/sap/bc/adt/";

impl Client {
    /// Ein Client für dieses System. Das Passwort steht nur im Speicher.
    pub fn neu(system: &System) -> Result<Client, AdtFehler> {
        Client::bauen(system, false)
    }

    /// Ein Client, dessen Aufrufe **eine** Sitzung bilden (Teil 2).
    pub fn neu_stateful(system: &System) -> Result<Client, AdtFehler> {
        Client::bauen(system, true)
    }

    fn bauen(system: &System, stateful: bool) -> Result<Client, AdtFehler> {
        // Keine Weiterleitung, feste Frist (kern::net). Das Zertifikat ist im
        // System schon nach der Regel des Kerns entschieden: abschalten nur auf
        // dem Loopback, sonst Prüfung oder eigenes CA-Zertifikat.
        let host = system.host().to_string();
        let (regel, _warnung) = quiver_tools_kern::net::zertifikat(
            !system.zertifikat_ungeprueft(),
            &host,
            system.ca_zertifikat.as_deref(),
        );
        let bauen = regel
            .anwenden(quiver_tools_kern::net::bauen(concat!(
                "quiver-abap/",
                env!("CARGO_PKG_VERSION")
            )))
            .map_err(|fehler| AdtFehler::Netz {
                text: fehler.to_string(),
            })?;
        let http = bauen
            .timeout(ZEITLIMIT)
            .build()
            .map_err(|fehler| AdtFehler::Netz {
                text: fehler.to_string(),
            })?;
        Ok(Client {
            system: system.clone(),
            http,
            token: Mutex::new(None),
            kekse: Mutex::new(BTreeMap::new()),
            stateful,
        })
    }

    /// Bilden die Aufrufe dieses Clients eine Sitzung?
    pub fn ist_stateful(&self) -> bool {
        self.stateful
    }

    /// Das System, mit dem dieser Client spricht.
    pub fn system(&self) -> &System {
        &self.system
    }

    /// Ein Aufruf: Token holen, schicken, bei `403 Required` einmal wiederholen.
    pub async fn anfrage(&self, anfrage: &Anfrage) -> Result<Antwort, AdtFehler> {
        let mut wiederholt = false;
        loop {
            let token = self.token().await?;
            let antwort = self.schicken(anfrage, &token).await?;
            self.kekse_merken(antwort.headers());

            let status = antwort.status().as_u16();
            let ort = antwort
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|wert| wert.to_str().ok())
                .map(|wert| wert.trim().to_string())
                .filter(|wert| !wert.is_empty());
            let csrf_erneuern = status == 403
                && antwort
                    .headers()
                    .get(CSRF)
                    .and_then(|wert| wert.to_str().ok())
                    .map(|wert| wert.eq_ignore_ascii_case("required"))
                    .unwrap_or(false);
            let rumpf = quiver_tools_kern::net::text_begrenzt(antwort)
                .await
                .unwrap_or_default();

            if (200..300).contains(&status) {
                return Ok(Antwort {
                    status,
                    text: rumpf,
                    ort,
                });
            }
            if status == 401 && !self.stateful {
                // Kein Wiederholen: das kostet dem Benutzer den Zugang.
                return Err(AdtFehler::Anmeldung {
                    system: self.system.name.clone(),
                });
            }
            if (csrf_erneuern || rumpf.contains("CSRF")) && !wiederholt {
                wiederholt = true;
                self.token_vergessen();
                continue;
            }
            return Err(AdtFehler::aus_antwort(status, &rumpf));
        }
    }

    /// Ein lesender Aufruf, der Text zurückgibt.
    pub async fn holen(&self, anfrage: &Anfrage) -> Result<String, AdtFehler> {
        Ok(self.anfrage(anfrage).await?.text)
    }

    // ------------------------------------------------------------- innen

    /// Das CSRF-Token, notfalls neu geholt.
    async fn token(&self) -> Result<String, AdtFehler> {
        if let Some(token) = self.token_lesen() {
            return Ok(token);
        }
        self.token_holen().await
    }

    fn token_lesen(&self) -> Option<String> {
        self.token
            .lock()
            .unwrap_or_else(|gift| gift.into_inner())
            .clone()
    }

    fn token_merken(&self, token: &str) {
        *self.token.lock().unwrap_or_else(|gift| gift.into_inner()) = Some(token.to_string());
    }

    fn token_vergessen(&self) {
        *self.token.lock().unwrap_or_else(|gift| gift.into_inner()) = None;
    }

    /// Das Token holen: derselbe Weg, den `abap-adt-api` beim Anmelden geht.
    async fn token_holen(&self) -> Result<String, AdtFehler> {
        let anfrage = Anfrage::holen("/sap/bc/adt/compatibility/graph")
            .mit_annahme("*/*")
            .mit_kopf(CSRF, "fetch");
        let antwort = self.schicken(&anfrage, "fetch").await?;
        self.kekse_merken(antwort.headers());

        let status = antwort.status().as_u16();
        let token = antwort
            .headers()
            .get(CSRF)
            .and_then(|wert| wert.to_str().ok())
            .map(str::to_string)
            .filter(|token| !token.is_empty());
        let rumpf = quiver_tools_kern::net::text_begrenzt(antwort)
            .await
            .unwrap_or_default();

        if status == 401 {
            return Err(AdtFehler::Anmeldung {
                system: self.system.name.clone(),
            });
        }
        if !(200..300).contains(&status) {
            return Err(AdtFehler::aus_antwort(status, &rumpf));
        }
        match token {
            Some(token) => {
                self.token_merken(&token);
                Ok(token)
            }
            None => Err(AdtFehler::Csrf {
                system: self.system.name.clone(),
            }),
        }
    }

    /// Den eigentlichen Aufruf schicken.
    async fn schicken(
        &self,
        anfrage: &Anfrage,
        token: &str,
    ) -> Result<reqwest::Response, AdtFehler> {
        let mut ziel = self
            .http
            .request(anfrage.methode.reqwest(), self.url(&anfrage.pfad)?);
        ziel = ziel
            .query(&[("sap-client", self.system.mandant.as_str())])
            .query(&[("sap-language", self.system.sprache.as_str())]);
        for (name, wert) in &anfrage.abfrage {
            ziel = ziel.query(&[(name.as_str(), wert.as_str())]);
        }
        ziel = ziel
            .header(ACCEPT, anfrage.annahme.as_deref().unwrap_or("*/*"))
            .header(CSRF, token)
            .header(COOKIE, self.kekse_string())
            .basic_auth(&self.system.benutzer, Some(&self.system.passwort));
        if self.stateful {
            // Nur so gilt ein Lock-Handle in dieser Sitzung.
            ziel = ziel.header(SITZUNGSART, "stateful");
        }
        for (name, wert) in &anfrage.kopfzeilen {
            ziel = ziel.header(name.as_str(), wert.as_str());
        }
        if let Some(inhalt) = &anfrage.inhalt {
            // Ein vom Aufrufer gesetzter `Content-Type` gilt (z. B. `application/*`
            // für XML-Rümpfe oder der Textelemente-Typ); sonst Klartext.
            let gesetzt = anfrage
                .kopfzeilen
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case("content-type"));
            if !gesetzt {
                ziel = ziel.header(CONTENT_TYPE, "text/plain; charset=utf-8");
            }
            ziel = ziel.body(inhalt.clone());
        }
        ziel.send().await.map_err(|fehler| AdtFehler::netz(&fehler))
    }

    /// Die Adresse eines Aufrufs – immer auf dem **eingetragenen** System.
    ///
    /// Jeder Aufruf trägt Benutzer und Passwort. Ein Pfad (auch aus einer
    /// Antwort) darf deshalb nie auf ein anderes System zeigen: Schema, Host und
    /// Port müssen genau passen.
    fn url(&self, pfad: &str) -> Result<reqwest::Url, AdtFehler> {
        let fremd = || AdtFehler::FremdesSystem {
            system: self.system.name.clone(),
        };
        let pfad = pfad.trim();
        if pfad.chars().any(|zeichen| zeichen.is_control()) {
            return Err(fremd());
        }
        let basis = reqwest::Url::parse(&self.system.url).map_err(|_| fremd())?;
        let absolut = pfad.to_ascii_lowercase();
        let adresse = if absolut.starts_with("http:") || absolut.starts_with("https:") {
            pfad.to_string()
        } else if pfad.starts_with('/') {
            format!("{}{}", self.system.url.trim_end_matches('/'), pfad)
        } else {
            format!("{}/{}", self.system.url.trim_end_matches('/'), pfad)
        };
        let adresse = reqwest::Url::parse(&adresse).map_err(|_| fremd())?;
        let gleich = adresse.scheme() == basis.scheme()
            && adresse.host_str() == basis.host_str()
            && adresse.port_or_known_default() == basis.port_or_known_default()
            && adresse.username().is_empty()
            && adresse.password().is_none()
            // `Url` hat `..` schon aufgelöst; kodierte Punkte lehnen wir ab.
            && adresse.path().starts_with(ADT_WURZEL)
            && !adresse.path().to_ascii_lowercase().contains("%2e%2e");
        if gleich {
            Ok(adresse)
        } else {
            Err(fremd())
        }
    }

    /// Die gemerkten Kekse als `Cookie`-Kopf.
    fn kekse_string(&self) -> String {
        self.kekse
            .lock()
            .unwrap_or_else(|gift| gift.into_inner())
            .iter()
            .map(|(name, wert)| format!("{name}={wert}"))
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// `Set-Cookie` merken; nur `Name=Wert` zählt.
    fn kekse_merken(&self, kopfzeilen: &HeaderMap) {
        let mut kekse = self.kekse.lock().unwrap_or_else(|gift| gift.into_inner());
        for wert in kopfzeilen.get_all(SET_COOKIE) {
            let Ok(wert) = wert.to_str() else { continue };
            let erstes = wert.split(';').next().unwrap_or_default().trim();
            if let Some((name, inhalt)) = erstes.split_once('=') {
                if !name.trim().is_empty() {
                    kekse.insert(name.trim().to_string(), inhalt.trim().to_string());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn system() -> System {
        System {
            name: "dev-100".into(),
            benutzer: "DEVELOPER".into(),
            url: "https://sap.example.test:44300".into(),
            mandant: "100".into(),
            sprache: "DE".into(),
            passwort: "geheim-4711".into(),
            zertifikat_pruefen: true,
            schreiben: true,
            ca_zertifikat: None,
        }
    }

    #[tokio::test]
    async fn die_adresse_bleibt_auf_dem_eingetragenen_system() {
        let client = Client::neu(&system()).unwrap();
        for (pfad, soll) in [
            ("/sap/bc/adt/x", "/sap/bc/adt/x"),
            ("sap/bc/adt/x", "/sap/bc/adt/x"),
            (
                "https://sap.example.test:44300/sap/bc/adt/x",
                "/sap/bc/adt/x",
            ),
            (
                "/sap/bc/adt/oo/classes/%2fneo%2fcl/source",
                "/sap/bc/adt/oo/classes/%2fneo%2fcl/source",
            ),
        ] {
            assert_eq!(
                client.url(pfad).unwrap().as_str(),
                format!("https://sap.example.test:44300{soll}")
            );
        }
        for boese in [
            "https://andere.example.test/sap/bc/adt/y",
            "HTTPS://andere.example.test/sap/bc/adt/y",
            "https:andere.example.test/sap/bc/adt/y",
            "https:\\\\andere.example.test/sap/bc/adt/y",
            "http://sap.example.test:44300/sap/bc/adt/y",
            "https://sap.example.test/sap/bc/adt/y",
            "https://sap.example.test:44300@andere.example.test/sap/bc/adt/y",
            "https://x:y@sap.example.test:44300/sap/bc/adt/y",
            "//andere.example.test/sap/bc/adt/y",
            "@andere.example.test/sap/bc/adt/y",
            "/sap/bc/adt/../../public/ping",
            "/sap/bc/adt/%2e%2e/%2E%2E/x",
            "/sap/bc/gui/x",
            "/sap/bc/adt/x\r\nHost: andere",
            "",
        ] {
            assert!(
                matches!(client.url(boese), Err(AdtFehler::FremdesSystem { .. })),
                "{boese:?} -> {:?}",
                client.url(boese)
            );
        }
    }

    /// Ein Aufruf mit fremder Adresse verlässt den Rechner gar nicht erst –
    /// auch nicht für das CSRF-Token – und eine Weiterleitung wird nicht verfolgt.
    #[tokio::test]
    async fn zugangsdaten_gehen_nie_an_ein_fremdes_system() {
        use std::io::{Read, Write};
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        let fremd = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let fremd_url = format!("http://{}", fremd.local_addr().unwrap());
        let treffer = Arc::new(AtomicUsize::new(0));
        let zaehler = treffer.clone();
        std::thread::spawn(move || {
            for strom in fremd.incoming().flatten() {
                zaehler.fetch_add(1, Ordering::SeqCst);
                drop(strom);
            }
        });
        let eigen = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let eigen_url = format!("http://{}", eigen.local_addr().unwrap());
        let ziel = fremd_url.clone();
        std::thread::spawn(move || {
            for mut strom in eigen.incoming().flatten() {
                let mut puffer = [0u8; 4096];
                let _ = strom.read(&mut puffer);
                let _ = write!(
                    strom,
                    "HTTP/1.1 307 Temporary Redirect\r\nLocation: {ziel}/sap/bc/adt/x\r\n\
                     x-csrf-token: t\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                );
            }
        });
        let mut system = system();
        system.url = eigen_url;
        let client = Client::neu(&system).unwrap();
        for pfad in [
            format!("{fremd_url}/sap/bc/adt/x"),
            "/sap/bc/adt/x".to_string(),
        ] {
            assert!(client.holen(&Anfrage::holen(&pfad)).await.is_err());
        }
        assert_eq!(treffer.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn debug_zeigt_kein_passwort() {
        let client = Client::neu(&system()).unwrap();
        let text = format!("{client:?}");
        assert!(!text.contains("geheim-4711"), "{text}");
        assert!(text.contains("dev-100"), "{text}");
    }

    #[test]
    fn ein_anfrageobjekt_laesst_sich_zusammenbauen() {
        let anfrage = Anfrage::senden(Methode::Post, "/sap/bc/adt/x", "inhalt")
            .mit_abfrage("rowNumber", "10")
            .mit_annahme("application/*")
            .mit_kopf("Content-Type", "application/*");
        assert_eq!(anfrage.methode, Methode::Post);
        assert_eq!(anfrage.inhalt.as_deref(), Some("inhalt"));
        assert_eq!(anfrage.abfrage, vec![("rowNumber".into(), "10".into())]);
        assert_eq!(anfrage.annahme.as_deref(), Some("application/*"));
        assert_eq!(anfrage.kopfzeilen.len(), 1);

        // Ohne Rumpf (Sperren, Entsperren).
        let ohne = Anfrage::ohne_inhalt(Methode::Post, "/sap/bc/adt/oo/classes/zcl_demo");
        assert_eq!(ohne.methode, Methode::Post);
        assert!(ohne.inhalt.is_none());
    }

    #[test]
    fn nur_die_sitzung_schaltet_den_sitzungskopf() {
        let system = system();
        assert!(!Client::neu(&system).unwrap().ist_stateful());
        let sitzung = Client::neu_stateful(&system).unwrap();
        assert!(sitzung.ist_stateful());
        // Auch im Debug steht kein Passwort.
        assert!(!format!("{sitzung:?}").contains("geheim-4711"));
    }
}
