// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! quiver-ssh gegen den Test-SSH-Server: die Wege aus Aufgabe 05.
//!
//! Kein Test spricht mit einem echten Host. Die Attrappe läuft auf 127.0.0.1,
//! `HOME` und `QUIVER_SSH_DATEN` sind temp-Ordner; der Prozess bekommt keine
//! fremde Umgebung.

mod attrappe;

use quiver_ssh::pfade::{self, Pfade};
use quiver_ssh::verbindung::Sammlung;
use quiver_ssh::{Ssh, SshServer};
use quiver_tools_kern::Zugang;
use rmcp::model::CallToolResult;
use rmcp::ErrorData as McpError;
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::runtime::Runtime;

use crate::attrappe::{Attrappe, BENUTZER, PASSWORT, SUDO_PASSWORT};

/// Ein Testaufbau: Server, Pfade und der temp-Ordner, der alles am Leben hält.
struct Aufbau {
    server: SshServer,
    pfade: Pfade,
    temp: TempDir,
}

impl Aufbau {
    /// Ein Server mit den angegebenen Verbindungen; `None` heißt: kein
    /// `QUIVER_ZUGAENGE` gesetzt (leere Liste).
    fn neu(zugaenge: Option<&[Value]>) -> Aufbau {
        let temp = tempfile::tempdir().unwrap();
        let pfade = Pfade::neu(
            temp.path().join("quiver-tools/ssh"),
            Some(temp.path().join("home")),
        );
        let sammlung = match zugaenge {
            Some(zugaenge) => Value::Array(zugaenge.to_vec()).to_string(),
            None => String::new(),
        };
        let sammlung = Sammlung::mit_umgebung(|name| {
            (!sammlung.is_empty() && name == "QUIVER_ZUGAENGE").then(|| sammlung.clone())
        })
        .unwrap();
        let server = SshServer::mit(
            Zugang::mit_umgebung(&Ssh, |_| None),
            pfade.clone(),
            Ok(sammlung),
        );
        Aufbau {
            server,
            pfade,
            temp,
        }
    }

    /// Derselbe Aufbau mit anderen Verbindungen (z. B. ohne Anmelde-Passwort).
    fn mit(&self, zugaenge: &[Value]) -> SshServer {
        let json = Value::Array(zugaenge.to_vec()).to_string();
        let sammlung =
            Sammlung::mit_umgebung(|name| (name == "QUIVER_ZUGAENGE").then(|| json.clone()))
                .unwrap();
        SshServer::mit(
            Zugang::mit_umgebung(&Ssh, |_| None),
            self.pfade.clone(),
            Ok(sammlung),
        )
    }

    fn home(&self) -> std::path::PathBuf {
        self.temp.path().join("home")
    }

    fn schluessel(&self, name: &str) -> std::path::PathBuf {
        self.pfade.schluessel(name).unwrap()
    }
}

/// Ein Aufruf eines Werkzeugs.
///
/// Er läuft in einem eigenen Thread mit eigener Laufzeit: die Tests sind teils
/// selbst in `block_on`, und tokio darf nicht ineinander geschachtelt werden.
fn aufruf(server: &SshServer, name: &str, argumente: Value) -> Result<CallToolResult, McpError> {
    let objekt = argumente.as_object().cloned().unwrap_or_default();
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                let laufzeit = Runtime::new().unwrap();
                laufzeit.block_on(server.aufrufen(name, Some(objekt)))
            })
            .join()
            .expect("Aufruf-Thread ist abgestürzt")
    })
}

/// Der Text einer erfolgreichen Antwort.
fn text(ergebnis: CallToolResult) -> String {
    ergebnis.content[0].as_text().unwrap().text.clone()
}

/// Der Text eines Fehlers.
fn fehlertext(fehler: McpError) -> String {
    fehler.message.to_string()
}

/// Eine Verbindung mit Schlüssel einrichten (Passwort-Anmeldung).
fn einrichten(aufbau: &Aufbau, server: &Attrappe, name: &str) {
    let zugaenge = [attrappe::zugang(
        name,
        server.port,
        Some(PASSWORT),
        Some(SUDO_PASSWORT),
    )];
    let antwort = text(
        aufruf(
            &aufbau.mit(&zugaenge),
            "ssh_exec",
            json!({ "befehl": "id -un" }),
        )
        .expect("Einrichtung scheitert"),
    );
    assert!(antwort.contains("Schlüssel für"), "{antwort}");
    assert!(aufbau.schluessel(name).exists(), "kein Schlüssel angelegt");
}

#[test]
fn erste_anmeldung_mit_passwort_richtet_den_schluessel_ein() {
    let laufzeit = Runtime::new().unwrap();
    laufzeit.block_on(async {
        let server = Attrappe::starte().await;
        let aufbau = Aufbau::neu(None);
        let zugaenge = [attrappe::zugang(
            "web-01",
            server.port,
            Some(PASSWORT),
            Some(SUDO_PASSWORT),
        )];

        let antwort = text(
            aufruf(
                &aufbau.mit(&zugaenge),
                "ssh_exec",
                json!({ "befehl": "id -un" }),
            )
            .unwrap(),
        );

        // Die eigentliche Arbeit lief schon über die Schlüssel-Verbindung.
        assert!(antwort.contains("root"), "{antwort}");
        assert!(
            antwort.contains("Schlüssel für web-01 eingerichtet (SHA256:"),
            "{antwort}"
        );
        assert!(
            antwort.contains("Das Anmelde-Passwort wird nicht mehr gebraucht"),
            "{antwort}"
        );
        assert!(
            antwort.contains("Host-Schlüssel zum ersten Mal gesehen und gemerkt"),
            "{antwort}"
        );

        // Der Schlüssel liegt mit engen Rechten da, der Kommentar ist sprechend.
        let datei = aufbau.schluessel("web-01");
        assert!(datei.exists());
        let oeffentlich = std::fs::read_to_string(pfade::oeffentlich(&datei)).unwrap();
        assert!(oeffentlich.contains(" quiver-ssh web-01"), "{oeffentlich}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let rechte = std::fs::metadata(&datei).unwrap().permissions().mode() & 0o777;
            assert_eq!(rechte, 0o600);
        }

        // In authorized_keys steht genau ein Eintrag.
        let zeilen = server.zustand.autorisierte_zeilen();
        assert_eq!(zeilen.len(), 1, "{zeilen:?}");
        assert!(zeilen[0].ends_with(" quiver-ssh web-01"), "{}", zeilen[0]);
        assert_eq!(server.zustand.passwort_versuche(), 1);

        // Die zweite Verbindung braucht das Passwort nicht mehr.
        let ohne_passwort = [attrappe::zugang("web-01", server.port, None, None)];
        let zweite = text(
            aufruf(
                &aufbau.mit(&ohne_passwort),
                "ssh_exec",
                json!({ "befehl": "id -un" }),
            )
            .unwrap(),
        );
        assert!(zweite.contains("root"), "{zweite}");
        assert!(!zweite.contains("eingerichtet"), "{zweite}");
        // Kein zweiter Passwort-Versuch.
        assert_eq!(server.zustand.passwort_versuche(), 1);

        server.stoppen().await;
    });
}

#[test]
fn lehnt_der_server_den_schluessel_ab_wird_abgebrochen_und_aufgeraeumt() {
    let laufzeit = Runtime::new().unwrap();
    laufzeit.block_on(async {
        let server = Attrappe::starte().await;
        server.zustand.sperre_schluessel();
        let aufbau = Aufbau::neu(None);
        let zugaenge = [attrappe::zugang(
            "web-01",
            server.port,
            Some(PASSWORT),
            None,
        )];

        let fehler = fehlertext(
            aufruf(
                &aufbau.mit(&zugaenge),
                "ssh_exec",
                json!({ "befehl": "id -un" }),
            )
            .unwrap_err(),
        );

        assert!(fehler.contains("PubkeyAuthentication no"), "{fehler}");
        assert!(
            fehler.contains("Schlüsseldateien wurden wieder entfernt"),
            "{fehler}"
        );
        // Keine Schlüsseldatei übrig – auch keine vorläufige.
        let ziel = aufbau.schluessel("web-01");
        let vorlaeufig = aufbau.pfade.vorlaeufig("web-01").unwrap();
        assert!(!ziel.exists(), "{} liegt noch da", ziel.display());
        assert!(
            !vorlaeufig.exists(),
            "{} liegt noch da",
            vorlaeufig.display()
        );
        assert!(!pfade::oeffentlich(&ziel).exists());
        assert!(!pfade::oeffentlich(&vorlaeufig).exists());

        server.stoppen().await;
    });
}

#[test]
fn ein_vorhandener_schluessel_bleibt_bei_gescheiterter_neueinrichtung_erhalten() {
    let laufzeit = Runtime::new().unwrap();
    laufzeit.block_on(async {
        let server = Attrappe::starte().await;
        let aufbau = Aufbau::neu(None);
        einrichten(&aufbau, &server, "web-01");

        let alt = std::fs::read_to_string(aufbau.schluessel("web-01")).unwrap();
        server.zustand.sperre_schluessel();
        let zugaenge = [attrappe::zugang(
            "web-01",
            server.port,
            Some(PASSWORT),
            None,
        )];

        let fehler = fehlertext(
            aufruf(
                &aufbau.mit(&zugaenge),
                "ssh_schluessel_einrichten",
                json!({}),
            )
            .unwrap_err(),
        );
        assert!(fehler.contains("PubkeyAuthentication no"), "{fehler}");
        // Der alte Schlüssel ist unverändert.
        assert_eq!(
            std::fs::read_to_string(aufbau.schluessel("web-01")).unwrap(),
            alt
        );

        server.stoppen().await;
    });
}

#[test]
fn server_kennt_den_schluessel_nicht_mehr_kein_stiller_passwort_rueckfall() {
    let laufzeit = Runtime::new().unwrap();
    laufzeit.block_on(async {
        let server = Attrappe::starte().await;
        let aufbau = Aufbau::neu(None);
        einrichten(&aufbau, &server, "web-01");
        let versuche_vorher = server.zustand.passwort_versuche();

        // Server-Neuaufbau: der Schlüssel ist weg, das Anmelde-Passwort steht
        // weiterhin im quiver-Fenster.
        server.zustand.vergiss_schluessel();
        let zugaenge = [attrappe::zugang(
            "web-01",
            server.port,
            Some(PASSWORT),
            None,
        )];

        let fehler = fehlertext(
            aufruf(
                &aufbau.mit(&zugaenge),
                "ssh_exec",
                json!({ "befehl": "id -un" }),
            )
            .unwrap_err(),
        );
        assert!(
            fehler.contains("Schlüssel-Anmeldung an web-01 fehlgeschlagen"),
            "{fehler}"
        );
        assert!(fehler.contains("ssh_schluessel_einrichten"), "{fehler}");
        // Kein stiller Rückfall aufs Passwort.
        assert_eq!(server.zustand.passwort_versuche(), versuche_vorher);
        // Der Schlüssel bleibt liegen, damit die Neueinrichtung ihn ersetzen kann.
        assert!(aufbau.schluessel("web-01").exists());

        server.stoppen().await;
    });
}

#[test]
fn ein_geaenderter_host_schluessel_ist_ein_harter_fehler() {
    let laufzeit = Runtime::new().unwrap();
    laufzeit.block_on(async {
        let server = Attrappe::starte().await;
        let aufbau = Aufbau::neu(None);

        // Ein gespeicherter Schlüssel derselben Art, den der Server nicht hat.
        let bekannt = tempfile::tempdir().unwrap();
        let fremd = quiver_ssh::schluessel::erzeugen(&bekannt.path().join("fremd"), "alt").unwrap();
        std::fs::create_dir_all(aufbau.home().join(".ssh")).unwrap();
        let zeile = format!(
            "[127.0.0.1]:{} {}\n",
            server.port,
            fremd
                .zeile
                .split_whitespace()
                .take(2)
                .collect::<Vec<_>>()
                .join(" ")
        );
        std::fs::write(aufbau.home().join(".ssh/known_hosts"), &zeile).unwrap();

        let zugaenge = [attrappe::zugang(
            "web-01",
            server.port,
            Some(PASSWORT),
            None,
        )];
        let fehler = fehlertext(
            aufruf(
                &aufbau.mit(&zugaenge),
                "ssh_exec",
                json!({ "befehl": "id -un" }),
            )
            .unwrap_err(),
        );

        assert!(fehler.contains("hat sich geändert"), "{fehler}");
        assert!(fehler.contains("Alt (gespeichert):"), "{fehler}");
        assert!(fehler.contains("Neu (angeboten):"), "{fehler}");
        assert!(fehler.contains(&fremd.fingerprint), "{fehler}");
        // Die Antwort nennt Datei und Zeile, die der User löschen muss.
        assert!(
            fehler.contains(&aufbau.home().join(".ssh/known_hosts").display().to_string()),
            "{fehler}"
        );
        assert!(fehler.contains("Zeile 1"), "{fehler}");
        // Es kam gar nicht erst zur Anmeldung.
        assert_eq!(server.zustand.passwort_versuche(), 0);
        assert_eq!(server.zustand.pubkey_versuche(), 0);
        // Nichts wurde überschrieben.
        assert_eq!(
            std::fs::read_to_string(aufbau.home().join(".ssh/known_hosts")).unwrap(),
            zeile
        );

        quiver_ssh::schluessel::entfernen(&fremd);
        server.stoppen().await;
    });
}

#[test]
fn sudo_nutzt_das_eigene_passwort_und_zeigt_es_nie() {
    let laufzeit = Runtime::new().unwrap();
    laufzeit.block_on(async {
        let server = Attrappe::starte().await;
        let aufbau = Aufbau::neu(None);
        einrichten(&aufbau, &server, "web-01");

        let mit_sudo = text(
            aufruf(
                &aufbau.mit(&[attrappe::zugang(
                    "web-01",
                    server.port,
                    None,
                    Some(SUDO_PASSWORT),
                )]),
                "ssh_exec",
                json!({ "befehl": "id -un", "sudo": true }),
            )
            .unwrap(),
        );
        assert!(mit_sudo.contains("root"), "{mit_sudo}");
        assert!(mit_sudo.contains("$ sudo id -un"), "{mit_sudo}");
        assert!(!mit_sudo.contains(SUDO_PASSWORT), "{mit_sudo}");
        assert!(!mit_sudo.contains(PASSWORT), "{mit_sudo}");

        // Falsches sudo-Passwort: klare Meldung, kein Passwort im Text.
        let falsch = text(
            aufruf(
                &aufbau.mit(&[attrappe::zugang(
                    "web-01",
                    server.port,
                    None,
                    Some("falsch-0815"),
                )]),
                "ssh_exec",
                json!({ "befehl": "id -un", "sudo": true }),
            )
            .unwrap(),
        );
        assert!(falsch.contains("wurde abgelehnt"), "{falsch}");
        assert!(falsch.contains("Sorry, try again."), "{falsch}");
        assert!(!falsch.contains("falsch-0815"), "{falsch}");
        assert!(!falsch.contains(SUDO_PASSWORT), "{falsch}");

        // Kein sudo-Passwort hinterlegt: Hinweis statt Fehler.
        let fehlt = text(
            aufruf(
                &aufbau.mit(&[attrappe::zugang("web-01", server.port, None, None)]),
                "ssh_exec",
                json!({ "befehl": "id -un", "sudo": true }),
            )
            .unwrap(),
        );
        assert!(
            fehlt.contains("sudo-Passwort für web-01 fehlt – im quiver-Fenster eintragen"),
            "{fehlt}"
        );
        assert!(!fehlt.contains(SUDO_PASSWORT), "{fehlt}");

        server.stoppen().await;
    });
}

#[test]
fn list_upload_und_download_gehen_ueber_sftp() {
    let laufzeit = Runtime::new().unwrap();
    laufzeit.block_on(async {
        let server = Attrappe::starte().await;
        let aufbau = Aufbau::neu(None);
        einrichten(&aufbau, &server, "web-01");
        let lokal = tempfile::tempdir().unwrap();
        let ohne_passwort = aufbau.mit(&[attrappe::zugang_mit_ordner(
            "web-01",
            server.port,
            lokal.path().to_str().unwrap(),
        )]);

        std::fs::write(lokal.path().join("notiz.txt"), "Zeile eins\nZeile zwei\n").unwrap();

        let hoch = text(
            aufruf(
                &ohne_passwort,
                "ssh_upload",
                json!({ "lokal": "notiz.txt", "entfernt": "notiz.txt" }),
            )
            .unwrap(),
        );
        assert!(
            hoch.contains("nach notiz.txt auf web-01 hochgeladen"),
            "{hoch}"
        );
        assert!(hoch.contains("22 B"), "{hoch}");
        assert_eq!(
            std::fs::read_to_string(server.zustand.dateien().join("notiz.txt")).unwrap(),
            "Zeile eins\nZeile zwei\n"
        );

        let liste = text(aufruf(&ohne_passwort, "ssh_list", json!({ "pfad": "." })).unwrap());
        assert!(liste.contains("notiz.txt"), "{liste}");
        assert!(liste.contains("(1 Eintrag)"), "{liste}");

        let runter = text(
            aufruf(
                &ohne_passwort,
                "ssh_download",
                json!({ "entfernt": "notiz.txt", "lokal": "zurueck.txt" }),
            )
            .unwrap(),
        );
        assert!(runter.contains("heruntergeladen"), "{runter}");
        assert_eq!(
            std::fs::read_to_string(lokal.path().join("zurueck.txt")).unwrap(),
            "Zeile eins\nZeile zwei\n"
        );

        // Download überschreibt nie.
        let fehler = aufruf(
            &ohne_passwort,
            "ssh_download",
            json!({ "entfernt": "notiz.txt", "lokal": "zurueck.txt" }),
        )
        .unwrap_err();
        assert!(
            fehlertext(fehler).contains("nicht überschrieben"),
            "{}",
            fehlertext(
                aufruf(
                    &ohne_passwort,
                    "ssh_download",
                    json!({ "entfernt": "notiz.txt", "lokal": "zurueck.txt" })
                )
                .unwrap_err()
            )
        );

        // Aus dem Austauschordner bricht nichts aus.
        for (lokal, entfernt) in [
            ("/etc/passwd", "notiz.txt"),
            ("../raus.txt", "notiz.txt"),
            ("notiz.txt", "../raus.txt"),
            ("notiz.txt", "/etc/passwd"),
        ] {
            assert!(
                aufruf(
                    &ohne_passwort,
                    "ssh_upload",
                    json!({ "lokal": lokal, "entfernt": entfernt }),
                )
                .is_err(),
                "{lokal} → {entfernt}"
            );
        }

        server.stoppen().await;
    });
}

#[test]
fn status_zeigt_alles_ohne_geheimnis() {
    let laufzeit = Runtime::new().unwrap();
    laufzeit.block_on(async {
        let server = Attrappe::starte().await;
        let aufbau = Aufbau::neu(None);
        einrichten(&aufbau, &server, "web-01");

        let uebersicht = text(
            aufruf(
                &aufbau.mit(&[attrappe::zugang(
                    "web-01",
                    server.port,
                    None,
                    Some(SUDO_PASSWORT),
                )]),
                "ssh_status",
                json!({}),
            )
            .unwrap(),
        );
        assert!(uebersicht.contains("Verbindungen (1):"), "{uebersicht}");
        assert!(
            uebersicht.contains(&format!("- web-01: {BENUTZER}@127.0.0.1:{}", server.port)),
            "{uebersicht}"
        );
        assert!(
            uebersicht.contains("Schlüssel: eingerichtet (SHA256:"),
            "{uebersicht}"
        );
        assert!(
            uebersicht.contains("sudo-Passwort: gesetzt"),
            "{uebersicht}"
        );
        assert!(
            uebersicht.contains("Host-Schlüssel: bekannt (SHA256:"),
            "{uebersicht}"
        );
        assert!(!uebersicht.contains(SUDO_PASSWORT), "{uebersicht}");

        // Ohne sudo-Passwort steht das da.
        let ohne_sudo = text(
            aufruf(
                &aufbau.mit(&[attrappe::zugang("web-01", server.port, None, None)]),
                "ssh_status",
                json!({}),
            )
            .unwrap(),
        );
        assert!(ohne_sudo.contains("sudo-Passwort: fehlt"), "{ohne_sudo}");

        server.stoppen().await;
    });
}

#[test]
fn schluessel_einrichten_ersetzt_den_alten_erst_nach_der_pruefung() {
    let laufzeit = Runtime::new().unwrap();
    laufzeit.block_on(async {
        let server = Attrappe::starte().await;
        let aufbau = Aufbau::neu(None);
        einrichten(&aufbau, &server, "web-01");
        let alt = std::fs::read_to_string(aufbau.schluessel("web-01")).unwrap();
        let alter_fingerabdruck = {
            let key = quiver_ssh::schluessel::laden(&aufbau.schluessel("web-01")).unwrap();
            quiver_ssh::schluessel::fingerprint(key.public_key())
        };

        let antwort = text(
            aufruf(
                &aufbau.mit(&[attrappe::zugang(
                    "web-01",
                    server.port,
                    Some(PASSWORT),
                    None,
                )]),
                "ssh_schluessel_einrichten",
                json!({}),
            )
            .unwrap(),
        );
        assert!(
            antwort.contains("Der bisherige Schlüssel wurde ersetzt."),
            "{antwort}"
        );
        assert!(
            antwort.contains("Schlüssel für web-01 eingerichtet (SHA256:"),
            "{antwort}"
        );
        assert!(antwort.contains("Privater Schlüssel:"), "{antwort}");

        let neu = std::fs::read_to_string(aufbau.schluessel("web-01")).unwrap();
        assert_ne!(neu, alt);
        let neuer_fingerabdruck = {
            let key = quiver_ssh::schluessel::laden(&aufbau.schluessel("web-01")).unwrap();
            quiver_ssh::schluessel::fingerprint(key.public_key())
        };
        assert_ne!(neuer_fingerabdruck, alter_fingerabdruck);
        assert!(antwort.contains(&neuer_fingerabdruck), "{antwort}");

        // Der neue Schlüssel steht in authorized_keys; der alte bleibt dort
        // (auf dem Server wird nichts entfernt – nur lokal ersetzt).
        let zeilen = server.zustand.autorisierte_zeilen();
        assert_eq!(zeilen.len(), 2, "{zeilen:?}");
        assert!(zeilen
            .iter()
            .any(|zeile| zeile.ends_with(" quiver-ssh web-01")));

        // Und die neue Verbindung klappt ohne Passwort.
        let ohne = text(
            aufruf(
                &aufbau.mit(&[attrappe::zugang("web-01", server.port, None, None)]),
                "ssh_exec",
                json!({ "befehl": "id -un" }),
            )
            .unwrap(),
        );
        assert!(ohne.contains("root"), "{ohne}");

        server.stoppen().await;
    });
}

#[test]
fn ohne_anmelde_passwort_und_ohne_schluessel_kommt_ein_hinweis() {
    let laufzeit = Runtime::new().unwrap();
    laufzeit.block_on(async {
        let server = Attrappe::starte().await;
        let aufbau = Aufbau::neu(None);
        let antwort = text(
            aufruf(
                &aufbau.mit(&[attrappe::zugang("web-01", server.port, None, None)]),
                "ssh_exec",
                json!({ "befehl": "id -un" }),
            )
            .unwrap(),
        );
        assert!(
            antwort.contains(
                "Für web-01 fehlt das Anmelde-Passwort für die erste Anmeldung – bitte im quiver-Fenster unter Tools → SSH eintragen."
            ),
            "{antwort}"
        );
        // Kein Verbindungsversuch.
        assert_eq!(server.zustand.passwort_versuche(), 0);
        assert_eq!(server.zustand.pubkey_versuche(), 0);

        server.stoppen().await;
    });
}

#[test]
fn mehrere_verbindungen_verlangen_einen_namen() {
    let aufbau = Aufbau::neu(Some(&[
        attrappe::zugang("a", 22, None, None),
        attrappe::zugang("b", 22, None, None),
    ]));

    // Die Übersicht zeigt ohne Namen alle Verbindungen.
    let alle = text(aufruf(&aufbau.server, "ssh_status", json!({})).unwrap());
    assert!(alle.contains("- a:"), "{alle}");
    assert!(alle.contains("- b:"), "{alle}");

    // Mit Namen nur die eine.
    let eine = text(aufruf(&aufbau.server, "ssh_status", json!({ "verbindung": "b" })).unwrap());
    assert!(eine.contains("- b:"), "{eine}");
    assert!(!eine.contains("- a:"), "{eine}");

    // Ein Werkzeug, das eine Verbindung braucht, verlangt bei mehreren den Namen.
    let fehler =
        fehlertext(aufruf(&aufbau.server, "ssh_list", json!({ "pfad": "." })).unwrap_err());
    assert!(fehler.contains("mehrere Verbindungen"), "{fehler}");
    assert!(fehler.contains("a, b"), "{fehler}");

    let fehler = fehlertext(
        aufruf(
            &aufbau.server,
            "ssh_exec",
            json!({ "befehl": "id", "verbindung": "gibtsnicht" }),
        )
        .unwrap_err(),
    );
    assert!(
        fehler.contains("Die Verbindung „gibtsnicht“ gibt es nicht"),
        "{fehler}"
    );
    assert!(fehler.contains("a, b"), "{fehler}");
}

#[test]
fn ohne_liste_kommt_der_hinweis_auf_das_quiver_fenster() {
    let aufbau = Aufbau::neu(None);
    for werkzeug in [
        "ssh_status",
        "ssh_list",
        "ssh_exec",
        "ssh_upload",
        "ssh_download",
    ] {
        let argumente = json!({
            "befehl": "id -un",
            "pfad": ".",
            "lokal": "/tmp/x",
            "entfernt": "x",
        });
        let antwort = text(aufruf(&aufbau.server, werkzeug, argumente).unwrap());
        assert!(antwort.contains("Zugang fehlt"), "{werkzeug}: {antwort}");
        assert!(antwort.contains("Tools → SSH"), "{werkzeug}: {antwort}");
    }
}

#[test]
fn kaputtes_json_wird_gemeldet_ohne_den_inhalt_zu_zeigen() {
    let temp = tempfile::tempdir().unwrap();
    let pfade = Pfade::neu(temp.path().join("quiver-tools/ssh"), None);
    let kaputt = Sammlung::mit_umgebung(|_| {
        Some(r#"[{"name":"a","werte":{"anmelde_passwort":"geheim-4711"}"#.to_string())
    });
    let server = SshServer::mit(Zugang::mit_umgebung(&Ssh, |_| None), pfade, kaputt);

    let fehler = fehlertext(aufruf(&server, "ssh_status", json!({})).unwrap_err());
    assert!(fehler.contains("kein gültiges JSON"), "{fehler}");
    assert!(!fehler.contains("geheim-4711"), "{fehler}");
    assert!(!fehler.contains("anmelde_passwort"), "{fehler}");
}

#[test]
fn ohne_schreibschalter_geht_kein_befehl_hinaus() {
    let laufzeit = Runtime::new().unwrap();
    laufzeit.block_on(async {
        let server = Attrappe::starte().await;
        let aufbau = Aufbau::neu(None);
        einrichten(&aufbau, &server, "web-01");
        let vorher = server.zustand.befehle().len();
        let ohne = aufbau.mit(&[attrappe::zugang_ohne_schreiben("web-01", server.port)]);

        // Lesen geht weiter.
        let status = text(aufruf(&ohne, "ssh_status", json!({})).unwrap());
        assert!(status.contains("web-01"), "{status}");

        // Befehle nicht – der Hinweis nennt Feld, Ort und Verbindung.
        let text = text(aufruf(&ohne, "ssh_exec", json!({ "befehl": "id -un" })).unwrap());
        assert!(text.contains("Schreiben erlauben"), "{text}");
        assert!(text.contains("quiver-Fenster"), "{text}");
        assert!(text.contains("web-01"), "{text}");
        assert_eq!(
            server.zustand.befehle().len(),
            vorher,
            "Befehl lief trotzdem"
        );

        server.stoppen().await;
    });
}
