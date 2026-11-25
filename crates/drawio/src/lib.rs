// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! `quiver-drawio` – draw.io als MCP-Tool (Aufgabe 18,
//! [ADR 0012](../../docs/adr/0012-drawio.md)).
//!
//! | Modul | Aufgabe |
//! |---|---|
//! | [`xml`] | strenger XML-Leser/-Schreiber: DTD und Entitäten abgelehnt, Tiefe begrenzt, Attribute maskiert |
//! | [`mx`] | das Format: Datei, Seiten, Zellen, Hüllen, Prüfung, Ändern, Entfernen |
//! | [`stil`] | die Stil-Zeichenkette: ein Teil ändern, den Rest stehen lassen |
//! | [`formen`] | die Formen und Farben, die dieses Werkzeug setzt |
//! | [`link`] | Base64 + raw inflate + Prozentkodierung, der `#R`-Link |
//! | [`aus_modell`] | Knoten/Kanten/Gruppen (mit Layout) → draw.io-Zellen |
//! | [`bild`] | draw.io-Zellen → SVG/PNG mit dem eigenen Zeichner |
//! | [`browser`] | wer den Browser öffnet (im Test nie) |
//! | [`format`] | reine Funktionen: Daten → Antworttext |
//! | [`server`] | die MCP-Werkzeuge |
//!
//! Es gibt **keine** Zugangsdaten: dieses Werkzeug arbeitet nur auf Dateien im
//! Arbeitsordner. Geschrieben wird nur dort, nie über Vorhandenes und nur mit
//! dem gelesenen `stand` (Aufgabe 16).

#![forbid(unsafe_code)]

pub mod aus_modell;
pub mod bild;
pub mod browser;
pub mod fehler;
pub mod format;
pub mod formen;
pub mod link;
pub mod mx;
pub mod server;
pub mod stil;
pub mod werkzeug;
pub mod xml;

pub use server::DrawioServer;
pub use werkzeug::{Drawio, TITEL};

pub use quiver_tools_kern::{starten, Profil, Werkzeug, Zugang};
