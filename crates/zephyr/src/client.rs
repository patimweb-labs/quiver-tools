// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der Zephyr-Client: ein HTTP-Gespräch je Zugang (Aufgabe 15).
//!
//! Abgelesen an der OpenAPI-Datei der REST-API v2 (Zephyr Scale Cloud) und an
//! der Vorlage `pi-zephyr-testmanager` (dort über `fetch`):
//!
//! - **Anmeldung** mit `Authorization: Bearer <api_token>`. Das Token steht nur
//!   in dieser Kopfzeile.
//! - **Keine Weiterleitungen** (`redirect::Policy::none`): das Token geht nur an
//!   den eingetragenen Host. Eine Antwort `3xx` und eine Folgeseite mit fremdem
//!   Ursprung werden abgelehnt, **bevor** eine weitere Anfrage entsteht.
//! - **401** heißt „Token gilt nicht“ und wird **nicht** wiederholt; **403**
//!   heißt „keine Rechte“.
//! - **429 und 503** werden nur bei **lesenden** Aufrufen wiederholt, höchstens
//!   zweimal, mit `Retry-After`. Verlangt der Server mehr als 30 s, wird sofort
//!   aufgegeben statt gewartet. Ein Anlegen oder Ändern wird nie wiederholt: ob
//!   es ankam, weiß nur der Server.
//! - Ein **leerer Rumpf** (`204`) nach einer Änderung ist Erfolg.
//! - Ein **unlesbarer Rumpf** ist dagegen ein Fehler und nie „leer“: bei einem
//!   lesenden Aufruf scheitert er, bei einer Änderung heißt es „ausgeführt,
//!   Antwort nicht lesbar“.
//! - Kommt auf eine Änderung **gar keine** Antwort (Zeitüberschreitung,
//!   Abbruch), heißt es „Ausgang unklar – Stand prüfen, nicht einfach
//!   wiederholen“.
//! - **Werte in Pfaden** werden nicht nur prozent-kodiert, sondern vorher
//!   geprüft: `.`, `..` und leere Werte werden **ohne Anfrage** abgelehnt, weil
//!   `url`/`reqwest` `%2E%2E` trotzdem zu `..` normalisieren.

use std::time::Duration;

use reqwest::header::{HeaderMap, ACCEPT, AUTHORIZATION, CONTENT_TYPE, RETRY_AFTER, USER_AGENT};
use reqwest::Method;
use serde_json::Value;
use url::Url;

use crate::daten;
use crate::fehler::ZephyrFehler;
use crate::zugaenge::{herkunft, Zugang};

/// Wie lange ein einzelner Aufruf höchstens dauern darf.
const ZEITLIMIT: Duration = Duration::from_secs(60);

/// So oft ein lesender Aufruf bei `429`/`503` wiederholt wird.
const WIEDERHOLUNGEN: u32 = 2;

/// So lange wird bei `429`/`503` höchstens gewartet.
const WARTEN_HOECHSTENS: Duration = Duration::from_secs(30);

/// So viele Seiten liest [`Client::alle`] höchstens.
pub const SEITEN_HOECHSTENS: u32 = 20;

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

/// Ein Aufruf an Zephyr.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anfrage {
    pub methode: Methode,
    /// Pfad ab der Basisadresse, z. B. `testcases/QA-T1`. Werte darin sind
    /// bereits kodiert ([`kodieren`]).
    pub pfad: String,
    /// Zusätzliche Abfrageparameter.
    pub abfrage: Vec<(String, String)>,
    /// Rumpf als Text.
    pub inhalt: Option<String>,
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

    /// Ein Aufruf mit Rumpf und eigener Methode (`PUT`).
    pub fn neu(methode: Methode, pfad: impl Into<String>) -> Anfrage {
        Anfrage {
            methode,
            pfad: pfad.into(),
            abfrage: Vec::new(),
            inhalt: None,
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

/// Der Client eines Zugangs.
pub struct Client {
    zugang: Zugang,
    http: reqwest::Client,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("zugang", &self.zugang.name)
            .finish()
    }
}

impl Client {
    /// Ein Client für diesen Zugang. Das Token steht nur im Speicher.
    pub fn neu(zugang: &Zugang) -> Result<Client, ZephyrFehler> {
        let http = reqwest::Client::builder()
            .timeout(ZEITLIMIT)
            .user_agent(concat!("quiver-zephyr/", env!("CARGO_PKG_VERSION")))
            // Das Token verlässt den eingetragenen Host nie.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|fehler| ZephyrFehler::Netz {
                text: fehler.to_string(),
            })?;
        Ok(Client {
            zugang: zugang.clone(),
            http,
        })
    }

    /// Der Zugang, mit dem dieser Client spricht.
    pub fn zugang(&self) -> &Zugang {
        &self.zugang
    }

    /// Die Adresse eines Aufrufs: Basisadresse plus Pfad plus Abfrage.
    pub fn ziel(&self, anfrage: &Anfrage) -> String {
        let basis = self.zugang.basis_url.trim_end_matches('/');
        let pfad = anfrage.pfad.trim_start_matches('/');
        let mut url = if pfad.is_empty() {
            basis.to_string()
        } else {
            format!("{basis}/{pfad}")
        };
        if !anfrage.abfrage.is_empty() {
            let teil: Vec<String> = anfrage
                .abfrage
                .iter()
                .map(|(name, wert)| format!("{}={}", kodieren(name), kodieren(wert)))
                .collect();
            url.push('?');
            url.push_str(&teil.join("&"));
        }
        url
    }

    /// Die Adresse eines Aufrufs, geprüft: der **fertig zerlegte** Pfad muss
    /// genau der gebaute sein.
    ///
    /// Geprüft wird die fertige [`Url`], nicht nur der einzelne Wert: `url` und
    /// `reqwest` normalisieren `%2E%2E` und `%2E` zu `..` und `.` und würden
    /// damit einen anderen Pfad aufrufen, als der Agent angegeben hat. Bleibt
    /// vom Pfad weniger übrig als gebaut, wird die Anfrage **gar nicht** erst
    /// gesendet.
    pub fn ziel_geprueft(&self, anfrage: &Anfrage) -> Result<String, ZephyrFehler> {
        let ziel = self.ziel(anfrage);
        let nicht_gesendet = |fehler: String| ZephyrFehler::Parameter {
            text: format!("{fehler} Es wurde nichts gesendet."),
        };
        let url = Url::parse(&ziel).map_err(|fehler| {
            nicht_gesendet(format!("Die Adresse „{ziel}“ ist nicht gültig ({fehler})."))
        })?;
        let basis = Url::parse(&self.zugang.basis_url).map_err(|fehler| {
            nicht_gesendet(format!(
                "Die Basisadresse „{}“ ist nicht gültig ({fehler}).",
                self.zugang.basis_url
            ))
        })?;

        let mut erwartet = basis.path().trim_end_matches('/').to_string();
        let pfad = anfrage.pfad.trim_start_matches('/');
        if !pfad.is_empty() {
            erwartet.push('/');
            erwartet.push_str(pfad);
        }
        if erwartet.is_empty() {
            erwartet.push('/');
        }
        if url.path() != erwartet {
            return Err(nicht_gesendet(format!(
                "Der Pfad „{}“ wird vom URL-Parser zu „{}“ umgeschrieben (`.`- oder \
                 `..`-Segment) – ein solcher Wert würde einen anderen Pfad aufrufen.",
                anfrage.pfad,
                url.path()
            )));
        }
        Ok(ziel)
    }

    /// Ein Aufruf: schicken, bei `429`/`503` (nur lesend) wiederholen, Fehler deuten.
    pub async fn anfrage(&self, anfrage: &Anfrage) -> Result<Antwort, ZephyrFehler> {
        let ziel = self.ziel_geprueft(anfrage)?;
        let mut wiederholt = 0;
        loop {
            let mut request = self
                .http
                .request(anfrage.methode.reqwest(), &ziel)
                .header(
                    USER_AGENT,
                    concat!("quiver-zephyr/", env!("CARGO_PKG_VERSION")),
                )
                // Das Token steht nur in dieser Kopfzeile.
                .header(AUTHORIZATION, format!("Bearer {}", self.zugang.token))
                .header(ACCEPT, "application/json");
            if let Some(inhalt) = &anfrage.inhalt {
                request = request
                    .header(CONTENT_TYPE, "application/json")
                    .body(inhalt.clone());
            }
            let antwort = request.send().await.map_err(|fehler| {
                ZephyrFehler::beim_senden(fehler, anfrage.ist_lesend(), &self.zugang.name)
            })?;

            let status = antwort.status().as_u16();
            let wartezeit = wartezeit(antwort.headers());
            // Ein unlesbarer Rumpf ist ein Fehler, keine leere Antwort.
            let gelesen = antwort.text().await;

            if status == 401 {
                // Kein Wiederholen: ein falsches Token wird davon nicht besser.
                return Err(ZephyrFehler::Anmeldung {
                    verbindung: self.zugang.name.clone(),
                });
            }

            if status == 403 {
                return Err(ZephyrFehler::Rechte {
                    verbindung: self.zugang.name.clone(),
                    text: gelesen
                        .map(|rumpf| crate::fehler::meldung(&rumpf))
                        .unwrap_or_default(),
                });
            }

            if status == 429 || status == 503 {
                if anfrage.ist_lesend() && wiederholt < WIEDERHOLUNGEN {
                    if wartezeit > WARTEN_HOECHSTENS {
                        return Err(ZephyrFehler::Http {
                            status,
                            text: format!(
                                "Zephyr verlangt {} s Wartezeit (Retry-After) – mehr als die \
                                 zulässigen {} s, darum nicht wiederholt. Bitte später erneut.",
                                wartezeit.as_secs(),
                                WARTEN_HOECHSTENS.as_secs()
                            ),
                        });
                    }
                    wiederholt += 1;
                    tokio::time::sleep(wartezeit.min(WARTEN_HOECHSTENS)).await;
                    continue;
                }
                let text = if anfrage.ist_lesend() {
                    format!(
                        "Zephyr antwortete mit HTTP {status} – nach {WIEDERHOLUNGEN} \
                         Wiederholungen aufgegeben, bitte kurz warten."
                    )
                } else {
                    format!(
                        "Zephyr antwortete mit HTTP {status} – verändernde Aufrufe werden nicht \
                         wiederholt, bitte den Stand prüfen."
                    )
                };
                return Err(ZephyrFehler::Http { status, text });
            }

            if (300..400).contains(&status) {
                return Err(ZephyrFehler::Weiterleitung { status });
            }

            let rumpf = match gelesen {
                Ok(rumpf) => rumpf,
                Err(fehler) => {
                    // 2xx: der Aufruf kam an, nur die Antwort ist kaputt. Bei
                    // einer Änderung ist das keine Wiederholung, sondern eine
                    // Prüfung wert.
                    return Err(if anfrage.ist_lesend() {
                        ZephyrFehler::Unlesbar {
                            status,
                            text: fehler.to_string(),
                        }
                    } else {
                        ZephyrFehler::AusgefuehrtNichtLesbar { status }
                    });
                }
            };

            if (200..300).contains(&status) {
                return Ok(Antwort {
                    status,
                    text: rumpf,
                });
            }
            return Err(ZephyrFehler::aus_antwort(status, &rumpf));
        }
    }

    /// Ein Aufruf, der JSON zurückgibt.
    ///
    /// Ein leerer Rumpf (`204`) ist Erfolg und ergibt `null`. Ist der Rumpf
    /// **nicht leer**, aber kein JSON, gilt: lesend ein Fehler, verändernd
    /// „ausgeführt, Antwort nicht lesbar“.
    pub async fn json(&self, anfrage: &Anfrage) -> Result<Value, ZephyrFehler> {
        let antwort = self.anfrage(anfrage).await?;
        match lesen(&antwort.text) {
            Ok(wert) => Ok(wert),
            Err(_) if !anfrage.ist_lesend() => Err(ZephyrFehler::AusgefuehrtNichtLesbar {
                status: antwort.status,
            }),
            Err(fehler) => Err(fehler),
        }
    }

    /// Alle Einträge einer Liste, Seite für Seite (`startAt`), höchstens
    /// [`SEITEN_HOECHSTENS`].
    ///
    /// Jede Seite wird auf eine **fremde** Folgeseite geprüft, bevor die
    /// nächste Anfrage entsteht. Zurück kommen die Einträge, die Gesamtzahl aus
    /// `total` und ob alle Seiten gelesen wurden.
    pub async fn alle(
        &self,
        pfad: &str,
        abfrage: Vec<(String, String)>,
    ) -> Result<(Vec<Value>, Option<i64>, bool), ZephyrFehler> {
        let mut eintraege = Vec::new();
        let mut gesamt = None;
        let mut start = 0u32;
        let mut vollstaendig = false;
        for _ in 0..SEITEN_HOECHSTENS {
            let mut anfrage = Anfrage::holen(pfad).mit_abfrage("maxResults", "100");
            for (name, wert) in &abfrage {
                anfrage = anfrage.mit_abfrage(name.clone(), wert.clone());
            }
            let baum = self
                .json(&anfrage.mit_abfrage("startAt", start.to_string()))
                .await?;
            self.folgeseite_pruefen(&baum)?;
            if let Some(gesamt_neu) = daten::gesamt(&baum) {
                gesamt = Some(gesamt_neu);
            }
            let werte = daten::liste(&baum, "values");
            if werte.is_empty() {
                vollstaendig = true;
                break;
            }
            eintraege.extend(werte.iter().cloned());
            if !daten::mehr(&baum, start, werte.len()) {
                vollstaendig = true;
                break;
            }
            start += werte.len() as u32;
        }
        Ok((eintraege, gesamt, vollstaendig))
    }

    /// Prüft, ob eine Antwort auf eine **fremde** Folgeseite zeigt.
    ///
    /// Zephyr nennt die nächste Seite als vollständige Adresse. Sie darf nur auf
    /// denselben Ursprung wie die Basisadresse zeigen – gleiches Schema, gleicher
    /// Host, gleicher Port; sonst wird sie abgelehnt, ohne dass eine Anfrage
    /// entsteht.
    pub fn folgeseite_pruefen(&self, baum: &Value) -> Result<(), ZephyrFehler> {
        let Some(next) = daten::folgeseite(baum) else {
            return Ok(());
        };
        let erwartet = herkunft(&self.zugang.basis_url);
        let gefunden = herkunft(&next);
        match (&gefunden, &erwartet) {
            (Some(gefunden), Some(erwartet)) if gefunden == erwartet => Ok(()),
            _ => Err(ZephyrFehler::FremdeFolgeseite {
                host: gefunden
                    .map(|gefunden| gefunden.to_string())
                    .unwrap_or_else(|| "(ohne Host)".to_string()),
            }),
        }
    }
}

/// Text als JSON lesen; ein leerer Rumpf ist `null`.
pub fn lesen(text: &str) -> Result<Value, ZephyrFehler> {
    if text.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(text).map_err(|fehler| ZephyrFehler::Json {
        text: format!("{} ({})", fehler, kurz(text)),
    })
}

/// Ein Pfadsegment kodieren (`QA-T1` bleibt, `a/b` → `a%2Fb`, `.` → `%2E`).
///
/// Alles außer Buchstaben, Ziffern und `-`, `_`, `~` wird kodiert – auch `.`,
/// `/`, `?` und `#`, damit ein Wert aus einem Agenten nie einen anderen Pfad
/// oder eine Abfrage erzeugen kann.
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

/// Ein Wert für ein Pfadsegment: geprüft und kodiert.
///
/// Abgelehnt werden leere Werte und Werte mit einem `.`- oder `..`-Segment –
/// **ohne** Anfrage. `%2E%2E` wird von `url`/`reqwest` trotzdem zu `..`
/// normalisiert, ein kodierter Wert allein genügt also nicht.
pub fn pfadwert(roh: &str, feld: &str) -> Result<String, ZephyrFehler> {
    let wert = roh.trim();
    if wert.is_empty() {
        return Err(ZephyrFehler::Parameter {
            text: format!("`{feld}` darf nicht leer sein – es wurde nichts gesendet."),
        });
    }
    if wert.split('/').any(|teil| teil == "." || teil == "..") {
        return Err(ZephyrFehler::Parameter {
            text: format!(
                "`{feld}` darf kein `.`- oder `..`-Segment enthalten („{wert}“) – solche Werte \
                 würden einen anderen Pfad aufrufen. Es wurde nichts gesendet."
            ),
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
    use serde_json::json;

    fn zugang() -> Zugang {
        Zugang {
            name: "kunde".into(),
            basis_url: "https://api.example.test/v2".into(),
            token: "token-4711".into(),
            projekt: Some("QA".into()),
            nur: vec![],
            schreiben: true,
        }
    }

    #[test]
    fn die_adresse_wird_aus_der_basis_gebaut() {
        let client = Client::neu(&zugang()).unwrap();
        assert_eq!(
            client.ziel(&Anfrage::holen("testcases")),
            "https://api.example.test/v2/testcases"
        );
        assert_eq!(
            client.ziel(&Anfrage::holen("/testcases/QA-T1")),
            "https://api.example.test/v2/testcases/QA-T1"
        );
        assert_eq!(
            client.ziel(
                &Anfrage::holen("testcases")
                    .mit_abfrage("projectKey", "QA")
                    .mit_abfrage("maxResults", "25")
            ),
            "https://api.example.test/v2/testcases?projectKey=QA&maxResults=25"
        );
    }

    #[test]
    fn werte_im_pfad_werden_kodiert() {
        assert_eq!(kodieren("QA-T1"), "QA-T1");
        assert_eq!(kodieren("a/b"), "a%2Fb");
        assert_eq!(kodieren(".."), "%2E%2E");
        assert_eq!(kodieren("a?b#c"), "a%3Fb%23c");
        assert_eq!(kodieren("ä"), "%C3%A4");
    }

    #[test]
    fn punkt_und_punktpunkt_und_leere_werte_werden_ohne_anfrage_abgelehnt() {
        for falsch in ["", " ", ".", "..", " . ", "a/../b", "../QA-T1"] {
            let fehler = pfadwert(falsch, "schluessel").unwrap_err();
            assert!(
                matches!(fehler, ZephyrFehler::Parameter { .. }),
                "{falsch}: {fehler}"
            );
            assert!(fehler.to_string().contains("nichts gesendet"), "{fehler}");
        }
        // Ein Punkt *innerhalb* eines Wertes ist harmlos – er wird kodiert.
        assert_eq!(pfadwert("QA-T1.2", "schluessel").unwrap(), "QA-T1%2E2");
        assert_eq!(pfadwert("a..b", "schluessel").unwrap(), "a%2E%2Eb");
    }

    #[test]
    fn die_fertige_adresse_hat_kein_punktsegment() {
        let client = Client::neu(&zugang()).unwrap();
        // Selbst ein kodierter Wert bleibt im Pfad stehen …
        let geprueft =
            client.ziel_geprueft(&Anfrage::holen(format!("testcases/{}", kodieren(".."))));
        assert!(
            matches!(geprueft, Err(ZephyrFehler::Parameter { .. })),
            "… und wird abgelehnt: {geprueft:?}"
        );

        // … und ein sauberer Wert ergibt genau den erwarteten Pfad.
        let ziel = client
            .ziel_geprueft(&Anfrage::holen(format!(
                "testcases/{}",
                kodieren("../A?b#c")
            )))
            .unwrap();
        let url = Url::parse(&ziel).unwrap();
        let segmente: Vec<&str> = url.path_segments().unwrap().collect();
        assert_eq!(segmente, vec!["v2", "testcases", "%2E%2E%2FA%3Fb%23c"]);
        assert_eq!(url.query(), None);
        assert_eq!(url.fragment(), None);
    }

    #[test]
    fn debug_zeigt_kein_token() {
        let client = Client::neu(&zugang()).unwrap();
        let text = format!("{client:?}");
        assert!(!text.contains("token-4711"), "{text}");
        assert!(text.contains("kunde"), "{text}");
    }

    #[test]
    fn ein_leerer_rumpf_ist_erfolg() {
        assert_eq!(lesen("").unwrap(), Value::Null);
        assert_eq!(lesen("   ").unwrap(), Value::Null);
        assert_eq!(lesen(r#"{"key":"QA-T1"}"#).unwrap()["key"], json!("QA-T1"));
        assert!(matches!(lesen("{kaputt"), Err(ZephyrFehler::Json { .. })));
    }

    #[test]
    fn eine_fremde_folgeseite_wird_abgelehnt() {
        let client = Client::neu(&zugang()).unwrap();
        assert!(client
            .folgeseite_pruefen(
                &json!({ "next": "https://api.example.test/v2/testcases?startAt=25" })
            )
            .is_ok());
        assert!(client.folgeseite_pruefen(&json!({ "values": [] })).is_ok());

        let fehler = client
            .folgeseite_pruefen(&json!({ "next": "https://fremder.example.test/v2/testcases" }))
            .unwrap_err();
        assert!(
            matches!(&fehler, ZephyrFehler::FremdeFolgeseite { host } if host.contains("fremder.example.test")),
            "{fehler}"
        );
        assert!(fehler.to_string().contains("folgt ihr nicht"), "{fehler}");

        // Gleicher Host, anderer Port oder anderes Schema: fremd.
        for fremd in [
            "https://api.example.test:8443/v2/testcases?startAt=25",
            "http://api.example.test/v2/testcases?startAt=25",
        ] {
            assert!(
                client
                    .folgeseite_pruefen(&json!({ "next": fremd }))
                    .is_err(),
                "{fremd} sollte abgelehnt werden"
            );
        }
    }

    #[test]
    fn nur_lesende_aufrufe_gelten_als_wiederholbar() {
        assert!(Methode::Get.ist_lesend());
        assert!(!Methode::Post.ist_lesend());
        assert!(!Methode::Put.ist_lesend());
        assert!(Anfrage::holen("x").ist_lesend());
        assert!(!Anfrage::post("x", "{}").ist_lesend());
        assert!(!Anfrage::neu(Methode::Put, "x").ist_lesend());
    }
}
