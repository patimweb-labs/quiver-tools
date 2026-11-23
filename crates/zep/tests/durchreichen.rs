// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Ende-zu-Ende: `quiver-zep` als **Kindprozess** über stdio (Aufgabe 06).
//!
//! So startet quiver das Programm wirklich: es setzt die Umgebung aus den
//! Feldern des Bibliothekseintrags (`ZEP_URL`, `ZEP_BENUTZER`, `ZEP_PASSWORT`)
//! und spricht JSON-RPC auf stdin/stdout. Der ZEP-MCP ist die Attrappe auf
//! `127.0.0.1`; `ZEP_TRESOR=speicher` hält den Test vom echten Schlüsselbund
//! fern, `ZEP_ZUSTAND` lenkt den Sperrzustand in ein temporäres Verzeichnis.

mod attrappe;

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use attrappe::{Attrappe, PASSWORT};

/// Wie lange auf eine Antwort des Kindprozesses gewartet wird.
const GEDULD: Duration = Duration::from_secs(60);

/// Ein laufendes `quiver-zep` mit stdin/stdout verbunden.
struct Kind {
    kind: Child,
    stdin: Option<ChildStdin>,
    empfaenger: mpsc::Receiver<String>,
    fehler: std::sync::Arc<std::sync::Mutex<String>>,
}

impl Kind {
    fn starte(umgebung: &[(&str, &str)]) -> Kind {
        let mut befehl = Command::new(env!("CARGO_BIN_EXE_quiver-zep"));
        // Nichts aus der Umgebung des Testlaufs soll hineinspielen.
        for name in [
            "ZEP_URL",
            "ZEP_BENUTZER",
            "ZEP_PASSWORT",
            "ZEP_TRESOR",
            "ZEP_ZUSTAND",
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
            .expect("quiver-zep startet nicht");

        let stdin = kind.stdin.take().unwrap();
        let stdout = kind.stdout.take().unwrap();
        let stderr = kind.stderr.take().unwrap();
        let fehler: std::sync::Arc<std::sync::Mutex<String>> = Default::default();
        {
            let fehler = std::sync::Arc::clone(&fehler);
            std::thread::spawn(move || {
                for zeile in BufReader::new(stderr).lines().map_while(Result::ok) {
                    if let Ok(mut fach) = fehler.lock() {
                        fach.push_str(&zeile);
                        fach.push('\n');
                    }
                }
            });
        }
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
            fehler,
        }
    }

    fn sende(&mut self, wert: Value) {
        let stdin = self.stdin.as_mut().expect("stdin ist schon geschlossen");
        writeln!(stdin, "{wert}").unwrap();
        stdin.flush().unwrap();
    }

    /// Die nächste Zeile, die als JSON lesbar ist.
    fn zeile(&self) -> Value {
        loop {
            let zeile = self
                .empfaenger
                .recv_timeout(GEDULD)
                .expect("keine Zeile vom Kindprozess");
            if let Ok(wert) = serde_json::from_str::<Value>(&zeile) {
                return wert;
            }
        }
    }

    fn antwort(&self, id: u64) -> Value {
        loop {
            let wert = self.zeile();
            if wert.get("id").and_then(Value::as_u64) == Some(id) {
                return wert;
            }
        }
    }

    /// Auf eine Meldung (ohne id) mit dieser Methode warten.
    fn warte_auf(&self, methode: &str) -> Value {
        let grenze = Instant::now() + GEDULD;
        while Instant::now() < grenze {
            let wert = self.zeile();
            if wert.get("method").and_then(Value::as_str) == Some(methode) {
                return wert;
            }
        }
        panic!("keine Meldung „{methode}“ vom Kindprozess");
    }

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
            json!("quiver-zep")
        );
        assert_eq!(handschlag["result"]["serverInfo"]["title"], json!("ZEP"));
        assert_eq!(
            handschlag["result"]["capabilities"]["tools"]["listChanged"],
            json!(true)
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

    fn aufrufen(&mut self, id: u64, name: &str) -> Value {
        self.sende(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": { "name": name, "arguments": {} }
        }));
        self.antwort(id)
    }

    fn stderr(&self) -> String {
        self.fehler
            .lock()
            .map(|fach| fach.clone())
            .unwrap_or_default()
    }

    fn beenden(mut self) -> String {
        drop(self.stdin.take());
        let ende = self.kind.wait().unwrap();
        assert!(ende.success(), "quiver-zep endete mit {ende:?}");
        // Dem Leser einen Moment, bis er das Dateiende gesehen hat.
        std::thread::sleep(Duration::from_millis(100));
        self.stderr()
    }
}

/// Ein Text aus einer Werkzeug-Antwort.
fn text(antwort: &Value) -> String {
    antwort["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

#[test]
fn kindprozess_reicht_die_werkzeuge_durch_und_meldet_list_changed() {
    let laufzeit = tokio::runtime::Runtime::new().unwrap();
    let mock = laufzeit.block_on(Attrappe::starte());
    let ordner = tempfile::tempdir().unwrap();
    let zustand = ordner.path().display().to_string();

    let mut kind = Kind::starte(&[
        ("ZEP_URL", mock.mcp_adresse().as_str()),
        ("ZEP_BENUTZER", attrappe::BENUTZER),
        ("ZEP_PASSWORT", PASSWORT),
        ("ZEP_TRESOR", "speicher"),
        ("ZEP_ZUSTAND", &zustand),
    ]);

    // 1. Die Werkzeuge sind die von ZEP – plus zep_profile.
    let namen = kind.begruessen();
    if namen.len() < 4 {
        eprintln!("stderr: {}", kind.stderr());
    }
    for erwartet in [
        "zep_projekte",
        "zep_zeiten",
        "zep_umschalten",
        "zep_profile",
    ] {
        assert!(
            namen.contains(&erwartet.to_string()),
            "{erwartet} fehlt: {namen:?}"
        );
    }
    // Die Anmeldung lief im Kindprozess über die Routine.
    assert_eq!(mock.login_posts(), 1, "die Routine meldet einmal an");

    // 2. Ein Aufruf geht durch: Text unverändert.
    let aufruf = kind.aufrufen(3, "zep_projekte");
    assert_eq!(text(&aufruf), "Projekte der Attrappe: Neoimpulse, Quiver.");
    assert_eq!(aufruf["result"]["isError"], json!(false));

    // 3. ZEP ändert seine Werkzeugliste: das Kind meldet es weiter …
    let umschalten = kind.aufrufen(4, "zep_umschalten");
    assert!(text(&umschalten).contains("Umgeschaltet"));
    let meldung = kind.warte_auf("notifications/tools/list_changed");
    assert_eq!(meldung["jsonrpc"], json!("2.0"));

    // … und liefert anschließend das neue Werkzeug.
    kind.sende(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/list",
        "params": {}
    }));
    let liste = kind.antwort(5);
    let werkzeuge = liste["result"]["tools"].as_array().unwrap();
    let namen: Vec<String> = werkzeuge
        .iter()
        .map(|werkzeug| werkzeug["name"].as_str().unwrap().to_string())
        .collect();
    assert!(namen.contains(&"zep_kunden".to_string()), "{namen:?}");

    // Die Annotationen kommen unverändert von ZEP: lesend bleibt lesend.
    let annotations = |name: &str| -> Value {
        werkzeuge
            .iter()
            .find(|werkzeug| werkzeug["name"] == json!(name))
            .map(|werkzeug| werkzeug["annotations"].clone())
            .unwrap_or(Value::Null)
    };
    assert_eq!(annotations("zep_projekte")["readOnlyHint"], json!(true));
    assert_eq!(annotations("zep_projekte")["destructiveHint"], json!(false));
    assert_eq!(annotations("zep_kunden")["readOnlyHint"], json!(true));
    assert_eq!(
        annotations("zep_umschalten")["destructiveHint"],
        json!(true)
    );
    assert_eq!(annotations("zep_profile")["readOnlyHint"], json!(true));

    // 4. zep_profile zeigt den Zugang ohne Geheimnis.
    let profil = kind.aufrufen(6, "zep_profile");
    let profil_text = text(&profil);
    assert!(profil_text.contains("standard"), "{profil_text}");
    assert!(profil_text.contains(attrappe::BENUTZER), "{profil_text}");
    assert!(!profil_text.contains(PASSWORT), "{profil_text}");

    let stderr = kind.beenden();
    assert!(!stderr.contains(PASSWORT), "Passwort auf stderr: {stderr}");
    assert!(
        !stderr.contains("rt-"),
        "Refresh-Token auf stderr: {stderr}"
    );
}

#[test]
fn ohne_umgebung_kommt_der_hinweis_und_kein_fehler() {
    let mut kind = Kind::starte(&[]);
    let namen = kind.begruessen();
    assert_eq!(namen, vec!["zep_profile".to_string()]);

    let aufruf = kind.aufrufen(3, "zep_projekte");
    let text = text(&aufruf);
    assert!(text.contains("Zugang fehlt"), "{text}");
    assert!(text.contains("Tools → ZEP"), "{text}");
    assert_eq!(aufruf["result"]["isError"], json!(false));

    kind.beenden();
}

#[test]
fn eintrag_liefert_den_bibliothekseintrag_als_json() {
    let ausgabe = Command::new(env!("CARGO_BIN_EXE_quiver-zep"))
        .args(["eintrag", "--befehl", "/pfad/quiver-zep"])
        .output()
        .expect("quiver-zep eintrag startet nicht");
    assert!(
        ausgabe.status.success(),
        "eintrag endete mit {:?}: {}",
        ausgabe.status,
        String::from_utf8_lossy(&ausgabe.stderr)
    );

    let eintrag: Value = serde_json::from_slice(&ausgabe.stdout).expect("kein JSON");
    assert_eq!(eintrag["id"], json!("zep"));
    assert_eq!(eintrag["titel"], json!("ZEP"));
    assert_eq!(eintrag["version"], json!(env!("CARGO_PKG_VERSION")));
    assert_eq!(eintrag["herkunft"], json!("eigen"));
    assert_eq!(eintrag["laufzeit"]["art"], json!("stdio"));
    assert_eq!(eintrag["laufzeit"]["befehl"], json!("/pfad/quiver-zep"));
    assert_eq!(eintrag["laufzeit"]["umgebung"]["ZEP_URL"], json!("${url}"));
    assert_eq!(
        eintrag["laufzeit"]["umgebung"]["ZEP_BENUTZER"],
        json!("${benutzer}")
    );
    assert_eq!(
        eintrag["laufzeit"]["umgebung"]["ZEP_PASSWORT"],
        json!("${passwort}")
    );
    assert_eq!(eintrag["felder"].as_array().unwrap().len(), 3);
    assert_eq!(eintrag["felder"][0]["name"], json!("url"));
    assert_eq!(eintrag["felder"][0]["quelle"], json!("client"));
    assert_eq!(eintrag["felder"][1]["name"], json!("benutzer"));
    assert_eq!(eintrag["felder"][1]["quelle"], json!("client"));
    assert_eq!(eintrag["felder"][2]["name"], json!("passwort"));
    assert_eq!(eintrag["felder"][2]["quelle"], json!("geheim"));
    assert!(eintrag.get("einrichtung").is_none(), "{eintrag}");
}

#[test]
fn testen_ohne_umgebung_endet_mit_fehler_und_nennt_die_variablen() {
    let ausgabe = Command::new(env!("CARGO_BIN_EXE_quiver-zep"))
        .arg("testen")
        .env_remove("ZEP_URL")
        .env_remove("ZEP_BENUTZER")
        .env_remove("ZEP_PASSWORT")
        .output()
        .expect("quiver-zep testen startet nicht");
    assert_eq!(ausgabe.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&ausgabe.stderr);
    assert!(stderr.contains("Zugang fehlt"), "{stderr}");
    assert!(stderr.contains("ZEP_URL"), "{stderr}");
    assert!(stderr.contains("ZEP_PASSWORT"), "{stderr}");
}

#[test]
fn testen_und_status_gegen_die_attrappe() {
    let laufzeit = tokio::runtime::Runtime::new().unwrap();
    let mock = laufzeit.block_on(Attrappe::starte());
    let ordner = tempfile::tempdir().unwrap();
    let zustand = ordner.path().display().to_string();
    let umgebung = [
        ("ZEP_URL", mock.mcp_adresse()),
        ("ZEP_BENUTZER", attrappe::BENUTZER.to_string()),
        ("ZEP_PASSWORT", PASSWORT.to_string()),
        ("ZEP_TRESOR", "speicher".to_string()),
        ("ZEP_ZUSTAND", zustand),
    ];

    // `status` vor der Anmeldung: Exit 1.
    let status = Command::new(env!("CARGO_BIN_EXE_quiver-zep"))
        .arg("status")
        .envs(umgebung.clone())
        .output()
        .expect("quiver-zep status startet nicht");
    assert_eq!(status.status.code(), Some(1));

    // `testen` meldet sich an und sagt, was ZEP anbietet.
    let testen = Command::new(env!("CARGO_BIN_EXE_quiver-zep"))
        .arg("testen")
        .envs(umgebung.clone())
        .output()
        .expect("quiver-zep testen startet nicht");
    assert_eq!(
        testen.status.code(),
        Some(0),
        "testen scheiterte: {}",
        String::from_utf8_lossy(&testen.stderr)
    );
    let meldung = String::from_utf8_lossy(&testen.stdout);
    assert!(meldung.contains("Angemeldet als"), "{meldung}");
    assert!(meldung.contains("Werkzeug"), "{meldung}");
    assert!(!meldung.contains(PASSWORT), "{meldung}");
    assert_eq!(mock.login_posts(), 1);

    // `anmelden` ist der Knopf: die Routine läuft einmal und meldet die Schritte.
    let anmelden = Command::new(env!("CARGO_BIN_EXE_quiver-zep"))
        .arg("anmelden")
        .envs(umgebung.clone())
        .output()
        .expect("quiver-zep anmelden startet nicht");
    assert_eq!(
        anmelden.status.code(),
        Some(0),
        "anmelden scheiterte: {}",
        String::from_utf8_lossy(&anmelden.stderr)
    );
    let bericht = String::from_utf8_lossy(&anmelden.stdout);
    assert!(bericht.contains("Anmeldeformular absenden"), "{bericht}");
    assert!(bericht.contains("Zustimmungsseite"), "{bericht}");
    assert!(!bericht.contains(PASSWORT), "{bericht}");
    assert_eq!(mock.login_posts(), 2);

    // `routine-pruefen` meldet jeden Schritt und endet mit 0.
    let pruefen = Command::new(env!("CARGO_BIN_EXE_quiver-zep"))
        .arg("routine-pruefen")
        .envs(umgebung.clone())
        .output()
        .expect("quiver-zep routine-pruefen startet nicht");
    assert_eq!(
        pruefen.status.code(),
        Some(0),
        "routine-pruefen scheiterte: {}",
        String::from_utf8_lossy(&pruefen.stderr)
    );
    let bericht = String::from_utf8_lossy(&pruefen.stdout);
    assert!(bericht.contains("Anmeldeformular absenden"), "{bericht}");
    assert!(bericht.contains("Client-Registrierung"), "{bericht}");
    assert!(!bericht.contains(PASSWORT), "{bericht}");
    assert!(!bericht.contains("rt-"), "{bericht}");
}
