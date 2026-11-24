// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! `quiver-confluence` – Confluence Cloud als MCP-Tool (Aufgabe 14).
//!
//! | Modul | Aufgabe | Zustand |
//! |---|---|---|
//! | [`werkzeug`] | die Felder einer Site, `mehrfach`, `testen` | keiner |
//! | [`sites`] | `QUIVER_ZUGAENGE` → geprüfte `Site`s, Auswahl, `nur` | Umgebungsvariable |
//! | [`client`] | der einzige Ort mit Netzverkehr (reqwest): Basic, Folgeseiten, 429 | keiner |
//! | [`fehler`] | 401, 403, 409, HTTP, Netz – ohne Token | keiner |
//! | [`daten`] | JSON → Werte (Punktpfade, Listen, Datum) | keiner |
//! | [`html`] | Storage-Format → Text, Text → Storage-Format | keiner |
//! | [`cql`] | CQL aus den Komfort-Parametern, korrekt maskiert | keiner |
//! | [`format`] | reine Funktionen: Daten → Text | keiner |
//! | [`server`] | die 13 MCP-Werkzeuge | Sites aus der Umgebung |
//!
//! Zugangsdaten stehen nie in einer Antwort: [`sites::Site`] zeigt im `Debug`
//! nur, *ob* Token und E-Mail gesetzt sind, und der Client schickt beides
//! ausschließlich als Basic-Kopfzeile (`email:token`) an den eingetragenen
//! Host – ohne Weiterleitungen und ohne fremde Folgeseiten.

pub mod client;
pub mod cql;
pub mod daten;
pub mod fehler;
pub mod format;
pub mod html;
pub mod server;
pub mod sites;
pub mod werkzeug;

pub use quiver_tools_kern::{starten, Profil, Werkzeug, Zugang};
pub use server::ConfluenceServer;
pub use werkzeug::{Confluence, TITEL};
