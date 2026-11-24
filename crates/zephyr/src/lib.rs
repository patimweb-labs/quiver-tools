// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! `quiver-zephyr` – Zephyr Scale Cloud (REST v2) als MCP-Tool (Aufgabe 15).
//!
//! | Modul | Aufgabe | Zustand |
//! |---|---|---|
//! | [`werkzeug`] | die Felder eines Zugangs, `mehrfach`, `testen` | keiner |
//! | [`zugaenge`] | `QUIVER_ZUGAENGE` → geprüfte `Zugang`e, Auswahl, `https`/Loopback | Umgebungsvariable |
//! | [`client`] | der einzige Ort mit Netzverkehr (reqwest): Bearer, keine Weiterleitung, 429/503 | keiner |
//! | [`fehler`] | 401, 403, HTTP, Weiterleitung, fremde Folgeseite – ohne Token | keiner |
//! | [`daten`] | JSON → Werte (Punktpfade, Verweise, Listen, Seiten, Datum) | keiner |
//! | [`namen`] | Verweise (`id`/`self`) → Namen über die Endpunkte der OpenAPI | merkt Antworten je Aufruf |
//! | [`format`] | reine Funktionen: Daten → Text, auch der Rumpf für `PUT` | keiner |
//! | [`server`] | die 14 MCP-Werkzeuge | Zugänge aus der Umgebung |
//!
//! Zugangsdaten stehen nie in einer Antwort: [`zugaenge::Zugang`] zeigt im
//! `Debug` nur, *ob* ein Token gesetzt ist, und der Client schickt es
//! ausschließlich als `Authorization: Bearer` an die eingetragene Basisadresse.

pub mod client;
pub mod daten;
pub mod fehler;
pub mod format;
pub mod namen;
pub mod server;
pub mod werkzeug;
pub mod zugaenge;

pub use server::ZephyrServer;
pub use werkzeug::{Zephyr, TITEL};

pub use quiver_tools_kern::{starten, Profil, Werkzeug, Zugang};
