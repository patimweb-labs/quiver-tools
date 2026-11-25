// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Aus Daten wird Text: knapp, ohne Emojis, ohne Nebenwirkung.
//!
//! Alles hier sind reine Funktionen. Lange Ausgaben gehen durch
//! [`quiver_tools_kern::kuerzen`], damit ein Agent nicht für jede Zeile
//! Tokens bezahlt.

use quiver_tools_kern::kuerzen;

use crate::client::{Art, Ausfuehrung, Eintrag};

/// Was `ssh_status` über eine Verbindung weiß.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Zustand {
    /// `SHA256:…`, wenn ein Schlüssel eingerichtet ist.
    pub schluessel: Option<String>,
    /// `SHA256:…`, wenn der Host-Schlüssel bekannt ist.
    pub host_schluessel: Option<String>,
    /// Ob ein Passwort für `sudo` hinterlegt ist.
    pub sudo: bool,
}

/// Die Übersicht aller Verbindungen, ohne jedes Geheimnis.
pub fn status(zeilen: &[(String, String, Zustand)]) -> String {
    let mut text = vec![format!("Verbindungen ({}):", zeilen.len())];
    for (name, ziel, zustand) in zeilen {
        text.push(format!("- {name}: {ziel}"));
        match &zustand.schluessel {
            Some(fingerprint) => text.push(format!("  Schlüssel: eingerichtet ({fingerprint})")),
            None => text.push(
                "  Schlüssel: fehlt (die nächste Verbindung richtet ihn mit dem Passwort ein)"
                    .to_string(),
            ),
        }
        text.push(format!(
            "  Passwort für sudo: {}",
            if zustand.sudo {
                "vorhanden"
            } else {
                "fehlt – im quiver-Fenster das Feld „Passwort“ ausfüllen"
            }
        ));
        match &zustand.host_schluessel {
            Some(fingerprint) => text.push(format!("  Host-Schlüssel: bekannt ({fingerprint})")),
            None => text.push("  Host-Schlüssel: noch nicht bekannt".to_string()),
        }
    }
    text.join("\n")
}

/// Das Ergebnis eines Befehls, beide Ströme je auf `max_zeichen` gekürzt.
pub fn ausfuehrung(
    name: &str,
    ausfuehrung: &Ausfuehrung,
    sudo: bool,
    max_zeichen: usize,
) -> String {
    let mut text = vec![format!(
        "{name}: $ {}{}",
        if sudo { "sudo " } else { "" },
        ausfuehrung.befehl
    )];
    text.push(format!(
        "[exit {}, {} ms]",
        ausfuehrung.status, ausfuehrung.dauer_ms
    ));

    let stdout = strom(&ausfuehrung.stdout, max_zeichen);
    let stderr = strom(&ausfuehrung.stderr, max_zeichen);
    if stdout.is_empty() && stderr.is_empty() {
        text.push(String::new());
        text.push("(keine Ausgabe)".to_string());
    }
    if !stdout.is_empty() {
        text.push(String::new());
        text.push(stdout);
    }
    if !stderr.is_empty() {
        text.push(String::new());
        text.push("stderr:".to_string());
        text.push(stderr);
    }
    text.join("\n")
}

/// Ein Ausgabestrom, gekürzt mit Hinweis.
fn strom(text: &str, max_zeichen: usize) -> String {
    let text = text.trim_end();
    if text.is_empty() {
        return String::new();
    }
    let (gekuerzt, weggelassen) = kuerzen(text, max_zeichen);
    match weggelassen {
        Some(weg) => format!("{gekuerzt}\n[… {weg} Zeichen gekürzt (max_zeichen)]"),
        None => gekuerzt,
    }
}

/// Ein entferntes Verzeichnis.
pub fn verzeichnis(name: &str, pfad: &str, eintraege: &[Eintrag]) -> String {
    if eintraege.is_empty() {
        return format!("{pfad} auf {name} ist leer.");
    }
    let mut text = vec![format!(
        "{pfad} auf {name} ({}):",
        if eintraege.len() == 1 {
            "1 Eintrag".to_string()
        } else {
            format!("{} Einträge", eintraege.len())
        }
    )];
    for eintrag in eintraege {
        let (zeichen, groesse) = match eintrag.art {
            Art::Ordner => ("/", String::new()),
            Art::Verknuepfung => ("@", String::new()),
            Art::Datei => ("", format!(" {:>10}", groesse(eintrag.groesse))),
            Art::Anderes => ("", String::new()),
        };
        text.push(format!(
            "{}{groesse}  {}{zeichen}",
            eintrag.rechte, eintrag.name
        ));
    }
    text.join("\n")
}

/// Eine Bytezahl lesbar: `812 B`, `1.4 KB`.
pub fn groesse(bytes: u64) -> String {
    const EINHEITEN: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut wert = bytes as f64 / 1024.0;
    let mut einheit = 0;
    while wert >= 1024.0 && einheit < EINHEITEN.len() - 1 {
        wert /= 1024.0;
        einheit += 1;
    }
    if wert >= 10.0 {
        format!("{wert:.0} {}", EINHEITEN[einheit])
    } else {
        format!("{wert:.1} {}", EINHEITEN[einheit])
    }
}

/// Hängt einen einmaligen Hinweis an eine Antwort.
pub fn mit_hinweis(text: String, hinweis: Option<String>) -> String {
    match hinweis {
        Some(hinweis) if !hinweis.trim().is_empty() => format!("{text}\n\n{hinweis}"),
        _ => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zustand(schluessel: bool, sudo: bool, host: bool) -> Zustand {
        Zustand {
            schluessel: schluessel.then(|| "SHA256:aaa".to_string()),
            host_schluessel: host.then(|| "SHA256:bbb".to_string()),
            sudo,
        }
    }

    #[test]
    fn status_nennt_alles_ohne_geheimnis() {
        let text = status(&[
            (
                "web-01".to_string(),
                "admin@web.example.test".to_string(),
                zustand(true, true, true),
            ),
            (
                "db-02".to_string(),
                "root@db.example.test:2222".to_string(),
                zustand(false, false, false),
            ),
        ]);
        assert!(text.contains("Verbindungen (2):"), "{text}");
        assert!(
            text.contains(
                "- web-01: admin@web.example.test\n  Schlüssel: eingerichtet (SHA256:aaa)"
            ),
            "{text}"
        );
        assert!(text.contains("  Passwort für sudo: vorhanden"), "{text}");
        assert!(
            text.contains("  Host-Schlüssel: bekannt (SHA256:bbb)"),
            "{text}"
        );
        assert!(text.contains("Schlüssel: fehlt"), "{text}");
        assert!(text.contains("Passwort für sudo: fehlt"), "{text}");
        assert!(
            text.contains("Host-Schlüssel: noch nicht bekannt"),
            "{text}"
        );
    }

    #[test]
    fn ausfuehrung_zeigt_status_stroeme_und_kuerzung() {
        let ausfuehrung = Ausfuehrung {
            befehl: "id -un".to_string(),
            stdout: "root\n".to_string(),
            stderr: "warnung\n".to_string(),
            status: 0,
            dauer_ms: 12,
        };
        let text = super::ausfuehrung("web-01", &ausfuehrung, false, 8000);
        assert!(
            text.starts_with("web-01: $ id -un\n[exit 0, 12 ms]"),
            "{text}"
        );
        assert!(text.contains("\nroot"), "{text}");
        assert!(text.contains("stderr:\nwarnung"), "{text}");

        let mit_sudo = super::ausfuehrung("web-01", &ausfuehrung, true, 8000);
        assert!(mit_sudo.contains("$ sudo id -un"), "{mit_sudo}");
    }

    #[test]
    fn leere_ausgabe_wird_gesagt() {
        let ausfuehrung = Ausfuehrung {
            befehl: "true".to_string(),
            stdout: "  \n".to_string(),
            stderr: String::new(),
            status: 0,
            dauer_ms: 1,
        };
        assert!(super::ausfuehrung("web-01", &ausfuehrung, false, 100).contains("(keine Ausgabe)"));
    }

    #[test]
    fn lange_ausgabe_wird_gekuerzt_mit_hinweis() {
        let ausfuehrung = Ausfuehrung {
            befehl: "cat gross".to_string(),
            stdout: "Zeile eins mit Text\nZeile zwei mit Text\n".to_string(),
            stderr: String::new(),
            status: 0,
            dauer_ms: 3,
        };
        let text = super::ausfuehrung("web-01", &ausfuehrung, false, 10);
        assert!(text.contains("Zeichen gekürzt"), "{text}");
        assert!(!text.contains("Zeile zwei"), "{text}");
    }

    #[test]
    fn verzeichnis_zeigt_art_rechte_und_groesse() {
        let eintraege = vec![
            Eintrag {
                name: "nginx".to_string(),
                art: Art::Ordner,
                groesse: 4096,
                rechte: "rwxr-xr-x".to_string(),
            },
            Eintrag {
                name: "access.log".to_string(),
                art: Art::Datei,
                groesse: 812,
                rechte: "rw-r--r--".to_string(),
            },
        ];
        let text = verzeichnis("web-01", "/var/log", &eintraege);
        assert!(text.contains("/var/log auf web-01 (2 Einträge):"), "{text}");
        assert!(
            verzeichnis("web-01", "/var/log", &eintraege[..1]).contains("(1 Eintrag)"),
            "Einzahl fehlt"
        );
        assert!(text.contains("rwxr-xr-x  nginx/"), "{text}");
        assert!(text.contains("rw-r--r--      812 B  access.log"), "{text}");
        assert!(verzeichnis("web-01", "/leer", &[]).contains("ist leer"));
    }

    #[test]
    fn groessen_werden_lesbar() {
        assert_eq!(groesse(0), "0 B");
        assert_eq!(groesse(812), "812 B");
        assert_eq!(groesse(1024), "1.0 KB");
        assert_eq!(groesse(1536), "1.5 KB");
        assert_eq!(groesse(20 * 1024), "20 KB");
        assert_eq!(groesse(3 * 1024 * 1024), "3.0 MB");
    }

    #[test]
    fn ein_hinweis_haengt_hinten_an() {
        assert_eq!(mit_hinweis("a".into(), None), "a");
        assert_eq!(mit_hinweis("a".into(), Some("  ".into())), "a");
        assert_eq!(mit_hinweis("a".into(), Some("b".into())), "a\n\nb");
    }
}
