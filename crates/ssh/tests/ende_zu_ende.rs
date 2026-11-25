// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Ende-zu-Ende: `quiver-ssh` als **Kindprozess** über stdio (SPEC §7).
//!
//! So startet quiver das Programm wirklich: es setzt `QUIVER_ZUGAENGE`,
//! `QUIVER_SSH_DATEN` und `HOME`, und spricht JSON-RPC auf stdin/stdout. Die
//! Attrappe antwortet auf `127.0.0.1`; `HOME` ist ein temp-Ordner, damit
//! niemals die echte `~/.ssh` gelesen oder geschrieben wird.

mod attrappe;

use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{json, Value};

/// Wie lange auf eine Antwort des Kindprozesses gewartet wird.
const GEDULD: Duration = Duration::from_secs(30);

/// Ein laufendes `quiver-ssh` mit stdin/stdout verbunden.
struct Kind {
    kind: Child,
    stdin: Option<ChildStdin>,
    empfaenger: mpsc::Receiver<String>,
}

impl Kind {
    /// Das Programm starten; `umgebung` sind die Paare, die quiver setzen würde.
    fn starte(umgebung: &[(&str, String)]) -> Kind {
        let mut befehl = Command::new(env!("CARGO_BIN_EXE_quiver-ssh"));
        // Nichts aus der Umgebung des Testlaufs soll hineinspielen.
        befehl
            .env_remove("QUIVER_ZUGAENGE")
            .env_remove("QUIVER_SSH_DATEN")
            .env_remove("HOME")
            .env_remove("USERPROFILE");
        for (name, wert) in umgebung {
            befehl.env(name, wert);
        }
        let mut kind = befehl
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("quiver-ssh startet nicht");

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
            json!("quiver-ssh")
        );
        assert_eq!(handschlag["result"]["serverInfo"]["title"], json!("SSH"));
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

    /// Ein Werkzeug aufrufen und den Text zurückgeben.
    fn aufrufen(&mut self, id: u64, name: &str, argumente: Value) -> (String, bool) {
        self.sende(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": { "name": name, "arguments": argumente }
        }));
        let antwort = self.antwort(id);
        let text = antwort["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("kein Text in der Antwort: {antwort}"))
            .to_string();
        let fehler = antwort["result"]["isError"].as_bool().unwrap_or(false);
        (text, fehler)
    }

    /// Das Programm beenden und stderr einsammeln.
    fn beenden(mut self) -> String {
        drop(self.stdin.take());
        let ende = self.kind.wait().unwrap();
        assert!(ende.success(), "quiver-ssh endete mit {ende:?}");
        let mut stderr = String::new();
        if let Some(mut rohr) = self.kind.stderr.take() {
            rohr.read_to_string(&mut stderr).unwrap();
        }
        stderr
    }
}

/// Die Umgebung, die quiver für eine Verbindung setzen würde.
fn umgebung(daten: &Path, home: &Path, zugaenge: &Value) -> Vec<(&'static str, String)> {
    vec![
        ("QUIVER_ZUGAENGE", zugaenge.to_string()),
        ("QUIVER_SSH_DATEN", daten.display().to_string()),
        ("HOME", home.display().to_string()),
    ]
}

fn pfade() -> (tempfile::TempDir, tempfile::TempDir) {
    (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap())
}

#[test]
fn kindprozess_richtet_den_schluessel_ein_und_nutzt_ihn_danach() {
    let laufzeit = tokio::runtime::Runtime::new().unwrap();
    laufzeit.block_on(async {
        let server = attrappe::Attrappe::starte().await;
        let (daten, home) = pfade();

        let austausch = tempfile::tempdir().unwrap();
        let mut eintrag = attrappe::zugang("web-01", server.port, Some(attrappe::PASSWORT), None);
        eintrag["werte"]["austauschordner"] = json!(austausch.path().to_str().unwrap());
        let mit_passwort = json!([eintrag]);

        // Erster Lauf: Passwort-Anmeldung richtet den Schlüssel ein.
        let mut kind = Kind::starte(&umgebung(daten.path(), home.path(), &mit_passwort));
        let namen = kind.begruessen();
        assert_eq!(namen.len(), 6, "unerwartete Werkzeuge: {namen:?}");
        for erwartet in [
            "ssh_status",
            "ssh_list",
            "ssh_exec",
            "ssh_upload",
            "ssh_download",
            "ssh_schluessel_einrichten",
        ] {
            assert!(namen.contains(&erwartet.to_string()), "{erwartet} fehlt");
        }

        let (text, fehler) = kind.aufrufen(3, "ssh_exec", json!({ "befehl": "id -un" }));
        assert!(!fehler, "{text}");
        assert!(text.contains("root"), "{text}");
        assert!(
            text.contains("Schlüssel für web-01 eingerichtet (SHA256:"),
            "{text}"
        );
        assert!(!text.contains(attrappe::PASSWORT), "{text}");

        let stderr = kind.beenden();
        assert!(
            !stderr.contains(attrappe::PASSWORT),
            "Passwort auf stderr: {stderr}"
        );

        // Der Schlüssel liegt unter QUIVER_SSH_DATEN/quiver-tools/ssh.
        let schluessel = daten
            .path()
            .join("quiver-tools/ssh/schluessel/web-01_ed25519");
        assert!(schluessel.exists(), "{} fehlt", schluessel.display());
        assert!(daten.path().join("quiver-tools/ssh/known_hosts").exists());

        // Zweiter Lauf: kein Anmelde-Passwort mehr – es geht trotzdem.
        let ohne_passwort = json!([attrappe::zugang("web-01", server.port, None, None)]);
        let mut kind = Kind::starte(&umgebung(daten.path(), home.path(), &ohne_passwort));
        kind.begruessen();
        let (text, fehler) = kind.aufrufen(3, "ssh_exec", json!({ "befehl": "id -un" }));
        assert!(!fehler, "{text}");
        assert!(text.contains("root"), "{text}");
        assert!(!text.contains("eingerichtet"), "{text}");

        let (status, fehler) = kind.aufrufen(4, "ssh_status", json!({}));
        assert!(!fehler, "{status}");
        assert!(
            status.contains("Schlüssel: eingerichtet (SHA256:"),
            "{status}"
        );
        kind.beenden();

        server.stoppen().await;
    });
}

#[test]
fn ohne_zugaenge_kommt_der_hinweis_und_kein_fehler() {
    let (daten, home) = pfade();
    // Kein QUIVER_ZUGAENGE, aber eigene Pfade – nie die echte ~/.ssh.
    let umgebung = [
        ("QUIVER_SSH_DATEN", daten.path().display().to_string()),
        ("HOME", home.path().display().to_string()),
    ];
    let mut kind = Kind::starte(&umgebung);

    kind.begruessen();
    let (text, fehler) = kind.aufrufen(3, "ssh_exec", json!({ "befehl": "id -un" }));
    assert!(!fehler, "{text}");
    assert!(text.contains("Zugang fehlt"), "{text}");
    assert!(text.contains("Tools → SSH"), "{text}");

    let (status, fehler) = kind.aufrufen(4, "ssh_status", json!({}));
    assert!(!fehler, "{status}");
    assert!(status.contains("Zugang fehlt"), "{status}");

    kind.beenden();
}

#[test]
fn eintrag_enthaelt_mehrfach_mit_sechs_feldern() {
    let ausgabe = Command::new(env!("CARGO_BIN_EXE_quiver-ssh"))
        .args(["eintrag", "--befehl", "/pfad/quiver-ssh"])
        .output()
        .expect("quiver-ssh eintrag startet nicht");
    assert!(
        ausgabe.status.success(),
        "eintrag endete mit {:?}: {}",
        ausgabe.status,
        String::from_utf8_lossy(&ausgabe.stderr)
    );

    let eintrag: Value = serde_json::from_slice(&ausgabe.stdout).expect("kein JSON");
    assert_eq!(eintrag["id"], json!("ssh"));
    assert_eq!(eintrag["titel"], json!("SSH"));
    assert_eq!(eintrag["version"], json!(env!("CARGO_PKG_VERSION")));
    assert_eq!(eintrag["herkunft"], json!("eigen"));
    assert_eq!(eintrag["laufzeit"]["art"], json!("stdio"));
    assert_eq!(eintrag["laufzeit"]["befehl"], json!("/pfad/quiver-ssh"));
    // Der Eintrag selbst hat keine Felder und keine Umgebung …
    assert_eq!(eintrag["felder"].as_array().unwrap().len(), 0, "{eintrag}");
    assert_eq!(
        eintrag["laufzeit"]["umgebung"].as_object().unwrap().len(),
        0,
        "{eintrag}"
    );

    // … sondern eine Liste von Verbindungen mit sechs Feldern.
    let mehrfach = &eintrag["mehrfach"];
    assert_eq!(mehrfach["titel"], json!("Verbindungen"));
    assert_eq!(mehrfach["einzahl"], json!("Verbindung"));
    let felder = mehrfach["felder"].as_array().unwrap();
    assert_eq!(felder.len(), 6, "{mehrfach}");
    let namen: Vec<&str> = felder
        .iter()
        .map(|feld| feld["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        namen,
        vec![
            "benutzer",
            "host",
            "port",
            "anmelde_passwort",
            "schreiben",
            "austauschordner"
        ]
    );
    assert_eq!(felder[0]["quelle"], json!("client"));
    assert_eq!(felder[0]["pflicht"], json!(true));
    assert_eq!(felder[2]["standard"], json!("22"));
    // Ein Passwort für Anmeldung und sudo; ein eigenes sudo-Feld gibt es nicht mehr.
    assert_eq!(felder[3]["titel"], json!("Passwort"));
    assert_eq!(felder[3]["quelle"], json!("geheim"));
    assert_eq!(
        felder[3]["beschreibung"],
        json!("Für die Anmeldung und für sudo. Bleibt verschlüsselt auf diesem Gerät.")
    );
}

#[test]
fn testen_ohne_zugaenge_endet_mit_fehler_und_nennt_die_variable() {
    let (daten, home) = pfade();
    let ausgabe = Command::new(env!("CARGO_BIN_EXE_quiver-ssh"))
        .arg("testen")
        .env_remove("QUIVER_ZUGAENGE")
        .env("QUIVER_SSH_DATEN", daten.path())
        .env("HOME", home.path())
        .env_remove("USERPROFILE")
        .output()
        .expect("quiver-ssh testen startet nicht");
    assert_eq!(ausgabe.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&ausgabe.stderr);
    assert!(stderr.contains("Zugang fehlt"), "{stderr}");
    assert!(stderr.contains("QUIVER_ZUGAENGE"), "{stderr}");
}

#[test]
fn hilfe_und_version_nennen_das_programm() {
    let hilfe = Command::new(env!("CARGO_BIN_EXE_quiver-ssh"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(hilfe.status.success());
    let text = String::from_utf8_lossy(&hilfe.stdout);
    assert!(text.contains("quiver-ssh eintrag"), "{text}");
    assert!(text.contains("MCP über stdio"), "{text}");

    let version = Command::new(env!("CARGO_BIN_EXE_quiver-ssh"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(version.status.success());
    assert!(String::from_utf8_lossy(&version.stdout).starts_with("ssh "));
}
