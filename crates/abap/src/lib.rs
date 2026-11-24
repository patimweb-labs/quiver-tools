// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! `quiver-abap` – ABAP über SAP ADT als MCP-Tool (SPEC §5, Aufgaben 07 und 08).
//!
//! | Modul | Aufgabe | Zustand |
//! |---|---|---|
//! | [`werkzeug`] | die Felder eines Systems, `mehrfach`, `testen` | keiner |
//! | [`systeme`] | `QUIVER_ZUGAENGE` → geprüfte `System`e, Auswahl | Umgebungsvariable |
//! | [`adt::client`] | der einzige Ort mit Netzverkehr (reqwest) | Token + Cookies je System |
//! | [`adt::sitzung`] | die zustandsbehaftete Sitzung je System, Lock-Tabelle | Cookies, Token, Locks |
//! | [`adt::xml`] | XML → Baum, ohne Namensräume | keiner |
//! | [`adt::daten`] | reine Leser: XML → Daten | keiner |
//! | [`adt::fehler`] | ADT-Ausnahme, HTTP, 401, abgelaufene Sitzung | keiner |
//! | [`schreiben`] | Schutzregeln und Rümpfe der schreibenden Werkzeuge | keiner |
//! | [`sql`] | „nur ein SELECT“ für die Datenvorschau | keiner |
//! | [`format`] | reine Funktionen: Daten → Text | keiner |
//! | [`server`] | die 25 MCP-Werkzeuge | Systeme aus der Umgebung, Sitzungen |
//!
//! Zugangsdaten stehen nie in einer Antwort: [`systeme::System`] zeigt im
//! `Debug` nur, *ob* ein Passwort gesetzt ist, und der Client schickt es
//! ausschließlich als Basic-Auth-Kopfzeile.

pub mod adt;
pub mod format;
pub mod schreiben;
pub mod server;
pub mod sql;
pub mod systeme;
pub mod werkzeug;

pub use server::AbapServer;
pub use werkzeug::{Abap, TITEL};

pub use quiver_tools_kern::{starten, Profil, Werkzeug, Zugang};
