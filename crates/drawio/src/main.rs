// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! `quiver-drawio`: MCP über stdio, `eintrag` für mcp-quiver, `testen` für den Arbeitsordner.
//!
//! Die `main` bleibt zwei Zeilen – den Rest entscheidet der Kern (ADR 0001).

use quiver_drawio::{starten, Drawio, DrawioServer};

#[tokio::main]
async fn main() -> std::process::ExitCode {
    starten(Drawio, DrawioServer::neu).await
}
