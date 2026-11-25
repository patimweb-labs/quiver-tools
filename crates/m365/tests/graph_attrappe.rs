// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Attrappe für Microsoft Graph (SPEC.md §6): `/me`, `/me/messages`, `/me/sendMail`,
//! `/me/calendarView`, `/me/events`, die Dateisuche – und ein Protokoll aller
//! Anfragen, damit die Tests Pfad, Query, Kopfzeilen und Körper prüfen können.
//!
//! Nie echte Microsoft-Server.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use axum::body::to_bytes;
use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Router;
use quiver_m365::kodierung::entkodieren;
use serde_json::{json, Value};

/// So lange darf eine Anfrage in der Attrappe höchstens dauern.
pub const GEDULD: Duration = Duration::from_secs(10);

/// Der Mandant, den ein echtes Entra-Token im `tid`-Anspruch trägt.
/// `teams_als_gelesen` schreibt ihn in den Rumpf (Aufgabe 07).
pub const MANDANT: &str = "test-mandant";

/// Ein Access-Token, wie Entra ihn liefert: JWT mit `tid`-Anspruch
/// (Kopf `{"alg":"none","typ":"JWT"}`, Nutzlast `{"tid":"test-mandant"}`).
pub fn access_token() -> String {
    "eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0.eyJ0aWQiOiJ0ZXN0LW1hbmRhbnQifQ.".to_string()
}

/// Wörter im langen Mailtext der Attrappe – mehr als die 4000 Zeichen, die
/// `mail_lesen` standardmäßig zurückgibt (Aufgabe 04).
pub const LANGE_MAIL_WOERTER: u32 = 1200;

/// Ein langer Mailtext aus lauter Wörtern (keine Satzzeichen, damit die
/// Wortgrenze eindeutig ist).
pub fn langer_text() -> String {
    (1..=LANGE_MAIL_WOERTER)
        .map(|nummer| format!("Wort{nummer}"))
        .collect::<Vec<String>>()
        .join(" ")
}

/// Eine Mail mit Leerzeilen-Wüsten (fünf, dann vier Leerzeilen hintereinander).
pub const LEERZEILEN_MAIL: &str =
    "Zeile 1\r\n\r\n\r\n\r\n\r\nZeile 2\r\n  \r\n\t\r\n\r\n\r\nZeile 3";

/// Der neue Teil der Mail `m1` – ohne den zitierten Verlauf.
pub const NEUER_TEIL: &str = "Nur der neue Teil.";
/// Der ganze Text von `m1` samt Zitat (`verlauf: true`).
pub const GANZER_TEXT: &str = "Nur der neue Teil.\n\nAm 20.11. schrieb Bert:\n> Der alte Teil.";

/// Antwort, die die Attrappe statt der normalen Daten schickt.
#[derive(Debug, Clone)]
pub struct Fehlerantwort {
    pub status: u16,
    pub kopfzeilen: Vec<(String, String)>,
    pub code: String,
    pub text: String,
}

impl Fehlerantwort {
    fn als_response(&self) -> Response {
        let body = json!({"error": {"code": self.code, "message": self.text}}).to_string();
        let mut antwort = (
            StatusCode::from_u16(self.status).expect("gültiger Status"),
            [(header::CONTENT_TYPE, "application/json")],
            body,
        )
            .into_response();
        for (name, wert) in &self.kopfzeilen {
            if let (Ok(name), Ok(wert)) = (
                header::HeaderName::from_bytes(name.as_bytes()),
                header::HeaderValue::from_str(wert),
            ) {
                antwort.headers_mut().insert(name, wert);
            }
        }
        antwort
    }
}

/// Eine Anfrage, wie sie bei Graph ankam.
#[derive(Debug, Clone)]
pub struct Aufzeichnung {
    pub methode: String,
    pub pfad: String,
    pub abfrage: Vec<(String, String)>,
    pub kopfzeilen: Vec<(String, String)>,
    pub body: Option<Value>,
    /// Der rohe Körper als Text – bei PUT/OneNote ist er kein JSON.
    pub roher_body: Option<String>,
    /// Länge des rohen Körpers in Bytes.
    pub rohe_bytes: Option<usize>,
}

impl Aufzeichnung {
    /// Ein Wert aus der Query (`startDateTime`, `$select`, …).
    pub fn query(&self, name: &str) -> Option<&str> {
        self.abfrage
            .iter()
            .find(|(feld, _)| feld == name)
            .map(|(_, wert)| wert.as_str())
    }

    /// Eine Kopfzeile, Name ohne Rücksicht auf Groß-/Kleinschreibung.
    pub fn kopf(&self, name: &str) -> Option<&str> {
        self.kopfzeilen
            .iter()
            .find(|(feld, _)| feld.eq_ignore_ascii_case(name))
            .map(|(_, wert)| wert.as_str())
    }

    /// Der Body (nur bei POST gesetzt).
    pub fn body(&self) -> &Value {
        self.body.as_ref().expect("POST ohne Body aufgezeichnet")
    }
}

struct Zustand {
    aufzeichnungen: Mutex<Vec<Aufzeichnung>>,
    /// Gilt für die nächste Anfrage, dann wieder normal.
    einmal: Mutex<Option<Fehlerantwort>>,
    /// Gilt ab jetzt für jede Anfrage.
    dauerhaft: Mutex<Option<Fehlerantwort>>,
    /// Was ein PATCH geändert hat (Pfad ohne `/v1.0` → Rumpf). Ein GET danach
    /// zeigt den neuen Stand – wie bei Graph (Aufgabe 09).
    geaendert: Mutex<HashMap<String, Value>>,
}

/// Der Graph-Mock.
pub struct GraphAttrappe {
    zustand: Arc<Zustand>,
    /// z. B. `http://127.0.0.1:52341/v1.0`
    pub adresse: String,
    bedienung: tokio::task::JoinHandle<()>,
}

impl GraphAttrappe {
    pub async fn starten() -> Self {
        let zustand = Arc::new(Zustand {
            aufzeichnungen: Mutex::new(Vec::new()),
            einmal: Mutex::new(None),
            dauerhaft: Mutex::new(None),
            geaendert: Mutex::new(HashMap::new()),
        });
        let app = Router::new()
            .fallback(dienen)
            .with_state(Arc::clone(&zustand));

        let lauscher = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("Graph-Attrappe bindet");
        let port = lauscher.local_addr().expect("Adresse").port();
        let bedienung = tokio::spawn(async move {
            let _ = axum::serve(lauscher, app).await;
        });

        Self {
            zustand,
            adresse: format!("http://127.0.0.1:{port}/v1.0"),
            bedienung,
        }
    }

    /// Alle Anfragen, die bisher ankamen.
    pub fn aufzeichnungen(&self) -> Vec<Aufzeichnung> {
        fach(&self.zustand.aufzeichnungen).clone()
    }

    /// Die einzige Anfrage (der Test erwartet genau eine).
    pub fn einzige(&self) -> Aufzeichnung {
        let alle = self.aufzeichnungen();
        assert_eq!(
            alle.len(),
            1,
            "genau eine Graph-Anfrage erwartet: {alle:#?}"
        );
        alle.into_iter().next().expect("eine Anfrage")
    }

    /// Nächste Anfrage scheitert mit 401 (Token abgelaufen).
    pub fn einmal_401(&self) {
        *fach(&self.zustand.einmal) = Some(Fehlerantwort {
            status: 401,
            kopfzeilen: Vec::new(),
            code: "InvalidAuthenticationToken".to_string(),
            text: "Access token has expired.".to_string(),
        });
    }

    /// Nächste Anfrage scheitert mit 429 und `Retry-After`.
    pub fn einmal_429(&self, sekunden: u64) {
        *fach(&self.zustand.einmal) = Some(Fehlerantwort {
            status: 429,
            kopfzeilen: vec![("retry-after".to_string(), sekunden.to_string())],
            code: "TooManyRequests".to_string(),
            text: "Please retry.".to_string(),
        });
    }

    /// Ab jetzt antwortet jede Anfrage mit 429 und `Retry-After` – dafür, dass
    /// ein lesender Aufruf höchstens zweimal wiederholt.
    pub fn ab_jetzt_429(&self, sekunden: u64) {
        *fach(&self.zustand.dauerhaft) = Some(Fehlerantwort {
            status: 429,
            kopfzeilen: vec![("retry-after".to_string(), sekunden.to_string())],
            code: "TooManyRequests".to_string(),
            text: "Please retry.".to_string(),
        });
    }

    /// Ab jetzt scheitert jede Anfrage so (Graph-Fehler).
    pub fn dauerfehler(&self, status: u16, code: &str, text: &str) {
        *fach(&self.zustand.dauerhaft) = Some(Fehlerantwort {
            status,
            kopfzeilen: Vec::new(),
            code: code.to_string(),
            text: text.to_string(),
        });
    }
}

impl Drop for GraphAttrappe {
    fn drop(&mut self) {
        self.bedienung.abort();
    }
}

fn fach<T>(fach: &Mutex<T>) -> MutexGuard<'_, T> {
    fach.lock()
        .unwrap_or_else(|vergiftet| vergiftet.into_inner())
}

/// Hat ein PATCH diesen Pfad geändert? Dann zeigt der GET danach den neuen Stand.
fn gemerkt(zustand: &Zustand, pfad: &str) -> Option<Value> {
    fach(&zustand.geaendert).get(pfad).cloned()
}

/// Den Rumpf eines PATCH über einen Grundwert legen – flach, wie Graph es tut.
fn ueberlagern(mut basis: Value, aenderung: Option<Value>) -> Value {
    if let (Value::Object(ziel), Some(Value::Object(quelle))) = (&mut basis, aenderung) {
        for (name, wert) in quelle {
            ziel.insert(name, wert);
        }
    }
    basis
}

/// Die Id des Eintrags aus `…/items/{id}` bzw. `…/items/{id}/fields`.
fn eintrag_id(pfad: &str) -> &str {
    pfad.split("/items/")
        .nth(1)
        .unwrap_or_default()
        .trim_end_matches("/fields")
        .trim_end_matches('/')
}

/// Die Spalten eines Listeneintrags (Grundstand der Attrappe).
fn listeneintrag_felder(id: &str) -> Value {
    listeneintraege()
        .into_iter()
        .find(|eintrag| eintrag["id"] == id)
        .and_then(|eintrag| eintrag.get("fields").cloned())
        .unwrap_or_else(|| json!({}))
}

async fn dienen(State(zustand): State<Arc<Zustand>>, request: Request) -> Response {
    let methode = request.method().to_string();
    let pfad = request.uri().path().to_string();
    let abfrage = felder(request.uri().query().unwrap_or_default());
    let host = request
        .headers()
        .get(axum::http::header::HOST)
        .and_then(|wert| wert.to_str().ok())
        .unwrap_or("127.0.0.1")
        .to_string();
    let kopfzeilen: Vec<(String, String)> = request
        .headers()
        .iter()
        .map(|(name, wert)| {
            (
                name.as_str().to_string(),
                String::from_utf8_lossy(wert.as_bytes()).to_string(),
            )
        })
        .collect();

    // POST/PATCH/PUT tragen einen Körper; der Text einer Datei ist kein JSON,
    // deshalb wird er zusätzlich roh aufgezeichnet.
    let (body, roher_body, rohe_bytes) = if matches!(methode.as_str(), "POST" | "PATCH" | "PUT") {
        let bytes = to_bytes(request.into_body(), 8 << 20)
            .await
            .expect("Body lesbar");
        let roh = String::from_utf8_lossy(&bytes).to_string();
        (
            serde_json::from_slice::<Value>(&bytes).ok(),
            Some(roh),
            Some(bytes.len()),
        )
    } else {
        (None, None, None)
    };

    fach(&zustand.aufzeichnungen).push(Aufzeichnung {
        methode: methode.clone(),
        pfad: pfad.clone(),
        abfrage: abfrage.clone(),
        kopfzeilen,
        body: body.clone(),
        roher_body: roher_body.clone(),
        rohe_bytes,
    });

    if let Some(antwort) = fach(&zustand.einmal).take() {
        return antwort.als_response();
    }
    if let Some(antwort) = fach(&zustand.dauerhaft).clone() {
        return antwort.als_response();
    }

    let ohne_basis = pfad.strip_prefix("/v1.0").unwrap_or(&pfad);

    // Was Graph nicht erlaubt, erlaubt die Attrappe auch nicht (Aufgabe 07):
    // so fallen dieselben Fehler schon im Test auf.
    if let Some(antwort) = graph_regeln(&methode, ohne_basis, &abfrage, &body) {
        return antwort;
    }

    // `PATCH` antwortet bei Graph mit **204 und leerem Rumpf** (Aufgabe 09):
    // die Änderung wird gemerkt, damit ein GET danach den neuen Stand zeigt.
    if methode == "PATCH" {
        if let Some(wert) = body.as_ref() {
            fach(&zustand.geaendert).insert(ohne_basis.to_string(), wert.clone());
        }
        return StatusCode::NO_CONTENT.into_response();
    }

    let antwort = match (methode.as_str(), ohne_basis) {
        // Graph antwortet auf `sendMail` mit 202 ohne Körper.
        ("POST", "/me/sendMail") => return StatusCode::ACCEPTED.into_response(),
        ("POST", "/me/events") => ereignis_aus(&body),
        ("GET", "/me") => json!({
            "id": "u-anna",
            "displayName": "Anna Muster",
            "mail": "anna@firma.de",
            "userPrincipalName": "anna@firma.de",
            "department": "Entwicklung",
        }),
        ("GET", "/me/messages") => json!({
            "value": [
                {"id": "m1", "subject": "Lage", "from": {"emailAddress": {"name": "Anna", "address": "anna@firma.de"}},
                 "receivedDateTime": "2026-11-27T09:00:00Z", "bodyPreview": "Kurz"},
                {"id": "m2", "subject": "Bericht", "from": {"emailAddress": {"name": "Bert", "address": "bert@firma.de"}},
                 "receivedDateTime": "2026-11-26T09:00:00Z", "bodyPreview": "Auch kurz"},
            ]
        }),
        ("GET", "/me/mailFolders/inbox/messages") => json!({
            "value": [
                {"id": "m1", "subject": "Lage", "from": {"emailAddress": {"name": "Anna", "address": "anna@firma.de"}},
                 "receivedDateTime": "2026-11-27T09:00:00Z", "bodyPreview": "Kurz"},
            ]
        }),
        ("GET", pfad) if pfad.starts_with("/me/messages/") => {
            let ohne_basis = pfad.strip_prefix("/me/messages/").unwrap_or_default();
            let (id, neu, ganz) = match ohne_basis {
                "lang" => ("lang", langer_text(), langer_text()),
                "leerzeilen" => (
                    "leerzeilen",
                    LEERZEILEN_MAIL.to_string(),
                    LEERZEILEN_MAIL.to_string(),
                ),
                _ => ("m1", NEUER_TEIL.to_string(), GANZER_TEXT.to_string()),
            };
            json!({
                "id": id,
                "subject": if id == "m1" { "Lage" } else { "Lang" },
                "from": {"emailAddress": {"name": "Anna", "address": "anna@firma.de"}},
                "toRecipients": [{"emailAddress": {"address": "bert@firma.de"}}],
                "ccRecipients": [{"emailAddress": {"name": "Clara", "address": "clara@firma.de"}}],
                "receivedDateTime": "2026-11-27T09:00:00Z",
                "uniqueBody": {"contentType": "text", "content": neu},
                "body": {"contentType": "text", "content": ganz},
                "webLink": format!("https://outlook.office365.com/mail/{id}"),
            })
        }
        // `calendarView`: Ortszeit **und** `timeZone`, wie Graph es liefert.
        ("GET", "/me/calendarView") => json!({
            "value": [
                {
                    "id": "t1",
                    "subject": "Jour fixe",
                    "start": {"dateTime": "2026-11-27T10:00:00.0000000", "timeZone": "Europe/Berlin"},
                    "end": {"dateTime": "2026-11-27T11:00:00.0000000", "timeZone": "Europe/Berlin"},
                    "location": {"displayName": "Raum 1"},
                    "attendees": [{"emailAddress": {"name": "Bert", "address": "bert@firma.de"}, "type": "required"}],
                    "isAllDay": false,
                    "webLink": "https://outlook.office365.com/calendar/t1",
                },
                {
                    "id": "t2",
                    "subject": "Sommerfest",
                    "start": {"dateTime": "2026-07-15T08:40:00.0000000", "timeZone": "Europe/Berlin"},
                    "end": {"dateTime": "2026-07-15T09:40:00.0000000", "timeZone": "Europe/Berlin"},
                    "isAllDay": false,
                },
                {
                    "id": "t3",
                    "subject": "Urlaub",
                    "start": {"dateTime": "2026-11-24T00:00:00.0000000", "timeZone": "Europe/Berlin"},
                    "end": {"dateTime": "2026-11-25T00:00:00.0000000", "timeZone": "Europe/Berlin"},
                    "isAllDay": true,
                },
                {
                    "id": "t4",
                    "subject": "Von woanders",
                    "start": {"dateTime": "2026-11-27T09:00:00.0000000", "timeZone": "UTC"},
                    "end": {"dateTime": "2026-11-27T10:00:00.0000000", "timeZone": "UTC"},
                    "isAllDay": false,
                },
            ]
        }),
        ("GET", pfad) if pfad.starts_with("/me/drive/root/search") => json!({
            "value": [{
                "id": "d1",
                "name": "Bericht.pdf",
                "webUrl": "https://onedrive.example/bericht",
                "lastModifiedDateTime": "2026-11-20T08:00:00Z",
                "size": 1234,
                "parentReference": {"path": "/drive/root:/Projekte"},
            }]
        }),
        // ------------------------------------------------------------------
        // Teams (docs/aufgaben/05-teams.md)
        // ------------------------------------------------------------------
        ("GET", "/me/joinedTeams") => json!({
            "value": [
                {"id": "team-1", "displayName": "Entwicklung", "description": "Software"},
                {"id": "team-2", "displayName": "Vertrieb", "description": "Verkauf"},
            ]
        }),
        ("GET", "/me/chats") => chats(),
        ("GET", "/users") => json!({"value": personen()}),
        ("GET", pfad) if pfad.starts_with("/teams/") && pfad.ends_with("/channels") => {
            json!({"value": kanaele()})
        }
        ("GET", pfad)
            if pfad.starts_with("/teams/")
                && pfad.contains("/channels/")
                && pfad.ends_with("/members") =>
        {
            json!({"value": kanal_mitglieder()})
        }
        ("GET", pfad) if pfad.starts_with("/teams/") && pfad.ends_with("/members") => {
            json!({"value": team_mitglieder()})
        }
        ("GET", pfad) if pfad.ends_with("/replies") => json!({"value": antworten(pfad)}),
        ("GET", pfad) if pfad.contains("/messages/") => nachricht(pfad),
        ("GET", pfad) if pfad.starts_with("/teams/") && pfad.ends_with("/messages") => {
            kanal_seite(pfad, &abfrage, &host)
        }
        ("GET", pfad) if pfad.starts_with("/chats/") && pfad.ends_with("/messages") => {
            json!({"value": chat_nachrichten()})
        }
        ("POST", "/chats") => chat_aus(&body),
        ("POST", "/search/query") => suchergebnis(),
        ("POST", pfad) if pfad.ends_with("/setReaction") || pfad.ends_with("/unsetReaction") => {
            return StatusCode::NO_CONTENT.into_response()
        }
        ("POST", pfad)
            if pfad.ends_with("/markChatReadForUser")
                || pfad.ends_with("/markChatUnreadForUser") =>
        {
            return StatusCode::NO_CONTENT.into_response()
        }
        // `softDelete` (Aufgabe 09) und `move` (Mail → Papierkorb) antworten
        // ohne Rumpf bzw. mit dem verschobenen Objekt.
        ("POST", pfad) if pfad.ends_with("/softDelete") => {
            return StatusCode::NO_CONTENT.into_response()
        }
        ("POST", pfad) if pfad.ends_with("/move") => {
            return (
                StatusCode::CREATED,
                axum::Json(json!({
                    "id": "m1-papierkorb",
                    "subject": "Lage",
                    "parentFolderId": "deleteditems",
                })),
            )
                .into_response()
        }
        ("POST", pfad) if pfad.ends_with("/replies") => gesendete_nachricht("neu-antwort"),
        ("POST", pfad) if pfad.ends_with("/messages") => gesendete_nachricht("neu-nachricht"),
        // ------------------------------------------------------------------
        // SharePoint, OneDrive, Listen (docs/aufgaben/06-graph-erweiterung.md)
        // ------------------------------------------------------------------
        ("GET", "/sites") => json!({"value": sites()}),
        ("GET", pfad) if pfad.starts_with("/sites/") && pfad.ends_with("/drives") => {
            json!({"value": laufwerke()})
        }
        ("GET", pfad) if pfad.starts_with("/sites/") && pfad.ends_with("/lists") => {
            json!({"value": listen()})
        }
        ("GET", pfad) if pfad.contains("/lists/") && pfad.ends_with("/items") => {
            json!({"value": listeneintraege()})
        }
        ("POST", pfad) if pfad.contains("/lists/") && pfad.ends_with("/items") => {
            let felder = body
                .as_ref()
                .and_then(|body| body.get("fields"))
                .cloned()
                .unwrap_or(Value::Null);
            json!({
                "id": "3",
                "webUrl": "https://contoso.sharepoint.com/sites/projekt-nord/Lists/Aufgaben/3",
                "lastModifiedDateTime": "2026-11-27T10:00:00Z",
                "fields": felder,
            })
        }
        //
        // Ein einzelner Listeneintrag: seine Spalten (ggf. mit gemerktem PATCH)
        // und der Eintrag selbst (Aufgabe 09).
        ("GET", pfad) if pfad.contains("/lists/") && pfad.ends_with("/fields") => ueberlagern(
            listeneintrag_felder(eintrag_id(pfad)),
            gemerkt(&zustand, pfad),
        ),
        ("GET", pfad)
            if pfad.contains("/lists/")
                && pfad.contains("/items/")
                && !pfad.ends_with("/fields") =>
        {
            let id = eintrag_id(pfad);
            let felder = ueberlagern(
                listeneintrag_felder(id),
                gemerkt(&zustand, &format!("{pfad}/fields")),
            );
            json!({
                "id": id,
                "webUrl": format!("https://contoso.sharepoint.com/sites/projekt-nord/Lists/Aufgaben/{id}"),
                "lastModifiedDateTime": "2026-11-27T09:00:00Z",
                "fields": felder,
            })
        }
        ("DELETE", pfad)
            if pfad.starts_with("/me/events/")
                || pfad.starts_with("/planner/tasks/")
                || pfad.starts_with("/me/contacts/")
                || pfad.starts_with("/me/onenote/pages/")
                || pfad.starts_with("/me/drive/items/")
                || pfad.starts_with("/drives/")
                || (pfad.starts_with("/me/todo/lists/") && pfad.contains("/tasks/"))
                || (pfad.starts_with("/sites/")
                    && pfad.contains("/lists/")
                    && pfad.contains("/items/")) =>
        {
            return StatusCode::NO_CONTENT.into_response()
        }
        ("GET", pfad) if pfad.ends_with("/children") => json!({"value": kinder()}),
        // Ein einzelner Eintrag (Metadaten) – nicht der Inhalt, nicht die Kinder.
        ("GET", pfad) if pfad.contains("/items/") && !pfad.ends_with("/content") => {
            item_aus_id(pfad)
        }
        ("GET", pfad)
            if pfad.contains("/root:/")
                && !pfad.ends_with("/content")
                && !pfad.ends_with("/children") =>
        {
            item_aus_name(pfad)
        }
        ("POST", pfad) if pfad.ends_with("/children") => {
            let name = body
                .as_ref()
                .and_then(|body| body.get("name"))
                .cloned()
                .unwrap_or_else(|| json!("Ordner"));
            json!({
                "id": "neu-ordner",
                "name": name,
                "folder": {"childCount": 0},
                "webUrl": "https://contoso.sharepoint.com/sites/projekt-nord/neu-ordner",
                "lastModifiedDateTime": "2026-11-27T10:00:00Z",
            })
        }
        ("PUT", pfad) if pfad.ends_with("/content") => {
            let name = name_aus_pfad(pfad);
            json!({
                "id": "neu-datei",
                "name": name,
                "size": rohe_bytes.unwrap_or(0),
                "file": {"mimeType": "text/plain"},
                "webUrl": "https://contoso.sharepoint.com/neu-datei",
                "lastModifiedDateTime": "2026-11-27T10:00:00Z",
            })
        }
        // Das Ziel der Weiterleitung von `/content` – ohne Token, wie bei Graph.
        ("GET", pfad) if pfad.starts_with("/download/") => {
            let name = pfad.trim_start_matches("/download/");
            let (typ, text) = textdatei(name);
            return (
                StatusCode::OK,
                [(header::CONTENT_TYPE, typ)],
                text.to_string(),
            )
                .into_response();
        }
        // ------------------------------------------------------------------
        // Planner und To Do
        // ------------------------------------------------------------------
        ("GET", "/me/planner/plans") => json!({"value": plaene()}),
        ("GET", "/groups") => json!({
            "value": [{"id": "group-1", "displayName": "Entwicklung"}]
        }),
        ("GET", pfad) if pfad.starts_with("/groups/") && pfad.ends_with("/planner/plans") => {
            json!({"value": plaene()})
        }
        ("GET", pfad) if pfad.starts_with("/planner/plans/") && pfad.ends_with("/buckets") => {
            json!({"value": buckets()})
        }
        ("GET", pfad) if pfad.starts_with("/planner/plans/") && pfad.ends_with("/tasks") => {
            json!({"value": planner_aufgaben()})
        }
        ("GET", "/me/planner/tasks") => json!({"value": planner_aufgaben()}),
        ("GET", pfad) if pfad.starts_with("/planner/tasks/") => {
            let id = pfad.trim_start_matches("/planner/tasks/");
            ueberlagern(planner_aufgabe(id), gemerkt(&zustand, pfad))
        }
        ("POST", "/planner/tasks") => {
            let body = body.unwrap_or(Value::Null);
            json!({
                "id": "task-neu",
                "planId": body.get("planId").cloned().unwrap_or(Value::Null),
                "bucketId": body.get("bucketId").cloned().unwrap_or(Value::Null),
                "title": body.get("title").cloned().unwrap_or(Value::Null),
                "percentComplete": 0,
                "dueDateTime": body.get("dueDateTime").cloned().unwrap_or(Value::Null),
                "assignments": body.get("assignments").cloned().unwrap_or(Value::Null),
                "@odata.etag": "W/\"neu\"",
            })
        }
        ("GET", "/me/todo/lists") => todo_listen_seite(&abfrage, &host),
        ("GET", pfad) if pfad.starts_with("/me/todo/lists/") && pfad.ends_with("/tasks") => {
            json!({"value": todo_aufgaben()})
        }
        // Eine einzelne To-Do-Aufgabe: der Stand nach einem PATCH (Aufgabe 09),
        // und vor dem Löschen der Titel.
        ("GET", pfad) if pfad.starts_with("/me/todo/lists/") && pfad.contains("/tasks/") => {
            let id = pfad.rsplit('/').next().unwrap_or(TODO_AUFGABE);
            ueberlagern(todo_aufgabe(id), gemerkt(&zustand, pfad))
        }
        ("POST", pfad) if pfad.starts_with("/me/todo/lists/") && pfad.ends_with("/tasks") => {
            let body = body.unwrap_or(Value::Null);
            let mut aufgabe = todo_aufgabe("todo-neu");
            for feld in ["title", "importance", "dueDateTime", "body"] {
                if let Some(wert) = body.get(feld) {
                    aufgabe[feld] = wert.clone();
                }
            }
            aufgabe
        }
        // ------------------------------------------------------------------
        // Kontakte und Personen
        // ------------------------------------------------------------------
        ("GET", "/me/contacts") => json!({"value": kontakte()}),
        ("GET", pfad) if pfad.starts_with("/me/events/") => {
            let id = pfad.trim_start_matches("/me/events/");
            json!({
                "id": id,
                "subject": "Jour fixe",
                "start": {"dateTime": "2026-11-27T10:00:00.0000000", "timeZone": "Europe/Berlin"},
                "end": {"dateTime": "2026-11-27T11:00:00.0000000", "timeZone": "Europe/Berlin"},
                "webLink": format!("https://outlook.office365.com/calendar/{id}"),
            })
        }
        ("GET", pfad) if pfad.starts_with("/me/contacts/") => {
            let id = pfad.trim_start_matches("/me/contacts/");
            let mut kontakt = kontakte()[0].clone();
            kontakt["id"] = json!(id);
            kontakt
        }
        ("POST", "/me/contacts") => {
            let body = body.unwrap_or(Value::Null);
            let mut kontakt = kontakte()[0].clone();
            if let (Value::Object(quelle), Value::Object(ziel)) = (&body, &mut kontakt) {
                for (name, wert) in quelle {
                    ziel.insert(name.clone(), wert.clone());
                }
            }
            kontakt["id"] = json!("k-neu");
            kontakt
        }
        ("GET", "/me/people") => json!({"value": personen_von_people()}),
        // ------------------------------------------------------------------
        // OneNote
        // ------------------------------------------------------------------
        ("GET", "/me/onenote/notebooks") => json!({"value": notizbuecher()}),
        ("GET", pfad) if pfad.starts_with("/me/onenote/") && pfad.ends_with("/sections") => {
            json!({"value": abschnitte()})
        }
        ("GET", pfad) if pfad.starts_with("/me/onenote/") && pfad.ends_with("/pages") => {
            json!({"value": onenote_seiten()})
        }
        ("GET", pfad) if pfad.starts_with("/me/onenote/pages/") && pfad.ends_with("/content") => {
            let text = "<html><head><title>Jour fixe</title></head><body><p>Erste Zeile</p><p>Zweite Zeile</p></body></html>";
            return (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "text/html")],
                text.to_string(),
            )
                .into_response();
        }
        ("GET", pfad) if pfad.starts_with("/me/onenote/pages/") => {
            let id = pfad.trim_start_matches("/me/onenote/pages/");
            onenote_seite(id)
        }
        ("POST", pfad) if pfad.starts_with("/me/onenote/") && pfad.ends_with("/pages") => {
            let titel = roher_body
                .as_deref()
                .and_then(|html| zwischen(html, "<title>", "</title>"))
                .unwrap_or_else(|| "Neue Seite".to_string());
            json!({
                "id": "1-page-neu-0001",
                "title": titel,
                "links": {"oneNoteWebUrl": {"href": "https://example/onenote/1-page-neu-0001"}},
                "createdDateTime": "2026-11-27T10:00:00Z",
                "lastModifiedDateTime": "2026-11-27T10:00:00Z",
            })
        }
        // Zuletzt, weil es jeden `/content`-Pfad fängt: Dateiinhalt (Text oder
        // die PDF-Ausgabe für Office-Dateien).
        ("GET", pfad) if pfad.ends_with("/content") => {
            return inhalt(pfad, &abfrage, &host);
        }
        _ => {
            return (
                StatusCode::NOT_FOUND,
                json!({"error": {"code": "NotFound", "message": format!("{methode} {pfad}")}})
                    .to_string(),
            )
                .into_response()
        }
    };

    (StatusCode::OK, axum::Json(antwort)).into_response()
}

// ---------------------------------------------------------------------------
// Regeln von Graph, die der Live-Test gezeigt hat (Aufgabe 07)
// ---------------------------------------------------------------------------

/// Ein Fehler, wie Graph ihn schickt: `{"error":{"code","message"}}`.
fn graph_antwort(status: u16, code: &str, text: impl Into<String>) -> Response {
    (
        StatusCode::from_u16(status).expect("gültiger Status"),
        axum::Json(json!({"error": {"code": code, "message": text.into()}})),
    )
        .into_response()
}

/// Was Graph auf den Endpunkten aus Aufgabe 07 nicht erlaubt, erlaubt die
/// Attrappe auch nicht – mit demselben Status und demselben Code.
fn graph_regeln(
    methode: &str,
    pfad: &str,
    abfrage: &[(String, String)],
    body: &Option<Value>,
) -> Option<Response> {
    let hat = |name: &str| abfrage.iter().any(|(feld, _)| feld == name);

    // `$top` ist auf joinedTeams und channels nicht erlaubt.
    if methode == "GET" && (pfad == "/me/joinedTeams" || ist_kanal_liste(pfad)) && hat("$top") {
        return Some(graph_antwort(
            400,
            "BadRequest",
            "Query option 'Top' is not allowed.",
        ));
    }

    // OneNote kennt kein `webUrl` bei Notizbüchern und Abschnitten.
    if methode == "GET" && ist_onenote_sammlung(pfad) && select_enthaelt(abfrage, "webUrl") {
        return Some(graph_antwort(
            400,
            "BadRequest",
            "Could not find a property named 'webUrl' on type 'microsoft.graph.onenoteNotebook'.",
        ));
    }

    // Auch Seiten kennen kein `webUrl` (Aufgabe 08).
    if methode == "GET" && ist_onenote_seite(pfad) && select_enthaelt(abfrage, "webUrl") {
        return Some(graph_antwort(
            400,
            "BadRequest",
            "Could not find a property named 'webUrl' on type 'microsoft.graph.onenotePage'.",
        ));
    }

    // `reactionType` muss das Unicode-Emoji sein (`Unicode 'heart' in the
    // payload is not supported`, Aufgabe 08).
    if methode == "POST" && (pfad.ends_with("/setReaction") || pfad.ends_with("/unsetReaction")) {
        match body
            .as_ref()
            .and_then(|body| body.get("reactionType"))
            .and_then(Value::as_str)
        {
            Some(reaktion) if ist_emoji(reaktion) => {}
            Some(reaktion) => {
                return Some(graph_antwort(
                    400,
                    "BadRequest",
                    format!("Unicode '{reaktion}' in the payload is not supported."),
                ))
            }
            None => {
                return Some(graph_antwort(
                    400,
                    "BadRequest",
                    "reactionType is missing in the payload.",
                ))
            }
        }
    }

    // `markChatReadForUser` braucht den Mandanten des Users im Rumpf.
    if methode == "POST"
        && (pfad.ends_with("/markChatReadForUser") || pfad.ends_with("/markChatUnreadForUser"))
        && !body
            .as_ref()
            .and_then(|body| body.get("user"))
            .and_then(|user| user.get("tenantId"))
            .and_then(Value::as_str)
            .is_some_and(|mandant| !mandant.trim().is_empty())
    {
        return Some(graph_antwort(
            400,
            "BadRequest",
            "TenantId for the user must be specified.",
        ));
    }

    // Jedes Chat-Mitglied braucht `user@odata.bind` – und zwar als Bindung an
    // die Ressource, nicht als `/me`.
    if methode == "POST" && pfad == "/chats" {
        let mitglieder = body
            .as_ref()
            .and_then(|body| body.get("members"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut gesehen: Vec<String> = Vec::with_capacity(mitglieder.len());
        for mitglied in &mitglieder {
            match mitglied.get("user@odata.bind").and_then(Value::as_str) {
                Some(bindung) if bindung.trim().ends_with("/me") => {
                    return Some(graph_antwort(
                        400,
                        "BadRequest",
                        "'user@odata.bind' must reference the user by id, not by '/me'.",
                    ))
                }
                Some(bindung) if !bindung.trim().is_empty() => {
                    // Ein Mitglied nur einmal (Aufgabe 08: „Duplicate chat members“).
                    let bindung = bindung.trim().to_lowercase();
                    if gesehen.contains(&bindung) {
                        return Some(graph_antwort(400, "BadRequest", "Duplicate chat members."));
                    }
                    gesehen.push(bindung);
                }
                _ => {
                    return Some(graph_antwort(
                        400,
                        "BadRequest",
                        "'user@odata.bind' field is missing for a member.",
                    ))
                }
            }
        }
    }

    // To Do: `$top` und `$select` sind auf `/me/todo/lists` nicht erlaubt, auf
    // den Aufgaben nur `$select` nicht.
    if methode == "GET" {
        if pfad == "/me/todo/lists" && (hat("$top") || hat("$select")) {
            return Some(graph_antwort(
                400,
                "invalidRequest",
                "The query is not valid (To Do lists allow neither $top nor $select).",
            ));
        }
        if ist_todo_aufgaben(pfad) && hat("$select") {
            return Some(graph_antwort(
                400,
                "invalidRequest",
                "The query is not valid (To Do tasks allow no $select).",
            ));
        }
    }

    None
}

/// Enthält `$select` das genannte Feld?
fn select_enthaelt(abfrage: &[(String, String)], name: &str) -> bool {
    abfrage
        .iter()
        .filter(|(feld, _)| feld == "$select")
        .flat_map(|(_, wert)| wert.split(','))
        .any(|feld| feld.trim().eq_ignore_ascii_case(name))
}

/// `GET /teams/{id}/channels` (nicht `/channels/{id}/messages`).
fn ist_kanal_liste(pfad: &str) -> bool {
    pfad.starts_with("/teams/") && pfad.ends_with("/channels")
}

/// `GET /me/onenote/notebooks`, `…/sections`, `…/notebooks/{id}/sections`.
fn ist_onenote_sammlung(pfad: &str) -> bool {
    pfad.starts_with("/me/onenote/")
        && (pfad.ends_with("/notebooks")
            || pfad.ends_with("/sections")
            || pfad == "/me/onenote/sections")
}

/// `GET …/sections/{id}/pages`, `GET /me/onenote/pages/{id}` – nicht `/content`.
fn ist_onenote_seite(pfad: &str) -> bool {
    pfad.starts_with("/me/onenote/")
        && !pfad.ends_with("/content")
        && (pfad.ends_with("/pages") || pfad.starts_with("/me/onenote/pages/"))
}

/// Ist das ein Emoji? Nur die Bildzeichen-Blöcke, aus denen Reaktionen kommen.
fn ist_emoji(text: &str) -> bool {
    let text = text.trim();
    !text.is_empty()
        && text.chars().all(|zeichen| {
            matches!(
                zeichen as u32,
                0x2190..=0x21FF
                    | 0x2600..=0x27BF
                    | 0x2B00..=0x2BFF
                    | 0x1F000..=0x1FAFF
                    | 0xFE00..=0xFE0F
                    | 0x200D
            )
        })
}

/// `GET /me/todo/lists/{id}/tasks`.
fn ist_todo_aufgaben(pfad: &str) -> bool {
    pfad.starts_with("/me/todo/lists/") && pfad.ends_with("/tasks")
}

/// Der angelegte Termin, so wie Graph ihn zurückgibt (aus dem Body gebaut).
fn ereignis_aus(body: &Option<Value>) -> Value {
    let body = body.clone().unwrap_or(Value::Null);
    let feld = |name: &str| body.get(name).cloned().unwrap_or(Value::Null);
    json!({
        "id": "neu-1",
        "subject": feld("subject"),
        "start": feld("start"),
        "end": feld("end"),
        "location": feld("location"),
        "attendees": feld("attendees"),
        "webLink": "https://outlook.office365.com/calendar/neu-1",
    })
}

/// `a=1&b=zwei%20Worte` → Paare, Prozent-Kodierung aufgelöst.
fn felder(abfrage: &str) -> Vec<(String, String)> {
    abfrage
        .split('&')
        .filter(|teil| !teil.is_empty())
        .map(|teil| {
            let (name, wert) = teil.split_once('=').unwrap_or((teil, ""));
            (entkodieren(name), entkodieren(wert))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Daten für Aufgabe 06: SharePoint, OneDrive, Listen, Planner, To Do, Kontakte,
// Personen, OneNote. Feste Ids, damit die Tests sie nennen können.
// ---------------------------------------------------------------------------

pub const SITE_NORD: &str = "contoso.sharepoint.com,1111,aaaa";
pub const SITE_SUED: &str = "contoso.sharepoint.com,2222,bbbb";
pub const SITE_INTERN: &str = "contoso.sharepoint.com,3333,cccc";
pub const DRIVE_DOKU: &str = "drive-doku";
pub const DRIVE_FOTOS: &str = "drive-fotos";
pub const LISTE_AUFGABEN: &str = "11111111-1111-1111-1111-111111111111";
pub const ITEM_ORDNER: &str = "item-projekte";
pub const ITEM_TEXT: &str = "item-notiz";
pub const ITEM_OFFICE: &str = "item-bericht";
pub const ITEM_PDF: &str = "item-anleitung";
pub const NOTIZ_INHALT: &str = "Zeile eins\nZeile zwei";
pub const PLAN_NORD: &str = "plan-1";
pub const PLAN_SUED: &str = "plan-2";
pub const BUCKET_OFFEN: &str = "bucket-1";
pub const BUCKET_FERTIG: &str = "bucket-2";
pub const AUFGABE_MIT_ETAG: &str = "task-1";
pub const AUFGABE_OHNE_ETAG: &str = "task-ohne-etag";
pub const TODO_LISTE: &str = "AAMkADtodo-aufgaben-1";
pub const TODO_LISTE_EINKAUF: &str = "AAMkADtodo-einkauf-2";
pub const TODO_AUFGABE: &str = "AAMkADtodo-task-1";
pub const NOTIZBUCH: &str = "1-nb-arbeit-0000000001";
pub const ABSCHNITT: &str = "1-sec-besprechungen-0001";
pub const ONENOTE_SEITE: &str = "1-page-jour-fixe-0001";

fn sites() -> Vec<Value> {
    vec![
        json!({
            "id": SITE_NORD,
            "displayName": "Projekt Nord",
            "name": "Projekt-Nord",
            "description": "Der Norden",
            "webUrl": "https://contoso.sharepoint.com/sites/projekt-nord",
        }),
        json!({
            "id": SITE_SUED,
            "displayName": "Projekt Süd",
            "name": "Projekt-Sued",
            "webUrl": "https://contoso.sharepoint.com/sites/projekt-sued",
        }),
        // Damit „Projekt“ mehrdeutig ist: drei Treffer, keiner genau.
        json!({
            "id": SITE_INTERN,
            "displayName": "Projekt Nord Intern",
            "name": "Projekt-Nord-Intern",
            "webUrl": "https://contoso.sharepoint.com/sites/projekt-nord-intern",
        }),
    ]
}

fn laufwerke() -> Vec<Value> {
    vec![
        json!({
            "id": DRIVE_DOKU,
            "name": "Dokumente",
            "driveType": "documentLibrary",
            "webUrl": "https://contoso.sharepoint.com/sites/projekt-nord/Dokumente",
        }),
        json!({
            "id": DRIVE_FOTOS,
            "name": "Fotos",
            "driveType": "documentLibrary",
        }),
    ]
}

fn listen() -> Vec<Value> {
    vec![
        json!({
            "id": LISTE_AUFGABEN,
            "name": "Aufgaben",
            "displayName": "Aufgaben",
            "description": "Was zu tun ist",
            "webUrl": "https://contoso.sharepoint.com/sites/projekt-nord/Lists/Aufgaben",
            "list": {"template": "genericList"},
        }),
        json!({
            "id": "22222222-2222-2222-2222-222222222222",
            "name": "Kontakte",
            "displayName": "Kontakte",
            "list": {"template": "contacts"},
        }),
    ]
}

fn listeneintraege() -> Vec<Value> {
    vec![
        json!({
            "id": "1",
            "webUrl": "https://contoso.sharepoint.com/sites/projekt-nord/Lists/Aufgaben/1",
            "lastModifiedDateTime": "2026-11-27T09:00:00Z",
            "fields": {"Titel": "Erster", "Status": "offen"},
        }),
        json!({
            "id": "2",
            "webUrl": "https://contoso.sharepoint.com/sites/projekt-nord/Lists/Aufgaben/2",
            "lastModifiedDateTime": "2026-11-26T09:00:00Z",
            "fields": {"Titel": "Zweiter", "Status": "fertig"},
        }),
    ]
}

fn kinder() -> Vec<Value> {
    vec![
        json!({
            "id": ITEM_ORDNER,
            "name": "Projekte",
            "folder": {"childCount": 2},
            "webUrl": "https://contoso.sharepoint.com/Projekte",
            "lastModifiedDateTime": "2026-11-25T08:00:00Z",
            "parentReference": {"path": "/drive/root:"},
        }),
        json!({
            "id": ITEM_TEXT,
            "name": "notiz.md",
            "size": 26,
            "file": {"mimeType": "text/markdown"},
            "webUrl": "https://contoso.sharepoint.com/notiz.md",
            "lastModifiedDateTime": "2026-11-20T08:00:00Z",
            "parentReference": {"path": "/drive/root:/Projekte"},
        }),
        json!({
            "id": ITEM_OFFICE,
            "name": "bericht.docx",
            "size": 1234,
            "file": {"mimeType": "application/vnd.openxmlformats-officedocument.wordprocessingml.document"},
            "webUrl": "https://contoso.sharepoint.com/bericht.docx",
        }),
        json!({
            "id": ITEM_PDF,
            "name": "anleitung.pdf",
            "size": 4321,
            "file": {"mimeType": "application/pdf"},
            "webUrl": "https://contoso.sharepoint.com/anleitung.pdf",
        }),
    ]
}

/// Ein Eintrag aus `…/items/{id}` (ohne `/content`).
fn item_aus_id(pfad: &str) -> Value {
    let id = pfad
        .split("/items/")
        .nth(1)
        .unwrap_or_default()
        .split('/')
        .next()
        .unwrap_or_default();
    kinder()
        .into_iter()
        .find(|eintrag| eintrag["id"] == id)
        .unwrap_or_else(|| kinder()[0].clone())
}

/// Ein Eintrag aus `…/root:/{pfad}` (ohne `/content`, ohne `/children`).
fn item_aus_name(pfad: &str) -> Value {
    let name = pfad
        .split("/root:/")
        .nth(1)
        .unwrap_or_default()
        .trim_matches(':');
    kinder()
        .into_iter()
        .find(|eintrag| eintrag["name"] == name)
        .unwrap_or_else(|| kinder()[0].clone())
}

/// Der Dateiname hinter `…/content` (aus dem Upload-Pfad).
fn name_aus_pfad(pfad: &str) -> String {
    let ohne = pfad.strip_suffix("/content").unwrap_or(pfad);
    ohne.rsplit(":/")
        .next()
        .unwrap_or(ohne)
        .rsplit('/')
        .next()
        .unwrap_or(ohne)
        .trim_matches(':')
        .to_string()
}

/// Die Id eines Laufwerkseintrags aus `…/items/{id}/content`.
fn item_aus_pfad(pfad: &str) -> &str {
    pfad.split("/items/")
        .nth(1)
        .unwrap_or_default()
        .trim_end_matches("/content")
}

/// Der Text einer Textdatei (für `/download/…` und `/content`).
fn textdatei(name: &str) -> (&'static str, &'static str) {
    if name.ends_with(".md") {
        ("text/markdown; charset=utf-8", NOTIZ_INHALT)
    } else {
        ("text/plain; charset=utf-8", NOTIZ_INHALT)
    }
}

/// Dateiinhalt: Textdatei über eine Weiterleitung (wie Graph), Office-Datei als
/// PDF (`?format=pdf`).
fn inhalt(pfad: &str, abfrage: &[(String, String)], host: &str) -> Response {
    let id = item_aus_pfad(pfad);
    let als_pdf = abfrage
        .iter()
        .any(|(name, wert)| name == "format" && wert == "pdf");
    if als_pdf {
        return (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/pdf")],
            "%PDF-1.7\n(Attrappe)".to_string(),
        )
            .into_response();
    }
    let name = match id {
        ITEM_TEXT => "notiz.md",
        ITEM_OFFICE => "bericht.docx",
        ITEM_PDF => "anleitung.pdf",
        _ => "unbekannt.txt",
    };
    (
        StatusCode::FOUND,
        [(header::LOCATION, format!("http://{host}/download/{name}"))],
    )
        .into_response()
}

fn plaene() -> Vec<Value> {
    vec![
        json!({
            "id": PLAN_NORD,
            "title": "Projekt Nord",
            "owner": "group-1",
            "createdDateTime": "2026-11-01T08:00:00Z",
        }),
        json!({
            "id": PLAN_SUED,
            "title": "Projekt Süd",
            "owner": "group-1",
            "createdDateTime": "2026-11-02T08:00:00Z",
        }),
    ]
}

fn buckets() -> Vec<Value> {
    vec![
        json!({"id": BUCKET_OFFEN, "name": "Zu erledigen", "planId": PLAN_NORD}),
        json!({"id": BUCKET_FERTIG, "name": "Erledigt", "planId": PLAN_NORD}),
    ]
}

fn planner_aufgaben() -> Vec<Value> {
    vec![
        planner_aufgabe(AUFGABE_MIT_ETAG),
        planner_aufgabe("task-2"),
        planner_aufgabe(AUFGABE_OHNE_ETAG),
    ]
}

/// Eine Planner-Aufgabe. `task-ohne-etag` liefert bewusst **kein** ETag – dann
/// darf kein PATCH entstehen.
fn planner_aufgabe(id: &str) -> Value {
    let (titel, bucket, fortschritt) = match id {
        "task-2" => ("Rechnung prüfen", BUCKET_FERTIG, 0),
        AUFGABE_OHNE_ETAG => ("Ohne ETag", BUCKET_OFFEN, 0),
        _ => ("Angebot schreiben", BUCKET_OFFEN, 50),
    };
    let mut aufgabe = json!({
        "id": id,
        "planId": PLAN_NORD,
        "bucketId": bucket,
        "title": titel,
        "percentComplete": fortschritt,
        "priority": 5,
        "dueDateTime": "2026-11-30T00:00:00Z",
        "createdDateTime": "2026-11-10T08:00:00Z",
        "assignments": {},
    });
    if id != AUFGABE_OHNE_ETAG {
        aufgabe["@odata.etag"] = json!(format!("W/\"{id}\""));
    }
    aufgabe
}

fn todo_listen() -> Vec<Value> {
    vec![
        json!({"id": TODO_LISTE, "displayName": "Aufgaben", "wellknownListName": "defaultList"}),
        json!({"id": TODO_LISTE_EINKAUF, "displayName": "Einkauf"}),
    ]
}

/// Eine Seite To-Do-Listen: Seite 1 mit `@odata.nextLink`, Seite 2 ohne –
/// `/me/todo/lists` verträgt weder `$top` noch `$select`, geblättert wird über
/// den Link (Aufgabe 07).
fn todo_listen_seite(abfrage: &[(String, String)], host: &str) -> Value {
    let seite: u32 = abfrage
        .iter()
        .find(|(name, _)| name == "seite")
        .and_then(|(_, wert)| wert.parse().ok())
        .unwrap_or(1);

    if seite == 1 {
        json!({
            "value": [todo_listen()[0].clone()],
            "@odata.nextLink": format!("http://{host}/v1.0/me/todo/lists?seite=2"),
        })
    } else {
        json!({"value": [todo_listen()[1].clone()]})
    }
}

fn todo_aufgaben() -> Vec<Value> {
    vec![todo_aufgabe(TODO_AUFGABE), {
        let mut fertig = todo_aufgabe("AAMkADtodo-task-2");
        fertig["title"] = json!("Bericht senden");
        fertig["status"] = json!("completed");
        fertig["completedDateTime"] = json!({
            "dateTime": "2026-11-27T09:00:00.0000000",
            "timeZone": "Europe/Berlin",
        });
        fertig
    }]
}

fn todo_aufgabe(id: &str) -> Value {
    json!({
        "id": id,
        "title": "Rechnung prüfen",
        "status": "notStarted",
        "importance": "high",
        "dueDateTime": {"dateTime": "2026-11-30T00:00:00.0000000", "timeZone": "Europe/Berlin"},
        "createdDateTime": "2026-11-10T08:00:00Z",
        "body": {"contentType": "text", "content": "Bis Freitag"},
    })
}

fn kontakte() -> Vec<Value> {
    vec![json!({
        "id": "k1",
        "displayName": "Bert Beispiel",
        "givenName": "Bert",
        "surname": "Beispiel",
        "companyName": "Beispiel GmbH",
        "jobTitle": "Einkauf",
        "emailAddresses": [{"name": "Bert", "address": "bert@firma.de"}],
        "businessPhones": ["+49 30 1234"],
        "mobilePhone": "+49 170 1234567",
    })]
}

fn personen_von_people() -> Vec<Value> {
    vec![
        json!({
            "id": "u-bert",
            "displayName": "Bert Beispiel",
            "department": "Vertrieb",
            "jobTitle": "Key Account",
            "companyName": "Beispiel GmbH",
            "scoredEmailAddresses": [{"address": "bert@firma.de"}],
            "phones": [{"number": "+49 30 1234", "type": "business"}],
        }),
        json!({
            "id": "u-clara",
            "displayName": "Clara Claudia",
            "department": "Entwicklung",
            "scoredEmailAddresses": [{"address": "clara@firma.de"}],
        }),
    ]
}

fn notizbuecher() -> Vec<Value> {
    vec![
        // Kein `webUrl` bei OneNote-Notizbüchern – der Link steht in `links`
        // (Aufgabe 07).
        json!({
            "id": NOTIZBUCH,
            "displayName": "Arbeit",
            "links": {"oneNoteWebUrl": {"href": "https://example/onenote/arbeit"}},
        }),
        json!({
            "id": "1-nb-privat-0000000002",
            "displayName": "Privat",
        }),
    ]
}

fn abschnitte() -> Vec<Value> {
    vec![
        json!({
            "id": ABSCHNITT,
            "displayName": "Besprechungen",
            "links": {"oneNoteWebUrl": {"href": "https://example/onenote/besprechungen"}},
        }),
        json!({
            "id": "1-sec-ideen-0000000002",
            "displayName": "Ideen",
        }),
    ]
}

fn onenote_seiten() -> Vec<Value> {
    vec![
        // Kein `webUrl` bei OneNote-Seiten – der Link steht in `links`
        // (Aufgabe 08).
        json!({
            "id": ONENOTE_SEITE,
            "title": "Jour fixe",
            "links": {"oneNoteWebUrl": {"href": "https://example/onenote/jour-fixe"}},
            "lastModifiedDateTime": "2026-11-27T09:00:00Z",
        }),
        json!({
            "id": "1-page-notizen-0000000002",
            "title": "Notizen",
        }),
    ]
}

fn onenote_seite(id: &str) -> Value {
    json!({
        "id": id,
        "title": "Jour fixe",
        "links": {"oneNoteWebUrl": {"href": "https://example/onenote/jour-fixe"}},
        "createdDateTime": "2026-11-27T08:00:00Z",
        "lastModifiedDateTime": "2026-11-27T09:00:00Z",
    })
}

/// Der Text zwischen zwei Markierungen (für den Titel einer neuen Seite).
fn zwischen(text: &str, von: &str, bis: &str) -> Option<String> {
    let rest = text.split_once(von)?.1;
    let ende = rest.split_once(bis)?.0;
    Some(ende.to_string())
}

// ---------------------------------------------------------------------------
// Teams-Daten. Zeiten sind relativ zu „jetzt“, damit `teams_eingang` (letzte
// `tage`) unabhängig vom Kalender des Testrechners prüfbar ist.
// ---------------------------------------------------------------------------

/// Ids und Namen, die die Tests erwarten.
pub const TEAM_ENTWICKLUNG: &str = "team-1";
pub const KANAL_ALLGEMEIN: &str = "19:kanal-allgemein@thread.tacv2";
pub const CHAT_PROJEKT: &str = "19:chat-projekt@thread.v2";
pub const CHAT_MITTAG: &str = "19:chat-mittag@thread.v2";
pub const CHAT_REISE: &str = "19:chat-reise@thread.v2";

fn vor_stunden(stunden: i64) -> String {
    let sekunden = jiff::Timestamp::now().as_second() - stunden * 3_600;
    jiff::Timestamp::from_second(sekunden)
        .expect("Zeitpunkt")
        .to_string()
}

fn person_anna() -> Value {
    json!({"displayName": "Anna Muster", "email": "anna@firma.de", "userId": "u-anna"})
}

fn person_bert() -> Value {
    json!({"displayName": "Bert Beispiel", "email": "bert@firma.de", "userId": "u-bert"})
}

fn person_clara() -> Value {
    json!({"displayName": "Clara Claudia", "email": "clara@firma.de", "userId": "u-clara"})
}

fn chats() -> Value {
    json!({
        "value": [
            {
                "id": CHAT_PROJEKT,
                "topic": "Projekt Nord",
                "chatType": "group",
                "members": [person_anna(), person_bert()],
                "lastMessagePreview": {
                    "id": "c1",
                    "createdDateTime": vor_stunden(1),
                    "body": {"contentType": "html", "content": "<p>Kurze Frage <b>zum Plan</b></p>"},
                    "from": {"user": {"id": "u-bert", "displayName": "Bert Beispiel"}},
                },
                "viewpoint": {"lastMessageReadDateTime": vor_stunden(2)},
            },
            {
                "id": CHAT_MITTAG,
                "topic": "Mittagessen",
                "chatType": "group",
                "members": [person_anna(), person_clara()],
                "lastMessagePreview": {
                    "id": "c2",
                    "createdDateTime": vor_stunden(3),
                    "body": {"contentType": "html", "content": "<p>Bis gleich</p>"},
                    "from": {"user": {"id": "u-anna", "displayName": "Anna Muster"}},
                },
                "viewpoint": {"lastMessageReadDateTime": vor_stunden(1)},
            },
            {
                "id": CHAT_REISE,
                "topic": "Reisekosten",
                "chatType": "oneOnOne",
                "members": [person_anna(), person_bert()],
                "lastMessagePreview": {
                    "id": "c3",
                    "createdDateTime": vor_stunden(5),
                    "body": {"contentType": "html", "content": "<p>Beleg fehlt noch</p>"},
                    "from": {"user": {"id": "u-bert", "displayName": "Bert Beispiel"}},
                },
            }
        ]
    })
}

fn kanaele() -> Value {
    json!([
        {
            "id": KANAL_ALLGEMEIN,
            "displayName": "Allgemein",
            "description": "Alles zum Team",
            "membershipType": "standard",
        },
        {
            "id": "19:kanal-zufall@thread.tacv2",
            "displayName": "Zufall",
            "membershipType": "standard",
        }
    ])
}

fn team_mitglieder() -> Value {
    json!([person_anna(), person_bert()])
}

fn kanal_mitglieder() -> Value {
    json!([person_anna(), person_bert(), person_clara()])
}

fn personen() -> Value {
    json!([
        {"id": "u-bert", "displayName": "Bert Beispiel", "mail": "bert@firma.de", "userPrincipalName": "bert@firma.de"},
        {"id": "u-clara", "displayName": "Clara Claudia", "mail": "clara@firma.de", "userPrincipalName": "clara@firma.de"},
        // Zwei Annas: „Anna“ ist mehrdeutig, „anna@firma.de“ nicht.
        {"id": "u-anna", "displayName": "Anna Muster", "mail": "anna@firma.de", "userPrincipalName": "anna@firma.de"},
        {"id": "u-anna2", "displayName": "Anna Anders", "mail": "anna.anders@firma.de", "userPrincipalName": "anna.anders@firma.de"},
    ])
}

/// Ein Kanalbeitrag mit HTML, Absender, Anhang und Web-Link.
fn beitrag_m1() -> Value {
    json!({
        "id": "m1",
        "messageType": "message",
        "createdDateTime": vor_stunden(2),
        "subject": "Status",
        "webUrl": "https://teams.example/m1",
        "from": {"user": {"id": "u-bert", "displayName": "Bert Beispiel"}},
        "body": {"contentType": "html", "content": "<p>Status <b>grün</b></p>"},
        "attachments": [{"id": "a1", "contentType": "reference", "name": "Anhang.pdf"}],
    })
}

fn beitrag_m3() -> Value {
    let lang = (1..=120)
        .map(|nummer| format!("Wort{nummer}"))
        .collect::<Vec<String>>()
        .join(" ");
    json!({
        "id": "m3",
        "messageType": "message",
        "createdDateTime": vor_stunden(4),
        "from": {"user": {"id": "u-clara", "displayName": "Clara Claudia"}},
        "body": {"contentType": "html", "content": format!("<p>{lang}</p>")},
    })
}

fn system_nachricht() -> Value {
    json!({
        "id": "m2",
        "messageType": "systemEventMessage",
        "createdDateTime": vor_stunden(3),
        "body": {"contentType": "html", "content": "Bert ist dem Team beigetreten"},
    })
}

fn geloeschte_nachricht() -> Value {
    json!({
        "id": "m4",
        "messageType": "message",
        "createdDateTime": vor_stunden(5),
        "deletedDateTime": vor_stunden(1),
        "body": {"contentType": "html", "content": "weg"},
    })
}

fn antworten(pfad: &str) -> Vec<Value> {
    if pfad.contains("/messages/m3/") {
        return vec![json!({
            "id": "r3",
            "messageType": "message",
            "createdDateTime": vor_stunden(1),
            "from": {"user": {"id": "u-bert", "displayName": "Bert Beispiel"}},
            "body": {"contentType": "html", "content": "<p>Danke</p>"},
        })];
    }
    vec![
        json!({
            "id": "r1",
            "messageType": "message",
            "replyToId": "m1",
            "createdDateTime": vor_stunden(1),
            "from": {"user": {"id": "u-clara", "displayName": "Clara Claudia"}},
            "body": {"contentType": "html", "content": "<p>Alles gut.</p>"},
        }),
        // System-Antworten gehören nicht in die Antwort.
        json!({
            "id": "r2",
            "messageType": "systemEventMessage",
            "replyToId": "m1",
            "createdDateTime": vor_stunden(1),
            "body": {"contentType": "html", "content": "Clara hat reagiert"},
        }),
    ]
}

fn nachricht(pfad: &str) -> Value {
    let id = pfad.rsplit("/messages/").next().unwrap_or("m1");
    match id {
        "m3" => beitrag_m3(),
        _ => beitrag_m1(),
    }
}

/// Eine Seite Kanalbeiträge. Seite 1 hat zwei Beiträge und einen `nextLink`,
/// Seite 2 die übrigen – so prüfen die Tests, dass nicht mehr Seiten als nötig
/// geholt werden.
fn kanal_seite(pfad: &str, abfrage: &[(String, String)], host: &str) -> Value {
    let seite: u32 = abfrage
        .iter()
        .find(|(name, _)| name == "seite")
        .and_then(|(_, wert)| wert.parse().ok())
        .unwrap_or(1);

    if seite == 1 {
        json!({
            "value": [beitrag_m1(), system_nachricht()],
            "@odata.nextLink": format!("http://{host}/v1.0{pfad}?seite=2"),
        })
    } else {
        json!({"value": [beitrag_m3(), geloeschte_nachricht()]})
    }
}

fn chat_nachrichten() -> Vec<Value> {
    vec![
        json!({
            "id": "n1",
            "messageType": "message",
            "createdDateTime": vor_stunden(1),
            "from": {"user": {"id": "u-bert", "displayName": "Bert Beispiel"}},
            "body": {"contentType": "html", "content": "<p>Kurze Frage <b>zum Plan</b></p>"},
        }),
        json!({
            "id": "n2",
            "messageType": "systemEventMessage",
            "createdDateTime": vor_stunden(2),
            "body": {"contentType": "html", "content": "Chat erstellt"},
        }),
    ]
}

fn gesendete_nachricht(id: &str) -> Value {
    json!({
        "id": id,
        "messageType": "message",
        "createdDateTime": vor_stunden(0),
        "body": {"contentType": "html", "content": ""},
        "webUrl": format!("https://teams.example/{id}"),
    })
}

fn chat_aus(body: &Option<Value>) -> Value {
    let body = body.clone().unwrap_or(Value::Null);
    json!({
        "id": "19:chat-neu@thread.v2",
        "chatType": body.get("chatType").cloned().unwrap_or(Value::Null),
        "topic": body.get("topic").cloned().unwrap_or(Value::Null),
        "members": body.get("members").cloned().unwrap_or(Value::Null),
    })
}

fn suchergebnis() -> Value {
    json!({
        "value": [{
            "hitsContainers": [{
                "hits": [
                    {
                        "hitId": "h1",
                        "summary": "Ein <c0>Plan</c0>",
                        "resource": {
                            "id": "s1",
                            "messageType": "message",
                            "createdDateTime": vor_stunden(2),
                            "body": {"contentType": "html", "content": "<p>Der <b>Plan</b> steht</p>"},
                            "from": {"user": {"id": "u-bert", "displayName": "Bert Beispiel"}},
                            "chatId": CHAT_PROJEKT,
                            "mentions": [{
                                "id": 0,
                                "mentionText": "Anna Muster",
                                "mentioned": {"user": {"id": "u-anna", "displayName": "Anna Muster"}},
                            }],
                        },
                    },
                    {
                        "hitId": "h2",
                        "summary": "Ein alter Treffer",
                        "resource": {
                            "id": "s2",
                            "messageType": "message",
                            "createdDateTime": vor_stunden(10 * 24),
                            "body": {"contentType": "html", "content": "<p>Alt</p>"},
                            "from": {"user": {"id": "u-clara", "displayName": "Clara Claudia"}},
                            "channelIdentity": {"teamId": TEAM_ENTWICKLUNG, "channelId": KANAL_ALLGEMEIN},
                            "mentions": [{
                                "id": 0,
                                "mentionText": "Anna Muster",
                                "mentioned": {"user": {"id": "u-anna", "displayName": "Anna Muster"}},
                            }],
                        },
                    },
                    {
                        "hitId": "h3",
                        "summary": "Ohne Erwähnung",
                        "resource": {
                            "id": "s3",
                            "messageType": "message",
                            "createdDateTime": vor_stunden(1),
                            "body": {"contentType": "html", "content": "<p>Nur so</p>"},
                            "from": {"user": {"id": "u-clara", "displayName": "Clara Claudia"}},
                            "chatId": CHAT_PROJEKT,
                        },
                    },
                ],
                "total": 3,
            }],
        }],
    })
}
