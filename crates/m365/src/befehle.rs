// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Kommandozeile von `quiver-m365`: MCP über stdio (Standard),
//! `anmelden` / `status` / `abmelden` für das quiver-Fenster, `freigeben` für
//! die Admin-Zustimmung, `--version`.
//!
//! Bewusst ohne Argument-Crate: vier Befehle brauchen keine Abhängigkeit. Die
//! reine Auswertung steht hier, damit sie ohne Prozessstart prüfbar ist.

/// Was das Programm tun soll.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Befehl {
    /// MCP über stdio (Standard, keine Argumente).
    Dienen,
    /// Anmelden über den Browser-Ablauf und das Konto ausgeben (Aufgabe 03).
    Anmelden,
    /// Ohne Browser prüfen, ob das Refresh-Token noch trägt (Aufgabe 03).
    Status,
    /// Refresh-Token löschen (Aufgabe 03).
    Abmelden,
    /// Admin-Freigabe im Browser (`--tenant` überschreibt `M365_TENANT`).
    Freigeben { tenant: Option<String> },
    /// Den Bibliothekseintrag für quiver ausgeben.
    Eintrag { befehl: Option<String> },
    /// `--version`
    Version,
    /// `--help`
    Hilfe,
    /// Etwas, das wir nicht kennen.
    Unbekannt(String),
}

pub const HILFE: &str = "\
quiver-m365 – Microsoft 365 für Agenten (MCP über stdio)

Aufruf:
  quiver-m365                      MCP-Server über stdio (dieser Aufruf ist der Normalfall)
  quiver-m365 anmelden             Anmeldung im Browser, danach das Konto ausgeben
                                   (Exit 0 = angemeldet, 2 = Fehler)
  quiver-m365 status               ohne Browser prüfen, ob die Anmeldung noch gilt
                                   (Exit 0 = angemeldet, 1 = nicht angemeldet, 2 = Fehler)
  quiver-m365 abmelden             Refresh-Token löschen
  quiver-m365 freigeben [--tenant <id|domain>]
                                   Admin-Freigabe im Browser, einmal je Organisation
  quiver-m365 eintrag [--befehl <pfad>]
                                   Bibliothekseintrag für quiver ausgeben (JSON)
  quiver-m365 --version            Version ausgeben
  quiver-m365 --help               diese Hilfe

Die Anmeldung löst der User im quiver-Fenster aus (Tools → Microsoft 365 → Anmelden).
Im MCP-Betrieb öffnet quiver-m365 nie einen Browser; nur mit M365_BROWSER=erlaubt.

Umgebung: M365_CLIENT_ID, M365_TENANT, M365_SCOPES, M365_AUTHORITY,
          M365_GRAPH_URL, M365_ZEITZONE, M365_ACCESS_TOKEN, M365_TRESOR,
          M365_BROWSER
";

/// Wertet die Argumente aus (ohne den Programmnamen).
pub fn befehl(args: &[String]) -> Befehl {
    let Some(erstes) = args.first() else {
        return Befehl::Dienen;
    };

    match erstes.as_str() {
        "--version" | "-V" | "version" => Befehl::Version,
        "--help" | "-h" | "help" => Befehl::Hilfe,
        "anmelden" => Befehl::Anmelden,
        "status" => Befehl::Status,
        "abmelden" => Befehl::Abmelden,
        "eintrag" => Befehl::Eintrag {
            befehl: befehl_aus(&args[1..]),
        },
        "freigeben" => Befehl::Freigeben {
            tenant: tenant_aus(&args[1..]),
        },
        anderes => Befehl::Unbekannt(anderes.to_string()),
    }
}

/// `--befehl wert` oder `--befehl=wert`.
fn befehl_aus(args: &[String]) -> Option<String> {
    let mut rest = args.iter();
    while let Some(argument) = rest.next() {
        if let Some(wert) = argument.strip_prefix("--befehl=") {
            return nicht_leer(wert);
        }
        if argument == "--befehl" {
            return rest.next().and_then(|wert| nicht_leer(wert));
        }
    }
    None
}

/// `--tenant wert` oder `--tenant=wert`; das letzte Vorkommen gewinnt.
fn tenant_aus(args: &[String]) -> Option<String> {
    let mut tenant = None;
    let mut rest = args.iter();
    while let Some(argument) = rest.next() {
        if let Some(wert) = argument.strip_prefix("--tenant=") {
            tenant = nicht_leer(wert);
        } else if argument == "--tenant" {
            if let Some(wert) = rest.next() {
                tenant = nicht_leer(wert);
            }
        }
    }
    tenant
}

fn nicht_leer(wert: &str) -> Option<String> {
    let wert = wert.trim();
    if wert.is_empty() {
        None
    } else {
        Some(wert.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(teile: &[&str]) -> Vec<String> {
        teile.iter().map(|teil| teil.to_string()).collect()
    }

    #[test]
    fn ohne_argumente_ist_mcp_ueber_stdio() {
        assert_eq!(befehl(&[]), Befehl::Dienen);
    }

    #[test]
    fn version_und_hilfe() {
        assert_eq!(befehl(&args(&["--version"])), Befehl::Version);
        assert_eq!(befehl(&args(&["-V"])), Befehl::Version);
        assert_eq!(befehl(&args(&["--help"])), Befehl::Hilfe);
        for wort in ["freigeben", "anmelden", "status", "abmelden"] {
            assert!(HILFE.contains(wort), "{wort} fehlt in der Hilfe");
        }
    }

    /// Die drei Unterbefehle, mit denen das quiver-Fenster die Anmeldung auslöst.
    #[test]
    fn anmelden_status_abmelden() {
        assert_eq!(befehl(&args(&["anmelden"])), Befehl::Anmelden);
        assert_eq!(befehl(&args(&["status"])), Befehl::Status);
        assert_eq!(befehl(&args(&["abmelden"])), Befehl::Abmelden);
    }

    #[test]
    fn freigeben_mit_und_ohne_tenant() {
        assert_eq!(
            befehl(&args(&["freigeben"])),
            Befehl::Freigeben { tenant: None }
        );
        assert_eq!(
            befehl(&args(&["freigeben", "--tenant", "firma.de"])),
            Befehl::Freigeben {
                tenant: Some("firma.de".to_string())
            }
        );
        assert_eq!(
            befehl(&args(&["freigeben", "--tenant=firma.onmicrosoft.com"])),
            Befehl::Freigeben {
                tenant: Some("firma.onmicrosoft.com".to_string())
            }
        );
        // Leerer Wert zählt als nicht gesetzt.
        assert_eq!(
            befehl(&args(&["freigeben", "--tenant", " "])),
            Befehl::Freigeben { tenant: None }
        );
    }

    #[test]
    fn unbekanntes_argument_wird_gemeldet() {
        assert_eq!(
            befehl(&args(&["quatsch"])),
            Befehl::Unbekannt("quatsch".to_string())
        );
    }
}
