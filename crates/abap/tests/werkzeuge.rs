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
fn eintrag(adresse: &str, name: &str, passwort: &str) -> Eintrag {
    Eintrag {
        name: name.to_string(),
        werte: [
            ("benutzer", BENUTZER),
            ("url", adresse),
            ("mandant", "100"),
            ("sprache", "DE"),
            ("passwort", passwort),
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
