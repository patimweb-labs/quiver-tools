// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Azure-DevOps-Attrappe (SPEC §7): ein kleines axum-Programm, das die
//! benutzten Endpunkte einer Organisation beantwortet und jeden Aufruf
//! mitschreibt.
//!
//! Kein Test spricht je mit einem echten Azure DevOps. Die Attrappe prüft die
//! Anmeldung (`Authorization: Basic base64(":" + PAT)`) und kann den PAT
//! ablehnen (401) oder auf eine Anmeldeseite umleiten (203 mit HTML) – genau
//! wie DevOps es tut. Sie kann außerdem `429` melden (mit `Retry-After`), damit
//! der Wiederholungsweg nur bei lesenden Aufrufen geprüft werden kann.
//!
//! Zwei Organisationen teilen sich **eine** Attrappe: beide PATs gelten, damit
//! „mehrere Zugänge“ ohne zweiten Server geprüft werden kann. Alle Namen,
//! Nummern und Token sind erfunden.

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

/// PAT der ersten Organisation. Kein echtes Geheimnis.
pub const PAT_A: &str = "pat-attrappe-4711";
/// PAT der zweiten Organisation. Kein echtes Geheimnis.
pub const PAT_B: &str = "pat-attrappe-4712";
/// Anzeigename des angemeldeten Benutzers.
pub const BENUTZER: &str = "Patrick Weppelmann";
/// Die ID des angemeldeten Benutzers (für Abstimmungen).
pub const BENUTZER_ID: &str = "5f0c1e2a-0000-4000-8000-000000000001";
/// Das Projekt der Attrappe.
pub const PROJEKT: &str = "SCODA";
/// Das Team der Attrappe.
pub const TEAM: &str = "SCODA Team";
/// Das Repository der Attrappe.
pub const REPO: &str = "scoda-web";
/// Die Projekt-ID im Artefakt-Pfad.
pub const PROJEKT_ID: &str = "8a1f2b3c-0000-4000-8000-00000000000a";
/// Die Repo-ID im Artefakt-Pfad.
pub const REPO_ID: &str = "9b2f3c4d-0000-4000-8000-00000000000b";
/// Das Arbeitsitem der Attrappe.
pub const ITEM: u64 = 123;
/// Das zweite Arbeitsitem.
pub const ITEM_2: u64 = 124;
/// Der Pull Request der Attrappe.
pub const PR: u64 = 42;
/// Die Pipeline der Attrappe.
pub const PIPELINE: u32 = 7;
/// Der Lauf der Attrappe.
pub const LAUF: u64 = 1234;
/// Die Log-Nummer im Lauf.
pub const LOG: u64 = 5;
/// Die Iteration der Attrappe.
pub const ITERATION: &str = "sprint-1";

/// Ein Aufruf, den die Attrappe gesehen hat.
#[derive(Debug, Clone)]
pub struct Aufruf {
    /// Pfad, z. B. `/_apis/projects`.
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

    /// Die `api-version` dieses Aufrufs.
    pub fn api_version(&self) -> Option<&str> {
        self.feld("api-version")
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
struct Zustand {
    aufzeichnung: Arc<Aufzeichnung>,
    /// Der PAT gilt nicht: Antwort `401`.
    pat_ungueltig: AtomicBool,
    /// DevOps leitet auf die Anmeldung um: Antwort `203` mit HTML.
    anmeldeseite: AtomicBool,
    /// So viele der nächsten Aufrufe werden mit `429` abgelehnt.
    zu_viele: AtomicUsize,
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
            pat_ungueltig: AtomicBool::new(false),
            anmeldeseite: AtomicBool::new(false),
            zu_viele: AtomicUsize::new(0),
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

    /// Jeder Aufruf wird mit `401` abgelehnt (ungültiger PAT).
    pub fn pat_ablehnen(&self) {
        self.zustand.pat_ungueltig.store(true, Ordering::SeqCst);
    }

    /// Jeder Aufruf antwortet mit `203` und einer HTML-Anmeldeseite.
    pub fn anmeldeseite(&self) {
        self.zustand.anmeldeseite.store(true, Ordering::SeqCst);
    }

    /// Die nächsten `anzahl` Aufrufe werden mit `429` abgelehnt.
    pub fn zu_viele_anfragen(&self, anzahl: usize) {
        self.zustand.zu_viele.store(anzahl, Ordering::SeqCst);
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
            "429 Too Many Requests",
        )
            .into_response();
    }

    if zustand.anmeldeseite.load(Ordering::SeqCst) {
        return (
            StatusCode::NON_AUTHORITATIVE_INFORMATION,
            [("content-type", "text/html")],
            "<!DOCTYPE html><html><body>Anmelden bei Microsoft</body></html>",
        )
            .into_response();
    }

    if zustand.pat_ungueltig.load(Ordering::SeqCst) || !anmeldung_gilt(&kopf) {
        return (
            StatusCode::UNAUTHORIZED,
            [("content-type", "application/json")],
            json!({ "message": "TF400813: Der Benutzer ist nicht berechtigt." }).to_string(),
        )
            .into_response();
    }

    let muster = pfad.to_lowercase();
    let antwort = beantworten(&muster, &methode, &abfrage, &rumpf);
    match antwort {
        Some(Antwort::Json(wert)) => (
            StatusCode::OK,
            [("content-type", "application/json; charset=utf-8")],
            wert.to_string(),
        )
            .into_response(),
        Some(Antwort::Text(text)) => {
            (StatusCode::OK, [("content-type", "text/plain")], text).into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            [("content-type", "application/json")],
            json!({ "message": format!("Attrappe kennt {methode} {pfad} nicht.") }).to_string(),
        )
            .into_response(),
    }
}

/// Was die Attrappe antwortet.
enum Antwort {
    Json(Value),
    Text(String),
}

fn json_wert(wert: Value) -> Option<Antwort> {
    Some(Antwort::Json(wert))
}

fn beantworten(
    pfad: &str,
    methode: &str,
    abfrage: &BTreeMap<String, String>,
    rumpf: &str,
) -> Option<Antwort> {
    let ist_post = methode.eq_ignore_ascii_case("post");
    let ist_patch = methode.eq_ignore_ascii_case("patch");
    let werte = |teil: &str| pfad.contains(teil);

    // ---------------------------------------------------------- Verbindung
    if werte("/_apis/connectiondata") {
        return json_wert(json!({
            "authenticatedUser": {
                "id": BENUTZER_ID,
                "providerDisplayName": BENUTZER,
                "properties": { "Account": { "$value": "neoimpulse" } }
            },
            "instanceId": "00000000-0000-0000-0000-000000000000",
            "webApplicationRelativeDirectory": "/"
        }));
    }

    if pfad.ends_with("/_apis/projects") {
        return json_wert(json!({
            "count": 2,
            "value": [
                {
                    "id": PROJEKT_ID, "name": PROJEKT, "state": "wellFormed",
                    "visibility": "private", "revision": 12,
                    "url": format!("https://dev.azure.com/x/_apis/projects/{PROJEKT_ID}"),
                    "_links": { "self": { "href": format!("https://dev.azure.com/x/_apis/projects/{PROJEKT_ID}") } }
                },
                {
                    "id": "p2", "name": "Anderes", "state": "wellFormed",
                    "visibility": "public",
                    "url": "https://dev.azure.com/x/_apis/projects/p2",
                    "_links": { "self": { "href": "https://dev.azure.com/x/_apis/projects/p2" } }
                }
            ]
        }));
    }

    // ------------------------------------------------------- Arbeitsitems
    if werte("/comments") {
        if ist_post {
            return json_wert(json!({ "id": 4711, "text": "angelegt" }));
        }
        return json_wert(json!({
            "count": 2,
            "comments": [
                { "id": 1, "text": "<div>Sieht gut aus</div>",
                  "createdBy": { "displayName": "Anna" },
                  "createdDate": "2026-11-20T10:00:00Z" },
                { "id": 2, "text": "<div>Bitte noch ein Test</div>",
                  "createdBy": { "displayName": BENUTZER },
                  "createdDate": "2026-11-20T11:00:00Z" }
            ]
        }));
    }

    if werte("/_apis/wit/workitemsbatch") {
        let ids: Vec<u64> = serde_json::from_str::<Value>(rumpf)
            .ok()
            .and_then(|wert| {
                wert.get("ids")
                    .and_then(Value::as_array)
                    .map(|liste| liste.iter().filter_map(Value::as_u64).collect::<Vec<u64>>())
            })
            .unwrap_or_else(|| vec![ITEM, ITEM_2]);
        let items: Vec<Value> = ids.iter().map(|id| item(*id)).collect();
        return json_wert(json!({ "count": items.len(), "value": items }));
    }

    if werte("/_apis/wit/wiql") {
        return json_wert(json!({
            "queryType": "flat",
            "workItems": [
                { "id": ITEM, "url": "https://dev.azure.com/x/_apis/wit/workItems/123" },
                { "id": ITEM_2, "url": "https://dev.azure.com/x/_apis/wit/workItems/124" }
            ]
        }));
    }

    if werte("/_apis/wit/workitemtypes") {
        return json_wert(json!({
            "count": 2,
            "value": [
                { "name": "Bug", "referenceName": "Microsoft.Bug", "color": "cc293d",
                  "states": [ { "name": "New" }, { "name": "Active" }, { "name": "Closed" } ] },
                { "name": "User Story", "referenceName": "Microsoft.Feature",
                  "states": [ { "name": "New" }, { "name": "Active" } ] }
            ]
        }));
    }

    // Anlegen (`POST …/workitems/$Bug`) und Ändern (`PATCH …/workitems/123`).
    if werte("/_apis/wit/workitems/") || werte("/_apis/wit/workitems$") {
        if ist_post {
            let titel =
                feld_im_patch(rumpf, "System.Title").unwrap_or_else(|| "(ohne Titel)".into());
            return json_wert(json!({
                "id": ITEM_2, "rev": 1,
                "fields": {
                    "System.WorkItemType": "Bug",
                    "System.State": "New",
                    "System.Title": titel,
                },
                "_links": { "html": { "href": format!("https://dev.azure.com/x/_workitems/edit/{ITEM_2}") } }
            }));
        }
        if ist_patch {
            let mut geaendert = item(ITEM);
            if let Some(status) = feld_im_patch(rumpf, "System.State") {
                geaendert["fields"]["System.State"] = json!(status);
            }
            if let Some(titel) = feld_im_patch(rumpf, "System.Title") {
                geaendert["fields"]["System.Title"] = json!(titel);
            }
            return json_wert(geaendert);
        }
        return json_wert(item(ITEM));
    }

    // ----------------------------------------------------------- Iterationen
    if werte("/teamsettings/iterations") {
        if pfad.ends_with("/workitems") {
            return json_wert(json!({
                "workItemRelations": [
                    { "target": { "id": ITEM } },
                    { "target": { "id": ITEM_2 } }
                ]
            }));
        }
        return json_wert(json!({
            "count": 1,
            "value": [{
                "id": ITERATION,
                "name": "Sprint 1",
                "path": format!("{PROJEKT}\\Sprint 1"),
                "attributes": {
                    "startDate": "2026-11-01T00:00:00Z",
                    "finishDate": "2026-11-14T00:00:00Z",
                    "timeFrame": abfrage
                        .get("$timeframe")
                        .map(|_| "current")
                        .unwrap_or("past")
                }
            }]
        }));
    }

    // ---------------------------------------------------------------- Git
    if pfad.ends_with("/refs") {
        return json_wert(json!({
            "count": 2,
            "value": [
                { "name": "refs/heads/main", "objectId": "abcdef1234567890",
                  "creator": { "displayName": BENUTZER } },
                { "name": "refs/heads/feature/login", "objectId": "1234567890abcdef",
                  "creator": { "displayName": "Anna" } }
            ]
        }));
    }

    if werte("/threads") {
        if ist_post {
            // Antwort in einem bestehenden Thread oder neuer Thread.
            if pfad.ends_with("/comments") {
                return json_wert(json!({ "id": 8, "content": "Antwort" }));
            }
            return json_wert(json!({ "id": 7, "status": "active", "comments": [] }));
        }
        return json_wert(json!({
            "count": 2,
            "value": [
                { "id": 1, "status": "active", "comments": [
                    { "id": 1, "content": "<div>Bitte ändern</div>",
                      "author": { "displayName": "Anna" },
                      "publishedDate": "2026-11-20T10:00:00Z" }
                ] },
                { "id": 2, "status": "closed",
                  "properties": { "CodeReviewThreadType": { "$value": "VoteUpdate" } } }
            ]
        }));
    }

    if pfad.ends_with("/commits") {
        return json_wert(json!({
            "count": 1,
            "value": [{
                "commitId": "abcdef1234567890",
                "comment": "Login reparieren",
                "author": { "name": BENUTZER, "date": "2026-11-20T09:30:00Z" }
            }]
        }));
    }

    if pfad.ends_with("/workitems") {
        return json_wert(json!({ "count": 1, "value": [ { "id": ITEM.to_string() } ] }));
    }

    if werte("/reviewers/") {
        return json_wert(json!({ "id": BENUTZER_ID, "vote": 10 }));
    }

    if werte("/pullrequests") {
        if ist_post {
            let gelesen = serde_json::from_str::<Value>(rumpf).unwrap_or(Value::Null);
            return json_wert(json!({
                "pullRequestId": 43,
                "title": gelesen.get("title").cloned().unwrap_or(json!("(ohne Titel)")),
                "status": "active",
                "isDraft": gelesen.get("isDraft").cloned().unwrap_or(json!(false)),
                "sourceRefName": gelesen.get("sourceRefName").cloned().unwrap_or(json!("refs/heads/x")),
                "targetRefName": gelesen.get("targetRefName").cloned().unwrap_or(json!("refs/heads/main")),
                "createdBy": { "displayName": BENUTZER },
                "repository": { "id": REPO_ID, "name": REPO, "project": { "id": PROJEKT_ID } },
                "_links": { "web": { "href": "https://dev.azure.com/x/pr/43" } }
            }));
        }
        // Detail (`…/pullrequests/42`) oder Liste (`…/pullrequests`).
        if pfad
            .rsplit('/')
            .next()
            .is_some_and(|letzte| letzte.chars().all(|zeichen| zeichen.is_ascii_digit()))
        {
            return json_wert(pull_request());
        }
        return json_wert(json!({
            "count": 2,
            "value": [ pull_request(), pull_request_von_anna() ]
        }));
    }

    // ----------------------------------------------------------- Pipelines
    if werte("/runs") {
        return json_wert(json!({
            "id": 5678,
            "status": "inProgress",
            "result": Value::Null,
            "definition": { "id": PIPELINE, "name": "scoda-ci" },
            "_links": { "web": { "href": "https://dev.azure.com/x/build/5678" } }
        }));
    }

    if pfad.ends_with("/_apis/pipelines") {
        return json_wert(json!({
            "count": 1,
            "value": [ { "id": PIPELINE, "name": "scoda-ci", "folder": "\\", "revision": 3 } ]
        }));
    }

    // --------------------------------------------------------------- Builds
    if pfad.ends_with("/timeline") {
        return json_wert(json!({
            "records": [
                { "id": "s1", "type": "Stage", "name": "Build", "result": "failed", "errorCount": 1 },
                { "id": "j1", "type": "Job", "name": "Compile", "result": "succeeded", "errorCount": 0 },
                { "id": "t1", "type": "Task", "name": "npm test", "result": "failed",
                  "errorCount": 2, "log": { "id": LOG } }
            ]
        }));
    }

    if werte("/logs/") {
        let zeilen: Vec<String> = (1..=30).map(|nummer| format!("Zeile {nummer}")).collect();
        return Some(Antwort::Text(zeilen.join("\n")));
    }

    if pfad.ends_with("/_apis/build/builds") {
        return json_wert(json!({
            "count": 1,
            "value": [ {
                "id": LAUF, "buildNumber": "20261120.1", "status": "completed", "result": "failed",
                "sourceBranch": "refs/heads/main",
                "queueTime": "2026-11-20T09:59:00Z",
                "startTime": "2026-11-20T10:00:00Z",
                "finishTime": "2026-11-20T10:05:00Z",
                "definition": { "id": PIPELINE, "name": "scoda-ci" }
            } ]
        }));
    }

    if werte("/_apis/build/builds/") {
        return json_wert(json!({
            "id": LAUF, "buildNumber": "20261120.1", "status": "completed", "result": "failed",
            "sourceBranch": "refs/heads/main",
            "requestedFor": { "displayName": BENUTZER },
            "startTime": "2026-11-20T10:00:00Z",
            "finishTime": "2026-11-20T10:05:00Z",
            "definition": { "id": PIPELINE, "name": "scoda-ci" },
            "_links": { "web": { "href": "https://dev.azure.com/x/build/1234" } }
        }));
    }

    // ---------------------------------------------------------------- Repos
    if pfad.ends_with("/_apis/git/repositories") {
        return json_wert(json!({
            "count": 1,
            "value": [ {
                "id": REPO_ID, "name": REPO,
                "defaultBranch": "refs/heads/main", "size": 4096,
                "url": format!("https://dev.azure.com/x/_apis/git/repositories/{REPO_ID}"),
                "remoteUrl": format!("https://dev.azure.com/x/{PROJEKT}/_git/{REPO}"),
                "_links": { "self": { "href": format!("https://dev.azure.com/x/_apis/git/repositories/{REPO_ID}") } }
            } ]
        }));
    }

    None
}

/// Ein Arbeitsitem der Attrappe.
fn item(id: u64) -> Value {
    // Form nach der Azure-DevOps-REST-Spezifikation 7.1: `url`, `_links.self`,
    // `rev`, `fields` und `relations`. Der Client liest nur einen Teil davon;
    // die Attrappe antwortet trotzdem wie der Dienst (Lehre aus dem 404).
    json!({
        "id": id,
        "url": format!("https://dev.azure.com/x/_apis/wit/workItems/{id}"),
        "rev": 3,
        "fields": {
            "System.TeamProject": PROJEKT,
            "System.WorkItemType": "User Story",
            "System.State": "Active",
            "System.Title": "Login kaputt",
            "System.AssignedTo": { "displayName": BENUTZER },
            "System.AreaPath": format!("{PROJEKT}\\Web"),
            "System.IterationPath": format!("{PROJEKT}\\Sprint 1"),
            "System.ChangedDate": "2026-11-20T10:15:00Z",
            "System.Tags": "login; bug",
            "System.Description": "<div>Der Anmelden-Knopf tut nichts.</div>",
            "Microsoft.VSTS.Common.AcceptanceCriteria": "<ul><li>Anmelden klappt</li></ul>"
        },
        "_links": {
            "self": { "href": format!("https://dev.azure.com/x/_apis/wit/workItems/{id}") },
            "html": { "href": format!("https://dev.azure.com/x/_workitems/edit/{id}") }
        },
        "relations": [
            { "rel": "System.LinkTypes.Hierarchy-Reverse",
              "url": "https://dev.azure.com/x/_apis/wit/workItems/100" }
        ]
    })
}

/// Der Pull Request der Attrappe.
fn pull_request() -> Value {
    json!({
        "pullRequestId": PR,
        "title": "Login reparieren",
        "status": "active",
        "isDraft": false,
        "createdBy": { "displayName": BENUTZER },
        "creationDate": "2026-11-20T09:00:00Z",
        "sourceRefName": "refs/heads/feature/login",
        "targetRefName": "refs/heads/main",
        "mergeStatus": "succeeded",
        "description": "<div>Bitte prüfen.</div>",
        "repository": { "id": REPO_ID, "name": REPO, "project": { "id": PROJEKT_ID } },
        "reviewers": [
            { "displayName": "Anna", "vote": 10 },
            { "displayName": "Ben", "vote": -5 }
        ],
        "_links": { "web": { "href": "https://dev.azure.com/x/pr/42" } }
    })
}

/// Ein Pull Request von jemand anderem (für den Ersteller-Filter).
fn pull_request_von_anna() -> Value {
    json!({
        "pullRequestId": 43,
        "title": "Doku",
        "status": "active",
        "createdBy": { "displayName": "Anna" },
        "sourceRefName": "refs/heads/docs",
        "targetRefName": "refs/heads/main",
        "repository": { "id": REPO_ID, "name": REPO }
    })
}

/// Ein Wert aus einem JSON-Patch-Rumpf (`/fields/System.Title`).
fn feld_im_patch(rumpf: &str, feld: &str) -> Option<String> {
    let wert: Value = serde_json::from_str(rumpf).ok()?;
    let pfad = format!("/fields/{feld}");
    wert.as_array()?
        .iter()
        .find(|op| op.get("path").and_then(Value::as_str) == Some(pfad.as_str()))
        .and_then(|op| op.get("value"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Gilt der `Authorization`-Kopf für einen der beiden PATs?
fn anmeldung_gilt(kopf: &HeaderMap) -> bool {
    let Some(wert) = kopfzeile(kopf, "authorization") else {
        return false;
    };
    let Some(kodiert) = wert.strip_prefix("Basic ") else {
        return false;
    };
    [PAT_A, PAT_B]
        .iter()
        .any(|pat| kodiert == basic(&format!(":{pat}")))
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

/// Die Attrappe für einen Test, der nur die Aufrufe zählt.
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
        // `Authorization: Basic base64(":" + PAT)`.
        let erwartet = basic(&format!(":{PAT_A}"));
        let mut kopf = HeaderMap::new();
        kopf.insert(
            "authorization",
            format!("Basic {erwartet}").parse().unwrap(),
        );
        assert!(anmeldung_gilt(&kopf));
        assert_eq!(erwartet, BASE64.encode(format!(":{PAT_A}").as_bytes()));

        let mut falsch = HeaderMap::new();
        falsch.insert("authorization", "Basic bml4".parse().unwrap());
        assert!(!anmeldung_gilt(&falsch));
        assert!(!anmeldung_gilt(&HeaderMap::new()));
    }

    #[test]
    fn die_abfrage_wird_dekodiert() {
        let abfrage = abfrage_lesen("api-version=7.1&$top=10&branchName=refs%2Fheads%2Fmain&x=a+b");
        assert_eq!(abfrage.get("api-version").map(String::as_str), Some("7.1"));
        assert_eq!(abfrage.get("$top").map(String::as_str), Some("10"));
        assert_eq!(
            abfrage.get("branchName").map(String::as_str),
            Some("refs/heads/main")
        );
        assert_eq!(abfrage.get("x").map(String::as_str), Some("a b"));
    }

    #[test]
    fn der_patch_wert_wird_gefunden() {
        let rumpf = r#"[
            {"op":"add","path":"/fields/System.Title","value":"Neu"},
            {"op":"replace","path":"/fields/System.State","value":"Closed"}
        ]"#;
        assert_eq!(feld_im_patch(rumpf, "System.Title").as_deref(), Some("Neu"));
        assert_eq!(
            feld_im_patch(rumpf, "System.State").as_deref(),
            Some("Closed")
        );
        assert_eq!(feld_im_patch(rumpf, "System.Tags"), None);
    }
}
