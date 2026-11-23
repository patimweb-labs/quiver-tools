// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! `quiver-zep`: MCP über stdio, `eintrag` für mcp-quiver, `testen` für den
//! Zugang – und die eigenen Wege `anmelden`, `status`, `abmelden`,
//! `routine-pruefen`.
//!
//! Die Reihenfolge ist wichtig: was hier nicht steht, entscheidet der Kern.

use std::process::ExitCode;

use quiver_zep::befehle;
use quiver_zep::{starten, Zep, ZepServer};

#[tokio::main]
async fn main() -> ExitCode {
    if let Some(befehl) = befehle::aus_args(std::env::args().skip(1)) {
        return befehle::ausfuehren(befehl).await;
    }
    starten(Zep, ZepServer::neu).await
}
