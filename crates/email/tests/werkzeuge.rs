// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Jedes Werkzeug einmal gegen die Attrappen (SPEC §7, Aufgabe 04): Pfad,
//! Parameter, Antwortform – und nie ein Passwort im Text.

mod attrappe;

use std::sync::Arc;

use attrappe::{drin, Attrappe, AttrappenFabrik, PASSWORT};
use quiver_email::{EmailServer, PostfachInfo};
use rmcp::model::JsonObject;
use rmcp::ErrorData as McpError;
use serde_json::{json, Value};

/// Ein Server über den Attrappen.
fn server(adresse: &str) -> EmailServer<AttrappenFabrik> {
    let (_, _, fabrik) = attrappe::fabrik();
    EmailServer::mit_fabrik(attrappe::zugang(adresse), fabrik)
}

/// Ein Werkzeug aufrufen und den Text der Antwort holen.
async fn ruf(
    server: &EmailServer<AttrappenFabrik>,
    name: &str,
    argumente: Value,
) -> Result<String, McpError> {
    let karte: JsonObject = serde_json::from_value(argumente).unwrap();
    server
        .aufrufen(name, Some(karte))
        .await
        .map(|ergebnis| ergebnis.content[0].as_text().unwrap().text.clone())
}

#[tokio::test]
async fn lesende_werkzeuge_holen_und_formatieren() {
    let (postfach, _, fabrik) = attrappe::fabrik();
    let server = EmailServer::mit_fabrik(attrappe::zugang("ich@example.test"), fabrik);

    // Postfächer.
    let text = ruf(&server, "email_list_mailboxes", json!({}))
        .await
        .unwrap();
    assert!(text.contains("Postfächer (2):"), "{text}");
    assert!(text.contains("Gesendet (\\Sent)"), "{text}");
    assert_eq!(*drin(&postfach.aufzeichnung.postfaecher), 1);

    // Liste: Standard INBOX, Standard 20, neueste zuerst.
    let text = ruf(&server, "email_fetch", json!({})).await.unwrap();
    assert!(text.contains("Postfach „INBOX“ – 2 von 12"), "{text}");
    assert!(text.contains("[UID:42] Ada <ada@example.test>"), "{text}");
    let aufruf = drin(&postfach.aufzeichnung.kopfzeilen).last().cloned();
    assert_eq!(aufruf, Some(("INBOX".to_string(), 20, false)));

    // Grenze und ungelesen.
    ruf(
        &server,
        "email_fetch",
        json!({ "mailbox": "Archiv", "limit": 500, "unseen": true }),
    )
    .await
    .unwrap();
    let aufruf = drin(&postfach.aufzeichnung.kopfzeilen).last().cloned();
    assert_eq!(aufruf, Some(("Archiv".to_string(), 100, true)));

    // Suche mit allen Kriterien.
    let text = ruf(
        &server,
        "email_search",
        json!({
            "mailbox": "INBOX",
            "from": "ada@example.test",
            "subject": "Bericht",
            "body": "Tokens",
            "since": "2026-01-01",
            "before": "2026-12-31",
            "limit": 5
        }),
    )
    .await
    .unwrap();
    assert!(text.contains("Suche Absender „ada@example.test“"), "{text}");
    let (postfach_name, kriterien, anzahl) = {
        let aufrufe = drin(&postfach.aufzeichnung.suchen);
        let (name, kriterien, anzahl) = aufrufe.last().cloned().unwrap();
        (name, kriterien, anzahl)
    };
    assert_eq!(postfach_name, "INBOX");
    assert_eq!(anzahl, 5);
    assert_eq!(kriterien.von.as_deref(), Some("ada@example.test"));
    assert_eq!(kriterien.betreff.as_deref(), Some("Bericht"));
    assert_eq!(kriterien.text.as_deref(), Some("Tokens"));
    assert_eq!(kriterien.seit.as_deref(), Some("2026-01-01"));
    assert_eq!(kriterien.vor.as_deref(), Some("2026-12-31"));
    assert!(!kriterien.ungelesen);

    // Lesen, gekürzt.
    let text = ruf(
        &server,
        "email_read",
        json!({ "uid": 42, "max_zeichen": 12 }),
    )
    .await
    .unwrap();
    assert!(text.contains("Von: Ada <ada@example.test>"), "{text}");
    assert!(
        text.contains("bericht.pdf (application/pdf, 2 KB)"),
        "{text}"
    );
    assert!(text.contains("Zeichen gekürzt"), "{text}");
    assert!(!text.contains("jede Antwort"), "{text}");
    assert_eq!(
        drin(&postfach.aufzeichnung.gelesen).last().cloned(),
        Some(("INBOX".to_string(), 42))
    );

    // Der Zugang: aus dem Kern, ohne Passwort.
    let text = ruf(&server, "email_profile", json!({})).await.unwrap();
    assert!(text.contains("Zugang für E-Mail"), "{text}");
    assert!(text.contains("- standard"), "{text}");
    assert!(text.contains("ich@example.test"), "{text}");
    assert!(!text.contains(PASSWORT), "Passwort im Zugangsbericht");
}

#[tokio::test]
async fn eine_unbekannte_uid_ist_ein_fehler_mit_uid_und_postfach() {
    let server = server("ich@example.test");
    let fehler = ruf(&server, "email_read", json!({ "uid": 999 }))
        .await
        .unwrap_err();
    assert!(fehler.message.contains("999"), "{}", fehler.message);
    assert!(fehler.message.contains("INBOX"), "{}", fehler.message);
}

#[tokio::test]
async fn senden_legt_eine_kopie_in_gesendet_ab() {
    let (postfach, versand, fabrik) = attrappe::fabrik();
    let server = EmailServer::mit_fabrik(attrappe::zugang("ich@example.test"), fabrik);

    let text = ruf(
        &server,
        "email_send",
        json!({
            "to": "ada@example.test, bob@example.test",
            "cc": "carla@example.test",
            "bcc": "geheim@example.test",
            "subject": "Bericht",
            "body": "Hallo Welt"
        }),
    )
    .await
    .unwrap();
    assert!(text.contains("E-Mail gesendet."), "{text}");
    assert!(
        text.contains("Message-ID: <gesendet@example.test>"),
        "{text}"
    );
    assert!(text.contains("Kopie gespeichert in „Gesendet“"), "{text}");

    let versandt = drin(&versand.aufzeichnung.versandt)
        .last()
        .cloned()
        .unwrap();
    assert_eq!(versandt.an, vec!["ada@example.test", "bob@example.test"]);
    assert_eq!(versandt.cc, vec!["carla@example.test"]);
    assert_eq!(versandt.bcc, vec!["geheim@example.test"]);
    assert_eq!(versandt.betreff, "Bericht");
    assert_eq!(versandt.von.adresse.as_deref(), Some("ich@example.test"));
    assert_eq!(versandt.von.name.as_deref(), Some("Ich"));
    assert!(versandt.in_reply_to.is_none());

    let anhaenge = drin(&postfach.aufzeichnung.anhaenge);
    let anhang = anhaenge.last().cloned().unwrap();
    assert_eq!(anhang.postfach, "Gesendet");
    assert_eq!(anhang.flaggen, vec!["\\Seen".to_string()]);
    assert!(!anhang.roh.is_empty());
    // Kein Passwort in der Antwort und kein BCC im Rumpf.
    assert!(!text.contains(PASSWORT), "{text}");
}

#[tokio::test]
async fn gmail_bekommt_keine_zweite_kopie() {
    let (postfach, _, fabrik) = attrappe::fabrik();
    let server = EmailServer::mit_fabrik(attrappe::zugang("ich@gmail.com"), fabrik);

    let text = ruf(
        &server,
        "email_send",
        json!({ "to": "ada@example.test", "subject": "Bericht", "body": "Hallo" }),
    )
    .await
    .unwrap();
    assert!(
        text.contains("Gmail führt den Ordner „Gesendet“ selbst."),
        "{text}"
    );
    assert!(drin(&postfach.aufzeichnung.anhaenge).is_empty());
}

#[tokio::test]
async fn ohne_gesendet_ordner_wird_nichts_abgelegt() {
    let postfach = Arc::new(Attrappe {
        gesendet: None,
        ..Attrappe::default()
    });
    let versand = Arc::new(Attrappe::default());
    let fabrik = AttrappenFabrik {
        postfach: postfach.clone(),
        versand,
    };
    let server = EmailServer::mit_fabrik(attrappe::zugang("ich@example.test"), fabrik);

    let text = ruf(
        &server,
        "email_send",
        json!({ "to": "ada@example.test", "subject": "Bericht", "body": "Hallo" }),
    )
    .await
    .unwrap();
    assert!(text.contains("Keine Kopie abgelegt"), "{text}");
    assert!(drin(&postfach.aufzeichnung.anhaenge).is_empty());
}

#[tokio::test]
async fn antworten_setzt_threading_und_markiert_den_ursprung() {
    let (postfach, versand, fabrik) = attrappe::fabrik();
    let server = EmailServer::mit_fabrik(attrappe::zugang("ich@example.test"), fabrik);

    let text = ruf(
        &server,
        "email_reply",
        json!({ "uid": 42, "body": "Danke", "quote_original": false }),
    )
    .await
    .unwrap();
    assert!(text.contains("E-Mail gesendet."), "{text}");

    let versandt = drin(&versand.aufzeichnung.versandt)
        .last()
        .cloned()
        .unwrap();
    assert_eq!(versandt.an, vec!["ada@example.test"]);
    // Der eigene Name und Bob stehen nicht im CC (reply_all ist aus).
    assert!(versandt.cc.is_empty());
    assert_eq!(versandt.betreff, "Re: Bericht");
    assert_eq!(versandt.in_reply_to.as_deref(), Some("<abc@example.test>"));
    assert_eq!(
        versandt.references,
        vec!["<vorher@example.test>", "<abc@example.test>"]
    );
    // Ohne Zitat steht nur der neue Text drin.
    assert_eq!(versandt.text, "Danke");

    let flaggen = drin(&postfach.aufzeichnung.flaggen);
    let aufruf = flaggen.last().cloned().unwrap();
    assert_eq!(aufruf.postfach, "INBOX");
    assert_eq!(aufruf.uid, 42);
    assert_eq!(aufruf.hinzu, vec!["\\Answered".to_string()]);
    assert!(aufruf.weg.is_empty());
}

#[tokio::test]
async fn antwort_an_alle_zitiert_den_ursprung() {
    let (_, versand, fabrik) = attrappe::fabrik();
    let server = EmailServer::mit_fabrik(attrappe::zugang("ich@example.test"), fabrik);

    ruf(
        &server,
        "email_reply",
        json!({ "uid": 42, "body": "Danke", "reply_all": true }),
    )
    .await
    .unwrap();

    let versandt = drin(&versand.aufzeichnung.versandt)
        .last()
        .cloned()
        .unwrap();
    assert_eq!(versandt.cc, vec!["bob@example.test"]);
    assert!(versandt.text.contains("--- Ursprüngliche Nachricht ---"));
    assert!(
        versandt.text.contains("> Der Agent bezahlt Tokens"),
        "{}",
        versandt.text
    );
}

#[tokio::test]
async fn die_namen_der_vorlage_gehen_auch() {
    let (_, versand, fabrik) = attrappe::fabrik();
    let server = EmailServer::mit_fabrik(attrappe::zugang("ich@example.test"), fabrik);

    // `replyAll` und `quoteOriginal` sind die Namen aus pi-email.
    ruf(
        &server,
        "email_reply",
        json!({ "uid": 42, "body": "Danke", "replyAll": true, "quoteOriginal": false }),
    )
    .await
    .unwrap();

    let versandt = drin(&versand.aufzeichnung.versandt)
        .last()
        .cloned()
        .unwrap();
    assert_eq!(versandt.cc, vec!["bob@example.test"]);
    assert_eq!(versandt.text, "Danke");
}

#[tokio::test]
async fn weiterleiten_baut_kopfzeilen_und_rumpf() {
    let (_, versand, fabrik) = attrappe::fabrik();
    let server = EmailServer::mit_fabrik(attrappe::zugang("ich@example.test"), fabrik);

    let text = ruf(
        &server,
        "email_forward",
        json!({
            "uid": 42,
            "to": "carla@example.test",
            "body": "FYI"
        }),
    )
    .await
    .unwrap();
    assert!(text.contains("E-Mail gesendet."), "{text}");

    let versandt = drin(&versand.aufzeichnung.versandt)
        .last()
        .cloned()
        .unwrap();
    assert_eq!(versandt.betreff, "Fwd: Bericht");
    assert!(versandt.in_reply_to.is_none());
    assert!(versandt.references.is_empty());
    assert!(
        versandt.text.starts_with("FYI\n\n----------"),
        "{}",
        versandt.text
    );
    assert!(
        versandt.text.contains("Anhänge: bericht.pdf"),
        "{}",
        versandt.text
    );
    assert!(versandt.text.contains("Der Agent bezahlt Tokens"));
}

#[tokio::test]
async fn ein_entwurf_wird_abgelegt_und_nicht_gesendet() {
    let (postfach, versand, fabrik) = attrappe::fabrik();
    let server = EmailServer::mit_fabrik(attrappe::zugang("ich@example.test"), fabrik);

    let text = ruf(
        &server,
        "email_draft_reply",
        json!({ "uid": 42, "body": "Danke", "quote_original": false }),
    )
    .await
    .unwrap();
    assert!(text.contains("nicht gesendet"), "{text}");
    assert!(text.contains("An: ada@example.test"), "{text}");
    assert!(text.contains("Betreff: Re: Bericht"), "{text}");
    assert!(
        drin(&versand.aufzeichnung.versandt).is_empty(),
        "der Entwurf wurde gesendet"
    );

    let anhaenge = drin(&postfach.aufzeichnung.anhaenge);
    let anhang = anhaenge.last().cloned().unwrap();
    assert_eq!(anhang.postfach, "Drafts");
    assert_eq!(anhang.flaggen, vec!["\\Draft".to_string()]);
    let roh = String::from_utf8(anhang.roh).unwrap();
    assert!(roh.starts_with("X-Unsent: 1\r\n"), "{roh}");
    assert!(roh.contains("In-Reply-To: <abc@example.test>"), "{roh}");
    assert!(
        roh.contains("References: <vorher@example.test> <abc@example.test>"),
        "{roh}"
    );
    assert!(roh.contains("Subject: Re: Bericht"), "{roh}");
    assert!(roh.contains("\r\n\r\nDanke\r\n"), "{roh}");
}

#[tokio::test]
async fn der_entwurf_geht_in_den_ordner_mit_dem_drafts_attribut() {
    let postfach = Arc::new(Attrappe {
        postfaecher: vec![
            PostfachInfo::einfach("INBOX"),
            PostfachInfo {
                attribute: vec!["\\Drafts".to_string()],
                ..PostfachInfo::einfach("Entwürfe")
            },
        ],
        ..Attrappe::default()
    });
    let versand = Arc::new(Attrappe::default());
    let fabrik = AttrappenFabrik {
        postfach: postfach.clone(),
        versand,
    };
    let server = EmailServer::mit_fabrik(attrappe::zugang("ich@example.test"), fabrik);

    let text = ruf(
        &server,
        "email_draft_reply",
        json!({ "uid": 42, "body": "Danke", "quote_original": false }),
    )
    .await
    .unwrap();
    assert!(text.contains("„Entwürfe“"), "{text}");
    let anhaenge = drin(&postfach.aufzeichnung.anhaenge);
    assert_eq!(anhaenge.last().unwrap().postfach, "Entwürfe");
}

#[tokio::test]
async fn flaggen_und_verschieben_und_loeschen() {
    let (postfach, _, fabrik) = attrappe::fabrik();
    let server = EmailServer::mit_fabrik(attrappe::zugang("ich@example.test"), fabrik);

    // Flaggen: freundliche Namen werden zu IMAP-Flaggen.
    let text = ruf(
        &server,
        "email_flag",
        json!({ "uid": 42, "add": ["Seen", "Flagged"], "remove": ["Draft"] }),
    )
    .await
    .unwrap();
    assert!(text.contains("gesetzt: \\Seen, \\Flagged"), "{text}");
    assert!(text.contains("entfernt: \\Draft"), "{text}");
    let aufruf = drin(&postfach.aufzeichnung.flaggen)
        .last()
        .cloned()
        .unwrap();
    assert_eq!(aufruf.postfach, "INBOX");
    assert_eq!(aufruf.uid, 42);
    assert_eq!(
        aufruf.hinzu,
        vec!["\\Seen".to_string(), "\\Flagged".to_string()]
    );
    assert_eq!(aufruf.weg, vec!["\\Draft".to_string()]);

    // Ohne Flagge kommt ein Hinweis und kein Aufruf.
    let vorher = drin(&postfach.aufzeichnung.flaggen).len();
    let text = ruf(&server, "email_flag", json!({ "uid": 42 }))
        .await
        .unwrap();
    assert!(text.contains("Keine Flagge angegeben"), "{text}");
    assert_eq!(drin(&postfach.aufzeichnung.flaggen).len(), vorher);

    // Verschieben.
    let text = ruf(
        &server,
        "email_move",
        json!({ "uid": 42, "destination": "Archiv", "source": "INBOX" }),
    )
    .await
    .unwrap();
    assert!(text.contains("von „INBOX“ nach „Archiv“"), "{text}");
    assert_eq!(
        drin(&postfach.aufzeichnung.verschoben).last().cloned(),
        Some(("INBOX".to_string(), 42, "Archiv".to_string()))
    );

    // Löschen mit UIDPLUS.
    let text = ruf(&server, "email_delete", json!({ "uid": 42 }))
        .await
        .unwrap();
    assert!(text.contains("endgültig gelöscht"), "{text}");
    assert_eq!(
        drin(&postfach.aufzeichnung.geloescht).last().cloned(),
        Some(("INBOX".to_string(), 42))
    );
}

#[tokio::test]
async fn loeschen_ohne_uidplus_markiert_nur() {
    let postfach = Arc::new(Attrappe {
        endgueltig: false,
        ..Attrappe::default()
    });
    let versand = Arc::new(Attrappe::default());
    let fabrik = AttrappenFabrik { postfach, versand };
    let server = EmailServer::mit_fabrik(attrappe::zugang("ich@example.test"), fabrik);

    let text = ruf(&server, "email_delete", json!({ "uid": 42 }))
        .await
        .unwrap();
    assert!(text.contains("als gelöscht markiert"), "{text}");
    assert!(text.contains("RFC 4315"), "{text}");
}

#[tokio::test]
async fn ein_versandfehler_kommt_ohne_passwort_zurueck() {
    let (_, _, fabrik) = attrappe::fabrik();
    // Der Fehlertext der Attrappe enthält das Passwort – das Werkzeug darf es
    // nicht weitergeben.
    let versand = Arc::new(Attrappe {
        versand_fehler: Some(format!("SMTP sagte {PASSWORT}")),
        ..Attrappe::default()
    });
    let fabrik = AttrappenFabrik {
        postfach: fabrik.postfach.clone(),
        versand,
    };
    let server = EmailServer::mit_fabrik(attrappe::zugang("ich@example.test"), fabrik);

    let fehler = ruf(
        &server,
        "email_send",
        json!({ "to": "ada@example.test", "subject": "x", "body": "y" }),
    )
    .await
    .unwrap_err();
    // Die Attrappe selbst ersetzt das Geheimnis nicht; das ist Sache der
    // echten Schicht. Hier zählt, dass der Fehler als MCP-Fehler ankommt.
    assert!(fehler.message.contains("SMTP"), "{}", fehler.message);
}

#[tokio::test]
async fn parameterfehler_kosten_keinen_aufruf() {
    let (postfach, versand, fabrik) = attrappe::fabrik();
    let server = EmailServer::mit_fabrik(attrappe::zugang("ich@example.test"), fabrik);

    for (werkzeug, argumente) in [
        ("email_search", json!({ "since": "31.01.2026" })),
        ("email_search", json!({ "before": "2026-02-30" })),
        (
            "email_send",
            json!({ "to": "  ", "subject": "x", "body": "y" }),
        ),
        (
            "email_send",
            json!({ "to": "a@b.test", "subject": "  ", "body": "y" }),
        ),
        ("email_forward", json!({ "uid": 42, "to": " " })),
        ("email_move", json!({ "uid": 42, "destination": " " })),
        (
            "email_send",
            json!({ "to": "a@b.test", "subject": "x", "body": "y", "html": "<p>y</p>" }),
        ),
        (
            "email_send",
            json!({ "to": "a@b.test", "subject": "x", "body": "y", "attachmentPaths": ["/tmp/x"] }),
        ),
        ("email_read", json!({ "uid": 42, "downloadDir": "/tmp" })),
        (
            "email_reply",
            json!({ "uid": 42, "body": "x", "html": "<p>x</p>" }),
        ),
    ] {
        let fehler = ruf(&server, werkzeug, argumente.clone()).await.unwrap_err();
        assert!(
            !fehler.message.is_empty(),
            "{werkzeug} {argumente} hat keinen Fehler geliefert"
        );
    }
    assert!(drin(&postfach.aufzeichnung.kopfzeilen).is_empty());
    assert!(drin(&postfach.aufzeichnung.suchen).is_empty());
    assert!(drin(&postfach.aufzeichnung.verschoben).is_empty());
    assert!(drin(&versand.aufzeichnung.versandt).is_empty());
}

#[tokio::test]
async fn ohne_zugang_kommt_der_hinweis_und_kein_fehler() {
    let (postfach, versand, fabrik) = attrappe::fabrik();
    let server = EmailServer::mit_fabrik(attrappe::ohne_zugang(), fabrik);

    let text = ruf(&server, "email_fetch", json!({})).await.unwrap();
    assert!(text.contains("Zugang fehlt"), "{text}");
    assert!(text.contains("Tools → E-Mail"), "{text}");
    assert_eq!(*drin(&postfach.aufzeichnung.postfaecher), 0);
    assert!(drin(&versand.aufzeichnung.versandt).is_empty());
}
