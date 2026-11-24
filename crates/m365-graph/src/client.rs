// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der Graph-Client (SPEC.md §4): Token über [`m365_anmeldung::Anmeldung`],
//! 401 → Token verwerfen und **einmal** wiederholen, 429/503 mit `Retry-After`
//! → höchstens **zwei** Wiederholungen, und nur bei **lesenden** Aufrufen
//! (Aufgabe 05: ein Senden wird nie automatisch wiederholt, sonst entstehen
//! doppelte Nachrichten; Aufgabe 06: dasselbe gilt für PATCH und PUT).
//!
//! Drei Dinge kamen mit Aufgabe 06 dazu:
//!
//! * `PATCH` (Planner-Aufgabe ändern, Listeneinträge) und `PUT` (Datei
//!   hochladen) – beide nie wiederholt.
//! * **Text** statt JSON: OneNote liefert HTML, `/content` liefert Dateiinhalte.
//! * Graph leitet `/content` mit 302 auf eine vor-signierte Adresse um. Dieses
//!   eine Ziel wird **ohne** Token geholt – der Link selbst ist die Erlaubnis.
//!
//! Aufgabe 09: Graph antwortet auf `PATCH` und `DELETE` oft mit **204 und leerem
//! Rumpf**, ebenso auf `setReaction`, `markChatReadForUser` und `softDelete`. Ein
//! leerer Rumpf ist bei diesen Aufrufen ein **Erfolg**, kein Grund, ein Modell zu
//! erwarten. Dafür stehen [`GraphClient::patch_ohne_antwort`],
//! [`GraphClient::post_ohne_antwort`] und [`GraphClient::delete`] bereit – sie
//! lesen nichts aus dem Rumpf.
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
/// So viel Antwort liest der Client höchstens in den Speicher (8 MB).
///
/// Alles darüber wäre kein Modell mehr, sondern ein Download; Dateien liest
/// `get_text` und kürzt danach ohnehin.
const HOECHSTENS_ANTWORT: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Methode {
    Get,
    Post,
    Patch,
    Put,
    Delete,
}

/// Ein Körper für Graph: JSON oder reiner Text (Dateiinhalt, OneNote-HTML).
enum Koerper<'a> {
    Kein,
    Json(&'a Value),
    Text { inhalt: &'a str, typ: &'a str },
}

/// Was Graph geantwortet hat – roh, damit auch Text (HTML, Dateiinhalt) geht.
struct RoheAntwort {
    status: u16,
    /// `Location` einer Weiterleitung (`/content` → vor-signierte Adresse).
    location: Option<String>,
    text: String,
    /// Wurde bei [`HOECHSTENS_ANTWORT`] abgeschnitten?
    abgeschnitten: bool,
}

impl RoheAntwort {
    fn erfolg(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// GET/POST/PATCH/PUT gegen Graph, mit den Rechten des angemeldeten Users.
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
            // Die eine Ausnahme (`/content`) holt [`GraphClient::weiterleitung`].
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
            .anfrage(Methode::Get, pfad, query, &Koerper::Kein, kopfzeilen)
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
            .anfrage_url(Methode::Get, ziel, &Koerper::Kein, &[])
            .await?;
        Self::lesen(wert)
    }

    /// Eine Adresse lesen und den Text **roh** zurückgeben: Dateiinhalt
    /// (`/content`) oder OneNote-HTML. Eine Weiterleitung auf eine
    /// vor-signierte Adresse wird ohne Token geholt.
    pub async fn get_text(
        &self,
        pfad: &str,
        query: &[(&str, String)],
    ) -> Result<String, GraphFehler> {
        self.get_text_mit_kopf(pfad, query, &[]).await
    }

    /// Wie [`GraphClient::get_text`], mit zusätzlichen Kopfzeilen.
    pub async fn get_text_mit_kopf(
        &self,
        pfad: &str,
        query: &[(&str, String)],
        kopfzeilen: &[(&str, &str)],
    ) -> Result<String, GraphFehler> {
        let mut ziel = self.adresse(pfad)?;
        if !query.is_empty() {
            ziel.set_query(Some(&abfrage_text(query)));
        }
        let antwort = self
            .anfrage_roh(Methode::Get, ziel, &Koerper::Kein, kopfzeilen, true)
            .await?;
        let antwort = self.weiterleitung(antwort).await?;
        if !antwort.erfolg() {
            return Err(fehler_aus(&antwort));
        }
        Ok(antwort.text)
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

    /// `POST <pfad>` mit JSON-Körper (wird nie automatisch wiederholt).
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
            .anfrage(Methode::Post, pfad, &[], &Koerper::Json(body), kopfzeilen)
            .await?;
        Self::lesen(wert)
    }

    /// `POST <pfad>` mit einem Textkörper (OneNote-Seite: HTML).
    pub async fn post_text<T: DeserializeOwned>(
        &self,
        pfad: &str,
        inhalt: &str,
        typ: &str,
    ) -> Result<T, GraphFehler> {
        let wert = self
            .anfrage(
                Methode::Post,
                pfad,
                &[],
                &Koerper::Text { inhalt, typ },
                &[],
            )
            .await?;
        Self::lesen(wert)
    }

    /// `PATCH <pfad>` ohne erwartete Antwort (Planner, To Do, Listenfelder).
    ///
    /// Graph antwortet hier mit **204 und leerem Rumpf**. Genau das ist der
    /// Erfolg: es wird nichts gelesen, ein 4xx/5xx bleibt ein Fehler. Wird nie
    /// automatisch wiederholt.
    ///
    /// Einen PATCH, der ein Modell zurückgibt, gibt es bei Graph nicht – deshalb
    /// **nur** diese Fassung (Aufgabe 09).
    pub async fn patch_ohne_antwort(
        &self,
        pfad: &str,
        body: &Value,
        kopfzeilen: &[(&str, &str)],
    ) -> Result<(), GraphFehler> {
        self.anfrage_ohne_inhalt(Methode::Patch, pfad, &Koerper::Json(body), kopfzeilen)
            .await
    }

    /// `POST <pfad>` ohne erwartete Antwort (`setReaction`, `markChatReadForUser`,
    /// `softDelete`, `move`).
    ///
    /// 204 oder leerer Rumpf ist Erfolg; ein 2xx mit Inhalt wird ebenfalls
    /// angenommen, nur eben nicht gelesen. Wird nie automatisch wiederholt.
    pub async fn post_ohne_antwort(&self, pfad: &str, body: &Value) -> Result<(), GraphFehler> {
        self.anfrage_ohne_inhalt(Methode::Post, pfad, &Koerper::Json(body), &[])
            .await
    }

    /// `DELETE <pfad>` – mit optionalen Kopfzeilen (`If-Match` bei Planner).
    ///
    /// Graph antwortet mit **204 und leerem Rumpf**; das ist der Erfolg. Wird nie
    /// automatisch wiederholt: ein zweites DELETE würde nur noch 404 liefern.
    pub async fn delete(&self, pfad: &str, kopfzeilen: &[(&str, &str)]) -> Result<(), GraphFehler> {
        self.anfrage_ohne_inhalt(Methode::Delete, pfad, &Koerper::Kein, kopfzeilen)
            .await
    }

    /// `PUT <pfad>?...` mit einem Textkörper (einfacher Upload bis 4 MB).
    ///
    /// Wird nie automatisch wiederholt: ein zweiter Upload könnte eine doppelte
    /// Datei anlegen.
    pub async fn put_text<T: DeserializeOwned>(
        &self,
        pfad: &str,
        query: &[(&str, String)],
        inhalt: &str,
        typ: &str,
    ) -> Result<T, GraphFehler> {
        let wert = self
            .anfrage(
                Methode::Put,
                pfad,
                query,
                &Koerper::Text { inhalt, typ },
                &[],
            )
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
        koerper: &Koerper<'_>,
        kopfzeilen: &[(&str, &str)],
    ) -> Result<Value, GraphFehler> {
        let mut ziel = reqwest::Url::parse(&format!("{}{}", self.basis, pfad))
            .map_err(|fehler| GraphFehler::Antwort(format!("Adresse unbrauchbar: {fehler}")))?;
        if !query.is_empty() {
            ziel.set_query(Some(&abfrage_text(query)));
        }
        self.anfrage_url(methode, ziel, koerper, kopfzeilen).await
    }

    /// Wie [`GraphClient::anfrage`], aber der Rumpf wird nicht gelesen: 204 und
    /// leerer Rumpf sind Erfolg (Aufgabe 09).
    async fn anfrage_ohne_inhalt(
        &self,
        methode: Methode,
        pfad: &str,
        koerper: &Koerper<'_>,
        kopfzeilen: &[(&str, &str)],
    ) -> Result<(), GraphFehler> {
        let ziel = reqwest::Url::parse(&format!("{}{}", self.basis, pfad))
            .map_err(|fehler| GraphFehler::Antwort(format!("Adresse unbrauchbar: {fehler}")))?;
        // 429/503 nur bei lesenden Aufrufen wiederholen: ein zweiter POST,
        // PATCH, PUT oder DELETE könnte doppelt wirken (Aufgabe 05/06/09).
        let wiederholbar = methode == Methode::Get;
        let antwort = self
            .anfrage_roh(methode, ziel, koerper, kopfzeilen, wiederholbar)
            .await?;
        auswerten_ohne_inhalt(antwort)
    }

    async fn anfrage_url(
        &self,
        methode: Methode,
        ziel: reqwest::Url,
        koerper: &Koerper<'_>,
        kopfzeilen: &[(&str, &str)],
    ) -> Result<Value, GraphFehler> {
        // 429/503 nur bei lesenden Aufrufen wiederholen: ein zweiter POST,
        // PATCH oder PUT könnte doppelt wirken (Aufgabe 05/06).
        let wiederholbar = methode == Methode::Get;
        let antwort = self
            .anfrage_roh(methode, ziel, koerper, kopfzeilen, wiederholbar)
            .await?;
        auswerten(antwort)
    }

    async fn anfrage_roh(
        &self,
        methode: Methode,
        ziel: reqwest::Url,
        koerper: &Koerper<'_>,
        kopfzeilen: &[(&str, &str)],
        wiederholbar: bool,
    ) -> Result<RoheAntwort, GraphFehler> {
        let mut erneuert = false;
        let mut wiederholungen = 0u32;
        loop {
            let token = self.token().await?;
            let mut anfrage = match methode {
                Methode::Get => self.http.get(ziel.clone()),
                Methode::Post => self.http.post(ziel.clone()),
                Methode::Patch => self.http.patch(ziel.clone()),
                Methode::Put => self.http.put(ziel.clone()),
                Methode::Delete => self.http.delete(ziel.clone()),
            }
            .bearer_auth(&token);
            for (name, wert) in kopfzeilen {
                anfrage = anfrage.header(*name, *wert);
            }
            match koerper {
                Koerper::Kein => {}
                Koerper::Json(wert) => {
                    anfrage = anfrage.json(wert);
                }
                Koerper::Text { inhalt, typ } => {
                    anfrage = anfrage
                        .header(reqwest::header::CONTENT_TYPE, *typ)
                        .body((*inhalt).to_string());
                }
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

            let location = antwort
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|wert| wert.to_str().ok())
                .map(str::to_string);
            let (text, abgeschnitten) = text_begrenzt(antwort).await?;
            return Ok(RoheAntwort {
                status: status.as_u16(),
                location,
                text,
                abgeschnitten,
            });
        }
    }

    /// Graph leitet `/content` mit 302 auf eine vor-signierte Adresse um. Diese
    /// eine Adresse wird **ohne** Token geholt (der Link enthält die Erlaubnis);
    /// alles andere bleibt liegen.
    async fn weiterleitung(&self, antwort: RoheAntwort) -> Result<RoheAntwort, GraphFehler> {
        if !matches!(antwort.status, 301 | 302 | 303 | 307 | 308) {
            return Ok(antwort);
        }
        let Some(location) = antwort.location.as_deref() else {
            return Ok(antwort);
        };
        let ziel = reqwest::Url::parse(location).map_err(|fehler| {
            GraphFehler::Antwort(format!("Weiterleitung unbrauchbar: {fehler}"))
        })?;
        if !matches!(ziel.scheme(), "http" | "https") {
            return Err(GraphFehler::Antwort(format!(
                "Weiterleitung mit unerwartetem Schema: {}",
                ziel.scheme()
            )));
        }
        let geholt = self
            .http
            .get(ziel)
            .send()
            .await
            .map_err(|fehler| GraphFehler::Netz(format!("{fehler}")))?;
        let status = geholt.status().as_u16();
        let (text, abgeschnitten) = text_begrenzt(geholt).await?;
        Ok(RoheAntwort {
            status,
            location: None,
            text,
            abgeschnitten,
        })
    }

    fn lesen<T: DeserializeOwned>(wert: Value) -> Result<T, GraphFehler> {
        serde_json::from_value(wert)
            .map_err(|fehler| GraphFehler::Antwort(format!("unerwartete Felder: {fehler}")))
    }
}

/// Antwort als JSON-Modell oder als Graph-Fehler.
fn auswerten(antwort: RoheAntwort) -> Result<Value, GraphFehler> {
    if antwort.erfolg() {
        if antwort.abgeschnitten {
            return Err(GraphFehler::Antwort(
                "Antwort von Graph ist zu groß für ein Modell".to_string(),
            ));
        }
        let text = antwort.text.trim();
        if text.is_empty() {
            // z. B. `sendMail` antwortet mit 202 ohne Körper.
            return Ok(Value::Null);
        }
        return serde_json::from_str(text)
            .map_err(|fehler| GraphFehler::Antwort(format!("kein JSON: {fehler}")));
    }
    Err(fehler_aus(&antwort))
}

/// Antwort ohne Inhalt auswerten: 2xx ist Erfolg, egal ob ein Rumpf dabei ist
/// (Aufgabe 09). Alles andere wird zum Graph-Fehler.
fn auswerten_ohne_inhalt(antwort: RoheAntwort) -> Result<(), GraphFehler> {
    if antwort.erfolg() {
        Ok(())
    } else {
        Err(fehler_aus(&antwort))
    }
}

/// Liest höchstens [`HOECHSTENS_ANTWORT`] Bytes; mehr landet nicht im Speicher.
async fn text_begrenzt(mut antwort: reqwest::Response) -> Result<(String, bool), GraphFehler> {
    let mut bytes: Vec<u8> = Vec::new();
    loop {
        let stueck = antwort
            .chunk()
            .await
            .map_err(|fehler| GraphFehler::Netz(format!("{fehler}")))?;
        let Some(stueck) = stueck else {
            break;
        };
        let platz = HOECHSTENS_ANTWORT.saturating_sub(bytes.len());
        if platz == 0 {
            return Ok((String::from_utf8_lossy(&bytes).to_string(), true));
        }
        bytes.extend_from_slice(&stueck[..stueck.len().min(platz)]);
        if bytes.len() >= HOECHSTENS_ANTWORT {
            return Ok((String::from_utf8_lossy(&bytes).to_string(), true));
        }
    }
    Ok((String::from_utf8_lossy(&bytes).to_string(), false))
}

/// Fehler von Graph (Status, Code, Text) – ohne Token.
fn fehler_aus(antwort: &RoheAntwort) -> GraphFehler {
    let (code, meldung) = graph_fehler_felder(&antwort.text);
    GraphFehler::Graph {
        status: antwort.status,
        code,
        text: kuerzen(&meldung),
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

    #[test]
    fn nur_erfolg_ist_erfolg() {
        let antwort = |status| RoheAntwort {
            status,
            location: None,
            text: String::new(),
            abgeschnitten: false,
        };
        assert!(antwort(200).erfolg());
        assert!(antwort(204).erfolg());
        assert!(!antwort(302).erfolg());
        assert!(!antwort(404).erfolg());
    }

    #[test]
    fn leerer_rumpf_ist_bei_aenderungen_ein_erfolg() {
        let antwort = |status, text: &str| RoheAntwort {
            status,
            location: None,
            text: text.to_string(),
            abgeschnitten: false,
        };
        // 204 ohne Rumpf: Erfolg, kein Modell erwartet (Aufgabe 09).
        assert!(auswerten_ohne_inhalt(antwort(204, "")).is_ok());
        // 200 mit Rumpf: wird nicht gelesen, ist aber auch Erfolg.
        assert!(auswerten_ohne_inhalt(antwort(200, r#"{"ok":true}"#)).is_ok());
        // Ein Fehler bleibt ein Fehler.
        assert!(matches!(
            auswerten_ohne_inhalt(antwort(403, r#"{"error":{"code":"accessDenied"}}"#)),
            Err(GraphFehler::Graph { status: 403, .. })
        ));
        // Und ein leerer Rumpf ist als JSON `Null` – genau das war der Fehler:
        // daraus lässt sich kein `PlannerAufgabe` bauen.
        assert_eq!(auswerten(antwort(204, "")).expect("Erfolg"), Value::Null);
    }

    #[test]
    fn eine_zu_grosse_antwort_wird_kein_modell() {
        let antwort = RoheAntwort {
            status: 200,
            location: None,
            text: "{}".to_string(),
            abgeschnitten: true,
        };
        assert!(matches!(auswerten(antwort), Err(GraphFehler::Antwort(_))));
    }
}
