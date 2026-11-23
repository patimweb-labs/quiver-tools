// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Ende-zu-Ende: `quiver-outline` als **Kindprozess** über stdio (SPEC §7).
//!
//! So startet quiver das Programm wirklich: es setzt die Umgebung aus den
//! Feldern des Bibliothekseintrags (`OUTLINE_URL`, `OUTLINE_API_KEY`) und
//! spricht JSON-RPC auf stdin/stdout. Ein echter Dienst ist nie im Spiel – die
//! Attrappe antwortet auf `127.0.0.1`.

mod attrappe;

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{json, Value};

/// Wie lange auf eine Antwort des Kindprozesses gewartet wird.
const GEDULD: Duration = Duration::from_secs(30);

/// Ein laufendes `quiver-outline` mit stdin/stdout verbunden.
struct Kind {
    kind: Child,
    stdin: Option<ChildStdin>,
    empfaenger: mpsc::Receiver<String>,
}

impl Kind {
    /// Das Programm starten; `umgebung` sind die Paare, die quiver setzen würde.
    fn starte(umgebung: &[(&str, &str)]) -> Kind {
        let mut befehl = Command::new(env!("CARGO_BIN_EXE_quiver-outline"));
        // Nichts aus der Umgebung des Testlaufs soll hineinspielen.
        befehl
            .env_remove("OUTLINE_URL")
            .env_remove("OUTLINE_API_KEY");
        for (name, wert) in umgebung {
            befehl.env(name, wert);
        }
        let mut kind = befehl
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("quiver-outline startet nicht");

        let stdin = kind.stdin.take().unwrap();
        let stdout = kind.stdout.take().unwrap();

        // Zeilen aus stdout in einen Kanal: so kann der Test auf eine Antwort
        // warten, statt bei einem Fehler ewig zu hängen.
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

    /// Das Programm beenden und stderr einsammeln.
    fn beenden(mut self) -> String {
        // stdin schließen beendet den Dienst ordentlich.
        drop(self.stdin.take());
        let ende = self.kind.wait().unwrap();
        assert!(ende.success(), "quiver-outline endete mit {ende:?}");
        let mut stderr = String::new();
        if let Some(mut rohr) = self.kind.stderr.take() {
            rohr.read_to_string(&mut stderr).unwrap();
        }
        stderr
    }
}

#[test]
fn kindprozess_spricht_mcp_ueber_stdio_und_nutzt_die_umgebung() {
    // Die Attrappe lebt im Testprozess; das Kind spricht über 127.0.0.1 mit ihr.
    let laufzeit = tokio::runtime::Runtime::new().unwrap();
    let mock = laufzeit.block_on(attrappe::Attrappe::starte());
    let mut kind = Kind::starte(&[
        ("OUTLINE_URL", mock.adresse.as_str()),
        ("OUTLINE_API_KEY", attrappe::SCHLUESSEL),
    ]);

    let namen = kind.begruessen();
    assert_eq!(namen.len(), 14, "unerwartete Werkzeuge: {namen:?}");
    for erwartet in [
        "outline_collections",
        "outline_search",
        "outline_read",
        "outline_profile",
        "outline_export",
    ] {
        assert!(namen.contains(&erwartet.to_string()), "{erwartet} fehlt");
    }

    // 3. Der Zugang kommt ohne Geheimnis zurück.
    kind.sende(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "outline_profile", "arguments": {} }
    }));
    let aufruf = kind.antwort(3);
    let text = aufruf["result"]["content"][0]["text"]
        .as_str()
        .expect("kein Text in der Antwort");
    assert!(text.contains("- standard"), "{text}");
    assert!(text.contains(&mock.adresse), "{text}");
    assert!(!text.contains(attrappe::SCHLUESSEL), "{text}");
    assert_eq!(aufruf["result"]["isError"], json!(false));

    // 4. Ein Werkzeug arbeitet mit der Umgebung, ohne `profil`-Parameter.
    kind.sende(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "outline_collections", "arguments": {} }
    }));
    let aufruf = kind.antwort(4);
    let text = aufruf["result"]["content"][0]["text"]
        .as_str()
        .expect("kein Text in der Antwort");
    assert!(text.contains("Sammlungen (2):"), "{text}");
    assert!(!text.contains(attrappe::SCHLUESSEL), "{text}");
    assert_eq!(aufruf["result"]["isError"], json!(false));
    assert_eq!(
        mock.aufzeichnung
            .gemerkt("collections.list")
            .unwrap()
            .anmeldung
            .as_deref(),
        Some(format!("Bearer {}", attrappe::SCHLUESSEL).as_str())
    );

    let stderr = kind.beenden();
    assert!(
        !stderr.contains(attrappe::SCHLUESSEL),
        "Schlüssel auf stderr: {stderr}"
    );
}

#[test]
fn ohne_umgebung_kommt_der_hinweis_und_kein_fehler() {
    let mut kind = Kind::starte(&[]);
    kind.begruessen();

    kind.sende(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "outline_collections", "arguments": {} }
    }));
    let aufruf = kind.antwort(3);
    let text = aufruf["result"]["content"][0]["text"]
        .as_str()
        .expect("kein Text in der Antwort");
    assert!(text.contains("Zugang fehlt"), "{text}");
    assert!(text.contains("Tools → Outline-Wiki"), "{text}");
    assert_eq!(aufruf["result"]["isError"], json!(false));

    kind.beenden();
}

#[test]
fn eintrag_liefert_den_bibliothekseintrag_als_json() {
    let ausgabe = Command::new(env!("CARGO_BIN_EXE_quiver-outline"))
        .args(["eintrag", "--befehl", "/pfad/quiver-outline"])
        .output()
        .expect("quiver-outline eintrag startet nicht");
    assert!(
        ausgabe.status.success(),
        "eintrag endete mit {:?}: {}",
        ausgabe.status,
        String::from_utf8_lossy(&ausgabe.stderr)
    );

    let eintrag: Value = serde_json::from_slice(&ausgabe.stdout).expect("kein JSON");
    assert_eq!(eintrag["id"], json!("outline"));
    assert_eq!(eintrag["titel"], json!("Outline-Wiki"));
    assert_eq!(eintrag["version"], json!(env!("CARGO_PKG_VERSION")));
    assert_eq!(eintrag["herkunft"], json!("eigen"));
    assert_eq!(eintrag["laufzeit"]["art"], json!("stdio"));
    assert_eq!(eintrag["laufzeit"]["befehl"], json!("/pfad/quiver-outline"));
    assert_eq!(
        eintrag["laufzeit"]["umgebung"]["OUTLINE_URL"],
        json!("${url}")
    );
    assert_eq!(
        eintrag["laufzeit"]["umgebung"]["OUTLINE_API_KEY"],
        json!("${api_key}")
    );
    assert_eq!(eintrag["felder"].as_array().unwrap().len(), 2);
    assert_eq!(eintrag["felder"][0]["name"], json!("url"));
    assert_eq!(eintrag["felder"][0]["quelle"], json!("client"));
    assert_eq!(eintrag["felder"][0]["pflicht"], json!(true));
    assert_eq!(eintrag["felder"][1]["quelle"], json!("geheim"));
    assert!(eintrag.get("einrichtung").is_none(), "{eintrag}");
}

#[test]
fn testen_ohne_umgebung_endet_mit_fehler_und_nennt_die_variablen() {
    let ausgabe = Command::new(env!("CARGO_BIN_EXE_quiver-outline"))
        .arg("testen")
        .env_remove("OUTLINE_URL")
        .env_remove("OUTLINE_API_KEY")
        .output()
        .expect("quiver-outline testen startet nicht");
    assert_eq!(ausgabe.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&ausgabe.stderr);
    assert!(stderr.contains("Zugang fehlt"), "{stderr}");
    assert!(stderr.contains("OUTLINE_URL"), "{stderr}");
    assert!(stderr.contains("OUTLINE_API_KEY"), "{stderr}");
}
