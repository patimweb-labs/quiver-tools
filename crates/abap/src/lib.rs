// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! `quiver-abap` – ABAP über SAP ADT als MCP-Tool (SPEC §5, Aufgabe 07).
//!
//! | Modul | Aufgabe | Zustand |
//! |---|---|---|
//! | [`werkzeug`] | die Felder eines Systems, `mehrfach`, `testen` | keiner |
//! | [`systeme`] | `QUIVER_ZUGAENGE` → geprüfte `System`e, Auswahl | Umgebungsvariable |
//! | [`adt::client`] | der einzige Ort mit Netzverkehr (reqwest) | Token + Cookies je System |
//! | [`adt::xml`] | XML → Baum, ohne Namensräume | keiner |
//! | [`adt::daten`] | reine Leser: XML → Daten | keiner |
//! | [`adt::fehler`] | ADT-Ausnahme, HTTP, 401 | keiner |
//! | [`format`] | reine Funktionen: Daten → Text | keiner |
//! | [`server`] | die neunzehn lesenden MCP-Werkzeuge | Systeme aus der Umgebung |
//!
//! Zugangsdaten stehen nie in einer Antwort: [`systeme::System`] zeigt im
//! `Debug` nur, *ob* ein Passwort gesetzt ist, und der Client schickt es
//! ausschließlich als Basic-Auth-Kopfzeile.

pub mod adt;
pub mod format;
pub mod server;
pub mod systeme;
pub mod werkzeug;

pub use server::AbapServer;
pub use werkzeug::{Abap, TITEL};

pub use quiver_tools_kern::{starten, Profil, Werkzeug, Zugang};
