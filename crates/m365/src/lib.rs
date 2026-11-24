// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! quiver-m365: Microsoft 365 als MCP-Server über stdio (SPEC.md).
//!
//! Die Bibliothek ist das Programm ohne `main`: `main.rs` verdrahtet nur
//! Umgebung, Tresor, Browser und Transport. So sind die Tools und die
//! Unterbefehle testbar.

pub mod befehle;
pub mod browserwahl;
pub mod freigeben;
pub mod kodierung;
pub mod server;
pub mod teams;
pub mod teams_tools;
pub mod tresorwahl;
pub mod umgebung;
pub mod unterbefehle;
pub mod zeit;

pub use browserwahl::BrowserWahl;
pub use server::{Quiver, ANMELDUNG_HINWEIS, NICHT_ANGEMELDET_HINWEIS, ZUSTIMMUNG_HINWEIS};
pub use tresorwahl::TresorWahl;
pub use unterbefehle::Ausgang;
