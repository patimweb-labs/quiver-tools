// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! `quiver-devops` – Azure DevOps als MCP-Tool (SPEC §5, Aufgabe 09).
//!
//! | Modul | Aufgabe | Zustand |
//! |---|---|---|
//! | [`werkzeug`] | die Felder einer Organisation, `mehrfach`, `testen` | keiner |
//! | [`organisationen`] | `QUIVER_ZUGAENGE` → geprüfte `Organisation`en, Auswahl | Umgebungsvariable |
//! | [`client`] | der einzige Ort mit Netzverkehr (reqwest): PAT, `api-version`, 429 | keiner |
//! | [`fehler`] | 401, 203 mit HTML, HTTP, Netz – ohne PAT | keiner |
//! | [`daten`] | JSON → Werte (Punktpfade, Listen, Datum) | keiner |
//! | [`html`] | HTML → Text für Beschreibungen und Kommentare | keiner |
//! | [`wiql`] | WIQL aus den Komfort-Parametern | keiner |
//! | [`format`] | reine Funktionen: Daten → Text | keiner |
//! | [`server`] | die 26 MCP-Werkzeuge | Organisationen aus der Umgebung |
//!
//! Zugangsdaten stehen nie in einer Antwort: [`organisationen::Organisation`]
//! zeigt im `Debug` nur, *ob* ein PAT gesetzt ist, und der Client schickt ihn
//! ausschließlich als Basic-Auth-Kopfzeile (`":"` + PAT).

pub mod client;
pub mod daten;
pub mod fehler;
pub mod format;
pub mod html;
pub mod organisationen;
pub mod server;
pub mod werkzeug;
pub mod wiql;

pub use server::DevopsServer;
pub use werkzeug::{Devops, TITEL};

pub use quiver_tools_kern::{starten, Profil, Werkzeug, Zugang};
