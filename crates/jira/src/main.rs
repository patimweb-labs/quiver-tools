// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! `quiver-jira`: MCP über stdio, `eintrag` für mcp-quiver, `testen` für den Zugang.
//!
//! Die `main` bleibt zwei Zeilen – den Rest entscheidet der Kern (ADR 0001).

use quiver_jira::{starten, Jira, JiraServer};

#[tokio::main]
async fn main() -> std::process::ExitCode {
    starten(Jira, JiraServer::neu).await
}
