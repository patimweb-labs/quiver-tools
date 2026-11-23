// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Ende-zu-Ende: `quiver-outline` als **Kindprozess** über stdio (SPEC §7).
//!
//! So startet quiver das Programm wirklich. Der Prozess bekommt einen eigenen
//! Profilordner (`QUIVER_TOOLS_DIR`) und den Tresor im Arbeitsspeicher – kein
//! Schlüsselbund, keine echten Zugänge.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use quiver_outline::Outline;
use quiver_tools_kern::{Profil, Profile, SpeicherTresor};
use serde_json::{json, Value};

/// Wie lange auf eine Antwort des Kindprozesses gewartet wird.
const GEDULD: Duration = Duration::from_secs(30);

/// Der Profilordner, in dem das Kind seine Profile findet.
fn profilordner(ordner: &std::path::Path) -> Profile {
    Profile::im_ordner(
        &Outline,
        std::sync::Arc::new(SpeicherTresor),
        ordner.join("outline"),
    )
}

#[test]
fn kindprozess_spricht_mcp_ueber_stdio() {
    let ordner = tempfile::tempdir().unwrap();

    // Profil vorher über die Kern-API anlegen. Der Schlüssel bleibt im Tresor
    // dieses Prozesses und erreicht das Kind nie – für `list_tools` und
    // `outline_profile` braucht es auch keinen.
    let profile = profilordner(ordner.path());
    let mut eintrag = Profil::leer("arbeit");
    eintrag.beschreibung = "Wiki im Büro".to_string();
    eintrag
        .werte
        .insert("url".to_string(), "https://wiki.example.test".to_string());
    eintrag
        .geheim
        .insert("api_key".to_string(), "ol_api_nur_im_test_4711".to_string());
    profile.speichern(&eintrag).unwrap();

    let mut kind = Command::new(env!("CARGO_BIN_EXE_quiver-outline"))
        .env("QUIVER_TOOLS_TRESOR", "speicher")
        .env("QUIVER_TOOLS_DIR", ordner.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("quiver-outline startet nicht");

    let mut stdin = kind.stdin.take().unwrap();
    let stdout = kind.stdout.take().unwrap();

    // Zeilen aus stdout in einen Kanal: so kann der Test auf eine Antwort
    // warten, statt bei einem Fehler ewig zu hängen.
    let (sender, empfänger) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for zeile in BufReader::new(stdout).lines().map_while(Result::ok) {
            if sender.send(zeile).is_err() {
                break;
            }
        }
    });

    let mut sende = |wert: Value| {
        writeln!(stdin, "{wert}").unwrap();
        stdin.flush().unwrap();
    };
    let antwort = |id: u64| -> Value {
        loop {
            let zeile = empfänger
                .recv_timeout(GEDULD)
                .unwrap_or_else(|_| panic!("keine Antwort auf Anfrage {id}"));
            let Ok(wert) = serde_json::from_str::<Value>(&zeile) else {
                continue;
            };
            if wert.get("id").and_then(Value::as_u64) == Some(id) {
                return wert;
            }
        }
    };

    // 1. Handschlag.
    sende(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "quiver-test", "version": "0" }
        }
    }));
    let handschlag = antwort(1);
    assert_eq!(
        handschlag["result"]["serverInfo"]["name"],
        json!("quiver-outline")
    );
    assert_eq!(
        handschlag["result"]["serverInfo"]["title"],
        json!("Outline-Wiki")
    );
    assert!(
        handschlag["result"]["capabilities"]["tools"].is_object(),
        "Werkzeuge fehlen in den Fähigkeiten: {handschlag}"
    );

    sende(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));

    // 2. tools/list ist vollständig und trägt die richtigen Hinweise.
    sende(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    }));
    let liste = antwort(2);
    let werkzeuge = liste["result"]["tools"].as_array().expect("tools fehlt");
    let namen: Vec<&str> = werkzeuge
        .iter()
        .map(|werkzeug| werkzeug["name"].as_str().unwrap())
        .collect();
    for erwartet in [
        "outline_collections",
        "outline_structure",
        "outline_search",
        "outline_list",
        "outline_read",
        "outline_comments",
        "outline_profile",
        "outline_create",
        "outline_update",
        "outline_move",
        "outline_archive",
        "outline_delete",
        "outline_comment",
        "outline_export",
    ] {
        assert!(namen.contains(&erwartet), "{erwartet} fehlt: {namen:?}");
    }
    assert_eq!(namen.len(), 14, "unerwartetes Werkzeug: {namen:?}");

    let lesend = werkzeuge
        .iter()
        .find(|werkzeug| werkzeug["name"] == json!("outline_collections"))
        .unwrap();
    assert_eq!(lesend["annotations"]["readOnlyHint"], json!(true));
    let schreibend = werkzeuge
        .iter()
        .find(|werkzeug| werkzeug["name"] == json!("outline_delete"))
        .unwrap();
    assert_eq!(schreibend["annotations"]["destructiveHint"], json!(true));

    // 3. Ein Aufruf klappt: das Profil kommt ohne Geheimnis zurück.
    sende(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "outline_profile", "arguments": {} }
    }));
    let aufruf = antwort(3);
    let text = aufruf["result"]["content"][0]["text"]
        .as_str()
        .expect("kein Text in der Antwort");
    assert!(text.contains("- arbeit (Standard)"), "{text}");
    assert!(text.contains("Wiki im Büro"), "{text}");
    assert!(text.contains("https://wiki.example.test"), "{text}");
    assert!(!text.contains("ol_api_nur_im_test_4711"), "{text}");
    assert_eq!(aufruf["result"]["isError"], json!(false));

    // 4. Ein unbekanntes Profil ist ein Fehler – ohne Geheimnis im Text.
    sende(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "outline_collections",
            "arguments": { "profil": "gibtsnicht" }
        }
    }));
    let fehler = antwort(4);
    let text = fehler["error"]["message"]
        .as_str()
        .expect("kein Fehlertext");
    assert!(text.contains("gibt es nicht"), "{text}");
    assert!(!text.contains("ol_api_nur_im_test_4711"), "{text}");

    // 5. Der Schlüssel bleibt im Elternprozess: das Kind hat nur den Tresor im
    //    Arbeitsspeicher und darin nichts.
    sende(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "outline_collections", "arguments": {} }
    }));
    let ohne_schluessel = antwort(5);
    let text = ohne_schluessel["error"]["message"]
        .as_str()
        .expect("kein Fehlertext");
    assert!(text.contains("API-Schlüssel"), "{text}");

    // stdin schließen beendet den Dienst ordentlich.
    drop(stdin);
    let ende = kind.wait().unwrap();
    assert!(ende.success(), "quiver-outline endete mit {ende:?}");

    let mut stderr = String::new();
    if let Some(mut rohr) = kind.stderr.take() {
        use std::io::Read;
        rohr.read_to_string(&mut stderr).unwrap();
    }
    assert!(
        !stderr.contains("ol_api_nur_im_test_4711"),
        "Schlüssel auf stderr: {stderr}"
    );
}
