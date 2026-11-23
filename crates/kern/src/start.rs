// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Das Programm-Gerüst aller quiver-Tools (SPEC §4).
//!
//! Jedes Tool ist ein Programm mit zwei Aufgaben: **MCP über stdio** (der
//! Normalfall, so startet quiver es) und **Einrichtung** (`quiver-<tool>
//! einrichten`, vom Knopf „Einrichten“ in quiver aufgerufen). Der Kern kennt
//! beide Wege, das Tool liefert nur sein [`Werkzeug`] und seinen rmcp-Server.
//!
//! Begründung der Form: die Einrichtung braucht den Profilspeicher, den Server
//! braucht MCP – und beides gehört zum Werkzeug, nicht zum Kern. Darum gibt das
//! Tool eine Funktion mit, die aus dem geladenen [`Profile`]-Speicher seinen
//! Server baut; der Kern entscheidet vorher, welcher Weg dran ist. So bleibt je
//! Tool eine `main` von zwei Zeilen, und die Profilwahl (`profil`-Parameter)
//! passiert zur Laufzeit je Aufruf, nicht beim Start.
//!
//! Logs und Meldungen gehen nach **stderr**: stdout gehört dem MCP-Protokoll.

use std::process::ExitCode;

use crate::profil::Profile;
use crate::seite;
use crate::tresor;
use crate::Werkzeug;
use rmcp::ServiceExt;

/// Was auf der Kommandozeile stand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Aufruf {
    /// Ohne Argument: MCP über stdio.
    Mcp,
    /// `einrichten`: die Einrichtungsseite im Browser.
    Einrichten,
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
            "einrichten" => Aufruf::Einrichten,
            "--version" | "-V" | "version" => Aufruf::Version,
            "--help" | "-h" | "help" => Aufruf::Hilfe,
            sonst => Aufruf::Unbekannt(sonst.to_string()),
        },
    }
}

/// Startet das Tool: `einrichten`, `--version`, `--help` oder MCP über stdio.
///
/// `server` baut den rmcp-Server aus dem Profilspeicher; er wird nur im
/// MCP-Fall aufgerufen.
pub async fn starten<W, S, F>(werkzeug: W, server: F) -> ExitCode
where
    W: Werkzeug,
    S: rmcp::ServerHandler,
    F: FnOnce(Profile) -> S,
{
    match aufruf_aus_args(std::env::args().skip(1)) {
        Aufruf::Einrichten => seite::einrichten(werkzeug).await,
        Aufruf::Version => {
            println!("{} {}", werkzeug.name(), env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Aufruf::Hilfe => {
            println!("{}", hilfe(werkzeug.name(), werkzeug.titel()));
            ExitCode::SUCCESS
        }
        Aufruf::Unbekannt(argument) => {
            eprintln!(
                "Unbekanntes Argument „{argument}“.\n\n{}",
                hilfe(werkzeug.name(), werkzeug.titel())
            );
            ExitCode::from(2)
        }
        Aufruf::Mcp => mcp(werkzeug, server).await,
    }
}

fn hilfe(name: &str, titel: &str) -> String {
    format!(
        "{titel} – MCP-Tool von quiver.\n\
         \n\
         Aufruf:\n\
         \x20 quiver-{name}                MCP über stdio (so startet quiver es)\n\
         \x20 quiver-{name} einrichten     Zugänge im Browser einrichten\n\
         \x20 quiver-{name} --version      Version ausgeben\n\
         \n\
         Zugänge (Profile) richtet nur der User auf der Einrichtungsseite ein;\n\
         sie liegen im Schlüsselbund und erreichen nie einen Agenten."
    )
}

/// Der MCP-Weg: Server bauen, über stdio dienen, auf das Ende warten.
async fn mcp<W, S, F>(werkzeug: W, server: F) -> ExitCode
where
    W: Werkzeug,
    S: rmcp::ServerHandler,
    F: FnOnce(Profile) -> S,
{
    let profile = Profile::laden(&werkzeug, tresor::aus_umgebung());
    tracing::info!(
        werkzeug = werkzeug.name(),
        ordner = %profile.ordner().display(),
        "MCP über stdio"
    );

    let dienst = match server(profile).serve(rmcp::transport::stdio()).await {
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
    fn einrichten_wird_erkannt() {
        assert_eq!(aufruf(&["einrichten"]), Aufruf::Einrichten);
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
    fn hilfe_nennt_beide_wege_und_kein_geheimnis() {
        let text = hilfe("outline", "Outline-Wiki");
        assert!(text.contains("quiver-outline einrichten"));
        assert!(text.contains("MCP über stdio"));
        assert!(text.contains("Schlüsselbund"));
    }
}
