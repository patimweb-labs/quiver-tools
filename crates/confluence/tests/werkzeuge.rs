// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Werkzeuge gegen die Confluence-Attrappe (SPEC §7, Aufgabe 14).
//!
//! Jeder Fall prüft eine Wirkung: was bei Confluence gefragt wurde (von der
//! Attrappe aufgezeichnet) und was beim Agenten ankommt. Ein echtes Confluence
//! ist nie im Spiel.

mod attrappe;

use attrappe::{
    Attrappe, BEREICH, BEREICH_2, BEREICH_ID, EMAIL, FREMDE_SEITE, KOMMENTAR, KONTO, NEUE_SEITE,
    SEITE, SEITE_INHALT, SEITE_TITEL, SEITE_VERSION, TOKEN_A, TOKEN_B, UNTERSEITE,
};
use quiver_confluence::sites::{SammelFehler, Sammlung};
use quiver_confluence::{Confluence, ConfluenceServer};
use quiver_tools_kern::zugaenge::Zugang as Eintrag;
use quiver_tools_kern::Zugang;
use rmcp::model::CallToolResult;
use rmcp::ErrorData as McpError;
use serde_json::{json, Value};

/// Eine Site für die Attrappe.
fn eintrag(name: &str, adresse: &str, nur: Option<&str>, token: &str, schreiben: bool) -> Eintrag {
    let mut werte: std::collections::BTreeMap<String, String> = [
        ("url", adresse),
        ("email", EMAIL),
        ("api_token", token),
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

fn server(eintraege: Vec<Eintrag>) -> ConfluenceServer {
    ConfluenceServer::mit(
        Zugang::mit_umgebung(&Confluence, |_| None),
        Ok(Sammlung::aus_zugaengen(eintraege).unwrap()),
    )
}

/// Eine Site ohne `nur`; Schreiben ist an.
fn eine(adresse: &str) -> ConfluenceServer {
    server(vec![eintrag("firma", adresse, None, TOKEN_A, true)])
}

/// Eine Site ohne `nur`; Schreiben ist **aus**.
fn eine_ohne_schreiben(adresse: &str) -> ConfluenceServer {
    server(vec![eintrag("firma", adresse, None, TOKEN_A, false)])
}

/// Eine Site, die nur in `nur` schreiben darf; Schreiben ist an.
fn eine_nur(adresse: &str, nur: &str) -> ConfluenceServer {
    server(vec![eintrag("firma", adresse, Some(nur), TOKEN_A, true)])
}

/// Zwei Sites: die erste darf schreiben, die zweite nicht.
fn zwei(adresse: &str) -> ConfluenceServer {
    server(vec![
        eintrag("firma", adresse, None, TOKEN_A, true),
        eintrag("kunde", adresse, None, TOKEN_B, false),
    ])
}

async fn aufruf(
    server: &ConfluenceServer,
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

async fn antwort(server: &ConfluenceServer, name: &str, argumente: Value) -> String {
    text(
        aufruf(server, name, argumente)
            .await
            .expect("unerwarteter Fehler"),
    )
}

/// Keine Antwort und kein Fehler enthält Token oder E-Mail (Sicherheit 3).
fn ohne_geheimnisse(text: &str) {
    assert!(!text.contains(TOKEN_A), "Token in der Antwort: {text}");
    assert!(!text.contains(TOKEN_B), "Token in der Antwort: {text}");
    assert!(!text.contains(EMAIL), "E-Mail in der Antwort: {text}");
}

// ------------------------------------------------------------------ Zugang

#[tokio::test]
async fn status_zeigt_die_sites_ohne_token_und_ohne_email() {
    let attrappe = Attrappe::starte().await;
    let server = zwei(&attrappe.adresse);

    let text = antwort(&server, "confluence_status", json!({})).await;
    assert!(text.contains("2 Site(s)"), "{text}");
    assert!(text.contains("firma"), "{text}");
    assert!(text.contains("kunde"), "{text}");
    assert!(text.contains("Schreiben an"), "{text}");
    assert!(text.contains("Schreiben aus"), "{text}");
    ohne_geheimnisse(&text);
    // `confluence_status` öffnet keine Verbindung.
    assert!(attrappe.aufzeichnung.alle().is_empty());
}

#[tokio::test]
async fn testen_meldet_sich_an_und_nennt_den_benutzer() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(&server, "confluence_testen", json!({})).await;
    assert!(text.contains("Anmeldung an firma erfolgreich"), "{text}");
    assert!(text.contains(attrappe::BENUTZER), "{text}");
    assert!(text.contains(KONTO), "{text}");
    ohne_geheimnisse(&text);

    // Genau eine Anfrage, mit Basic-Anmeldung.
    assert_eq!(attrappe.aufzeichnung.anzahl("user/current"), 1);
    let aufruf = attrappe.aufzeichnung.letzter().unwrap();
    assert!(aufruf.anmeldung.unwrap().starts_with("Basic "));
}

#[tokio::test]
async fn ohne_zugang_kommt_der_hinweis_und_kein_fehler() {
    let server = server(vec![]);
    let text = antwort(&server, "confluence_bereiche", json!({})).await;
    assert!(text.contains("Zugang fehlt"), "{text}");
    assert!(text.contains("Tools → Confluence"), "{text}");
}

#[tokio::test]
async fn eine_site_darf_ohne_namen_gewaehlt_werden() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);
    let text = antwort(&server, "confluence_labels", json!({ "id": SEITE })).await;
    assert!(text.contains("global:anmeldung"), "{text}");
}

#[tokio::test]
async fn zwei_sites_verlangen_einen_namen() {
    let attrappe = Attrappe::starte().await;
    let server = zwei(&attrappe.adresse);

    let fehler = aufruf(&server, "confluence_bereiche", json!({}))
        .await
        .unwrap_err();
    assert!(
        fehler.message.contains("mehrere Sites"),
        "{}",
        fehler.message
    );
    assert!(fehler.message.contains("firma"), "{}", fehler.message);
    assert!(attrappe.aufzeichnung.alle().is_empty());

    // Unbekannt → Hinweis mit den bekannten Namen, ohne Anfrage.
    let fehler = aufruf(
        &server,
        "confluence_bereiche",
        json!({ "verbindung": "gibtsnicht" }),
    )
    .await
    .unwrap_err();
    assert!(fehler.message.contains("gibtsnicht"), "{}", fehler.message);
    assert!(fehler.message.contains("kunde"), "{}", fehler.message);
    assert!(attrappe.aufzeichnung.alle().is_empty());

    // Mit Namen klappt es.
    let text = antwort(
        &server,
        "confluence_bereiche",
        json!({ "verbindung": "kunde", "anzahl": 2 }),
    )
    .await;
    assert!(text.contains("2 Bereich(e)"), "{text}");
}

#[tokio::test]
async fn eine_adresse_ohne_https_ist_ein_fehler() {
    let kaputt = Sammlung::aus_zugaengen(vec![eintrag(
        "firma",
        "http://firma.atlassian.net",
        None,
        TOKEN_A,
        true,
    )])
    .unwrap_err();
    assert!(matches!(kaputt, SammelFehler::Site(_)), "{kaputt:?}");

    let server = ConfluenceServer::mit(Zugang::mit_umgebung(&Confluence, |_| None), Err(kaputt));
    let fehler = aufruf(&server, "confluence_status", json!({}))
        .await
        .unwrap_err();
    assert!(fehler.message.contains("https://"), "{}", fehler.message);
}

// --------------------------------------------------------------- Bereiche

#[tokio::test]
async fn bereiche_werden_gelesen_und_geblaettert() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    // Fünf Bereiche, zwei je Seite: der Client lädt drei Folgeseiten.
    let text = antwort(&server, "confluence_bereiche", json!({})).await;
    assert!(text.contains("5 Bereich(e)"), "{text}");
    assert!(text.contains("- ENG · Engineering · id 10"), "{text}");
    assert!(text.contains("- QA · Qualität · id 14"), "{text}");
    assert!(!text.contains("es gibt weitere"), "{text}");
    ohne_geheimnisse(&text);
    assert_eq!(attrappe.aufzeichnung.anzahl("api/v2/spaces"), 3);
    assert_eq!(
        attrappe
            .aufzeichnung
            .erster("api/v2/spaces")
            .unwrap()
            .feld("limit"),
        Some("25")
    );

    // `anzahl` begrenzt und nennt den Rest.
    let text = antwort(&server, "confluence_bereiche", json!({ "anzahl": 2 })).await;
    assert!(text.contains("2 Bereich(e)"), "{text}");
    assert!(text.contains("es gibt weitere Bereiche"), "{text}");

    // `schluessel` filtert (der Server entscheidet, was passt).
    let text = antwort(
        &server,
        "confluence_bereiche",
        json!({ "schluessel": BEREICH_2 }),
    )
    .await;
    assert!(text.contains("1 Bereich(e)"), "{text}");
    assert!(text.contains(BEREICH_2), "{text}");
    assert_eq!(
        attrappe
            .aufzeichnung
            .zu("api/v2/spaces")
            .last()
            .unwrap()
            .feld("keys"),
        Some(BEREICH_2)
    );
}

// ------------------------------------------------------------------ Seite

#[tokio::test]
async fn eine_seite_zeigt_version_bereich_und_inhalt_als_text() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(&server, "confluence_seite", json!({ "id": SEITE })).await;
    assert!(text.starts_with("123 Anmeldung [current]"), "{text}");
    assert!(text.contains("- Bereich: ENG (10)"), "{text}");
    assert!(text.contains("- Version: 3"), "{text}");
    assert!(
        text.contains("- URL: ") && text.contains("/spaces/ENG/pages/123/Anmeldung"),
        "{text}"
    );
    assert!(
        text.contains("## Inhalt\nDer Anmelden-Knopf tut nichts.\n- Login klappt"),
        "{text}"
    );
    assert!(!text.contains("<p>"), "{text}");
    ohne_geheimnisse(&text);

    // Zwei Anfragen: die Seite und ihr Bereich (für den Schlüssel).
    assert_eq!(attrappe.aufzeichnung.anzahl("api/v2/pages/123"), 1);
    assert_eq!(attrappe.aufzeichnung.anzahl("api/v2/spaces/10"), 1);
    assert_eq!(
        attrappe
            .aufzeichnung
            .erster("api/v2/pages/123")
            .unwrap()
            .feld("body-format"),
        Some("storage")
    );

    // `als_storage` gibt das Original.
    let text = antwort(
        &server,
        "confluence_seite",
        json!({ "id": SEITE, "als_storage": true }),
    )
    .await;
    assert!(text.contains("## Inhalt (Storage-Format)"), "{text}");
    assert!(
        text.contains("<p>Der Anmelden-Knopf tut nichts.</p>"),
        "{text}"
    );
}

#[tokio::test]
async fn eine_seite_wird_auch_ueber_bereich_und_titel_gefunden() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(
        &server,
        "confluence_seite",
        json!({ "bereich": BEREICH, "titel": SEITE_TITEL }),
    )
    .await;
    assert!(text.starts_with("123 Anmeldung [current]"), "{text}");

    let liste = attrappe
        .aufzeichnung
        .zu("api/v2/pages")
        .into_iter()
        .find(|aufruf| aufruf.pfad.ends_with("/pages"))
        .expect("keine Suche über den Titel");
    assert_eq!(liste.feld("space-id"), Some(BEREICH_ID));
    assert_eq!(liste.feld("title"), Some(SEITE_TITEL));
    assert_eq!(liste.feld("body-format"), Some("storage"));

    // Kein Treffer ist ein Text, kein Fehler.
    let text = antwort(
        &server,
        "confluence_seite",
        json!({ "bereich": BEREICH, "titel": "Gibt es nicht" }),
    )
    .await;
    assert!(text.contains("Keine Seite „Gibt es nicht“"), "{text}");

    // Weder Id noch Bereich und Titel: Parameterfehler, keine Anfrage.
    let vorher = attrappe.aufzeichnung.alle().len();
    let fehler = aufruf(&server, "confluence_seite", json!({ "bereich": BEREICH }))
        .await
        .unwrap_err();
    assert!(fehler.message.contains("titel"), "{}", fehler.message);
    assert_eq!(attrappe.aufzeichnung.alle().len(), vorher);
}

// -------------------------------------------------- Unterseiten und Anhang

#[tokio::test]
async fn unterseiten_kommentare_anhaenge_und_labels_werden_gelesen() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(&server, "confluence_unterseiten", json!({ "id": SEITE })).await;
    assert!(text.contains("1 Unterseite(n) unter 123"), "{text}");
    assert!(text.contains(UNTERSEITE), "{text}");
    assert!(text.contains("Unterpunkt"), "{text}");

    let text = antwort(&server, "confluence_kommentare", json!({ "id": SEITE })).await;
    assert!(text.contains("2 Kommentar(e) an 123"), "{text}");
    assert!(text.contains(KOMMENTAR), "{text}");
    assert!(text.contains("Sieht gut aus"), "{text}");
    assert!(text.contains("Bitte noch ein Test"), "{text}");
    assert!(!text.contains("<p>"), "{text}");
    ohne_geheimnisse(&text);
    assert_eq!(
        attrappe
            .aufzeichnung
            .erster("footer-comments")
            .unwrap()
            .feld("body-format"),
        Some("storage")
    );

    let text = antwort(&server, "confluence_anhaenge", json!({ "id": SEITE })).await;
    assert!(
        text.contains("- bild.png · image/png · 4 kB · Version 2 · id 900"),
        "{text}"
    );
    assert!(
        text.contains("- handbuch.pdf · application/pdf · 1.0 MB"),
        "{text}"
    );

    let text = antwort(&server, "confluence_labels", json!({ "id": SEITE })).await;
    assert!(text.contains("2 Label(s) an 123"), "{text}");
    assert!(text.contains("global:anmeldung"), "{text}");
    assert!(text.contains("global:doku"), "{text}");

    // Eine unbekannte Seite ist ein kurzer Fehler ohne Geheimnis.
    let fehler = aufruf(&server, "confluence_anhaenge", json!({ "id": "999" }))
        .await
        .unwrap_err();
    assert!(fehler.message.contains("404"), "{}", fehler.message);
    ohne_geheimnisse(&fehler.message);
    assert_eq!(attrappe.aufzeichnung.anzahl("api/v2/pages/999"), 1);
}

// ------------------------------------------------------------------ Suche

#[tokio::test]
async fn die_suche_baut_die_cql_aus_den_komfort_parametern() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(
        &server,
        "confluence_suchen",
        json!({ "text": "Anmeldung", "bereich": BEREICH }),
    )
    .await;
    assert!(text.contains("3 Treffer:"), "{text}");
    assert!(
        text.contains("- 123 [page] Anmeldung · Bereich ENG"),
        "{text}"
    );
    assert!(text.contains("Der Anmelden-Knopf tut nichts."), "{text}");
    assert!(!text.contains("<b>"), "{text}");
    ohne_geheimnisse(&text);

    let erster = attrappe.aufzeichnung.erster("rest/api/search").unwrap();
    assert_eq!(
        erster.feld("cql"),
        Some("space = \"ENG\" AND text ~ \"Anmeldung\"")
    );
    assert_eq!(erster.feld("limit"), Some("25"));
    // Drei Treffer, zwei je Seite: eine Folgeseite.
    assert_eq!(attrappe.aufzeichnung.anzahl("rest/api/search"), 2);

    // Ein fertiges `cql` ersetzt die Komfort-Felder.
    let text = antwort(
        &server,
        "confluence_suchen",
        json!({ "cql": "space = \"OPS\"", "text": "egal" }),
    )
    .await;
    assert!(text.contains("3 Treffer:"), "{text}");
    assert_eq!(
        attrappe
            .aufzeichnung
            .zu("rest/api/search")
            .last()
            .unwrap()
            .feld("cql"),
        Some("space = \"OPS\"")
    );

    // Ohne alles: Parameterfehler, keine Anfrage.
    let vorher = attrappe.aufzeichnung.alle().len();
    let fehler = aufruf(&server, "confluence_suchen", json!({}))
        .await
        .unwrap_err();
    assert!(fehler.message.contains("cql"), "{}", fehler.message);
    assert_eq!(attrappe.aufzeichnung.alle().len(), vorher);

    // Ein Anführungszeichen im Suchtext zerlegt die Abfrage nicht.
    antwort(
        &server,
        "confluence_suchen",
        json!({ "text": "Kunde sagt \"Hallo\"" }),
    )
    .await;
    let cql = attrappe
        .aufzeichnung
        .zu("rest/api/search")
        .last()
        .unwrap()
        .feld("cql")
        .unwrap()
        .to_string();
    assert_eq!(cql, "text ~ \"Kunde sagt \\\"Hallo\\\"\"");
}

// --------------------------------------------------------------- Schreiben

#[tokio::test]
async fn eine_seite_wird_angelegt_und_der_text_maskiert() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(
        &server,
        "confluence_seite_anlegen",
        json!({
            "bereich": BEREICH,
            "titel": "Neue Seite",
            "text": "Hallo & Welt\n\nZweiter Absatz",
            "eltern": SEITE
        }),
    )
    .await;
    assert!(
        text.contains(&format!(
            "Seite angelegt: {NEUE_SEITE} Neue Seite [current] · Version 1"
        )),
        "{text}"
    );
    ohne_geheimnisse(&text);

    // Erst der Bereich vom Server, dann das Anlegen.
    let bereich = attrappe.aufzeichnung.erster("api/v2/spaces").unwrap();
    assert_eq!(bereich.feld("keys"), Some(BEREICH));
    let post = attrappe
        .aufzeichnung
        .zu_mit("api/v2/pages", "POST")
        .pop()
        .unwrap();
    let rumpf = post.json();
    assert_eq!(rumpf["spaceId"], json!(BEREICH_ID));
    assert_eq!(rumpf["title"], json!("Neue Seite"));
    assert_eq!(rumpf["status"], json!("current"));
    assert_eq!(rumpf["parentId"], json!(SEITE));
    assert_eq!(rumpf["body"]["representation"], json!("storage"));
    // `text` wird maskiert, nicht als HTML gedeutet.
    assert_eq!(
        rumpf["body"]["value"],
        json!("<p>Hallo &amp; Welt</p><p>Zweiter Absatz</p>")
    );

    // `storage` geht unverändert durch.
    antwort(
        &server,
        "confluence_seite_anlegen",
        json!({ "bereich": BEREICH, "titel": "Roh", "storage": "<p><strong>roh</strong></p>" }),
    )
    .await;
    let rumpf = attrappe
        .aufzeichnung
        .zu_mit("api/v2/pages", "POST")
        .pop()
        .unwrap()
        .json();
    assert_eq!(rumpf["body"]["value"], json!("<p><strong>roh</strong></p>"));

    // Beides zugleich ist ein Parameterfehler, keine Anfrage.
    let vorher = attrappe.aufzeichnung.anzahl_mit("api/v2/pages", "POST");
    let fehler = aufruf(
        &server,
        "confluence_seite_anlegen",
        json!({ "bereich": BEREICH, "titel": "x", "text": "a", "storage": "b" }),
    )
    .await
    .unwrap_err();
    assert!(
        fehler.message.contains("nicht beides"),
        "{}",
        fehler.message
    );
    assert_eq!(
        attrappe.aufzeichnung.anzahl_mit("api/v2/pages", "POST"),
        vorher
    );
}

#[tokio::test]
async fn eine_seite_wird_geaendert_ohne_titel_und_status_zu_verlieren() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(
        &server,
        "confluence_seite_aendern",
        json!({ "id": SEITE, "version": SEITE_VERSION, "text": "Neuer Text" }),
    )
    .await;
    assert!(
        text.contains("Seite 123 geändert: Anmeldung · Version 4 (Inhalt ersetzt)"),
        "{text}"
    );
    ohne_geheimnisse(&text);

    let put = attrappe
        .aufzeichnung
        .zu_mit("api/v2/pages/123", "PUT")
        .pop()
        .unwrap();
    let rumpf = put.json();
    assert_eq!(rumpf["id"], json!(SEITE));
    // Titel und Status bleiben, wenn sie nicht angegeben sind.
    assert_eq!(rumpf["title"], json!(SEITE_TITEL));
    assert_eq!(rumpf["status"], json!("current"));
    // Die Version zählt hoch.
    assert_eq!(rumpf["version"]["number"], json!(SEITE_VERSION + 1));
    assert_eq!(rumpf["body"]["value"], json!("<p>Neuer Text</p>"));

    // `anhaengen` hängt an, statt zu ersetzen.
    antwort(
        &server,
        "confluence_seite_aendern",
        json!({ "id": SEITE, "version": SEITE_VERSION, "text": "Zusatz", "anhaengen": true }),
    )
    .await;
    let rumpf = attrappe
        .aufzeichnung
        .zu_mit("api/v2/pages/123", "PUT")
        .pop()
        .unwrap()
        .json();
    // Der alte Storage-Inhalt bleibt **bytegleich**, der neue kommt danach.
    assert_eq!(
        rumpf["body"]["value"],
        json!(format!("{SEITE_INHALT}<p>Zusatz</p>"))
    );
}

#[tokio::test]
async fn eine_abweichende_version_aendert_nichts() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(
        &server,
        "confluence_seite_aendern",
        json!({ "id": SEITE, "version": SEITE_VERSION - 1, "text": "Neuer Text" }),
    )
    .await;
    assert!(text.contains("wurde inzwischen geändert"), "{text}");
    assert!(text.contains("nichts überschrieben"), "{text}");
    assert!(text.contains("confluence_seite"), "{text}");
    // Gelesen ja, geschrieben nein.
    assert_eq!(attrappe.aufzeichnung.anzahl("api/v2/pages/123"), 1);
    assert_eq!(
        attrappe.aufzeichnung.anzahl_mit("api/v2/pages/123", "PUT"),
        0
    );
}

#[tokio::test]
async fn ein_409_beim_aendern_wird_zu_neu_lesen() {
    let attrappe = Attrappe::starte().await;
    attrappe.konflikt();
    let server = eine(&attrappe.adresse);

    let text = antwort(
        &server,
        "confluence_seite_aendern",
        json!({ "id": SEITE, "version": SEITE_VERSION, "text": "Neuer Text" }),
    )
    .await;
    assert!(text.contains("wurde inzwischen geändert"), "{text}");
    assert!(text.contains("nichts überschrieben"), "{text}");
    // Genau ein Versuch: nicht wiederholt (ob es ankam, weiß nur der Server).
    assert_eq!(
        attrappe.aufzeichnung.anzahl_mit("api/v2/pages/123", "PUT"),
        1
    );
}

#[tokio::test]
async fn kommentar_und_label_werden_geschrieben() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(
        &server,
        "confluence_kommentieren",
        json!({ "id": SEITE, "text": "Sieht gut aus" }),
    )
    .await;
    assert!(
        text.contains(&format!("Kommentar {KOMMENTAR} an Seite 123 angelegt")),
        "{text}"
    );
    let post = attrappe
        .aufzeichnung
        .zu_mit("footer-comments", "POST")
        .pop()
        .unwrap();
    // Genau die Felder der Spezifikation (`CreateFooterCommentModel`).
    assert_eq!(
        post.json(),
        json!({
            "pageId": SEITE,
            "body": { "representation": "storage", "value": "<p>Sieht gut aus</p>" }
        })
    );

    let text = antwort(
        &server,
        "confluence_label_hinzufuegen",
        json!({ "id": SEITE, "label": "anmeldung" }),
    )
    .await;
    assert!(
        text.contains("Label anmeldung an Seite 123 hinzugefügt"),
        "{text}"
    );
    // v2 kennt hier nur `GET`; geschrieben wird über die ältere API.
    assert_eq!(
        attrappe.aufzeichnung.anzahl_mit("pages/123/labels", "POST"),
        0
    );
    let post = attrappe
        .aufzeichnung
        .zu_mit("rest/api/content/123/label", "POST")
        .pop()
        .expect("kein POST auf /wiki/rest/api/content/{id}/label");
    assert_eq!(post.pfad, "/wiki/rest/api/content/123/label");
    assert_eq!(
        post.json(),
        json!([{ "prefix": "global", "name": "anmeldung" }])
    );
}

#[tokio::test]
async fn eine_leere_antwort_204_ist_erfolg() {
    let attrappe = Attrappe::starte().await;
    attrappe.leere_antwort();
    let server = eine(&attrappe.adresse);

    let text = antwort(
        &server,
        "confluence_label_hinzufuegen",
        json!({ "id": SEITE, "label": "anmeldung" }),
    )
    .await;
    assert!(text.contains("hinzugefügt"), "{text}");

    let text = antwort(
        &server,
        "confluence_kommentieren",
        json!({ "id": SEITE, "text": "Ohne Rumpf" }),
    )
    .await;
    assert!(text.contains("Kommentar an Seite 123 angelegt"), "{text}");
}

#[tokio::test]
async fn schreiben_aus_macht_bei_allen_vier_werkzeugen_keine_anfrage() {
    let attrappe = Attrappe::starte().await;
    let server = eine_ohne_schreiben(&attrappe.adresse);

    for (name, argumente) in [
        (
            "confluence_seite_anlegen",
            json!({ "bereich": BEREICH, "titel": "Neu", "text": "x" }),
        ),
        (
            "confluence_seite_aendern",
            json!({ "id": SEITE, "version": SEITE_VERSION, "text": "x" }),
        ),
        (
            "confluence_kommentieren",
            json!({ "id": SEITE, "text": "hallo" }),
        ),
        (
            "confluence_label_hinzufuegen",
            json!({ "id": SEITE, "label": "anmeldung" }),
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
async fn nur_verhindert_schreiben_in_einem_fremden_bereich() {
    let attrappe = Attrappe::starte().await;
    // Erlaubt ist nur OPS; die Seite 123 liegt in ENG.
    let server = eine_nur(&attrappe.adresse, BEREICH_2);

    for (name, argumente) in [
        (
            "confluence_seite_aendern",
            json!({ "id": SEITE, "version": SEITE_VERSION, "text": "x" }),
        ),
        (
            "confluence_kommentieren",
            json!({ "id": SEITE, "text": "hallo" }),
        ),
        (
            "confluence_label_hinzufuegen",
            json!({ "id": SEITE, "label": "anmeldung" }),
        ),
        (
            "confluence_seite_anlegen",
            json!({ "bereich": BEREICH, "titel": "Neu", "text": "x" }),
        ),
    ] {
        let text = antwort(&server, name, argumente).await;
        assert!(text.contains("nicht freigegeben"), "{name}: {text}");
        assert!(text.contains("nichts geändert"), "{name}: {text}");
    }

    // Keine verändernde Anfrage – der Schlüssel kam vom Server.
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("api/v2/pages", "POST"), 0);
    assert_eq!(
        attrappe.aufzeichnung.anzahl_mit("api/v2/pages/123", "PUT"),
        0
    );
    assert_eq!(
        attrappe.aufzeichnung.anzahl_mit("footer-comments", "POST"),
        0
    );
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("labels", "POST"), 0);

    // Der Bereichsschlüssel wurde gelesen, nicht geglaubt.
    assert_eq!(attrappe.aufzeichnung.anzahl("api/v2/spaces/10"), 3);

    // Groß-/Kleinschreibung zählt nicht: mit `eng` ist ENG erlaubt.
    let server = eine_nur(&attrappe.adresse, "eng");
    let text = antwort(
        &server,
        "confluence_seite_aendern",
        json!({ "id": SEITE, "version": SEITE_VERSION, "text": "x" }),
    )
    .await;
    assert!(text.contains("Seite 123 geändert"), "{text}");
    assert_eq!(
        attrappe.aufzeichnung.anzahl_mit("api/v2/pages/123", "PUT"),
        1
    );
}

// ------------------------------------------------- Datenverlust und Vertrag

#[tokio::test]
async fn ein_punkt_segment_wird_ohne_anfrage_abgelehnt() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    for id in ["..", ".", "..."] {
        for (name, argumente) in [
            ("confluence_seite", json!({ "id": id })),
            ("confluence_unterseiten", json!({ "id": id })),
            ("confluence_kommentare", json!({ "id": id })),
            ("confluence_anhaenge", json!({ "id": id })),
            ("confluence_labels", json!({ "id": id })),
            (
                "confluence_seite_aendern",
                json!({ "id": id, "version": SEITE_VERSION, "text": "x" }),
            ),
            ("confluence_kommentieren", json!({ "id": id, "text": "x" })),
            (
                "confluence_label_hinzufuegen",
                json!({ "id": id, "label": "x" }),
            ),
        ] {
            let fehler = aufruf(&server, name, argumente).await.unwrap_err();
            assert!(
                fehler.message.contains("nicht erlaubt"),
                "{name}/{id}: {}",
                fehler.message
            );
            assert!(
                fehler.message.contains("nicht gesendet"),
                "{name}/{id}: {}",
                fehler.message
            );
            ohne_geheimnisse(&fehler.message);
        }
    }
    // Nichts davon hat den Host erreicht.
    assert!(
        attrappe.aufzeichnung.alle().is_empty(),
        "trotz Punkt-Segment gab es Anfragen: {:?}",
        attrappe::aufruf_pfade(&attrappe.aufzeichnung)
    );
}

#[tokio::test]
async fn ein_anhaengen_ohne_alten_inhalt_ersetzt_nichts() {
    let attrappe = Attrappe::starte().await;
    attrappe.seite_ohne_inhalt();
    let server = eine(&attrappe.adresse);

    let fehler = aufruf(
        &server,
        "confluence_seite_aendern",
        json!({ "id": SEITE, "version": SEITE_VERSION, "text": "Zusatz", "anhaengen": true }),
    )
    .await
    .unwrap_err();
    assert!(
        fehler.message.contains("keinen Inhalt"),
        "{}",
        fehler.message
    );
    // Ohne den alten Inhalt würde die Seite geleert: kein PUT.
    assert_eq!(
        attrappe.aufzeichnung.anzahl_mit("api/v2/pages/123", "PUT"),
        0
    );

    // Ohne `anhaengen` ist ein fehlender alter Inhalt harmlos.
    let text = antwort(
        &server,
        "confluence_seite_aendern",
        json!({ "id": SEITE, "version": SEITE_VERSION, "text": "Neu" }),
    )
    .await;
    assert!(text.contains("Seite 123 geändert"), "{text}");
}

#[tokio::test]
async fn eine_seite_ohne_titel_wird_nicht_ueberschrieben() {
    let attrappe = Attrappe::starte().await;
    attrappe.seite_ohne_titel();
    let server = eine(&attrappe.adresse);

    let fehler = aufruf(
        &server,
        "confluence_seite_aendern",
        json!({ "id": SEITE, "version": SEITE_VERSION, "text": "Neu" }),
    )
    .await
    .unwrap_err();
    assert!(
        fehler.message.contains("keinen Titel"),
        "{}",
        fehler.message
    );
    assert!(
        !fehler.message.contains("(ohne Titel)"),
        "{}",
        fehler.message
    );
    assert_eq!(
        attrappe.aufzeichnung.anzahl_mit("api/v2/pages/123", "PUT"),
        0
    );

    // Mit ausdrücklichem Titel geht es.
    let text = antwort(
        &server,
        "confluence_seite_aendern",
        json!({ "id": SEITE, "version": SEITE_VERSION, "titel": "Neu", "text": "x" }),
    )
    .await;
    assert!(text.contains("Seite 123 geändert: Neu"), "{text}");
}

#[tokio::test]
async fn eine_ungueltige_version_wird_vor_der_anfrage_abgelehnt() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    // 0 ist keine Version.
    let fehler = aufruf(
        &server,
        "confluence_seite_aendern",
        json!({ "id": SEITE, "version": 0, "text": "x" }),
    )
    .await
    .unwrap_err();
    assert!(fehler.message.contains("version"), "{}", fehler.message);

    // Eine Kommazahl ist keine ganze Version.
    let fehler = aufruf(
        &server,
        "confluence_seite_aendern",
        json!({ "id": SEITE, "version": 3.5, "text": "x" }),
    )
    .await
    .unwrap_err();
    assert!(fehler.message.contains("Parameter"), "{}", fehler.message);

    // Zu groß für `i64` – kein Überlauf, sondern „neu lesen“.
    let text = antwort(
        &server,
        "confluence_seite_aendern",
        json!({ "id": SEITE, "version": u64::MAX, "text": "x" }),
    )
    .await;
    assert!(text.contains("neu lesen"), "{text}");
    assert_eq!(
        attrappe.aufzeichnung.anzahl_mit("api/v2/pages/123", "PUT"),
        0
    );
}

#[tokio::test]
async fn status_wird_nur_laut_spezifikation_gesendet() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let fehler = aufruf(
        &server,
        "confluence_seite_aendern",
        json!({ "id": SEITE, "version": SEITE_VERSION, "text": "x", "status": "archived" }),
    )
    .await
    .unwrap_err();
    assert!(fehler.message.contains("current"), "{}", fehler.message);
    assert!(fehler.message.contains("draft"), "{}", fehler.message);
    assert!(attrappe.aufzeichnung.alle().is_empty());

    // Ausdrücklich `draft` ist laut Spezifikation erlaubt.
    antwort(
        &server,
        "confluence_seite_aendern",
        json!({ "id": SEITE, "version": SEITE_VERSION, "text": "x", "status": "draft" }),
    )
    .await;
    let rumpf = attrappe
        .aufzeichnung
        .zu_mit("api/v2/pages/123", "PUT")
        .pop()
        .unwrap()
        .json();
    assert_eq!(rumpf["status"], json!("draft"));

    // Fehlt der Status in der Antwort, wird nichts geschrieben …
    let attrappe = Attrappe::starte().await;
    attrappe.seite_ohne_status();
    let server = eine(&attrappe.adresse);
    let fehler = aufruf(
        &server,
        "confluence_seite_aendern",
        json!({ "id": SEITE, "version": SEITE_VERSION, "text": "x" }),
    )
    .await
    .unwrap_err();
    assert!(
        fehler.message.contains("keinen Status"),
        "{}",
        fehler.message
    );
    assert_eq!(
        attrappe.aufzeichnung.anzahl_mit("api/v2/pages/123", "PUT"),
        0
    );
}

#[tokio::test]
async fn ein_falscher_bereichsschluessel_aus_der_antwort_wird_abgelehnt() {
    let attrappe = Attrappe::starte().await;
    attrappe.falscher_schluessel();
    let server = eine(&attrappe.adresse);

    let fehler = aufruf(
        &server,
        "confluence_seite_anlegen",
        json!({ "bereich": BEREICH, "titel": "Neu", "text": "x" }),
    )
    .await
    .unwrap_err();
    assert!(
        fehler.message.contains("nennt den Bereich"),
        "{}",
        fehler.message
    );
    // Der Schlüssel der Antwort passt nicht: kein POST.
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("api/v2/pages", "POST"), 0);

    // Groß-/Kleinschreibung zählt beim Vergleich nicht.
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);
    let text = antwort(
        &server,
        "confluence_seite_anlegen",
        json!({ "bereich": "eng", "titel": "Neu", "text": "x" }),
    )
    .await;
    assert!(text.contains("Seite angelegt"), "{text}");
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("api/v2/pages", "POST"), 1);
}

#[tokio::test]
async fn ein_bereich_ohne_schluessel_in_der_antwort_ist_ein_fehler() {
    let attrappe = Attrappe::starte().await;
    attrappe.bereich_ohne_schluessel();
    let server = eine(&attrappe.adresse);

    let fehler = aufruf(&server, "confluence_seite", json!({ "id": SEITE }))
        .await
        .unwrap_err();
    assert!(
        fehler.message.contains("keinen Schlüssel"),
        "{}",
        fehler.message
    );
    // Die Id ist kein Schlüssel – sie wird nicht ersatzweise gemeldet.
    assert!(
        !fehler.message.contains("Bereich: 10"),
        "{}",
        fehler.message
    );

    // Mit `nur` wird der Bereich auch beim Kommentieren geprüft.
    let server = eine_nur(&attrappe.adresse, BEREICH);
    let fehler = aufruf(
        &server,
        "confluence_kommentieren",
        json!({ "id": SEITE, "text": "Hallo" }),
    )
    .await
    .unwrap_err();
    assert!(
        fehler.message.contains("keinen Schlüssel"),
        "{}",
        fehler.message
    );
    assert_eq!(
        attrappe.aufzeichnung.anzahl_mit("footer-comments", "POST"),
        0
    );
}

#[tokio::test]
async fn eine_elternseite_in_einem_anderen_bereich_wird_abgelehnt() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(
        &server,
        "confluence_seite_anlegen",
        json!({ "bereich": BEREICH, "titel": "Neu", "text": "x", "eltern": FREMDE_SEITE }),
    )
    .await;
    assert!(text.contains("anderen Bereich"), "{text}");
    assert!(text.contains("nichts angelegt"), "{text}");
    // Die Elternseite wurde gelesen, aber nichts angelegt.
    assert_eq!(attrappe.aufzeichnung.anzahl("api/v2/pages/300"), 1);
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("api/v2/pages", "POST"), 0);

    // Eine unbekannte Elternseite ist ein Fehler, kein POST.
    let fehler = aufruf(
        &server,
        "confluence_seite_anlegen",
        json!({ "bereich": BEREICH, "titel": "Neu", "text": "x", "eltern": "999" }),
    )
    .await
    .unwrap_err();
    assert!(fehler.message.contains("404"), "{}", fehler.message);
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("api/v2/pages", "POST"), 0);
}

#[tokio::test]
async fn ein_unlesbarer_rumpf_ist_ein_fehler_und_nie_leer() {
    // Ein Server, dessen Antwort kürzer ist als ihr `Content-Length`.
    let server = eine(&attrappe::kaputte_antwort());
    let fehler = aufruf(&server, "confluence_bereiche", json!({}))
        .await
        .unwrap_err();
    assert!(
        fehler.message.contains("nicht lesbar"),
        "{}",
        fehler.message
    );
    ohne_geheimnisse(&fehler.message);

    // Bei einer Änderung mit 2xx: „ausgeführt, Antwort nicht lesbar“.
    let server = eine(&attrappe::kaputte_aenderung());
    let fehler = aufruf(
        &server,
        "confluence_seite_aendern",
        json!({ "id": SEITE, "version": SEITE_VERSION, "text": "Neu" }),
    )
    .await
    .unwrap_err();
    assert!(fehler.message.contains("ausgeführt"), "{}", fehler.message);
    assert!(
        fehler.message.contains("nicht lesbar"),
        "{}",
        fehler.message
    );
    ohne_geheimnisse(&fehler.message);
}

// ------------------------------------------------------------ Fehlerwege

#[tokio::test]
async fn ein_ungueltiger_token_wird_nicht_wiederholt() {
    let attrappe = Attrappe::starte().await;
    attrappe.token_ablehnen();
    let server = eine(&attrappe.adresse);

    let fehler = aufruf(&server, "confluence_bereiche", json!({}))
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
        fehler.message.contains("Tools → Confluence erneuern"),
        "{}",
        fehler.message
    );
    ohne_geheimnisse(&fehler.message);
    // Genau ein Versuch.
    assert_eq!(attrappe.aufzeichnung.anzahl("api/v2/spaces"), 1);
}

#[tokio::test]
async fn fehlende_rechte_werden_als_solche_gemeldet() {
    let attrappe = Attrappe::starte().await;
    attrappe.rechte_verweigern();
    let server = eine(&attrappe.adresse);

    let fehler = aufruf(&server, "confluence_bereiche", json!({}))
        .await
        .unwrap_err();
    assert!(
        fehler.message.contains("Fehlende Rechte für firma"),
        "{}",
        fehler.message
    );
    assert!(
        fehler.message.contains("Rechte prüfen"),
        "{}",
        fehler.message
    );
    ohne_geheimnisse(&fehler.message);
    assert_eq!(attrappe.aufzeichnung.anzahl("api/v2/spaces"), 1);
}

#[tokio::test]
async fn ein_lesender_aufruf_wiederholt_bei_429_und_503_hoechstens_zweimal() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    // 429: zwei abgelehnte Versuche, dann klappt es.
    attrappe.zu_viele_anfragen(2);
    let text = antwort(&server, "confluence_bereiche", json!({ "anzahl": 2 })).await;
    assert!(text.contains("2 Bereich(e)"), "{text}");
    assert_eq!(attrappe.aufzeichnung.anzahl("api/v2/spaces"), 3);

    // 503 zählt genauso.
    attrappe.ausgelastet(1);
    let text = antwort(&server, "confluence_bereiche", json!({ "anzahl": 2 })).await;
    assert!(text.contains("2 Bereich(e)"), "{text}");
    assert_eq!(attrappe.aufzeichnung.anzahl("api/v2/spaces"), 5);

    // Mehr als zwei Wiederholungen gibt es nicht.
    attrappe.zu_viele_anfragen(3);
    let fehler = aufruf(&server, "confluence_bereiche", json!({ "anzahl": 2 }))
        .await
        .unwrap_err();
    assert!(fehler.message.contains("429"), "{}", fehler.message);
    assert!(
        fehler.message.contains("Wiederholungen"),
        "{}",
        fehler.message
    );
    assert_eq!(attrappe.aufzeichnung.anzahl("api/v2/spaces"), 8);
}

#[tokio::test]
async fn ein_anlegender_aufruf_wiederholt_bei_429_nicht() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    // Nur der verändernde Aufruf wird abgelehnt (der Bereich davor ist lesend).
    attrappe.zu_viele_schreibende_anfragen(1);
    let fehler = aufruf(
        &server,
        "confluence_seite_anlegen",
        json!({ "bereich": BEREICH, "titel": "Neu", "text": "x" }),
    )
    .await
    .unwrap_err();
    assert!(fehler.message.contains("429"), "{}", fehler.message);
    // Genau ein Versuch – ob der Aufruf ankam, weiß nur der Server.
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("api/v2/pages", "POST"), 1);
}

#[tokio::test]
async fn einer_weiterleitung_wird_nicht_gefolgt() {
    let ziel = Attrappe::starte().await;
    let attrappe = Attrappe::starte().await;
    attrappe.weiterleiten(&ziel.adresse);
    let server = eine(&attrappe.adresse);

    let fehler = aufruf(&server, "confluence_bereiche", json!({ "anzahl": 2 }))
        .await
        .unwrap_err();
    assert!(fehler.message.contains("302"), "{}", fehler.message);
    ohne_geheimnisse(&fehler.message);
    // Der fremde Host hat nichts gesehen: keine Kopfzeile, keine Anfrage.
    assert!(
        ziel.aufzeichnung.alle().is_empty(),
        "die Weiterleitung wurde verfolgt: {:?}",
        attrappe::aufruf_pfade(&ziel.aufzeichnung)
    );
}

#[tokio::test]
async fn eine_folgeseite_mit_fremdem_host_wird_ohne_anfrage_abgelehnt() {
    let ziel = Attrappe::starte().await;
    let attrappe = Attrappe::starte().await;
    attrappe.fremde_folgeseite(&ziel.adresse);
    let server = eine(&attrappe.adresse);

    let fehler = aufruf(&server, "confluence_bereiche", json!({}))
        .await
        .unwrap_err();
    assert!(
        fehler.message.contains("anderen Host"),
        "{}",
        fehler.message
    );
    assert!(
        fehler.message.contains("nicht gesendet"),
        "{}",
        fehler.message
    );
    ohne_geheimnisse(&fehler.message);
    // Genau die erste Seite – die Folgeseite ging nirgendwohin.
    assert_eq!(attrappe.aufzeichnung.anzahl("api/v2/spaces"), 1);
    assert!(
        ziel.aufzeichnung.alle().is_empty(),
        "die fremde Folgeseite wurde geladen: {:?}",
        attrappe::aufruf_pfade(&ziel.aufzeichnung)
    );
}

#[tokio::test]
async fn werte_im_pfad_werden_prozent_kodiert() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    for (id, kodiert) in [
        ("../spaces/10", "%2E%2E%2Fspaces%2F10"),
        ("a/b", "a%2Fb"),
        ("a?b", "a%3Fb"),
        ("a#b", "a%23b"),
    ] {
        let fehler = aufruf(&server, "confluence_seite", json!({ "id": id }))
            .await
            .unwrap_err();
        assert!(fehler.message.contains("404"), "{}", fehler.message);

        let aufruf = attrappe.aufzeichnung.letzter().unwrap();
        // Der kodierte Wert bleibt **ein** Pfadsegment – kein anderer Endpunkt.
        assert_eq!(aufruf.pfad, format!("/wiki/api/v2/pages/{kodiert}"));
        assert!(!aufruf.pfad.contains(".."), "{}", aufruf.pfad);
        assert!(!aufruf.pfad.contains('?'), "{}", aufruf.pfad);
        assert!(!aufruf.pfad.contains('#'), "{}", aufruf.pfad);
    }
}

// ------------------------------------------------------------- Querschnitt

#[tokio::test]
async fn keine_antwort_nennt_token_oder_email() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let antworten = vec![
        antwort(&server, "confluence_testen", json!({})).await,
        antwort(&server, "confluence_bereiche", json!({ "anzahl": 2 })).await,
        antwort(&server, "confluence_suchen", json!({ "text": "Anmeldung" })).await,
        antwort(&server, "confluence_seite", json!({ "id": SEITE })).await,
        antwort(&server, "confluence_unterseiten", json!({ "id": SEITE })).await,
        antwort(&server, "confluence_kommentare", json!({ "id": SEITE })).await,
        antwort(&server, "confluence_anhaenge", json!({ "id": SEITE })).await,
        antwort(&server, "confluence_labels", json!({ "id": SEITE })).await,
    ];
    for text in &antworten {
        ohne_geheimnisse(text);
    }

    for aufruf in attrappe.aufzeichnung.alle() {
        // Beides steckt nur in der Kopfzeile.
        assert!(!aufruf.pfad.contains(TOKEN_A));
        assert!(!aufruf.pfad.contains(EMAIL));
        assert!(!aufruf.rumpf.contains(TOKEN_A));
        assert!(!aufruf.rumpf.contains(EMAIL));
    }
}
