// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! `quiver-abap`: MCP über stdio, `eintrag` für mcp-quiver, `testen` für den Zugang.
//!
//! Die `main` bleibt zwei Zeilen – den Rest entscheidet der Kern (ADR 0001).

use quiver_abap::{starten, Abap, AbapServer};

#[tokio::main]
async fn main() -> std::process::ExitCode {
    starten(Abap, AbapServer::neu).await
}
