// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! quiver-m365: Microsoft 365 als MCP-Server über stdio (SPEC.md).
//!
//! Die Bibliothek ist das Programm ohne `main`: `main.rs` verdrahtet nur
//! Umgebung, Tresor, Browser und Transport. So sind die Tools testbar.

pub mod befehle;
pub mod freigeben;
pub mod kodierung;
pub mod server;
pub mod tresorwahl;
pub mod umgebung;
pub mod zeit;

pub use server::{Quiver, ANMELDUNG_HINWEIS};
pub use tresorwahl::TresorWahl;
