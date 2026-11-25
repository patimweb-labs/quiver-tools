// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Confluence-Attrappe (SPEC §7): ein kleines axum-Programm, das die
//! benutzten Endpunkte einer Site beantwortet und jeden Aufruf mitschreibt.
//!
//! Kein Test spricht je mit einem echten Confluence. Die Attrappe ist
//! **streng** (Aufgabe 12): ein unbekannter Pfad oder Parameter ist `400`, ein
//! falscher `Authorization`-Kopf `401`, Pflichtfelder im Rumpf werden geprüft,
//! und die Antworten haben die Form der echten API (v2: `results` plus
//! `_links.next`).
//!
//! Sie kann außerdem `401`, `403`, `429`, `503`, `409`, eine leere Antwort
//! (`204`), eine Weiterleitung (`302`) und eine Folgeseite mit fremdem Host
//! liefern – damit jede Sicherheitsregel einzeln prüfbar ist.
//!
//! Zwei Sites teilen sich **eine** Attrappe: beide Token gelten, damit
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

/// E-Mail des Kontos. Kein echtes Konto.
pub const EMAIL: &str = "alex@example.test";
/// Token der ersten Site. Kein echtes Geheimnis.
pub const TOKEN_A: &str = "token-attrappe-4711";
/// Token der zweiten Site. Kein echtes Geheimnis.
pub const TOKEN_B: &str = "token-attrappe-4712";
/// Anzeigename des angemeldeten Benutzers.
pub const BENUTZER: &str = "Alex Beispiel";
/// Die Konto-Id des angemeldeten Benutzers.
pub const KONTO: &str = "5f0c1e2a-0000-4000-8000-000000000001";
/// Der Bereich der Attrappe.
pub const BEREICH: &str = "ENG";
/// Die Id des Bereichs.
pub const BEREICH_ID: &str = "10";
/// Ein zweiter Bereich (für `nur`).
pub const BEREICH_2: &str = "OPS";
/// Die Id des zweiten Bereichs.
pub const BEREICH_2_ID: &str = "11";
/// Die Seite der Attrappe.
pub const SEITE: &str = "123";
/// Der Titel der Seite.
pub const SEITE_TITEL: &str = "Anmeldung";
/// Die Version der Seite auf dem Server.
pub const SEITE_VERSION: i64 = 3;
/// Die Unterseite der Attrappe.
pub const UNTERSEITE: &str = "124";
/// Eine Seite in einem **anderen** Bereich (für `eltern`).
pub const FREMDE_SEITE: &str = "300";
/// Der Kommentar der Attrappe.
pub const KOMMENTAR: &str = "500";
/// Der Anhang der Attrappe.
pub const ANHANG: &str = "900";
/// Die neu angelegte Seite.
pub const NEUE_SEITE: &str = "200";

/// Die fünf Bereiche der Attrappe (mehr als eine Seite – für `_links.next`).
fn bereiche() -> Vec<Value> {
    [("10", "ENG", "Engineering"), ("11", "OPS", "Operations"),
     ("12", "DOC", "Dokumentation"), ("13", "HR", "Personal"), ("14", "QA", "Qualität")]
    .iter()
    .map(|(id, key, name)| {
        json!({ "id": id, "key": key, "name": name, "type": "global", "status": "current" })
    })
    .collect()
}

/// Ein Aufruf, den die Attrappe gesehen hat.
#[derive(Debug, Clone)]
pub struct Aufruf {
    /// Pfad, z. B. `/wiki/api/v2/spaces` (so wie er auf der Leitung stand).
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
struct Zustand {
    aufzeichnung: Arc<Aufzeichnung>,
    /// Token und E-Mail gelten nicht: Antwort `401`.
    token_ungueltig: AtomicBool,
    /// Das Konto darf nicht: Antwort `403`.
    keine_rechte: AtomicBool,
    /// So viele der nächsten Aufrufe werden mit `429` abgelehnt.
    zu_viele: AtomicUsize,
    /// So viele der nächsten Aufrufe werden mit `503` abgelehnt.
    ausgelastet: AtomicUsize,
    /// So viele der nächsten **verändernden** Aufrufe werden mit `429` abgelehnt.
    zu_viele_schreibend: AtomicUsize,
    /// Jede Änderung an einer Seite endet mit `409`.
    konflikt: AtomicBool,
    /// Änderungen antworten mit `204` ohne Rumpf.
    leere_antwort: AtomicBool,
    /// `GET /wiki/api/v2/spaces` leitet auf diese Adresse um.
    weiterleitung: Mutex<Option<String>>,
    /// Die Folgeseite der Bereiche zeigt auf diesen fremden Host.
    fremde_folgeseite: Mutex<Option<String>>,
    /// `GET /spaces?keys=…` antwortet mit einem **anderen** Schlüssel.
    falscher_schluessel: AtomicBool,
    /// `GET /spaces/{id}` antwortet ohne `key`.
    bereich_ohne_schluessel: AtomicBool,
    /// `GET /pages/{id}` antwortet ohne `body`.
    seite_ohne_inhalt: AtomicBool,
    /// `GET /pages/{id}` antwortet ohne `title`.
    seite_ohne_titel: AtomicBool,
    /// `GET /pages/{id}` antwortet ohne `status`.
    seite_ohne_status: AtomicBool,
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
            token_ungueltig: AtomicBool::new(false),
            keine_rechte: AtomicBool::new(false),
            zu_viele: AtomicUsize::new(0),
            ausgelastet: AtomicUsize::new(0),
            zu_viele_schreibend: AtomicUsize::new(0),
            konflikt: AtomicBool::new(false),
            leere_antwort: AtomicBool::new(false),
            weiterleitung: Mutex::new(None),
            fremde_folgeseite: Mutex::new(None),
            falscher_schluessel: AtomicBool::new(false),
            bereich_ohne_schluessel: AtomicBool::new(false),
            seite_ohne_inhalt: AtomicBool::new(false),
            seite_ohne_titel: AtomicBool::new(false),
            seite_ohne_status: AtomicBool::new(false),
        });

        let app = Router::new()
            .fallback(bearbeiten)
            .with_state(zustand.clone());
        let hoerer =
            tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
                .await
                .expect("kein freier Port");
        let adresse = hoerer.local_addr().unwrap();
        let ende = tokio::spawn(async move {
            let _ = axum::serve(hoerer, app).await;
        });

        Attrappe {
            adresse: format!("http://127.0.0.1:{}", adresse.port()),
            aufzeichnung,
            zustand,
            ende,
        }
    }

    /// Jeder Aufruf wird mit `401` abgelehnt (Token gilt nicht).
    pub fn token_ablehnen(&self) {
        self.zustand.token_ungueltig.store(true, Ordering::SeqCst);
    }

    /// Jeder Aufruf wird mit `403` abgelehnt (Rechte fehlen).
    pub fn rechte_verweigern(&self) {
        self.zustand.keine_rechte.store(true, Ordering::SeqCst);
    }

    /// Die nächsten `anzahl` Aufrufe werden mit `429` abgelehnt.
    pub fn zu_viele_anfragen(&self, anzahl: usize) {
        self.zustand.zu_viele.store(anzahl, Ordering::SeqCst);
    }

    /// Die nächsten `anzahl` Aufrufe werden mit `503` abgelehnt.
    pub fn ausgelastet(&self, anzahl: usize) {
        self.zustand.ausgelastet.store(anzahl, Ordering::SeqCst);
    }

    /// Die nächsten `anzahl` **verändernden** Aufrufe werden mit `429` abgelehnt.
    ///
    /// Damit lässt sich prüfen, dass ein Anlegen oder Ändern nicht wiederholt
    /// wird, ohne dass ein lesender Aufruf davor den 429 verbraucht.
    pub fn zu_viele_schreibende_anfragen(&self, anzahl: usize) {
        self.zustand
            .zu_viele_schreibend
            .store(anzahl, Ordering::SeqCst);
    }

    /// Änderungen an Seiten enden mit `409` (jemand war schneller).
    pub fn konflikt(&self) {
        self.zustand.konflikt.store(true, Ordering::SeqCst);
    }

    /// Änderungen antworten mit `204` ohne Rumpf.
    pub fn leere_antwort(&self) {
        self.zustand.leere_antwort.store(true, Ordering::SeqCst);
    }

    /// `GET /wiki/api/v2/spaces` leitet auf `ziel` um (fremder Host).
    pub fn weiterleiten(&self, ziel: &str) {
        *griff(&self.zustand.weiterleitung) = Some(ziel.to_string());
    }

    /// Die Folgeseite der Bereiche zeigt auf `ziel` (fremder Host).
    pub fn fremde_folgeseite(&self, ziel: &str) {
        *griff(&self.zustand.fremde_folgeseite) = Some(ziel.to_string());
    }

    /// `GET /spaces?keys=…` antwortet mit einem Schlüssel, der nicht passt.
    pub fn falscher_schluessel(&self) {
        self.zustand
            .falscher_schluessel
            .store(true, Ordering::SeqCst);
    }

    /// `GET /spaces/{id}` antwortet ohne `key`.
    pub fn bereich_ohne_schluessel(&self) {
        self.zustand
            .bereich_ohne_schluessel
            .store(true, Ordering::SeqCst);
    }

    /// `GET /pages/{id}` antwortet ohne Inhalt (`body`).
    pub fn seite_ohne_inhalt(&self) {
        self.zustand.seite_ohne_inhalt.store(true, Ordering::SeqCst);
    }

    /// `GET /pages/{id}` antwortet ohne `title`.
    pub fn seite_ohne_titel(&self) {
        self.zustand.seite_ohne_titel.store(true, Ordering::SeqCst);
    }

    /// `GET /pages/{id}` antwortet ohne `status`.
    pub fn seite_ohne_status(&self) {
        self.zustand.seite_ohne_status.store(true, Ordering::SeqCst);
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

    // 429 und 503 vor allem anderen: so lässt sich prüfen, ob nur lesende
    // Aufrufe wiederholen.
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
    let offen = zustand.ausgelastet.load(Ordering::SeqCst);
    if offen > 0 {
        zustand.ausgelastet.store(offen - 1, Ordering::SeqCst);
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [("retry-after", "0")],
            "503 Service Unavailable",
        )
            .into_response();
    }

    if !methode.eq_ignore_ascii_case("get") {
        let offen = zustand.zu_viele_schreibend.load(Ordering::SeqCst);
        if offen > 0 {
            zustand
                .zu_viele_schreibend
                .store(offen - 1, Ordering::SeqCst);
            return (
                StatusCode::TOO_MANY_REQUESTS,
                [("retry-after", "0")],
                "429 Too Many Requests",
            )
                .into_response();
        }
    }

    if zustand.token_ungueltig.load(Ordering::SeqCst) || !anmeldung_gilt(&kopf) {
        return (
            StatusCode::UNAUTHORIZED,
            [("content-type", "application/json")],
            json!({ "message": "Unauthorized: authentication required." }).to_string(),
        )
            .into_response();
    }

    if zustand.keine_rechte.load(Ordering::SeqCst) {
        return (
            StatusCode::FORBIDDEN,
            [("content-type", "application/json")],
            json!({ "message": "You are not permitted to perform this operation." }).to_string(),
        )
            .into_response();
    }

    // Eine Weiterleitung: der Client darf ihr nicht folgen.
    if methode.eq_ignore_ascii_case("get") && pfad == "/wiki/api/v2/spaces" {
        if let Some(ziel) = griff(&zustand.weiterleitung).clone() {
            return (
                StatusCode::FOUND,
                [("location", format!("{ziel}/wiki/api/v2/spaces").as_str())],
                "",
            )
                .into_response();
        }
    }

    // Ein Rumpf, der nicht zu seinem `Content-Length` passt: `text().await`
    // scheitert – so lässt sich prüfen, dass „nicht lesbar“ ein Fehler ist.
    let antwort = beantworten(&pfad, &methode, &abfrage, &rumpf, &zustand);
    match antwort {
        Antwort::Json(wert) => (
            StatusCode::OK,
            [("content-type", "application/json")],
            wert.to_string(),
        )
            .into_response(),
        Antwort::Leer => StatusCode::NO_CONTENT.into_response(),
        Antwort::Fehler(status, text) => (
            StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_REQUEST),
            [("content-type", "application/json")],
            json!({ "message": text }).to_string(),
        )
            .into_response(),
    }
}

/// Was die Attrappe antwortet.
enum Antwort {
    Json(Value),
    /// `204`: Erfolg ohne Rumpf.
    Leer,
    /// Ein Fehler mit Status und kurzer Meldung.
    Fehler(u16, String),
}

fn fehler(status: u16, text: impl Into<String>) -> Antwort {
    Antwort::Fehler(status, text.into())
}

/// Ein unbekannter Abfrageparameter: die Attrappe ist streng.
fn unbekannt(abfrage: &BTreeMap<String, String>, erlaubt: &[&str]) -> Option<Antwort> {
    abfrage
        .keys()
        .find(|name| !erlaubt.contains(&name.as_str()))
        .map(|name| fehler(400, format!("Unbekannter Parameter „{name}“.")))
}

/// Ein Pflichtparameter der Abfrage.
fn pflicht_abfrage<'a>(
    abfrage: &'a BTreeMap<String, String>,
    name: &str,
) -> Result<&'a str, Antwort> {
    abfrage
        .get(name)
        .map(String::as_str)
        .filter(|wert| !wert.trim().is_empty())
        .ok_or_else(|| fehler(400, format!("Parameter „{name}“ fehlt.")))
}

/// Ein Wert unter einem Punktpfad im Rumpf.
fn feld<'a>(baum: &'a Value, pfad: &str) -> Option<&'a Value> {
    let mut aktuell = baum;
    for teil in pfad.split('.').filter(|teil| !teil.is_empty()) {
        aktuell = match teil.parse::<usize>() {
            Ok(index) => aktuell.get(index)?,
            Err(_) => aktuell.get(teil)?,
        };
    }
    (!aktuell.is_null()).then_some(aktuell)
}

/// Ein Pflichttext im Rumpf.
fn pflicht_feld(baum: &Value, pfad: &str) -> Result<String, Antwort> {
    feld(baum, pfad)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|wert| !wert.trim().is_empty())
        .ok_or_else(|| fehler(400, format!("Feld „{pfad}“ fehlt im Rumpf.")))
}

/// Nur diese Felder sind im Rumpf erlaubt.
fn erlaubte_felder(baum: &Value, erlaubt: &[&str]) -> Option<Antwort> {
    let objekt = baum.as_object()?;
    objekt
        .keys()
        .find(|name| !erlaubt.contains(&name.as_str()))
        .map(|name| fehler(400, format!("Unbekanntes Feld „{name}“ im Rumpf.")))
}

fn beantworten(
    pfad: &str,
    methode: &str,
    abfrage: &BTreeMap<String, String>,
    rumpf: &str,
    zustand: &Zustand,
) -> Antwort {
    let ist = |m: &str| methode.eq_ignore_ascii_case(m);

    // ------------------------------------------------------- ältere API (v1)
    // Labels schreibt v2 nicht (`GET /pages/{id}/labels` gibt es dort nur lesend).
    if let Some(id) = pfad
        .strip_prefix("/wiki/rest/api/content/")
        .and_then(|rest| rest.strip_suffix("/label"))
    {
        if !ist("POST") {
            return fehler(400, format!("{methode} {pfad} kennt die Attrappe nicht."));
        }
        if let Some(f) = unbekannt(abfrage, &[]) {
            return f;
        }
        let Ok(wert) = serde_json::from_str::<Value>(rumpf) else {
            return fehler(400, "Der Rumpf ist kein JSON.".to_string());
        };
        let Some(liste) = wert.as_array() else {
            return fehler(400, "Der Rumpf muss eine Liste sein.".to_string());
        };
        for label in liste {
            if let Some(f) = erlaubte_felder(label, &["prefix", "name"]) {
                return f;
            }
            if let Err(f) = pflicht_feld(label, "prefix").and_then(|_| pflicht_feld(label, "name"))
            {
                return f;
            }
        }
        if id != SEITE && id != NEUE_SEITE {
            return fehler(404, format!("Die Seite {id} gibt es nicht."));
        }
        if zustand.leere_antwort.load(Ordering::SeqCst) {
            return Antwort::Leer;
        }
        return Antwort::Json(json!({ "results": [
            { "id": "l3", "name": feld(&wert, "0.name").cloned().unwrap_or(json!("label")),
              "prefix": "global" }
        ] }));
    }

    if pfad == "/wiki/rest/api/user/current" {
        if let Some(f) = unbekannt(abfrage, &[]) {
            return f;
        }
        if !ist("GET") {
            return fehler(400, format!("{methode} {pfad} kennt die Attrappe nicht."));
        }
        return Antwort::Json(json!({
            "accountId": KONTO,
            "displayName": BENUTZER,
            "publicName": "pweppelmann",
            "type": "known",
            "email": EMAIL
        }));
    }

    if pfad == "/wiki/rest/api/search" {
        if !ist("GET") {
            return fehler(400, format!("{methode} {pfad} kennt die Attrappe nicht."));
        }
        if let Some(f) = unbekannt(abfrage, &["cql", "limit", "cursor"]) {
            return f;
        }
        let cql = match pflicht_abfrage(abfrage, "cql") {
            Ok(cql) => cql.to_string(),
            Err(f) => return f,
        };
        let treffer = vec![
            json!({
                "content": { "id": SEITE, "type": "page", "status": "current",
                             "title": SEITE_TITEL, "space": { "key": BEREICH } },
                "title": SEITE_TITEL,
                "excerpt": "<b>Der</b> Anmelden-Knopf tut nichts.",
                "url": "/spaces/ENG/pages/123/Anmeldung"
            }),
            json!({
                "content": { "id": "125", "type": "page", "status": "current",
                             "title": "Anmeldung – Details", "space": { "key": BEREICH } },
                "title": "Anmeldung – Details",
                "excerpt": "Wie die Anmeldung aufgebaut ist."
            }),
            json!({
                "content": { "id": "126", "type": "page", "status": "current",
                             "title": "Abmeldung", "space": { "key": BEREICH_2 } },
                "title": "Abmeldung",
                "excerpt": "Was beim Abmelden passiert."
            }),
        ];
        let mut blatt = blaettern(&treffer, abfrage, "/wiki/rest/api/search");
        if let Antwort::Json(wert) = &mut blatt {
            wert["cqlQuery"] = json!(cql);
        }
        return blatt;
    }

    // ------------------------------------------------------- API v2
    let Some(rest) = pfad.strip_prefix("/wiki/api/v2/") else {
        return fehler(400, format!("Attrappe kennt {methode} {pfad} nicht."));
    };
    let teile: Vec<&str> = rest.split('/').collect();

    match teile.as_slice() {
        // ---------------------------------------------------------- Bereiche
        ["spaces"] => {
            if !ist("GET") {
                return fehler(400, format!("{methode} {pfad} kennt die Attrappe nicht."));
            }
            if let Some(f) = unbekannt(abfrage, &["limit", "cursor", "keys"]) {
                return f;
            }
            let alle: Vec<Value> = match abfrage.get("keys") {
                Some(schluessel) => bereiche()
                    .into_iter()
                    .filter(|bereich| {
                        bereich["key"]
                            .as_str()
                            .is_some_and(|key| key.eq_ignore_ascii_case(schluessel))
                            || bereich["id"].as_str() == Some(schluessel.as_str())
                    })
                    .map(|mut bereich| {
                        // Für den Test „der Schlüssel muss passen“ antwortet die
                        // Attrappe mit einem anderen Schlüssel als gefragt.
                        if zustand.falscher_schluessel.load(Ordering::SeqCst) {
                            bereich["key"] = json!(BEREICH_2);
                        }
                        bereich
                    })
                    .collect(),
                None => bereiche(),
            };
            let mut blatt = blaettern(&alle, abfrage, "/wiki/api/v2/spaces");
            if let (Antwort::Json(wert), Some(fremd)) =
                (&mut blatt, griff(&zustand.fremde_folgeseite).clone())
            {
                if wert.get("_links").is_some() {
                    wert["_links"]["next"] =
                        json!(format!("{fremd}/wiki/api/v2/spaces?limit=2&cursor=2"));
                }
            }
            blatt
        }
        ["spaces", id] => {
            if !ist("GET") {
                return fehler(400, format!("{methode} {pfad} kennt die Attrappe nicht."));
            }
            if let Some(f) = unbekannt(abfrage, &[]) {
                return f;
            }
            match bereiche()
                .into_iter()
                .find(|bereich| bereich["id"].as_str() == Some(id))
            {
                Some(mut bereich) => {
                    if zustand.bereich_ohne_schluessel.load(Ordering::SeqCst) {
                        bereich.as_object_mut().unwrap().remove("key");
                    }
                    Antwort::Json(bereich)
                }
                None => fehler(404, format!("Den Bereich {id} gibt es nicht.")),
            }
        }

        // ------------------------------------------------------------ Seiten
        ["pages"] if ist("POST") => {
            if let Some(f) = unbekannt(abfrage, &[]) {
                return f;
            }
            let Ok(wert) = serde_json::from_str::<Value>(rumpf) else {
                return fehler(400, "Der Rumpf ist kein JSON.".to_string());
            };
            if let Some(f) = erlaubte_felder(
                &wert,
                &["spaceId", "status", "title", "body", "parentId", "subtype"],
            ) {
                return f;
            }
            let space_id = match pflicht_feld(&wert, "spaceId") {
                Ok(wert) => wert,
                Err(f) => return f,
            };
            let titel = match pflicht_feld(&wert, "title") {
                Ok(wert) => wert,
                Err(f) => return f,
            };
            let inhalt = match pflicht_feld(&wert, "body.value") {
                Ok(wert) => wert,
                Err(f) => return f,
            };
            if feld(&wert, "body.representation").and_then(Value::as_str) != Some("storage") {
                return fehler(400, "body.representation muss „storage“ sein.".to_string());
            }
            if !bereiche()
                .iter()
                .any(|bereich| bereich["id"].as_str() == Some(space_id.as_str()))
            {
                return fehler(400, format!("Den Bereich {space_id} gibt es nicht."));
            }
            let mut seite = seite_json(NEUE_SEITE, 1, &inhalt);
            seite["title"] = json!(titel);
            seite["spaceId"] = json!(space_id);
            seite["parentId"] = wert.get("parentId").cloned().unwrap_or(Value::Null);
            Antwort::Json(seite)
        }
        ["pages"] => {
            if !ist("GET") {
                return fehler(400, format!("{methode} {pfad} kennt die Attrappe nicht."));
            }
            if let Some(f) = unbekannt(
                abfrage,
                &["space-id", "title", "body-format", "limit", "cursor"],
            ) {
                return f;
            }
            let space_id = match pflicht_abfrage(abfrage, "space-id") {
                Ok(wert) => wert,
                Err(f) => return f,
            };
            let titel = match pflicht_abfrage(abfrage, "title") {
                Ok(wert) => wert,
                Err(f) => return f,
            };
            let treffer: Vec<Value> = if space_id == BEREICH_ID && titel == SEITE_TITEL {
                vec![seite_json(SEITE, SEITE_VERSION, SEITE_INHALT)]
            } else {
                Vec::new()
            };
            Antwort::Json(json!({ "results": treffer }))
        }
        ["pages", id] => {
            if let Some(f) = unbekannt(abfrage, &["body-format"]) {
                return f;
            }
            if *id == FREMDE_SEITE && ist("GET") {
                return Antwort::Json(fremde_seite_json(id));
            }
            if *id != SEITE && *id != NEUE_SEITE {
                return fehler(404, format!("Die Seite {id} gibt es nicht."));
            }
            if ist("GET") {
                let version = if *id == NEUE_SEITE { 1 } else { SEITE_VERSION };
                return Antwort::Json(seite_json_gefiltert(id, version, SEITE_INHALT, zustand));
            }
            if ist("PUT") {
                if zustand.konflikt.load(Ordering::SeqCst) {
                    return fehler(409, "Version conflict: the page has changed.");
                }
                let Ok(wert) = serde_json::from_str::<Value>(rumpf) else {
                    return fehler(400, "Der Rumpf ist kein JSON.".to_string());
                };
                // Genau die Felder von `PageUpdateRequest`.
                if let Some(f) = erlaubte_felder(
                    &wert,
                    &[
                        "id", "status", "title", "spaceId", "parentId", "ownerId", "body",
                        "version",
                    ],
                ) {
                    return f;
                }
                if pflicht_feld(&wert, "id").ok().as_deref() != Some(id) {
                    return fehler(400, "id im Rumpf passt nicht zum Pfad.".to_string());
                }
                let titel = match pflicht_feld(&wert, "title") {
                    Ok(wert) => wert,
                    Err(f) => return f,
                };
                let status = match pflicht_feld(&wert, "status") {
                    Ok(wert) => wert,
                    Err(f) => return f,
                };
                let inhalt = match pflicht_feld(&wert, "body.value") {
                    Ok(wert) => wert,
                    Err(f) => return f,
                };
                let Some(version) = feld(&wert, "version.number").and_then(Value::as_i64) else {
                    return fehler(400, "version.number fehlt im Rumpf.".to_string());
                };
                if version != SEITE_VERSION + 1 {
                    return fehler(
                        409,
                        format!(
                            "Version conflict: erwartet {}, bekommen {version}.",
                            SEITE_VERSION + 1
                        ),
                    );
                }
                let mut seite = seite_json(id, version, &inhalt);
                seite["title"] = json!(titel);
                seite["status"] = json!(status);
                return Antwort::Json(seite);
            }
            fehler(400, format!("{methode} {pfad} kennt die Attrappe nicht."))
        }
        ["pages", id, "direct-children"] => {
            if !ist("GET") {
                return fehler(400, format!("{methode} {pfad} kennt die Attrappe nicht."));
            }
            if let Some(f) = unbekannt(abfrage, &["limit", "cursor"]) {
                return f;
            }
            if *id != SEITE {
                return fehler(404, format!("Die Seite {id} gibt es nicht."));
            }
            // Laut Spezifikation kurze Einträge – auch Ordner, Whiteboards …
            let kinder = vec![json!({
                "id": UNTERSEITE, "status": "current", "title": "Unterpunkt",
                "type": "page", "spaceId": "900", "childPosition": 0
            })];
            blaettern(&kinder, abfrage, "/wiki/api/v2/pages/123/direct-children")
        }
        ["pages", id, "footer-comments"] => {
            if !ist("GET") {
                return fehler(400, format!("{methode} {pfad} kennt die Attrappe nicht."));
            }
            if let Some(f) = unbekannt(abfrage, &["body-format", "limit", "cursor"]) {
                return f;
            }
            if *id != SEITE {
                return fehler(404, format!("Die Seite {id} gibt es nicht."));
            }
            let kommentare = vec![
                json!({
                    "id": KOMMENTAR, "status": "current", "title": "Re: Anmeldung",
                    "pageId": SEITE,
                    "authorId": KONTO,
                    "version": { "number": 1, "authorId": KONTO,
                                 "createdAt": "2026-11-20T11:00:00Z" },
                    "body": { "storage": { "representation": "storage",
                                           "value": "<p>Sieht gut aus</p>" } }
                }),
                json!({
                    "id": "501", "status": "current", "title": "Re: Anmeldung",
                    "pageId": SEITE,
                    "authorId": "5f0c1e2a-0000-4000-8000-000000000002",
                    "version": { "number": 1, "authorId": "5f0c1e2a-0000-4000-8000-000000000002",
                                 "createdAt": "2026-11-20T12:00:00Z" },
                    "body": { "storage": { "representation": "storage",
                                           "value": "<p>Bitte noch ein Test</p>" } }
                }),
            ];
            blaettern(
                &kommentare,
                abfrage,
                "/wiki/api/v2/pages/123/footer-comments",
            )
        }
        ["pages", id, "attachments"] => {
            if !ist("GET") {
                return fehler(400, format!("{methode} {pfad} kennt die Attrappe nicht."));
            }
            if let Some(f) = unbekannt(abfrage, &["limit", "cursor"]) {
                return f;
            }
            if *id != SEITE {
                return fehler(404, format!("Die Seite {id} gibt es nicht."));
            }
            let anhaenge = vec![
                json!({ "id": ANHANG, "title": "bild.png", "mediaType": "image/png",
                        "fileSize": 4096, "version": { "number": 2 },
                        "downloadLink": "/download/attachments/123/bild.png" }),
                json!({ "id": "901", "title": "handbuch.pdf", "mediaType": "application/pdf",
                        "fileSize": 1048576, "version": { "number": 1 },
                        "downloadLink": "/download/attachments/123/handbuch.pdf" }),
            ];
            blaettern(&anhaenge, abfrage, "/wiki/api/v2/pages/123/attachments")
        }
        ["pages", id, "labels"] => {
            // v2 kennt hier nur `GET`; geschrieben wird über die ältere API
            // (`POST /wiki/rest/api/content/{id}/label`).
            if !ist("GET") {
                return fehler(400, format!("{methode} {pfad} kennt die Attrappe nicht."));
            }
            if let Some(f) = unbekannt(abfrage, &["limit", "cursor"]) {
                return f;
            }
            if *id != SEITE {
                return fehler(404, format!("Die Seite {id} gibt es nicht."));
            }
            let labels = vec![
                json!({ "id": "l1", "name": "anmeldung", "prefix": "global" }),
                json!({ "id": "l2", "name": "doku", "prefix": "global" }),
            ];
            blaettern(&labels, abfrage, "/wiki/api/v2/pages/123/labels")
        }

        // ---------------------------------------------------------- Kommentar
        ["footer-comments"] if ist("POST") => {
            if let Some(f) = unbekannt(abfrage, &[]) {
                return f;
            }
            let Ok(wert) = serde_json::from_str::<Value>(rumpf) else {
                return fehler(400, "Der Rumpf ist kein JSON.".to_string());
            };
            // Genau die Felder von `CreateFooterCommentModel` – `status` gehört
            // nicht dazu.
            if let Some(f) = erlaubte_felder(
                &wert,
                &[
                    "blogPostId",
                    "pageId",
                    "parentCommentId",
                    "attachmentId",
                    "customContentId",
                    "body",
                ],
            ) {
                return f;
            }
            let page_id = match pflicht_feld(&wert, "pageId") {
                Ok(wert) => wert,
                Err(f) => return f,
            };
            if page_id != SEITE {
                return fehler(404, format!("Die Seite {page_id} gibt es nicht."));
            }
            let inhalt = match pflicht_feld(&wert, "body.value") {
                Ok(wert) => wert,
                Err(f) => return f,
            };
            if zustand.leere_antwort.load(Ordering::SeqCst) {
                return Antwort::Leer;
            }
            Antwort::Json(json!({
                "id": KOMMENTAR, "status": "current", "pageId": page_id,
                "authorId": KONTO,
                "version": { "number": 1, "authorId": KONTO,
                             "createdAt": "2026-11-20T13:00:00Z" },
                "body": { "storage": { "representation": "storage", "value": inhalt } }
            }))
        }

        _ => fehler(400, format!("Attrappe kennt {methode} {pfad} nicht.")),
    }
}

/// Der Inhalt der Seite auf dem Server.
pub const SEITE_INHALT: &str =
    "<p>Der Anmelden-Knopf tut nichts.</p><ul><li>Login klappt</li></ul>";

/// Eine Seite der Attrappe.
fn seite_json(id: &str, version: i64, inhalt: &str) -> Value {
    json!({
        "id": id,
        "title": SEITE_TITEL,
        "status": "current",
        "spaceId": BEREICH_ID,
        "version": { "number": version,
                     "authorId": KONTO,
                     "createdAt": "2026-11-20T10:15:00Z" },
        "body": { "storage": { "representation": "storage", "value": inhalt } },
        "_links": { "webui": format!("/spaces/{BEREICH}/pages/{id}/Anmeldung") }
    })
}

/// Eine Seite mit den Abweichungen, die ein Test verlangt.
fn seite_json_gefiltert(id: &str, version: i64, inhalt: &str, zustand: &Zustand) -> Value {
    let mut seite = seite_json(id, version, inhalt);
    if zustand.seite_ohne_inhalt.load(Ordering::SeqCst) {
        seite.as_object_mut().unwrap().remove("body");
    }
    if zustand.seite_ohne_titel.load(Ordering::SeqCst) {
        seite.as_object_mut().unwrap().remove("title");
    }
    if zustand.seite_ohne_status.load(Ordering::SeqCst) {
        seite.as_object_mut().unwrap().remove("status");
    }
    seite
}

/// Eine Seite in einem **anderen** Bereich (für `eltern`).
fn fremde_seite_json(id: &str) -> Value {
    let mut seite = seite_json(id, 1, "<p>Anderer Bereich</p>");
    seite["spaceId"] = json!(BEREICH_2_ID);
    seite["_links"]["webui"] = json!(format!("/spaces/{BEREICH_2}/pages/{id}/Fremd"));
    seite
}

/// Eine Seite aus einer Liste, höchstens zwei je Antwort, mit `_links.next`.
fn blaettern(alle: &[Value], abfrage: &BTreeMap<String, String>, pfad: &str) -> Antwort {
    let limit = match abfrage.get("limit") {
        Some(limit) => match limit.parse::<usize>() {
            Ok(limit) if limit > 0 => limit,
            _ => return fehler(400, "limit muss eine positive Zahl sein.".to_string()),
        },
        None => return fehler(400, "Parameter „limit“ fehlt.".to_string()),
    };
    let cursor: usize = match abfrage.get("cursor") {
        Some(cursor) => match cursor.parse::<usize>() {
            Ok(cursor) => cursor,
            Err(_) => return fehler(400, "cursor ist keine Zahl.".to_string()),
        },
        None => 0,
    };
    if cursor > alle.len() {
        return fehler(400, "cursor liegt hinter dem Ende.".to_string());
    }

    // Die Attrappe liefert höchstens zwei Einträge je Seite – so wird die
    // Folgeseite auch bei kleinem `anzahl` geprüft.
    let seite = limit.min(2);
    let eintraege: Vec<Value> = alle.iter().skip(cursor).take(seite).cloned().collect();
    let mut antwort = json!({ "results": eintraege });
    let naechster = cursor + eintraege.len();
    if naechster < alle.len() {
        // Die Suche (v1) braucht ihre `cql` in der Folgeseite.
        let cql = abfrage
            .get("cql")
            .map(|cql| format!("&cql={}", kodieren(cql)))
            .unwrap_or_default();
        antwort["_links"] = json!({
            "next": format!("{pfad}?limit={limit}{cql}&cursor={naechster}")
        });
    }
    Antwort::Json(antwort)
}

/// Gilt der `Authorization`-Kopf für einen der beiden Token?
fn anmeldung_gilt(kopf: &HeaderMap) -> bool {
    let Some(wert) = kopfzeile(kopf, "authorization") else {
        return false;
    };
    let Some(kodiert) = wert.strip_prefix("Basic ") else {
        return false;
    };
    [TOKEN_A, TOKEN_B]
        .iter()
        .any(|token| kodiert == basic(&format!("{EMAIL}:{token}")))
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

/// Einen Wert für eine Adresse kodieren (wie der Client).
fn kodieren(wert: &str) -> String {
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

/// Die Attrappe für einen Test, der nur die Aufrufe zählt.
pub fn aufruf_pfade(aufzeichnung: &Aufzeichnung) -> Vec<String> {
    aufzeichnung
        .alle()
        .into_iter()
        .map(|aufruf| format!("{} {}", aufruf.methode, aufruf.pfad))
        .collect()
}

// ------------------------------------------------- Antwort nicht lesbar

/// Eine Seite, wie sie der kaputte Server auf ein `GET` antwortet.
const KAPUTTE_SEITE: &str = concat!(
    "{\"id\":\"123\",\"title\":\"Anmeldung\",\"status\":\"current\",\"spaceId\":\"10\",",
    "\"version\":{\"number\":3},",
    "\"body\":{\"storage\":{\"representation\":\"storage\",\"value\":\"<p>Alt</p>\"}}}"
);

/// Ein winziger Server, dessen Antwort **kürzer** ist als ihr `Content-Length`.
///
/// Damit scheitert `text().await` – so lässt sich prüfen, dass „nicht lesbar“
/// ein Fehler ist und nie als leerer Rumpf gilt (Korrekturen 3).
fn kaputter_server(nur_aenderungen: bool) -> String {
    let hoerer = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("kein freier Port");
    let adresse = format!("http://127.0.0.1:{}", hoerer.local_addr().unwrap().port());
    std::thread::spawn(move || {
        for verbindung in hoerer.incoming() {
            let Ok(mut verbindung) = verbindung else {
                break;
            };
            std::thread::spawn(move || {
                let anfrage = anfrage_lesen(&mut verbindung);
                let lesend = anfrage.starts_with("GET");
                let antwort = if nur_aenderungen && lesend {
                    format!(
                        "HTTP/1.1 200 OK\r\nconnection: close\r\ncontent-type: application/json\r\ncontent-length: \
                         {}\r\n\r\n{KAPUTTE_SEITE}",
                        KAPUTTE_SEITE.len()
                    )
                } else {
                    // Der Rumpf ist kürzer als angekündigt: das Lesen scheitert.
                    "HTTP/1.1 200 OK\r\nconnection: close\r\ncontent-type: application/json\r\ncontent-length: \
                     1000\r\n\r\n{\"results\":["
                        .to_string()
                };
                let _ = std::io::Write::write_all(&mut verbindung, antwort.as_bytes());
                let _ = std::io::Write::flush(&mut verbindung);
                // Geordnet schließen: erst das eigene Ende, dann warten, bis
                // die Gegenseite fertig ist. Sonst bricht Windows hart ab (RST)
                // und der Client sieht die Antwort nie.
                let _ = verbindung.shutdown(std::net::Shutdown::Write);
                let mut rest = [0u8; 1024];
                while matches!(std::io::Read::read(&mut verbindung, &mut rest), Ok(n) if n > 0) {}
            });
        }
    });
    adresse
}

/// Die ganze Anfrage lesen: Kopf und so viel Rumpf, wie `Content-Length` nennt.
/// Bleiben Bytes ungelesen, verwirft Windows beim Schließen die Antwort.
fn anfrage_lesen(verbindung: &mut std::net::TcpStream) -> String {
    let _ = verbindung.set_read_timeout(Some(std::time::Duration::from_secs(5)));
    let mut daten = Vec::new();
    let mut puffer = [0u8; 8192];
    loop {
        let gelesen = std::io::Read::read(verbindung, &mut puffer).unwrap_or(0);
        if gelesen == 0 {
            break;
        }
        daten.extend_from_slice(&puffer[..gelesen]);
        let text = String::from_utf8_lossy(&daten).to_ascii_lowercase();
        if let Some(ende) = daten.windows(4).position(|teil| teil == b"\r\n\r\n") {
            let laenge = text
                .lines()
                .find_map(|zeile| zeile.strip_prefix("content-length:"))
                .and_then(|wert| wert.trim().parse::<usize>().ok())
                .unwrap_or(0);
            if daten.len() >= ende + 4 + laenge {
                break;
            }
        }
    }
    String::from_utf8_lossy(&daten).to_string()
}

/// Jede Antwort ist nicht lesbar.
pub fn kaputte_antwort() -> String {
    kaputter_server(false)
}

/// Nur die Antwort auf eine Änderung ist nicht lesbar (das `GET` davor klappt).
pub fn kaputte_aenderung() -> String {
    kaputter_server(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn der_authorization_kopf_wird_geprueft() {
        let erwartet = basic(&format!("{EMAIL}:{TOKEN_A}"));
        let mut kopf = HeaderMap::new();
        kopf.insert(
            "authorization",
            format!("Basic {erwartet}").parse().unwrap(),
        );
        assert!(anmeldung_gilt(&kopf));

        // Die E-Mail allein genügt nicht: das Token ist das Passwort.
        let ohne_token = basic(EMAIL);
        let mut falsch = HeaderMap::new();
        falsch.insert(
            "authorization",
            format!("Basic {ohne_token}").parse().unwrap(),
        );
        assert!(!anmeldung_gilt(&falsch));
        assert!(!anmeldung_gilt(&HeaderMap::new()));
    }

    #[test]
    fn die_abfrage_wird_dekodiert() {
        let abfrage = abfrage_lesen("limit=10&keys=ENG&cql=space%20%3D%20%22ENG%22&x=a+b");
        assert_eq!(abfrage.get("limit").map(String::as_str), Some("10"));
        assert_eq!(
            abfrage.get("cql").map(String::as_str),
            Some("space = \"ENG\"")
        );
        assert_eq!(abfrage.get("x").map(String::as_str), Some("a b"));
    }

    #[test]
    fn eine_folgeseite_traegt_limit_und_cursor() {
        let alle: Vec<Value> = (0..5).map(|nummer| json!({ "id": nummer })).collect();
        let mut abfrage = BTreeMap::new();
        abfrage.insert("limit".to_string(), "25".to_string());
        let Antwort::Json(wert) = blaettern(&alle, &abfrage, "/wiki/api/v2/spaces") else {
            panic!("keine Liste");
        };
        assert_eq!(wert["results"].as_array().unwrap().len(), 2);
        assert_eq!(
            wert["_links"]["next"],
            json!("/wiki/api/v2/spaces?limit=25&cursor=2")
        );
    }
}
