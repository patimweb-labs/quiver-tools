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
}

impl Aufruf {
    /// Der Wert eines Abfrageparameters.
    pub fn feld(&self, name: &str) -> Option<&str> {
        self.abfrage.get(name).map(String::as_str)
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

    pub fn erster(&self, teil: &str) -> Option<Aufruf> {
        self.zu(teil).into_iter().next()
    }

    pub fn merken(&self, aufruf: Aufruf) {
        griff(&self.aufrufe).push(aufruf);
    }
}

/// Der veränderliche Teil der Attrappe.
#[derive(Debug, Default)]
pub struct Lage {
    /// Das aktuell gültige CSRF-Token.
    pub token: String,
    /// Wie oft ein Token geholt wurde.
    pub token_holt: u32,
    /// Der Wert des Sitzungskekses.
    pub sitzung: String,
}

struct Zustand {
    aufzeichnung: Arc<Aufzeichnung>,
    lage: Mutex<Lage>,
    /// Beim nächsten Aufruf das Token entwerten (prüft den Wiederholungsweg).
    csrf_erneuern: AtomicBool,
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
            lage: Mutex::new(Lage {
                token: "start".to_string(),
                token_holt: 0,
                sitzung: "SAP_SESSIONID_DEV_100=kein-echter-keks".to_string(),
            }),
            csrf_erneuern: AtomicBool::new(false),
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

    /// Wie oft ein CSRF-Token geholt wurde.
    pub fn token_holt(&self) -> u32 {
        griff(&self.zustand.lage).token_holt
    }

    /// Das aktuell gültige Token (für Erwartungen im Test).
    pub fn token(&self) -> String {
        griff(&self.zustand.lage).token.clone()
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

    zustand.aufzeichnung.merken(Aufruf {
        pfad: pfad.clone(),
        methode: methode.clone(),
        abfrage: abfrage.clone(),
        rumpf: rumpf.clone(),
        anmeldung: anmeldung.clone(),
        cookie: cookie.clone(),
        csrf: csrf.clone(),
    });

    // 1) Anmeldung: ohne richtige Basic-Auth gibt es nichts.
    if anmeldung.as_deref() != Some(basic()) {
        return antwort(StatusCode::UNAUTHORIZED, "Anmeldung fehlt oder ist falsch.");
    }

    // 2) Ein Token holen ist immer erlaubt – auch ohne Keks.
    if csrf.as_deref() == Some("fetch") {
        let (token, sitzung) = {
            let mut lage = griff(&zustand.lage);
            lage.token_holt += 1;
            (lage.token.clone(), lage.sitzung.clone())
        };
        return mit_kopfzeilen(
            StatusCode::OK,
            vec![("x-csrf-token", token), ("set-cookie", sitzung)],
            GRAPH,
        );
    }

    // 3) Ohne Keks keine Sitzung.
    match cookie.as_deref() {
        Some(cookie) if cookie.contains("SAP_SESSIONID") => {}
        _ => return antwort(StatusCode::UNAUTHORIZED, "Keine Sitzung."),
    }

    // 4) Das Token entwerten, falls der Test das verlangt.
    if zustand.csrf_erneuern.swap(false, Ordering::SeqCst) {
        let neues = neues_token(&zustand);
        return mit_kopfzeilen(
            StatusCode::FORBIDDEN,
            vec![("x-csrf-token", "Required".to_string())],
            &format!("Das Token {neues} ist noch nicht gültig."),
        );
    }
    // 5) Und sonst muss das Token stimmen.
    let gueltig = griff(&zustand.lage).token.clone();
    if csrf.as_deref() != Some(gueltig.as_str()) {
        return mit_kopfzeilen(
            StatusCode::FORBIDDEN,
            vec![("x-csrf-token", "Required".to_string())],
            "CSRF-Token ungültig.",
        );
    }

    // 6) Der eigentliche Endpunkt.
    match pfad.as_str() {
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

fn neues_token(zustand: &Zustand) -> String {
    let mut lage = griff(&zustand.lage);
    lage.token = format!("token-{}", lage.token_holt + 1);
    lage.token.clone()
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
  adtcore:responsible="DEVELOPER" adtcore:language="DE">
  <atom:link href="/sap/bc/adt/oo/classes/zcl_demo" rel="self" type="text/html"/>
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
