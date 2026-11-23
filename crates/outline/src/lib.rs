// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! `quiver-outline` – das Outline-Wiki als MCP-Tool (SPEC §5, Aufgabe 02).
//!
//! Aufbau wie in der Vorlage `pi-outline-wiki`, nur in Rust und ohne den
//! TypeScript-Code zu übersetzen:
//!
//! | Modul | Aufgabe |
//! |---|---|
//! | [`werkzeug`] | `Werkzeug`-Beschreibung (Felder, Adressprüfung, `testen`) |
//! | [`client`] | der einzige Ort mit Netzverkehr: `POST /api/<methode>` |
//! | [`domain`] | die Daten, die Outline liefert (`serde`) |
//! | [`format`] | reine Funktionen: Daten → Text |
//! | [`datei`] | Markdown auf die Platte, ohne aus dem Zielordner auszubrechen |
//! | [`server`] | die MCP-Werkzeuge über stdio |
//!
//! Den Zugang trägt der User im quiver-Fenster ein (SPEC §2); der Kern liest
//! ihn aus der Umgebung (`OUTLINE_URL`, `OUTLINE_API_KEY`). Der API-Schlüssel
//! liegt in der Tresor-Datei und erscheint in keiner Antwort.

pub mod client;
pub mod datei;
pub mod domain;
pub mod format;
pub mod server;
pub mod werkzeug;

pub use client::{Client, OutlineFehler, Seiten};
pub use server::OutlineServer;
pub use werkzeug::{wurzel, Outline, ADRESSE, NAME, SCHLUESSEL, TITEL};

pub use quiver_tools_kern::{starten, Profil, Werkzeug, Zugang};
