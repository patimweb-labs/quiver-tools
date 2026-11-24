// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Befehle neben MCP (Aufgabe 06): `anmelden`, `status`, `abmelden` und
//! `routine-pruefen`.
//!
//! Den Normalfall (`eintrag`, `testen`, `--version`, `--help`, MCP über stdio)
//! bedient der Kern; hier stehen nur die Wege, die es nur bei diesem Werkzeug
//! gibt. Ausgaben gehen nach stdout, Fehler nach stderr, der Exit-Code sagt
//! 0 oder 1 – nie ein Wert aus dem Zugang.

use std::process::ExitCode;
use std::sync::Arc;

use quiver_tools_kern::{hinweis_zugang, Werkzeug, Zugang};

use crate::anmeldung::{Anmeldung, RoutineLauf};
use crate::browser::OeffneBrowser;
use crate::werkzeug::Zep;

/// Ein Befehl, den nur dieses Werkzeug kennt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Befehl {
    /// `anmelden` – die Routine mit Benutzer und Passwort, einmal.
    Anmelden,
    /// `anmelden --browser` – der Notfall über den Browser.
    AnmeldenBrowser,
    /// `status` – ist eine Anmeldung da?
    Status,
    /// `abmelden` – Refresh-Token löschen und Sperre aufheben.
    Abmelden,
    /// `routine-pruefen` – die Routine einmal ausführen und jeden Schritt melden.
    RoutinePruefen,
}

/// Liest den Befehl aus den Argumenten, falls es einer von hier ist.
///
/// Alles andere (auch keine Argumente) gehört dem Kern.
pub fn aus_args(args: impl IntoIterator<Item = String>) -> Option<Befehl> {
    let mut args = args.into_iter();
    let erster = args.next()?;
    match erster.as_str() {
        "anmelden" | "anmelden:" => {
            let mut mit_browser = false;
            for weiteres in args {
                match weiteres.as_str() {
                    "--browser" => mit_browser = true,
                    sonst => {
                        eprintln!("Unbekanntes Argument „{sonst}“ zu „anmelden“.");
                        return Some(Befehl::Anmelden);
                    }
                }
            }
            Some(if mit_browser {
                Befehl::AnmeldenBrowser
            } else {
                Befehl::Anmelden
            })
        }
        "status" => Some(Befehl::Status),
        "abmelden" => Some(Befehl::Abmelden),
        "routine-pruefen" => Some(Befehl::RoutinePruefen),
        _ => None,
    }
}

/// Den Befehl ausführen. Liest den Zugang aus der Umgebung (SPEC §2).
pub async fn ausfuehren(befehl: Befehl) -> ExitCode {
    let zugang = Zugang::neu(&Zep);
    let Ok(profil) = zugang.profil() else {
        eprintln!("{}", hinweis_zugang(Zep.titel()));
        for feld in zugang.fehlende() {
            eprintln!(
                "  fehlt: {} ({})",
                quiver_tools_kern::umgebung::schluessel(Zep.name(), feld.name),
                feld.titel
            );
        }
        return ExitCode::FAILURE;
    };
    let Ok(konfig) = crate::werkzeug::ZepKonfig::aus_profil(&profil) else {
        eprintln!("{}", hinweis_zugang(Zep.titel()));
        return ExitCode::FAILURE;
    };

    match befehl {
        Befehl::Anmelden => {
            // Der Knopf: kein Browser, nur die Routine.
            let anmeldung = Anmeldung::neu(
                konfig,
                crate::tresor::aus_umgebung(),
                Arc::new(crate::browser::KeinBrowser::neu()),
            );
            berichten(&anmeldung.routine_lauf().await, false)
        }
        Befehl::AnmeldenBrowser => {
            let anmeldung = Anmeldung::neu(
                konfig,
                crate::tresor::aus_umgebung(),
                Arc::new(OeffneBrowser::neu()),
            );
            berichten(&anmeldung.browser_lauf().await, true)
        }
        Befehl::Status => {
            let anmeldung = Anmeldung::aus_konfig(konfig);
            let status = anmeldung.status();
            if status.ist_angemeldet() {
                println!("{}", status.text());
                ExitCode::SUCCESS
            } else {
                eprintln!("{}", status.text());
                ExitCode::FAILURE
            }
        }
        Befehl::Abmelden => {
            let anmeldung = Anmeldung::aus_konfig(konfig);
            match anmeldung.abmelden() {
                Ok(()) => {
                    println!("Abgemeldet. Das Refresh-Token ist gelöscht.");
                    ExitCode::SUCCESS
                }
                Err(fehler) => {
                    eprintln!("Abmelden fehlgeschlagen: {fehler}");
                    ExitCode::FAILURE
                }
            }
        }
        Befehl::RoutinePruefen => {
            let anmeldung = Anmeldung::neu(
                konfig,
                crate::tresor::aus_umgebung(),
                Arc::new(crate::browser::KeinBrowser::neu()),
            );
            berichten(&anmeldung.routine_lauf().await, false)
        }
    }
}

/// Den Bericht ausgeben und den Exit-Code bestimmen.
fn berichten(lauf: &RoutineLauf, mit_browser: bool) -> ExitCode {
    for zeile in &lauf.bericht {
        println!("{zeile}");
    }
    match &lauf.fehler {
        None => ExitCode::SUCCESS,
        Some(fehler) => {
            eprintln!("{fehler}");
            if !mit_browser {
                eprintln!(
                    "Kommt die Routine an einer unbekannten Seite nicht weiter (z. B. MFA), \
                     hilft der Notfall: quiver-zep anmelden --browser"
                );
            }
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn befehl(args: &[&str]) -> Option<Befehl> {
        aus_args(args.iter().map(|argument| argument.to_string()))
    }

    #[test]
    fn die_eigenen_befehle_werden_erkannt() {
        assert_eq!(befehl(&["anmelden"]), Some(Befehl::Anmelden));
        assert_eq!(
            befehl(&["anmelden", "--browser"]),
            Some(Befehl::AnmeldenBrowser)
        );
        assert_eq!(befehl(&["status"]), Some(Befehl::Status));
        assert_eq!(befehl(&["abmelden"]), Some(Befehl::Abmelden));
        assert_eq!(befehl(&["routine-pruefen"]), Some(Befehl::RoutinePruefen));
    }

    #[test]
    fn alles_andere_gehoert_dem_kern() {
        assert_eq!(befehl(&[]), None);
        assert_eq!(befehl(&["eintrag"]), None);
        assert_eq!(befehl(&["testen"]), None);
        assert_eq!(befehl(&["--version"]), None);
        assert_eq!(befehl(&["quatsch"]), None);
    }
}
