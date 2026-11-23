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
    let ordner = tempfile::tempdir().unwrap();
    let (profile, _tresor) = attrappe::speicher(ordner.path(), &[("arbeit", &mock.adresse)]);
    let server = OutlineServer::neu(profile);

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

    // Profile: aus dem Kern, ohne Geheimnis.
    let text = ruf(&server, "outline_profile", json!({})).await.unwrap();
    assert!(text.contains("- arbeit (Standard)"), "{text}");
    assert!(text.contains(&mock.adresse), "{text}");
    assert!(!text.contains(SCHLUESSEL), "Schlüssel in der Profilliste");

    // Auch die Profilliste nennt den Schlüssel nicht.
    assert!(!text.contains(SCHLUESSEL));
}

#[tokio::test]
async fn schreibende_werkzeuge_senden_den_erwarteten_rumpf() {
    let mock = Attrappe::starte().await;
    let ordner = tempfile::tempdir().unwrap();
    let (profile, _tresor) = attrappe::speicher(ordner.path(), &[("arbeit", &mock.adresse)]);
    let server = OutlineServer::neu(profile);

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
async fn export_schreibt_markdown_dateien() {
    let mock = Attrappe::starte().await;
    let ordner = tempfile::tempdir().unwrap();
    let (profile, _tresor) = attrappe::speicher(ordner.path(), &[("arbeit", &mock.adresse)]);
    let server = OutlineServer::neu(profile);
    let ziel = tempfile::tempdir().unwrap();
    let ziel_pfad = ziel.path().join("wiki");

    let text = ruf(
        &server,
        "outline_export",
        json!({
            "target_dir": ziel_pfad.to_str().unwrap(),
            "collection_id": "c1"
        }),
    )
    .await
    .unwrap();

    assert!(text.contains("2 Dokument(e) nach"), "{text}");
    assert_eq!(mock.aufzeichnung.anzahl("documents.export"), 2);
    let erste = std::fs::read_to_string(ziel_pfad.join("betriebshandbuch.md")).unwrap();
    assert!(erste.contains("Markdown aus der Attrappe"), "{erste}");
    assert!(ziel_pfad.join("notfall.md").exists());

    // Ein Dokument ohne Ziel ist ein Parameterfehler – und kostet kein Netz.
    let vorher = mock.aufzeichnung.alle().len();
    let fehler = ruf(
        &server,
        "outline_export",
        json!({ "target_dir": ziel_pfad.to_str().unwrap() }),
    )
    .await
    .unwrap_err();
    assert!(fehler.message.contains("document_id"), "{}", fehler.message);
    let fehler = ruf(
        &server,
        "outline_export",
        json!({
            "target_dir": ziel_pfad.to_str().unwrap(),
            "document_id": "d1",
            "collection_id": "c1"
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(mock.aufzeichnung.alle().len(), vorher);
    assert!(fehler.message.contains("schließen einander aus"));
}

#[tokio::test]
async fn eine_seite_mit_kindern_exportieren() {
    let mock = Attrappe::starte().await;
    let ordner = tempfile::tempdir().unwrap();
    let (profile, _tresor) = attrappe::speicher(ordner.path(), &[("arbeit", &mock.adresse)]);
    let server = OutlineServer::neu(profile);
    let ziel = tempfile::tempdir().unwrap();

    let text = ruf(
        &server,
        "outline_export",
        json!({
            "target_dir": ziel.path().to_str().unwrap(),
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
    let ordner = tempfile::tempdir().unwrap();

    // 403: Outline spiegelt den Kopf zurück – das Werkzeug gibt ihn nicht weiter.
    let (profile, _tresor) = attrappe::speicher(
        ordner.path(),
        &[("kaputt", &format!("{}/kaputt", mock.adresse))],
    );
    let server = OutlineServer::neu(profile);
    let fehler = ruf(&server, "outline_collections", json!({}))
        .await
        .unwrap_err();
    assert!(!fehler.message.contains(SCHLUESSEL), "{}", fehler.message);
    assert!(fehler.message.contains("[geheim]"), "{}", fehler.message);
    assert!(fehler.message.contains("403"), "{}", fehler.message);

    // 401: der abgelehnte Schlüssel kommt gar nicht erst in den Text.
    let dritter = tempfile::tempdir().unwrap();
    let (profile, _tresor) = attrappe::speicher(
        dritter.path(),
        &[("abgelehnt", &format!("{}/abgelehnt", mock.adresse))],
    );
    let server = OutlineServer::neu(profile);
    let fehler = ruf(&server, "outline_collections", json!({}))
        .await
        .unwrap_err();
    assert!(fehler.message.contains("401"), "{}", fehler.message);
    assert!(fehler.message.contains("abgelehnt"), "{}", fehler.message);
    assert!(!fehler.message.contains(SCHLUESSEL), "{}", fehler.message);

    // HTML statt JSON: die Adresse ist kein Outline.
    let zweiter = tempfile::tempdir().unwrap();
    let (profile, _tresor) = attrappe::speicher(
        zweiter.path(),
        &[("falsch", &format!("{}/nichts", mock.adresse))],
    );
    let server = OutlineServer::neu(profile);
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
async fn ohne_profil_kommt_der_hinweis_und_kein_fehler() {
    let ordner = tempfile::tempdir().unwrap();
    let (profile, _tresor) = attrappe::speicher(ordner.path(), &[]);
    let server = OutlineServer::neu(profile);

    let text = ruf(&server, "outline_collections", json!({}))
        .await
        .unwrap();
    assert!(text.contains("Noch kein Zugang eingerichtet"), "{text}");
    assert!(text.contains("Outline-Wiki"), "{text}");

    // Auch für schreibende Werkzeuge und für die Suche über alle Profile.
    let text = ruf(
        &server,
        "outline_create",
        json!({ "title": "X", "publish": false }),
    )
    .await
    .unwrap();
    assert!(text.contains("Noch kein Zugang eingerichtet"), "{text}");
    let text = ruf(
        &server,
        "outline_search",
        json!({ "query": "x", "alle_profile": true }),
    )
    .await
    .unwrap();
    assert!(text.contains("Noch kein Zugang eingerichtet"), "{text}");
}

#[tokio::test]
async fn das_profil_waehlt_das_wiki() {
    let mock = Attrappe::starte().await;
    let ordner = tempfile::tempdir().unwrap();
    let zweig = format!("{}/zweig", mock.adresse);
    let (profile, _tresor) = attrappe::speicher(
        ordner.path(),
        &[("arbeit", &mock.adresse), ("kunde", &zweig)],
    );
    let server = OutlineServer::neu(profile);

    ruf(&server, "outline_collections", json!({ "profil": "kunde" }))
        .await
        .unwrap();
    assert_eq!(
        mock.aufzeichnung.alle().last().unwrap().pfad,
        "/zweig/api/collections.list"
    );

    let fehler = ruf(
        &server,
        "outline_collections",
        json!({ "profil": "gibtsnicht" }),
    )
    .await
    .unwrap_err();
    assert!(
        fehler.message.contains("gibt es nicht"),
        "{}",
        fehler.message
    );
}

#[tokio::test]
async fn alle_profile_durchsucht_jedes_wiki() {
    let mock = Attrappe::starte().await;
    let ordner = tempfile::tempdir().unwrap();
    let zweig = format!("{}/zweig", mock.adresse);
    let (profile, _tresor) = attrappe::speicher(
        ordner.path(),
        &[("arbeit", &mock.adresse), ("kunde", &zweig)],
    );
    let server = OutlineServer::neu(profile);

    let text = ruf(
        &server,
        "outline_search",
        json!({ "query": "handbuch", "alle_profile": true }),
    )
    .await
    .unwrap();
    assert!(text.contains("über 2 Profile – 2 Treffer"), "{text}");
    assert!(text.contains("[arbeit] 1 Treffer:"), "{text}");
    assert!(text.contains("[kunde] 1 Treffer:"), "{text}");
    assert!(text.contains("profil arbeit"), "{text}");
    assert_eq!(mock.aufzeichnung.anzahl("documents.search"), 2);

    // Widersprüchliche Parameter werden abgewiesen.
    let fehler = ruf(
        &server,
        "outline_search",
        json!({ "query": "x", "alle_profile": true, "collection_id": "c1" }),
    )
    .await
    .unwrap_err();
    assert!(
        fehler.message.contains("passen nicht zu `alle_profile`"),
        "{}",
        fehler.message
    );
}

#[tokio::test]
async fn parameterfehler_kosten_kein_netz() {
    let mock = Attrappe::starte().await;
    let ordner = tempfile::tempdir().unwrap();
    let (profile, _tresor) = attrappe::speicher(ordner.path(), &[("arbeit", &mock.adresse)]);
    let server = OutlineServer::neu(profile);

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
    let ordner = tempfile::tempdir().unwrap();
    let (profile, _tresor) = attrappe::speicher(ordner.path(), &[("arbeit", &mock.adresse)]);
    let server = OutlineServer::neu(profile);

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
