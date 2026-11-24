// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Ende-zu-Ende: `quiver-email` als **Kindprozess** über stdio (SPEC §7).
//!
//! So startet quiver das Programm wirklich: es setzt die Umgebung aus den
//! Feldern des Bibliothekseintrags (`EMAIL_ADRESSE`, `EMAIL_PASSWORT`, …) und
//! spricht JSON-RPC auf stdin/stdout. Hier wird nur geprüft, was ohne
//! Mailserver geht: Handschlag, `tools/list`, der Zugangsbericht, der
//! Hinweistext ohne Umgebung, `eintrag` und `testen`. Die Protokollschicht
//! selbst prüfen die Attrappen in `tests/werkzeuge.rs`.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{json, Value};

/// Wie lange auf eine Antwort des Kindprozesses gewartet wird.
const GEDULD: Duration = Duration::from_secs(30);

/// Das Passwort, das quiver setzen würde; es darf nirgends auftauchen.
const PASSWORT: &str = "ende-zu-ende-passwort-4711";

/// Ein laufendes `quiver-email` mit stdin/stdout verbunden.
struct Kind {
    kind: Child,
    stdin: Option<ChildStdin>,
    empfaenger: mpsc::Receiver<String>,
}

impl Kind {
    fn starte(umgebung: &[(&str, &str)]) -> Kind {
        let mut befehl = Command::new(env!("CARGO_BIN_EXE_quiver-email"));
        // Nichts aus der Umgebung des Testlaufs soll hineinspielen.
        for name in [
            "EMAIL_ADRESSE",
            "EMAIL_NAME",
            "EMAIL_IMAP_HOST",
            "EMAIL_IMAP_PORT",
            "EMAIL_IMAP_SICHERHEIT",
            "EMAIL_SMTP_HOST",
            "EMAIL_SMTP_PORT",
            "EMAIL_SMTP_SICHERHEIT",
            "EMAIL_BENUTZER",
            "EMAIL_PASSWORT",
            "EMAIL_ZERTIFIKAT_PRUEFEN",
        ] {
            befehl.env_remove(name);
        }
        for (name, wert) in umgebung {
            befehl.env(name, wert);
        }
        let mut kind = befehl
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("quiver-email startet nicht");

        let stdin = kind.stdin.take().unwrap();
        let stdout = kind.stdout.take().unwrap();

        let (sender, empfaenger) = mpsc::channel::<String>();
        std::thread::spawn(move || {
            for zeile in BufReader::new(stdout).lines().map_while(Result::ok) {
                if sender.send(zeile).is_err() {
                    break;
                }
            }
        });

        Kind {
            kind,
            stdin: Some(stdin),
            empfaenger,
        }
    }

    fn sende(&mut self, wert: Value) {
        let stdin = self.stdin.as_mut().expect("stdin ist schon geschlossen");
        writeln!(stdin, "{wert}").unwrap();
        stdin.flush().unwrap();
    }

    fn antwort(&self, id: u64) -> Value {
        loop {
            let zeile = self
                .empfaenger
                .recv_timeout(GEDULD)
                .unwrap_or_else(|_| panic!("keine Antwort auf Anfrage {id}"));
            let Ok(wert) = serde_json::from_str::<Value>(&zeile) else {
                continue;
            };
            if wert.get("id").and_then(Value::as_u64) == Some(id) {
                return wert;
            }
        }
    }

    /// Handschlag und `tools/list`; gibt die Werkzeugnamen zurück.
    fn begruessen(&mut self) -> Vec<String> {
        self.sende(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "quiver-test", "version": "0" }
            }
        }));
        let handschlag = self.antwort(1);
        assert_eq!(
            handschlag["result"]["serverInfo"]["name"],
            json!("quiver-email")
        );
        assert_eq!(handschlag["result"]["serverInfo"]["title"], json!("E-Mail"));
        assert!(
            handschlag["result"]["capabilities"]["tools"].is_object(),
            "Werkzeuge fehlen in den Fähigkeiten: {handschlag}"
        );

        self.sende(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
        self.sende(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        }));
        let liste = self.antwort(2);
        liste["result"]["tools"]
            .as_array()
            .expect("tools fehlt")
            .iter()
            .map(|werkzeug| werkzeug["name"].as_str().unwrap().to_string())
            .collect()
    }

    fn beenden(mut self) -> String {
        drop(self.stdin.take());
        let ende = self.kind.wait().unwrap();
        assert!(ende.success(), "quiver-email endete mit {ende:?}");
        let mut stderr = String::new();
        if let Some(mut rohr) = self.kind.stderr.take() {
            rohr.read_to_string(&mut stderr).unwrap();
        }
        stderr
    }
}

#[test]
fn kindprozess_spricht_mcp_ueber_stdio_und_nutzt_die_umgebung() {
    let mut kind = Kind::starte(&[
        ("EMAIL_ADRESSE", "ich@example.test"),
        ("EMAIL_NAME", "Ich"),
        ("EMAIL_IMAP_HOST", "imap.example.test"),
        ("EMAIL_SMTP_HOST", "smtp.example.test"),
        ("EMAIL_PASSWORT", PASSWORT),
    ]);

    let namen = kind.begruessen();
    assert_eq!(namen.len(), 12, "unerwartete Werkzeuge: {namen:?}");
    for erwartet in [
        "email_profile",
        "email_list_mailboxes",
        "email_fetch",
        "email_search",
        "email_read",
        "email_send",
        "email_reply",
        "email_forward",
        "email_draft_reply",
        "email_flag",
        "email_move",
        "email_delete",
    ] {
        assert!(namen.contains(&erwartet.to_string()), "{erwartet} fehlt");
    }

    // Der Zugang kommt ohne Geheimnis zurück und kostet kein Netz.
    kind.sende(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "email_profile", "arguments": {} }
    }));
    let aufruf = kind.antwort(3);
    let text = aufruf["result"]["content"][0]["text"]
        .as_str()
        .expect("kein Text in der Antwort");
    assert!(text.contains("- standard"), "{text}");
    assert!(text.contains("ich@example.test"), "{text}");
    assert!(text.contains("imap.example.test"), "{text}");
    assert!(!text.contains(PASSWORT), "{text}");
    assert_eq!(aufruf["result"]["isError"], json!(false));

    let stderr = kind.beenden();
    assert!(!stderr.contains(PASSWORT), "Passwort auf stderr: {stderr}");
}

#[test]
fn ohne_umgebung_kommt_der_hinweis_und_kein_fehler() {
    let mut kind = Kind::starte(&[]);
    kind.begruessen();

    kind.sende(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "email_fetch", "arguments": {} }
    }));
    let aufruf = kind.antwort(3);
    let text = aufruf["result"]["content"][0]["text"]
        .as_str()
        .expect("kein Text in der Antwort");
    assert!(text.contains("Zugang fehlt"), "{text}");
    assert!(text.contains("Tools → E-Mail"), "{text}");
    assert_eq!(aufruf["result"]["isError"], json!(false));

    kind.beenden();
}

#[test]
fn eintrag_liefert_den_bibliothekseintrag_als_json() {
    let ausgabe = Command::new(env!("CARGO_BIN_EXE_quiver-email"))
        .args(["eintrag", "--befehl", "/pfad/quiver-email"])
        .output()
        .expect("quiver-email eintrag startet nicht");
    assert!(
        ausgabe.status.success(),
        "eintrag endete mit {:?}: {}",
        ausgabe.status,
        String::from_utf8_lossy(&ausgabe.stderr)
    );

    let eintrag: Value = serde_json::from_slice(&ausgabe.stdout).expect("kein JSON");
    assert_eq!(eintrag["id"], json!("email"));
    assert_eq!(eintrag["titel"], json!("E-Mail"));
    assert_eq!(eintrag["version"], json!(env!("CARGO_PKG_VERSION")));
    assert_eq!(eintrag["herkunft"], json!("eigen"));
    assert_eq!(eintrag["laufzeit"]["art"], json!("stdio"));
    assert_eq!(eintrag["laufzeit"]["befehl"], json!("/pfad/quiver-email"));
    assert_eq!(
        eintrag["laufzeit"]["umgebung"]["EMAIL_ADRESSE"],
        json!("${adresse}")
    );
    assert_eq!(
        eintrag["laufzeit"]["umgebung"]["EMAIL_PASSWORT"],
        json!("${passwort}")
    );
    assert_eq!(
        eintrag["laufzeit"]["umgebung"]["EMAIL_IMAP_SICHERHEIT"],
        json!("${imap_sicherheit}")
    );

    let felder = eintrag["felder"].as_array().unwrap();
    assert_eq!(felder.len(), 12);
    assert_eq!(felder[0]["name"], json!("adresse"));
    assert_eq!(felder[0]["quelle"], json!("client"));
    assert_eq!(felder[0]["pflicht"], json!(true));
    assert_eq!(felder[0]["standard"], json!(null));

    let port = felder
        .iter()
        .find(|feld| feld["name"] == json!("imap_port"))
        .unwrap();
    assert_eq!(port["standard"], json!("993"));
    assert_eq!(port["pflicht"], json!(false));

    let passwort = felder
        .iter()
        .find(|feld| feld["name"] == json!("passwort"))
        .unwrap();
    assert_eq!(passwort["quelle"], json!("geheim"));
    assert_eq!(passwort["pflicht"], json!(true));

    let schalter = felder
        .iter()
        .find(|feld| feld["name"] == json!("zertifikat_pruefen"))
        .unwrap();
    assert_eq!(schalter["standard"], json!("an"));
    assert_eq!(schalter["quelle"], json!("client"));

    // Der Schreibschutz ist im Zweifel aus.
    let schreiben = felder
        .iter()
        .find(|feld| feld["name"] == json!("schreiben"))
        .unwrap();
    assert_eq!(schreiben["art"], json!("schalter"));
    assert_eq!(schreiben["standard"], json!("aus"));

    assert!(eintrag.get("einrichtung").is_none(), "{eintrag}");
}

#[test]
fn testen_ohne_umgebung_endet_mit_fehler_und_nennt_die_variablen() {
    let mut befehl = Command::new(env!("CARGO_BIN_EXE_quiver-email"));
    befehl.arg("testen");
    for name in [
        "EMAIL_ADRESSE",
        "EMAIL_IMAP_HOST",
        "EMAIL_SMTP_HOST",
        "EMAIL_PASSWORT",
    ] {
        befehl.env_remove(name);
    }
    let ausgabe = befehl.output().expect("quiver-email testen startet nicht");
    assert_eq!(ausgabe.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&ausgabe.stderr);
    assert!(stderr.contains("Zugang fehlt"), "{stderr}");
    assert!(stderr.contains("EMAIL_ADRESSE"), "{stderr}");
    assert!(stderr.contains("EMAIL_IMAP_HOST"), "{stderr}");
    assert!(stderr.contains("EMAIL_SMTP_HOST"), "{stderr}");
    assert!(stderr.contains("EMAIL_PASSWORT"), "{stderr}");
}

#[test]
fn version_und_hilfe_nennen_das_programm() {
    let version = Command::new(env!("CARGO_BIN_EXE_quiver-email"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(version.status.success());
    let text = String::from_utf8_lossy(&version.stdout);
    assert!(text.starts_with("email "), "{text}");

    let hilfe = Command::new(env!("CARGO_BIN_EXE_quiver-email"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(hilfe.status.success());
    let text = String::from_utf8_lossy(&hilfe.stdout);
    assert!(text.contains("quiver-email eintrag"), "{text}");
    assert!(text.contains("quiver-email testen"), "{text}");
    assert!(!text.contains("einrichten"), "{text}");

    let unbekannt = Command::new(env!("CARGO_BIN_EXE_quiver-email"))
        .arg("setup")
        .output()
        .unwrap();
    assert_eq!(unbekannt.status.code(), Some(2));
}
