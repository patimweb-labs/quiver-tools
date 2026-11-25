// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Werkzeuge gegen die ADT-Attrappe (SPEC §7, Aufgabe 07).
//!
//! Jeder Fall prüft eine Wirkung: was im SAP-System gefragt wurde (aufgezeichnet
//! von der Attrappe) und was beim Agenten ankommt. Ein echtes SAP-System ist nie
//! im Spiel.

mod attrappe;

use attrappe::{Attrappe, BENUTZER, PASSWORT};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use quiver_abap::systeme::Sammlung;
use quiver_abap::{Abap, AbapServer};
use quiver_tools_kern::zugaenge::Zugang as Eintrag;
use quiver_tools_kern::Zugang;
use rmcp::model::CallToolResult;
use rmcp::ErrorData as McpError;
use serde_json::{json, Value};

/// Ein System für die Attrappe; das Passwort ist erfunden.
///
/// Der Schalter `schreiben` steht auf `an`, damit die schreibenden Werkzeuge
/// geprüft werden können; die lesenden Tests ändern daran nichts.
fn eintrag(adresse: &str, name: &str, passwort: &str) -> Eintrag {
    eintrag_mit(adresse, name, passwort, true)
}

/// Ein System mit gewähltem Schalter `schreiben`.
fn eintrag_mit(adresse: &str, name: &str, passwort: &str, schreiben: bool) -> Eintrag {
    Eintrag {
        name: name.to_string(),
        werte: [
            ("benutzer", BENUTZER),
            ("url", adresse),
            ("mandant", "100"),
            ("sprache", "DE"),
            ("passwort", passwort),
            ("schreiben", if schreiben { "an" } else { "aus" }),
        ]
        .into_iter()
        .map(|(feld, wert)| (feld.to_string(), wert.to_string()))
        .collect(),
    }
}

/// Ein Server mit genau einem System.
fn server(adresse: &str) -> AbapServer {
    server_mit_passwort(adresse, PASSWORT)
}

/// Ein Server, auf dem Schreiben **aus** ist.
fn server_ohne_schreiben(adresse: &str) -> AbapServer {
    AbapServer::mit(
        Zugang::mit_umgebung(&Abap, |_| None),
        Ok(
            Sammlung::aus_zugaengen(vec![eintrag_mit(adresse, "dev-100", PASSWORT, false)])
                .unwrap(),
        ),
    )
}

fn server_mit_passwort(adresse: &str, passwort: &str) -> AbapServer {
    AbapServer::mit(
        Zugang::mit_umgebung(&Abap, |_| None),
        Ok(Sammlung::aus_zugaengen(vec![eintrag(adresse, "dev-100", passwort)]).unwrap()),
    )
}

/// Zwei Systeme, beide auf dieselbe Attrappe.
fn server_mit_zwei_systemen(adresse: &str) -> AbapServer {
    AbapServer::mit(
        Zugang::mit_umgebung(&Abap, |_| None),
        Ok(Sammlung::aus_zugaengen(vec![
            eintrag(adresse, "dev-100", PASSWORT),
            eintrag(adresse, "qas-200", PASSWORT),
        ])
        .unwrap()),
    )
}

async fn aufruf(
    server: &AbapServer,
    name: &str,
    argumente: Value,
) -> Result<CallToolResult, McpError> {
    let objekt = argumente.as_object().cloned().unwrap_or_default();
    server.aufrufen(name, Some(objekt)).await
}

/// Der Text einer Antwort.
fn text(ergebnis: CallToolResult) -> String {
    ergebnis.content[0]
        .as_text()
        .expect("keine Textantwort")
        .text
        .clone()
}

async fn antwort(server: &AbapServer, name: &str, argumente: Value) -> String {
    text(
        aufruf(server, name, argumente)
            .await
            .expect("unerwarteter Fehler"),
    )
}

// ------------------------------------------------------- Zugang und Anmeldung

#[tokio::test]
async fn status_zeigt_die_systeme_ohne_passwort() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    let text = antwort(&server, "abap_status", json!({})).await;
    assert!(text.contains("dev-100"), "{text}");
    assert!(text.contains(BENUTZER), "{text}");
    assert!(text.contains("Mandant 100"), "{text}");
    assert!(!text.contains(PASSWORT), "{text}");
    // `abap_status` öffnet keine Verbindung.
    assert!(attrappe.aufzeichnung.alle().is_empty());
}

#[tokio::test]
async fn test_connection_meldet_sich_an_und_zaehlt_die_bereiche() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    let text = antwort(&server, "abap_test_connection", json!({})).await;
    assert!(text.contains("Anmeldung an dev-100 erfolgreich"), "{text}");
    assert!(text.contains("ADT-Bereiche (2): Objekte, Daten"), "{text}");
    assert!(!text.contains(PASSWORT), "{text}");

    // Erst das CSRF-Token, dann die Discovery.
    assert_eq!(attrappe.token_holt(), 1);
    assert_eq!(attrappe.aufzeichnung.anzahl("core/discovery"), 1);
    let erster = attrappe.aufzeichnung.erster("compatibility/graph").unwrap();
    assert_eq!(erster.csrf.as_deref(), Some("fetch"));
    assert_eq!(erster.feld("sap-client"), Some("100"));
    assert_eq!(erster.feld("sap-language"), Some("DE"));
}

#[tokio::test]
async fn ein_falsches_passwort_wird_nicht_wiederholt() {
    let attrappe = Attrappe::starte().await;
    let server = server_mit_passwort(&attrappe.adresse, "falsch-4711");

    let fehler = aufruf(&server, "abap_search", json!({ "query": "ZCL_DEMO" }))
        .await
        .unwrap_err();
    assert!(fehler.message.contains("abgelehnt"), "{}", fehler.message);
    assert!(fehler.message.contains("dev-100"), "{}", fehler.message);
    assert!(
        !fehler.message.contains("falsch-4711"),
        "{}",
        fehler.message
    );

    // Genau ein Anmeldeversuch – SAP sperrt sonst den Benutzer.
    assert_eq!(attrappe.aufzeichnung.anzahl("compatibility/graph"), 1);
    assert_eq!(attrappe.aufzeichnung.anzahl("informationsystem/search"), 0);
}

#[tokio::test]
async fn ein_abgelehntes_csrf_token_wird_einmal_wiederholt() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    attrappe.csrf_erneuern();
    let text = antwort(&server, "abap_search", json!({ "query": "ZCL_DEMO" })).await;
    assert!(text.contains("ZCL_DEMO"), "{text}");

    // Ein Token beim Anmelden, ein zweites nach dem 403.
    assert_eq!(attrappe.token_holt(), 2);
    // Der Aufruf ging zweimal raus: einmal abgelehnt, einmal erfolgreich.
    assert_eq!(attrappe.aufzeichnung.anzahl("informationsystem/search"), 2);
}

#[tokio::test]
async fn eine_adt_ausnahme_kommt_als_meldung_an() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    let fehler = aufruf(&server, "abap_search", json!({ "query": "ZFEHLER" }))
        .await
        .unwrap_err();
    assert!(
        fehler.message.contains("ExceptionResourceNotFound"),
        "{}",
        fehler.message
    );
    assert!(
        fehler
            .message
            .contains("Die Suche hat kein Ergebnis geliefert."),
        "{}",
        fehler.message
    );
    assert!(!fehler.message.contains(PASSWORT), "{}", fehler.message);
}

#[tokio::test]
async fn mehrere_systeme_verlangen_einen_namen() {
    let attrappe = Attrappe::starte().await;
    let server = server_mit_zwei_systemen(&attrappe.adresse);

    let fehler = aufruf(&server, "abap_search", json!({ "query": "ZCL_DEMO" }))
        .await
        .unwrap_err();
    assert!(
        fehler.message.contains("mehrere Systeme"),
        "{}",
        fehler.message
    );

    let text = antwort(
        &server,
        "abap_search",
        json!({ "query": "ZCL_DEMO", "system": "qas-200" }),
    )
    .await;
    assert!(text.contains("ZCL_DEMO"), "{text}");
}

#[tokio::test]
async fn ohne_systeme_kommt_der_hinweis_und_kein_fehler() {
    let server = AbapServer::mit(
        Zugang::mit_umgebung(&Abap, |_| None),
        Ok(Sammlung::default()),
    );
    let text = antwort(&server, "abap_read", json!({ "name": "ZCL_DEMO" })).await;
    assert!(text.contains("Zugang fehlt"), "{text}");
    assert!(text.contains("Tools → ABAP"), "{text}");
}

// -------------------------------------------------------------- Lesen

#[tokio::test]
async fn die_suche_gibt_treffer_und_uri_zurueck() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    let text = antwort(
        &server,
        "abap_search",
        json!({ "query": "ZCL_*", "objectType": "CLAS/OC", "max": 5 }),
    )
    .await;
    assert!(text.contains("2 Objekt(e) für „ZCL_*“"), "{text}");
    assert!(text.contains("ZCL_DEMO | type=CLAS/OC"), "{text}");
    assert!(text.contains("/sap/bc/adt/oo/classes/zcl_demo"), "{text}");

    let aufruf = attrappe
        .aufzeichnung
        .erster("informationsystem/search")
        .unwrap();
    assert_eq!(aufruf.feld("operation"), Some("quickSearch"));
    assert_eq!(aufruf.feld("query"), Some("ZCL_*"));
    assert_eq!(aufruf.feld("objectType"), Some("CLAS"));
    assert_eq!(aufruf.feld("maxResults"), Some("5"));
    // Die Anmeldung läuft über Basic-Auth, das Passwort steht nur dort.
    assert!(aufruf
        .anmeldung
        .as_deref()
        .unwrap_or_default()
        .starts_with("Basic "));
    assert!(aufruf.cookie.unwrap_or_default().contains("SAP_SESSIONID"));
}

#[tokio::test]
async fn die_objektstruktur_nennt_includes_und_quelltext() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    let text = antwort(
        &server,
        "abap_object_structure",
        json!({ "name": "ZCL_DEMO" }),
    )
    .await;
    assert!(text.contains("Name: ZCL_DEMO"), "{text}");
    assert!(text.contains("Verantwortlich: DEVELOPER"), "{text}");
    assert!(text.contains("Includes: definitions, main"), "{text}");
    assert!(
        text.contains("Quelltext: /sap/bc/adt/oo/classes/zcl_demo/includes/implementations"),
        "{text}"
    );
}

#[tokio::test]
async fn der_quelltext_wird_gelesen_und_gekuerzt() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    let text = antwort(&server, "abap_read", json!({ "name": "ZCL_DEMO" })).await;
    assert!(text.contains("CLASS zcl_demo DEFINITION"), "{text}");
    assert!(text.contains("Zeile(n)"), "{text}");

    let text = antwort(
        &server,
        "abap_read",
        json!({ "name": "ZCL_DEMO", "max_zeichen": 40 }),
    )
    .await;
    assert!(text.contains("Zeichen weggelassen"), "{text}");
    // Der Hinweis nennt die Grenze.
    assert!(text.contains("max_zeichen=40"), "{text}");
}

#[tokio::test]
async fn eine_objekt_url_geht_vor_dem_namen() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    let text = antwort(
        &server,
        "abap_read",
        json!({ "objectUrl": "/sap/bc/adt/oo/classes/zcl_demo" }),
    )
    .await;
    assert!(text.contains("ZCL_DEMO"), "{text}");
    // Ohne Suche: die Struktur kommt direkt.
    assert_eq!(attrappe.aufzeichnung.anzahl("informationsystem/search"), 0);
}

#[tokio::test]
async fn ein_unbekannter_name_ist_ein_parameterfehler() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    let fehler = aufruf(&server, "abap_read", json!({ "name": "ZNIX" }))
        .await
        .unwrap_err();
    assert!(
        fehler.message.contains("Kein Objekt gefunden"),
        "{}",
        fehler.message
    );
}

#[tokio::test]
async fn wo_verwendet_liest_die_liste() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    let text = antwort(&server, "abap_where_used", json!({ "name": "ZCL_DEMO" })).await;
    assert!(text.contains("1 Verwendung(en)"), "{text}");
    assert!(text.contains("ZREPORT (PROG/P)"), "{text}");

    let aufruf = attrappe
        .aufzeichnung
        .erster("usageReferences")
        .expect("kein Aufruf");
    assert_eq!(aufruf.methode, "POST");
    assert_eq!(
        aufruf.feld("uri"),
        Some("/sap/bc/adt/oo/classes/zcl_demo/includes/implementations")
    );
    assert!(
        aufruf.rumpf.contains("usageReferenceRequest"),
        "{}",
        aufruf.rumpf
    );
}

#[tokio::test]
async fn die_objektarten_kommen_mit_typ_und_verwendung() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    let text = antwort(&server, "abap_object_types", json!({})).await;
    assert!(text.contains("2 Objektart(en)"), "{text}");
    assert!(
        text.contains("- CLAS (CLAS/OC) — Klasse [search, whereUsed]"),
        "{text}"
    );

    let aufruf = attrappe
        .aufzeichnung
        .erster("informationsystem/objecttypes")
        .unwrap();
    assert_eq!(aufruf.feld("maxItemCount"), Some("999"));
    assert_eq!(aufruf.feld("data"), Some("usedByProvider"));
}

#[tokio::test]
async fn der_paketinhalt_wird_gelesen() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    let text = antwort(
        &server,
        "abap_node_contents",
        json!({ "parentType": "DEVC/K", "parentName": "$TMP" }),
    )
    .await;
    assert!(text.contains("1 Objekt(e)"), "{text}");
    assert!(
        text.contains("- ZCL_DEMO  [CLAS/OC] (Demo-Klasse)"),
        "{text}"
    );

    let aufruf = attrappe.aufzeichnung.erster("nodestructure").unwrap();
    assert_eq!(aufruf.methode, "POST");
    assert_eq!(aufruf.feld("parent_type"), Some("DEVC/K"));
    assert_eq!(aufruf.feld("parent_name"), Some("$TMP"));
    assert_eq!(aufruf.feld("withShortDescriptions"), Some("true"));
}

#[tokio::test]
async fn eine_tabelle_wird_abgefragt_und_ausgerichtet() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    let text = antwort(
        &server,
        "abap_table",
        json!({ "table": "sflight", "max_zeilen": 5000, "filter": "CARRID = 'LH'" }),
    )
    .await;
    assert!(text.contains("CARRID | CONNID"), "{text}");
    assert!(text.contains("AA     | 0017"), "{text}");
    assert!(text.contains("2 Zeile(n)."), "{text}");

    let aufruf = attrappe.aufzeichnung.erster("datapreview/ddic").unwrap();
    assert_eq!(aufruf.methode, "POST");
    assert_eq!(aufruf.feld("ddicEntityName"), Some("SFLIGHT"));
    // 5000 wird auf 1000 gekappt.
    assert_eq!(aufruf.feld("rowNumber"), Some("1000"));
    assert_eq!(aufruf.rumpf, "WHERE CARRID = 'LH'");
}

#[tokio::test]
async fn eine_freie_abfrage_geht_an_den_sql_endpunkt() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    let text = antwort(
        &server,
        "abap_query",
        json!({ "sql": "SELECT carrid, connid FROM sflight" }),
    )
    .await;
    assert!(text.contains("2 Zeile(n)."), "{text}");

    let aufruf = attrappe
        .aufzeichnung
        .erster("datapreview/freestyle")
        .unwrap();
    assert_eq!(aufruf.rumpf, "SELECT carrid, connid FROM sflight");
    // Ohne Angabe: 100 Zeilen.
    assert_eq!(aufruf.feld("rowNumber"), Some("100"));
}

#[tokio::test]
async fn der_syntaxcheck_schickt_den_quelltext_base64() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    let text = antwort(&server, "abap_syntax_check", json!({ "name": "ZCL_DEMO" })).await;
    assert!(text.contains("[W] Zeile 3"), "{text}");
    assert!(
        text.contains("Die Methode RUN wird nie aufgerufen."),
        "{text}"
    );

    let aufruf = attrappe.aufzeichnung.erster("checkruns").unwrap();
    assert_eq!(aufruf.feld("reporters"), Some("abapCheckRun"));
    assert!(
        aufruf.rumpf.contains(&BASE64.encode(
            "CLASS zcl_demo DEFINITION PUBLIC FINAL CREATE PUBLIC.\n  PUBLIC SECTION.\n    METHODS run.\nENDCLASS.\n\nCLASS zcl_demo IMPLEMENTATION.\n  METHOD run.\n    WRITE 'Hallo Attrappe'.\n  ENDMETHOD.\nENDCLASS.\n"
        )),
        "{}",
        aufruf.rumpf
    );
}

#[tokio::test]
async fn ein_uebergebener_quelltext_wird_geprueft_statt_des_gespeicherten() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    antwort(
        &server,
        "abap_syntax_check",
        json!({ "name": "ZCL_DEMO", "source": "REPORT zreport." }),
    )
    .await;

    let aufruf = attrappe.aufzeichnung.erster("checkruns").unwrap();
    assert!(
        aufruf.rumpf.contains(&BASE64.encode("REPORT zreport.")),
        "{}",
        aufruf.rumpf
    );
    // Der gespeicherte Quelltext wurde nicht geholt.
    assert_eq!(attrappe.aufzeichnung.anzahl("includes/implementations"), 0);
}

#[tokio::test]
async fn unit_tests_zeigen_methoden_und_fehlschlaege() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    let text = antwort(
        &server,
        "abap_unit_test",
        json!({ "name": "ZCL_DEMO", "critical": true }),
    )
    .await;
    assert!(
        text.starts_with("2 Testmethode(n), 1 fehlgeschlagen."),
        "{text}"
    );
    assert!(text.contains("  - TEST_GUT: bestanden"), "{text}");
    assert!(text.contains("  - TEST_SCHLECHT: fehlgeschlagen"), "{text}");
    assert!(text.contains("Erwartet 1, war 2"), "{text}");

    let aufruf = attrappe.aufzeichnung.erster("abapunit/testruns").unwrap();
    assert!(
        aufruf.rumpf.contains("critical=\"true\""),
        "{}",
        aufruf.rumpf
    );
    assert!(
        aufruf
            .rumpf
            .contains("adtcore:uri=\"/sap/bc/adt/oo/classes/zcl_demo\""),
        "{}",
        aufruf.rumpf
    );
}

#[tokio::test]
async fn atc_startet_einen_lauf_und_liest_die_befunde() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    let text = antwort(&server, "abap_atc", json!({ "name": "ZCL_DEMO" })).await;
    assert!(text.contains("1 ATC-Befund(e)"), "{text}");
    assert!(
        text.contains("[P1] ZCL_DEMO (CLAS/OC): Sicherheitsprüfung"),
        "{text}"
    );
    assert!(text.contains("(Zeile 42)"), "{text}");

    let lauf = attrappe.aufzeichnung.erster("atc/runs").unwrap();
    assert_eq!(lauf.feld("worklistId"), Some("DEFAULT"));
    let liste = attrappe
        .aufzeichnung
        .erster("atc/worklists/WL4711")
        .unwrap();
    assert_eq!(liste.feld("includeExemptedFindings"), Some("false"));
}

#[tokio::test]
async fn dumps_traces_und_auswertung() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    let text = antwort(&server, "abap_dumps", json!({})).await;
    assert!(text.contains("1 Kurzspeicherauszug"), "{text}");
    assert!(
        text.contains("- text [20260101-120000-1]: MESSAGE_TYPE_UNKNOWN"),
        "{text}"
    );

    let text = antwort(&server, "abap_traces", json!({})).await;
    assert!(text.contains("1 Aufzeichnung(en)"), "{text}");
    assert!(
        text.contains("- Aufzeichnung 1 (ZREPORT) [Aktiv] 12s — id=/sap/bc/adt/runtime/traces/abaptraces/T0001"),
        "{text}"
    );
    // Ohne Benutzer gilt der angemeldete.
    assert_eq!(
        attrappe
            .aufzeichnung
            .erster("runtime/traces/abaptraces")
            .unwrap()
            .feld("user"),
        Some("DEVELOPER")
    );

    let text = antwort(&server, "abap_trace_hitlist", json!({ "traceId": "T0001" })).await;
    assert!(
        text.contains("Aussagen (aggregierter Aufrufbaum):"),
        "{text}"
    );
    assert!(
        text.contains("CALL METHOD — Aufrufe=3, brutto=1.5 ms"),
        "{text}"
    );
    assert!(
        text.contains("LOOP über flight — Aufrufe=7, brutto=42.5 ms (61 %)"),
        "{text}"
    );
    assert_eq!(
        attrappe.aufzeichnung.anzahl("T0001/statements"),
        1,
        "die Aussagen wurden geholt"
    );
}

#[tokio::test]
async fn transporte_und_ein_auftrag_im_detail() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    let text = antwort(&server, "abap_transports", json!({})).await;
    assert!(text.contains("1 Auftrag/Aufträge"), "{text}");
    assert!(text.contains("Workbench:"), "{text}");
    assert!(text.contains("DEVK900123 [D] Demo-Auftrag"), "{text}");
    assert_eq!(
        attrappe
            .aufzeichnung
            .erster("cts/transportrequests")
            .unwrap()
            .feld("targets"),
        Some("true")
    );

    let text = antwort(
        &server,
        "abap_transport_details",
        json!({ "transportNumber": "devk900123" }),
    )
    .await;
    assert!(text.contains("Transport: DEVK900123"), "{text}");
    assert!(text.contains("Inhaber: DEVELOPER"), "{text}");
    assert!(text.contains("  - ZCL_DEMO (CLAS/OC)"), "{text}");
}

#[tokio::test]
async fn textelemente_werden_gelesen_und_404_ist_leer() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    let text = antwort(&server, "abap_text_elements", json!({ "name": "ZCL_DEMO" })).await;
    assert!(
        text.contains("2 Textelement(e) der Art „symbols“ für ZCL_DEMO"),
        "{text}"
    );
    assert!(text.contains("- 001 = Anzahl (max 30)"), "{text}");

    // Ein Objekt ohne Selectionstexte antwortet mit 404 – das ist kein Fehler.
    let text = antwort(
        &server,
        "abap_text_elements",
        json!({ "name": "ZCL_DEMO", "category": "selections" }),
    )
    .await;
    assert!(
        text.contains("Keine Textelemente der Art „selections“"),
        "{text}"
    );

    let fehler = aufruf(
        &server,
        "abap_text_elements",
        json!({ "name": "ZCL_DEMO", "category": "quatsch" }),
    )
    .await
    .unwrap_err();
    assert!(fehler.message.contains("symbols"), "{}", fehler.message);
}

// ------------------------------------------------------------ Schreiben

/// Der Quelltext, den die Tests schreiben.
const NEUER_QUELLTEXT: &str = "CLASS zcl_demo DEFINITION PUBLIC FINAL CREATE PUBLIC.\n  PUBLIC SECTION.\n    METHODS run.\nENDCLASS.";

#[tokio::test]
async fn sperren_schreiben_und_entsperren_laufen_in_einer_sitzung() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    // 1) Sperren.
    let text = antwort(&server, "abap_lock", json!({ "name": "ZCL_DEMO" })).await;
    assert!(text.contains("ZCL_DEMO ist gesperrt"), "{text}");
    assert!(text.contains("Lock-Handle: LH1"), "{text}");
    assert!(text.contains("nicht nötig (lokales Objekt)"), "{text}");
    assert!(text.contains("abap_unlock"), "{text}");

    let lock = attrappe
        .aufzeichnung
        .zu_mit("oo/classes/zcl_demo", "POST")
        .pop()
        .expect("kein LOCK");
    assert_eq!(lock.methode, "POST");
    assert_eq!(lock.feld("_action"), Some("LOCK"));
    assert_eq!(lock.feld("accessMode"), Some("MODIFY"));
    assert!(lock.ist_stateful(), "die Sperre braucht eine Sitzung");
    assert_eq!(attrappe.offene_locks(), 1);

    // 2) Schreiben mit dem Handle – die Sperre bleibt stehen.
    let text = antwort(
        &server,
        "abap_write",
        json!({ "name": "ZCL_DEMO", "source": NEUER_QUELLTEXT, "lockHandle": "LH1" }),
    )
    .await;
    assert!(text.contains("Quelltext geschrieben"), "{text}");
    assert!(text.contains("Lock bleibt"), "{text}");
    assert!(text.contains("abap_activate"), "{text}");
    assert_eq!(attrappe.offene_locks(), 1, "die Sperre bleibt stehen");
    assert_eq!(attrappe.unlocks(), 0);

    let put = attrappe
        .aufzeichnung
        .zu_mit("oo/classes/zcl_demo", "PUT")
        .pop()
        .expect("kein PUT");
    assert_eq!(
        put.pfad,
        "/sap/bc/adt/oo/classes/zcl_demo/includes/implementations"
    );
    assert_eq!(put.feld("lockHandle"), Some("LH1"));
    assert_eq!(put.rumpf, NEUER_QUELLTEXT);
    assert!(put.ist_stateful(), "das Schreiben gehört zur Sitzung");
    // Dieselbe Sitzung wie die Sperre.
    assert_eq!(put.cookie, lock.cookie);

    // 3) Freigeben.
    let text = antwort(
        &server,
        "abap_unlock",
        json!({ "name": "ZCL_DEMO", "lockHandle": "LH1" }),
    )
    .await;
    assert!(text.contains("ZCL_DEMO ist freigegeben"), "{text}");
    assert_eq!(attrappe.offene_locks(), 0);
    assert_eq!(attrappe.unlocks(), 1);
}

#[tokio::test]
async fn schreiben_ohne_handle_sperrt_schreibt_und_gibt_frei() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    let text = antwort(
        &server,
        "abap_write",
        json!({
            "name": "ZCL_DEMO",
            "source": NEUER_QUELLTEXT,
            "transport": "DEVK900123"
        }),
    )
    .await;
    assert!(text.contains("Quelltext geschrieben"), "{text}");
    assert!(text.contains("Lock freigegeben"), "{text}");
    assert_eq!(attrappe.offene_locks(), 0);
    assert_eq!(attrappe.unlocks(), 1);

    let sperren = attrappe.aufzeichnung.zu_mit("zcl_demo", "POST");
    assert_eq!(
        sperren
            .iter()
            .filter(|aufruf| aufruf.feld("_action") == Some("LOCK"))
            .count(),
        1
    );
    let puts = attrappe.aufzeichnung.zu_mit("zcl_demo", "PUT");
    assert_eq!(puts.len(), 1);
    assert_eq!(puts[0].feld("corrNr"), Some("DEVK900123"));
    let entsperren = attrappe.aufzeichnung.zu_mit("zcl_demo", "POST");
    assert!(entsperren
        .iter()
        .any(|aufruf| aufruf.feld("_action") == Some("UNLOCK")));
}

#[tokio::test]
async fn ein_gescheitertes_schreiben_gibt_die_sperre_trotzdem_frei() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    attrappe.put_scheitert();
    let fehler = aufruf(
        &server,
        "abap_write",
        json!({ "name": "ZCL_DEMO", "source": NEUER_QUELLTEXT }),
    )
    .await
    .unwrap_err();
    assert!(
        fehler.message.contains("nicht speichern"),
        "{}",
        fehler.message
    );
    assert!(!fehler.message.contains(PASSWORT), "{}", fehler.message);

    // Wie `finally` in der Vorlage: entsperrt, obwohl das Schreiben scheiterte.
    assert_eq!(attrappe.unlocks(), 1);
    assert_eq!(attrappe.offene_locks(), 0);
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("zcl_demo", "PUT"), 1);
}

#[tokio::test]
async fn ein_unbekannter_handle_wird_ohne_anfrage_abgelehnt() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    let fehler = aufruf(
        &server,
        "abap_write",
        json!({
            "name": "ZCL_DEMO",
            "source": NEUER_QUELLTEXT,
            "lockHandle": "LH-FREMD"
        }),
    )
    .await
    .unwrap_err();
    assert!(
        fehler.message.contains("Lock-Handle unbekannt"),
        "{}",
        fehler.message
    );
    assert!(fehler.message.contains("abap_lock"), "{}", fehler.message);
    // Keine einzige Anfrage an SAP.
    assert!(attrappe.aufzeichnung.alle().is_empty());

    // Auch abap_unlock lehnt einen fremden Handle ohne Anfrage ab.
    let fehler = aufruf(
        &server,
        "abap_unlock",
        json!({ "name": "ZCL_DEMO", "lockHandle": "LH-FREMD" }),
    )
    .await
    .unwrap_err();
    assert!(
        fehler.message.contains("Lock-Handle unbekannt"),
        "{}",
        fehler.message
    );
    assert!(attrappe.aufzeichnung.alle().is_empty());
}

#[tokio::test]
async fn ohne_schreiben_antworten_alle_schreibenden_werkzeuge_ohne_anfrage() {
    let attrappe = Attrappe::starte().await;
    let server = server_ohne_schreiben(&attrappe.adresse);

    let faelle = [
        ("abap_lock", json!({ "name": "ZCL_DEMO" })),
        (
            "abap_unlock",
            json!({ "name": "ZCL_DEMO", "lockHandle": "LH1" }),
        ),
        (
            "abap_write",
            json!({ "name": "ZCL_DEMO", "source": NEUER_QUELLTEXT }),
        ),
        ("abap_activate", json!({ "name": "ZCL_DEMO" })),
        (
            "abap_create_object",
            json!({
                "objectType": "CLAS/OC",
                "name": "ZCL_NEU",
                "parentName": "$TMP",
                "description": "Neu"
            }),
        ),
        (
            "abap_set_text_elements",
            json!({
                "name": "ZCL_DEMO",
                "category": "symbols",
                "elements": [{ "id": "001", "text": "Anzahl" }]
            }),
        ),
    ];
    for (name, argumente) in faelle {
        let text = antwort(&server, name, argumente).await;
        assert!(
            text.contains("Schreiben ist für dev-100 aus"),
            "{name}: {text}"
        );
        assert!(text.contains("Tools → ABAP"), "{name}: {text}");
        assert!(text.contains("‚schreiben‘ einschalten"), "{name}: {text}");
    }
    // Kein einziger Aufruf an SAP.
    assert!(attrappe.aufzeichnung.alle().is_empty());
}

#[tokio::test]
async fn fremder_namensraum_wird_ohne_anfrage_abgelehnt() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    // Ein SAP-Standardobjekt.
    let fehler = aufruf(
        &server,
        "abap_write",
        json!({ "name": "CL_ABAP_TYPEDESCR", "source": NEUER_QUELLTEXT }),
    )
    .await
    .unwrap_err();
    assert!(
        fehler.message.contains("Kundennamensraum"),
        "{}",
        fehler.message
    );

    // Auch über eine Objekt-URL erkannt.
    let fehler = aufruf(
        &server,
        "abap_lock",
        json!({ "objectUrl": "/sap/bc/adt/oo/classes/cl_abap_typedescr" }),
    )
    .await
    .unwrap_err();
    assert!(
        fehler.message.contains("Kundennamensraum"),
        "{}",
        fehler.message
    );

    // Ein Paket außerhalb des Namensraums.
    let fehler = aufruf(
        &server,
        "abap_create_object",
        json!({
            "objectType": "CLAS/OC",
            "name": "ZCL_NEU",
            "parentName": "SAPBC",
            "description": "Neu"
        }),
    )
    .await
    .unwrap_err();
    assert!(
        fehler.message.contains("nicht erlaubt"),
        "{}",
        fehler.message
    );

    assert!(attrappe.aufzeichnung.alle().is_empty());
}

#[tokio::test]
async fn ein_zu_grosser_quelltext_wird_ohne_anfrage_abgelehnt() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    let zu_gross = "x".repeat(2 * 1024 * 1024 + 1);
    let fehler = aufruf(
        &server,
        "abap_write",
        json!({ "name": "ZCL_DEMO", "source": zu_gross }),
    )
    .await
    .unwrap_err();
    assert!(fehler.message.contains("zu groß"), "{}", fehler.message);
    assert!(attrappe.aufzeichnung.alle().is_empty());
}

#[tokio::test]
async fn aktivieren_zeigt_die_meldungen_mit_zeile() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    let text = antwort(&server, "abap_activate", json!({ "name": "ZCL_DEMO" })).await;
    assert!(
        text.contains("Aktivierung von ZCL_DEMO nicht erfolgreich"),
        "{text}"
    );
    assert!(
        text.contains("[E] Zeile 3: Die Anweisung WRITE ist hier nicht erlaubt."),
        "{text}"
    );
    assert!(text.contains("[W] Zeile 1"), "{text}");

    let aufruf = attrappe.aufzeichnung.erster("adt/activation").unwrap();
    assert_eq!(aufruf.methode, "POST");
    assert_eq!(aufruf.feld("method"), Some("activate"));
    assert_eq!(aufruf.feld("preauditRequested"), Some("true"));
    assert!(
        aufruf
            .rumpf
            .contains("adtcore:uri=\"/sap/bc/adt/oo/classes/zcl_demo?context="),
        "{}",
        aufruf.rumpf
    );
    assert!(
        aufruf.ist_stateful(),
        "die Aktivierung läuft in der Sitzung"
    );
}

#[tokio::test]
async fn anlegen_nimmt_die_richtigen_pfade_und_rumpf() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    let text = antwort(
        &server,
        "abap_create_object",
        json!({
            "objectType": "CLAS/OC",
            "name": "ZCL_NEU",
            "parentName": "$TMP",
            "description": "Neue Klasse",
            "transport": "DEVK900123"
        }),
    )
    .await;
    assert!(text.contains("CLAS/OC ZCL_NEU in $TMP angelegt"), "{text}");
    assert!(text.contains("/sap/bc/adt/oo/classes/zcl_neu"), "{text}");

    let klasse = attrappe.aufzeichnung.erster("oo/classes").unwrap();
    assert_eq!(klasse.methode, "POST");
    assert_eq!(klasse.pfad, "/sap/bc/adt/oo/classes");
    assert_eq!(klasse.feld("corrNr"), Some("DEVK900123"));
    assert!(
        klasse.rumpf.contains("<class:abapClass"),
        "{}",
        klasse.rumpf
    );
    assert!(
        klasse.rumpf.contains("adtcore:name=\"ZCL_NEU\""),
        "{}",
        klasse.rumpf
    );
    assert!(
        klasse
            .rumpf
            .contains("<adtcore:packageRef adtcore:name=\"$TMP\"/>"),
        "{}",
        klasse.rumpf
    );

    // Ein Programm geht an den Programm-Pfad, mit eigener Wurzel.
    let text = antwort(
        &server,
        "abap_create_object",
        json!({
            "objectType": "PROG/P",
            "name": "ZREPORT",
            "parentName": "Z_PAKET",
            "description": "Report"
        }),
    )
    .await;
    assert!(
        text.contains("PROG/P ZREPORT in Z_PAKET angelegt"),
        "{text}"
    );
    let programm = attrappe
        .aufzeichnung
        .zu("programs/programs")
        .into_iter()
        .find(|aufruf| aufruf.methode == "POST")
        .expect("kein Programm angelegt");
    assert!(programm.rumpf.contains("<program:abapProgram"));
    assert!(programm
        .rumpf
        .contains("<adtcore:packageRef adtcore:name=\"Z_PAKET\"/>"));
}

#[tokio::test]
async fn textelemente_werden_im_format_der_vorlage_geschrieben() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    let text = antwort(
        &server,
        "abap_set_text_elements",
        json!({
            "name": "ZCL_DEMO",
            "category": "symbols",
            "elements": [
                { "id": "001", "text": "Anzahl", "maxLength": 30 },
                { "id": "002", "text": "Summe" }
            ]
        }),
    )
    .await;
    assert!(
        text.contains("2 Textelement(e) der Art „symbols“ für ZCL_DEMO geschrieben"),
        "{text}"
    );
    assert!(text.contains("Lock freigegeben"), "{text}");

    let put = attrappe
        .aufzeichnung
        .zu_mit("textelements", "PUT")
        .pop()
        .expect("kein PUT auf Textelemente");
    assert_eq!(
        put.pfad,
        "/sap/bc/adt/textelements/classes/zcl_demo/source/symbols"
    );
    assert_eq!(put.rumpf, "@MaxLength:30\n001=Anzahl\n\n002=Summe\n");
    assert!(put.ist_stateful());
    assert!(put.feld("lockHandle").is_some());
    // Gesperrt und wieder freigegeben.
    assert_eq!(attrappe.unlocks(), 1);
    assert_eq!(attrappe.offene_locks(), 0);
}

#[tokio::test]
async fn eine_abgelaufene_sitzung_meldet_sich_und_wiederholt_nichts() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    // Sperren (Sitzung entsteht) …
    antwort(&server, "abap_lock", json!({ "name": "ZCL_DEMO" })).await;
    assert_eq!(attrappe.offene_locks(), 1);

    // … dann läuft die Sitzung ab.
    attrappe.sitzung_ablaufen();
    let fehler = aufruf(
        &server,
        "abap_write",
        json!({ "name": "ZCL_DEMO", "source": NEUER_QUELLTEXT, "lockHandle": "LH1" }),
    )
    .await
    .unwrap_err();
    assert!(fehler.message.contains("abgelaufen"), "{}", fehler.message);
    assert!(fehler.message.contains("abap_lock"), "{}", fehler.message);
    // Kein zweiter Versuch: genau ein PUT ging hinaus.
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("zcl_demo", "PUT"), 1);
    // Die Lock-Tabelle ist verworfen: der Handle gilt nicht mehr.
    let fehler = aufruf(
        &server,
        "abap_write",
        json!({ "name": "ZCL_DEMO", "source": NEUER_QUELLTEXT, "lockHandle": "LH1" }),
    )
    .await
    .unwrap_err();
    assert!(
        fehler.message.contains("Lock-Handle unbekannt"),
        "{}",
        fehler.message
    );
}

#[tokio::test]
async fn ein_abgelehntes_csrf_token_wird_in_der_sitzung_erneuert() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    antwort(&server, "abap_lock", json!({ "name": "ZCL_DEMO" })).await;

    attrappe.csrf_erneuern_in_sitzung();
    let text = antwort(
        &server,
        "abap_write",
        json!({ "name": "ZCL_DEMO", "source": NEUER_QUELLTEXT, "lockHandle": "LH1" }),
    )
    .await;
    assert!(text.contains("Quelltext geschrieben"), "{text}");

    // Einmal abgelehnt, einmal erfolgreich – und zwar **in derselben Sitzung**:
    // gleicher Keks, neues Token.
    let puts = attrappe.aufzeichnung.zu_mit("zcl_demo", "PUT");
    assert_eq!(puts.len(), 2, "einmal abgelehnt, einmal wiederholt");
    assert_eq!(puts[0].cookie, puts[1].cookie, "dieselbe Sitzung");
    assert_ne!(puts[0].csrf, puts[1].csrf, "neues Token");
    assert!(puts.iter().all(|aufruf| aufruf.ist_stateful()));
}

#[tokio::test]
async fn eine_abgelehnte_anmeldung_wird_nicht_wiederholt() {
    let attrappe = Attrappe::starte().await;
    let server = server_mit_passwort(&attrappe.adresse, "falsch-4711");

    let fehler = aufruf(&server, "abap_lock", json!({ "name": "ZCL_DEMO" }))
        .await
        .unwrap_err();
    assert!(fehler.message.contains("abgelehnt"), "{}", fehler.message);
    assert!(
        !fehler.message.contains("falsch-4711"),
        "{}",
        fehler.message
    );
    // Genau ein Anmeldeversuch.
    assert_eq!(attrappe.aufzeichnung.anzahl("compatibility/graph"), 1);
}

#[tokio::test]
async fn eine_andere_sitzung_darf_nicht_entsperren() {
    let attrappe = Attrappe::starte().await;

    // Zwei Server auf dasselbe SAP-System: zwei getrennte Sitzungen.
    let einer = server(&attrappe.adresse);
    let anderer = server(&attrappe.adresse);

    antwort(&einer, "abap_lock", json!({ "name": "ZCL_DEMO" })).await;
    assert_eq!(attrappe.offene_locks(), 1);

    // Der andere Server weiß nichts von der Sperre: keine Anfrage, klare Meldung.
    let fehler = aufruf(
        &anderer,
        "abap_unlock",
        json!({ "name": "ZCL_DEMO", "lockHandle": "LH1" }),
    )
    .await
    .unwrap_err();
    assert!(
        fehler.message.contains("Lock-Handle unbekannt"),
        "{}",
        fehler.message
    );
    assert_eq!(attrappe.offene_locks(), 1);
}

#[tokio::test]
async fn textelemente_pruefen_ihre_schluessel_vor_der_anfrage() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    let fehler = aufruf(
        &server,
        "abap_set_text_elements",
        json!({
            "name": "ZCL_DEMO",
            "category": "symbols",
            "elements": [{ "id": "01", "text": "Zu kurz" }]
        }),
    )
    .await
    .unwrap_err();
    assert!(
        fehler.message.contains("drei Zeichen"),
        "{}",
        fehler.message
    );

    let fehler = aufruf(
        &server,
        "abap_set_text_elements",
        json!({
            "name": "ZCL_DEMO",
            "category": "quatsch",
            "elements": [{ "id": "001", "text": "Anzahl" }]
        }),
    )
    .await
    .unwrap_err();
    assert!(fehler.message.contains("symbols"), "{}", fehler.message);
    assert!(attrappe.aufzeichnung.alle().is_empty());
}

#[tokio::test]
async fn name_und_fremde_url_zusammen_werden_ohne_anfrage_abgelehnt() {
    // Geschrieben wird an die URL – also muss auch sie im Kundennamensraum
    // liegen, nicht nur der Name.
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);
    for werkzeug in ["abap_write", "abap_lock", "abap_activate"] {
        let fehler = aufruf(
            &server,
            werkzeug,
            json!({
                "name": "ZCL_DEMO",
                "objectUrl": "/sap/bc/adt/oo/classes/cl_abap_typedescr",
                "source": NEUER_QUELLTEXT
            }),
        )
        .await
        .unwrap_err();
        assert!(
            fehler.message.contains("Kundennamensraum"),
            "{werkzeug}: {}",
            fehler.message
        );
    }
    assert!(attrappe.aufzeichnung.alle().is_empty());
}

#[tokio::test]
async fn beim_schreiben_zaehlt_nur_ein_exakter_treffer() {
    // Die Suche der Attrappe liefert immer ZCL_DEMO – für „ZCL_ANDERS“ ist das
    // kein exakter Treffer, also wird nichts gesperrt und nichts geschrieben.
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);
    let fehler = aufruf(
        &server,
        "abap_write",
        json!({ "name": "ZCL_ANDERS", "source": NEUER_QUELLTEXT }),
    )
    .await
    .unwrap_err();
    assert!(
        fehler.message.contains("nicht eindeutig"),
        "{}",
        fehler.message
    );
    // Gesucht und gelesen darf werden, gesperrt (POST) und geschrieben (PUT) nicht.
    assert_eq!(
        attrappe.aufzeichnung.anzahl_mit("/sap/bc/adt/oo/", "POST"),
        0
    );
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("/sap/bc/adt/", "PUT"), 0);
}

#[tokio::test]
async fn nur_ein_select_geht_an_die_datenvorschau() {
    let attrappe = Attrappe::starte().await;
    let server = server(&attrappe.adresse);

    for falsch in [
        "DELETE FROM sflight",
        "UPDATE sflight SET carrid = 'X'",
        "SELECT carrid FROM sflight; DELETE FROM sflight",
        "SELECT carrid INTO TABLE @data(t) FROM sflight",
        "SELECT carrid FROM sflight FOR UPDATE",
        "SELECT carrid FROM sflight WHERE carrid = 'LH",
    ] {
        let fehler = server
            .aufrufen(
                "abap_query",
                Some(serde_json::from_value(json!({ "sql": falsch })).unwrap()),
            )
            .await
            .unwrap_err();
        assert!(
            fehler.message.contains("SELECT")
                || fehler.message.contains("Semikolon")
                || fehler.message.contains("Kette")
                || fehler.message.contains("INTO")
                || fehler.message.contains("FOR UPDATE")
                || fehler.message.contains("Zeichenkette"),
            "{falsch}: {}",
            fehler.message
        );
    }
    // Keine einzige Anfrage an die Vorschau.
    assert_eq!(attrappe.aufzeichnung.anzahl("datapreview"), 0);

    // Ein harmloses SELECT geht weiterhin durch, auch mit Semikolon in einer
    // Zeichenkette.
    let text = antwort(
        &server,
        "abap_query",
        json!({ "sql": "SELECT carrid FROM sflight WHERE carrid = 'a;b'" }),
    )
    .await;
    assert!(text.contains("Zeile(n)"), "{text}");
    assert_eq!(attrappe.aufzeichnung.anzahl("datapreview/freestyle"), 1);

    // Der Filter von abap_table nimmt kein Semikolon und keinen Kommentar.
    for falsch in ["CARRID = 'LH'; DELETE FROM sflight", "1 = 1 -- immer wahr"] {
        let fehler = server
            .aufrufen(
                "abap_table",
                Some(
                    serde_json::from_value(json!({ "table": "SFLIGHT", "filter": falsch }))
                        .unwrap(),
                ),
            )
            .await
            .unwrap_err();
        assert!(
            fehler.message.contains("Semikolon") || fehler.message.contains("Kommentar"),
            "{falsch}: {}",
            fehler.message
        );
    }
    assert_eq!(attrappe.aufzeichnung.anzahl("datapreview/ddic"), 0);
}
