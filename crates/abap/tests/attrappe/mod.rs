// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die ADT-Attrappe (SPEC §7): ein kleines axum-Programm, das die benutzten
//! Endpunkte beantwortet und jeden Aufruf mitschreibt.
//!
//! Kein Test spricht je mit einem echten SAP-System. Die Attrappe prüft die
//! Anmeldung (HTTP Basic), verlangt ein Cookie und ein CSRF-Token – genau wie
//! ein echter ADT-Server – und kann das Token auf Wunsch entwerten, damit der
//! Wiederholungsweg (`403 Required` → neues Token → einmal wiederholen)
//! geprüft werden kann. Das Passwort ist erfunden.
//!
//! Für Teil 2 kommt die Sitzung dazu: **jeder** Token-Abruf legt eine eigene
//! Sitzung an (eigener Keks, eigenes Token). Gesperrt, geschrieben und
//! entsperrt wird nur mit dem Keks der Sitzung, die gesperrt hat – eine andere
//! Sitzung bekommt `423`. Auf Wunsch scheitert ein `PUT` mit `500`, oder die
//! nächste stateful Anfrage meldet „Session timed out“.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::Router;

/// Benutzer, den die Attrappe erwartet. Kein echtes Konto.
pub const BENUTZER: &str = "DEVELOPER";
/// Passwort, das die Attrappe erwartet. Kein echtes Geheimnis.
pub const PASSWORT: &str = "attrappe-4711";

/// Ein Aufruf, den die Attrappe gesehen hat.
#[derive(Debug, Clone)]
pub struct Aufruf {
    /// Pfad, z. B. `/sap/bc/adt/repository/informationsystem/search`.
    pub pfad: String,
    pub methode: String,
    /// Abfrageparameter (Wert dekodiert).
    pub abfrage: BTreeMap<String, String>,
    pub rumpf: String,
    /// Der `Authorization`-Kopf.
    pub anmeldung: Option<String>,
    /// Der `Cookie`-Kopf.
    pub cookie: Option<String>,
    /// Der `x-csrf-token`-Kopf.
    pub csrf: Option<String>,
    /// Der `X-sap-adt-sessiontype`-Kopf (`stateful` nur bei Teil 2).
    pub sessiontype: Option<String>,
}

impl Aufruf {
    /// Der Wert eines Abfrageparameters.
    pub fn feld(&self, name: &str) -> Option<&str> {
        self.abfrage.get(name).map(String::as_str)
    }

    /// War der Aufruf Teil einer stateful Sitzung?
    pub fn ist_stateful(&self) -> bool {
        self.sessiontype.as_deref() == Some("stateful")
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

    /// Alle Aufrufe, deren Pfad `teil` enthält.
    pub fn zu(&self, teil: &str) -> Vec<Aufruf> {
        self.alle()
            .into_iter()
            .filter(|aufruf| aufruf.pfad.contains(teil))
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

    /// Wie viele Aufrufe mit dieser Methode auf den Pfadteil gingen.
    pub fn anzahl_mit(&self, teil: &str, methode: &str) -> usize {
        self.zu_mit(teil, methode).len()
    }

    pub fn erster(&self, teil: &str) -> Option<Aufruf> {
        self.zu(teil).into_iter().next()
    }

    pub fn merken(&self, aufruf: Aufruf) {
        griff(&self.aufrufe).push(aufruf);
    }
}

/// Eine Sperre in der Attrappe.
#[derive(Debug, Clone)]
pub struct Gesperrt {
    pub handle: String,
    /// Die Sitzung, die gesperrt hat (ihr Keksinhalt).
    pub sitzung: String,
    /// Der Transportauftrag; leer bei lokalen Objekten.
    pub auftrag: String,
}

/// Der veränderliche Teil der Attrappe.
#[derive(Debug, Default)]
pub struct Lage {
    /// Das zuletzt ausgegebene CSRF-Token.
    pub token: String,
    /// Wie oft ein Token geholt wurde (und damit eine Sitzung entstand).
    pub token_holt: u32,
    /// Der zuletzt ausgegebene Keks (`SAP_SESSIONID_DEV_100=…`).
    pub sitzung: String,
    /// Sitzungskennung → gültiges Token.
    pub token_je_sitzung: BTreeMap<String, String>,
    /// Objekt-URL → Sperre.
    pub locks: BTreeMap<String, Gesperrt>,
    /// Wie viele Sperren ausgegeben wurden.
    pub lock_zaehler: u32,
    /// Wie viele Sperren freigegeben wurden.
    pub unlocks: u32,
}

struct Zustand {
    aufzeichnung: Arc<Aufzeichnung>,
    lage: Mutex<Lage>,
    /// Beim nächsten Aufruf das Token entwerten (prüft den Wiederholungsweg).
    csrf_erneuern: AtomicBool,
    /// Dasselbe, aber nur für stateful Aufrufe (Sitzung, Teil 2).
    csrf_erneuern_in_sitzung: AtomicBool,
    /// Der nächste `PUT` auf einen Quelltext scheitert mit `500`.
    put_scheitert: AtomicBool,
    /// Die nächste stateful Anfrage meldet „Session timed out“.
    sitzung_ablaufen: AtomicBool,
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
            lage: Mutex::new(Lage::default()),
            csrf_erneuern: AtomicBool::new(false),
            csrf_erneuern_in_sitzung: AtomicBool::new(false),
            put_scheitert: AtomicBool::new(false),
            sitzung_ablaufen: AtomicBool::new(false),
        });

        let app = Router::new().fallback(adt).with_state(zustand.clone());

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
            zustand,
            ende,
        }
    }

    /// Beim nächsten Aufruf das CSRF-Token entwerten: der Client muss es neu
    /// holen und den Aufruf **einmal** wiederholen.
    pub fn csrf_erneuern(&self) {
        self.zustand.csrf_erneuern.store(true, Ordering::SeqCst);
    }

    /// Dasselbe, aber nur für die nächste stateful Anfrage (Teil 2) – so lässt
    /// sich der Wiederholungsweg **in der Sitzung** prüfen, ohne dass die
    /// zustandslosen Aufrufe dazwischen ihn verbrauchen.
    pub fn csrf_erneuern_in_sitzung(&self) {
        self.zustand
            .csrf_erneuern_in_sitzung
            .store(true, Ordering::SeqCst);
    }

    /// Der nächste `PUT` auf einen Quelltext scheitert mit `500` – prüft, ob
    /// der Client trotzdem entsperrt.
    pub fn put_scheitert(&self) {
        self.zustand.put_scheitert.store(true, Ordering::SeqCst);
    }

    /// Die nächste stateful Anfrage meldet „Session timed out“.
    pub fn sitzung_ablaufen(&self) {
        self.zustand.sitzung_ablaufen.store(true, Ordering::SeqCst);
    }

    /// Wie oft ein CSRF-Token (und damit eine Sitzung) geholt wurde.
    pub fn token_holt(&self) -> u32 {
        griff(&self.zustand.lage).token_holt
    }

    /// Das zuletzt ausgegebene Token (für Erwartungen im Test).
    pub fn token(&self) -> String {
        griff(&self.zustand.lage).token.clone()
    }

    /// Wie viele Sperren freigegeben wurden.
    pub fn unlocks(&self) -> u32 {
        griff(&self.zustand.lage).unlocks
    }

    /// Wie viele Objekte gerade gesperrt sind.
    pub fn offene_locks(&self) -> usize {
        griff(&self.zustand.lage).locks.len()
    }

    /// Der Handle, mit dem dieses Objekt gerade gesperrt ist.
    pub fn lock_handle(&self, objekt_url: &str) -> Option<String> {
        griff(&self.zustand.lage)
            .locks
            .get(objekt_url)
            .map(|sperre| sperre.handle.clone())
    }
}

// ------------------------------------------------------------------ Ablauf

async fn adt(State(zustand): State<Arc<Zustand>>, anfrage: Request) -> Response {
    let methode = anfrage.method().as_str().to_string();
    let pfad = anfrage.uri().path().to_string();
    let abfrage = parameter(anfrage.uri().query().unwrap_or_default());
    let kopf = anfrage.headers().clone();
    let rumpf = axum::body::to_bytes(anfrage.into_body(), 4 * 1024 * 1024)
        .await
        .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
        .unwrap_or_default();

    let anmeldung = kopf_zeile(&kopf, "authorization");
    let cookie = kopf_zeile(&kopf, "cookie");
    let csrf = kopf_zeile(&kopf, "x-csrf-token");
    let sessiontype = kopf_zeile(&kopf, "x-sap-adt-sessiontype");
    let stateful = sessiontype.as_deref() == Some("stateful");

    zustand.aufzeichnung.merken(Aufruf {
        pfad: pfad.clone(),
        methode: methode.clone(),
        abfrage: abfrage.clone(),
        rumpf: rumpf.clone(),
        anmeldung: anmeldung.clone(),
        cookie: cookie.clone(),
        csrf: csrf.clone(),
        sessiontype: sessiontype.clone(),
    });

    // 1) Anmeldung: ohne richtige Basic-Auth gibt es nichts.
    if anmeldung.as_deref() != Some(basic()) {
        return antwort(StatusCode::UNAUTHORIZED, "Anmeldung fehlt oder ist falsch.");
    }

    // 2) Ein Token holen ist immer erlaubt – und legt eine neue Sitzung an,
    //    wenn noch keine dabei ist. Innerhalb einer Sitzung bleibt der Keks
    //    gleich, nur das Token ist neu.
    if csrf.as_deref() == Some("fetch") {
        let vorhandene = cookie
            .as_deref()
            .map(sitzungs_id)
            .filter(|kennung| !kennung.is_empty());
        let (token, sitzung) = {
            let mut lage = griff(&zustand.lage);
            lage.token_holt += 1;
            let token = format!("token-{}", lage.token_holt);
            let kennung =
                match vorhandene.filter(|kennung| lage.token_je_sitzung.contains_key(kennung)) {
                    Some(kennung) => kennung,
                    None => format!("sitzung-{}", lage.token_holt),
                };
            lage.token_je_sitzung.insert(kennung.clone(), token.clone());
            lage.token = token.clone();
            lage.sitzung = format!("SAP_SESSIONID_DEV_100={kennung}");
            (token, lage.sitzung.clone())
        };
        return mit_kopfzeilen(
            StatusCode::OK,
            vec![("x-csrf-token", token), ("set-cookie", sitzung)],
            GRAPH,
        );
    }

    // 3) Ohne Keks keine Sitzung.
    let sitzung = match cookie
        .as_deref()
        .filter(|keks| keks.contains("SAP_SESSIONID"))
    {
        Some(keks) => sitzungs_id(keks),
        None => return antwort(StatusCode::UNAUTHORIZED, "Keine Sitzung."),
    };

    // 4) Das Token entwerten, falls der Test das verlangt.
    let erneuern = zustand.csrf_erneuern_in_sitzung.load(Ordering::SeqCst) && stateful;
    if erneuern {
        zustand
            .csrf_erneuern_in_sitzung
            .store(false, Ordering::SeqCst);
    }
    if erneuern || zustand.csrf_erneuern.swap(false, Ordering::SeqCst) {
        return mit_kopfzeilen(
            StatusCode::FORBIDDEN,
            vec![("x-csrf-token", "Required".to_string())],
            "Bitte ein neues Token holen.",
        );
    }

    // 5) Eine abgelaufene Sitzung: nur im stateful Betrieb, wie bei SAP.
    if stateful && zustand.sitzung_ablaufen.swap(false, Ordering::SeqCst) {
        return antwort(StatusCode::BAD_REQUEST, "Session timed out");
    }

    // 6) Und sonst muss das Token zu **dieser** Sitzung passen.
    let erwartet = griff(&zustand.lage).token_je_sitzung.get(&sitzung).cloned();
    if erwartet.as_deref() != csrf.as_deref() {
        return mit_kopfzeilen(
            StatusCode::FORBIDDEN,
            vec![("x-csrf-token", "Required".to_string())],
            "CSRF-Token ungültig.",
        );
    }

    // 7) Sperren und Entsperren: `POST <objekt-url>?_action=…`.
    match abfrage.get("_action").map(String::as_str) {
        Some("LOCK") => return lock_aufnehmen(&zustand, &pfad, &sitzung),
        Some("UNLOCK") => return lock_freigeben(&zustand, &pfad, &abfrage, &sitzung),
        Some(sonst) => {
            return antwort(
                StatusCode::BAD_REQUEST,
                &format!("Unbekannte Aktion {sonst}."),
            )
        }
        None => {}
    }

    // 8) Schreiben: Quelltext (Hauptinclude) und Textelemente.
    if methode == "PUT" {
        return schreiben(&zustand, &pfad, &abfrage, &sitzung);
    }

    // 9) Anlegen: `POST` auf einen Anlegepfad, `Location` zurück.
    if methode == "POST" && ANLEGEPFADE.contains(&pfad.as_str()) {
        return anlegen(&pfad, &rumpf);
    }

    // 10) Der eigentliche Endpunkt.
    match pfad.as_str() {
        "/sap/bc/adt/activation" => mit_kopfzeilen(
            StatusCode::OK,
            vec![("content-type", "application/xml".to_string())],
            AKTIVIERUNG,
        ),
        "/sap/bc/adt/core/discovery" => mit_kopfzeilen(
            StatusCode::OK,
            vec![("content-type", "application/xml".to_string())],
            DISCOVERY,
        ),
        "/sap/bc/adt/repository/informationsystem/search" => {
            if abfrage.get("query").map(String::as_str) == Some("ZFEHLER") {
                return mit_kopfzeilen(
                    StatusCode::BAD_REQUEST,
                    vec![("content-type", "application/xml".to_string())],
                    AUSNAHME,
                );
            }
            let rumpf = if abfrage.get("query").map(String::as_str) == Some("ZNIX") {
                "<adtcore:objectReferences xmlns:adtcore=\"http://www.sap.com/adt/core\"/>"
            } else {
                SUCHE
            };
            mit_kopfzeilen(
                StatusCode::OK,
                vec![("content-type", "application/xml".to_string())],
                rumpf,
            )
        }
        "/sap/bc/adt/oo/classes/zcl_demo" => mit_kopfzeilen(
            StatusCode::OK,
            vec![("content-type", "application/xml".to_string())],
            KLASSE,
        ),
        "/sap/bc/adt/oo/classes/zcl_demo/includes/implementations" => mit_kopfzeilen(
            StatusCode::OK,
            vec![("content-type", "text/plain; charset=utf-8".to_string())],
            QUELLTEXT,
        ),
        "/sap/bc/adt/repository/informationsystem/usageReferences" => mit_kopfzeilen(
            StatusCode::OK,
            vec![("content-type", "application/xml".to_string())],
            VERWENDUNGEN,
        ),
        "/sap/bc/adt/repository/informationsystem/objecttypes" => mit_kopfzeilen(
            StatusCode::OK,
            vec![("content-type", "application/xml".to_string())],
            OBJEKTARTEN,
        ),
        "/sap/bc/adt/repository/nodestructure" => mit_kopfzeilen(
            StatusCode::OK,
            vec![("content-type", "application/xml".to_string())],
            KNOTEN,
        ),
        "/sap/bc/adt/datapreview/ddic" | "/sap/bc/adt/datapreview/freestyle" => mit_kopfzeilen(
            StatusCode::OK,
            vec![("content-type", "application/xml".to_string())],
            TABELLE,
        ),
        "/sap/bc/adt/checkruns" => mit_kopfzeilen(
            StatusCode::OK,
            vec![("content-type", "application/xml".to_string())],
            SYNTAX,
        ),
        "/sap/bc/adt/abapunit/testruns" => mit_kopfzeilen(
            StatusCode::OK,
            vec![("content-type", "application/xml".to_string())],
            UNIT,
        ),
        "/sap/bc/adt/atc/runs" => mit_kopfzeilen(
            StatusCode::OK,
            vec![("content-type", "application/xml".to_string())],
            ATC_LAUF,
        ),
        "/sap/bc/adt/atc/worklists/WL4711" => mit_kopfzeilen(
            StatusCode::OK,
            vec![("content-type", "application/xml".to_string())],
            ATC_LISTE,
        ),
        "/sap/bc/adt/runtime/dumps" => mit_kopfzeilen(
            StatusCode::OK,
            vec![("content-type", "application/xml".to_string())],
            DUMPS,
        ),
        "/sap/bc/adt/runtime/traces/abaptraces" => mit_kopfzeilen(
            StatusCode::OK,
            vec![("content-type", "application/xml".to_string())],
            TRACES,
        ),
        "/sap/bc/adt/runtime/traces/abaptraces/T0001/hitlist" => mit_kopfzeilen(
            StatusCode::OK,
            vec![("content-type", "application/xml".to_string())],
            HITLIST,
        ),
        "/sap/bc/adt/runtime/traces/abaptraces/T0001/statements" => mit_kopfzeilen(
            StatusCode::OK,
            vec![("content-type", "application/xml".to_string())],
            STATEMENTS,
        ),
        "/sap/bc/adt/cts/transportrequests" => mit_kopfzeilen(
            StatusCode::OK,
            vec![("content-type", "application/xml".to_string())],
            TRANSPORTE,
        ),
        "/sap/bc/adt/cts/transportrequests/DEVK900123" => mit_kopfzeilen(
            StatusCode::OK,
            vec![("content-type", "application/xml".to_string())],
            TRANSPORTDETAIL,
        ),
        "/sap/bc/adt/textelements/classes/zcl_demo/source/symbols" => mit_kopfzeilen(
            StatusCode::OK,
            vec![("content-type", "text/plain; charset=utf-8".to_string())],
            TEXTELEMENTE,
        ),
        "/sap/bc/adt/textelements/classes/zcl_demo/source/selections" => {
            antwort(StatusCode::NOT_FOUND, "Keine Selectionstexte.")
        }
        sonst => antwort(
            StatusCode::NOT_FOUND,
            &format!("Kein ADT-Endpunkt für {sonst}."),
        ),
    }
}

/// Die Sitzungskennung aus dem Keks der Attrappe.
fn sitzungs_id(cookie: &str) -> String {
    cookie
        .split(';')
        .filter_map(|teil| teil.trim().split_once('='))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("SAP_SESSIONID_DEV_100"))
        .map(|(_, wert)| wert.trim().to_string())
        .unwrap_or_default()
}

/// Die Anlegepfade, die die Attrappe kennt (`api/objectcreator.js`).
const ANLEGEPFADE: [&str; 9] = [
    "/sap/bc/adt/oo/classes",
    "/sap/bc/adt/oo/interfaces",
    "/sap/bc/adt/programs/programs",
    "/sap/bc/adt/programs/includes",
    "/sap/bc/adt/functions/groups",
    "/sap/bc/adt/ddic/tables",
    "/sap/bc/adt/ddic/dataelements",
    "/sap/bc/adt/ddic/domains",
    "/sap/bc/adt/ddic/ddl/sources",
];

/// Ein Objekt sperren: nur diese Sitzung darf es danach schreiben.
fn lock_aufnehmen(zustand: &Zustand, objekt_url: &str, sitzung: &str) -> Response {
    let mut lage = griff(&zustand.lage);
    if lage.locks.contains_key(objekt_url) {
        return antwort(StatusCode::CONFLICT, "Das Objekt ist schon gesperrt.");
    }
    lage.lock_zaehler += 1;
    let handle = format!("LH{}", lage.lock_zaehler);
    // ZCL_DEMO liegt in $TMP und braucht keinen Transport; alles andere schon.
    let lokal = objekt_url.ends_with("zcl_demo");
    let auftrag = if lokal {
        String::new()
    } else {
        "DEVK900123".to_string()
    };
    lage.locks.insert(
        objekt_url.to_string(),
        Gesperrt {
            handle: handle.clone(),
            sitzung: sitzung.to_string(),
            auftrag: auftrag.clone(),
        },
    );
    let daten = if lokal {
        format!("<asx:values><DATA><LOCK_HANDLE>{handle}</LOCK_HANDLE><IS_LOCAL>X</IS_LOCAL></DATA></asx:values>")
    } else {
        format!(
            "<asx:values><DATA><LOCK_HANDLE>{handle}</LOCK_HANDLE><CORRNR>{auftrag}</CORRNR></DATA></asx:values>"
        )
    };
    mit_kopfzeilen(
        StatusCode::OK,
        vec![("content-type", "application/xml".to_string())],
        &format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<asx:abap xmlns:asx=\"http://www.sap.com/abapxml\" version=\"1.0\">{daten}</asx:abap>"
        ),
    )
}

/// Ein Objekt entsperren: nur die Sitzung, die gesperrt hat, und nur mit
/// ihrem Handle.
fn lock_freigeben(
    zustand: &Zustand,
    objekt_url: &str,
    abfrage: &BTreeMap<String, String>,
    sitzung: &str,
) -> Response {
    let handle = abfrage.get("lockHandle").cloned().unwrap_or_default();
    let mut lage = griff(&zustand.lage);
    let bekannt = lage
        .locks
        .get(objekt_url)
        .map(|sperre| (sperre.handle.clone(), sperre.sitzung.clone()));
    match bekannt {
        None => antwort(StatusCode::CONFLICT, "Das Objekt ist nicht gesperrt."),
        Some((gesperrt_mit, gesperrt_von)) if gesperrt_mit != handle || gesperrt_von != sitzung => {
            antwort(
                StatusCode::LOCKED,
                "Die Sperre gehört zu einer anderen Sitzung oder der Handle stimmt nicht.",
            )
        }
        Some(_) => {
            lage.locks.remove(objekt_url);
            lage.unlocks += 1;
            antwort(StatusCode::OK, "Entsperrt.")
        }
    }
}

/// Den Objektnamen aus einer Textelemente-Adresse lesen.
fn textelement_objekt(pfad: &str) -> Option<String> {
    for art in ["/classes/", "/programs/", "/functiongroups/"] {
        if let Some(rest) = pfad.split(art).nth(1) {
            return rest.split('/').next().map(str::to_string);
        }
    }
    None
}

/// Die Sperre, die zu einem schreibenden Aufruf passt (gleiches Objekt).
fn sperre_zu(lage: &Lage, pfad: &str) -> Option<Gesperrt> {
    let name = textelement_objekt(pfad);
    let treffer = match &name {
        Some(name) => lage
            .locks
            .iter()
            .find(|(url, _)| url.rsplit('/').next() == Some(name.as_str())),
        None => lage
            .locks
            .iter()
            .find(|(url, _)| pfad.starts_with(url.as_str())),
    };
    treffer.map(|(_, sperre)| sperre.clone())
}

/// Quelltext oder Textelemente schreiben (`PUT`).
fn schreiben(
    zustand: &Zustand,
    pfad: &str,
    abfrage: &BTreeMap<String, String>,
    sitzung: &str,
) -> Response {
    let handle = abfrage.get("lockHandle").cloned().unwrap_or_default();
    let lage = griff(&zustand.lage);
    let Some(sperre) = sperre_zu(&lage, pfad) else {
        return antwort(StatusCode::LOCKED, "Das Objekt ist nicht gesperrt.");
    };
    drop(lage);
    if sperre.sitzung != sitzung || sperre.handle != handle {
        return antwort(
            StatusCode::LOCKED,
            "Die Sperre gehört zu einer anderen Sitzung oder der Handle stimmt nicht.",
        );
    }
    if zustand.put_scheitert.swap(false, Ordering::SeqCst) {
        return antwort(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Der Quelltext ließ sich nicht speichern.",
        );
    }
    antwort(StatusCode::OK, "Gespeichert.")
}

/// Ein Objekt anlegen: `201` mit `Location`.
fn anlegen(pfad: &str, rumpf: &str) -> Response {
    let name = attribut_wert(rumpf, "adtcore:name").unwrap_or_else(|| "zobjekt".to_string());
    mit_kopfzeilen(
        StatusCode::CREATED,
        vec![
            ("location", format!("{pfad}/{}", name.to_lowercase())),
            ("content-type", "text/plain; charset=utf-8".to_string()),
        ],
        "",
    )
}

/// Der erste Attributwert `name="…"` aus einem XML-Rumpf.
fn attribut_wert(rumpf: &str, name: &str) -> Option<String> {
    let suche = format!("{name}=\"");
    let rest = rumpf.split(&suche).nth(1)?;
    rest.split('"').next().map(str::to_string)
}

fn kopf_zeile(kopf: &HeaderMap, name: &str) -> Option<String> {
    kopf.get(name)
        .and_then(|wert| wert.to_str().ok())
        .map(str::to_string)
}

fn antwort(status: StatusCode, text: &str) -> Response {
    mit_kopfzeilen(
        status,
        vec![("content-type", "text/plain; charset=utf-8".to_string())],
        text,
    )
}

fn mit_kopfzeilen(status: StatusCode, kopfzeilen: Vec<(&str, String)>, text: &str) -> Response {
    let mut bauen = Response::builder().status(status);
    for (name, wert) in kopfzeilen {
        bauen = bauen.header(name, wert);
    }
    bauen
        .body(Body::from(text.to_string()))
        .expect("Antwort der Attrappe")
}

/// Der erwartete `Authorization`-Kopf (HTTP Basic).
fn basic() -> &'static str {
    // DEVELOPER:attrappe-4711, base64-kodiert.
    "Basic REVWRUxPUEVSOmF0dHJhcHBlLTQ3MTE="
}

/// Abfrageparameter dekodieren (`+` ist ein Leerzeichen, `%XX` ein Zeichen).
fn parameter(roh: &str) -> BTreeMap<String, String> {
    let mut alle = BTreeMap::new();
    for paar in roh.split('&').filter(|paar| !paar.is_empty()) {
        let (name, wert) = match paar.split_once('=') {
            Some((name, wert)) => (name, wert),
            None => (paar, ""),
        };
        alle.insert(dekodiere(name), dekodiere(wert));
    }
    alle
}

fn dekodiere(wert: &str) -> String {
    let bytes = wert.as_bytes();
    let mut ergebnis = Vec::with_capacity(bytes.len());
    let mut stelle = 0;
    while stelle < bytes.len() {
        match bytes[stelle] {
            b'+' => {
                ergebnis.push(b' ');
                stelle += 1;
            }
            b'%' if stelle + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[stelle + 1..stelle + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(zeichen) => {
                        ergebnis.push(zeichen);
                        stelle += 3;
                    }
                    Err(_) => {
                        ergebnis.push(b'%');
                        stelle += 1;
                    }
                }
            }
            zeichen => {
                ergebnis.push(zeichen);
                stelle += 1;
            }
        }
    }
    String::from_utf8_lossy(&ergebnis).to_string()
}

// ------------------------------------------------------------- Antworten

const GRAPH: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<compatibility:graph xmlns:compatibility="http://www.sap.com/adt/compatibility">
  <compatibility:nodes>
    <compatibility:node compatibility:name="ObjectModel" compatibility:nameSpace="com.sap.adt"/>
  </compatibility:nodes>
  <compatibility:edges/>
</compatibility:graph>"#;

const DISCOVERY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<app:service xmlns:app="http://www.w3.org/2007/app" xmlns:atom="http://www.w3.org/2005/Atom">
  <app:workspace>
    <atom:title>Objekte</atom:title>
    <app:collection href="/sap/bc/adt/repository/informationsystem/search"><atom:title>Suche</atom:title></app:collection>
  </app:workspace>
  <app:workspace>
    <atom:title>Daten</atom:title>
    <app:collection href="/sap/bc/adt/datapreview/ddic"><atom:title>Tabelle</atom:title></app:collection>
  </app:workspace>
</app:service>"#;

const SUCHE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<adtcore:objectReferences xmlns:adtcore="http://www.sap.com/adt/core">
  <adtcore:objectReference adtcore:uri="/sap/bc/adt/oo/classes/zcl_demo" adtcore:type="CLAS/OC"
    adtcore:name="ZCL_DEMO" adtcore:packageName="$TMP" adtcore:description="Demo-Klasse"/>
  <adtcore:objectReference adtcore:uri="/sap/bc/adt/programs/programs/zreport" adtcore:type="PROG/P"
    adtcore:name="ZREPORT" adtcore:packageName="$TMP" adtcore:description="Demo-Report"/>
</adtcore:objectReferences>"#;

const AUSNAHME: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<exc:exception xmlns:exc="http://www.sap.com/abapxml/types/communicationframework">
  <exc:type id="ExceptionResourceNotFound"/>
  <exc:message lang="DE">Die Suche hat kein Ergebnis geliefert.</exc:message>
</exc:exception>"#;

const KLASSE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<class:abapClass xmlns:class="http://www.sap.com/adt/oo/classes"
  xmlns:adtcore="http://www.sap.com/adt/core" xmlns:atom="http://www.w3.org/2005/Atom"
  adtcore:name="ZCL_DEMO" adtcore:type="CLAS/OC" adtcore:description="Demo-Klasse"
  adtcore:responsible="DEVELOPER" adtcore:language="DE"
  adtcore:version="active" adtcore:versionTitle="Aktiv">
  <!-- Wie ADT: der eigene Pfad relativ, dazu die Versionsverweise. -->
  <atom:link href="/sap/bc/adt/oo/classes/zcl_demo" rel="self" type="text/html"/>
  <atom:link href="/sap/bc/adt/oo/classes/zcl_demo?version=active" rel="version"/>
  <atom:link href="/sap/bc/adt/versions/zcl_demo/versions" rel="http://www.sap.com/adt/relations/versions"/>
  <class:include class:includeType="definitions">
    <atom:link href="/sap/bc/adt/oo/classes/zcl_demo/includes/definitions" rel="self" type="text/plain"/>
  </class:include>
  <class:include class:includeType="main">
    <atom:link href="/sap/bc/adt/oo/classes/zcl_demo/includes/implementations" rel="self" type="text/plain"/>
  </class:include>
</class:abapClass>"#;

const QUELLTEXT: &str = "CLASS zcl_demo DEFINITION PUBLIC FINAL CREATE PUBLIC.\n  PUBLIC SECTION.\n    METHODS run.\nENDCLASS.\n\nCLASS zcl_demo IMPLEMENTATION.\n  METHOD run.\n    WRITE 'Hallo Attrappe'.\n  ENDMETHOD.\nENDCLASS.\n";

const VERWENDUNGEN: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<usageReferences:usageReferenceResult xmlns:usageReferences="http://www.sap.com/adt/ris/usageReferences"
  xmlns:adtcore="http://www.sap.com/adt/core">
  <usageReferences:referencedObjects>
    <usageReferences:referencedObject uri="/sap/bc/adt/programs/programs/zreport/source/main#start=10,4">
      <usageReferences:adtObject adtcore:name="ZREPORT" adtcore:type="PROG/P"
        adtcore:description="Demo-Report"/>
    </usageReferences:referencedObject>
  </usageReferences:referencedObjects>
</usageReferences:usageReferenceResult>"#;

const OBJEKTARTEN: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<nameditem:namedItemList xmlns:nameditem="http://www.sap.com/adt/nameditem">
  <nameditem:namedItem>
    <nameditem:name>CLAS</nameditem:name>
    <nameditem:description>Klasse</nameditem:description>
    <nameditem:data>type:CLAS/OC;usedBy:search,whereUsed</nameditem:data>
  </nameditem:namedItem>
  <nameditem:namedItem>
    <nameditem:name>PROG</nameditem:name>
    <nameditem:description>Programm</nameditem:description>
    <nameditem:data>type:PROG/P;usedBy:search</nameditem:data>
  </nameditem:namedItem>
</nameditem:namedItemList>"#;

const KNOTEN: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<asx:abap xmlns:asx="http://www.sap.com/abapxml" version="1.0">
  <asx:values>
    <DATA>
      <TREE_CONTENT>
        <SEU_ADT_REPOSITORY_OBJ_NODE>
          <OBJECT_TYPE>CLAS/OC</OBJECT_TYPE>
          <OBJECT_NAME>ZCL_DEMO</OBJECT_NAME>
          <OBJECT_URI>/sap/bc/adt/oo/classes/zcl_demo</OBJECT_URI>
          <DESCRIPTION>Demo-Klasse</DESCRIPTION>
        </SEU_ADT_REPOSITORY_OBJ_NODE>
      </TREE_CONTENT>
    </DATA>
  </asx:values>
</asx:abap>"#;

const TABELLE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<dataPreview:tableData xmlns:dataPreview="http://www.sap.com/adt/dataPreview">
  <dataPreview:columns>
    <dataPreview:metadata dataPreview:name="CARRID" dataPreview:type="C" dataPreview:description="Airline"/>
    <dataPreview:dataSet>
      <dataPreview:data>AA</dataPreview:data>
      <dataPreview:data>LH</dataPreview:data>
    </dataPreview:dataSet>
  </dataPreview:columns>
  <dataPreview:columns>
    <dataPreview:metadata dataPreview:name="CONNID" dataPreview:type="N"/>
    <dataPreview:dataSet>
      <dataPreview:data>0017</dataPreview:data>
      <dataPreview:data>0400</dataPreview:data>
    </dataPreview:dataSet>
  </dataPreview:columns>
</dataPreview:tableData>"#;

const SYNTAX: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<chkrun:checkRunReports xmlns:chkrun="http://www.sap.com/adt/checkrun">
  <chkrun:checkReport chkrun:reporter="abapCheckRun">
    <chkrun:checkMessageList>
      <chkrun:checkMessage chkrun:uri="/sap/bc/adt/oo/classes/zcl_demo/includes/implementations#start=3,5"
        chkrun:type="W" chkrun:shortText="Die Methode RUN wird nie aufgerufen."/>
    </chkrun:checkMessageList>
  </chkrun:checkReport>
</chkrun:checkRunReports>"#;

const UNIT: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<aunit:runResult xmlns:aunit="http://www.sap.com/adt/aunit" xmlns:adtcore="http://www.sap.com/adt/core">
  <program adtcore:name="ZCL_DEMO" adtcore:type="CLAS/OC">
    <testClasses>
      <testClass adtcore:name="LTCL_DEMO" adtcore:type="CLAS/OC" riskLevel="harmless">
        <testMethods>
          <testMethod adtcore:name="TEST_GUT"/>
          <testMethod adtcore:name="TEST_SCHLECHT">
            <alerts>
              <alert kind="failedAssertion" severity="critical" title="Assertion fehlgeschlagen">
                <details>
                  <detail text="Erwartet 1, war 2"/>
                </details>
              </alert>
            </alerts>
          </testMethod>
        </testMethods>
      </testClass>
    </testClasses>
  </program>
</aunit:runResult>"#;

const ATC_LAUF: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<worklistRun xmlns="http://www.sap.com/adt/atc">
  <worklistId>WL4711</worklistId>
  <worklistTimestamp>2026-01-02T03:04:05Z</worklistTimestamp>
</worklistRun>"#;

const ATC_LISTE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<worklist xmlns="http://www.sap.com/adt/atc" timestamp="2026-01-02T03:04:05Z">
  <objects>
    <object uri="/sap/bc/adt/oo/classes/zcl_demo" type="CLAS/OC" name="ZCL_DEMO"
      packageName="$TMP" author="DEVELOPER">
      <findings>
        <finding uri="/sap/bc/adt/atc/findings/1" priority="1" checkId="CL_CI_TEST_SECURITY"
          checkTitle="Sicherheitsprüfung" messageId="001"
          messageTitle="Fehlender Autoritätscheck vor einem Datenbankzugriff"
          location="/sap/bc/adt/oo/classes/zcl_demo/includes/implementations#start=42,3"/>
      </findings>
    </object>
  </objects>
</worklist>"#;

const DUMPS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <id>/sap/bc/adt/runtime/dumps</id>
  <title>Dumps</title>
  <entry>
    <id>20260101-120000-1</id>
    <author><name>DEVELOPER</name></author>
    <summary type="text">MESSAGE_TYPE_UNKNOWN</summary>
  </entry>
</feed>"#;

const TRACES: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <id>/sap/bc/adt/runtime/traces/abaptraces</id>
  <title>Traces</title>
  <entry>
    <id>/sap/bc/adt/runtime/traces/abaptraces/T0001</id>
    <title>Aufzeichnung 1</title>
    <extendedData>
      <objectName>ZREPORT</objectName>
      <state value="A" text="Aktiv"/>
      <runtime>12.5</runtime>
    </extendedData>
  </entry>
</feed>"#;

const HITLIST: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<hitlist>
  <link rel="self" href="/sap/bc/adt/runtime/traces/abaptraces/T0001/hitlist"/>
  <entry topDownIndex="1" index="1" hitCount="7" recursionDepth="0" description="LOOP über flight">
    <grossTime time="42.5" percentage="61.2"/>
    <traceEventNetTime time="40.0" percentage="60.0"/>
    <proceduralNetTime time="39.0" percentage="59.0"/>
  </entry>
</hitlist>"#;

const STATEMENTS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<statements withDetails="false" withSysEvents="false" count="1">
  <link rel="self" href="/sap/bc/adt/runtime/traces/abaptraces/T0001/statements"/>
  <statement index="1" id="1" description="CALL METHOD" hitCount="3">
    <grossTime time="1.5" percentage="1.0"/>
  </statement>
</statements>"#;

const TRANSPORTE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<tm:root xmlns:tm="http://www.sap.com/adt/cts/transportmanager">
  <tm:workbench>
    <tm:target tm:name="A4H" tm:desc="Entwicklung">
      <tm:modifiable>
        <tm:request tm:number="DEVK900123" tm:status="D" tm:desc="Demo-Auftrag" tm:owner="DEVELOPER"/>
      </tm:modifiable>
      <tm:released/>
    </tm:target>
  </tm:workbench>
  <tm:customizing/>
</tm:root>"#;

const TRANSPORTDETAIL: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<tm:root xmlns:tm="http://www.sap.com/adt/cts/transportmanager">
  <tm:request tm:number="DEVK900123" tm:owner="DEVELOPER" tm:desc="Demo-Auftrag" tm:status="D">
    <tm:abap_object tm:name="ZCL_DEMO" tm:type="CLAS/OC"/>
    <tm:task tm:number="DEVK900123" tm:desc="Aufgabe 1">
      <tm:abap_object tm:name="ZCL_DEMO" tm:type="CLAS/OC"/>
    </tm:task>
  </tm:request>
</tm:root>"#;

const TEXTELEMENTE: &str = "@MaxLength:30\n001=Anzahl\n002=Summe\n";

/// Die Antwort der Aktivierung: eine Warnung und ein Fehler in Zeile 3.
const AKTIVIERUNG: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<chkl:checkMessages xmlns:chkl="http://www.sap.com/adt/checkrun">
  <chkl:messages>
    <chkl:msg type="W" line="1" id="W01">
      <chkl:shortText><chkl:txt>Der Parameter wird nie gelesen.</chkl:txt></chkl:shortText>
    </chkl:msg>
    <chkl:msg type="E" line="3" id="E01">
      <chkl:shortText><chkl:txt>Die Anweisung WRITE ist hier nicht erlaubt.</chkl:txt></chkl:shortText>
    </chkl:msg>
  </chkl:messages>
</chkl:checkMessages>"#;

#[cfg(test)]
mod form_tests {
    use super::*;

    /// Die Attrappe antwortet in der Form, die ADT wirklich schickt: relative
    /// Adressen, Namensräume, Versionsverweise. Der Test hält das fest –
    /// ein Werkzeug, das absolute Adressen erwartet, fällt hier auf.
    #[test]
    fn die_objektantwort_hat_relative_adressen_und_versionsverweise() {
        assert!(KLASSE.contains(r#"adtcore:version="active""#), "{KLASSE}");
        assert!(KLASSE.contains(r#"href="/sap/bc/adt/oo/classes/zcl_demo" rel="self""#));
        assert!(KLASSE.contains(r#"rel="version""#));
        assert!(KLASSE.contains(r#"class:includeType="definitions""#));
        // Kein `https://…`: ADT nennt Pfade relativ zur Wurzel.
        assert!(!KLASSE.contains("https://"), "{KLASSE}");
    }

    #[test]
    fn suche_und_verwendung_nennen_relative_uris() {
        assert!(SUCHE.contains(r#"adtcore:uri="/sap/bc/adt/oo/classes/zcl_demo""#));
        assert!(VERWENDUNGEN.contains(r#"uri="/sap/bc/adt/"#));
        assert!(!SUCHE.contains("https://"), "{SUCHE}");
    }
}
