// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! `quiver-jira` – Jira Cloud als MCP-Tool (SPEC §5, Aufgabe 13).
//!
//! | Modul | Aufgabe | Zustand |
//! |---|---|---|
//! | [`werkzeug`] | die Felder einer Site, `mehrfach`, `testen` | keiner |
//! | [`sites`] | `QUIVER_ZUGAENGE` → geprüfte `Site`s, Auswahl, `nur` | Umgebungsvariable |
//! | [`client`] | der einzige Ort mit Netzverkehr (reqwest): Basic, Pfad- und `next`-Prüfung, 429, Rumpf-Fehler | keiner |
//! | [`fehler`] | 401, 403, HTTP, Netz, unlesbarer Rumpf, unklarer Ausgang – ohne Token | keiner |
//! | [`daten`] | JSON → Werte (Punktpfade, Listen, Datum) | keiner |
//! | [`adf`] | Text ⇄ Atlassian Document Format | keiner |
//! | [`jql`] | JQL aus den Komfort-Parametern, korrekt maskiert | keiner |
//! | [`uebergang`] | welcher Workflow-Übergang gemeint ist | keiner |
//! | [`format`] | reine Funktionen: Daten → Text | keiner |
//! | [`server`] | die 17 MCP-Werkzeuge | Sites aus der Umgebung |
//!
//! Zugangsdaten stehen nie in einer Antwort: [`sites::Site`] zeigt im `Debug`
//! nur, *ob* E-Mail und Token gesetzt sind, und der Client schickt sie
//! ausschließlich als Basic-Auth-Kopfzeile (`email:token`). Weiterleitungen
//! folgt der Client nicht, eine Folgeseite mit fremdem Host wird **vor** der
//! Anfrage abgelehnt, und ein Pfadwert mit `.`/`..` (auch `%2E%2E`) kommt gar
//! nicht erst in eine Adresse.

pub mod adf;
pub mod client;
pub mod daten;
pub mod fehler;
pub mod format;
pub mod jql;
pub mod server;
pub mod sites;
pub mod uebergang;
pub mod werkzeug;

pub use server::JiraServer;
pub use werkzeug::{Jira, TITEL};

pub use quiver_tools_kern::{starten, Profil, Werkzeug, Zugang};
