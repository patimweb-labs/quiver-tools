// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Das Programm `quiver-m365` (SPEC.md §5).
//!
//! Hier wird nur verdrahtet: Konfig aus der Umgebung, Tresor (Datei oder
//! `M365_TRESOR=speicher`), Browser, Graph-Client, rmcp über stdio.
//!
//! **stdout gehört MCP.** Logs gehen nach stderr, `--version`, `anmelden`,
//! `status`, `abmelden` und `freigeben` schreiben auf stdout.

use std::process::ExitCode;

use m365_anmeldung::{EntraAnmeldung, Konfig, OeffneBrowser};
use m365_graph::GraphClient;
use quiver_m365::befehle::{befehl, Befehl, HILFE};
use quiver_m365::freigeben::freigeben;
use quiver_m365::unterbefehle::{abmelden, anmelden, status, Ausgang};
use quiver_m365::{umgebung, BrowserWahl, Quiver, TresorWahl};
use rmcp::ServiceExt;

#[tokio::main]
async fn main() -> ExitCode {
    loggen();

    let argumente: Vec<String> = std::env::args().skip(1).collect();
    match befehl(&argumente) {
        Befehl::Version => {
            println!("quiver-m365 {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Befehl::Hilfe => {
            print!("{HILFE}");
            ExitCode::SUCCESS
        }
        Befehl::Unbekannt(argument) => {
            eprintln!("Unbekanntes Argument {argument:?}.\n\n{HILFE}");
            ExitCode::FAILURE
        }
        Befehl::Eintrag { befehl } => {
            println!("{:#}", quiver_m365::umgebung::eintrag(befehl.as_deref()));
            ExitCode::SUCCESS
        }
        Befehl::Freigeben { tenant } => freigeben_ausfuehren(tenant).await,
        Befehl::Anmelden => anmelden_ausfuehren().await,
        Befehl::Status => status_ausfuehren().await,
        Befehl::Abmelden => abmelden_ausfuehren(),
        Befehl::Dienen => dienen().await,
    }
}

/// Logs nach stderr (`RUST_LOG`, Standard `info`). stdout bleibt für MCP frei.
fn loggen() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();
}

/// MCP über stdio, bis der Client die Verbindung schließt.
async fn dienen() -> ExitCode {
    let konfig = match Konfig::aus_umgebung() {
        Ok(konfig) => konfig,
        Err(fehler) => return fehler_ausgeben(fehler),
    };
    let tresor = TresorWahl::aus_umgebung();
    // Im MCP-Betrieb öffnet `quiver-m365` nie selbst einen Browser (Aufgabe 03):
    // die Anmeldung löst der User im quiver-Fenster aus (Unterbefehl `anmelden`).
    let browser = BrowserWahl::aus_umgebung();
    let ohne_browser = !browser.erlaubt();
    tracing::info!(
        "quiver-m365 startet (Tenant {}, Tresor {}, Browser {}, Graph {})",
        konfig.tenant,
        tresor.name(),
        browser.name(),
        umgebung::graph_url()
    );

    let anmeldung = match EntraAnmeldung::neu(konfig, tresor, browser) {
        Ok(anmeldung) => anmeldung,
        Err(fehler) => return fehler_ausgeben(fehler),
    };
    let anmeldung = if ohne_browser {
        anmeldung.ohne_browser()
    } else {
        anmeldung
    };
    let graph = match GraphClient::neu(umgebung::graph_url(), anmeldung) {
        Ok(graph) => graph,
        Err(fehler) => return fehler_ausgeben(fehler),
    };

    let server = Quiver::neu(graph, umgebung::zeitzone());
    let dienst = match server.serve(rmcp::transport::stdio()).await {
        Ok(dienst) => dienst,
        Err(fehler) => return fehler_ausgeben(anyhow::anyhow!("MCP startet nicht: {fehler}")),
    };
    tracing::info!("MCP über stdio bereit");

    match dienst.waiting().await {
        Ok(grund) => {
            tracing::info!("MCP beendet ({grund:?})");
            ExitCode::SUCCESS
        }
        Err(fehler) => fehler_ausgeben(anyhow::anyhow!("MCP endet unsauber: {fehler}")),
    }
}

/// Admin-Freigabe im Browser (SPEC.md §2a).
async fn freigeben_ausfuehren(tenant: Option<String>) -> ExitCode {
    let konfig = match Konfig::aus_umgebung() {
        Ok(konfig) => konfig,
        Err(fehler) => return fehler_ausgeben(fehler),
    };
    let tenant = tenant.unwrap_or_else(|| konfig.tenant.clone());

    match freigeben(&konfig, &tenant, &OeffneBrowser::neu()).await {
        Ok(freigabe) => {
            println!("Freigegeben für Tenant {}", freigabe.tenant);
            ExitCode::SUCCESS
        }
        Err(fehler) => fehler_ausgeben(fehler),
    }
}

/// `quiver-m365 anmelden` (Aufgabe 03): Browser-Ablauf, danach das Konto.
async fn anmelden_ausfuehren() -> ExitCode {
    let konfig = match Konfig::aus_umgebung() {
        Ok(konfig) => konfig,
        Err(fehler) => return fehler_ausgeben(fehler),
    };
    ausgang_ausgeben(
        anmelden(
            konfig,
            &umgebung::graph_url(),
            TresorWahl::aus_umgebung(),
            OeffneBrowser::neu(),
        )
        .await,
    )
}

/// `quiver-m365 status`: ohne Browser prüfen, ob die Anmeldung noch gilt.
async fn status_ausfuehren() -> ExitCode {
    let konfig = match Konfig::aus_umgebung() {
        Ok(konfig) => konfig,
        Err(fehler) => return fehler_ausgeben(fehler),
    };
    ausgang_ausgeben(status(konfig, &umgebung::graph_url(), TresorWahl::aus_umgebung()).await)
}

/// `quiver-m365 abmelden`: Refresh-Token löschen.
fn abmelden_ausfuehren() -> ExitCode {
    let konfig = match Konfig::aus_umgebung() {
        Ok(konfig) => konfig,
        Err(fehler) => return fehler_ausgeben(fehler),
    };
    ausgang_ausgeben(abmelden(konfig, TresorWahl::aus_umgebung()))
}

/// Gibt aus, was ein Unterbefehl liefert, und macht daraus den Exit-Code.
fn ausgang_ausgeben(ausgang: Ausgang) -> ExitCode {
    if let Some(text) = ausgang.stdout {
        println!("{text}");
    }
    if let Some(text) = ausgang.stderr {
        eprintln!("quiver-m365: {text}");
    }
    ExitCode::from(ausgang.code)
}

fn fehler_ausgeben(fehler: anyhow::Error) -> ExitCode {
    eprintln!("quiver-m365: {fehler}");
    ExitCode::FAILURE
}
