// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Jedes Werkzeug einmal gegen die Attrappe (SPEC §7): Pfad, Rumpf, Antwortform
//! – und nie ein Schlüssel im Text.

mod attrappe;

use attrappe::{Attrappe, SCHLUESSEL};
use quiver_outline::OutlineServer;
use rmcp::model::JsonObject;
use rmcp::ErrorData as McpError;
use serde_json::{json, Value};

/// Ein Werkzeug aufrufen und den Text der Antwort holen.
async fn ruf(server: &OutlineServer, name: &str, argumente: Value) -> Result<String, McpError> {
    let karte: JsonObject = serde_json::from_value(argumente).unwrap();
    server
        .aufrufen(name, Some(karte))
        .await
        .map(|ergebnis| ergebnis.content[0].as_text().unwrap().text.clone())
}

#[tokio::test]
async fn lesende_werkzeuge_holen_und_formatieren() {
    let mock = Attrappe::starte().await;
    let server = OutlineServer::neu(attrappe::zugang(&mock.adresse));

    // Sammlungen: Pfad, Rumpf, Anmeldung und Antwortform.
    let text = ruf(&server, "outline_collections", json!({}))
        .await
        .unwrap();
    assert!(text.contains("Sammlungen (2):"), "{text}");
    assert!(
        text.contains("- Technik (id c1) – Alles Technische"),
        "{text}"
    );
    let aufruf = mock.aufzeichnung.gemerkt("collections.list").unwrap();
    assert_eq!(aufruf.pfad, "/api/collections.list");
    assert_eq!(aufruf.feld("limit"), json!(50));
    assert_eq!(aufruf.feld("offset"), json!(0));
    assert_eq!(
        aufruf.anmeldung.as_deref(),
        Some(format!("Bearer {SCHLUESSEL}").as_str())
    );

    // Dokumentbaum: die Sammlungs-id geht mit, der Name wird nachgeschlagen.
    let text = ruf(
        &server,
        "outline_structure",
        json!({ "collection_id": "c1" }),
    )
    .await
    .unwrap();
    assert!(text.contains("Dokumentbaum von „Technik“:"), "{text}");
    assert!(text.contains("- Betriebshandbuch (id d1)"), "{text}");
    assert!(text.contains("  - Notfall (id d2)"), "{text}");
    assert_eq!(
        mock.aufzeichnung
            .gemerkt("collections.documents")
            .unwrap()
            .rumpf,
        json!({ "id": "c1" })
    );

    // Suche.
    let text = ruf(
        &server,
        "outline_search",
        json!({ "query": "handbuch", "collection_id": "c1" }),
    )
    .await
    .unwrap();
    assert!(text.contains("Suche „handbuch“ – 1 Treffer"), "{text}");
    assert!(text.contains("id d1 | geändert 2026-09-02 10:11"), "{text}");
    let suchrumpf = mock.aufzeichnung.gemerkt("documents.search").unwrap().rumpf;
    assert_eq!(suchrumpf["query"], json!("handbuch"));
    assert_eq!(suchrumpf["collectionId"], json!("c1"));
    assert_eq!(suchrumpf["limit"], json!(15));

    // Liste.
    let text = ruf(&server, "outline_list", json!({ "collection_id": "c1" }))
        .await
        .unwrap();
    assert!(text.contains("Dokumente in Sammlung c1 (2):"), "{text}");
    let listenrumpf = mock.aufzeichnung.gemerkt("documents.list").unwrap().rumpf;
    assert_eq!(listenrumpf["sort"], json!("updatedAt"));
    assert_eq!(listenrumpf["direction"], json!("DESC"));

    // Entwürfe gehen an documents.drafts.
    let text = ruf(&server, "outline_list", json!({ "drafts": true }))
        .await
        .unwrap();
    assert!(text.contains("Eigene Entwürfe (1):"), "{text}");
    assert_eq!(mock.aufzeichnung.anzahl("documents.drafts"), 1);

    // Seite lesen, gekürzt.
    let text = ruf(
        &server,
        "outline_read",
        json!({ "id": "d1", "max_zeichen": 20 }),
    )
    .await
    .unwrap();
    assert!(text.contains("Titel: Betriebshandbuch"), "{text}");
    assert!(text.contains("Zeichen gekürzt"), "{text}");
    assert!(!text.contains("Zeile drei"), "{text}");
    assert_eq!(
        mock.aufzeichnung.gemerkt("documents.info").unwrap().rumpf,
        json!({ "id": "d1" })
    );

    // Kommentare, Antworten eingerückt und aus ProseMirror gelesen.
    let text = ruf(&server, "outline_comments", json!({ "document_id": "d1" }))
        .await
        .unwrap();
    assert!(text.contains("Kommentare an Dokument d1 (2):"), "{text}");
    assert!(text.contains("- Ada am 2026-09-02 10:11 (id k1)"), "{text}");
    assert!(text.contains("  - Bob am - (id k2)"), "{text}");
    assert!(text.contains("    Ja, seit gestern."), "{text}");

    // Der Zugang: aus dem Kern, ohne Geheimnis.
    let text = ruf(&server, "outline_profile", json!({})).await.unwrap();
    assert!(
        text.contains("Zugang für Outline-Wiki (aus der Umgebung)"),
        "{text}"
    );
    assert!(text.contains("- standard"), "{text}");
    assert!(text.contains(&mock.adresse), "{text}");
    assert!(!text.contains(SCHLUESSEL), "Schlüssel im Zugangsbericht");
}

#[tokio::test]
async fn schreibende_werkzeuge_senden_den_erwarteten_rumpf() {
    let mock = Attrappe::starte().await;
    let server = OutlineServer::neu(attrappe::zugang(&mock.adresse));

    // Anlegen.
    let text = ruf(
        &server,
        "outline_create",
        json!({
            "title": "Neue Seite",
            "text": "Inhalt",
            "collection_id": "c1"
        }),
    )
    .await
    .unwrap();
    assert!(
        text.starts_with("Dokument erstellt (veröffentlicht): Neue Seite"),
        "{text}"
    );
    let rumpf = mock.aufzeichnung.gemerkt("documents.create").unwrap().rumpf;
    assert_eq!(rumpf["title"], json!("Neue Seite"));
    assert_eq!(rumpf["text"], json!("Inhalt"));
    assert_eq!(rumpf["collectionId"], json!("c1"));
    assert_eq!(rumpf["publish"], json!(true));

    // Anhängen: erst lesen, dann den ganzen Text schicken.
    let text = ruf(
        &server,
        "outline_update",
        json!({ "id": "d1", "text": "Nachtrag", "mode": "append" }),
    )
    .await
    .unwrap();
    assert!(text.contains("Modus: append"), "{text}");
    let aenderungsrumpf = mock.aufzeichnung.gemerkt("documents.update").unwrap().rumpf;
    let neuer_text = aenderungsrumpf["text"].as_str().unwrap();
    assert!(neuer_text.starts_with("Zeile eins"), "{neuer_text}");
    assert!(neuer_text.ends_with("Nachtrag\n"), "{neuer_text}");

    // Verschieben an den Anfang der Sammlung: Sammlung kommt aus documents.info.
    let text = ruf(
        &server,
        "outline_move",
        json!({ "id": "d1", "to_top_level": true }),
    )
    .await
    .unwrap();
    assert!(text.contains("2 Dokument(e) betroffen"), "{text}");
    let verschieberumpf = mock.aufzeichnung.gemerkt("documents.move").unwrap().rumpf;
    assert_eq!(verschieberumpf["collectionId"], json!("c1"));
    assert!(verschieberumpf["parentDocumentId"].is_null());

    // Archivieren und zurückholen.
    let text = ruf(
        &server,
        "outline_archive",
        json!({ "id": "d1", "action": "restore" }),
    )
    .await
    .unwrap();
    assert!(text.contains("wiederhergestellt"), "{text}");
    assert_eq!(mock.aufzeichnung.anzahl("documents.restore"), 1);

    // Löschen in den Papierkorb: der Titel wird vorher geholt.
    let text = ruf(&server, "outline_delete", json!({ "id": "d1" }))
        .await
        .unwrap();
    assert!(text.contains("liegt jetzt im Papierkorb"), "{text}");
    let loeschrumpf = mock.aufzeichnung.gemerkt("documents.delete").unwrap().rumpf;
    assert_eq!(loeschrumpf["permanent"], json!(false));

    // Kommentieren: Text und ProseMirror gehen beide mit.
    let text = ruf(
        &server,
        "outline_comment",
        json!({ "document_id": "d1", "text": "Bitte prüfen" }),
    )
    .await
    .unwrap();
    assert!(
        text.contains("Kommentar an Dokument d1 geschrieben"),
        "{text}"
    );
    assert!(text.contains("Kommentar-Id: k9"), "{text}");
    let kommentarrumpf = mock.aufzeichnung.gemerkt("comments.create").unwrap().rumpf;
    assert_eq!(kommentarrumpf["text"], json!("Bitte prüfen"));
    assert_eq!(kommentarrumpf["data"]["type"], json!("doc"));
    assert!(kommentarrumpf.get("parentCommentId").is_none());

    // Kein Fehlertext und keine Antwort enthält den Schlüssel.
    for aufruf in mock.aufzeichnung.alle() {
        assert!(!aufruf.rumpf.to_string().contains(SCHLUESSEL));
    }
}

#[tokio::test]
async fn export_schreibt_markdown_dateien_in_den_arbeitsordner() {
    let mock = Attrappe::starte().await;
    let ziel = tempfile::tempdir().unwrap();
    let server = OutlineServer::neu(attrappe::zugang_mit_ordner(
        &mock.adresse,
        ziel.path().to_str().unwrap(),
    ));

    let text = ruf(
        &server,
        "outline_export",
        json!({ "target_dir": "wiki", "collection_id": "c1" }),
    )
    .await
    .unwrap();

    assert!(text.contains("2 Dokument(e) nach"), "{text}");
    assert_eq!(mock.aufzeichnung.anzahl("documents.export"), 2);
    let erste = std::fs::read_to_string(ziel.path().join("wiki/betriebshandbuch.md")).unwrap();
    assert!(erste.contains("Markdown aus der Attrappe"), "{erste}");
    assert!(ziel.path().join("wiki/notfall.md").exists());

    // Kein Überschreiben: ein zweiter Lauf legt nichts neu an und sagt es.
    let text = ruf(
        &server,
        "outline_export",
        json!({ "target_dir": "wiki", "collection_id": "c1" }),
    )
    .await
    .unwrap();
    assert!(text.contains("0 Dokument(e) nach"), "{text}");
    assert!(text.contains("nicht überschrieben"), "{text}");
    assert_eq!(
        std::fs::read_to_string(ziel.path().join("wiki/betriebshandbuch.md")).unwrap(),
        erste
    );

    // Ein absoluter Unterordner bricht nicht aus dem Arbeitsordner aus.
    let text = ruf(
        &server,
        "outline_export",
        json!({ "target_dir": "/etc", "collection_id": "c1" }),
    )
    .await
    .unwrap();
    assert!(text.contains("0 Dokument(e) nach"), "{text}");
    assert!(text.contains("außerhalb des Arbeitsordners"), "{text}");
}

#[tokio::test]
async fn ohne_arbeitsordner_wird_nichts_geschrieben() {
    let mock = Attrappe::starte().await;
    let server = OutlineServer::neu(attrappe::zugang(&mock.adresse));
    let fehler = ruf(&server, "outline_export", json!({ "collection_id": "c1" }))
        .await
        .unwrap_err();
    assert!(
        fehler.message.contains("Arbeitsordner"),
        "{}",
        fehler.message
    );
    assert_eq!(mock.aufzeichnung.alle().len(), 0);
}

#[tokio::test]
async fn ohne_schreibschalter_geht_keine_anfrage_hinaus() {
    let mock = Attrappe::starte().await;
    let server = OutlineServer::neu(attrappe::zugang_ohne_schreiben(&mock.adresse));

    // Lesen geht weiter.
    let text = ruf(&server, "outline_collections", json!({}))
        .await
        .unwrap();
    assert!(text.contains("Sammlungen"), "{text}");

    // Schreiben nicht – der Hinweis nennt das Feld im quiver-Fenster.
    let text = ruf(
        &server,
        "outline_create",
        json!({ "title": "Neu", "text": "x", "collection_id": "c1" }),
    )
    .await
    .unwrap();
    assert!(text.contains("Schreiben erlauben"), "{text}");
    assert!(text.contains("quiver-Fenster"), "{text}");
    assert!(text.contains("outline_create"), "{text}");
    assert_eq!(mock.aufzeichnung.anzahl("documents.create"), 0);
    assert_eq!(mock.aufzeichnung.anzahl("collections.list"), 1);
}

#[tokio::test]
async fn eine_seite_mit_kindern_exportieren() {
    let mock = Attrappe::starte().await;
    let ziel = tempfile::tempdir().unwrap();
    let server = OutlineServer::neu(attrappe::zugang_mit_ordner(
        &mock.adresse,
        ziel.path().to_str().unwrap(),
    ));

    let text = ruf(
        &server,
        "outline_export",
        json!({
            "document_id": "d1",
            "include_children": true
        }),
    )
    .await
    .unwrap();
    assert!(text.contains("2 Dokument(e)"), "{text}");
    assert_eq!(mock.aufzeichnung.anzahl("documents.export"), 2);
}

#[tokio::test]
async fn fehler_von_outline_nennen_status_und_nie_den_schluessel() {
    let mock = Attrappe::starte().await;
    // 403: Outline spiegelt den Kopf zurück – das Werkzeug gibt ihn nicht weiter.
    let server = OutlineServer::neu(attrappe::zugang(&format!("{}/kaputt", mock.adresse)));
    let fehler = ruf(&server, "outline_collections", json!({}))
        .await
        .unwrap_err();
    assert!(!fehler.message.contains(SCHLUESSEL), "{}", fehler.message);
    assert!(fehler.message.contains("[geheim]"), "{}", fehler.message);
    assert!(fehler.message.contains("403"), "{}", fehler.message);

    // 401: der abgelehnte Schlüssel kommt gar nicht erst in den Text.
    let server = OutlineServer::neu(attrappe::zugang(&format!("{}/abgelehnt", mock.adresse)));
    let fehler = ruf(&server, "outline_collections", json!({}))
        .await
        .unwrap_err();
    assert!(fehler.message.contains("401"), "{}", fehler.message);
    assert!(fehler.message.contains("abgelehnt"), "{}", fehler.message);
    assert!(!fehler.message.contains(SCHLUESSEL), "{}", fehler.message);

    // HTML statt JSON: die Adresse ist kein Outline.
    let server = OutlineServer::neu(attrappe::zugang(&format!("{}/nichts", mock.adresse)));
    let fehler = ruf(&server, "outline_collections", json!({}))
        .await
        .unwrap_err();
    assert!(
        fehler.message.contains("HTML statt JSON"),
        "{}",
        fehler.message
    );
    assert!(!fehler.message.contains(SCHLUESSEL));
}

#[tokio::test]
async fn ohne_zugang_kommt_der_hinweis_und_kein_fehler() {
    let server = OutlineServer::neu(attrappe::ohne_zugang());

    let text = ruf(&server, "outline_collections", json!({}))
        .await
        .unwrap();
    assert!(text.contains("Zugang fehlt"), "{text}");
    assert!(text.contains("Tools → Outline-Wiki"), "{text}");

    // Auch für schreibende Werkzeuge.
    let text = ruf(
        &server,
        "outline_create",
        json!({ "title": "X", "publish": false }),
    )
    .await
    .unwrap();
    assert!(text.contains("Zugang fehlt"), "{text}");

    // Und für die Suche.
    let text = ruf(&server, "outline_search", json!({ "query": "x" }))
        .await
        .unwrap();
    assert!(text.contains("Zugang fehlt"), "{text}");

    // Der Zugangsbericht sagt dasselbe, ohne Fehler.
    let text = ruf(&server, "outline_profile", json!({})).await.unwrap();
    assert!(text.contains("Zugang fehlt"), "{text}");
}

#[tokio::test]
async fn parameterfehler_kosten_kein_netz() {
    let mock = Attrappe::starte().await;
    let server = OutlineServer::neu(attrappe::zugang(&mock.adresse));

    for (werkzeug, argumente) in [
        ("outline_search", json!({ "query": "   " })),
        (
            "outline_search",
            json!({ "query": "x", "status_filter": ["quatsch"] }),
        ),
        (
            "outline_search",
            json!({ "query": "x", "date_filter": "jahrzehnt" }),
        ),
        ("outline_list", json!({ "sort": "quatsch" })),
        ("outline_list", json!({ "direction": "seitwärts" })),
        ("outline_structure", json!({ "collection_id": " " })),
        ("outline_read", json!({ "id": "" })),
        ("outline_update", json!({ "id": "d1" })),
        (
            "outline_update",
            json!({ "id": "d1", "mode": "loeschen", "text": "x" }),
        ),
        ("outline_create", json!({ "title": "X" })),
        ("outline_move", json!({ "id": "d1" })),
        (
            "outline_move",
            json!({ "id": "d1", "to_top_level": true, "parent_document_id": "d2" }),
        ),
        (
            "outline_archive",
            json!({ "id": "d1", "action": "verbrennen" }),
        ),
        (
            "outline_comment",
            json!({ "document_id": "d1", "text": "  " }),
        ),
    ] {
        let fehler = ruf(&server, werkzeug, argumente.clone()).await.unwrap_err();
        assert!(
            !fehler.message.is_empty(),
            "{werkzeug} {argumente} hat keinen Fehler geliefert"
        );
    }
    assert!(mock.aufzeichnung.alle().is_empty(), "es ging doch ins Netz");
}

#[tokio::test]
async fn saemtliche_texte_bleiben_ohne_schluessel() {
    let mock = Attrappe::starte().await;
    let server = OutlineServer::neu(attrappe::zugang(&mock.adresse));

    let texte = [
        ruf(&server, "outline_collections", json!({}))
            .await
            .unwrap(),
        ruf(&server, "outline_search", json!({ "query": "x" }))
            .await
            .unwrap(),
        ruf(&server, "outline_list", json!({})).await.unwrap(),
        ruf(&server, "outline_read", json!({ "id": "d1" }))
            .await
            .unwrap(),
        ruf(&server, "outline_comments", json!({ "document_id": "d1" }))
            .await
            .unwrap(),
        ruf(&server, "outline_profile", json!({})).await.unwrap(),
    ];
    for text in texte {
        assert!(!text.contains(SCHLUESSEL), "Schlüssel im Text: {text}");
    }
}
