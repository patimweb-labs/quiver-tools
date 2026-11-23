// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Das Programm-Gerüst aller quiver-Tools (SPEC §4).
//!
//! Jedes Tool ist ein Programm mit vier Aufgaben: **MCP über stdio** (der
//! Normalfall, so startet quiver es), **`eintrag`** (der Bibliothekseintrag für
//! quiver als JSON), **`testen`** (den Zugang aus der Umgebung prüfen) und
//! `--version`/`--help`. Der Kern kennt alle Wege, das Tool liefert nur sein
//! [`Werkzeug`] und seinen rmcp-Server. Der Server bekommt den [`Zugang`] aus
//! der Umgebung; ein beim Start gelesener Zugang bleibt gültig, weil quiver die
//! Umgebung des Prozesses setzt.
//!
//! Logs und Meldungen gehen nach **stderr**: stdout gehört dem MCP-Protokoll.

use std::process::ExitCode;

use crate::eintrag;
use crate::profil::Profil;
use crate::umgebung::Zugang;
use crate::{hinweis_zugang, Werkzeug};
use rmcp::ServiceExt;

/// Was auf der Kommandozeile stand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Aufruf {
    /// Ohne Argument: MCP über stdio.
    Mcp,
    /// `eintrag [--befehl <pfad>]`: der Bibliothekseintrag für mcp-quiver.
    Eintrag { befehl: Option<String> },
    /// `testen`: den Zugang aus der Umgebung prüfen.
    Testen,
    /// `--version`.
    Version,
    /// `--help`.
    Hilfe,
    /// Etwas anderes – das Programm sagt, was es kennt.
    Unbekannt(String),
}

/// Liest den Aufruf aus den Argumenten (ohne den Programmnamen).
pub fn aufruf_aus_args(args: impl IntoIterator<Item = String>) -> Aufruf {
    let mut args = args.into_iter();
    match args.next() {
        None => Aufruf::Mcp,
        Some(argument) => match argument.as_str() {
            "eintrag" => match befehl_aus_args(&mut args) {
                Ok(befehl) => Aufruf::Eintrag { befehl },
                Err(argument) => Aufruf::Unbekannt(argument),
            },
            "testen" => Aufruf::Testen,
            "--version" | "-V" | "version" => Aufruf::Version,
            "--help" | "-h" | "help" => Aufruf::Hilfe,
            sonst => Aufruf::Unbekannt(sonst.to_string()),
        },
    }
}

/// `--befehl <pfad>` oder `--befehl=<pfad>` nach `eintrag`.
fn befehl_aus_args(args: &mut impl Iterator<Item = String>) -> Result<Option<String>, String> {
    let mut befehl = None;
    while let Some(argument) = args.next() {
        if let Some(wert) = argument.strip_prefix("--befehl=") {
            befehl = Some(wert.to_string());
        } else if argument == "--befehl" {
            let wert = args
                .next()
                .ok_or_else(|| "--befehl ohne Pfad".to_string())?;
            befehl = Some(wert);
        } else {
            return Err(argument);
        }
    }
    Ok(befehl)
}

/// Startet das Tool: `eintrag`, `testen`, `--version`, `--help` oder MCP über stdio.
///
/// `server` baut den rmcp-Server aus dem Zugang; er wird nur im MCP-Fall
/// aufgerufen.
pub async fn starten<W, S, F>(werkzeug: W, server: F) -> ExitCode
where
    W: Werkzeug,
    S: rmcp::ServerHandler,
    F: FnOnce(Zugang) -> S,
{
    match aufruf_aus_args(std::env::args().skip(1)) {
        Aufruf::Mcp => mcp(werkzeug, server).await,
        Aufruf::Eintrag { befehl } => {
            let eintrag = eintrag::eintrag(&werkzeug, befehl);
            match serde_json::to_string_pretty(&eintrag) {
                Ok(text) => {
                    println!("{text}");
                    ExitCode::SUCCESS
                }
                Err(fehler) => {
                    eprintln!("Der Eintrag lässt sich nicht schreiben: {fehler}");
                    ExitCode::FAILURE
                }
            }
        }
        Aufruf::Testen => testen(werkzeug).await,
        Aufruf::Version => {
            println!("{} {}", werkzeug.name(), env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Aufruf::Hilfe => {
            println!(
                "{}",
                hilfe(
                    werkzeug.name(),
                    werkzeug.titel(),
                    werkzeug.mehrfach().is_some()
                )
            );
            ExitCode::SUCCESS
        }
        Aufruf::Unbekannt(argument) => {
            eprintln!(
                "Unbekanntes Argument „{argument}“.\n\n{}",
                hilfe(
                    werkzeug.name(),
                    werkzeug.titel(),
                    werkzeug.mehrfach().is_some()
                )
            );
            ExitCode::from(2)
        }
    }
}

fn hilfe(name: &str, titel: &str, mehrfach: bool) -> String {
    let angaben = if mehrfach {
        format!(
            "Die Verbindungen legt der User im quiver-Fenster unter Tools → {titel} an;\n\
             quiver übergibt sie als JSON in QUIVER_ZUGAENGE. Geheimnisse liegen im\n\
             Schlüsselbund und erreichen nie einen Agenten."
        )
    } else {
        format!(
            "Die Angaben trägt der User im quiver-Fenster unter Tools → {titel} ein;\n\
             quiver setzt sie als Umgebung ({name} in Großbuchstaben, Unterstrich, Feldname).\n\
             Geheimnisse liegen im Schlüsselbund und erreichen nie einen Agenten."
        )
    };
    format!(
        "{titel} – MCP-Tool von quiver.\n\
         \n\
         Aufruf:\n\
         \x20 quiver-{name}                       MCP über stdio (so startet quiver es)\n\
         \x20 quiver-{name} eintrag [--befehl X]  Bibliothekseintrag als JSON\n\
         \x20 quiver-{name} testen                Zugang aus der Umgebung prüfen\n\
         \x20 quiver-{name} --version             Version ausgeben\n\
         \n\
         {angaben}"
    )
}

/// Der MCP-Weg: Zugang lesen, Server bauen, über stdio dienen, auf das Ende warten.
async fn mcp<W, S, F>(werkzeug: W, server: F) -> ExitCode
where
    W: Werkzeug,
    S: rmcp::ServerHandler,
    F: FnOnce(Zugang) -> S,
{
    let zugang = Zugang::neu(&werkzeug);
    tracing::info!(
        werkzeug = werkzeug.name(),
        zugang = ?zugang.fehlende().iter().map(|feld| feld.name).collect::<Vec<_>>(),
        "MCP über stdio"
    );

    let dienst = match server(zugang).serve(rmcp::transport::stdio()).await {
        Ok(dienst) => dienst,
        Err(fehler) => {
            eprintln!("{} konnte nicht starten: {fehler}", werkzeug.titel());
            return ExitCode::FAILURE;
        }
    };

    match dienst.waiting().await {
        Ok(_) => ExitCode::SUCCESS,
        Err(fehler) => {
            eprintln!("{} wurde unsanft beendet: {fehler}", werkzeug.titel());
            ExitCode::FAILURE
        }
    }
}

/// `testen`: die Umgebung lesen, die Verbindung prüfen, Exit 0/1.
///
/// Die Meldung geht im Erfolgsfall nach stdout, ein Fehler nach stderr. Fehlt
/// der Zugang, nennt die Ausgabe die fehlenden Umgebungsvariablen – aber nie
/// einen Wert.
async fn testen<W: Werkzeug>(werkzeug: W) -> ExitCode {
    // Mehrere Zugänge: die Liste liest das Werkzeug selbst (QUIVER_ZUGAENGE);
    // ein einzelnes Profil gibt es dafür nicht.
    if werkzeug.mehrfach().is_some() {
        return match werkzeug.testen(&Profil::leer()).await {
            Ok(meldung) => {
                println!("{meldung}");
                ExitCode::SUCCESS
            }
            Err(fehler) => {
                eprintln!("{}: {fehler}", werkzeug.titel());
                ExitCode::FAILURE
            }
        };
    }

    let zugang = Zugang::neu(&werkzeug);
    let profil = match zugang.profil() {
        Ok(profil) => profil,
        Err(_) => {
            eprintln!("{}", hinweis_zugang(werkzeug.titel()));
            for feld in zugang.fehlende() {
                eprintln!(
                    "  fehlt: {} ({})",
                    crate::umgebung::schluessel(werkzeug.name(), feld.name),
                    feld.titel
                );
            }
            return ExitCode::FAILURE;
        }
    };

    match werkzeug.testen(&profil).await {
        Ok(meldung) => {
            println!("{meldung}");
            ExitCode::SUCCESS
        }
        Err(fehler) => {
            eprintln!("{}: {fehler}", werkzeug.titel());
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aufruf(args: &[&str]) -> Aufruf {
        aufruf_aus_args(args.iter().map(|argument| argument.to_string()))
    }

    #[test]
    fn ohne_argument_ist_mcp() {
        assert_eq!(aufruf(&[]), Aufruf::Mcp);
    }

    #[test]
    fn eintrag_und_testen_werden_erkannt() {
        assert_eq!(aufruf(&["eintrag"]), Aufruf::Eintrag { befehl: None });
        assert_eq!(
            aufruf(&["eintrag", "--befehl", "/pfad/quiver-outline"]),
            Aufruf::Eintrag {
                befehl: Some("/pfad/quiver-outline".to_string())
            }
        );
        assert_eq!(
            aufruf(&["eintrag", "--befehl=/pfad/quiver-outline"]),
            Aufruf::Eintrag {
                befehl: Some("/pfad/quiver-outline".to_string())
            }
        );
        assert_eq!(aufruf(&["testen"]), Aufruf::Testen);
    }

    #[test]
    fn einrichten_gibt_es_nicht_mehr() {
        assert_eq!(
            aufruf(&["einrichten"]),
            Aufruf::Unbekannt("einrichten".into())
        );
    }

    #[test]
    fn falsche_eintrag_argumente_werden_gemeldet() {
        assert_eq!(
            aufruf(&["eintrag", "quatsch"]),
            Aufruf::Unbekannt("quatsch".into())
        );
        assert_eq!(
            aufruf(&["eintrag", "--befehl"]),
            Aufruf::Unbekannt("--befehl ohne Pfad".into())
        );
    }

    #[test]
    fn version_und_hilfe() {
        assert_eq!(aufruf(&["--version"]), Aufruf::Version);
        assert_eq!(aufruf(&["-V"]), Aufruf::Version);
        assert_eq!(aufruf(&["--help"]), Aufruf::Hilfe);
        assert_eq!(aufruf(&["-h"]), Aufruf::Hilfe);
    }

    #[test]
    fn unbekanntes_wird_gemeldet() {
        assert_eq!(aufruf(&["setup"]), Aufruf::Unbekannt("setup".to_string()));
    }

    #[test]
    fn hilfe_nennt_alle_wege_und_kein_geheimnis() {
        let text = hilfe("outline", "Outline-Wiki", false);
        assert!(text.contains("quiver-outline eintrag"));
        assert!(text.contains("quiver-outline testen"));
        assert!(text.contains("MCP über stdio"));
        assert!(text.contains("Schlüsselbund"));
        assert!(!text.contains("einrichten"));
    }
}
