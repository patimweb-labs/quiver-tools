// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Jira-Attrappe (SPEC §7, Aufgabe 12): ein kleines axum-Programm, das die
//! benutzten Endpunkte einer Site beantwortet und jeden Aufruf mitschreibt.
//!
//! Kein Test spricht je mit einem echten Jira. Die Attrappe ist **streng**:
//!
//! - unbekannte Pfade und unbekannte Abfrageparameter → `400` mit einer
//!   Jira-Fehlermeldung,
//! - ein falscher `Authorization`-Kopf → `401`,
//! - Pflichtfelder im Rumpf werden geprüft (`400` mit `errors`),
//! - Antworten haben die Form der echten API (`values`, `issues`, `comments`,
//!   `transitions`, `issueLinkTypes`, ADF für Texte).
//!
//! Sie kann außerdem `403` melden, `429` mit `Retry-After`, auf einen fremden
//! Host weiterleiten und eine Folgeseite mit fremdem Host liefern – damit die
//! Sicherheitsregeln einzeln prüfbar sind. Alle Namen, Nummern und Token sind
//! erfunden.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Router;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde_json::{json, Value};

/// E-Mail und Token der ersten Site. Kein echtes Geheimnis.
pub const EMAIL_A: &str = "patrick@example.test";
pub const TOKEN_A: &str = "token-attrappe-4711";
/// E-Mail und Token der zweiten Site.
pub const EMAIL_B: &str = "anna@example.test";
pub const TOKEN_B: &str = "token-attrappe-4712";

/// Anzeigename und Konto des angemeldeten Benutzers.
pub const BENUTZER: &str = "Patrick Weppelmann";
pub const BENUTZER_ID: &str = "acc-123";

/// Die Projekte der Attrappe.
pub const PROJEKT: &str = "ABC";
pub const PROJEKT_NAME: &str = "Alpha";
pub const PROJEKT_FREMD: &str = "XYZ";

/// Die Vorgänge der Attrappe.
pub const VORGANG: &str = "ABC-1";
pub const VORGANG_2: &str = "ABC-2";
pub const VORGANG_FREMD: &str = "XYZ-1";
pub const UNTERAUFGABE: &str = "ABC-4";
pub const ELTERN: &str = "ABC-0";

/// Vorgangstyp der Attrappe.
pub const TYP: &str = "Bug";
pub const TYP_ID: &str = "10001";
/// Filter und Kommentar der Attrappe.
pub const FILTER: &str = "10000";
pub const KOMMENTAR: &str = "100";
/// Die zweite Seite einer Suche.
pub const SEITE_2: &str = "seite-2";
/// Die ID einer neuen Verknüpfung.
pub const VERKNUEPFUNG: &str = "70000";

/// Ein Aufruf, den die Attrappe gesehen hat.
#[derive(Debug, Clone)]
pub struct Aufruf {
    /// Pfad, z. B. `/rest/api/3/project/search`.
    pub pfad: String,
    pub methode: String,
    /// Abfrageparameter (Wert dekodiert).
    pub abfrage: BTreeMap<String, String>,
    pub rumpf: String,
    /// Der `Authorization`-Kopf.
    pub anmeldung: Option<String>,
    /// Der `Content-Type`-Kopf.
    pub content_type: Option<String>,
}

impl Aufruf {
    /// Der Wert eines Abfrageparameters.
    pub fn feld(&self, name: &str) -> Option<&str> {
        self.abfrage.get(name).map(String::as_str)
    }

    /// Der Rumpf als JSON.
    pub fn json(&self) -> Value {
        serde_json::from_str(&self.rumpf).unwrap_or(Value::Null)
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

    /// Alle Aufrufe, deren Pfad `teil` enthält (Groß-/Kleinschreibung egal).
    pub fn zu(&self, teil: &str) -> Vec<Aufruf> {
        let teil = teil.to_lowercase();
        self.alle()
            .into_iter()
            .filter(|aufruf| aufruf.pfad.to_lowercase().contains(&teil))
            .collect()
    }

    pub fn anzahl(&self, teil: &str) -> usize {
        self.zu(teil).len()
    }

    /// Alle Aufrufe auf einen Pfadteil **mit** dieser Methode.
    pub fn zu_mit(&self, teil: &str, methode: &str) -> Vec<Aufruf> {
        self.zu(teil)
            .into_iter()
            .filter(|aufruf| aufruf.methode.eq_ignore_ascii_case(methode))
            .collect()
    }

    pub fn anzahl_mit(&self, teil: &str, methode: &str) -> usize {
        self.zu_mit(teil, methode).len()
    }

    pub fn erster(&self, teil: &str) -> Option<Aufruf> {
        self.zu(teil).into_iter().next()
    }

    pub fn letzter(&self) -> Option<Aufruf> {
        self.alle().pop()
    }

    pub fn merken(&self, aufruf: Aufruf) {
        griff(&self.aufrufe).push(aufruf);
    }
}

/// Der veränderliche Teil der Attrappe.
#[derive(Default)]
struct Zustand {
    aufzeichnung: Arc<Aufzeichnung>,
    /// E-Mail und Token gelten nicht: Antwort `401`.
    token_ungueltig: AtomicBool,
    /// Das Konto darf das nicht: Antwort `403`.
    keine_rechte: AtomicBool,
    /// So viele der nächsten Aufrufe werden mit `429` abgelehnt.
    zu_viele: AtomicUsize,
    /// Die Folgeseite zeigt auf einen fremden Host.
    fremde_seite: AtomicBool,
    /// Jede Antwort ist eine Weiterleitung auf einen fremden Host.
    weiterleitung: AtomicBool,
    /// Eine angelegte Verknüpfung wird verdreht gemerkt – die Gegenprobe zur
    /// gemeldeten Richtung.
    verdreht: AtomicBool,
    /// Der Status je Vorgang – ein Übergang ändert ihn wirklich.
    status: Mutex<BTreeMap<String, String>>,
    /// Angelegte Verknüpfungen, in der Form von `issueLink`.
    verknuepfungen: Mutex<Vec<Value>>,
}

impl Zustand {
    /// Der Status eines Vorgangs (Vorgabe: „In Progress“).
    fn status_von(&self, key: &str) -> String {
        griff(&self.status)
            .get(key)
            .cloned()
            .unwrap_or_else(|| "In Progress".to_string())
    }

    fn status_setzen(&self, key: &str, status: &str) {
        griff(&self.status).insert(key.to_string(), status.to_string());
    }

    fn verknuepfung_merken(&self, link: Value) {
        griff(&self.verknuepfungen).push(link);
    }
}

/// Eine laufende Attrappe.
pub struct Attrappe {
    /// `http://127.0.0.1:<port>` – ohne Schrägstrich am Ende.
    pub adresse: String,
    pub aufzeichnung: Arc<Aufzeichnung>,
    zustand: Arc<Zustand>,
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
        let zustand = Arc::new(Zustand {
            aufzeichnung: aufzeichnung.clone(),
            ..Zustand::default()
        });

        let app = Router::new()
            .fallback(bearbeiten)
            .with_state(zustand.clone());
        let hörer =
            tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
                .await
                .expect("kein freier Port");
        let adresse = hörer.local_addr().unwrap();
        let ende = tokio::spawn(async move {
            let _ = axum::serve(hörer, app).await;
        });

        Attrappe {
            adresse: format!("http://127.0.0.1:{}", adresse.port()),
            aufzeichnung,
            zustand,
            ende,
        }
    }

    /// Jeder Aufruf wird mit `401` abgelehnt (ungültiger Token).
    pub fn token_ablehnen(&self) {
        self.zustand.token_ungueltig.store(true, Ordering::SeqCst);
    }

    /// Jeder Aufruf wird mit `403` abgelehnt (keine Rechte).
    pub fn keine_rechte(&self) {
        self.zustand.keine_rechte.store(true, Ordering::SeqCst);
    }

    /// Die nächsten `anzahl` Aufrufe werden mit `429` abgelehnt.
    pub fn zu_viele_anfragen(&self, anzahl: usize) {
        self.zustand.zu_viele.store(anzahl, Ordering::SeqCst);
    }

    /// Die Folgeseite von `/project/search` zeigt auf einen fremden Host.
    pub fn fremde_folgeseite(&self) {
        self.zustand.fremde_seite.store(true, Ordering::SeqCst);
    }

    /// Jede Antwort ist eine `302`-Weiterleitung auf einen fremden Host.
    pub fn weiterleitung(&self) {
        self.zustand.weiterleitung.store(true, Ordering::SeqCst);
    }

    /// Eine angelegte Verknüpfung wird am Vorgang **andersherum** gezeigt.
    pub fn verdrehte_verknuepfung(&self) {
        self.zustand.verdreht.store(true, Ordering::SeqCst);
    }
}

// ------------------------------------------------------------------ Aufruf

async fn bearbeiten(State(zustand): State<Arc<Zustand>>, anfrage: Request) -> Response {
    let methode = anfrage.method().to_string();
    let pfad = anfrage.uri().path().to_string();
    let abfrage = abfrage_lesen(anfrage.uri().query().unwrap_or_default());
    let kopf = anfrage.headers().clone();
    let rumpf = axum::body::to_bytes(anfrage.into_body(), 4 * 1024 * 1024)
        .await
        .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
        .unwrap_or_default();

    zustand.aufzeichnung.merken(Aufruf {
        pfad: pfad.clone(),
        methode: methode.clone(),
        abfrage: abfrage.clone(),
        rumpf: rumpf.clone(),
        anmeldung: kopfzeile(&kopf, "authorization"),
        content_type: kopfzeile(&kopf, "content-type"),
    });

    // 429 vor allem anderen: so lässt sich prüfen, ob nur lesende Aufrufe
    // wiederholen.
    let offen = zustand.zu_viele.load(Ordering::SeqCst);
    if offen > 0 {
        zustand.zu_viele.store(offen - 1, Ordering::SeqCst);
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", "0")],
            fehler_rumpf("Zu viele Anfragen"),
        )
            .into_response();
    }

    if zustand.weiterleitung.load(Ordering::SeqCst) {
        return (
            StatusCode::FOUND,
            [("location", "http://fremd.example.test/rest/api/3/myself")],
            "",
        )
            .into_response();
    }

    if zustand.token_ungueltig.load(Ordering::SeqCst) || !anmeldung_gilt(&kopf) {
        return (
            StatusCode::UNAUTHORIZED,
            [("content-type", "application/json")],
            fehler_rumpf("Die Anmeldung ist ungültig."),
        )
            .into_response();
    }

    if zustand.keine_rechte.load(Ordering::SeqCst) {
        return (
            StatusCode::FORBIDDEN,
            [("content-type", "application/json")],
            fehler_rumpf("You do not have permission to perform this action."),
        )
            .into_response();
    }

    let basis = format!(
        "http://{}/rest/api/3",
        kopfzeile(&kopf, "host").unwrap_or_else(|| "127.0.0.1".to_string())
    );
    let antwort = beantworten(&basis, &zustand, &methode, &pfad, &abfrage, &rumpf);
    match antwort {
        Some(Antwort::Json(wert)) => (
            StatusCode::OK,
            [("content-type", "application/json")],
            wert.to_string(),
        )
            .into_response(),
        Some(Antwort::Angelegt(wert)) => (
            StatusCode::CREATED,
            [("content-type", "application/json")],
            wert.to_string(),
        )
            .into_response(),
        Some(Antwort::Ungueltig(wert)) => (
            StatusCode::BAD_REQUEST,
            [("content-type", "application/json")],
            wert.to_string(),
        )
            .into_response(),
        Some(Antwort::Leer) => StatusCode::NO_CONTENT.into_response(),
        Some(Antwort::AngelegtLeer) => StatusCode::CREATED.into_response(),
        None => (
            StatusCode::BAD_REQUEST,
            [("content-type", "application/json")],
            fehler_rumpf(&format!("Die Attrappe kennt {methode} {pfad} nicht.")),
        )
            .into_response(),
    }
}

/// Was die Attrappe antwortet.
enum Antwort {
    Json(Value),
    Angelegt(Value),
    /// Ein Fehler mit `400` (Pflichtfeld fehlt) – in der Form der echten API.
    Ungueltig(Value),
    Leer,
    /// `201` ohne Rumpf (so antwortet `POST issueLink` laut Spezifikation).
    AngelegtLeer,
}

fn json_wert(wert: Value) -> Option<Antwort> {
    Some(Antwort::Json(wert))
}

fn angelegt(wert: Value) -> Option<Antwort> {
    Some(Antwort::Angelegt(wert))
}

/// Ein Jira-Fehlerrumpf.
fn fehler_rumpf(meldung: &str) -> String {
    json!({ "errorMessages": [meldung], "errors": {} }).to_string()
}

/// Ein Fehler mit Feldmeldungen (`errors`).
fn feld_fehler(felder: Value) -> String {
    json!({ "errorMessages": [], "errors": felder }).to_string()
}

/// Sind alle Abfrageparameter erlaubt? Unbekannte sind ein `400`.
fn parameter_ok(abfrage: &BTreeMap<String, String>, erlaubt: &[&str]) -> bool {
    abfrage.keys().all(|name| erlaubt.contains(&name.as_str()))
}

fn beantworten(
    basis: &str,
    zustand: &Zustand,
    methode: &str,
    pfad: &str,
    abfrage: &BTreeMap<String, String>,
    rumpf: &str,
) -> Option<Antwort> {
    let fremde_seite = zustand.fremde_seite.load(Ordering::SeqCst);
    let rest = pfad.strip_prefix("/rest/api/3/")?;
    let ist_get = methode.eq_ignore_ascii_case("get");
    let ist_post = methode.eq_ignore_ascii_case("post");
    let ist_put = methode.eq_ignore_ascii_case("put");

    // ---------------------------------------------------------- Verbindung
    if rest == "myself" {
        if !ist_get || !parameter_ok(abfrage, &[]) {
            return None;
        }
        return json_wert(json!({
            "accountId": BENUTZER_ID,
            "displayName": BENUTZER,
            "emailAddress": EMAIL_A,
            "active": true,
            "timeZone": "Europe/Berlin"
        }));
    }

    // ------------------------------------------------------------ Projekte
    if rest == "project/search" {
        if !ist_get || !parameter_ok(abfrage, &["startAt", "maxResults"]) {
            return None;
        }
        let size: usize = abfrage
            .get("maxResults")
            .and_then(|wert| wert.parse().ok())
            .unwrap_or(50)
            // Kleine Seiten, damit der Weg über `next` geprüft werden kann.
            .clamp(1, 2);
        let start: usize = abfrage
            .get("startAt")
            .and_then(|wert| wert.parse().ok())
            .unwrap_or(0);
        let alle = [
            projekt(PROJEKT, PROJEKT_NAME),
            projekt(PROJEKT_FREMD, "Xsite"),
            projekt("DEF", "Delta"),
        ];
        let werte: Vec<Value> = alle.iter().skip(start).take(size).cloned().collect();
        let letzte = start + werte.len() >= alle.len();
        let mut antwort = json!({
            "self": format!("{basis}/project/search"),
            "maxResults": size,
            "startAt": start,
            "total": alle.len(),
            "isLast": letzte,
            "values": werte,
        });
        if !letzte {
            let next = if fremde_seite {
                format!(
                    "http://fremd.example.test/rest/api/3/project/search?startAt={}&maxResults={size}",
                    start + size
                )
            } else {
                format!(
                    "{basis}/project/search?startAt={}&maxResults={size}",
                    start + size
                )
            };
            antwort["next"] = json!(next);
        }
        return json_wert(antwort);
    }

    if let Some(key) = rest.strip_prefix("project/") {
        if !ist_get || !parameter_ok(abfrage, &[]) {
            return None;
        }
        // Jira nimmt Projektschlüssel ohne Rücksicht auf Groß-/Kleinschreibung.
        let gefunden = [PROJEKT, PROJEKT_FREMD, "DEF"]
            .into_iter()
            .find(|bekannt| bekannt.eq_ignore_ascii_case(key));
        return json_wert(match gefunden {
            Some(PROJEKT) => projekt(PROJEKT, PROJEKT_NAME),
            Some(PROJEKT_FREMD) => projekt(PROJEKT_FREMD, "Xsite"),
            Some(anderes) => projekt(anderes, "Delta"),
            None => json!({ "errorMessages": ["Projekt nicht gefunden"] }),
        });
    }

    // ------------------------------------------------------------- Suche
    if rest == "search/jql" {
        if !ist_post || !parameter_ok(abfrage, &[]) {
            return None;
        }
        let gelesen: Value = serde_json::from_str(rumpf).ok()?;
        let jql = gelesen.get("jql").and_then(Value::as_str).unwrap_or("");
        if jql.trim().is_empty() {
            return Some(Antwort::Ungueltig(json!({
                "errorMessages": [], "errors": { "jql": "Die JQL darf nicht leer sein." }
            })));
        }
        // Die Spezifikation verlangt eine **begrenzte** Abfrage.
        if !jql_begrenzt(jql) {
            return Some(Antwort::Ungueltig(json!({
                "errorMessages": [
                    "Die Abfrage muss begrenzt sein (bounded query); eine reine Sortierung \
                     genügt nicht."
                ],
                "errors": {}
            })));
        }
        // Nur die Felder, die die Spezifikation kennt.
        let erlaubt = [
            "expand",
            "fields",
            "fieldsByKeys",
            "includeArchivedProjects",
            "jql",
            "maxResults",
            "nextPageToken",
            "properties",
            "reconcileIssues",
        ];
        if let Value::Object(objekt) = &gelesen {
            if let Some(unbekannt) = objekt.keys().find(|name| !erlaubt.contains(&name.as_str())) {
                return Some(Antwort::Ungueltig(json!({
                    "errorMessages": [], "errors": { unbekannt: "Unbekanntes Feld." }
                })));
            }
        }
        let seite = gelesen.get("nextPageToken").and_then(Value::as_str);
        if seite == Some(SEITE_2) {
            return json_wert(json!({
                "issues": [ vorgang_baum(zustand, VORGANG_FREMD) ],
                "isLast": true
            }));
        }
        return json_wert(json!({
            "issues": [
                vorgang_baum(zustand, VORGANG),
                vorgang_baum(zustand, VORGANG_2)
            ],
            "nextPageToken": SEITE_2,
            "isLast": false
        }));
    }

    // --------------------------------------------------------- Vorgänge
    if rest == "issueLinkType" {
        if !ist_get || !parameter_ok(abfrage, &[]) {
            return None;
        }
        return json_wert(json!({
            "issueLinkTypes": [
                { "id": "10000", "name": "Blocks", "inward": "is blocked by", "outward": "blocks" },
                { "id": "10001", "name": "Relates", "inward": "relates to", "outward": "relates to" }
            ]
        }));
    }

    if rest == "issueLink" {
        if !ist_post || !parameter_ok(abfrage, &[]) {
            return None;
        }
        let gelesen: Value = serde_json::from_str(rumpf).ok()?;
        let mut fehler = serde_json::Map::new();
        let typ_name = gelesen
            .get("type")
            .and_then(|typ| typ.get("name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("");
        if typ_name.is_empty() {
            fehler.insert("type".into(), json!("Die Verknüpfungsart fehlt."));
        }
        let mut keys = Vec::new();
        for seite in ["inwardIssue", "outwardIssue"] {
            let key = gelesen
                .get(seite)
                .and_then(|teil| teil.get("key"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("");
            if key.is_empty() {
                fehler.insert(seite.into(), json!("Der Vorgang fehlt."));
            }
            keys.push(key.to_string());
        }
        if let Some(adf) = gelesen.get("comment").and_then(|teil| teil.get("body")) {
            if !adf_gueltig(adf) {
                fehler.insert(
                    "comment".into(),
                    json!("Der Kommentar ist kein gültiges ADF."),
                );
            }
        }
        if !fehler.is_empty() {
            return Some(Antwort::Ungueltig(
                json!({ "errorMessages": [], "errors": fehler }),
            ));
        }
        // Die Verknüpfung wirklich merken: `GET issue/…` zeigt sie danach.
        let (mut inward, mut outward) = (keys[0].clone(), keys[1].clone());
        if zustand.verdreht.load(Ordering::SeqCst) {
            std::mem::swap(&mut inward, &mut outward);
        }
        let (inward_text, outward_text) = match typ_name {
            "Blocks" => ("is blocked by", "blocks"),
            "Relates" => ("relates to", "relates to"),
            sonst => (sonst, sonst),
        };
        zustand.verknuepfung_merken(json!({
            "id": VERKNUEPFUNG,
            "type": { "name": typ_name, "inward": inward_text, "outward": outward_text },
            "outwardIssue": { "key": outward },
            "inwardIssue": { "key": inward }
        }));
        // Die Spezifikation nennt für `201` keinen Rumpf.
        return Some(Antwort::AngelegtLeer);
    }

    if rest == "issue/createmeta" || rest.starts_with("issue/createmeta/") {
        if !ist_get || !parameter_ok(abfrage, &[]) {
            return None;
        }
        let teile: Vec<&str> = rest.split('/').collect();
        // issue/createmeta/{projekt}/issuetypes[/{typ}]
        match teile.as_slice() {
            ["issue", "createmeta", _, "issuetypes"] => {
                // Die Form der Spezifikation: `PageOfCreateMetaIssueTypes`.
                return json_wert(json!({
                    "issueTypes": [
                        { "id": TYP_ID, "name": TYP, "subtask": false },
                        { "id": "10002", "name": "Task", "subtask": false }
                    ],
                    "startAt": 0,
                    "maxResults": 50,
                    "total": 2
                }));
            }
            ["issue", "createmeta", _, "issuetypes", typ] => {
                if *typ != TYP_ID && *typ != "10002" {
                    return None;
                }
                // Die Form der Spezifikation: `PageOfCreateMetaIssueTypeWithField`.
                return json_wert(json!({
                    "fields": {
                        "summary": { "required": true, "name": "Summary", "key": "summary",
                                     "operations": [], "schema": { "type": "string" } },
                        "priority": { "required": false, "name": "Priority", "key": "priority",
                                      "operations": [], "schema": { "type": "priority" },
                                      "allowedValues": [ { "name": "High" }, { "name": "Low" } ] }
                    },
                    "startAt": 0,
                    "maxResults": 50,
                    "total": 2
                }));
            }
            _ => return None,
        }
    }

    if let Some(key) = rest.strip_prefix("issue/") {
        if key.ends_with("/comment") {
            if ist_post {
                let gelesen: Value = serde_json::from_str(rumpf).ok()?;
                let adf = gelesen.get("body").cloned().unwrap_or(Value::Null);
                if !adf_gueltig(&adf) {
                    return Some(Antwort::Ungueltig(json!({
                        "errorMessages": [],
                        "errors": { "body": "Der Kommentar muss ein gültiges ADF-Dokument sein." }
                    })));
                }
                return angelegt(json!({ "id": KOMMENTAR, "body": adf }));
            }
            if !ist_get || !parameter_ok(abfrage, &["maxResults", "startAt", "orderBy", "expand"]) {
                return None;
            }
            return json_wert(json!({
                "startAt": 0,
                "maxResults": 25,
                "total": 2,
                "comments": [
                    { "id": "100", "author": { "displayName": "Anna" },
                      "created": "2026-11-20T10:00:00.000+0000",
                      "body": adf_absatz("Sieht gut aus") },
                    { "id": "101", "author": { "displayName": BENUTZER },
                      "created": "2026-11-20T11:00:00.000+0000",
                      "body": adf_absatz("Bitte noch ein Test") }
                ]
            }));
        }

        if let Some(key) = key.strip_suffix("/transitions") {
            if ist_post {
                let gelesen: Value = serde_json::from_str(rumpf).ok()?;
                let id = gelesen
                    .get("transition")
                    .and_then(|teil| teil.get("id"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let mut fehler = serde_json::Map::new();
                if id.is_empty() {
                    fehler.insert("transition".into(), json!("Die Übergangs-ID fehlt."));
                }
                if let Some(adf) = gelesen
                    .get("update")
                    .and_then(|teil| teil.get("comment"))
                    .and_then(Value::as_array)
                    .and_then(|eintraege| eintraege.first())
                    .and_then(|eintrag| eintrag.get("add"))
                    .and_then(|add| add.get("body"))
                {
                    if !adf_gueltig(adf) {
                        fehler.insert(
                            "update".into(),
                            json!("Der Kommentar ist kein gültiges ADF."),
                        );
                    }
                }
                if !fehler.is_empty() {
                    return Some(Antwort::Ungueltig(
                        json!({ "errorMessages": [], "errors": fehler }),
                    ));
                }
                // Ein Übergang ändert den Status wirklich – die folgende
                // Abfrage des Vorgangs sieht ihn.
                let ziel = match id {
                    "11" => Some("In Progress"),
                    "21" => Some("Done"),
                    "31" => Some("Closed"),
                    _ => None,
                };
                match ziel {
                    Some(ziel) => zustand.status_setzen(key, ziel),
                    None => {
                        return Some(Antwort::Ungueltig(json!({
                            "errorMessages": [
                                format!("Der Übergang {id} ist für {key} nicht möglich.")
                            ],
                            "errors": {}
                        })))
                    }
                }
                return Some(Antwort::Leer);
            }
            if !ist_get
                || !parameter_ok(
                    abfrage,
                    &[
                        "expand",
                        "transitionId",
                        "skipRemoteOnlyCondition",
                        "includeUnavailableTransitions",
                        "sortByOpsBarAndStatus",
                    ],
                )
            {
                return None;
            }
            return json_wert(json!({
                "transitions": [
                    { "id": "11", "name": "Start Progress", "to": { "name": "In Progress" } },
                    { "id": "21", "name": "Fertig", "to": { "name": "Done" } },
                    // Zwei Übergänge mit demselben Namen: der Name allein ist
                    // mehrdeutig, nur die ID führt weiter.
                    { "id": "31", "name": "Fertig", "to": { "name": "Closed" } }
                ]
            }));
        }

        // Detail oder Ändern.
        if ist_put {
            if !parameter_ok(abfrage, &[]) {
                return None;
            }
            let gelesen: Value = serde_json::from_str(rumpf).ok()?;
            let felder = gelesen.get("fields").and_then(Value::as_object);
            if felder.map(|felder| felder.is_empty()).unwrap_or(true) {
                return Some(Antwort::Ungueltig(json!({
                    "errorMessages": [],
                    "errors": { "fields": "Es wurden keine Felder angegeben." }
                })));
            }
            return Some(Antwort::Leer);
        }
        if !ist_get
            || !parameter_ok(
                abfrage,
                &[
                    "fields",
                    "fieldsByKeys",
                    "expand",
                    "properties",
                    "updateHistory",
                    "failFast",
                ],
            )
        {
            return None;
        }
        return json_wert(match key {
            VORGANG | VORGANG_2 | VORGANG_FREMD | UNTERAUFGABE | ELTERN => {
                vorgang_baum(zustand, key)
            }
            _ => json!({ "errorMessages": ["Vorgang nicht gefunden"] }),
        });
    }

    // ------------------------------------------------------ Kommentar neu
    if rest == "issue" {
        if !ist_post || !parameter_ok(abfrage, &[]) {
            return None;
        }
        let gelesen: Value = serde_json::from_str(rumpf).ok()?;
        let felder = gelesen.get("fields").cloned().unwrap_or(Value::Null);
        let mut fehler = serde_json::Map::new();
        let projekt = felder
            .get("project")
            .and_then(|teil| teil.get("key"))
            .and_then(Value::as_str)
            .unwrap_or("");
        if projekt.is_empty() {
            fehler.insert("project".into(), json!("Das Projekt fehlt."));
        }
        let typ = felder.get("issuetype").and_then(|teil| {
            teil.get("name")
                .or_else(|| teil.get("id"))
                .and_then(Value::as_str)
        });
        if typ.unwrap_or("").is_empty() {
            fehler.insert("issuetype".into(), json!("Der Vorgangstyp fehlt."));
        }
        if felder
            .get("summary")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .is_empty()
        {
            fehler.insert("summary".into(), json!("Der Titel fehlt."));
        }
        if !fehler.is_empty() {
            return Some(Antwort::Ungueltig(
                json!({ "errorMessages": [], "errors": fehler }),
            ));
        }
        return angelegt(json!({
            "id": "10009",
            "key": format!("{projekt}-9"),
            "self": format!("{basis}/issue/10009")
        }));
    }

    // ------------------------------------------------------------- Filter
    if rest == "filter/search" {
        if !ist_get || !parameter_ok(abfrage, &["filterName", "maxResults"]) {
            return None;
        }
        return json_wert(json!({
            "maxResults": 25,
            "startAt": 0,
            "isLast": true,
            "total": 1,
            "values": [ {
                "id": FILTER,
                "name": "Meine Bugs",
                "jql": format!("project = {PROJEKT} AND issuetype = {TYP}"),
                "owner": { "displayName": BENUTZER }
            } ]
        }));
    }

    if let Some(id) = rest.strip_prefix("filter/") {
        if !ist_get || !parameter_ok(abfrage, &[]) {
            return None;
        }
        if id != FILTER {
            return json_wert(json!({ "errorMessages": ["Filter nicht gefunden"] }));
        }
        return json_wert(json!({
            "id": FILTER,
            "name": "Meine Bugs",
            "jql": format!("project = {PROJEKT} AND issuetype = {TYP}"),
            "description": "Alle Bugs aus Alpha",
            "owner": { "displayName": BENUTZER }
        }));
    }

    // ------------------------------------------------------------ Personen
    if rest == "user/search" {
        if !ist_get || !parameter_ok(abfrage, &["query", "maxResults"]) {
            return None;
        }
        if abfrage
            .get("query")
            .map(|wert| wert.trim().is_empty())
            .unwrap_or(true)
        {
            return json_wert(json!([]));
        }
        return json_wert(json!([
            { "accountId": BENUTZER_ID, "displayName": BENUTZER,
              "emailAddress": EMAIL_A, "active": true },
            { "accountId": "acc-456", "displayName": "Anna Beispiel",
              "emailAddress": EMAIL_B, "active": false }
        ]));
    }

    None
}

/// Ein Projekt in der Form von `/project/search`.
fn projekt(key: &str, name: &str) -> Value {
    json!({
        "id": format!("id-{key}"),
        "key": key,
        "name": name,
        "projectTypeKey": "software",
        "simplified": false,
        "lead": { "displayName": BENUTZER },
        "avatarUrls": {}
    })
}

/// Ein Absatz als ADF-Dokument.
fn adf_absatz(text: &str) -> Value {
    json!({
        "type": "doc",
        "version": 1,
        "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": text } ] } ]
    })
}

/// Ein Vorgang in der Form von `GET /issue/{key}`.
///
/// Status und Verknüpfungen kommen aus dem Zustand: ein Übergang ändert den
/// Status wirklich, und eine angelegte Verknüpfung ist am Vorgang zu sehen.
fn vorgang_baum(zustand: &Zustand, key: &str) -> Value {
    let (projekt, summary) = match key {
        VORGANG => (PROJEKT, "Login kaputt"),
        VORGANG_2 => (PROJEKT, "Suche langsam"),
        VORGANG_FREMD => (PROJEKT_FREMD, "Fremder Vorgang"),
        UNTERAUFGABE => (PROJEKT, "Unteraufgabe"),
        ELTERN => (PROJEKT, "Elternvorgang"),
        _ => (PROJEKT, "Unbekannt"),
    };
    let mut felder = json!({
        "summary": summary,
        "status": { "name": zustand.status_von(key),
                    "statusCategory": { "key": "indeterminate" } },
        "issuetype": { "name": TYP },
        "project": { "key": projekt, "name": if projekt == PROJEKT { PROJEKT_NAME } else { "Xsite" } },
        "assignee": { "accountId": BENUTZER_ID, "displayName": BENUTZER },
        "reporter": { "displayName": "Anna Beispiel" },
        "priority": { "name": "High" },
        "labels": ["login", "bug"],
        "created": "2026-11-01T09:00:00.000+0000",
        "updated": "2026-11-20T10:15:00.000+0000",
        "duedate": "2026-11-30",
        "description": json!({
            "type": "doc",
            "version": 1,
            "content": [
                { "type": "paragraph", "content": [
                    { "type": "text", "text": "Der Anmelden-Knopf tut nichts." }
                ] },
                { "type": "bulletList", "content": [
                    { "type": "listItem", "content": [
                        { "type": "paragraph", "content": [ { "type": "text", "text": "Schritt eins" } ] }
                    ] }
                ] },
                { "type": "codeBlock", "attrs": { "language": "text" },
                  "content": [ { "type": "text", "text": "Fehler 500" } ] }
            ]
        }),
        "subtasks": [
            { "key": UNTERAUFGABE, "fields": { "summary": "Unteraufgabe",
              "status": { "name": "To Do" } } }
        ],
        "parent": { "key": ELTERN, "fields": { "summary": "Elternvorgang",
            "status": { "name": "Offen" } } }
    });

    let mut links: Vec<Value> = Vec::new();
    if key == VORGANG {
        links.push(json!(
            { "id": "1", "type": { "name": "Blocks", "inward": "is blocked by",
                                   "outward": "blocks" },
              "outwardIssue": { "key": VORGANG_2 } }
        ));
        links.push(json!(
            { "id": "2", "type": { "name": "Relates", "inward": "relates to",
                                   "outward": "relates to" },
              "inwardIssue": { "key": "ABC-3" } }
        ));
    }
    links.extend(
        griff(&zustand.verknuepfungen)
            .iter()
            .filter_map(|link| verknuepfung_sicht(link, key)),
    );
    felder["issuelinks"] = Value::Array(links);
    if key != VORGANG {
        // Die anderen Vorgänge brauchen keine Unteraufgaben und keinen Eltern.
        felder["subtasks"] = json!([]);
        felder.as_object_mut().unwrap().remove("parent");
    }
    json!({ "id": format!("id-{key}"), "key": key, "self": format!("/issue/{key}"), "fields": felder })
}

/// Eine angelegte Verknüpfung aus der Sicht **eines** Vorgangs: die eine Seite
/// steht im `outwardIssue`, die andere im `inwardIssue` – wie bei Jira.
fn verknuepfung_sicht(link: &Value, key: &str) -> Option<Value> {
    let aus = link.get("outwardIssue")?.get("key")?.as_str()?;
    let ein = link.get("inwardIssue")?.get("key")?.as_str()?;
    let typ = link.get("type")?.clone();
    let id = link.get("id").cloned().unwrap_or(Value::Null);
    if aus.eq_ignore_ascii_case(key) {
        Some(json!({ "id": id, "type": typ, "outwardIssue": { "key": ein } }))
    } else if ein.eq_ignore_ascii_case(key) {
        Some(json!({ "id": id, "type": typ, "inwardIssue": { "key": aus } }))
    } else {
        None
    }
}

/// Ist das ein ADF-Dokument, wie es die Spezifikation verlangt? Leere
/// `text`-Knoten und unbekannte Knoten sind ungültig.
fn adf_gueltig(wert: &Value) -> bool {
    if wert.get("type").and_then(Value::as_str) != Some("doc") {
        return false;
    }
    knoten_gueltig(wert)
}

fn knoten_gueltig(wert: &Value) -> bool {
    const KNOTEN: &[&str] = &[
        "doc",
        "paragraph",
        "text",
        "hardBreak",
        "bulletList",
        "orderedList",
        "listItem",
        "codeBlock",
        "heading",
        "blockquote",
        "rule",
        "panel",
        "mention",
        "emoji",
        "status",
        "inlineCard",
    ];
    let Some(art) = wert.get("type").and_then(Value::as_str) else {
        return false;
    };
    if !KNOTEN.contains(&art) {
        return false;
    }
    if art == "text" {
        return wert
            .get("text")
            .and_then(Value::as_str)
            .map(|text| !text.is_empty())
            .unwrap_or(false);
    }
    match wert.get("content") {
        None => true,
        Some(Value::Array(kinder)) => kinder.iter().all(knoten_gueltig),
        Some(_) => false,
    }
}

/// `search/jql` verlangt eine begrenzte Abfrage; eine reine Sortierung genügt
/// nicht (Spezifikation, `jql`: „requires a bounded query“).
fn jql_begrenzt(jql: &str) -> bool {
    let klein = jql.to_ascii_lowercase();
    match klein.find("order by") {
        Some(stelle) => !jql[..stelle].trim().is_empty(),
        None => !jql.trim().is_empty(),
    }
}

/// Gilt der `Authorization`-Kopf für eine der beiden Sites?
fn anmeldung_gilt(kopf: &HeaderMap) -> bool {
    let Some(wert) = kopfzeile(kopf, "authorization") else {
        return false;
    };
    let Some(kodiert) = wert.strip_prefix("Basic ") else {
        return false;
    };
    [(EMAIL_A, TOKEN_A), (EMAIL_B, TOKEN_B)]
        .iter()
        .any(|(email, token)| kodiert == basic(&format!("{email}:{token}")))
}

/// `base64(text)` – wie HTTP Basic es schickt.
fn basic(text: &str) -> String {
    BASE64.encode(text.as_bytes())
}

fn kopfzeile(kopf: &HeaderMap, name: &str) -> Option<String> {
    kopf.get(name)
        .and_then(|wert| wert.to_str().ok())
        .map(str::to_string)
}

/// Die Abfrageparameter einer Adresse (Werte dekodiert).
fn abfrage_lesen(roh: &str) -> BTreeMap<String, String> {
    let mut ergebnis = BTreeMap::new();
    for paar in roh.split('&').filter(|paar| !paar.is_empty()) {
        let (name, wert) = match paar.split_once('=') {
            Some((name, wert)) => (name, wert),
            None => (paar, ""),
        };
        ergebnis.insert(dekodieren(name), dekodieren(wert));
    }
    ergebnis
}

/// `%2F` → `/`, `+` → Leerzeichen; alles andere bleibt stehen.
fn dekodieren(wert: &str) -> String {
    let bytes = wert.as_bytes();
    let mut ergebnis = Vec::with_capacity(bytes.len());
    let mut stelle = 0;
    while stelle < bytes.len() {
        if bytes[stelle] == b'%' && stelle + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[stelle + 1..stelle + 3]).unwrap_or("");
            if let Ok(zeichen) = u8::from_str_radix(hex, 16) {
                ergebnis.push(zeichen);
                stelle += 3;
                continue;
            }
        }
        if bytes[stelle] == b'+' {
            ergebnis.push(b' ');
        } else {
            ergebnis.push(bytes[stelle]);
        }
        stelle += 1;
    }
    String::from_utf8_lossy(&ergebnis).to_string()
}

/// Die Aufrufe als `METHODE PFAD` (für Fehlermeldungen in Tests).
pub fn aufruf_pfade(aufzeichnung: &Aufzeichnung) -> Vec<String> {
    aufzeichnung
        .alle()
        .into_iter()
        .map(|aufruf| format!("{} {}", aufruf.methode, aufruf.pfad))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn der_authorization_kopf_wird_geprueft() {
        let erwartet = basic(&format!("{EMAIL_A}:{TOKEN_A}"));
        let mut kopf = HeaderMap::new();
        kopf.insert(
            "authorization",
            format!("Basic {erwartet}").parse().unwrap(),
        );
        assert!(anmeldung_gilt(&kopf));
        assert_eq!(
            erwartet,
            BASE64.encode(format!("{EMAIL_A}:{TOKEN_A}").as_bytes())
        );

        // Nur die E-Mail oder nur das Token reicht nicht.
        for falsch in [
            basic(&format!("{EMAIL_A}:falsch")),
            basic(&format!("falsch:{TOKEN_A}")),
            basic(TOKEN_A),
        ] {
            let mut kopf = HeaderMap::new();
            kopf.insert("authorization", format!("Basic {falsch}").parse().unwrap());
            assert!(!anmeldung_gilt(&kopf), "{falsch}");
        }
        assert!(!anmeldung_gilt(&HeaderMap::new()));
    }

    #[test]
    fn die_abfrage_wird_dekodiert() {
        let abfrage = abfrage_lesen("startAt=2&maxResults=10&query=Anna+B&jql=a%2Fb");
        assert_eq!(abfrage.get("startAt").map(String::as_str), Some("2"));
        assert_eq!(abfrage.get("maxResults").map(String::as_str), Some("10"));
        assert_eq!(abfrage.get("query").map(String::as_str), Some("Anna B"));
        assert_eq!(abfrage.get("jql").map(String::as_str), Some("a/b"));
    }

    #[test]
    fn unbekannte_parameter_werden_erkannt() {
        let abfrage = abfrage_lesen("startAt=0&quatsch=1");
        assert!(!parameter_ok(&abfrage, &["startAt", "maxResults"]));
        assert!(parameter_ok(&abfrage, &["startAt", "quatsch"]));
        assert!(parameter_ok(&BTreeMap::new(), &["startAt"]));
    }

    #[test]
    fn ein_unbekannter_pfad_ist_ein_fehler() {
        let zustand = Zustand::default();
        let gefunden = beantworten(
            "http://127.0.0.1:1/rest/api/3",
            &zustand,
            "GET",
            "/rest/api/3/gibtsnicht",
            &BTreeMap::new(),
            "",
        );
        assert!(gefunden.is_none());
        let gefunden = beantworten(
            "http://127.0.0.1:1/rest/api/3",
            &zustand,
            "GET",
            "/etwas/anderes",
            &BTreeMap::new(),
            "",
        );
        assert!(gefunden.is_none());
    }
}
