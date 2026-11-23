// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! `quiver-outline`: MCP über stdio, `einrichten` für die Seite im Browser.
//!
//! Die `main` bleibt zwei Zeilen – den Rest entscheidet der Kern (ADR 0001).

use quiver_outline::{starten, Outline, OutlineServer};

#[tokio::main]
async fn main() -> std::process::ExitCode {
    starten(Outline, OutlineServer::neu).await
}
