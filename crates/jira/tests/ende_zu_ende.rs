// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Ende-zu-Ende: `quiver-jira` als **Kindprozess** über stdio (SPEC §7).
//!
//! So startet quiver das Programm wirklich: es setzt `QUIVER_ZUGAENGE` und
//! spricht JSON-RPC auf stdin/stdout. Die Attrappe antwortet auf `127.0.0.1`;
//! ein echtes Jira ist nie im Spiel.

mod attrappe;

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{json, Value};

/// Wie lange auf eine Antwort des Kindprozesses gewartet wird.
const GEDULD: Duration = Duration::from_secs(30);

/// Ein laufendes `quiver-jira` mit stdin/stdout verbunden.
struct Kind {
    kind: Child,
    stdin: Option<ChildStdin>,
    empfaenger: mpsc::Receiver<String>,
}

impl Kind {
    /// Das Programm starten; `umgebung` sind die Paare, die quiver setzen würde.
    fn starte(umgebung: &[(&'static str, String)]) -> Kind {
        let mut befehl = Command::new(env!("CARGO_BIN_EXE_quiver-jira"));
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
            .expect("quiver-jira startet nicht");

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
            json!("quiver-jira")
        );
        assert_eq!(handschlag["result"]["serverInfo"]["title"], json!("Jira"));
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
        assert!(ende.success(), "quiver-jira endete mit {ende:?}");
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
        "name": "firma",
        "werte": {
            "url": adresse,
            "email": attrappe::EMAIL_A,
            "api_token": attrappe::TOKEN_A,
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
        assert_eq!(namen.len(), 18, "unerwartete Werkzeuge: {namen:?}");
        for erwartet in [
            "jira_status",
            "jira_testen",
            "jira_projekte",
            "jira_suchen",
            "jira_vorgang",
            "jira_kommentare",
            "jira_uebergaenge",
            "jira_felder_zum_anlegen",
            "jira_filter_liste",
            "jira_filter",
            "jira_verknuepfungsarten",
            "jira_person_suchen",
            "jira_anlegen",
            "jira_aendern",
            "jira_uebergang",
            "jira_kommentieren",
            "jira_verknuepfen",
            "jira_profile",
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
            let veraendernd = matches!(
                name,
                "jira_anlegen"
                    | "jira_aendern"
                    | "jira_uebergang"
                    | "jira_kommentieren"
                    | "jira_verknuepfen"
            );
            assert_eq!(
                hinweise["readOnlyHint"].as_bool(),
                Some(!veraendernd),
                "{name}: {hinweise}"
            );
            assert_eq!(
                hinweise["destructiveHint"].as_bool(),
                Some(veraendernd),
                "{name}: {hinweise}"
            );
        }

        let (status, fehler) = kind.aufrufen(3, "jira_status", json!({}));
        assert!(!fehler, "{status}");
        assert!(status.contains("firma"), "{status}");
        assert!(status.contains("Schreiben an"), "{status}");
        assert!(!status.contains(attrappe::TOKEN_A), "{status}");

        // `jira_profile` kommt aus dem Kern und nennt die Sites.
        let (profil, fehler) = kind.aufrufen(30, "jira_profile", json!({}));
        assert!(!fehler, "{profil}");
        assert!(profil.contains("firma"), "{profil}");
        assert!(!profil.contains(attrappe::TOKEN_A), "{profil}");

        let (text, fehler) = kind.aufrufen(4, "jira_vorgang", json!({ "key": attrappe::VORGANG }));
        assert!(!fehler, "{text}");
        assert!(text.contains("Login kaputt"), "{text}");
        assert!(text.contains("Der Anmelden-Knopf tut nichts."), "{text}");
        assert!(!text.contains(attrappe::TOKEN_A), "{text}");

        let (text, fehler) = kind.aufrufen(5, "jira_suchen", json!({ "text": "Login" }));
        assert!(!fehler, "{text}");
        assert!(text.contains("ABC-1"), "{text}");

        let stderr = kind.beenden();
        assert!(
            !stderr.contains(attrappe::TOKEN_A),
            "Token auf stderr: {stderr}"
        );
    });
}

#[test]
fn kindprozess_legt_an_und_kommentiert() {
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
            "jira_anlegen",
            json!({ "projekt": "ABC", "vorgangstyp": "Bug", "titel": "Neuer Fehler" }),
        );
        assert!(!fehler, "{text}");
        assert!(text.contains("Vorgang angelegt: ABC-9"), "{text}");
        assert_eq!(server.aufzeichnung.anzahl_mit("issue", "POST"), 1);

        let (text, fehler) = kind.aufrufen(
            4,
            "jira_kommentieren",
            json!({ "key": attrappe::VORGANG, "text": "Hallo" }),
        );
        assert!(!fehler, "{text}");
        assert!(text.contains("angelegt"), "{text}");

        let stderr = kind.beenden();
        assert!(!stderr.contains(attrappe::TOKEN_A), "{stderr}");
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
            "jira_anlegen",
            json!({ "projekt": "ABC", "vorgangstyp": "Bug", "titel": "Neuer Fehler" }),
        );
        assert!(!fehler, "{text}");
        assert!(text.contains("Schreiben ist für firma aus"), "{text}");
        assert!(server.aufzeichnung.alle().is_empty());

        kind.beenden();
    });
}

#[test]
fn ohne_zugaenge_kommt_der_hinweis_und_kein_fehler() {
    let mut kind = Kind::starte(&[]);

    let werkzeuge = kind.begruessen();
    assert_eq!(namen(&werkzeuge).len(), 18);
    let (text, fehler) = kind.aufrufen(3, "jira_projekte", json!({}));
    assert!(!fehler, "{text}");
    assert!(text.contains("Zugang fehlt"), "{text}");
    assert!(text.contains("Tools → Jira"), "{text}");

    let (status, fehler) = kind.aufrufen(4, "jira_status", json!({}));
    assert!(!fehler, "{status}");
    assert!(status.contains("Zugang fehlt"), "{status}");

    kind.beenden();
}

#[test]
fn eintrag_enthaelt_mehrfach_mit_fuenf_feldern_und_token_link() {
    let ausgabe = Command::new(env!("CARGO_BIN_EXE_quiver-jira"))
        .args(["eintrag", "--befehl", "/pfad/quiver-jira"])
        .output()
        .expect("quiver-jira eintrag startet nicht");
    assert!(
        ausgabe.status.success(),
        "eintrag endete mit {:?}: {}",
        ausgabe.status,
        String::from_utf8_lossy(&ausgabe.stderr)
    );

    let eintrag: Value = serde_json::from_slice(&ausgabe.stdout).expect("kein JSON");
    assert_eq!(eintrag["id"], json!("jira"));
    assert_eq!(eintrag["titel"], json!("Jira"));
    assert_eq!(eintrag["version"], json!(env!("CARGO_PKG_VERSION")));
    assert_eq!(eintrag["herkunft"], json!("eigen"));
    assert_eq!(eintrag["laufzeit"]["art"], json!("stdio"));
    assert_eq!(eintrag["laufzeit"]["befehl"], json!("/pfad/quiver-jira"));
    // Der Eintrag selbst hat keine Felder und keine Umgebung …
    assert_eq!(eintrag["felder"].as_array().unwrap().len(), 0, "{eintrag}");
    assert_eq!(
        eintrag["laufzeit"]["umgebung"].as_object().unwrap().len(),
        0,
        "{eintrag}"
    );

    // … sondern eine Liste von Sites mit sieben Feldern.
    let mehrfach = &eintrag["mehrfach"];
    assert_eq!(mehrfach["titel"], json!("Sites"));
    assert_eq!(mehrfach["einzahl"], json!("Site"));
    let felder = mehrfach["felder"].as_array().unwrap();
    assert_eq!(felder.len(), 7, "{mehrfach}");
    let namen: Vec<&str> = felder
        .iter()
        .map(|feld| feld["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        namen,
        vec![
            "url",
            "email",
            "api_token",
            "nur",
            "unverschluesselt_erlauben",
            "ca_zertifikat",
            "schreiben"
        ]
    );

    assert_eq!(felder[0]["quelle"], json!("client"));
    assert_eq!(felder[0]["pflicht"], json!(true));
    assert_eq!(felder[0]["art"], json!("url"));
    assert!(felder[0]["beschreibung"]
        .as_str()
        .unwrap()
        .contains("atlassian.net"));
    assert_eq!(felder[1]["quelle"], json!("client"));
    assert_eq!(felder[1]["pflicht"], json!(true));
    assert_eq!(felder[1]["art"], json!("text"));
    // Das Token ist geheim und hat einen Hilfe-Link auf die Token-Seite.
    assert_eq!(felder[2]["quelle"], json!("geheim"));
    assert_eq!(felder[2]["art"], json!("passwort"));
    assert_eq!(felder[2]["pflicht"], json!(true));
    assert_eq!(felder[2]["link"]["text"], json!("API-Token erstellen"));
    assert_eq!(
        felder[2]["link"]["url"],
        json!("https://id.atlassian.com/manage-profile/security/api-tokens")
    );
    // Die Projektliste und der Schalter stehen danach.
    assert_eq!(felder[3]["name"], json!("nur"));
    assert_eq!(felder[3]["art"], json!("text"));
    assert_eq!(felder[3]["pflicht"], json!(false));
    assert!(felder[3]["beschreibung"].as_str().unwrap().contains("ABC"));
    // Härtung: http nur mit Schalter, eigenes CA-Zertifikat.
    assert_eq!(felder[4]["name"], json!("unverschluesselt_erlauben"));
    assert_eq!(felder[4]["art"], json!("schalter"));
    assert_eq!(felder[4]["standard"], json!("aus"));
    assert_eq!(felder[5]["name"], json!("ca_zertifikat"));
    assert_eq!(felder[5]["art"], json!("text"));
    assert_eq!(felder[6]["name"], json!("schreiben"));
    assert_eq!(felder[6]["art"], json!("schalter"));
    assert_eq!(felder[6]["standard"], json!("aus"));
    assert_eq!(felder[6]["pflicht"], json!(false));
}

#[test]
fn testen_ohne_zugaenge_endet_mit_fehler_und_nennt_die_variable() {
    let ausgabe = Command::new(env!("CARGO_BIN_EXE_quiver-jira"))
        .arg("testen")
        .env_remove("QUIVER_ZUGAENGE")
        .output()
        .expect("quiver-jira testen startet nicht");
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
        let ausgabe = Command::new(env!("CARGO_BIN_EXE_quiver-jira"))
            .arg("testen")
            .env(
                "QUIVER_ZUGAENGE",
                zugaenge(&server.adresse, true).to_string(),
            )
            .output()
            .expect("quiver-jira testen startet nicht");
        assert!(
            ausgabe.status.success(),
            "testen scheiterte: {}",
            String::from_utf8_lossy(&ausgabe.stderr)
        );
        let stdout = String::from_utf8_lossy(&ausgabe.stdout);
        assert!(stdout.contains("1 Site(s) erreichbar"), "{stdout}");
        assert!(stdout.contains(attrappe::BENUTZER), "{stdout}");
        assert!(!stdout.contains(attrappe::TOKEN_A), "{stdout}");
    });
}

#[test]
fn hilfe_und_version_nennen_das_programm() {
    let hilfe = Command::new(env!("CARGO_BIN_EXE_quiver-jira"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(hilfe.status.success());
    let text = String::from_utf8_lossy(&hilfe.stdout);
    assert!(text.contains("quiver-jira eintrag"), "{text}");
    assert!(text.contains("MCP über stdio"), "{text}");
    assert!(text.contains("QUIVER_ZUGAENGE"), "{text}");

    let version = Command::new(env!("CARGO_BIN_EXE_quiver-jira"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(version.status.success());
    assert!(String::from_utf8_lossy(&version.stdout).starts_with("jira "));
}
