// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! `quiver-excalidraw` – Excalidraw-Dateien als MCP-Tool (Aufgabe 17).
//!
//! | Modul | Aufgabe |
//! |---|---|
//! | [`werkzeug`] | die zwei Felder (`ordner`, `schreiben`) und `testen` |
//! | [`datei`] | das Format: `type`, `version`, `source`, `elements`, `appState`, `files` |
//! | [`pruefen`] | die strenge Prüfung (Pflichtfelder, Bindungen beiderseits, `link`) |
//! | [`bauen`] | Modell → Elemente; Formen, Texte, Pfeile, Randpunkte |
//! | [`aendern`] | Text, Farbe, Form, Lage, Verbinden, Lösen, Entfernen |
//! | [`bild`] | Datei → [`quiver_diagramm::zeichnen`] → SVG/PNG |
//! | [`farben`] | Namen ↔ Werte, Formen, Kantenstile |
//! | [`browser`] | wer excalidraw.com öffnet (hinter einem Trait) |
//! | [`format`] | reine Funktionen: Daten → Text |
//! | [`server`] | die elf MCP-Werkzeuge |
//!
//! Der gemeinsame Unterbau (Ablage, Modell, Layout, Zeichnen) liegt in
//! [`quiver_diagramm`]; dieses Crate kennt nur das Excalidraw-Format. Es gibt
//! **keine** Zugangsdaten: gearbeitet wird auf Dateien in einem Arbeitsordner.
//!
//! ```no_run
//! use quiver_diagramm::modell::{Diagramm, Knoten};
//! use quiver_excalidraw::bauen;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut diagramm = Diagramm::leer();
//! diagramm.knoten = vec![Knoten::neu("start", "Antrag")];
//! let datei = bauen::aus_diagramm(&diagramm)?;
//! println!("{}", datei.als_text()?);
//! # Ok(())
//! # }
//! ```

pub mod aendern;
pub mod bauen;
pub mod bild;
pub mod browser;
pub mod datei;
pub mod farben;
pub mod fehler;
pub mod format;
pub mod pruefen;
pub mod server;
pub mod werkzeug;

pub use datei::{Datei, Element};
pub use fehler::Fehler;
pub use server::ExcalidrawServer;
pub use werkzeug::{Excalidraw, TITEL};

pub use quiver_tools_kern::{starten, Profil, Werkzeug, Zugang};
