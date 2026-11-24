// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! `quiver-excalidraw`: MCP über stdio, `eintrag` für mcp-quiver, `testen` für den Arbeitsordner.
//!
//! Die `main` bleibt zwei Zeilen – den Rest entscheidet der Kern (ADR 0001).

use quiver_excalidraw::{starten, Excalidraw, ExcalidrawServer};

#[tokio::main]
async fn main() -> std::process::ExitCode {
    starten(Excalidraw, ExcalidrawServer::neu).await
}
