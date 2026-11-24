// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Ende-zu-Ende: `quiver-abap` als **Kindprozess** über stdio (SPEC §7).
//!
//! So startet quiver das Programm wirklich: es setzt `QUIVER_ZUGAENGE` und
//! spricht JSON-RPC auf stdin/stdout. Die Attrappe antwortet auf `127.0.0.1`;
//! ein echtes SAP-System ist nie im Spiel.

mod attrappe;

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{json, Value};

/// Wie lange auf eine Antwort des Kindprozesses gewartet wird.
const GEDULD: Duration = Duration::from_secs(30);

/// Ein laufendes `quiver-abap` mit stdin/stdout verbunden.
struct Kind {
    kind: Child,
    stdin: Option<ChildStdin>,
    empfaenger: mpsc::Receiver<String>,
}

impl Kind {
    /// Das Programm starten; `umgebung` sind die Paare, die quiver setzen würde.
    fn starte(umgebung: &[(&'static str, String)]) -> Kind {
        let mut befehl = Command::new(env!("CARGO_BIN_EXE_quiver-abap"));
        // Nichts aus der Umgebung des Testlaufs soll hineinspielen.
        befehl.env_remove("QUIVER_ZUGAENGE");
        for (name, wert) in umgebung {
            befehl.env(name, wert);
        }
        let mut kind = befehl
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("quiver-abap startet nicht");

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

    /// Handschlag und `tools/list`; gibt die Werkzeuge zurück.
    fn begruessen(&mut self) -> Vec<Value> {
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
            json!("quiver-abap")
        );
        assert_eq!(handschlag["result"]["serverInfo"]["title"], json!("ABAP"));
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
            .clone()
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
        assert!(ende.success(), "quiver-abap endete mit {ende:?}");
        let mut stderr = String::new();
        if let Some(mut rohr) = self.kind.stderr.take() {
            rohr.read_to_string(&mut stderr).unwrap();
        }
        stderr
    }
}

/// Die Namen der Werkzeuge aus `tools/list`.
fn namen(werkzeuge: &[Value]) -> Vec<String> {
    werkzeuge
        .iter()
        .map(|werkzeug| werkzeug["name"].as_str().unwrap().to_string())
        .collect()
}

/// Die Zugangsliste, die quiver setzen würde.
fn zugaenge(adresse: &str) -> Value {
    zugaenge_mit(adresse, true)
}

/// Dasselbe mit gewähltem Schalter `schreiben`.
fn zugaenge_mit(adresse: &str, schreiben: bool) -> Value {
    json!([{
        "name": "dev-100",
        "werte": {
            "benutzer": attrappe::BENUTZER,
            "url": adresse,
            "mandant": "100",
            "sprache": "DE",
            "passwort": attrappe::PASSWORT,
            "schreiben": if schreiben { "an" } else { "aus" },
        }
    }])
}

#[test]
fn kindprozess_listet_werkzeuge_und_liest_ein_objekt() {
    let laufzeit = tokio::runtime::Runtime::new().unwrap();
    laufzeit.block_on(async {
        let server = attrappe::Attrappe::starte().await;
        let umgebung = [("QUIVER_ZUGAENGE", zugaenge(&server.adresse).to_string())];

        let mut kind = Kind::starte(&umgebung);
        let werkzeuge = kind.begruessen();
        let namen = namen(&werkzeuge);
        assert_eq!(namen.len(), 26, "unerwartete Werkzeuge: {namen:?}");
        for erwartet in [
            "abap_status",
            "abap_test_connection",
            "abap_search",
            "abap_object_structure",
            "abap_read",
            "abap_where_used",
            "abap_object_types",
            "abap_node_contents",
            "abap_table",
            "abap_query",
            "abap_syntax_check",
            "abap_unit_test",
            "abap_atc",
            "abap_dumps",
            "abap_traces",
            "abap_trace_hitlist",
            "abap_transports",
            "abap_transport_details",
            "abap_text_elements",
            "abap_lock",
            "abap_unlock",
            "abap_write",
            "abap_activate",
            "abap_create_object",
            "abap_set_text_elements",
            "abap_profile",
        ] {
            assert!(namen.contains(&erwartet.to_string()), "{erwartet} fehlt");
        }

        // Die Annotationen stimmen: lesend, schreibend oder nur ändernd.
        for werkzeug in &werkzeuge {
            let name = werkzeug["name"].as_str().unwrap();
            let hinweise = &werkzeug["annotations"];
            let (lesend, destruktiv) = match name {
                "abap_write"
                | "abap_activate"
                | "abap_create_object"
                | "abap_set_text_elements" => (false, true),
                "abap_lock" | "abap_unlock" => (false, false),
                _ => (true, false),
            };
            assert_eq!(
                hinweise["readOnlyHint"].as_bool(),
                Some(lesend),
                "{name}: {hinweise}"
            );
            assert_eq!(
                hinweise["destructiveHint"].as_bool(),
                Some(destruktiv),
                "{name}: {hinweise}"
            );
        }

        let (status, fehler) = kind.aufrufen(3, "abap_status", json!({}));
        assert!(!fehler, "{status}");
        assert!(status.contains("dev-100"), "{status}");
        assert!(!status.contains(attrappe::PASSWORT), "{status}");

        // `abap_profile` kommt aus dem Kern und nennt bei einer Systemliste die
        // Systeme – ohne Passwort.
        let (profil, fehler) = kind.aufrufen(30, "abap_profile", json!({}));
        assert!(!fehler, "{profil}");
        assert!(profil.contains("dev-100"), "{profil}");
        assert!(!profil.contains(attrappe::PASSWORT), "{profil}");

        let (text, fehler) = kind.aufrufen(4, "abap_read", json!({ "name": "ZCL_DEMO" }));
        assert!(!fehler, "{text}");
        assert!(text.contains("CLASS zcl_demo DEFINITION"), "{text}");
        assert!(!text.contains(attrappe::PASSWORT), "{text}");

        let (text, fehler) = kind.aufrufen(5, "abap_table", json!({ "table": "SFLIGHT" }));
        assert!(!fehler, "{text}");
        assert!(text.contains("CARRID | CONNID"), "{text}");

        let stderr = kind.beenden();
        assert!(
            !stderr.contains(attrappe::PASSWORT),
            "Passwort auf stderr: {stderr}"
        );
    });
}

#[test]
fn kindprozess_sperrt_schreibt_aktiviert_und_gibt_frei() {
    let laufzeit = tokio::runtime::Runtime::new().unwrap();
    laufzeit.block_on(async {
        let server = attrappe::Attrappe::starte().await;
        let umgebung = [("QUIVER_ZUGAENGE", zugaenge(&server.adresse).to_string())];

        let mut kind = Kind::starte(&umgebung);
        kind.begruessen();

        let (text, fehler) = kind.aufrufen(3, "abap_lock", json!({ "name": "ZCL_DEMO" }));
        assert!(!fehler, "{text}");
        assert!(text.contains("Lock-Handle: LH1"), "{text}");
        assert_eq!(server.offene_locks(), 1);

        let (text, fehler) = kind.aufrufen(
            4,
            "abap_write",
            json!({
                "name": "ZCL_DEMO",
                "source": "CLASS zcl_demo DEFINITION PUBLIC FINAL CREATE PUBLIC.\nENDCLASS.",
                "lockHandle": "LH1"
            }),
        );
        assert!(!fehler, "{text}");
        assert!(text.contains("Quelltext geschrieben"), "{text}");
        assert!(text.contains("Lock bleibt"), "{text}");

        let (text, fehler) = kind.aufrufen(5, "abap_activate", json!({ "name": "ZCL_DEMO" }));
        assert!(!fehler, "{text}");
        assert!(text.contains("[E] Zeile 3"), "{text}");

        let (text, fehler) = kind.aufrufen(
            6,
            "abap_create_object",
            json!({
                "objectType": "PROG/P",
                "name": "ZREPORT",
                "parentName": "$TMP",
                "description": "Neuer Report"
            }),
        );
        assert!(!fehler, "{text}");
        assert!(text.contains("PROG/P ZREPORT in $TMP angelegt"), "{text}");

        let (text, fehler) = kind.aufrufen(
            7,
            "abap_unlock",
            json!({ "name": "ZCL_DEMO", "lockHandle": "LH1" }),
        );
        assert!(!fehler, "{text}");
        assert!(text.contains("freigegeben"), "{text}");
        assert_eq!(server.offene_locks(), 0);

        let stderr = kind.beenden();
        assert!(
            !stderr.contains(attrappe::PASSWORT),
            "Passwort auf stderr: {stderr}"
        );
    });
}

#[test]
fn ohne_schreiben_antwortet_der_kindprozess_ohne_anfrage() {
    let laufzeit = tokio::runtime::Runtime::new().unwrap();
    laufzeit.block_on(async {
        let server = attrappe::Attrappe::starte().await;
        let umgebung = [(
            "QUIVER_ZUGAENGE",
            zugaenge_mit(&server.adresse, false).to_string(),
        )];

        let mut kind = Kind::starte(&umgebung);
        kind.begruessen();

        let (text, fehler) = kind.aufrufen(3, "abap_lock", json!({ "name": "ZCL_DEMO" }));
        assert!(!fehler, "{text}");
        assert!(text.contains("Schreiben ist für dev-100 aus"), "{text}");
        assert!(server.aufzeichnung.alle().is_empty());

        kind.beenden();
    });
}

#[test]
fn beenden_gibt_offene_locks_frei() {
    let laufzeit = tokio::runtime::Runtime::new().unwrap();
    laufzeit.block_on(async {
        let server = attrappe::Attrappe::starte().await;
        let umgebung = [("QUIVER_ZUGAENGE", zugaenge(&server.adresse).to_string())];

        let mut kind = Kind::starte(&umgebung);
        kind.begruessen();

        let (text, fehler) = kind.aufrufen(3, "abap_lock", json!({ "name": "ZCL_DEMO" }));
        assert!(!fehler, "{text}");
        assert_eq!(server.offene_locks(), 1);

        // stdin zu heißt: das Programm endet – und gibt seine Locks frei.
        kind.beenden();
        assert_eq!(server.offene_locks(), 0, "Lock blieb beim Beenden stehen");
        assert_eq!(server.unlocks(), 1);
    });
}

#[test]
fn ohne_zugaenge_kommt_der_hinweis_und_kein_fehler() {
    let mut kind = Kind::starte(&[]);

    let werkzeuge = kind.begruessen();
    assert_eq!(namen(&werkzeuge).len(), 26);
    let (text, fehler) = kind.aufrufen(3, "abap_read", json!({ "name": "ZCL_DEMO" }));
    assert!(!fehler, "{text}");
    assert!(text.contains("Zugang fehlt"), "{text}");
    assert!(text.contains("Tools → ABAP"), "{text}");

    let (status, fehler) = kind.aufrufen(4, "abap_status", json!({}));
    assert!(!fehler, "{status}");
    assert!(status.contains("Zugang fehlt"), "{status}");

    kind.beenden();
}

#[test]
fn eintrag_enthaelt_mehrfach_mit_sieben_feldern() {
    let ausgabe = Command::new(env!("CARGO_BIN_EXE_quiver-abap"))
        .args(["eintrag", "--befehl", "/pfad/quiver-abap"])
        .output()
        .expect("quiver-abap eintrag startet nicht");
    assert!(
        ausgabe.status.success(),
        "eintrag endete mit {:?}: {}",
        ausgabe.status,
        String::from_utf8_lossy(&ausgabe.stderr)
    );

    let eintrag: Value = serde_json::from_slice(&ausgabe.stdout).expect("kein JSON");
    assert_eq!(eintrag["id"], json!("abap"));
    assert_eq!(eintrag["titel"], json!("ABAP"));
    assert_eq!(eintrag["version"], json!(env!("CARGO_PKG_VERSION")));
    assert_eq!(eintrag["herkunft"], json!("eigen"));
    assert_eq!(eintrag["laufzeit"]["art"], json!("stdio"));
    assert_eq!(eintrag["laufzeit"]["befehl"], json!("/pfad/quiver-abap"));
    // Der Eintrag selbst hat keine Felder und keine Umgebung …
    assert_eq!(eintrag["felder"].as_array().unwrap().len(), 0, "{eintrag}");
    assert_eq!(
        eintrag["laufzeit"]["umgebung"].as_object().unwrap().len(),
        0,
        "{eintrag}"
    );

    // … sondern eine Liste von Systemen mit sechs Feldern.
    let mehrfach = &eintrag["mehrfach"];
    assert_eq!(mehrfach["titel"], json!("Systeme"));
    assert_eq!(mehrfach["einzahl"], json!("System"));
    let felder = mehrfach["felder"].as_array().unwrap();
    assert_eq!(felder.len(), 7, "{mehrfach}");
    let namen: Vec<&str> = felder
        .iter()
        .map(|feld| feld["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        namen,
        vec![
            "benutzer",
            "url",
            "mandant",
            "sprache",
            "passwort",
            "zertifikat_pruefen",
            "schreiben"
        ]
    );
    assert_eq!(felder[0]["quelle"], json!("client"));
    assert_eq!(felder[0]["pflicht"], json!(true));
    assert_eq!(felder[1]["quelle"], json!("client"));
    assert_eq!(felder[2]["pflicht"], json!(true));
    assert_eq!(felder[3]["standard"], json!("DE"));
    assert_eq!(felder[4]["quelle"], json!("geheim"));
    assert_eq!(felder[5]["standard"], json!("an"));
    assert!(felder[4]["beschreibung"]
        .as_str()
        .unwrap()
        .contains("SAP-Passwort"));
    assert!(felder[5]["beschreibung"]
        .as_str()
        .unwrap()
        .contains("Zertifikatsspeicher"));
    // Der Schalter „schreiben“ steht am Ende und ist im Zweifel aus.
    assert_eq!(felder[6]["name"], json!("schreiben"));
    assert_eq!(felder[6]["standard"], json!("aus"));
    assert_eq!(felder[6]["pflicht"], json!(false));
    assert!(felder[6]["beschreibung"]
        .as_str()
        .unwrap()
        .contains("Produktivsysteme aus lassen"));
}

#[test]
fn testen_ohne_zugaenge_endet_mit_fehler_und_nennt_die_variable() {
    let ausgabe = Command::new(env!("CARGO_BIN_EXE_quiver-abap"))
        .arg("testen")
        .env_remove("QUIVER_ZUGAENGE")
        .output()
        .expect("quiver-abap testen startet nicht");
    assert_eq!(ausgabe.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&ausgabe.stderr);
    assert!(stderr.contains("Zugang fehlt"), "{stderr}");
    assert!(stderr.contains("QUIVER_ZUGAENGE"), "{stderr}");
}

#[test]
fn testen_meldet_sich_an_der_attrappe_an() {
    let laufzeit = tokio::runtime::Runtime::new().unwrap();
    laufzeit.block_on(async {
        let server = attrappe::Attrappe::starte().await;
        let ausgabe = Command::new(env!("CARGO_BIN_EXE_quiver-abap"))
            .arg("testen")
            .env("QUIVER_ZUGAENGE", zugaenge(&server.adresse).to_string())
            .output()
            .expect("quiver-abap testen startet nicht");
        assert!(
            ausgabe.status.success(),
            "testen scheiterte: {}",
            String::from_utf8_lossy(&ausgabe.stderr)
        );
        let stdout = String::from_utf8_lossy(&ausgabe.stdout);
        assert!(stdout.contains("1 System(e) erreichbar"), "{stdout}");
        assert!(stdout.contains("angemeldet als"), "{stdout}");
        assert!(!stdout.contains(attrappe::PASSWORT), "{stdout}");
    });
}

#[test]
fn hilfe_und_version_nennen_das_programm() {
    let hilfe = Command::new(env!("CARGO_BIN_EXE_quiver-abap"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(hilfe.status.success());
    let text = String::from_utf8_lossy(&hilfe.stdout);
    assert!(text.contains("quiver-abap eintrag"), "{text}");
    assert!(text.contains("MCP über stdio"), "{text}");
    assert!(text.contains("QUIVER_ZUGAENGE"), "{text}");

    let version = Command::new(env!("CARGO_BIN_EXE_quiver-abap"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(version.status.success());
    assert!(String::from_utf8_lossy(&version.stdout).starts_with("abap "));
}
