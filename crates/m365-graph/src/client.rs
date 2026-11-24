// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der Graph-Client (SPEC.md §4): Token über [`m365_anmeldung::Anmeldung`],
//! 401 → Token verwerfen und **einmal** wiederholen, 429/503 mit `Retry-After`
//! → höchstens **zwei** Wiederholungen, und nur bei **lesenden** Aufrufen
//! (Aufgabe 05: ein Senden wird nie automatisch wiederholt, sonst entstehen
//! doppelte Nachrichten).
//!
//! Tokens stehen nie in einem Fehlertext und nie in einem Log.

use std::time::Duration;

use m365_anmeldung::{Anmeldung, Token};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::fehler::GraphFehler;
use crate::modelle::Sammlung;

/// Länger als so wartet ein erneuter Versuch nach `Retry-After` nie.
pub const HOECHSTENS_WARTEN: Duration = Duration::from_secs(10);
/// So oft versucht es ein **lesender** Aufruf nach 429/503 höchstens erneut.
const HOECHSTENS_WIEDERHOLUNGEN: u32 = 2;
/// Länger wird ein Fehlertext nicht (Antworten bleiben knapp, SPEC.md §4).
const HOECHSTENS_TEXT: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Methode {
    Get,
    Post,
}

/// GET/POST gegen Graph, mit den Rechten des angemeldeten Users.
pub struct GraphClient<A: Anmeldung> {
    /// z. B. `https://graph.microsoft.com/v1.0`, ohne Schrägstrich am Ende.
    basis: String,
    anmeldung: A,
    http: reqwest::Client,
}

impl<A: Anmeldung> GraphClient<A> {
    /// `basis_url` ist z. B. `https://graph.microsoft.com/v1.0` oder die Attrappe im Test.
    pub fn neu(basis_url: impl Into<String>, anmeldung: A) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            // Graph leitet nicht um; kein Folgen verhindert Missverständnisse.
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(60))
            .build()?;
        Ok(Self {
            basis: basis_url.into().trim_end_matches('/').to_string(),
            anmeldung,
            http,
        })
    }

    /// Die Anmeldung, die dieser Client benutzt (für `abmelden`).
    pub fn anmeldung(&self) -> &A {
        &self.anmeldung
    }

    /// `GET <pfad>?...` und die Antwort als Modell.
    pub async fn get<T: DeserializeOwned>(
        &self,
        pfad: &str,
        query: &[(&str, String)],
    ) -> Result<T, GraphFehler> {
        self.get_mit_kopf(pfad, query, &[]).await
    }

    /// Wie [`GraphClient::get`], mit zusätzlichen Kopfzeilen (z. B. `Prefer`).
    pub async fn get_mit_kopf<T: DeserializeOwned>(
        &self,
        pfad: &str,
        query: &[(&str, String)],
        kopfzeilen: &[(&str, &str)],
    ) -> Result<T, GraphFehler> {
        let wert = self
            .anfrage(Methode::Get, pfad, query, None, kopfzeilen)
            .await?;
        Self::lesen(wert)
    }

    /// Eine Liste lesen und über `@odata.nextLink` so lange blättern, bis
    /// `hoechstens` Einträge da sind oder Graph keine Seite mehr anbietet.
    ///
    /// Mehr Seiten als für `hoechstens` nötig holt das nie.
    pub async fn get_seiten<T: DeserializeOwned>(
        &self,
        pfad: &str,
        query: &[(&str, String)],
        hoechstens: usize,
    ) -> Result<Vec<T>, GraphFehler> {
        let erste: Sammlung<T> = self.get(pfad, query).await?;
        let Sammlung {
            value: mut werte,
            mut next_link,
        } = erste;

        while werte.len() < hoechstens {
            let Some(link) = next_link.take() else {
                break;
            };
            let seite: Sammlung<T> = self.get_link(&link).await?;
            werte.extend(seite.value);
            next_link = seite.next_link;
        }

        werte.truncate(hoechstens);
        Ok(werte)
    }

    /// Eine volle Adresse lesen (`@odata.nextLink` ist absolut). Relative
    /// Adressen gelten gegen die Basis.
    pub async fn get_link<T: DeserializeOwned>(&self, link: &str) -> Result<T, GraphFehler> {
        let ziel = self.adresse(link)?;
        let wert = self
            .anfrage_url(Methode::Get, ziel, None, &[], true)
            .await?;
        Self::lesen(wert)
    }

    /// Absolute Adresse unverändert, relative gegen die Basis.
    fn adresse(&self, link: &str) -> Result<reqwest::Url, GraphFehler> {
        let link = link.trim();
        let ziel = match reqwest::Url::parse(link) {
            Ok(ziel) => ziel,
            Err(_) => reqwest::Url::parse(&format!("{}{}", self.basis, link))
                .map_err(|fehler| GraphFehler::Antwort(format!("Adresse unbrauchbar: {fehler}")))?,
        };
        if !matches!(ziel.scheme(), "http" | "https") {
            return Err(GraphFehler::Antwort(format!(
                "Adresse mit unerwartetem Schema: {}",
                ziel.scheme()
            )));
        }
        Ok(ziel)
    }

    /// `POST <pfad>` mit JSON-Körper.
    pub async fn post<T: DeserializeOwned>(
        &self,
        pfad: &str,
        body: &Value,
    ) -> Result<T, GraphFehler> {
        self.post_mit_kopf(pfad, body, &[]).await
    }

    /// Wie [`GraphClient::post`], mit zusätzlichen Kopfzeilen.
    pub async fn post_mit_kopf<T: DeserializeOwned>(
        &self,
        pfad: &str,
        body: &Value,
        kopfzeilen: &[(&str, &str)],
    ) -> Result<T, GraphFehler> {
        let wert = self
            .anfrage(Methode::Post, pfad, &[], Some(body), kopfzeilen)
            .await?;
        Self::lesen(wert)
    }

    /// Gültiges Token oder der passende Fehler (die Anmeldung läuft im Browser).
    async fn token(&self) -> Result<String, GraphFehler> {
        match self.anmeldung.access_token().await {
            Ok(Token::Bereit(token)) => Ok(token),
            Ok(Token::WartetAufBrowser) => Err(GraphFehler::AnmeldungLaeuft),
            Err(fehler) => Err(GraphFehler::Anmeldung(fehler)),
        }
    }

    async fn anfrage(
        &self,
        methode: Methode,
        pfad: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
        kopfzeilen: &[(&str, &str)],
    ) -> Result<Value, GraphFehler> {
        let mut ziel = reqwest::Url::parse(&format!("{}{}", self.basis, pfad))
            .map_err(|fehler| GraphFehler::Antwort(format!("Adresse unbrauchbar: {fehler}")))?;
        if !query.is_empty() {
            ziel.set_query(Some(&abfrage_text(query)));
        }
        // 429/503 nur bei lesenden Aufrufen wiederholen: ein zweiter POST
        // könnte eine doppelte Nachricht erzeugen (Aufgabe 05).
        let wiederholbar = methode == Methode::Get;
        self.anfrage_url(methode, ziel, body, kopfzeilen, wiederholbar)
            .await
    }

    async fn anfrage_url(
        &self,
        methode: Methode,
        ziel: reqwest::Url,
        body: Option<&Value>,
        kopfzeilen: &[(&str, &str)],
        wiederholbar: bool,
    ) -> Result<Value, GraphFehler> {
        let mut erneuert = false;
        let mut wiederholungen = 0u32;
        loop {
            let token = self.token().await?;
            let mut anfrage = match methode {
                Methode::Get => self.http.get(ziel.clone()),
                Methode::Post => self.http.post(ziel.clone()),
            }
            .bearer_auth(&token);
            for (name, wert) in kopfzeilen {
                anfrage = anfrage.header(*name, *wert);
            }
            if let Some(body) = body {
                anfrage = anfrage.json(body);
            }

            let antwort = anfrage
                .send()
                .await
                .map_err(|fehler| GraphFehler::Netz(format!("{fehler}")))?;
            let status = antwort.status();

            // 401: Token ist nichts mehr wert – verwerfen und genau einmal wiederholen.
            if status == reqwest::StatusCode::UNAUTHORIZED && !erneuert {
                erneuert = true;
                tracing::info!("Graph antwortet 401 – Token verworfen, ein neuer Versuch");
                self.anmeldung.verwerfen();
                continue;
            }

            // 429/503 mit Retry-After: lesend höchstens zweimal erneut.
            if wiederholbar
                && wiederholungen < HOECHSTENS_WIEDERHOLUNGEN
                && (status == reqwest::StatusCode::TOO_MANY_REQUESTS
                    || status == reqwest::StatusCode::SERVICE_UNAVAILABLE)
            {
                if let Some(warten) = wartezeit(antwort.headers()) {
                    wiederholungen += 1;
                    tracing::info!(
                        "Graph antwortet {} – neuer Versuch in {} s ({} von {})",
                        status.as_u16(),
                        warten.as_secs(),
                        wiederholungen,
                        HOECHSTENS_WIEDERHOLUNGEN
                    );
                    tokio::time::sleep(warten).await;
                    continue;
                }
            }

            return Self::auswerten(status.as_u16(), antwort).await;
        }
    }

    async fn auswerten(status: u16, antwort: reqwest::Response) -> Result<Value, GraphFehler> {
        let text = antwort
            .text()
            .await
            .map_err(|fehler| GraphFehler::Netz(format!("{fehler}")))?;

        if (200..300).contains(&status) {
            if text.trim().is_empty() {
                // z. B. `sendMail` antwortet mit 202 ohne Körper.
                return Ok(Value::Null);
            }
            return serde_json::from_str(&text)
                .map_err(|fehler| GraphFehler::Antwort(format!("kein JSON: {fehler}")));
        }

        let (code, meldung) = graph_fehler_felder(&text);
        Err(GraphFehler::Graph {
            status,
            code,
            text: kuerzen(&meldung),
        })
    }

    fn lesen<T: DeserializeOwned>(wert: Value) -> Result<T, GraphFehler> {
        serde_json::from_value(wert)
            .map_err(|fehler| GraphFehler::Antwort(format!("unerwartete Felder: {fehler}")))
    }
}

/// Die Query als Text: Werte prozent-kodiert, also `%20` statt `+` für Leerzeichen
/// (eindeutig für Graph, nicht von der Formular-Kodierung abhängig).
fn abfrage_text(query: &[(&str, String)]) -> String {
    let mut text = String::new();
    for (name, wert) in query {
        if !text.is_empty() {
            text.push('&');
        }
        text.push_str(name);
        text.push('=');
        text.push_str(&kodieren(wert));
    }
    text
}

/// Prozent-Kodierung für Query-Werte: alles außer `A–Z a–z 0–9 - . _ ~`.
fn kodieren(wert: &str) -> String {
    let mut raus = String::with_capacity(wert.len());
    for byte in wert.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                raus.push(*byte as char)
            }
            anderes => {
                use std::fmt::Write as _;
                let _ = write!(raus, "%{anderes:02X}");
            }
        }
    }
    raus
}

/// `{"error": {"code": …, "message": …}}` in seine zwei Teile zerlegen.
fn graph_fehler_felder(text: &str) -> (String, String) {
    let Ok(wert) = serde_json::from_str::<Value>(text) else {
        return (String::new(), kuerzen(text));
    };
    let Some(fehler) = wert.get("error") else {
        return (String::new(), kuerzen(text));
    };
    let code = fehler
        .get("code")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let meldung = fehler
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if meldung.is_empty() {
        (code, kuerzen(text))
    } else {
        (code, meldung)
    }
}

/// `Retry-After` in Sekunden, höchstens [`HOECHSTENS_WARTEN`].
fn wartezeit(kopfzeilen: &reqwest::header::HeaderMap) -> Option<Duration> {
    let roh = kopfzeilen.get(reqwest::header::RETRY_AFTER)?;
    let text = roh.to_str().ok()?.trim();
    let sekunden: u64 = text.parse().ok()?;
    Some(Duration::from_secs(sekunden).min(HOECHSTENS_WARTEN))
}

/// Text auf eine feste Länge kürzen (an einer Zeichengrenze).
fn kuerzen(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= HOECHSTENS_TEXT {
        return text.to_string();
    }
    let mut gekuerzt: String = text.chars().take(HOECHSTENS_TEXT).collect();
    gekuerzt.push('…');
    gekuerzt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fehlerfelder_werden_gelesen() {
        let (code, text) = graph_fehler_felder(
            r#"{"error":{"code":"InvalidAuthenticationToken","message":"Access token has expired."}}"#,
        );
        assert_eq!(code, "InvalidAuthenticationToken");
        assert_eq!(text, "Access token has expired.");
    }

    #[test]
    fn unbekannte_fehlerform_bleibt_text() {
        let (code, text) = graph_fehler_felder("<html>kaputt</html>");
        assert!(code.is_empty());
        assert_eq!(text, "<html>kaputt</html>");
    }

    #[test]
    fn langer_text_wird_gekuerzt() {
        let lang = "a".repeat(HOECHSTENS_TEXT + 50);
        let gekuerzt = kuerzen(&lang);
        assert_eq!(gekuerzt.chars().count(), HOECHSTENS_TEXT + 1);
        assert!(gekuerzt.ends_with('…'));
    }

    #[test]
    fn abfrage_wird_prozent_kodiert() {
        let text = abfrage_text(&[
            ("$select", "id,subject".to_string()),
            ("$orderby", "receivedDateTime desc".to_string()),
            ("$search", "\"Urlaub\"".to_string()),
        ]);
        assert_eq!(
            text,
            "$select=id%2Csubject&$orderby=receivedDateTime%20desc&$search=%22Urlaub%22"
        );
        assert!(abfrage_text(&[]).is_empty());
    }

    #[test]
    fn retry_after_wird_gedeckelt() {
        let mut kopfzeilen = reqwest::header::HeaderMap::new();
        kopfzeilen.insert(reqwest::header::RETRY_AFTER, "3".parse().unwrap());
        assert_eq!(wartezeit(&kopfzeilen), Some(Duration::from_secs(3)));

        kopfzeilen.insert(reqwest::header::RETRY_AFTER, "600".parse().unwrap());
        assert_eq!(wartezeit(&kopfzeilen), Some(HOECHSTENS_WARTEN));

        kopfzeilen.insert(
            reqwest::header::RETRY_AFTER,
            "Wed, 21 Oct 2026 07:28:00 GMT".parse().unwrap(),
        );
        assert_eq!(wartezeit(&kopfzeilen), None, "Datum verstehen wir nicht");

        assert_eq!(wartezeit(&reqwest::header::HeaderMap::new()), None);
    }
}
