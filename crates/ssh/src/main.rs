// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! `quiver-ssh`: MCP über stdio, `eintrag` für mcp-quiver, `testen` für den Zugang.
//!
//! Die `main` bleibt zwei Zeilen – den Rest entscheidet der Kern (ADR 0001).

use quiver_ssh::{starten, Ssh, SshServer};

#[tokio::main]
async fn main() -> std::process::ExitCode {
    starten(Ssh, SshServer::neu).await
}
