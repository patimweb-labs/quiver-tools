// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Attrappe für Microsoft Graph (SPEC.md §6): `/me`, `/me/messages`, `/me/sendMail`,
//! `/me/calendarView`, `/me/events`, die Dateisuche – und ein Protokoll aller
//! Anfragen, damit die Tests Pfad, Query, Kopfzeilen und Körper prüfen können.
//!
//! Nie echte Microsoft-Server.

#![allow(dead_code)]

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

async fn dienen(State(zustand): State<Arc<Zustand>>, request: Request) -> Response {
    let methode = request.method().to_string();
    let pfad = request.uri().path().to_string();
    let abfrage = felder(request.uri().query().unwrap_or_default());
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

    let body = if methode == "POST" {
        let bytes = to_bytes(request.into_body(), 1 << 20)
            .await
            .expect("Body lesbar");
        serde_json::from_slice::<Value>(&bytes).ok()
    } else {
        None
    };

    fach(&zustand.aufzeichnungen).push(Aufzeichnung {
        methode: methode.clone(),
        pfad: pfad.clone(),
        abfrage,
        kopfzeilen,
        body: body.clone(),
    });

    if let Some(antwort) = fach(&zustand.einmal).take() {
        return antwort.als_response();
    }
    if let Some(antwort) = fach(&zustand.dauerhaft).clone() {
        return antwort.als_response();
    }

    let ohne_basis = pfad.strip_prefix("/v1.0").unwrap_or(&pfad);
    let antwort = match (methode.as_str(), ohne_basis) {
        // Graph antwortet auf `sendMail` mit 202 ohne Körper.
        ("POST", "/me/sendMail") => return StatusCode::ACCEPTED.into_response(),
        ("POST", "/me/events") => ereignis_aus(&body),
        ("GET", "/me") => json!({
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
