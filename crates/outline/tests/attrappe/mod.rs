// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Outline-Attrappe (SPEC §7): ein kleines axum-Programm, das die
//! benutzten Endpunkte beantwortet und jeden Aufruf mitschreibt.
//!
//! Kein Test spricht je mit einem echten Outline. Der Schlüssel ist erfunden
//! und steht bewusst in keiner Antwort – außer in der 401-Antwort von
//! `/kaputt`, die prüft, dass die Attrappe ihn zurückspiegeln darf und das
//! Werkzeug ihn trotzdem nicht weitergibt.

#![allow(dead_code)]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex, MutexGuard};

use axum::body::Bytes;
use axum::extract::{OriginalUri, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use quiver_outline::Outline;
use quiver_tools_kern::Zugang;
use serde_json::{json, Value};

/// Der Schlüssel, den die Attrappe erwartet. Kein echtes Geheimnis.
pub const SCHLUESSEL: &str = "ol_api_attrappe_4711_kein_test_token";

/// Ein Aufruf, den die Attrappe gesehen hat.
#[derive(Debug, Clone)]
pub struct Aufruf {
    /// Pfad, z. B. `/api/documents.info` (mitsamt Zweig, falls einer davor steht).
    pub pfad: String,
    /// Methode, z. B. `documents.info`.
    pub methode: String,
    /// Der JSON-Rumpf, den das Werkzeug geschickt hat.
    pub rumpf: Value,
    /// Der `Authorization`-Kopf, damit Tests die Anmeldung prüfen können.
    pub anmeldung: Option<String>,
}

impl Aufruf {
    /// Der Wert eines Feldes im Rumpf.
    pub fn feld(&self, name: &str) -> Value {
        self.rumpf.get(name).cloned().unwrap_or(Value::Null)
    }
}

#[derive(Default)]
pub struct Aufzeichnung {
    aufrufe: Mutex<Vec<Aufruf>>,
}

fn griff<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|gift| gift.into_inner())
}

impl Aufzeichnung {
    pub fn alle(&self) -> Vec<Aufruf> {
        griff(&self.aufrufe).clone()
    }

    pub fn gemerkt(&self, methode: &str) -> Option<Aufruf> {
        self.alle()
            .into_iter()
            .find(|aufruf| aufruf.methode == methode)
    }

    pub fn anzahl(&self, methode: &str) -> usize {
        self.alle()
            .iter()
            .filter(|aufruf| aufruf.methode == methode)
            .count()
    }
}

/// Eine laufende Attrappe.
pub struct Attrappe {
    /// `http://127.0.0.1:<port>` – ohne Schrägstrich am Ende.
    pub adresse: String,
    pub aufzeichnung: Arc<Aufzeichnung>,
    ende: tokio::task::JoinHandle<()>,
}

impl Drop for Attrappe {
    fn drop(&mut self) {
        self.ende.abort();
    }
}

impl Attrappe {
    /// Die Attrappe auf einem freien Port starten.
    pub async fn starte() -> Attrappe {
        let aufzeichnung = Arc::new(Aufzeichnung::default());
        let zustand = aufzeichnung.clone();
        let app = Router::new()
            .route("/api/{methode}", post(api))
            .route("/zweig/api/{methode}", post(api))
            .route("/kaputt/api/{methode}", post(kaputt))
            .route("/abgelehnt/api/{methode}", post(abgelehnt))
            .fallback(|| async {
                (
                    StatusCode::NOT_FOUND,
                    [("content-type", "text/html; charset=utf-8")],
                    "<html><body>kein Outline</body></html>",
                )
            })
            .with_state(zustand);

        let listener =
            tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
                .await
                .unwrap();
        let adresse = format!("http://{}", listener.local_addr().unwrap());
        let ende = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Attrappe {
            adresse,
            aufzeichnung,
            ende,
        }
    }
}

async fn api(
    State(aufzeichnung): State<Arc<Aufzeichnung>>,
    OriginalUri(uri): OriginalUri,
    Path(methode): Path<String>,
    kopf: HeaderMap,
    koerper: Bytes,
) -> Response {
    let anmeldung = kopf
        .get("authorization")
        .and_then(|wert| wert.to_str().ok())
        .map(str::to_string);

    if anmeldung.as_deref() != Some(&format!("Bearer {SCHLUESSEL}")) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "ok": false, "error": "unauthorized" })),
        )
            .into_response();
    }

    let rumpf: Value = serde_json::from_slice(&koerper).unwrap_or(Value::Null);
    griff(&aufzeichnung.aufrufe).push(Aufruf {
        pfad: uri.path().to_string(),
        methode: methode.clone(),
        rumpf: rumpf.clone(),
        anmeldung,
    });

    match antwort(&methode, &rumpf) {
        Some(daten) => Json(daten).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "ok": false, "error": "not_found", "message": "Methode unbekannt" })),
        )
            .into_response(),
    }
}

/// Der Fehlerfall: Outline verweigert den Zugriff und spiegelt den Kopf zurück.
async fn kaputt(Path(methode): Path<String>, kopf: HeaderMap) -> Response {
    let anmeldung = kopf
        .get("authorization")
        .and_then(|wert| wert.to_str().ok())
        .unwrap_or("")
        .to_string();
    (
        StatusCode::FORBIDDEN,
        Json(json!({
            "ok": false,
            "error": "permission_denied",
            "message": format!("Der Schlüssel in „{anmeldung}“ darf das nicht (Aufruf {methode})")
        })),
    )
        .into_response()
}

/// Derselbe Fehler mit 401: der Schlüssel wird abgelehnt.
async fn abgelehnt(Path(methode): Path<String>, kopf: HeaderMap) -> Response {
    let anmeldung = kopf
        .get("authorization")
        .and_then(|wert| wert.to_str().ok())
        .unwrap_or("")
        .to_string();
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({
            "ok": false,
            "error": "unauthorized",
            "message": format!("„{anmeldung}“ ist kein gültiger Schlüssel (Aufruf {methode})")
        })),
    )
        .into_response()
}

/// Was die Attrappe auf eine Methode antwortet.
fn antwort(methode: &str, rumpf: &Value) -> Option<Value> {
    let id = rumpf
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("d1")
        .to_string();
    let titel = rumpf
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Betriebshandbuch")
        .to_string();

    let dokument = |id: &str, titel: &str| {
        json!({
            "id": id,
            "urlId": format!("{id}-slug"),
            "title": titel,
            "url": format!("https://wiki.example.test/doc/{id}"),
            "collectionId": "c1",
            "parentDocumentId": null,
            "updatedAt": "2026-09-02T10:11:12.000Z",
            "publishedAt": "2026-09-01T08:00:00.000Z",
            "updatedBy": { "name": "Ada", "email": "ada@example.test" }
        })
    };

    let daten = match methode {
        "auth.info" => json!({
            "ok": true,
            "data": {
                "user": { "name": "Ada", "email": "ada@example.test" },
                "team": { "name": "Firma", "url": "https://firma.getoutline.com" }
            }
        }),
        "collections.list" => json!({
            "ok": true,
            "data": [
                { "id": "c1", "name": "Technik", "description": "Alles Technische" },
                { "id": "c2", "name": "Kunde A", "description": null }
            ],
            "pagination": { "offset": 0, "limit": 50, "total": 2 }
        }),
        "collections.documents" => json!({
            "ok": true,
            "data": [
                {
                    "id": "d1",
                    "title": "Betriebshandbuch",
                    "url": "https://wiki.example.test/doc/d1",
                    "children": [
                        { "id": "d2", "title": "Notfall", "children": [] }
                    ]
                }
            ]
        }),
        "documents.search" => json!({
            "ok": true,
            "data": [{
                "ranking": 0.91,
                "context": "… hier steht der Treffer …",
                "document": dokument("d1", "Betriebshandbuch")
            }],
            "pagination": { "offset": 0, "limit": 15, "total": 1 }
        }),
        "documents.list" => json!({
            "ok": true,
            "data": [
                dokument("d1", "Betriebshandbuch"),
                dokument("d2", "Notfall")
            ],
            "pagination": { "offset": 0, "limit": 25, "total": 2 }
        }),
        "documents.drafts" => json!({
            "ok": true,
            "data": [{
                "id": "d3",
                "title": "Entwurf",
                "collectionId": null,
                "createdAt": "2026-09-03T09:00:00.000Z"
            }]
        }),
        "documents.info" => {
            let mut seite = dokument(&id, "Betriebshandbuch");
            seite["text"] = json!("Zeile eins\nZeile zwei mit mehr Text\nZeile drei");
            json!({ "ok": true, "data": seite })
        }
        "documents.create" => {
            let mut seite = dokument("neu-1", &titel);
            seite["text"] = json!(rumpf.get("text").cloned().unwrap_or(Value::Null));
            json!({ "ok": true, "data": seite })
        }
        "documents.update" => {
            let mut seite = dokument(&id, &titel);
            seite["text"] = json!(rumpf.get("text").cloned().unwrap_or(Value::Null));
            json!({ "ok": true, "data": seite })
        }
        "documents.move" => json!({
            "ok": true,
            "data": { "documents": [{ "id": id }, { "id": "d2" }] }
        }),
        "documents.archive" | "documents.restore" | "documents.unpublish" => {
            json!({ "ok": true, "data": dokument(&id, "Betriebshandbuch") })
        }
        "documents.delete" => json!({ "ok": true, "data": dokument(&id, "Betriebshandbuch") }),
        "documents.export" => json!({
            "ok": true,
            "data": format!("# Betriebshandbuch {id}\n\nMarkdown aus der Attrappe.\n")
        }),
        "comments.list" => json!({
            "ok": true,
            "data": [
                {
                    "id": "k1",
                    "text": "Ist das noch aktuell?",
                    "createdAt": "2026-09-02T10:11:12.000Z",
                    "createdBy": { "name": "Ada" }
                },
                {
                    "id": "k2",
                    "parentCommentId": "k1",
                    "data": {
                        "type": "doc",
                        "content": [{
                            "type": "paragraph",
                            "content": [{ "type": "text", "text": "Ja, seit gestern." }]
                        }]
                    },
                    "createdBy": { "name": "Bob" }
                }
            ]
        }),
        "comments.create" => json!({
            "ok": true,
            "data": {
                "id": "k9",
                "documentId": rumpf.get("documentId").cloned().unwrap_or(Value::Null),
                "text": rumpf.get("text").cloned().unwrap_or(Value::Null),
                "createdBy": { "name": "Ada" }
            }
        }),
        _ => return None,
    };
    Some(daten)
}

/// Der Zugang, den quiver für dieses Tool setzen würde: `OUTLINE_URL` und
/// `OUTLINE_API_KEY`. Keine Tresor-Datei, keine Umgebungsvariablen im Prozess.
pub fn zugang(adresse: &str) -> Zugang {
    Zugang::mit_umgebung(&Outline, |name| match name {
        "OUTLINE_URL" => Some(adresse.to_string()),
        "OUTLINE_API_KEY" => Some(SCHLUESSEL.to_string()),
        // Die Tests prüfen die Werkzeuge, nicht den Schalter: Schreiben ist an.
        "OUTLINE_SCHREIBEN" => Some("an".to_string()),
        _ => None,
    })
}

/// Ein Zugang ohne Schreibschalter – so wie nach dem Update.
pub fn zugang_ohne_schreiben(adresse: &str) -> Zugang {
    Zugang::mit_umgebung(&Outline, |name| match name {
        "OUTLINE_URL" => Some(adresse.to_string()),
        "OUTLINE_API_KEY" => Some(SCHLUESSEL.to_string()),
        _ => None,
    })
}

/// Ein Zugang mit Arbeitsordner (für `outline_export`).
pub fn zugang_mit_ordner(adresse: &str, ordner: &str) -> Zugang {
    Zugang::mit_umgebung(&Outline, |name| match name {
        "OUTLINE_URL" => Some(adresse.to_string()),
        "OUTLINE_API_KEY" => Some(SCHLUESSEL.to_string()),
        "OUTLINE_SCHREIBEN" => Some("an".to_string()),
        "OUTLINE_ORDNER" => Some(ordner.to_string()),
        _ => None,
    })
}

/// Ein Zugang, dem nichts gesetzt ist – wie vor dem Eintragen im quiver-Fenster.
pub fn ohne_zugang() -> Zugang {
    Zugang::mit_umgebung(&Outline, |_| None)
}
