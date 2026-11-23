// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! `quiver-zep` – der offizielle ZEP-MCP als MCP-Tool, dauerhaft angemeldet
//! (Aufgabe 06).
//!
//! Kein Nachbau von ZEP: dieses Werkzeug ist ein lokaler stdio-MCP, der den
//! ZEP-MCP (Streamable HTTP, OAuth) anspricht und dessen Werkzeuge 1:1
//! durchreicht. Seine eigene Aufgabe ist nur: **angemeldet bleiben**.
//!
//! | Modul | Aufgabe |
//! |---|---|
//! | [`werkzeug`] | `Werkzeug`-Beschreibung (Felder, `testen`) und die Konfiguration |
//! | [`metadaten`] | die Metadaten von MCP und Authorization Server (RFC 9728/8414) |
//! | [`oauth`] | Registrierung (DCR), Authorize-Adresse, Code-Tausch, Erneuerung |
//! | [`pkce`] | PKCE (S256) und `state` |
//! | [`html`] | der kleine HTML-Formular-Leser für die Anmeldeseiten |
//! | [`cookies`] | der Cookie-Speicher des Ablaufs |
//! | [`routine`] | der Ablauf ohne Browser: Formular, Zustimmung, Code |
//! | [`browser`] | der Notfall über den Browser (Loopback) |
//! | [`anmeldung`] | Token, Tresor, Sperrschutz, Hintergrund-Erneuerung |
//! | [`tresor`] | Schlüsselbund (Dienst `quiver-zep`, Schlüssel `<url>`) |
//! | [`sperre`] | der Sperrzustand in einer Datei |
//! | [`vermittler`] | der rmcp-Client zum ZEP-MCP |
//! | [`server`] | der MCP-Server über stdio |
//! | [`befehle`] | `anmelden`, `status`, `abmelden`, `routine-pruefen` |
//!
//! Der User trägt Adresse, Benutzer und Passwort im quiver-Fenster ein (SPEC §2);
//! der Kern liest sie aus der Umgebung (`ZEP_URL`, `ZEP_BENUTZER`,
//! `ZEP_PASSWORT`). Kein Passwort und kein Token verlässt dieses Programm in
//! einer Antwort, einem Log oder einer Datei.

pub mod anmeldung;
pub mod befehle;
pub mod browser;
pub mod cookies;
pub mod html;
pub mod metadaten;
pub mod oauth;
pub mod pkce;
pub mod routine;
pub mod server;
pub mod sperre;
pub mod tresor;
pub mod vermittler;
pub mod werkzeug;

pub use anmeldung::{AnmeldeFehler, Anmeldung, RoutineLauf, Status, HINWEIS_ANMELDUNG};
pub use browser::{Browser, KeinBrowser, OeffneBrowser};
pub use server::ZepServer;
pub use tresor::{Schluesselbund, SpeicherTresor, Tresor};
pub use vermittler::Vermittler;
pub use werkzeug::{Zep, ZepKonfig, ADRESSE, BENUTZER, NAME, PASSWORT, TITEL};

pub use quiver_tools_kern::{starten, Profil, Werkzeug, Zugang};
