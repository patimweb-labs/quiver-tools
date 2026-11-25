// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der Jira-Client: ein HTTP-Gespräch je Site (Aufgabe 13).
//!
//! In Rust neu gebaut, abgelesen an der REST-API v3 von Jira Cloud und an der
//! Vorlage `pi-jira-multiproject`. Fünf Regeln stehen hier:
//!
//! - **Anmeldung** mit HTTP Basic: `Authorization: Basic base64(email:token)`.
//!   E-Mail und Token stehen nur in dieser Kopfzeile.
//! - **Keine Weiterleitung.** `redirect::Policy::none()` – der Token geht nur an
//!   den eingetragenen Host. Eine Folgeseite (`next`, `_links.next`) mit fremdem
//!   Host wird **vor** der Anfrage abgelehnt ([`Client::naechste_seite`]).
//! - **401** heißt „Token gilt nicht“ und wird nicht wiederholt; **403** heißt
//!   „keine Rechte“.
//! - **429/503** wird nur bei **lesenden** Aufrufen wiederholt, höchstens
//!   zweimal, mit `Retry-After` (höchstens 30 s). Ein Anlegen, Ändern oder
//!   Kommentieren wird nie wiederholt: ob es ankam, weiß nur der Server.
//! - **Leerer Rumpf** (204) ist Erfolg, kein Deserialisierungsfehler.
//! - **Pfadwerte** mit `.` oder `..` (auch `%2E%2E`) und leere Werte werden
//!   **ohne Anfrage** abgelehnt: `url` normalisiert `%2E%2E` sonst stillschweigend
//!   zu `..` und der Pfad verließe sein Segment ([`pfadwert`], [`Client::ziel`]).
//! - **Der Rumpf gehört zum Ergebnis.** Lässt er sich nicht lesen, ist das bei
//!   einem lesenden Aufruf ein Fehler (nie „leer“), bei einer Änderung mit 2xx
//!   „ausgeführt, Antwort nicht lesbar“. Kommt bei einer Änderung **keine**
//!   Antwort, ist der Ausgang unklar – niemand wiederholt blind.

use std::time::Duration;

use reqwest::header::{HeaderMap, ACCEPT, CONTENT_TYPE, RETRY_AFTER, USER_AGENT};
use reqwest::Method;
use url::Url;

use crate::fehler::JiraFehler;
use crate::sites::Site;

/// Wie lange ein einzelner Aufruf höchstens dauern darf.
const ZEITLIMIT: Duration = Duration::from_secs(60);

/// So oft ein lesender Aufruf bei `429`/`503` wiederholt wird.
const WIEDERHOLUNGEN: u32 = 2;

/// So lange wird bei `429`/`503` höchstens gewartet.
const WARTEN_HOECHSTENS: Duration = Duration::from_secs(30);

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

    /// Lesend heißt: darf bei `429`/`503` wiederholt werden.
    pub fn ist_lesend(self) -> bool {
        self == Methode::Get
    }
}

/// Ein Aufruf an Jira.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anfrage {
    pub methode: Methode,
    /// Pfad ab der API-Basis, z. B. `issue/ABC-1`; bereits kodierte Segmente.
    /// Eine **vollständige** Adresse ist nur erlaubt, wenn der Host stimmt.
    pub pfad: String,
    /// Zusätzliche Abfrageparameter.
    pub abfrage: Vec<(String, String)>,
    /// Rumpf als Text (JSON).
    pub inhalt: Option<String>,
    /// Der `Accept`-Kopf; ohne Angabe `application/json`.
    pub annahme: Option<String>,
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

    /// Ein Aufruf mit Rumpf (`PUT`).
    pub fn put(pfad: impl Into<String>, inhalt: impl Into<String>) -> Anfrage {
        Anfrage::neu(Methode::Put, pfad).mit_inhalt(inhalt)
    }

    /// Ein Aufruf mit eigener Methode.
    pub fn neu(methode: Methode, pfad: impl Into<String>) -> Anfrage {
        Anfrage {
            methode,
            pfad: pfad.into(),
            abfrage: Vec::new(),
            inhalt: None,
            annahme: None,
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

    /// Der `Accept`-Kopf, wenn JSON nicht passt.
    pub fn mit_annahme(mut self, annahme: impl Into<String>) -> Anfrage {
        self.annahme = Some(annahme.into());
        self
    }

    /// Ist das ein lesender Aufruf?
    ///
    /// Die JQL-Suche ist technisch ein `POST`, ändert aber nichts.
    pub fn ist_lesend(&self) -> bool {
        self.methode.ist_lesend() || (self.methode == Methode::Post && self.pfad == "search/jql")
    }
}

/// Was der Server geantwortet hat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Antwort {
    pub status: u16,
    pub text: String,
}

/// Der Client einer Site.
pub struct Client {
    site: Site,
    http: reqwest::Client,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("site", &self.site.name)
            .finish()
    }
}

impl Client {
    /// Ein Client für diese Site. E-Mail und Token stehen nur im Speicher.
    pub fn neu(site: &Site) -> Result<Client, JiraFehler> {
        Client::mit_zeitlimit(site, ZEITLIMIT)
    }

    /// Wie [`Client::neu`], aber mit eigenem Zeitlimit (für die Tests: eine
    /// Zeitüberschreitung darf nicht 60 s dauern).
    fn mit_zeitlimit(site: &Site, limit: Duration) -> Result<Client, JiraFehler> {
        // Keine Weiterleitung, feste Frist (kern::net); ein eigenes
        // CA-Zertifikat ersetzt den Systemspeicher, sonst bleibt die Prüfung an.
        let host = site
            .url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split(['/', ':'])
            .next()
            .unwrap_or(&site.url);
        let (regel, _warnung) =
            quiver_tools_kern::net::zertifikat(true, host, site.ca_zertifikat.as_deref());
        let http = regel
            .anwenden(quiver_tools_kern::net::bauen(concat!(
                "quiver-jira/",
                env!("CARGO_PKG_VERSION")
            )))
            .and_then(|bauer| {
                bauer
                    .timeout(limit)
                    .build()
                    .map_err(|fehler| quiver_tools_kern::net::NetFehler::Client(fehler.to_string()))
            })
            .map_err(|fehler| JiraFehler::Netz {
                text: fehler.to_string(),
            })?;
        Ok(Client {
            site: site.clone(),
            http,
        })
    }

    /// Die Site, mit der dieser Client spricht.
    pub fn site(&self) -> &Site {
        &self.site
    }

    /// Die Adresse eines relativen Pfads an der API-Basis.
    pub fn url(&self, pfad: &str) -> String {
        let basis = self.site.basis();
        if pfad.is_empty() {
            basis
        } else {
            format!("{basis}/{}", pfad.trim_start_matches('/'))
        }
    }

    /// Eine vom Server gelieferte Folgeseite prüfen.
    ///
    /// Erlaubt ist nur derselbe Host (und dasselbe Schema) wie die eingetragene
    /// Site. Alles andere ist ein [`JiraFehler::FremderHost`] – **ohne**
    /// Anfrage, damit der Token nicht an einen fremden Host geht.
    pub fn naechste_seite(&self, link: &str) -> Result<String, JiraFehler> {
        let eigene = Url::parse(&self.site.basis()).map_err(|_| JiraFehler::FremderHost {
            host: "(eigene Adresse)".to_string(),
        })?;
        let url = Url::parse(link).map_err(|_| JiraFehler::FremderHost {
            host: link.to_string(),
        })?;
        let fremd = JiraFehler::FremderHost {
            host: url.host_str().unwrap_or("(ohne Host)").to_string(),
        };
        // Zugangsdaten und ein Fragment gehören nicht in eine Folgeseite.
        if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
            return Err(fremd);
        }
        // Schema, Host und Port – verglichen am geparsten `Url`, nicht an einer
        // eigenen Zerlegung.
        if url.scheme() != eigene.scheme() || !self.site.gleicher_host(&url) {
            return Err(fremd);
        }
        Ok(url.to_string())
    }

    /// Ein Aufruf: schicken, bei `429`/`503` (nur lesend) wiederholen, Fehler deuten.
    pub async fn anfrage(&self, anfrage: &Anfrage) -> Result<Antwort, JiraFehler> {
        let mut wiederholt = 0;
        loop {
            let ziel = self.ziel(anfrage)?;
            let mut aufbau = self
                .http
                .request(anfrage.methode.reqwest(), &ziel)
                .header(
                    USER_AGENT,
                    concat!("quiver-jira/", env!("CARGO_PKG_VERSION")),
                )
                // Basic: `email:token` – nur in dieser Kopfzeile.
                .basic_auth(&self.site.email, Some(&self.site.token))
                .header(
                    ACCEPT,
                    anfrage.annahme.as_deref().unwrap_or("application/json"),
                );
            if anfrage.inhalt.is_some() {
                aufbau = aufbau.header(CONTENT_TYPE, "application/json");
            }
            if let Some(inhalt) = &anfrage.inhalt {
                aufbau = aufbau.body(inhalt.clone());
            }
            let antwort = match aufbau.send().await {
                Ok(antwort) => antwort,
                Err(fehler) => {
                    // Lesend: eine Meldung, die nichts behauptet. Verändernd:
                    // ob die Änderung ankam, weiß nur der Server.
                    if anfrage.ist_lesend() {
                        return Err(JiraFehler::netz(&fehler));
                    }
                    return Err(JiraFehler::Unklar {
                        text: fehler.to_string(),
                    });
                }
            };

            let status = antwort.status().as_u16();
            let wartezeit = wartezeit(antwort.headers());
            // Der Rumpf gehört zum Ergebnis: ein Lesefehler ist nie „leer“.
            let rumpf = match begrenzt(antwort).await {
                Ok(text) => text,
                Err(fehler) => {
                    if (200..300).contains(&status) {
                        if anfrage.ist_lesend() {
                            return Err(JiraFehler::Lesen {
                                text: fehler.to_string(),
                            });
                        }
                        return Err(JiraFehler::Ausgefuehrt {
                            text: fehler.to_string(),
                        });
                    }
                    // Ein Fehlerstatus bleibt ein Fehler; der Rumpf fehlt nur.
                    format!("(Antwort nicht lesbar: {fehler})")
                }
            };

            // 429/503: nur lesende Aufrufe werden wiederholt.
            if (status == 429 || status == 503) && anfrage.ist_lesend() {
                if wiederholt < WIEDERHOLUNGEN {
                    wiederholt += 1;
                    tokio::time::sleep(wartezeit.min(WARTEN_HOECHSTENS)).await;
                    continue;
                }
                return Err(JiraFehler::Http {
                    status,
                    text: format!(
                        "Zu viele Anfragen ({status}) – nach {WIEDERHOLUNGEN} Wiederholungen \
                         aufgegeben, bitte kurz warten."
                    ),
                });
            }

            if status == 401 {
                // Kein Wiederholen: ein falscher Token wird davon nicht besser.
                return Err(JiraFehler::Anmeldung {
                    verbindung: self.site.name.clone(),
                });
            }
            if status == 403 {
                return Err(JiraFehler::Rechte {
                    verbindung: self.site.name.clone(),
                    text: crate::fehler::meldung_kurz(&rumpf),
                });
            }

            if (200..300).contains(&status) {
                return Ok(Antwort {
                    status,
                    text: rumpf,
                });
            }
            return Err(JiraFehler::aus_antwort(status, &rumpf));
        }
    }

    /// Ein Aufruf, der JSON zurückgibt. Ein leerer Rumpf (204) ist `Null`.
    pub async fn json(&self, anfrage: &Anfrage) -> Result<serde_json::Value, JiraFehler> {
        let antwort = self.anfrage(anfrage).await?;
        lesen(&antwort.text).map_err(|fehler| match fehler {
            // 2xx auf eine Änderung heißt: ausgeführt, auch ohne gültiges JSON.
            JiraFehler::Json { text } if !anfrage.ist_lesend() => JiraFehler::Ausgefuehrt { text },
            fehler => fehler,
        })
    }

    // ------------------------------------------------------------- innen

    /// Die vollständige Adresse inklusive Abfrage.
    fn ziel(&self, anfrage: &Anfrage) -> Result<String, JiraFehler> {
        // Eine vollständige Adresse (Folgeseite) wird vorher geprüft.
        let vollstaendig =
            anfrage.pfad.starts_with("http://") || anfrage.pfad.starts_with("https://");
        if !vollstaendig {
            // Vor dem Zusammensetzen: `url` würde `%2E%2E` zu `..` normalisieren.
            punktsegmente(&anfrage.pfad, "pfad")?;
        }
        let basis = if vollstaendig {
            self.naechste_seite(&anfrage.pfad)?
        } else {
            self.url(&anfrage.pfad)
        };
        let adresse = if anfrage.abfrage.is_empty() {
            basis
        } else {
            let teil: Vec<String> = anfrage
                .abfrage
                .iter()
                .map(|(name, wert)| format!("{}={}", kodieren(name), kodieren(wert)))
                .collect();
            let trenner = if basis.contains('?') { '&' } else { '?' };
            format!("{basis}{trenner}{}", teil.join("&"))
        };
        // Die Wahrheit ist die fertige `Url` – nicht der String davor.
        let url = Url::parse(&adresse).map_err(|fehler| JiraFehler::Wert {
            name: "pfad".to_string(),
            grund: format!("keine gültige Adresse ({fehler})"),
        })?;
        if let Some(segment) = url
            .path_segments()
            .map(|segmente| segmente.collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
            .find(|segment| *segment == "." || *segment == "..")
        {
            return Err(JiraFehler::Wert {
                name: "pfad".to_string(),
                grund: format!("das Segment „{segment}“ verlässt den Pfad"),
            });
        }
        Ok(url.to_string())
    }
}

/// Einen Pfadwert prüfen und kodieren (`PI Agent` → `PI%20Agent`).
///
/// Abgelehnt werden **leere** Werte und Werte mit einem Segment `.` oder `..`
/// (auch `%2E`), denn `url` normalisiert solche Segmente: aus
/// `issue/%2E%2E/x` würde stillschweigend `x`. Das geschieht **vor** jeder
/// Anfrage.
pub fn pfadwert(wert: &str, name: &str) -> Result<String, JiraFehler> {
    let wert = wert.trim();
    if wert.is_empty() {
        return Err(JiraFehler::Wert {
            name: name.to_string(),
            grund: "der Wert ist leer".to_string(),
        });
    }
    punktsegmente(wert, name)?;
    Ok(kodieren(wert))
}

/// Steckt in einem Pfad ein Segment `.` oder `..`? (auch prozent-kodiert)
fn punktsegmente(pfad: &str, name: &str) -> Result<(), JiraFehler> {
    for segment in pfad.split('/') {
        if ist_punktsegment(segment) {
            return Err(JiraFehler::Wert {
                name: name.to_string(),
                grund: format!("das Segment „{segment}“ verlässt den Pfad"),
            });
        }
    }
    Ok(())
}

/// Dekodiert ein Segment so weit, wie es für `.`/`..` zählt: `%2E` → `.`.
fn ist_punktsegment(segment: &str) -> bool {
    let mut dekodiert = String::with_capacity(segment.len());
    let bytes = segment.as_bytes();
    let mut stelle = 0;
    while stelle < bytes.len() {
        if bytes[stelle] == b'%' && stelle + 2 < bytes.len() {
            let hex = &segment[stelle + 1..stelle + 3];
            if hex.eq_ignore_ascii_case("2e") {
                dekodiert.push('.');
                stelle += 3;
                continue;
            }
        }
        dekodiert.push(bytes[stelle] as char);
        stelle += 1;
    }
    dekodiert == "." || dekodiert == ".."
}

/// Text als JSON lesen; ein leerer Rumpf ist `Null`, ein kaputter ein Fehler.
pub fn lesen(text: &str) -> Result<serde_json::Value, JiraFehler> {
    if text.trim().is_empty() {
        return Ok(serde_json::Value::Null);
    }
    serde_json::from_str(text).map_err(|fehler| JiraFehler::Json {
        text: format!("{} ({})", fehler, kurz(text)),
    })
}

/// Eine Antwort lesen – höchstens 8 MB, sonst Abbruch als Fehler.
async fn begrenzt(antwort: reqwest::Response) -> Result<String, quiver_tools_kern::net::NetFehler> {
    quiver_tools_kern::net::text_begrenzt(antwort).await
}

/// Ein Pfadsegment kodieren (`PI Agent` → `PI%20Agent`, `..` → `%2E%2E`).
///
/// Erlaubt bleiben nur Buchstaben, Ziffern, `-`, `_` und `~`. Damit sind `/`,
/// `?`, `#`, `%`, `\` und auch `..` sicher: sie können keinen Pfad verlassen.
/// **Allein** reicht das nicht: `url` normalisiert `%2E%2E` wieder zu `..`.
/// Darum geht jeder Wert aus einem Werkzeug durch [`pfadwert`].
pub fn kodieren(wert: &str) -> String {
    let mut ergebnis = String::with_capacity(wert.len());
    for byte in wert.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'~' => {
                ergebnis.push(byte as char)
            }
            _ => ergebnis.push_str(&format!("%{byte:02X}")),
        }
    }
    ergebnis
}

/// Wie lange nach `429`/`503` gewartet wird: `Retry-After` in Sekunden, sonst 1 s.
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
    use quiver_tools_kern::zugaenge::Zugang as Eintrag;

    fn site(url: &str) -> Site {
        Site::aus_zugang(&Eintrag {
            name: "firma".into(),
            werte: [
                ("url", url),
                ("email", "alex@example.test"),
                ("api_token", "token-4711"),
            ]
            .into_iter()
            .map(|(feld, wert)| (feld.to_string(), wert.to_string()))
            .collect(),
        })
        .unwrap()
    }

    #[test]
    fn die_adresse_wird_aus_der_site_gebaut() {
        let client = Client::neu(&site("https://firma.atlassian.net")).unwrap();
        assert_eq!(
            client.url("issue/ABC-1"),
            "https://firma.atlassian.net/rest/api/3/issue/ABC-1"
        );
        assert_eq!(client.url(""), "https://firma.atlassian.net/rest/api/3");
    }

    #[test]
    fn jede_abfrage_wird_kodiert() {
        let client = Client::neu(&site("https://firma.atlassian.net")).unwrap();
        let anfrage = Anfrage::holen("user/search").mit_abfrage("query", "Anna Bäcker & Co");
        let ziel = client.ziel(&anfrage).unwrap();
        assert_eq!(
            ziel,
            "https://firma.atlassian.net/rest/api/3/user/search?query=Anna%20B%C3%A4cker%20%26%20Co"
        );
    }

    #[test]
    fn segmente_werden_kodiert() {
        assert_eq!(kodieren("ABC-1"), "ABC-1");
        assert_eq!(kodieren("a/b"), "a%2Fb");
        assert_eq!(kodieren(".."), "%2E%2E");
        assert_eq!(kodieren("../x"), "%2E%2E%2Fx");
        assert_eq!(kodieren("a?b#c"), "a%3Fb%23c");
        assert_eq!(kodieren("a\\b"), "a%5Cb");
        assert_eq!(kodieren("100%"), "100%25");
        assert_eq!(kodieren("Müller"), "M%C3%BCller");
    }

    #[test]
    fn debug_zeigt_keinen_token_und_keine_email() {
        let client = Client::neu(&site("https://firma.atlassian.net")).unwrap();
        let text = format!("{client:?}");
        assert!(!text.contains("token-4711"), "{text}");
        assert!(!text.contains("alex@example.test"), "{text}");
        assert!(text.contains("firma"), "{text}");
    }

    #[test]
    fn eine_folgeseite_mit_fremdem_host_wird_abgelehnt() {
        let client = Client::neu(&site("https://firma.atlassian.net")).unwrap();
        // Derselbe Host ist erlaubt.
        assert_eq!(
            client
                .naechste_seite("https://firma.atlassian.net/rest/api/3/project/search?startAt=2")
                .unwrap(),
            "https://firma.atlassian.net/rest/api/3/project/search?startAt=2"
        );
        for fremd in [
            "https://boese.example.test/rest/api/3/project/search",
            "http://firma.atlassian.net/rest/api/3/project/search",
            "https://firma.atlassian.net:8443/rest/api/3/project/search",
            "https://firma.atlassian.net@boese.example.test/x",
            "https://user:geheim@firma.atlassian.net/x",
            "https://firma.atlassian.net/rest/api/3/project/search#abschnitt",
            "keine-adresse",
        ] {
            let fehler = client.naechste_seite(fremd).unwrap_err();
            assert!(
                matches!(fehler, JiraFehler::FremderHost { .. }),
                "{fremd}: {fehler}"
            );
        }
        // Auch ein absoluter Pfad in einer Anfrage wird geprüft.
        let anfrage = Anfrage::holen("https://boese.example.test/x");
        assert!(matches!(
            client.ziel(&anfrage),
            Err(JiraFehler::FremderHost { .. })
        ));
    }

    #[test]
    fn nur_lesende_aufrufe_gelten_als_wiederholbar() {
        assert!(Methode::Get.ist_lesend());
        assert!(!Methode::Post.ist_lesend());
        assert!(!Methode::Put.ist_lesend());
        assert!(Anfrage::holen("x").ist_lesend());
        assert!(!Anfrage::post("x", "{}").ist_lesend());
        assert!(!Anfrage::put("x", "{}").ist_lesend());
    }

    #[test]
    fn punkt_segmente_und_leere_werte_werden_ohne_anfrage_abgelehnt() {
        // `kodieren` allein genügt nicht: `url` macht aus `%2E%2E` wieder `..`.
        let naiv =
            Url::parse("https://firma.atlassian.net/rest/api/3/issue/%2E%2E/comment").unwrap();
        assert_eq!(naiv.path(), "/rest/api/3/comment");

        for falsch in [
            "..",
            ".",
            "../admin",
            "a/../../b",
            "%2E%2E",
            ".%2E",
            "",
            "   ",
        ] {
            assert!(pfadwert(falsch, "key").is_err(), "{falsch}");
        }
        assert_eq!(pfadwert(" ABC-1 ", "key").unwrap(), "ABC-1");
        assert_eq!(pfadwert("a/b", "key").unwrap(), "a%2Fb");
        assert_eq!(pfadwert("Ab..c", "key").unwrap(), "Ab%2E%2Ec");

        // Auch eine schon kodierte Anfrage kommt nicht durch – vor der Url.
        let client = Client::neu(&site("https://firma.atlassian.net")).unwrap();
        for pfad in ["issue/%2E%2E/comment", "issue/../comment", "issue"] {
            let ergebnis = client.ziel(&Anfrage::holen(pfad));
            if pfad == "issue" {
                assert!(ergebnis.is_ok(), "{pfad}: {ergebnis:?}");
            } else {
                assert!(
                    matches!(ergebnis, Err(JiraFehler::Wert { .. })),
                    "{pfad}: {ergebnis:?}"
                );
            }
        }

        // Ein sauberer Pfad bleibt in der fertigen `Url`, wie er ist.
        let ziel = client.ziel(&Anfrage::holen("issue/ABC-1/comment")).unwrap();
        assert_eq!(
            Url::parse(&ziel).unwrap().path(),
            "/rest/api/3/issue/ABC-1/comment"
        );
    }

    /// Ein roher Socket, der genau `bytes` antwortet und dann schließt.
    async fn roh(bytes: &'static [u8]) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let hoerer = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let adresse = hoerer.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut socket, _)) = hoerer.accept().await {
                let mut puffer = [0u8; 4096];
                let _ = socket.read(&mut puffer).await;
                let _ = socket.write_all(bytes).await;
                let _ = socket.shutdown().await;
            }
        });
        format!("http://127.0.0.1:{}", adresse.port())
    }

    /// Ein Socket, der die Anfrage annimmt und nie antwortet.
    async fn stumm() -> String {
        let hoerer = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let adresse = hoerer.local_addr().unwrap();
        tokio::spawn(async move {
            let _verbindung = hoerer.accept().await;
            tokio::time::sleep(Duration::from_secs(30)).await;
        });
        format!("http://127.0.0.1:{}", adresse.port())
    }

    #[tokio::test]
    async fn ein_unlesbarer_rumpf_ist_bei_einem_lesenden_aufruf_ein_fehler() {
        let adresse = roh(
            b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 400\r\n\r\n{\"a\":1}",
        )
        .await;
        let client = Client::neu(&site(&adresse)).unwrap();
        let fehler = client
            .anfrage(&Anfrage::holen("myself"))
            .await
            .expect_err("ein unlesbarer Rumpf ist kein Erfolg");
        assert!(matches!(fehler, JiraFehler::Lesen { .. }), "{fehler:?}");
        assert!(!fehler.to_string().contains("leer"));
    }

    #[tokio::test]
    async fn ein_unlesbarer_rumpf_nach_einer_aenderung_heisst_ausgefuehrt() {
        let adresse = roh(
            b"HTTP/1.1 201 Created\r\ncontent-type: application/json\r\ncontent-length: 400\r\n\r\n{\"id\":1}",
        )
        .await;
        let client = Client::neu(&site(&adresse)).unwrap();
        let fehler = client
            .anfrage(&Anfrage::put("issue/ABC-1", "{}"))
            .await
            .expect_err("unlesbar ist kein Erfolg");
        assert!(fehler.to_string().starts_with("Ausgeführt"), "{fehler}");
    }

    #[tokio::test]
    async fn eine_aenderung_ohne_antwort_laesst_den_ausgang_offen() {
        let adresse = stumm().await;
        let client = Client::mit_zeitlimit(&site(&adresse), Duration::from_millis(200)).unwrap();
        let fehler = client
            .anfrage(&Anfrage::post("issue", "{}"))
            .await
            .expect_err("ohne Antwort kein Erfolg");
        let text = fehler.to_string();
        assert!(text.contains("Ausgang unklar"), "{text}");
        assert!(text.contains("nicht einfach wiederholen"), "{text}");
    }

    #[tokio::test]
    async fn ein_lesender_aufruf_ohne_antwort_ist_ein_netzfehler() {
        let adresse = stumm().await;
        let client = Client::mit_zeitlimit(&site(&adresse), Duration::from_millis(200)).unwrap();
        let fehler = client
            .anfrage(&Anfrage::holen("myself"))
            .await
            .expect_err("ohne Antwort kein Erfolg");
        assert!(matches!(fehler, JiraFehler::Netz { .. }), "{fehler:?}");
        assert!(!fehler.to_string().contains("Ausgang unklar"));
    }

    #[test]
    fn ein_leerer_rumpf_ist_kein_jsonfehler() {
        assert_eq!(lesen("").unwrap(), serde_json::Value::Null);
        assert_eq!(lesen("   ").unwrap(), serde_json::Value::Null);
        assert_eq!(lesen(r#"{"a":1}"#).unwrap()["a"], 1);
        assert!(matches!(lesen("{kaputt"), Err(JiraFehler::Json { .. })));
    }
}
