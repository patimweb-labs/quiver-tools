// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Zephyr-Attrappe (SPEC §7): ein kleines axum-Programm, das die benutzten
//! Endpunkte der REST-API v2 beantwortet und jeden Aufruf mitschreibt.
//!
//! Kein Test spricht je mit einem echten Zephyr. Die Attrappe ist **streng**:
//! unbekannte Pfade und Abfrageparameter sind `400`, ein falscher
//! `Authorization`-Kopf ist `401`, Pflichtfelder im Rumpf werden geprüft, und
//! jeder Rumpf darf **nur** Felder aus dem Schema der OpenAPI-Datei enthalten.
//! Antworten haben **genau** die Formen der OpenAPI-Datei: Verweise
//! (`project`, `status`, `priority`, `testCase`, `testCycle`, `environment`,
//! `folder`) tragen nur `id` und `self` – keinen `key` und keinen `name`.
//!
//! Sie kann außerdem `401`, `403`, `429`, `503`, eine Weiterleitung (`302`) und
//! eine Folgeseite mit fremdem Host melden – damit jede Sicherheitsregel aus
//! [12-atlassian-gemeinsam.md](../../docs/aufgaben/12-atlassian-gemeinsam.md)
//! einzeln prüfbar ist. Mit [`Attrappe::seite_max`] liefert jede Liste kleinere
//! Seiten, damit „mehr als eine Seite“ prüfbar ist.
//!
//! Zwei Zugänge teilen sich **eine** Attrappe: beide Token gelten, damit
//! „mehrere Zugänge“ ohne zweiten Server geprüft werden kann. Alle Namen,
//! Nummern und Token sind erfunden.

#![allow(dead_code)]

pub mod defekt;

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use axum::Router;
use serde_json::{json, Value};

/// Token des ersten Zugangs. Kein echtes Geheimnis.
pub const TOKEN_A: &str = "zephyr-token-attrappe-4711";
/// Token des zweiten Zugangs. Kein echtes Geheimnis.
pub const TOKEN_B: &str = "zephyr-token-attrappe-4712";
/// Das Projekt der Attrappe.
pub const PROJEKT: &str = "QA";
/// Die Id des Projekts.
pub const PROJEKT_ID: i64 = 1;
/// Ein zweites Projekt (für `nur` und die Auswahl).
pub const PROJEKT_2: &str = "OPS";
/// Die Id des zweiten Projekts.
pub const PROJEKT_2_ID: i64 = 2;
/// Der erste Testfall.
pub const TESTFALL: &str = "QA-T1";
/// Die Id des ersten Testfalls.
pub const TESTFALL_ID: i64 = 11;
/// Der zweite Testfall (mit Skript statt Schritten).
pub const TESTFALL_2: &str = "QA-T2";
/// Ein Testfall im zweiten Projekt.
pub const TESTFALL_OPS: &str = "OPS-T1";
/// Der Testzyklus der Attrappe (`[A-Z]+-R[0-9]+`, wie die OpenAPI es verlangt).
pub const ZYKLUS: &str = "QA-R1";
/// Die Id des Testzyklus.
pub const ZYKLUS_ID: i64 = 31;
/// Ein Testzyklus im zweiten Projekt.
pub const ZYKLUS_OPS: &str = "OPS-R1";
/// Die Ausführung der Attrappe (Schlüssel).
pub const AUSFUEHRUNG: &str = "QA-E1";
/// Die Id der Ausführung.
pub const AUSFUEHRUNG_ID: i64 = 101;
/// Eine Ausführung im zweiten Projekt.
pub const AUSFUEHRUNG_OPS: &str = "OPS-E1";
/// Der Ordner der Attrappe.
pub const ORDNER: u32 = 12;
/// Die Umgebung der Attrappe.
pub const UMGEBUNG: i64 = 5;
/// Der Name der Umgebung.
pub const UMGEBUNG_NAME: &str = "Test";
/// Ein Status der Attrappe.
pub const STATUS_PASS: &str = "Pass";

/// Die erfundene API-Basis, die in `self`-Verweisen steht (wird nie geholt).
const BASIS: &str = "https://zephyr.example.test/v2";

/// Ein Aufruf, den die Attrappe gesehen hat.
#[derive(Debug, Clone)]
pub struct Aufruf {
    /// Pfad, z. B. `/v2/testcases`.
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
    /// Das Token gilt nicht: Antwort `401`.
    token_ablehnen: AtomicBool,
    /// Das Token darf nicht: Antwort `403`.
    keine_rechte: AtomicBool,
    /// So viele der nächsten Aufrufe werden mit `429` abgelehnt.
    zu_viele: AtomicUsize,
    /// So viele der nächsten Aufrufe werden mit `503` abgelehnt.
    nicht_verfuegbar: AtomicUsize,
    /// Der `Retry-After`-Wert für `429`/`503` in Sekunden.
    wartezeit: AtomicU64,
    /// Jeder Aufruf antwortet mit `302` auf einen fremden Host.
    weiterleitung: AtomicBool,
    /// Listen nennen eine Folgeseite auf einem fremden Host.
    fremde_folgeseite: AtomicBool,
    /// So viele Einträge hat eine Seite höchstens.
    seite_max: AtomicUsize,
    /// Dieser Testfall scheitert beim Anlegen einer Ausführung (`500`).
    fehler_bei_testfall: Mutex<Option<String>>,
    /// `http://127.0.0.1:<port>` – für Folgeseiten.
    basis: Mutex<String>,
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
            token_ablehnen: AtomicBool::new(false),
            keine_rechte: AtomicBool::new(false),
            zu_viele: AtomicUsize::new(0),
            nicht_verfuegbar: AtomicUsize::new(0),
            wartezeit: AtomicU64::new(0),
            weiterleitung: AtomicBool::new(false),
            fremde_folgeseite: AtomicBool::new(false),
            seite_max: AtomicUsize::new(200),
            fehler_bei_testfall: Mutex::new(None),
            basis: Mutex::new(String::new()),
        });

        let app = Router::new()
            .fallback(bearbeiten)
            .with_state(zustand.clone());
        let hoerer =
            tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
                .await
                .expect("kein freier Port");
        let adresse = hoerer.local_addr().unwrap();
        let basis = format!("http://127.0.0.1:{}", adresse.port());
        *griff(&zustand.basis) = basis.clone();
        let ende = tokio::spawn(async move {
            let _ = axum::serve(hoerer, app).await;
        });

        Attrappe {
            adresse: basis,
            aufzeichnung,
            zustand,
            ende,
        }
    }

    /// Die Basisadresse inklusive `/v2`, wie sie im Zugang steht.
    pub fn basis_url(&self) -> String {
        format!("{}/v2", self.adresse)
    }

    /// Jeder Aufruf wird mit `401` abgelehnt (ungültiges Token).
    pub fn token_ablehnen(&self) {
        self.zustand.token_ablehnen.store(true, Ordering::SeqCst);
    }

    /// Jeder Aufruf wird mit `403` abgelehnt (keine Rechte).
    pub fn keine_rechte(&self) {
        self.zustand.keine_rechte.store(true, Ordering::SeqCst);
    }

    /// Die nächsten `anzahl` Aufrufe werden mit `429` abgelehnt.
    pub fn zu_viele_anfragen(&self, anzahl: usize, wartezeit_sekunden: u64) {
        self.zustand.zu_viele.store(anzahl, Ordering::SeqCst);
        self.zustand
            .wartezeit
            .store(wartezeit_sekunden, Ordering::SeqCst);
    }

    /// Die nächsten `anzahl` Aufrufe werden mit `503` abgelehnt.
    pub fn nicht_verfuegbar(&self, anzahl: usize, wartezeit_sekunden: u64) {
        self.zustand
            .nicht_verfuegbar
            .store(anzahl, Ordering::SeqCst);
        self.zustand
            .wartezeit
            .store(wartezeit_sekunden, Ordering::SeqCst);
    }

    /// Jeder Aufruf antwortet mit einer Weiterleitung auf einen fremden Host.
    pub fn weiterleitung(&self) {
        self.zustand.weiterleitung.store(true, Ordering::SeqCst);
    }

    /// Listen nennen eine Folgeseite auf einem fremden Host.
    pub fn fremde_folgeseite(&self) {
        self.zustand.fremde_folgeseite.store(true, Ordering::SeqCst);
    }

    /// So viele Einträge hat eine Seite ab jetzt höchstens (mindestens 1).
    pub fn seite_max(&self, anzahl: usize) {
        self.zustand
            .seite_max
            .store(anzahl.max(1), Ordering::SeqCst);
    }

    /// Das Anlegen einer Ausführung für diesen Testfall scheitert (`500`).
    pub fn fehler_bei(&self, testfall: &str) {
        *griff(&self.zustand.fehler_bei_testfall) = Some(testfall.to_string());
    }
}

// ------------------------------------------------------------------ Aufruf

async fn bearbeiten(
    State(zustand): State<Arc<Zustand>>,
    anfrage: axum::extract::Request,
) -> Response {
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

    // 429/503 vor allem anderen: so lässt sich prüfen, ob nur lesende Aufrufe
    // wiederholen.
    let wartezeit = zustand.wartezeit.load(Ordering::SeqCst).to_string();
    let offen = zustand.zu_viele.load(Ordering::SeqCst);
    if offen > 0 {
        zustand.zu_viele.store(offen - 1, Ordering::SeqCst);
        return antwort_mit(
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", wartezeit.as_str())],
            "429 Too Many Requests",
        );
    }
    let offen = zustand.nicht_verfuegbar.load(Ordering::SeqCst);
    if offen > 0 {
        zustand.nicht_verfuegbar.store(offen - 1, Ordering::SeqCst);
        return antwort_mit(
            StatusCode::SERVICE_UNAVAILABLE,
            [("retry-after", wartezeit.as_str())],
            "503 Service Unavailable",
        );
    }

    if zustand.weiterleitung.load(Ordering::SeqCst) {
        return (
            StatusCode::FOUND,
            [("location", "https://fremder.example.test/v2/projects")],
            "",
        )
            .into_response();
    }

    if zustand.token_ablehnen.load(Ordering::SeqCst) || !anmeldung_gilt(&kopf) {
        return fehler(
            StatusCode::UNAUTHORIZED,
            "Bearer token is missing or invalid",
        );
    }

    if zustand.keine_rechte.load(Ordering::SeqCst) {
        return fehler(
            StatusCode::FORBIDDEN,
            "The token has no permission for this project",
        );
    }

    match beantworten(&zustand, &pfad, &methode, &abfrage, &rumpf) {
        Ok(antwort) => antwort,
        Err(MockFehler::Anfrage(text)) => fehler(StatusCode::BAD_REQUEST, &text),
        Err(MockFehler::NichtGefunden(text)) => fehler(StatusCode::NOT_FOUND, &text),
    }
}

/// Warum die Attrappe eine Anfrage ablehnt.
enum MockFehler {
    /// Falscher Aufruf: `400`.
    Anfrage(String),
    /// Gibt es nicht: `404`.
    NichtGefunden(String),
}

impl From<String> for MockFehler {
    fn from(text: String) -> MockFehler {
        MockFehler::Anfrage(text)
    }
}

fn fehler(status: StatusCode, text: &str) -> Response {
    (status, Json(json!({ "message": text }))).into_response()
}

fn antwort_mit<const N: usize>(
    status: StatusCode,
    kopfzeilen: [(&str, &str); N],
    text: &str,
) -> Response {
    let mut antwort = (status, text.to_string()).into_response();
    for (name, wert) in kopfzeilen {
        if let Ok(name) = name.parse::<axum::http::HeaderName>() {
            if let Ok(wert) = wert.parse::<axum::http::HeaderValue>() {
                antwort.headers_mut().insert(name, wert);
            }
        }
    }
    antwort
}

// --------------------------------------------------------------- Beantworten

fn beantworten(
    zustand: &Arc<Zustand>,
    pfad: &str,
    methode: &str,
    abfrage: &BTreeMap<String, String>,
    rumpf: &str,
) -> Result<Response, MockFehler> {
    let rest = pfad.strip_prefix("/v2/").ok_or_else(|| {
        MockFehler::Anfrage(format!(
            "Unbekannter Pfad {pfad} – die Attrappe kennt nur /v2/…"
        ))
    })?;
    let teile: Vec<&str> = rest.trim_matches('/').split('/').collect();

    match (teile.as_slice(), methode) {
        // --------------------------------------------------------- Projekte
        (["projects"], "GET") => {
            pruefe_abfrage(abfrage, &["maxResults", "startAt"])?;
            Ok(seite(zustand, "projects", projekte(), abfrage))
        }
        (["projects", kennung], "GET") => {
            pruefe_abfrage(abfrage, &[])?;
            let projekt = projekt(kennung)
                .ok_or_else(|| MockFehler::NichtGefunden(format!("Projekt {kennung} fehlt")))?;
            Ok(Json(projekt).into_response())
        }
        // -------------------------------------------------------- Testfälle
        (["testcases"], "GET") => {
            pruefe_abfrage(
                abfrage,
                &["projectKey", "folderId", "maxResults", "startAt"],
            )?;
            let projekt = pflicht_feld(abfrage, "projectKey")?;
            Ok(seite(zustand, "testcases", testfaelle(&projekt), abfrage))
        }
        (["testcases"], "POST") => {
            pruefe_abfrage(abfrage, &[])?;
            let rumpf = rumpf_json(rumpf)?;
            // Felder aus `TestCaseInput`, Pflichtfelder `name` und `projectKey`.
            pruefe_rumpf(
                &rumpf,
                &[
                    "projectKey",
                    "name",
                    "objective",
                    "precondition",
                    "estimatedTime",
                    "componentId",
                    "priorityName",
                    "statusName",
                    "folderId",
                    "ownerId",
                    "labels",
                    "customFields",
                ],
                &["projectKey", "name"],
            )?;
            let schluessel = text(&rumpf, "projectKey")
                .ok_or_else(|| MockFehler::Anfrage("projectKey fehlt".to_string()))?;
            let projekt = projekt(&schluessel)
                .ok_or_else(|| MockFehler::NichtGefunden(format!("Projekt {schluessel} fehlt")))?;
            // `201 KeyedCreatedResource`: `id`, `self`, `key` – sonst nichts.
            let angelegt = json!({
                "id": 13,
                "self": format!("{BASIS}/testcases/13"),
                "key": format!("{}-T3", text(&projekt, "key").unwrap_or_default())
            });
            Ok((StatusCode::CREATED, Json(angelegt)).into_response())
        }
        (["testcases", schluessel], "GET") => {
            pruefe_abfrage(abfrage, &[])?;
            let testfall = testfall(schluessel)
                .ok_or_else(|| MockFehler::NichtGefunden(format!("Testfall {schluessel} fehlt")))?;
            Ok(Json(testfall).into_response())
        }
        (["testcases", schluessel, "testscript"], "GET") => {
            pruefe_abfrage(abfrage, &[])?;
            match skript(schluessel) {
                Some(skript) => Ok(Json(skript).into_response()),
                // `422 TestCaseHasStepsError`: der Testfall hat Schritte.
                None => {
                    testfall(schluessel).ok_or_else(|| {
                        MockFehler::NichtGefunden(format!("Testfall {schluessel} fehlt"))
                    })?;
                    Ok(fehler(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "TestCaseHasStepsError",
                    ))
                }
            }
        }
        (["testcases", schluessel, "teststeps"], "GET") => {
            pruefe_abfrage(abfrage, &["maxResults", "startAt"])?;
            match schritte(schluessel) {
                Some(schritte) => Ok(seite(zustand, "teststeps", schritte, abfrage)),
                // `422 TestCaseIsScriptError`: der Testfall ist ein Skript.
                None => {
                    testfall(schluessel).ok_or_else(|| {
                        MockFehler::NichtGefunden(format!("Testfall {schluessel} fehlt"))
                    })?;
                    Ok(fehler(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "TestCaseIsScriptError",
                    ))
                }
            }
        }
        // -------------------------------------------------------- Testzyklen
        (["testcycles"], "GET") => {
            pruefe_abfrage(abfrage, &["projectKey", "maxResults", "startAt"])?;
            let projekt = pflicht_feld(abfrage, "projectKey")?;
            Ok(seite(zustand, "testcycles", testzyklen(&projekt), abfrage))
        }
        (["testcycles", kennung], "GET") => {
            pruefe_abfrage(abfrage, &[])?;
            let zyklus = testzyklus(kennung)
                .ok_or_else(|| MockFehler::NichtGefunden(format!("Testzyklus {kennung} fehlt")))?;
            Ok(Json(zyklus).into_response())
        }
        // ------------------------------------------------------- Ausführungen
        (["testexecutions"], "GET") => {
            pruefe_abfrage(
                abfrage,
                &[
                    "projectKey",
                    "testCycle",
                    "testCase",
                    "onlyLastExecutions",
                    "maxResults",
                    "startAt",
                ],
            )?;
            let projekt = pflicht_feld(abfrage, "projectKey")?;
            if let Some(zyklus) = abfrage.get("testCycle") {
                // Die API nimmt hier einen **Schlüssel**, keine Id.
                if !ist_zyklusschluessel(zyklus) {
                    return Err(MockFehler::Anfrage(format!(
                        "testCycle muss ein Zyklusschlüssel sein, nicht „{zyklus}“"
                    )));
                }
            }
            let mut eintraege = ausfuehrungen(&projekt);
            if let Some(zyklus) = abfrage.get("testCycle") {
                eintraege.retain(|ausfuehrung| {
                    text(ausfuehrung, "testCycle.id").as_deref()
                        == zyklus_id(zyklus).map(|id| id.to_string()).as_deref()
                });
            }
            Ok(seite(zustand, "testexecutions", eintraege, abfrage))
        }
        (["testexecutions"], "POST") => {
            pruefe_abfrage(abfrage, &[])?;
            let rumpf = rumpf_json(rumpf)?;
            // Felder aus `TestExecutionInput`; `statusName` ist Pflicht.
            pruefe_rumpf(
                &rumpf,
                &[
                    "projectKey",
                    "testCaseKey",
                    "testCycleKey",
                    "statusName",
                    "testScriptResults",
                    "environmentName",
                    "actualEndDate",
                    "executionTime",
                    "executedById",
                    "assignedToId",
                    "comment",
                    "customFields",
                    "automated",
                ],
                &["projectKey", "testCaseKey", "testCycleKey", "statusName"],
            )?;
            let projekt = text(&rumpf, "projectKey")
                .ok_or_else(|| MockFehler::Anfrage("projectKey fehlt".to_string()))?;
            if self::projekt(&projekt).is_none() {
                return Err(MockFehler::NichtGefunden(format!(
                    "Projekt {projekt} fehlt"
                )));
            }
            let testfall = text(&rumpf, "testCaseKey")
                .ok_or_else(|| MockFehler::Anfrage("testCaseKey fehlt".to_string()))?;
            if self::testfall(&testfall).is_none() {
                return Err(MockFehler::NichtGefunden(format!(
                    "Testfall {testfall} fehlt"
                )));
            }
            let zyklus = text(&rumpf, "testCycleKey")
                .ok_or_else(|| MockFehler::Anfrage("testCycleKey fehlt".to_string()))?;
            if self::testzyklus(&zyklus).is_none() {
                return Err(MockFehler::NichtGefunden(format!(
                    "Testzyklus {zyklus} fehlt"
                )));
            }
            if griff(&zustand.fehler_bei_testfall).as_deref() == Some(testfall.as_str()) {
                return Ok(fehler(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Die Attrappe lehnt diesen Testfall ab",
                ));
            }
            let status = text(&rumpf, "statusName")
                .ok_or_else(|| MockFehler::Anfrage("statusName fehlt".to_string()))?;
            if status_id(&projekt, "TEST_EXECUTION", &status).is_none() {
                // `422 TestExecutionStatusNameNotFoundError`.
                return Ok(fehler(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "TestExecutionStatusNameNotFoundError",
                ));
            }
            // `201 CreatedResource`: nur `id` und `self` – keinen Schlüssel.
            let mut angelegt = json!({
                "id": 200 + testfaelle_nr(&testfall),
                "self": format!("{BASIS}/testexecutions/{}", 200 + testfaelle_nr(&testfall)),
                "project": projekt_link(projekt_id(&projekt).unwrap_or(PROJEKT_ID)),
                "testCase": json!({
                    "id": testfall_id(&testfall).unwrap_or(0),
                    "self": format!("{BASIS}/testcases/{testfall}/versions/1")
                }),
                "testCycle": zyklus_link(zyklus_id(&zyklus).unwrap_or(0)),
                "testExecutionStatus": status_link(
                    status_id(&projekt, "TEST_EXECUTION", &status).unwrap_or(0)
                )
            });
            if let Some(umgebung) = text(&rumpf, "environmentName") {
                if umgebung_id(&projekt, &umgebung).is_none() {
                    return Ok(fehler(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "TestExecutionEnvironmentNameNotFoundError",
                    ));
                }
                angelegt["environment"] =
                    umgebung_link(umgebung_id(&projekt, &umgebung).unwrap_or(0));
            }
            Ok((StatusCode::CREATED, Json(angelegt)).into_response())
        }
        (["testexecutions", kennung], "GET") => {
            pruefe_abfrage(abfrage, &["includeStepLinks"])?;
            let ausfuehrung = ausfuehrung(kennung)
                .ok_or_else(|| MockFehler::NichtGefunden(format!("Ausführung {kennung} fehlt")))?;
            Ok(Json(ausfuehrung).into_response())
        }
        (["testexecutions", kennung], "PUT") => {
            pruefe_abfrage(abfrage, &[])?;
            if ausfuehrung(kennung).is_none() {
                return Err(MockFehler::NichtGefunden(format!(
                    "Ausführung {kennung} fehlt"
                )));
            }
            let rumpf = rumpf_json(rumpf)?;
            // Genau `TestExecutionUpdate`: alles andere ist ein Fehler. So
            // fällt auf, wenn ein Werkzeug Felder schickt, die es nicht gibt.
            pruefe_rumpf(
                &rumpf,
                &[
                    "statusName",
                    "environmentName",
                    "actualEndDate",
                    "executionTime",
                    "automated",
                    "executedById",
                    "assignedToId",
                    "comment",
                ],
                &[],
            )?;
            // `200` ohne Rumpf: nach einer Änderung ist auch das Erfolg.
            Ok(StatusCode::OK.into_response())
        }
        // ------------------------------------------------------------ Status
        (["statuses"], "GET") => {
            pruefe_abfrage(
                abfrage,
                &["projectKey", "statusType", "maxResults", "startAt"],
            )?;
            let projekt = pflicht_feld(abfrage, "projectKey")?;
            let art = match abfrage.get("statusType") {
                Some(art) => {
                    if !["TEST_CASE", "TEST_PLAN", "TEST_CYCLE", "TEST_EXECUTION"]
                        .contains(&art.as_str())
                    {
                        return Err(MockFehler::Anfrage(format!(
                            "statusType „{art}“ kennt die API nicht"
                        )));
                    }
                    art.clone()
                }
                None => "TEST_CASE".to_string(),
            };
            Ok(seite(
                zustand,
                "statuses",
                status_liste(&projekt, &art),
                abfrage,
            ))
        }
        (["statuses", kennung], "GET") => {
            pruefe_abfrage(abfrage, &[])?;
            let status = [PROJEKT, PROJEKT_2]
                .iter()
                .flat_map(|projekt| {
                    ["TEST_CASE", "TEST_CYCLE", "TEST_EXECUTION"]
                        .iter()
                        .flat_map(move |art| status_liste(projekt, art))
                })
                .find(|status| text(status, "id").as_deref() == Some(kennung))
                .ok_or_else(|| MockFehler::NichtGefunden(format!("Status {kennung} fehlt")))?;
            Ok(Json(status).into_response())
        }
        // ------------------------------------------------------- Umgebungen
        (["environments"], "GET") => {
            pruefe_abfrage(abfrage, &["projectKey", "maxResults", "startAt"])?;
            let projekt = pflicht_feld(abfrage, "projectKey")?;
            Ok(seite(
                zustand,
                "environments",
                umgebungen(&projekt),
                abfrage,
            ))
        }
        (["environments", kennung], "GET") => {
            pruefe_abfrage(abfrage, &[])?;
            let umgebung = [PROJEKT, PROJEKT_2]
                .iter()
                .flat_map(|projekt| umgebungen(projekt))
                .find(|umgebung| text(umgebung, "id").as_deref() == Some(kennung))
                .ok_or_else(|| MockFehler::NichtGefunden(format!("Umgebung {kennung} fehlt")))?;
            Ok(Json(umgebung).into_response())
        }
        // ------------------------------------------------------ Prioritäten
        (["priorities"], "GET") => {
            pruefe_abfrage(abfrage, &["projectKey", "maxResults", "startAt"])?;
            let projekt = pflicht_feld(abfrage, "projectKey")?;
            Ok(seite(
                zustand,
                "priorities",
                prioritaeten(&projekt),
                abfrage,
            ))
        }
        // ----------------------------------------------------------- Ordner
        (["folders"], "GET") => {
            pruefe_abfrage(
                abfrage,
                &["projectKey", "folderType", "maxResults", "startAt"],
            )?;
            let projekt = pflicht_feld(abfrage, "projectKey")?;
            let art = pflicht_feld(abfrage, "folderType")?;
            if !["TEST_CASE", "TEST_PLAN", "TEST_CYCLE"].contains(&art.as_str()) {
                return Err(MockFehler::Anfrage(format!(
                    "folderType „{art}“ kennt die API nicht"
                )));
            }
            Ok(seite(zustand, "folders", ordner(&projekt, &art), abfrage))
        }
        _ => Err(MockFehler::Anfrage(format!(
            "Unbekannter Pfad {methode} {pfad} – die Attrappe kennt ihn nicht"
        ))),
    }
}

/// Eine Seite einer Liste bauen: `values`, `total`, `isLast`, `next`.
fn seite(
    zustand: &Arc<Zustand>,
    endpoint: &str,
    alle: Vec<Value>,
    abfrage: &BTreeMap<String, String>,
) -> Response {
    let start = abfrage
        .get("startAt")
        .and_then(|wert| wert.parse::<usize>().ok())
        .unwrap_or(0);
    let grenze = zustand.seite_max.load(Ordering::SeqCst).max(1);
    let max = abfrage
        .get("maxResults")
        .and_then(|wert| wert.parse::<usize>().ok())
        .unwrap_or(25)
        .clamp(1, 1000)
        .min(grenze);
    let gesamt = alle.len();
    let werte: Vec<Value> = alle.into_iter().skip(start).take(max).collect();
    let is_last = start + werte.len() >= gesamt;

    let fremd = zustand.fremde_folgeseite.load(Ordering::SeqCst);
    let basis = griff(&zustand.basis).clone();
    let next = if is_last {
        Value::Null
    } else if fremd {
        json!(format!(
            "https://fremder.example.test/v2/{endpoint}?startAt=0"
        ))
    } else {
        json!(format!(
            "{basis}/v2/{endpoint}?startAt={}",
            start + werte.len()
        ))
    };

    Json(json!({
        "values": werte,
        "total": gesamt,
        "isLast": is_last,
        "next": next,
        "startAt": start,
        "maxResults": max,
    }))
    .into_response()
}

/// Nur bekannte Abfrageparameter; alles andere ist ein Fehler.
fn pruefe_abfrage(abfrage: &BTreeMap<String, String>, erlaubt: &[&str]) -> Result<(), MockFehler> {
    for name in abfrage.keys() {
        if !erlaubt.contains(&name.as_str()) {
            return Err(MockFehler::Anfrage(format!(
                "Unbekannter Abfrageparameter „{name}“"
            )));
        }
    }
    for name in ["maxResults", "startAt"] {
        if let Some(wert) = abfrage.get(name) {
            wert.parse::<u32>()
                .map_err(|_| format!("{name} muss eine Zahl sein, nicht „{wert}“"))?;
        }
    }
    Ok(())
}

/// Ein Pflichtparameter der Abfrage.
fn pflicht_feld(abfrage: &BTreeMap<String, String>, name: &str) -> Result<String, MockFehler> {
    abfrage
        .get(name)
        .map(|wert| wert.trim().to_string())
        .filter(|wert| !wert.is_empty())
        .ok_or_else(|| MockFehler::Anfrage(format!("Der Abfrageparameter „{name}“ fehlt")))
}

/// Den Rumpf lesen und gegen die erlaubten Felder prüfen.
fn rumpf_json(rumpf: &str) -> Result<Value, MockFehler> {
    let wert: Value = serde_json::from_str(rumpf)
        .map_err(|_| MockFehler::Anfrage("Der Rumpf ist kein JSON".to_string()))?;
    if !wert.is_object() {
        return Err(MockFehler::Anfrage("Der Rumpf ist kein Objekt".to_string()));
    }
    Ok(wert)
}

fn pruefe_rumpf(rumpf: &Value, erlaubt: &[&str], pflicht: &[&str]) -> Result<(), MockFehler> {
    let objekt = rumpf
        .as_object()
        .ok_or_else(|| MockFehler::Anfrage("Der Rumpf ist kein Objekt".to_string()))?;
    for name in objekt.keys() {
        if !erlaubt.contains(&name.as_str()) {
            return Err(MockFehler::Anfrage(format!(
                "Unbekanntes Feld „{name}“ im Rumpf – das Schema kennt es nicht"
            )));
        }
    }
    for name in pflicht {
        if text(rumpf, name).is_none() {
            return Err(MockFehler::Anfrage(format!(
                "Das Pflichtfeld „{name}“ fehlt"
            )));
        }
    }
    Ok(())
}

// --------------------------------------------------------------- Testdaten

/// Ein Verweis mit nur `id` und `self` – so, wie die OpenAPI es vorgibt.
fn verweis(pfad: &str, id: i64) -> Value {
    json!({ "id": id, "self": format!("{BASIS}/{pfad}/{id}") })
}

fn projekt_link(id: i64) -> Value {
    verweis("projects", id)
}

fn status_link(id: i64) -> Value {
    verweis("statuses", id)
}

fn priority_link(id: i64) -> Value {
    verweis("priorities", id)
}

fn folder_link(id: i64) -> Value {
    verweis("folders", id)
}

fn umgebung_link(id: i64) -> Value {
    verweis("environments", id)
}

fn zyklus_link(id: i64) -> Value {
    verweis("testcycles", id)
}

/// Ein Projekt: `id`, `jiraProjectId`, `key`, `enabled` – keinen Namen.
fn projekte() -> Vec<Value> {
    vec![
        json!({ "id": PROJEKT_ID, "jiraProjectId": 10000, "key": PROJEKT, "enabled": true }),
        json!({ "id": PROJEKT_2_ID, "jiraProjectId": 10001, "key": PROJEKT_2, "enabled": true }),
    ]
}

fn projekt(kennung: &str) -> Option<Value> {
    projekte().into_iter().find(|projekt| {
        text(projekt, "key").as_deref() == Some(kennung)
            || text(projekt, "id").as_deref() == Some(kennung)
    })
}

fn projekt_id(key: &str) -> Option<i64> {
    projekt(key)
        .and_then(|projekt| text(&projekt, "id"))
        .and_then(|id| id.parse().ok())
}

fn testfaelle(projekt_key: &str) -> Vec<Value> {
    match projekt_key {
        PROJEKT => vec![
            json!({
                "id": TESTFALL_ID, "key": TESTFALL, "name": "Login prüfen",
                "project": projekt_link(PROJEKT_ID),
                "status": status_link(1), "priority": priority_link(2),
                "folder": folder_link(ORDNER as i64),
                "createdOn": "2026-11-19T08:00:00Z",
                "objective": "Anmelden klappt",
                "precondition": "Benutzer besteht",
                "owner": { "accountId": "anna-4711", "self": "https://jira.example.test/rest/api/2/user?accountId=anna-4711" },
                "testScript": { "self": format!("{BASIS}/testcases/{TESTFALL}/testscript") }
            }),
            json!({
                "id": 12, "key": TESTFALL_2, "name": "Logout prüfen",
                "project": projekt_link(PROJEKT_ID),
                "status": status_link(2), "priority": priority_link(3),
                "folder": folder_link(13),
                "testScript": { "self": format!("{BASIS}/testcases/{TESTFALL_2}/testscript") }
            }),
        ],
        PROJEKT_2 => vec![json!({
            "id": 21, "key": TESTFALL_OPS, "name": "Server neu starten",
            "project": projekt_link(PROJEKT_2_ID),
            "status": status_link(1), "priority": priority_link(9)
        })],
        _ => Vec::new(),
    }
}

fn testfall(schluessel: &str) -> Option<Value> {
    [PROJEKT, PROJEKT_2]
        .iter()
        .flat_map(|projekt| testfaelle(projekt))
        .find(|testfall| text(testfall, "key").as_deref() == Some(schluessel))
}

fn testfall_id(schluessel: &str) -> Option<i64> {
    testfall(schluessel)
        .and_then(|testfall| text(&testfall, "id"))
        .and_then(|id| id.parse().ok())
}

/// Das Skript eines Testfalls; `None`, wenn er Schritte hat.
fn skript(schluessel: &str) -> Option<Value> {
    match schluessel {
        TESTFALL_2 => Some(json!({
            "id": 7, "type": "plain", "text": "1. Abmelden\n2. Prüfen"
        })),
        _ => None,
    }
}

/// Die Schritte eines Testfalls; `None`, wenn er ein Skript hat.
fn schritte(schluessel: &str) -> Option<Vec<Value>> {
    match schluessel {
        TESTFALL => Some(vec![
            json!({ "id": 1, "inline": { "description": "Seite öffnen", "expectedResult": "Formular sichtbar" } }),
            json!({ "id": 2, "inline": { "description": "Anmelden", "testData": "paula/pw" } }),
        ]),
        _ => None,
    }
}

fn testzyklen(projekt_key: &str) -> Vec<Value> {
    match projekt_key {
        PROJEKT => vec![testzyklus(ZYKLUS).unwrap()],
        PROJEKT_2 => vec![testzyklus(ZYKLUS_OPS).unwrap()],
        _ => Vec::new(),
    }
}

fn testzyklus(kennung: &str) -> Option<Value> {
    let bauen = |id: i64, key: &str, name: &str, projekt: i64, von: &str, bis: &str| {
        json!({
            "id": id, "key": key, "name": name,
            "project": projekt_link(projekt),
            "status": status_link(1),
            "plannedStartDate": von,
            "plannedEndDate": bis
        })
    };
    match kennung {
        ZYKLUS => Some(bauen(
            ZYKLUS_ID,
            ZYKLUS,
            "Login-Zyklus",
            PROJEKT_ID,
            "2026-11-01T00:00:00Z",
            "2026-11-14T00:00:00Z",
        )),
        ZYKLUS_OPS => Some(bauen(
            32,
            ZYKLUS_OPS,
            "Betrieb-Zyklus",
            PROJEKT_2_ID,
            "2026-11-05T00:00:00Z",
            "2026-11-06T00:00:00Z",
        )),
        "31" => Some(bauen(
            ZYKLUS_ID,
            ZYKLUS,
            "Login-Zyklus",
            PROJEKT_ID,
            "2026-11-01T00:00:00Z",
            "2026-11-14T00:00:00Z",
        )),
        _ => None,
    }
}

fn zyklus_id(kennung: &str) -> Option<i64> {
    testzyklus(kennung)
        .and_then(|zyklus| text(&zyklus, "id"))
        .and_then(|id| id.parse().ok())
}

fn ausfuehrungen(projekt_key: &str) -> Vec<Value> {
    match projekt_key {
        PROJEKT => vec![
            json!({
                "id": AUSFUEHRUNG_ID, "key": AUSFUEHRUNG,
                "project": projekt_link(PROJEKT_ID),
                "testCase": { "id": TESTFALL_ID, "self": format!("{BASIS}/testcases/{TESTFALL}/versions/1") },
                "testCycle": zyklus_link(ZYKLUS_ID),
                "testExecutionStatus": status_link(1),
                "environment": umgebung_link(UMGEBUNG),
                "executionTime": 120,
                "actualEndDate": "2026-11-20T10:15:00Z",
                "comment": "läuft",
                "automated": false,
                "customFields": { "browser": "chrome" }
            }),
            json!({
                "id": 102, "key": "QA-E2",
                "project": projekt_link(PROJEKT_ID),
                "testCase": { "id": 12, "self": format!("{BASIS}/testcases/{TESTFALL_2}/versions/1") },
                "testCycle": zyklus_link(ZYKLUS_ID),
                "testExecutionStatus": status_link(2)
            }),
        ],
        PROJEKT_2 => vec![json!({
            "id": 111, "key": AUSFUEHRUNG_OPS,
            "project": projekt_link(PROJEKT_2_ID),
            "testCase": { "id": 21, "self": format!("{BASIS}/testcases/{TESTFALL_OPS}/versions/1") },
            "testCycle": zyklus_link(32),
            "testExecutionStatus": status_link(9)
        })],
        _ => Vec::new(),
    }
}

fn ausfuehrung(kennung: &str) -> Option<Value> {
    [PROJEKT, PROJEKT_2]
        .iter()
        .flat_map(|projekt| ausfuehrungen(projekt))
        .find(|ausfuehrung| {
            text(ausfuehrung, "id").as_deref() == Some(kennung)
                || text(ausfuehrung, "key").as_deref() == Some(kennung)
        })
}

/// Ein Status: `OptionValue` plus `color`, `archived`, `default`.
fn status(id: i64, projekt: i64, name: &str, index: i64, standard: bool) -> Value {
    json!({
        "id": id,
        "project": projekt_link(projekt),
        "name": name,
        "index": index,
        "description": format!("{name} (Attrappe)"),
        "color": "#FFFFFF",
        "archived": false,
        "default": standard
    })
}

fn status_liste(projekt_key: &str, art: &str) -> Vec<Value> {
    let projekt = projekt_id(projekt_key).unwrap_or(PROJEKT_ID);
    match (projekt_key, art) {
        (PROJEKT, "TEST_CASE") => vec![
            status(1, projekt, "Draft", 0, false),
            status(2, projekt, "Approved", 1, false),
            status(3, projekt, "Deprecated", 2, false),
        ],
        (PROJEKT, "TEST_CYCLE") => vec![
            status(1, projekt, "Draft", 0, true),
            status(2, projekt, "Done", 1, false),
        ],
        (PROJEKT, "TEST_EXECUTION") => vec![
            status(1, projekt, "Pass", 0, true),
            status(2, projekt, "Fail", 1, false),
            status(3, projekt, "Blocked", 2, false),
            status(4, projekt, "Not Executed", 3, false),
            status(5, projekt, "In Arbeit", 4, false),
        ],
        (PROJEKT_2, "TEST_EXECUTION") => vec![status(9, projekt, "In Arbeit", 0, true)],
        (PROJEKT_2, _) => vec![status(1, projekt, "Draft", 0, true)],
        _ => Vec::new(),
    }
}

fn status_id(projekt: &str, art: &str, name: &str) -> Option<i64> {
    status_liste(projekt, art)
        .into_iter()
        .find(|status| text(status, "name").as_deref() == Some(name))
        .and_then(|status| text(&status, "id"))
        .and_then(|id| id.parse().ok())
}

fn umgebungen(projekt_key: &str) -> Vec<Value> {
    match projekt_key {
        PROJEKT => vec![json!({
            "id": UMGEBUNG, "project": projekt_link(PROJEKT_ID), "name": UMGEBUNG_NAME,
            "index": 0, "archived": false
        })],
        PROJEKT_2 => vec![json!({
            "id": 6, "project": projekt_link(PROJEKT_2_ID), "name": "Produktion",
            "index": 0, "archived": false
        })],
        _ => Vec::new(),
    }
}

fn umgebung_id(projekt: &str, name: &str) -> Option<i64> {
    umgebungen(projekt)
        .into_iter()
        .find(|umgebung| text(umgebung, "name").as_deref() == Some(name))
        .and_then(|umgebung| text(&umgebung, "id"))
        .and_then(|id| id.parse().ok())
}

fn prioritaeten(projekt_key: &str) -> Vec<Value> {
    let projekt = projekt_id(projekt_key).unwrap_or(PROJEKT_ID);
    match projekt_key {
        PROJEKT => vec![
            json!({ "id": 2, "project": projekt_link(projekt), "name": "High", "index": 0, "default": true }),
            json!({ "id": 3, "project": projekt_link(projekt), "name": "Normal", "index": 1, "default": false }),
        ],
        PROJEKT_2 => vec![
            json!({ "id": 9, "project": projekt_link(projekt), "name": "Normal", "index": 0, "default": true }),
        ],
        _ => Vec::new(),
    }
}

fn ordner(projekt_key: &str, art: &str) -> Vec<Value> {
    match (projekt_key, art) {
        (PROJEKT, "TEST_CASE") => vec![
            json!({
                "id": ORDNER, "name": "Login", "folderType": "TEST_CASE",
                "parentId": 3, "index": 0, "project": projekt_link(PROJEKT_ID)
            }),
            json!({
                "id": 13, "name": "Logout", "folderType": "TEST_CASE",
                "parentId": null, "index": 1, "project": projekt_link(PROJEKT_ID)
            }),
        ],
        (PROJEKT, "TEST_PLAN") => {
            vec![
                json!({ "id": 21, "name": "Plan A", "folderType": "TEST_PLAN", "parentId": null, "index": 0 }),
            ]
        }
        (PROJEKT_2, "TEST_CASE") => {
            vec![
                json!({ "id": 22, "name": "Betrieb", "folderType": "TEST_CASE", "parentId": null, "index": 0 }),
            ]
        }
        _ => Vec::new(),
    }
}

/// `[A-Z][A-Z_0-9]+-R[0-9]+` – so sieht ein Testzyklusschlüssel aus.
fn ist_zyklusschluessel(wert: &str) -> bool {
    let Some((projekt, nummer)) = wert.rsplit_once("-R") else {
        return false;
    };
    !projekt.is_empty()
        && projekt.chars().all(|zeichen| {
            zeichen.is_ascii_uppercase() || zeichen.is_ascii_digit() || zeichen == '_'
        })
        && !nummer.is_empty()
        && nummer.chars().all(|zeichen| zeichen.is_ascii_digit())
}

/// Eine laufende Nummer für erfundene Ausführungen.
fn testfaelle_nr(schluessel: &str) -> usize {
    match schluessel {
        TESTFALL => 1,
        TESTFALL_2 => 2,
        _ => 9,
    }
}

/// Ein Punktpfad in einer Antwort; `None`, wenn dort nichts steht.
fn text(baum: &Value, feld: &str) -> Option<String> {
    let mut aktuell = baum;
    for teil in feld.split('.') {
        aktuell = aktuell.get(teil)?;
    }
    match aktuell {
        Value::String(wert) => Some(wert.clone()),
        Value::Number(zahl) => Some(zahl.to_string()),
        Value::Bool(wahr) => Some(wahr.to_string()),
        _ => None,
    }
}

// ---------------------------------------------------------------- Anmeldung

/// Gilt der `Authorization`-Kopf für eines der beiden Token?
fn anmeldung_gilt(kopf: &HeaderMap) -> bool {
    let Some(wert) = kopfzeile(kopf, "authorization") else {
        return false;
    };
    let Some(token) = wert.strip_prefix("Bearer ") else {
        return false;
    };
    [TOKEN_A, TOKEN_B].contains(&token)
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
        let mut kopf = HeaderMap::new();
        kopf.insert(
            "authorization",
            format!("Bearer {TOKEN_A}").parse().unwrap(),
        );
        assert!(anmeldung_gilt(&kopf));

        let mut falsch = HeaderMap::new();
        falsch.insert("authorization", "Basic bml4".parse().unwrap());
        assert!(!anmeldung_gilt(&falsch));
        assert!(!anmeldung_gilt(&HeaderMap::new()));
    }

    #[test]
    fn die_abfrage_wird_dekodiert() {
        let abfrage = abfrage_lesen("projectKey=QA&startAt=10&testCycle=QA-R1&x=a+b");
        assert_eq!(abfrage.get("projectKey").map(String::as_str), Some("QA"));
        assert_eq!(abfrage.get("startAt").map(String::as_str), Some("10"));
        assert_eq!(abfrage.get("testCycle").map(String::as_str), Some("QA-R1"));
        assert_eq!(abfrage.get("x").map(String::as_str), Some("a b"));
    }

    #[test]
    fn der_rumpf_wird_streng_geprueft() {
        let rumpf = json!({ "projectKey": "QA", "name": "Neu" });
        assert!(pruefe_rumpf(&rumpf, &["projectKey", "name"], &["name"]).is_ok());
        assert!(pruefe_rumpf(&rumpf, &["projectKey"], &["name"]).is_err());
        assert!(pruefe_rumpf(&rumpf, &["projectKey", "name"], &["fehlt"]).is_err());
        assert!(rumpf_json("kein json").is_err());
        assert!(rumpf_json("[1,2]").is_err());
    }

    #[test]
    fn verweise_haben_nur_id_und_self() {
        for verweis in [
            projekt_link(1),
            status_link(1),
            priority_link(2),
            folder_link(12),
            umgebung_link(5),
            zyklus_link(31),
        ] {
            let feld = verweis.as_object().unwrap();
            let namen: Vec<&String> = feld.keys().collect();
            assert_eq!(feld.len(), 2, "{verweis}");
            assert!(namen.iter().any(|name| *name == "id"), "{verweis}");
            assert!(namen.iter().any(|name| *name == "self"), "{verweis}");
        }
    }

    #[test]
    fn die_testdaten_halten_die_muster_der_openapi_datei_ein() {
        // Testfallschlüssel: `.+-T[0-9]+`, Testzyklus: `[A-Z][A-Z_0-9]+-R[0-9]+`,
        // Ausführung: `[A-Z]+-E[0-9]+`, Projekt: `[A-Z][A-Z_0-9]+`.
        assert!(TESTFALL.contains("-T1"));
        assert!(ZYKLUS.starts_with("QA-R"));
        assert!(AUSFUEHRUNG.starts_with("QA-E"));
        assert!(projekt(PROJEKT).is_some());
        assert!(projekt("1").is_some(), "die Projekt-Id ist auch ein Pfad");
        assert!(
            testzyklus("31").is_some(),
            "die Zyklus-Id ist auch ein Pfad"
        );
    }
}
