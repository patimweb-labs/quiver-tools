// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Werkzeuge gegen die Jira-Attrappe (SPEC §7, Aufgaben 12 und 13).
//!
//! Jeder Fall prüft eine Wirkung: was bei Jira gefragt wurde (von der Attrappe
//! aufgezeichnet) und was beim Agenten ankommt. Ein echtes Jira ist nie im
//! Spiel.

mod attrappe;

use attrappe::{
    Attrappe, ELTERN, EMAIL_A, FILTER, KOMMENTAR, PROJEKT, PROJEKT_FREMD, TOKEN_A, TOKEN_B, TYP,
    VORGANG, VORGANG_FREMD,
};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use quiver_jira::sites::Sammlung;
use quiver_jira::{Jira, JiraServer};
use quiver_tools_kern::zugaenge::Zugang as Eintrag;
use quiver_tools_kern::Zugang;
use rmcp::model::CallToolResult;
use rmcp::ErrorData as McpError;
use serde_json::{json, Value};

/// Eine Site für die Attrappe.
fn eintrag(name: &str, adresse: &str, schreiben: bool, nur: Option<&str>) -> Eintrag {
    let mut werte: std::collections::BTreeMap<String, String> = [
        ("url", adresse),
        ("email", EMAIL_A),
        ("api_token", TOKEN_A),
        ("schreiben", if schreiben { "an" } else { "aus" }),
    ]
    .into_iter()
    .map(|(feld, wert)| (feld.to_string(), wert.to_string()))
    .collect();
    if let Some(nur) = nur {
        werte.insert("nur".to_string(), nur.to_string());
    }
    Eintrag {
        name: name.to_string(),
        werte,
    }
}

fn server(eintraege: Vec<Eintrag>) -> JiraServer {
    JiraServer::mit(
        Zugang::mit_umgebung(&Jira, |_| None),
        Ok(Sammlung::aus_zugaengen(eintraege).unwrap()),
    )
}

/// Eine Site mit vollem Zugriff; Schreiben ist an.
fn eine(adresse: &str) -> JiraServer {
    server(vec![eintrag("firma", adresse, true, None)])
}

/// Eine Site mit vollem Zugriff; Schreiben ist **aus**.
fn eine_ohne_schreiben(adresse: &str) -> JiraServer {
    server(vec![eintrag("firma", adresse, false, None)])
}

/// Zwei Sites (die Attrappe akzeptiert beide Anmeldungen).
fn zwei(adresse: &str) -> JiraServer {
    server(vec![
        eintrag("firma", adresse, true, None),
        eintrag("kunde", adresse, false, None),
    ])
}

async fn aufruf(
    server: &JiraServer,
    name: &str,
    argumente: Value,
) -> Result<CallToolResult, McpError> {
    let objekt = argumente.as_object().cloned().unwrap_or_default();
    server.aufrufen(name, Some(objekt)).await
}

fn text(ergebnis: CallToolResult) -> String {
    ergebnis.content[0]
        .as_text()
        .expect("keine Textantwort")
        .text
        .clone()
}

async fn antwort(server: &JiraServer, name: &str, argumente: Value) -> String {
    text(
        aufruf(server, name, argumente)
            .await
            .expect("unerwarteter Fehler"),
    )
}

/// Alle Antworten dieses Tests dürfen weder Token noch E-Mail-Kopf enthalten.
fn ohne_token(text: &str) {
    assert!(!text.contains(TOKEN_A), "Token in der Antwort: {text}");
    assert!(!text.contains(TOKEN_B), "Token in der Antwort: {text}");
}

// ------------------------------------------------------------------ Zugang

#[tokio::test]
async fn status_zeigt_die_sites_ohne_token() {
    let attrappe = Attrappe::starte().await;
    let server = zwei(&attrappe.adresse);

    let text = antwort(&server, "jira_status", json!({})).await;
    assert!(text.contains("2 Site(s)"), "{text}");
    assert!(text.contains("firma"), "{text}");
    assert!(text.contains("kunde"), "{text}");
    assert!(text.contains("Schreiben an"), "{text}");
    assert!(text.contains("Schreiben aus"), "{text}");
    ohne_token(&text);
    // `jira_status` öffnet keine Verbindung.
    assert!(attrappe.aufzeichnung.alle().is_empty());
}

#[tokio::test]
async fn testen_meldet_sich_an_und_nennt_den_benutzer() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(&server, "jira_testen", json!({})).await;
    assert!(text.contains("Anmeldung an firma erfolgreich"), "{text}");
    assert!(text.contains(attrappe::BENUTZER), "{text}");
    assert!(text.contains(attrappe::BENUTZER_ID), "{text}");
    ohne_token(&text);

    // Genau eine Anfrage, mit Basic `email:token`.
    assert_eq!(attrappe.aufzeichnung.anzahl("myself"), 1);
    let aufruf = attrappe.aufzeichnung.letzter().unwrap();
    let kodiert = aufruf
        .anmeldung
        .expect("kein Authorization-Kopf")
        .strip_prefix("Basic ")
        .unwrap()
        .to_string();
    let lesbar = BASE64.decode(kodiert).unwrap();
    assert_eq!(
        String::from_utf8(lesbar).unwrap(),
        format!("{EMAIL_A}:{TOKEN_A}")
    );
}

#[tokio::test]
async fn ohne_zugang_kommt_der_hinweis_und_kein_fehler() {
    let server = server(vec![]);
    let text = antwort(&server, "jira_projekte", json!({})).await;
    assert!(text.contains("Zugang fehlt"), "{text}");
    assert!(text.contains("Tools → Jira"), "{text}");
}

#[tokio::test]
async fn eine_site_darf_ohne_namen_gewaehlt_werden() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);
    let text = antwort(&server, "jira_projekte", json!({})).await;
    assert!(text.contains(PROJEKT), "{text}");
}

#[tokio::test]
async fn zwei_sites_verlangen_einen_namen() {
    let attrappe = Attrappe::starte().await;
    let server = zwei(&attrappe.adresse);

    let fehler = aufruf(&server, "jira_projekte", json!({}))
        .await
        .unwrap_err();
    assert!(
        fehler.message.contains("mehrere Sites"),
        "{}",
        fehler.message
    );
    assert!(fehler.message.contains("firma"), "{}", fehler.message);
    assert!(attrappe.aufzeichnung.alle().is_empty());

    // Unbekannt → Fehler mit den bekannten Namen, ohne Anfrage.
    let fehler = aufruf(
        &server,
        "jira_projekte",
        json!({ "verbindung": "gibtsnicht" }),
    )
    .await
    .unwrap_err();
    assert!(fehler.message.contains("gibtsnicht"), "{}", fehler.message);
    assert!(fehler.message.contains("kunde"), "{}", fehler.message);
    assert!(attrappe.aufzeichnung.alle().is_empty());

    // Mit Namen klappt es; die Schreibweise ist egal.
    let text = antwort(&server, "jira_projekte", json!({ "verbindung": "FIRMA" })).await;
    assert!(text.contains(PROJEKT), "{text}");
}

// ------------------------------------------------------------- Sicherheit

#[tokio::test]
async fn ein_ungueltiger_token_wird_nicht_wiederholt() {
    let attrappe = Attrappe::starte().await;
    attrappe.token_ablehnen();
    let server = eine(&attrappe.adresse);

    let fehler = aufruf(&server, "jira_projekte", json!({}))
        .await
        .unwrap_err();
    assert!(
        fehler
            .message
            .contains("Token für firma ungültig oder abgelaufen"),
        "{}",
        fehler.message
    );
    assert!(
        fehler.message.contains("im quiver-Fenster erneuern"),
        "{}",
        fehler.message
    );
    assert!(
        fehler.message.contains("Tools → Jira"),
        "{}",
        fehler.message
    );
    ohne_token(&fehler.message);
    // Genau ein Versuch.
    assert_eq!(attrappe.aufzeichnung.anzahl("project/search"), 1);
}

#[tokio::test]
async fn fehlende_rechte_werden_als_solche_gemeldet() {
    let attrappe = Attrappe::starte().await;
    attrappe.keine_rechte();
    let server = eine(&attrappe.adresse);

    let fehler = aufruf(&server, "jira_vorgang", json!({ "key": VORGANG }))
        .await
        .unwrap_err();
    assert!(
        fehler.message.contains("Keine Rechte"),
        "{}",
        fehler.message
    );
    assert!(fehler.message.contains("permission"), "{}", fehler.message);
    ohne_token(&fehler.message);
    assert_eq!(attrappe.aufzeichnung.anzahl("issue/"), 1);
}

#[tokio::test]
async fn eine_weiterleitung_wird_nicht_verfolgt() {
    let attrappe = Attrappe::starte().await;
    attrappe.weiterleitung();
    let server = eine(&attrappe.adresse);

    let fehler = aufruf(&server, "jira_testen", json!({})).await.unwrap_err();
    assert!(fehler.message.contains("302"), "{}", fehler.message);
    // Genau ein Aufruf an die Attrappe; dem fremden Host wird nicht gefolgt.
    assert_eq!(attrappe.aufzeichnung.anzahl("myself"), 1);
}

#[tokio::test]
async fn eine_folgeseite_mit_fremdem_host_wird_ohne_anfrage_abgelehnt() {
    let attrappe = Attrappe::starte().await;
    attrappe.fremde_folgeseite();
    let server = eine(&attrappe.adresse);

    let fehler = aufruf(&server, "jira_projekte", json!({ "anzahl": 100 }))
        .await
        .unwrap_err();
    assert!(
        fehler.message.contains("fremden Host"),
        "{}",
        fehler.message
    );
    assert!(
        fehler.message.contains("fremd.example.test"),
        "{}",
        fehler.message
    );
    // Nur die erste Seite wurde geholt, die Folgeseite nie angefragt.
    assert_eq!(attrappe.aufzeichnung.anzahl("project/search"), 1);
}

#[tokio::test]
async fn nur_erlaubt_kein_schreiben_ausserhalb_der_projekte() {
    let attrappe = Attrappe::starte().await;
    let server = server(vec![eintrag(
        "firma",
        &attrappe.adresse,
        true,
        Some(PROJEKT),
    )]);

    // Ändern eines fremden Vorgangs: das Projekt kommt vom Server (XYZ).
    let text = antwort(
        &server,
        "jira_aendern",
        json!({ "key": VORGANG_FREMD, "titel": "Neu" }),
    )
    .await;
    assert!(text.contains("nicht freigegeben"), "{text}");
    assert!(text.contains(PROJEKT_FREMD), "{text}");
    assert!(text.contains(PROJEKT), "{text}");
    assert!(text.contains("nichts geändert"), "{text}");
    // Eine lesende Anfrage gab es (Projekt lesen), eine Änderung nicht.
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("issue/", "PUT"), 0);
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("issue/", "GET"), 1);

    // Anlegen in einem fremden Projekt: der Schlüssel kommt vom Server.
    let text = antwort(
        &server,
        "jira_anlegen",
        json!({ "projekt": PROJEKT_FREMD, "vorgangstyp": TYP, "titel": "Neu" }),
    )
    .await;
    assert!(text.contains("nicht freigegeben"), "{text}");
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("project/", "GET"), 1);
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("issue", "POST"), 0);

    // Verknüpfen über die Grenze: ein Vorgang ist erlaubt, einer nicht.
    let text = antwort(
        &server,
        "jira_verknuepfen",
        json!({ "vorgang": VORGANG, "ziel": VORGANG_FREMD, "art": "Blocks" }),
    )
    .await;
    assert!(text.contains("nicht freigegeben"), "{text}");
    assert_eq!(attrappe.aufzeichnung.anzahl("issueLink"), 0);

    // Im erlaubten Projekt klappt es.
    let text = antwort(
        &server,
        "jira_aendern",
        json!({ "key": VORGANG, "titel": "Neu" }),
    )
    .await;
    assert!(text.contains("geändert"), "{text}");
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("issue/", "PUT"), 1);
}

// ------------------------------------------------------------- Lesen

#[tokio::test]
async fn projekte_werden_gelesen_und_folgeseiten_geladen() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    // Drei Projekte bei Seiten von zwei: die Folgeseite kommt am selben Host.
    let text = antwort(&server, "jira_projekte", json!({ "anzahl": 3 })).await;
    assert!(text.contains("3 Projekt(e) in firma"), "{text}");
    assert!(text.contains("- ABC — Alpha"), "{text}");
    assert!(text.contains("- XYZ — Xsite"), "{text}");
    assert!(text.contains("- DEF — Delta"), "{text}");
    assert!(!text.contains("weitere Einträge"), "{text}");
    assert_eq!(attrappe.aufzeichnung.anzahl("project/search"), 2);
    assert_eq!(
        attrappe
            .aufzeichnung
            .zu("project/search")
            .last()
            .unwrap()
            .feld("startAt"),
        Some("2")
    );

    // Reicht eine Seite, wird nicht weitergeblättert – und es gibt mehr.
    let text = antwort(&server, "jira_projekte", json!({ "anzahl": 2 })).await;
    assert!(text.contains("2 von 3 Projekt(e) in firma"), "{text}");
    assert!(text.contains("weitere Einträge"), "{text}");
    assert_eq!(attrappe.aufzeichnung.anzahl("project/search"), 3);
}

#[tokio::test]
async fn die_suche_baut_die_jql_aus_den_komfort_parametern() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(
        &server,
        "jira_suchen",
        json!({
            "projekt": PROJEKT,
            "status": "In Progress",
            "zugewiesen_an_mich": true,
            "text": "Kunde's Login",
            "anzahl": 10
        }),
    )
    .await;
    assert!(
        text.contains("- ABC-1 [Bug] [In Progress] Login kaputt"),
        "{text}"
    );
    assert!(text.contains("seite-2"), "{text}");

    let aufruf = attrappe.aufzeichnung.erster("search/jql").unwrap();
    assert_eq!(aufruf.methode, "POST");
    assert_eq!(aufruf.json()["maxResults"], json!(10));
    let abfrage = aufruf.json()["jql"].as_str().unwrap().to_string();
    assert!(abfrage.contains("project = 'ABC'"), "{abfrage}");
    assert!(abfrage.contains("status = 'In Progress'"), "{abfrage}");
    assert!(abfrage.contains("assignee = currentUser()"), "{abfrage}");
    // Der Apostroph ist maskiert, die Abfrage bleibt eine Zeichenkette.
    assert!(abfrage.contains("text ~ 'Kunde\\'s Login'"), "{abfrage}");
    assert!(abfrage.ends_with("ORDER BY updated DESC"), "{abfrage}");
}

#[tokio::test]
async fn eine_eigene_jql_und_die_naechste_seite_werden_durchgereicht() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(
        &server,
        "jira_suchen",
        json!({ "jql": "project = ABC", "seite": attrappe::SEITE_2 }),
    )
    .await;
    assert!(text.contains(VORGANG_FREMD), "{text}");
    assert!(text.contains("letzte Seite"), "{text}");

    let aufruf = attrappe.aufzeichnung.erster("search/jql").unwrap();
    assert_eq!(aufruf.json()["jql"], json!("project = ABC"));
    assert_eq!(aufruf.json()["nextPageToken"], json!(attrappe::SEITE_2));
}

#[tokio::test]
async fn ohne_projekt_grenzt_nur_die_suche_ein() {
    let attrappe = Attrappe::starte().await;
    let server = server(vec![eintrag(
        "firma",
        &attrappe.adresse,
        true,
        Some(PROJEKT),
    )]);

    antwort(&server, "jira_suchen", json!({})).await;
    let abfrage = attrappe.aufzeichnung.erster("search/jql").unwrap().json()["jql"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(abfrage.contains("project IN ('ABC')"), "{abfrage}");
}

#[tokio::test]
async fn ein_vorgang_zeigt_beschreibung_links_und_unteraufgaben() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(&server, "jira_vorgang", json!({ "key": VORGANG })).await;
    assert!(
        text.starts_with("ABC-1 [Bug] [In Progress] Login kaputt"),
        "{text}"
    );
    assert!(text.contains("## Beschreibung"), "{text}");
    assert!(text.contains("Der Anmelden-Knopf tut nichts."), "{text}");
    assert!(text.contains("- Schritt eins"), "{text}");
    assert!(text.contains("Fehler 500"), "{text}");
    assert!(
        !text.contains("\"type\""),
        "ADF ist durchgeschlagen: {text}"
    );
    assert!(text.contains("- blocks ABC-2"), "{text}");
    assert!(text.contains("- relates to ABC-3"), "{text}");
    assert!(
        text.contains("## Unteraufgaben\n- ABC-4 [To Do] Unteraufgabe"),
        "{text}"
    );
    assert!(
        text.contains("## Eltern\n- ABC-0 [Offen] Elternvorgang"),
        "{text}"
    );
    ohne_token(&text);
}

#[tokio::test]
async fn kommentare_und_uebergaenge_werden_gelesen() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(&server, "jira_kommentare", json!({ "key": VORGANG })).await;
    assert!(text.contains("2 Kommentar(e) zu ABC-1"), "{text}");
    assert!(text.contains("**Anna**"), "{text}");
    assert!(text.contains("Sieht gut aus"), "{text}");
    assert!(text.contains("Kommentar 100"), "{text}");
    assert_eq!(
        attrappe
            .aufzeichnung
            .erster("comment")
            .unwrap()
            .feld("orderBy"),
        Some("created")
    );

    let text = antwort(&server, "jira_uebergaenge", json!({ "key": VORGANG })).await;
    assert!(text.contains("3 Übergang/Übergänge für ABC-1"), "{text}");
    assert!(text.contains("- 11 Start Progress → In Progress"), "{text}");
    assert!(text.contains("- 21 Fertig → Done"), "{text}");
    assert!(text.contains("- 31 Fertig → Closed"), "{text}");
}

#[tokio::test]
async fn felder_zum_anlegen_zeigt_typen_und_pflichtfelder() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(
        &server,
        "jira_felder_zum_anlegen",
        json!({ "projekt": PROJEKT }),
    )
    .await;
    assert!(text.contains("2 Vorgangstyp(en) in ABC"), "{text}");
    assert!(
        text.contains(&format!("- {TYP} · Typ-ID {}", attrappe::TYP_ID)),
        "{text}"
    );

    // Mit Namen: erst die Typ-ID auflösen, dann die Felder holen.
    let text = antwort(
        &server,
        "jira_felder_zum_anlegen",
        json!({ "projekt": PROJEKT, "vorgangstyp": TYP }),
    )
    .await;
    assert!(
        text.contains("2 Feld(er) für Bug in ABC (1 Pflicht)"),
        "{text}"
    );
    assert!(
        text.contains("- summary (Pflicht): Summary · Typ string"),
        "{text}"
    );
    assert!(
        text.contains("- priority: Priority · Typ priority · erlaubt: High, Low"),
        "{text}"
    );
    assert_eq!(attrappe.aufzeichnung.anzahl("createmeta"), 3);

    // Unbekannter Typ: Hinweis ohne weitere Anfrage.
    let fehler = aufruf(
        &server,
        "jira_felder_zum_anlegen",
        json!({ "projekt": PROJEKT, "vorgangstyp": "Gibtsnicht" }),
    )
    .await
    .unwrap_err();
    assert!(fehler.message.contains("Gibtsnicht"), "{}", fehler.message);
}

#[tokio::test]
async fn filter_verknuepfungsarten_und_personen_werden_gelesen() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(&server, "jira_filter_liste", json!({})).await;
    assert!(text.contains("1 Filter"), "{text}");
    assert!(text.contains(&format!("- {FILTER} Meine Bugs")), "{text}");
    assert_eq!(
        attrappe
            .aufzeichnung
            .erster("filter/search")
            .unwrap()
            .feld("maxResults"),
        Some("25")
    );

    let text = antwort(&server, "jira_filter", json!({ "id": FILTER })).await;
    assert!(text.contains("10000 Meine Bugs"), "{text}");
    assert!(text.contains("JQL: project = ABC"), "{text}");
    assert!(text.contains("Besitzer: Alex Beispiel"), "{text}");

    let text = antwort(&server, "jira_verknuepfungsarten", json!({})).await;
    assert!(
        text.contains("- Blocks (ID 10000): blocks / is blocked by"),
        "{text}"
    );
    assert!(
        text.contains("- Relates (ID 10001): relates to / relates to"),
        "{text}"
    );

    let text = antwort(&server, "jira_person_suchen", json!({ "suche": "Alex" })).await;
    assert!(text.contains("2 Person(en)"), "{text}");
    assert!(
        text.contains(&format!("accountId {}", attrappe::BENUTZER_ID)),
        "{text}"
    );
    assert!(text.contains("inaktiv"), "{text}");
    assert_eq!(
        attrappe
            .aufzeichnung
            .erster("user/search")
            .unwrap()
            .feld("query"),
        Some("Alex")
    );
}

// -------------------------------------------------------------- Schreiben

#[tokio::test]
async fn schreiben_aus_macht_bei_allen_fuenf_werkzeugen_keine_anfrage() {
    let attrappe = Attrappe::starte().await;
    let server = eine_ohne_schreiben(&attrappe.adresse);

    for (name, argumente) in [
        (
            "jira_anlegen",
            json!({ "projekt": PROJEKT, "vorgangstyp": TYP, "titel": "Neu" }),
        ),
        ("jira_aendern", json!({ "key": VORGANG, "titel": "Neu" })),
        (
            "jira_uebergang",
            json!({ "key": VORGANG, "uebergang": "11" }),
        ),
        (
            "jira_kommentieren",
            json!({ "key": VORGANG, "text": "hallo" }),
        ),
        (
            "jira_verknuepfen",
            json!({ "vorgang": VORGANG, "ziel": "ABC-2", "art": "Blocks" }),
        ),
    ] {
        let text = antwort(&server, name, argumente).await;
        assert!(
            text.contains("Schreiben ist für firma aus"),
            "{name}: {text}"
        );
        assert!(
            text.contains("im quiver-Fenster einschalten"),
            "{name}: {text}"
        );
    }
    assert!(
        attrappe.aufzeichnung.alle().is_empty(),
        "trotz „schreiben aus“ gab es Anfragen: {:?}",
        attrappe::aufruf_pfade(&attrappe.aufzeichnung)
    );
}

#[tokio::test]
async fn ein_vorgang_wird_angelegt() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(
        &server,
        "jira_anlegen",
        json!({
            "projekt": PROJEKT,
            "vorgangstyp": TYP,
            "titel": "Neuer Fehler",
            "beschreibung": "Erster Absatz\n- Punkt",
            "eltern": ELTERN,
            "zugewiesen": attrappe::BENUTZER_ID
        }),
    )
    .await;
    assert!(text.contains("Vorgang angelegt: ABC-9"), "{text}");
    assert!(
        text.contains(&format!("{}/browse/ABC-9", attrappe.adresse)),
        "{text}"
    );
    ohne_token(&text);

    let aufruf = attrappe.aufzeichnung.erster("issue").unwrap();
    assert_eq!(aufruf.methode, "POST");
    assert_eq!(aufruf.content_type.as_deref(), Some("application/json"));
    let felder = &aufruf.json()["fields"];
    assert_eq!(felder["project"]["key"], json!(PROJEKT));
    assert_eq!(felder["issuetype"]["name"], json!(TYP));
    assert_eq!(felder["summary"], json!("Neuer Fehler"));
    assert_eq!(felder["parent"]["key"], json!(ELTERN));
    assert_eq!(
        felder["assignee"]["accountId"],
        json!(attrappe::BENUTZER_ID)
    );
    // Die Beschreibung ist ADF, kein Text.
    assert_eq!(felder["description"]["type"], json!("doc"));
    assert_eq!(
        felder["description"]["content"][0]["type"],
        json!("paragraph")
    );
    assert_eq!(
        felder["description"]["content"][1]["type"],
        json!("bulletList")
    );
}

#[tokio::test]
async fn ein_vorgang_wird_geaendert_und_204_ist_erfolg() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(
        &server,
        "jira_aendern",
        json!({ "key": VORGANG, "titel": "Neuer Titel", "beschreibung": "Neu" }),
    )
    .await;
    assert!(text.contains("Vorgang ABC-1 geändert"), "{text}");
    assert!(text.contains("- summary"), "{text}");
    assert!(text.contains("- description"), "{text}");

    let aufruf = attrappe.aufzeichnung.zu_mit("issue/", "PUT").pop().unwrap();
    assert_eq!(aufruf.content_type.as_deref(), Some("application/json"));
    let felder = &aufruf.json()["fields"];
    assert_eq!(felder["summary"], json!("Neuer Titel"));
    assert_eq!(felder["description"]["type"], json!("doc"));
}

#[tokio::test]
async fn ein_uebergang_wird_ausgefuehrt_und_204_ist_erfolg() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(
        &server,
        "jira_uebergang",
        json!({ "key": VORGANG, "uebergang": "11", "kommentar": "Los geht's" }),
    )
    .await;
    assert!(
        text.contains("Vorgang ABC-1: Übergang 11 „Start Progress“ ausgeführt"),
        "{text}"
    );
    // Den neuen Status liest das Tool am Vorgang – es rät ihn nicht.
    assert!(text.contains("Neuer Status: In Progress"), "{text}");

    let aufruf = attrappe
        .aufzeichnung
        .zu_mit("transitions", "POST")
        .pop()
        .unwrap();
    assert_eq!(aufruf.json()["transition"]["id"], json!("11"));
    assert_eq!(
        aufruf.json()["update"]["comment"][0]["add"]["body"]["type"],
        json!("doc")
    );
    // Erst die Übergänge lesen, dann den Status (`fields=status`).
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("transitions", "GET"), 1);
    let status_abfrage = attrappe.aufzeichnung.alle().into_iter().find(|aufruf| {
        aufruf.methode == "GET"
            && aufruf.pfad.ends_with("/issue/ABC-1")
            && aufruf.feld("fields") == Some("status")
    });
    assert!(
        status_abfrage.is_some(),
        "keine Statusabfrage: {:?}",
        attrappe::aufruf_pfade(&attrappe.aufzeichnung)
    );
}

#[tokio::test]
async fn ein_kommentar_wird_geschrieben() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(
        &server,
        "jira_kommentieren",
        json!({ "key": VORGANG, "text": "Sieht gut aus\n- getestet" }),
    )
    .await;
    assert!(
        text.contains(&format!("Kommentar {KOMMENTAR} zu ABC-1 angelegt")),
        "{text}"
    );

    let aufruf = attrappe
        .aufzeichnung
        .zu_mit("comment", "POST")
        .pop()
        .unwrap();
    let rumpf = aufruf.json();
    assert_eq!(rumpf["body"]["type"], json!("doc"));
    assert_eq!(rumpf["body"]["content"][1]["type"], json!("bulletList"));
}

#[tokio::test]
async fn zwei_vorgaenge_werden_verknuepft() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(
        &server,
        "jira_verknuepfen",
        json!({ "vorgang": VORGANG, "ziel": "ABC-2", "art": "Blocks", "kommentar": "hängt zusammen" }),
    )
    .await;
    assert!(
        text.contains("Vorgang ABC-1 mit ABC-2 als „Blocks“ verknüpft"),
        "{text}"
    );

    let aufruf = attrappe
        .aufzeichnung
        .zu_mit("issueLink", "POST")
        .pop()
        .unwrap();
    let rumpf = aufruf.json();
    assert_eq!(rumpf["type"]["name"], json!("Blocks"));
    assert_eq!(rumpf["outwardIssue"]["key"], json!(VORGANG));
    assert_eq!(rumpf["inwardIssue"]["key"], json!("ABC-2"));
    assert_eq!(rumpf["comment"]["body"]["type"], json!("doc"));
}

// ------------------------------------------------------- Wiederholung (429)

#[tokio::test]
async fn ein_lesender_aufruf_wiederholt_bei_429_hoechstens_zweimal() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    attrappe.zu_viele_anfragen(2);
    let text = antwort(&server, "jira_projekte", json!({})).await;
    assert!(text.contains(PROJEKT), "{text}");
    // Zwei abgelehnte Versuche, dann die erste Seite – und die Folgeseite.
    let nach_erfolg = attrappe.aufzeichnung.anzahl_mit("project/search", "GET");
    assert_eq!(nach_erfolg, 4);

    // Mehr als zwei Wiederholungen gibt es nicht: der dritte 429 bleibt stehen.
    attrappe.zu_viele_anfragen(3);
    let fehler = aufruf(&server, "jira_projekte", json!({}))
        .await
        .unwrap_err();
    assert!(fehler.message.contains("429"), "{}", fehler.message);
    assert!(
        fehler.message.contains("Wiederholungen"),
        "{}",
        fehler.message
    );
    assert_eq!(
        attrappe.aufzeichnung.anzahl_mit("project/search", "GET") - nach_erfolg,
        3
    );
}

#[tokio::test]
async fn ein_schreibender_aufruf_wiederholt_bei_429_nicht() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    attrappe.zu_viele_anfragen(1);
    let fehler = aufruf(
        &server,
        "jira_kommentieren",
        json!({ "key": VORGANG, "text": "Hallo" }),
    )
    .await
    .unwrap_err();
    assert!(fehler.message.contains("429"), "{}", fehler.message);
    // Genau ein Versuch – ob der Aufruf ankam, weiß nur der Server.
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("comment", "POST"), 1);
}

// ------------------------------------------------------- Felder und Richtung

#[tokio::test]
async fn freie_felder_sind_json_und_doppelte_werden_abgelehnt() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    // Listen und Objekte gehen als JSON in den Rumpf – nicht als Text.
    let text = antwort(
        &server,
        "jira_aendern",
        json!({ "key": VORGANG,
                "felder": { "labels": ["login", "dringend"],
                            "priority": { "name": "High" },
                            "customfield_10010": 3 } }),
    )
    .await;
    assert!(text.contains("geändert"), "{text}");
    let felder = attrappe
        .aufzeichnung
        .zu_mit("issue/", "PUT")
        .pop()
        .unwrap()
        .json()["fields"]
        .clone();
    assert_eq!(felder["labels"], json!(["login", "dringend"]));
    assert_eq!(felder["priority"], json!({ "name": "High" }));
    assert_eq!(felder["customfield_10010"], json!(3));

    // `project` lässt sich nicht ändern – ohne Anfrage.
    let fehler = aufruf(
        &server,
        "jira_aendern",
        json!({ "key": VORGANG, "felder": { "project": { "key": PROJEKT } } }),
    )
    .await
    .unwrap_err();
    assert!(fehler.message.contains("project"), "{}", fehler.message);
    assert!(
        fehler.message.contains("nicht ändern"),
        "{}",
        fehler.message
    );

    // Dasselbe Feld zweimal (hier `summary` und `titel`) ist ein Fehler.
    let fehler = aufruf(
        &server,
        "jira_aendern",
        json!({ "key": VORGANG, "titel": "Neu", "felder": { "summary": "Auch neu" } }),
    )
    .await
    .unwrap_err();
    assert!(fehler.message.contains("doppelt"), "{}", fehler.message);

    // Ein kaputtes ADF in `description` wird vor der Anfrage abgelehnt.
    let fehler = aufruf(
        &server,
        "jira_aendern",
        json!({ "key": VORGANG, "felder": { "description": {
            "type": "doc", "version": 1, "content": [
                { "type": "paragraph", "content": [ { "type": "text", "text": "" } ] }
            ] } } }),
    )
    .await
    .unwrap_err();
    assert!(fehler.message.contains("ADF"), "{}", fehler.message);

    // Genau eine Änderung ist bei der Attrappe angekommen (die erste).
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("issue/", "PUT"), 1);
}

#[tokio::test]
async fn ein_elternvorgang_ausserhalb_von_nur_wird_abgelehnt() {
    let attrappe = Attrappe::starte().await;
    let server = server(vec![eintrag(
        "firma",
        &attrappe.adresse,
        true,
        Some(PROJEKT),
    )]);

    // `felder.parent` außerhalb von `nur`: Hinweis, keine Änderung.
    let text = antwort(
        &server,
        "jira_aendern",
        json!({ "key": VORGANG, "felder": { "parent": { "key": VORGANG_FREMD } } }),
    )
    .await;
    assert!(text.contains("nicht freigegeben"), "{text}");
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("issue/", "PUT"), 0);

    // Eine Id, ein Schlüssel **mit** Id oder eine andere Schreibweise kommen
    // nicht an der Prüfung vorbei.
    for felder in [
        json!({ "parent": { "id": "20001" } }),
        json!({ "parent": { "key": ELTERN, "id": "20001" } }),
        json!({ "Parent": { "key": VORGANG_FREMD } }),
        json!({ "parent": 7 }),
        json!({ "Project": { "key": "ANDERS" } }),
    ] {
        let fehler = aufruf(
            &server,
            "jira_aendern",
            json!({ "key": VORGANG, "felder": felder.clone() }),
        )
        .await;
        assert!(fehler.is_err(), "{felder}");
    }
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("issue/", "PUT"), 0);

    // `eltern` beim Anlegen ebenso – auch wenn das Projekt selbst erlaubt ist.
    let text = antwort(
        &server,
        "jira_anlegen",
        json!({ "projekt": PROJEKT, "vorgangstyp": TYP, "titel": "Neu",
                "eltern": VORGANG_FREMD }),
    )
    .await;
    assert!(text.contains("nicht freigegeben"), "{text}");
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("issue", "POST"), 0);

    // Im erlaubten Projekt geht es.
    let text = antwort(
        &server,
        "jira_aendern",
        json!({ "key": VORGANG, "felder": { "parent": ELTERN } }),
    )
    .await;
    assert!(text.contains("geändert"), "{text}");
    assert_eq!(
        attrappe
            .aufzeichnung
            .zu_mit("issue/", "PUT")
            .pop()
            .unwrap()
            .json()["fields"]["parent"],
        json!({ "key": ELTERN })
    );
}

#[tokio::test]
async fn beim_anlegen_wird_der_projektschluessel_vom_server_gesendet() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    // Der Agent schreibt klein – gesendet wird, was Jira nennt.
    let text = antwort(
        &server,
        "jira_anlegen",
        json!({ "projekt": "abc", "vorgangstyp": TYP, "titel": "Neu" }),
    )
    .await;
    assert!(text.contains("Vorgang angelegt: ABC-9"), "{text}");
    let felder = attrappe
        .aufzeichnung
        .zu_mit("issue", "POST")
        .pop()
        .unwrap()
        .json()["fields"]
        .clone();
    assert_eq!(felder["project"], json!({ "key": PROJEKT }), "{felder}");

    // Ein Feld mit eigenem Parameter doppelt angegeben: abgelehnt.
    let fehler = aufruf(
        &server,
        "jira_anlegen",
        json!({ "projekt": PROJEKT, "vorgangstyp": TYP, "titel": "Neu",
                "felder": { "summary": "noch ein Titel" } }),
    )
    .await
    .unwrap_err();
    assert!(fehler.message.contains("doppelt"), "{}", fehler.message);
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("issue", "POST"), 1);
}

#[tokio::test]
async fn ein_uebergang_muss_von_jira_angeboten_sein() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    // Unbekannt: Kandidaten nennen, nichts senden.
    let text = antwort(
        &server,
        "jira_uebergang",
        json!({ "key": VORGANG, "uebergang": "Gibt es nicht" }),
    )
    .await;
    assert!(text.contains("nicht"), "{text}");
    assert!(text.contains("11 Start Progress → In Progress"), "{text}");
    assert!(text.contains("nichts geändert"), "{text}");
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("transitions", "POST"), 0);

    // Mehrdeutig: „Fertig“ gibt es zweimal → Kandidaten statt Anfrage.
    let text = antwort(
        &server,
        "jira_uebergang",
        json!({ "key": VORGANG, "uebergang": "fertig" }),
    )
    .await;
    assert!(
        text.contains("mehrdeutig") || text.contains("auf 2 Übergänge"),
        "{text}"
    );
    assert!(text.contains("21 Fertig → Done"), "{text}");
    assert!(text.contains("31 Fertig → Closed"), "{text}");
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("transitions", "POST"), 0);

    // Mit Namen eindeutig: der Übergang läuft und der neue Status wird gelesen.
    let text = antwort(
        &server,
        "jira_uebergang",
        json!({ "key": VORGANG, "uebergang": "Start Progress" }),
    )
    .await;
    assert!(text.contains("Neuer Status: In Progress"), "{text}");
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("transitions", "POST"), 1);
}

#[tokio::test]
async fn eine_verknuepfung_wird_in_richtung_der_art_gemeldet() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    // „ABC-2 blocks ABC-1“: vorgang ist die outward-Seite.
    let text = antwort(
        &server,
        "jira_verknuepfen",
        json!({ "vorgang": "ABC-2", "ziel": VORGANG, "art": "Blocks" }),
    )
    .await;
    assert!(text.contains("Jira zeigt: ABC-2 blocks ABC-1"), "{text}");
    assert!(
        text.contains("bei ABC-1: ABC-1 is blocked by ABC-2"),
        "{text}"
    );
    let rumpf = attrappe
        .aufzeichnung
        .zu_mit("issueLink", "POST")
        .pop()
        .unwrap()
        .json();
    assert_eq!(rumpf["outwardIssue"]["key"], json!("ABC-2"));
    assert_eq!(rumpf["inwardIssue"]["key"], json!(VORGANG));
}

#[tokio::test]
async fn eine_verdrehte_verknuepfung_wird_deutlich_gemeldet() {
    let attrappe = Attrappe::starte().await;
    attrappe.verdrehte_verknuepfung();
    let server = eine(&attrappe.adresse);

    let text = antwort(
        &server,
        "jira_verknuepfen",
        json!({ "vorgang": "ABC-2", "ziel": VORGANG, "art": "Blocks" }),
    )
    .await;
    assert!(text.contains("ACHTUNG"), "{text}");
    assert!(text.contains("andersherum"), "{text}");
    assert!(text.contains("ABC-1 blocks ABC-2"), "{text}");
}

#[tokio::test]
async fn eine_jql_ohne_einschraenkung_wird_verstaendlich_abgelehnt() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    // `search/jql` verlangt eine begrenzte Abfrage – gesucht wird gar nicht.
    let text = antwort(&server, "jira_suchen", json!({})).await;
    assert!(text.contains("nur eine begrenzte JQL"), "{text}");
    assert!(text.contains("Es wurde nicht gesucht"), "{text}");
    assert_eq!(attrappe.aufzeichnung.anzahl("search/jql"), 0);

    let text = antwort(
        &server,
        "jira_suchen",
        json!({ "jql": "ORDER BY key DESC" }),
    )
    .await;
    assert!(text.contains("nur eine begrenzte JQL"), "{text}");
    assert_eq!(attrappe.aufzeichnung.anzahl("search/jql"), 0);

    // Mit Einschränkung geht es – reines JQL, kein `SELECT … WHERE`.
    antwort(&server, "jira_suchen", json!({ "jql": "project = ABC" })).await;
    let jql = attrappe.aufzeichnung.erster("search/jql").unwrap().json()["jql"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(jql, "project = ABC");
}

#[tokio::test]
async fn pfadwerte_mit_punkten_werden_ohne_anfrage_abgelehnt() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    for wert in ["..", ".", "a/../../b", ""] {
        let fehler = aufruf(&server, "jira_vorgang", json!({ "key": wert }))
            .await
            .unwrap_err();
        assert!(fehler.message.contains("key"), "{wert}: {}", fehler.message);
    }
    let fehler = aufruf(&server, "jira_vorgang", json!({ "key": ".." }))
        .await
        .unwrap_err();
    assert!(fehler.message.contains("Pfad"), "{}", fehler.message);
    let fehler = aufruf(&server, "jira_vorgang", json!({ "key": "../admin" }))
        .await
        .unwrap_err();
    assert!(fehler.message.contains("Pfad"), "{}", fehler.message);
    // Auch ein leerer Wert kommt nicht bis Jira.
    let fehler = aufruf(&server, "jira_vorgang", json!({ "key": "   " }))
        .await
        .unwrap_err();
    assert!(fehler.message.contains("key"), "{}", fehler.message);
    assert!(
        attrappe.aufzeichnung.alle().is_empty(),
        "{:?}",
        attrappe::aufruf_pfade(&attrappe.aufzeichnung)
    );
}

// ------------------------------------------------------------- Querschnitt

#[tokio::test]
async fn keine_antwort_enthaelt_token_oder_passwort() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let antworten = vec![
        antwort(&server, "jira_testen", json!({})).await,
        antwort(&server, "jira_projekte", json!({})).await,
        antwort(&server, "jira_suchen", json!({})).await,
        antwort(&server, "jira_vorgang", json!({ "key": VORGANG })).await,
        antwort(&server, "jira_kommentare", json!({ "key": VORGANG })).await,
        antwort(&server, "jira_uebergaenge", json!({ "key": VORGANG })).await,
        antwort(&server, "jira_person_suchen", json!({ "suche": "Alex" })).await,
    ];
    for text in &antworten {
        ohne_token(text);
    }

    for aufruf in attrappe.aufzeichnung.alle() {
        // E-Mail und Token stecken nur im Authorization-Kopf.
        assert!(!aufruf.pfad.contains(TOKEN_A), "{}", aufruf.pfad);
        assert!(!aufruf.rumpf.contains(TOKEN_A), "{}", aufruf.rumpf);
    }
}
