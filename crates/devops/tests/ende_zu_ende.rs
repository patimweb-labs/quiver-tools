// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Ende-zu-Ende: `quiver-devops` als **Kindprozess** über stdio (SPEC §7).
//!
//! So startet quiver das Programm wirklich: es setzt `QUIVER_ZUGAENGE` und
//! spricht JSON-RPC auf stdin/stdout. Die Attrappe antwortet auf `127.0.0.1`;
//! ein echtes Azure DevOps ist nie im Spiel.

mod attrappe;

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{json, Value};

/// Wie lange auf eine Antwort des Kindprozesses gewartet wird.
const GEDULD: Duration = Duration::from_secs(30);

/// Ein laufendes `quiver-devops` mit stdin/stdout verbunden.
struct Kind {
    kind: Child,
    stdin: Option<ChildStdin>,
    empfaenger: mpsc::Receiver<String>,
}

impl Kind {
    /// Das Programm starten; `umgebung` sind die Paare, die quiver setzen würde.
    fn starte(umgebung: &[(&'static str, String)]) -> Kind {
        let mut befehl = Command::new(env!("CARGO_BIN_EXE_quiver-devops"));
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
            .expect("quiver-devops startet nicht");

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
            json!("quiver-devops")
        );
        assert_eq!(handschlag["result"]["serverInfo"]["title"], json!("DevOps"));
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

    /// Ein Werkzeug aufrufen und Text plus Fehlerkennzeichen zurückgeben.
    fn aufrufen(&mut self, id: u64, name: &str, argumente: Value) -> (String, bool) {
        self.sende(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": { "name": name, "arguments": argumente }
        }));
        let antwort = self.antwort(id);
        let fehler = antwort.get("error").is_some()
            || antwort["result"]["isError"].as_bool().unwrap_or(false);
        let text = antwort["result"]["content"][0]["text"]
            .as_str()
            .map(str::to_string)
            .or_else(|| antwort["error"]["message"].as_str().map(str::to_string))
            .unwrap_or_else(|| panic!("kein Text in der Antwort: {antwort}"));
        (text, fehler)
    }

    /// Das Programm beenden und stderr einsammeln.
    fn beenden(mut self) -> String {
        drop(self.stdin.take());
        let ende = self.kind.wait().unwrap();
        assert!(ende.success(), "quiver-devops endete mit {ende:?}");
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
fn zugaenge(adresse: &str, schreiben: bool) -> Value {
    json!([{
        "name": "beispiel",
        "werte": {
            "url": adresse,
            "projekt": attrappe::PROJEKT,
            "pat": attrappe::PAT_A,
            "schreiben": if schreiben { "an" } else { "aus" },
        }
    }])
}

#[test]
fn kindprozess_listet_alle_werkzeuge_und_liest() {
    let laufzeit = tokio::runtime::Runtime::new().unwrap();
    laufzeit.block_on(async {
        let server = attrappe::Attrappe::starte().await;
        let umgebung = [(
            "QUIVER_ZUGAENGE",
            zugaenge(&server.adresse, true).to_string(),
        )];

        let mut kind = Kind::starte(&umgebung);
        let werkzeuge = kind.begruessen();
        let namen = namen(&werkzeuge);
        assert_eq!(namen.len(), 27, "unerwartete Werkzeuge: {namen:?}");
        for erwartet in [
            "devops_status",
            "devops_testen",
            "devops_projekte",
            "devops_workitems_suchen",
            "devops_workitem",
            "devops_workitem_kommentare",
            "devops_workitem_typen",
            "devops_iterationen",
            "devops_iteration_workitems",
            "devops_repos",
            "devops_branches",
            "devops_pull_requests",
            "devops_pull_request",
            "devops_pull_request_threads",
            "devops_pipelines",
            "devops_laeufe",
            "devops_lauf",
            "devops_lauf_log",
            "devops_workitem_anlegen",
            "devops_workitem_aendern",
            "devops_workitem_kommentieren",
            "devops_workitem_verknuepfen",
            "devops_pull_request_anlegen",
            "devops_pull_request_kommentieren",
            "devops_pull_request_abstimmen",
            "devops_pipeline_starten",
            "devops_profile",
        ] {
            assert!(namen.contains(&erwartet.to_string()), "{erwartet} fehlt");
        }

        // Jedes Schema ist ein Objekt – sonst lehnen manche Clients die Liste ab.
        for werkzeug in &werkzeuge {
            let schema = &werkzeug["inputSchema"];
            assert_eq!(
                schema["type"], "object",
                "{}: kein Objekt-Schema: {schema}",
                werkzeug["name"]
            );
        }

        // Die Annotationen stimmen: lesend oder verändernd.
        for werkzeug in &werkzeuge {
            let name = werkzeug["name"].as_str().unwrap();
            let hinweise = &werkzeug["annotations"];
            let verändernd = matches!(
                name,
                "devops_workitem_anlegen"
                    | "devops_workitem_aendern"
                    | "devops_workitem_kommentieren"
                    | "devops_workitem_verknuepfen"
                    | "devops_pull_request_anlegen"
                    | "devops_pull_request_kommentieren"
                    | "devops_pull_request_abstimmen"
                    | "devops_pipeline_starten"
            );
            assert_eq!(
                hinweise["readOnlyHint"].as_bool(),
                Some(!verändernd),
                "{name}: {hinweise}"
            );
            assert_eq!(
                hinweise["destructiveHint"].as_bool(),
                Some(verändernd),
                "{name}: {hinweise}"
            );
        }

        let (status, fehler) = kind.aufrufen(3, "devops_status", json!({}));
        assert!(!fehler, "{status}");
        assert!(status.contains("beispiel"), "{status}");
        assert!(status.contains("Schreiben an"), "{status}");
        assert!(!status.contains(attrappe::PAT_A), "{status}");

        // `devops_profile` kommt aus dem Kern und nennt die Organisationen.
        let (profil, fehler) = kind.aufrufen(30, "devops_profile", json!({}));
        assert!(!fehler, "{profil}");
        assert!(profil.contains("beispiel"), "{profil}");
        assert!(!profil.contains(attrappe::PAT_A), "{profil}");

        let (text, fehler) = kind.aufrufen(4, "devops_workitem", json!({ "id": attrappe::ITEM }));
        assert!(!fehler, "{text}");
        assert!(text.contains("Login kaputt"), "{text}");
        assert!(!text.contains(attrappe::PAT_A), "{text}");

        let (text, fehler) = kind.aufrufen(5, "devops_pipelines", json!({}));
        assert!(!fehler, "{text}");
        assert!(text.contains("projekt-ci"), "{text}");

        let stderr = kind.beenden();
        assert!(
            !stderr.contains(attrappe::PAT_A),
            "PAT auf stderr: {stderr}"
        );
    });
}

#[test]
fn kindprozess_legt_ein_arbeitsitem_an() {
    let laufzeit = tokio::runtime::Runtime::new().unwrap();
    laufzeit.block_on(async {
        let server = attrappe::Attrappe::starte().await;
        let umgebung = [(
            "QUIVER_ZUGAENGE",
            zugaenge(&server.adresse, true).to_string(),
        )];

        let mut kind = Kind::starte(&umgebung);
        kind.begruessen();

        let (text, fehler) = kind.aufrufen(
            3,
            "devops_workitem_anlegen",
            json!({ "typ": "Bug", "titel": "Neuer Fehler", "beschreibung": "<div>x</div>" }),
        );
        assert!(!fehler, "{text}");
        assert!(text.contains("Arbeitsitem angelegt"), "{text}");
        assert_eq!(server.aufzeichnung.anzahl_mit("wit/workitems", "POST"), 1);

        let (text, fehler) = kind.aufrufen(
            4,
            "devops_workitem_aendern",
            json!({ "id": attrappe::ITEM, "status": "Closed" }),
        );
        assert!(!fehler, "{text}");
        assert!(text.contains("System.State → Closed"), "{text}");

        let stderr = kind.beenden();
        assert!(!stderr.contains(attrappe::PAT_A), "{stderr}");
    });
}

#[test]
fn ohne_schreiben_antwortet_der_kindprozess_ohne_anfrage() {
    let laufzeit = tokio::runtime::Runtime::new().unwrap();
    laufzeit.block_on(async {
        let server = attrappe::Attrappe::starte().await;
        let umgebung = [(
            "QUIVER_ZUGAENGE",
            zugaenge(&server.adresse, false).to_string(),
        )];

        let mut kind = Kind::starte(&umgebung);
        kind.begruessen();

        let (text, fehler) = kind.aufrufen(
            3,
            "devops_workitem_anlegen",
            json!({ "typ": "Bug", "titel": "Neuer Fehler" }),
        );
        assert!(!fehler, "{text}");
        assert!(text.contains("Schreiben ist für beispiel aus"), "{text}");
        assert!(server.aufzeichnung.alle().is_empty());

        kind.beenden();
    });
}

#[test]
fn ohne_zugaenge_kommt_der_hinweis_und_kein_fehler() {
    let mut kind = Kind::starte(&[]);

    let werkzeuge = kind.begruessen();
    assert_eq!(namen(&werkzeuge).len(), 27);
    let (text, fehler) = kind.aufrufen(3, "devops_repos", json!({}));
    assert!(!fehler, "{text}");
    assert!(text.contains("Zugang fehlt"), "{text}");
    assert!(text.contains("Tools → DevOps"), "{text}");

    let (status, fehler) = kind.aufrufen(4, "devops_status", json!({}));
    assert!(!fehler, "{status}");
    assert!(status.contains("Zugang fehlt"), "{status}");

    kind.beenden();
}

#[test]
fn eintrag_enthaelt_mehrfach_mit_vier_feldern_und_pat_link() {
    let ausgabe = Command::new(env!("CARGO_BIN_EXE_quiver-devops"))
        .args(["eintrag", "--befehl", "/pfad/quiver-devops"])
        .output()
        .expect("quiver-devops eintrag startet nicht");
    assert!(
        ausgabe.status.success(),
        "eintrag endete mit {:?}: {}",
        ausgabe.status,
        String::from_utf8_lossy(&ausgabe.stderr)
    );

    let eintrag: Value = serde_json::from_slice(&ausgabe.stdout).expect("kein JSON");
    assert_eq!(eintrag["id"], json!("devops"));
    assert_eq!(eintrag["titel"], json!("DevOps"));
    assert_eq!(eintrag["version"], json!(env!("CARGO_PKG_VERSION")));
    assert_eq!(eintrag["herkunft"], json!("eigen"));
    assert_eq!(eintrag["laufzeit"]["art"], json!("stdio"));
    assert_eq!(eintrag["laufzeit"]["befehl"], json!("/pfad/quiver-devops"));
    // Der Eintrag selbst hat keine Felder und keine Umgebung …
    assert_eq!(eintrag["felder"].as_array().unwrap().len(), 0, "{eintrag}");
    assert_eq!(
        eintrag["laufzeit"]["umgebung"].as_object().unwrap().len(),
        0,
        "{eintrag}"
    );

    // … sondern eine Liste von Organisationen mit sechs Feldern.
    let mehrfach = &eintrag["mehrfach"];
    assert_eq!(mehrfach["titel"], json!("Organisationen"));
    assert_eq!(mehrfach["einzahl"], json!("Organisation"));
    let felder = mehrfach["felder"].as_array().unwrap();
    assert_eq!(felder.len(), 6, "{mehrfach}");
    let namen: Vec<&str> = felder
        .iter()
        .map(|feld| feld["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        namen,
        vec![
            "url",
            "projekt",
            "pat",
            "unverschluesselt_erlauben",
            "ca_zertifikat",
            "schreiben"
        ]
    );

    assert_eq!(felder[0]["quelle"], json!("client"));
    assert_eq!(felder[0]["pflicht"], json!(true));
    assert!(felder[0]["beschreibung"]
        .as_str()
        .unwrap()
        .contains("dev.azure.com"));
    assert_eq!(felder[0]["art"], json!("url"));
    assert_eq!(felder[1]["quelle"], json!("client"));
    assert_eq!(felder[1]["pflicht"], json!(false));
    assert_eq!(felder[1]["titel"], json!("Standardprojekt"));
    assert_eq!(felder[1]["art"], json!("text"));
    // Der PAT ist geheim und hat einen Hilfe-Link auf die Token-Seite.
    assert_eq!(felder[2]["quelle"], json!("geheim"));
    assert_eq!(felder[2]["art"], json!("passwort"));
    assert_eq!(felder[2]["pflicht"], json!(true));
    assert_eq!(felder[2]["link"]["text"], json!("PAT erstellen"));
    assert_eq!(
        felder[2]["link"]["url"],
        json!("${url}/_usersSettings/tokens")
    );
    // Härtung: http nur mit Schalter, eigenes CA-Zertifikat.
    assert_eq!(felder[3]["name"], json!("unverschluesselt_erlauben"));
    assert_eq!(felder[3]["art"], json!("schalter"));
    assert_eq!(felder[3]["standard"], json!("aus"));
    assert_eq!(felder[4]["name"], json!("ca_zertifikat"));
    assert_eq!(felder[4]["art"], json!("text"));
    // Der Schalter „schreiben“ steht am Ende und ist im Zweifel aus.
    assert_eq!(felder[5]["name"], json!("schreiben"));
    assert_eq!(felder[5]["art"], json!("schalter"));
    assert_eq!(felder[5]["standard"], json!("aus"));
    assert_eq!(felder[5]["pflicht"], json!(false));
    assert!(felder[5]["beschreibung"]
        .as_str()
        .unwrap()
        .contains("Pipelines starten"));
}

#[test]
fn testen_ohne_zugaenge_endet_mit_fehler_und_nennt_die_variable() {
    let ausgabe = Command::new(env!("CARGO_BIN_EXE_quiver-devops"))
        .arg("testen")
        .env_remove("QUIVER_ZUGAENGE")
        .output()
        .expect("quiver-devops testen startet nicht");
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
        let ausgabe = Command::new(env!("CARGO_BIN_EXE_quiver-devops"))
            .arg("testen")
            .env(
                "QUIVER_ZUGAENGE",
                zugaenge(&server.adresse, true).to_string(),
            )
            .output()
            .expect("quiver-devops testen startet nicht");
        assert!(
            ausgabe.status.success(),
            "testen scheiterte: {}",
            String::from_utf8_lossy(&ausgabe.stderr)
        );
        let stdout = String::from_utf8_lossy(&ausgabe.stdout);
        assert!(stdout.contains("1 Organisation(en) erreichbar"), "{stdout}");
        assert!(stdout.contains(attrappe::BENUTZER), "{stdout}");
        assert!(!stdout.contains(attrappe::PAT_A), "{stdout}");

        // `connectionData` ist Vorschau: mit `7.1` antwortet der Dienst mit
        // `400` und `testen` scheitert.
        let aufrufe = server.aufzeichnung.zu("connectionData");
        assert_eq!(aufrufe.len(), 1, "{:?}", aufrufe.len());
        assert_eq!(aufrufe[0].api_version(), Some(attrappe::VORSCHAU_VERSION));
    });
}

#[test]
fn hilfe_und_version_nennen_das_programm() {
    let hilfe = Command::new(env!("CARGO_BIN_EXE_quiver-devops"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(hilfe.status.success());
    let text = String::from_utf8_lossy(&hilfe.stdout);
    assert!(text.contains("quiver-devops eintrag"), "{text}");
    assert!(text.contains("MCP über stdio"), "{text}");
    assert!(text.contains("QUIVER_ZUGAENGE"), "{text}");

    let version = Command::new(env!("CARGO_BIN_EXE_quiver-devops"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(version.status.success());
    assert!(String::from_utf8_lossy(&version.stdout).starts_with("devops "));
}
