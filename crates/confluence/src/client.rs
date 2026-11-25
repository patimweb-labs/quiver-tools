// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der Confluence-Client: ein HTTP-Gespräch je Site (Aufgabe 14).
//!
//! In Rust neu gebaut, abgelesen an der REST-Dokumentation (Confluence Cloud,
//! API v2) und an der Vorlage `pi-confluence-multiproject` (dort `fetch`):
//!
//! - **Anmeldung** mit HTTP Basic, E-Mail als Benutzer und API-Token als
//!   Passwort: `Authorization: Basic base64("email:token")`. Beides steht nur in
//!   dieser Kopfzeile (Aufgabe 12, Sicherheit 3).
//! - **Keine Weiterleitung.** Der Client folgt keinem `3xx`
//!   (`redirect::Policy::none`) und schickt die Kopfzeile darum nie an einen
//!   anderen Host (Sicherheit 2).
//! - **Folgeseiten** (`_links.next`) werden nur geladen, wenn sie zum
//!   eingetragenen Host gehören; sonst kommt ein Fehler **ohne** Anfrage.
//! - **429 und 503** werden nur bei **lesenden** Aufrufen wiederholt, höchstens
//!   zweimal, mit `Retry-After` (höchstens 30 s). Ein Anlegen oder Ändern wird
//!   nie wiederholt: ob es ankam, weiß nur der Server.
//! - **204 ohne Rumpf** nach einer Änderung ist Erfolg, kein Lesefehler.
//! - **Werte im Pfad** werden prozent-kodiert, auch `/`, `.`, `?` und `#`
//!   (Sicherheit 4). Leere Werte und Punkt-Segmente (`.`, `..`) sind verboten:
//!   `%2E%2E` normalisiert der URL-Leser trotzdem zu `..` (Korrekturen 2).
//! - **Adressen** prüft nur `url::Url` (Schema, Host, Port), nie eine eigene
//!   Zerlegung; ein Benutzerteil oder Fragment wird abgelehnt (Korrekturen 4).
//! - **Ein unlesbarer Rumpf** ist ein Fehler, nie „leer“; bei einer Änderung
//!   heißt es „ausgeführt“, ohne jede Antwort „Ausgang unklar“ (Korrekturen 3).

use std::time::Duration;

use reqwest::header::{HeaderMap, ACCEPT, CONTENT_TYPE, RETRY_AFTER, USER_AGENT};
use reqwest::Method;
use serde_json::Value;
use url::Url;

use crate::daten;
use crate::fehler::Fehler;
use crate::sites::Site;

/// Die Basis von Confluence Cloud, API v2.
pub const API: &str = "/wiki/api/v2";
/// Die Suche über CQL liegt in der älteren API (Aufgabe 14).
pub const SUCHE: &str = "/wiki/rest/api/search";
/// Labels schreibt die ältere API – v2 kennt dort nur `GET /pages/{id}/labels`.
pub const LABEL: &str = "/wiki/rest/api/content";
/// Wer ist angemeldet? (Confluence Cloud kennt dafür nur die ältere API.)
pub const BENUTZER: &str = "/wiki/rest/api/user/current";

/// Wie lange ein einzelner Aufruf höchstens dauern darf.
const ZEITLIMIT: Duration = Duration::from_secs(60);

/// So oft ein lesender Aufruf bei `429`/`503` wiederholt wird.
pub const WIEDERHOLUNGEN: u32 = 2;

/// So lange wird bei `429`/`503` höchstens gewartet.
const WARTEN_HOECHSTENS: Duration = Duration::from_secs(30);

/// So viele Folgeseiten werden höchstens geladen (Schutz vor einer Schleife).
const MAX_SEITEN: usize = 20;

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

/// Ein Aufruf an Confluence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anfrage {
    pub methode: Methode,
    /// Pfad ab der Site-Adresse – oder, bei `vollstaendig`, die ganze Adresse.
    pub pfad: String,
    /// Zusätzliche Abfrageparameter.
    pub abfrage: Vec<(String, String)>,
    /// Rumpf als Text.
    pub inhalt: Option<String>,
    /// `pfad` ist eine vollständige Folgeseite und bringt die Abfrage mit.
    pub vollstaendig: bool,
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
            vollstaendig: false,
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

    /// Mehrere Abfrageparameter.
    pub fn mit_abfragen<I, K, V>(mut self, abfragen: I) -> Anfrage
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        for (name, wert) in abfragen {
            self.abfrage.push((name.into(), wert.into()));
        }
        self
    }

    /// Eine vollständige Folgeseite (Adresse mit Abfrage).
    pub fn folgeseite(url: impl Into<String>) -> Anfrage {
        let mut anfrage = Anfrage::holen("");
        anfrage.pfad = url.into();
        anfrage.vollstaendig = true;
        anfrage
    }

    /// Ist das ein lesender Aufruf?
    pub fn ist_lesend(&self) -> bool {
        self.methode.ist_lesend()
    }
}

/// Was der Server geantwortet hat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Antwort {
    pub status: u16,
    pub text: String,
}

/// Eine Liste vom Server, schon auf `anzahl` gekürzt.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Auszug {
    /// Die Einträge aus `results`.
    pub eintraege: Vec<Value>,
    /// Gibt es auf dem Server noch mehr?
    pub mehr: bool,
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
    /// Ein Client für diese Site. Token und E-Mail bleiben im Speicher.
    pub fn neu(site: &Site) -> Result<Client, Fehler> {
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
                "quiver-confluence/",
                env!("CARGO_PKG_VERSION")
            )))
            .and_then(|bauer| {
                bauer
                    .timeout(ZEITLIMIT)
                    .build()
                    .map_err(|fehler| quiver_tools_kern::net::NetFehler::Client(fehler.to_string()))
            })
            .map_err(|fehler| Fehler::Netz {
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

    /// Die vollständige Adresse eines Pfades (ohne Abfrage).
    pub fn url(&self, pfad: &str) -> String {
        if pfad.starts_with("/") {
            format!("{}{}", self.site.url, pfad)
        } else {
            format!("{}/{}", self.site.url, pfad)
        }
    }

    /// Ein Aufruf: schicken, bei `429`/`503` (nur lesend) wiederholen, Fehler deuten.
    pub async fn anfrage(&self, anfrage: &Anfrage) -> Result<Antwort, Fehler> {
        let ziel = self.ziel(anfrage)?;
        let mut wiederholt = 0;
        loop {
            let mut bau = self
                .http
                .request(anfrage.methode.reqwest(), ziel.clone())
                .header(
                    USER_AGENT,
                    concat!("quiver-confluence/", env!("CARGO_PKG_VERSION")),
                )
                // Basic mit E-Mail und API-Token – die einzige Stelle, an der
                // beides das Programm verlässt.
                .basic_auth(&self.site.email, Some(&self.site.token))
                .header(ACCEPT, "application/json");
            if !anfrage.abfrage.is_empty() {
                bau = bau.query(&anfrage.abfrage);
            }
            if let Some(inhalt) = &anfrage.inhalt {
                bau = bau
                    .header(CONTENT_TYPE, "application/json")
                    .body(inhalt.clone());
            }
            // Ohne Antwort ist bei einer Änderung der Ausgang unklar; ein
            // lesender Aufruf darf als Netzfehler gemeldet werden.
            let antwort = bau.send().await.map_err(|fehler| {
                if anfrage.ist_lesend() {
                    Fehler::netz(&fehler)
                } else {
                    Fehler::unklar(&fehler)
                }
            })?;

            let status = antwort.status().as_u16();
            let wartezeit = wartezeit(antwort.headers());

            // 429 und 503: nur lesende Aufrufe werden wiederholt.
            if (status == 429 || status == 503) && anfrage.ist_lesend() {
                if wiederholt < WIEDERHOLUNGEN {
                    wiederholt += 1;
                    tokio::time::sleep(wartezeit.min(WARTEN_HOECHSTENS)).await;
                    continue;
                }
                return Err(Fehler::Http {
                    status,
                    text: format!(
                        "Nach {WIEDERHOLUNGEN} Wiederholungen aufgegeben – bitte kurz warten."
                    ),
                });
            }

            if status == 401 {
                // Kein Wiederholen: ein falsches Token wird davon nicht besser.
                return Err(Fehler::Anmeldung {
                    verbindung: self.site.name.clone(),
                });
            }
            if status == 403 {
                return Err(Fehler::Rechte {
                    verbindung: self.site.name.clone(),
                });
            }

            // Den Rumpf erst nach den Statusregeln lesen: ein Fehlschlag ist
            // ein Fehler, nie „leer“ (Korrekturen 3). Höchstens 8 MB.
            let rumpf = begrenzt(antwort).await;
            if !(200..300).contains(&status) {
                let text = rumpf.unwrap_or_else(|fehler| format!("Rumpf nicht lesbar: {fehler}"));
                return Err(Fehler::aus_antwort(status, &text));
            }
            return match rumpf {
                Ok(text) => Ok(Antwort { status, text }),
                Err(fehler) if anfrage.ist_lesend() => Err(Fehler::Unlesbar {
                    text: fehler.to_string(),
                }),
                Err(fehler) => Err(Fehler::Ausgefuehrt {
                    text: fehler.to_string(),
                }),
            };
        }
    }

    /// Ein Aufruf, der JSON zurückgibt. Ein leerer Rumpf (204) ist `Null` –
    /// Erfolg, kein Lesefehler (Aufgabe 12, Sicherheit 8).
    pub async fn json(&self, anfrage: &Anfrage) -> Result<Value, Fehler> {
        let antwort = self.anfrage(anfrage).await?;
        if antwort.text.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&antwort.text).map_err(|fehler| {
            let text = format!("{} ({})", fehler, daten::kurz(&antwort.text));
            // 2xx auf eine Änderung heißt: ausgeführt – auch wenn die Antwort
            // kein JSON ist. Sonst wiederholt ein Agent sie womöglich.
            if anfrage.ist_lesend() {
                Fehler::Json { text }
            } else {
                Fehler::Ausgefuehrt { text }
            }
        })
    }

    /// Eine Liste lesen und – wenn nötig – Folgeseiten laden.
    ///
    /// `anzahl` begrenzt die Einträge; `mehr` sagt, ob der Server noch etwas
    /// hätte. Eine Folgeseite mit fremdem Host endet mit einem Fehler **ohne**
    /// Anfrage (Aufgabe 12, Sicherheit 2).
    pub async fn liste(
        &self,
        pfad: &str,
        abfrage: Vec<(String, String)>,
        anzahl: usize,
    ) -> Result<Auszug, Fehler> {
        let anzahl = anzahl.max(1);
        let mut alle: Vec<Value> = Vec::new();
        let mut naechste: Option<String> = None;
        let mut erste = true;

        for _ in 0..MAX_SEITEN {
            let antwort = match naechste.take() {
                Some(ziel) => self.json(&Anfrage::folgeseite(ziel)).await?,
                None if erste => {
                    erste = false;
                    self.json(&Anfrage::holen(pfad).mit_abfragen(abfrage.clone()))
                        .await?
                }
                None => {
                    return Ok(Auszug {
                        eintraege: alle,
                        mehr: false,
                    })
                }
            };

            alle.extend(daten::liste(&antwort, "results").iter().cloned());
            let weiter = daten::text(&antwort, "_links.next");

            if alle.len() >= anzahl {
                alle.truncate(anzahl);
                return Ok(Auszug {
                    eintraege: alle,
                    mehr: weiter.is_some(),
                });
            }
            match weiter {
                Some(link) => naechste = Some(self.aufloesen(&link)?),
                None => {
                    return Ok(Auszug {
                        eintraege: alle,
                        mehr: false,
                    })
                }
            }
        }

        alle.truncate(anzahl);
        Ok(Auszug {
            eintraege: alle,
            mehr: true,
        })
    }

    // ------------------------------------------------------------- innen

    /// Die geprüfte Adresse eines Aufrufs.
    fn ziel(&self, anfrage: &Anfrage) -> Result<Url, Fehler> {
        let vollstaendig = anfrage.vollstaendig
            || anfrage.pfad.starts_with("http://")
            || anfrage.pfad.starts_with("https://");
        let roh = if vollstaendig {
            anfrage.pfad.clone()
        } else {
            self.url(&anfrage.pfad)
        };
        let geprueft = Url::parse(&roh).map_err(|fehler| Fehler::Adresse {
            text: format!("{} ist keine gültige Adresse ({fehler})", daten::kurz(&roh)),
        })?;
        self.pruefen_host(&geprueft)?;

        // Ein Punkt-Segment (`%2E%2E`) verschwindet beim Lesen der Adresse und
        // spricht damit einen anderen Endpunkt an (Korrekturen 2). Verglichen
        // wird die fertige `Url`, nicht nur die Kodierung.
        if !vollstaendig {
            let erwartet = pfad_teile(&anfrage.pfad);
            let tatsaechlich = geprueft
                .path_segments()
                .map(|segmente| segmente.filter(|teil| !teil.is_empty()).count())
                .unwrap_or(0);
            if erwartet != tatsaechlich {
                return Err(Fehler::Pfad {
                    wert: daten::kurz(&anfrage.pfad),
                });
            }
        }
        Ok(geprueft)
    }

    /// Nur die eigene Site: gleiches Schema, gleicher Host mit Port, kein
    /// Benutzerteil – geprüft über `url::Url`, nicht über eigene Zerlegung
    /// (Korrekturen 4).
    fn pruefen_host(&self, url: &Url) -> Result<(), Fehler> {
        if !url.username().is_empty() || url.password().is_some() {
            return Err(Fehler::Adresse {
                text: format!("{} enthält einen Benutzerteil", daten::kurz(url.as_str())),
            });
        }
        if url.fragment().is_some() {
            return Err(Fehler::Adresse {
                text: format!("{} enthält ein Fragment", daten::kurz(url.as_str())),
            });
        }
        let basis = Url::parse(&self.site.url).map_err(|fehler| Fehler::Adresse {
            text: format!("Die Adresse der Site ist nicht lesbar ({fehler})"),
        })?;
        let gleich = basis.scheme().eq_ignore_ascii_case(url.scheme())
            && basis.host_str() == url.host_str()
            && basis.port_or_known_default() == url.port_or_known_default();
        if !gleich {
            return Err(Fehler::FremderHost {
                url: daten::kurz(url.as_str()),
            });
        }
        Ok(())
    }

    /// Eine Folgeseite zur vollständigen Adresse machen und den Host prüfen.
    ///
    /// Die Spezifikation nennt `_links.next` eine *relative* URL; aufgelöst
    /// wird sie mit [`Url::join`] – nie mit eigener Zerlegung (Korrekturen 4).
    fn aufloesen(&self, link: &str) -> Result<String, Fehler> {
        let link = link.trim();
        let basis = Url::parse(&self.site.url).map_err(|fehler| Fehler::Adresse {
            text: format!("Die Adresse der Site ist nicht lesbar ({fehler})"),
        })?;
        let geprueft = basis.join(link).map_err(|fehler| Fehler::Antwort {
            text: format!(
                "Die Folgeseite „{}“ hat keine gültige Adresse ({fehler}).",
                daten::kurz(link)
            ),
        })?;
        self.pruefen_host(&geprueft)?;
        Ok(geprueft.to_string())
    }

    /// Gehört die Adresse zu dieser Site (gleiches Schema, gleicher Host mit Port)?
    #[cfg(test)]
    fn gehoert_zu(&self, url: &str) -> bool {
        Url::parse(url)
            .map(|url| self.pruefen_host(&url).is_ok())
            .unwrap_or(false)
    }
}

/// Eine Antwort lesen – höchstens 8 MB, sonst Abbruch als Fehler.
async fn begrenzt(antwort: reqwest::Response) -> Result<String, quiver_tools_kern::net::NetFehler> {
    quiver_tools_kern::net::text_begrenzt(antwort).await
}

/// Wie viele Segmente hat ein selbst gebauter Pfad?
fn pfad_teile(pfad: &str) -> usize {
    pfad.split('/').filter(|teil| !teil.is_empty()).count()
}

/// Ein Pfadsegment kodieren: nur `A-Z a-z 0-9 - _ ~` bleiben stehen.
///
/// Damit wird auch `/`, `.`, `?` und `#` kodiert – ein Wert des Agenten kann
/// keinen anderen Endpunkt ansprechen (Aufgabe 12, Sicherheit 4).
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

/// Ein Wert, der in einen Pfad geht: nicht leer, kein Punkt-Segment, kodiert.
///
/// `%2E%2E` würde beim Lesen der Adresse wieder zu `..` und einen anderen
/// Endpunkt ansprechen – so ein Wert ist ohne Anfrage verboten, ebenso ein
/// leerer (Korrekturen 2).
pub fn pfad_wert(wert: &str) -> Result<String, Fehler> {
    let wert = wert.trim();
    if wert.is_empty() || wert.chars().all(|zeichen| zeichen == '.') {
        return Err(Fehler::Pfad {
            wert: daten::kurz(wert),
        });
    }
    Ok(kodieren(wert))
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
pub fn kurz(text: &str) -> String {
    daten::kurz(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site() -> Site {
        Site {
            name: "firma".into(),
            url: "https://firma.atlassian.net".into(),
            email: "alex@example.test".into(),
            token: "token-4711".into(),
            nur: Vec::new(),
            schreiben: true,
            ca_zertifikat: None,
        }
    }

    #[test]
    fn die_adresse_wird_aus_der_site_gebaut() {
        let client = Client::neu(&site()).unwrap();
        assert_eq!(
            client.url("/wiki/api/v2/spaces"),
            "https://firma.atlassian.net/wiki/api/v2/spaces"
        );
        assert_eq!(
            client.url("wiki/rest/api/search"),
            "https://firma.atlassian.net/wiki/rest/api/search"
        );
    }

    #[test]
    fn segmente_werden_vollstaendig_kodiert() {
        assert_eq!(kodieren("123"), "123");
        assert_eq!(kodieren("Eng"), "Eng");
        assert_eq!(kodieren("a b"), "a%20b");
        // `/`, `.`, `?` und `#` dürfen keinen Pfad oder keine Abfrage öffnen.
        assert_eq!(kodieren("a/b"), "a%2Fb");
        assert_eq!(kodieren(".."), "%2E%2E");
        assert_eq!(kodieren("a?b#c"), "a%3Fb%23c");
        assert_eq!(kodieren("../spaces/x"), "%2E%2E%2Fspaces%2Fx");
        // Umlaute sind UTF-8-Bytes.
        assert_eq!(kodieren("Grüße"), "Gr%C3%BC%C3%9Fe");
    }

    #[test]
    fn nur_der_eigene_host_zaehlt() {
        let client = Client::neu(&site()).unwrap();
        assert!(client.gehoert_zu("https://firma.atlassian.net/wiki/api/v2/spaces"));
        assert!(client.gehoert_zu("https://FIRMA.atlassian.net/x"));
        assert!(client.gehoert_zu("https://firma.atlassian.net:443/x"));
        assert!(!client.gehoert_zu("https://andere.atlassian.net/x"));
        assert!(!client.gehoert_zu("http://firma.atlassian.net/x"));
        assert!(!client.gehoert_zu("https://firma.atlassian.net@evil.test/x"));
        assert!(!client.gehoert_zu("file:///etc/passwd"));
        assert!(!client.gehoert_zu("https://firma.atlassian.net:8443/x"));
    }

    #[test]
    fn eine_fremde_folgeseite_wird_ohne_anfrage_abgelehnt() {
        let client = Client::neu(&site()).unwrap();
        let fehler = client
            .aufloesen("https://evil.test/wiki/api/v2/spaces")
            .unwrap_err();
        assert!(matches!(fehler, Fehler::FremderHost { .. }));
        // Ein relatives Ziel wird an die eigene Site gehängt.
        assert_eq!(
            client.aufloesen("/wiki/api/v2/spaces?cursor=2").unwrap(),
            "https://firma.atlassian.net/wiki/api/v2/spaces?cursor=2"
        );
        // Auch eine relative Adresse ohne Schrägstrich bleibt auf der Site …
        assert_eq!(
            client.aufloesen("spaces?cursor=2").unwrap(),
            "https://firma.atlassian.net/spaces?cursor=2"
        );
        // … und eine protokollrelative zeigt auf einen fremden Host.
        assert!(matches!(
            client.aufloesen("//evil.test/x"),
            Err(Fehler::FremderHost { .. })
        ));
        assert!(matches!(
            client.aufloesen("https://user:pass@firma.atlassian.net/x"),
            Err(Fehler::Adresse { .. })
        ));
        assert!(matches!(
            client.aufloesen("https://firma.atlassian.net/x#frag"),
            Err(Fehler::Adresse { .. })
        ));
    }

    #[test]
    fn eine_vollstaendige_adresse_wird_vor_dem_senden_geprueft() {
        let client = Client::neu(&site()).unwrap();
        let fremd = Anfrage::folgeseite("https://evil.test/x");
        assert!(matches!(
            client.ziel(&fremd),
            Err(Fehler::FremderHost { .. })
        ));
        let eigen = Anfrage::folgeseite("https://firma.atlassian.net/x");
        assert_eq!(
            client.ziel(&eigen).unwrap().as_str(),
            "https://firma.atlassian.net/x"
        );
    }

    #[test]
    fn ein_punkt_segment_wuerde_die_adresse_verbiegen() {
        // Beweis über die fertige Adresse: `%2E%2E` normalisiert der URL-Leser
        // zu `..` und verkürzt den Pfad.
        let roh = format!("https://firma.atlassian.net{API}/pages/{}", kodieren(".."));
        let gelesen = Url::parse(&roh).unwrap();
        assert_eq!(gelesen.path(), "/wiki/api/v2/", "{roh}");

        // Darum ist so ein Wert schon vor dem Bauen verboten.
        for verboten in ["", "   ", ".", "..", "..."] {
            assert!(pfad_wert(verboten).is_err(), "{verboten:?}");
        }

        // Und die fertige Anfrage wird ohne Senden abgelehnt.
        let client = Client::neu(&site()).unwrap();
        let anfrage = Anfrage::holen(format!("{API}/pages/{}", kodieren("..")));
        let fehler = client.ziel(&anfrage).unwrap_err();
        assert!(matches!(fehler, Fehler::Pfad { .. }), "{fehler:?}");
        assert!(fehler.to_string().contains("nicht gesendet"));

        // Ein harmloser Wert bleibt ein Segment und kommt unverändert an.
        let wert = pfad_wert("../spaces/10").unwrap();
        let gelesen =
            Url::parse(&format!("https://firma.atlassian.net{API}/pages/{wert}")).unwrap();
        assert_eq!(gelesen.path(), "/wiki/api/v2/pages/%2E%2E%2Fspaces%2F10");
    }

    #[test]
    fn eine_adresse_mit_benutzerteil_ist_keine_site_adresse() {
        let client = Client::neu(&site()).unwrap();
        for fremd in [
            "https://firma.atlassian.net@evil.test/x",
            "https://user:pass@firma.atlassian.net/x",
            "http://evil.test\\@127.0.0.1/",
            "https://firma.atlassian.net:8443/x",
            "http://firma.atlassian.net/x",
        ] {
            assert!(!client.gehoert_zu(fremd), "{fremd}");
        }
    }

    #[tokio::test]
    async fn eine_aenderung_ohne_antwort_ist_unklar_und_wird_nicht_wiederholt() {
        let mut site = site();
        // Port 1: niemand hört zu – die Verbindung wird sofort abgelehnt.
        site.url = "http://127.0.0.1:1".into();
        let client = Client::neu(&site).unwrap();

        let fehler = client
            .anfrage(&Anfrage::put(format!("{API}/pages/1"), "{}"))
            .await
            .unwrap_err();
        assert!(matches!(fehler, Fehler::Unklar { .. }), "{fehler:?}");
        assert!(fehler.to_string().contains("Ausgang unklar"));
        assert!(fehler.to_string().contains("nicht einfach wiederholen"));

        // Ein lesender Aufruf bleibt ein Netzfehler.
        let fehler = client
            .anfrage(&Anfrage::holen(format!("{API}/pages/1")))
            .await
            .unwrap_err();
        assert!(matches!(fehler, Fehler::Netz { .. }), "{fehler:?}");
    }

    #[test]
    fn debug_zeigt_weder_token_noch_email() {
        let client = Client::neu(&site()).unwrap();
        let text = format!("{client:?}");
        assert!(!text.contains("token-4711"), "{text}");
        assert!(!text.contains("alex@example.test"), "{text}");
        assert!(text.contains("firma"), "{text}");
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
    fn die_folgeseite_traegt_ihre_abfrage_schon() {
        let anfrage = Anfrage::folgeseite("https://firma.atlassian.net/x?cursor=2");
        assert!(anfrage.vollstaendig);
        assert!(anfrage.abfrage.is_empty());
    }

    #[test]
    fn retry_after_wird_gelesen() {
        let mut kopf = HeaderMap::new();
        assert_eq!(wartezeit(&kopf), Duration::from_secs(1));
        kopf.insert(RETRY_AFTER, "7".parse().unwrap());
        assert_eq!(wartezeit(&kopf), Duration::from_secs(7));
        kopf.insert(RETRY_AFTER, "kaputt".parse().unwrap());
        assert_eq!(wartezeit(&kopf), Duration::from_secs(1));
    }
}
