// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! `quiver-zephyr`: MCP über stdio, `eintrag` für mcp-quiver, `testen` für den Zugang.
//!
//! Die `main` bleibt zwei Zeilen – den Rest entscheidet der Kern (ADR 0001).

use quiver_zephyr::{starten, Zephyr, ZephyrServer};

#[tokio::main]
async fn main() -> std::process::ExitCode {
    starten(Zephyr, ZephyrServer::neu).await
}
