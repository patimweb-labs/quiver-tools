// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! `quiver-ssh` – SSH und SFTP als MCP-Tool (SPEC §5, Aufgabe 05).
//!
//! | Modul | Aufgabe | Zustand |
//! |---|---|---|
//! | [`werkzeug`] | die Felder einer Verbindung, `mehrfach`, `testen` | keiner |
//! | [`verbindung`] | `QUIVER_ZUGAENGE` → geprüfte `Verbindung`en, Auswahl | Umgebungsvariable |
//! | [`pfade`] | wo Schlüssel und `known_hosts` liegen, enge Rechte | Dateien |
//! | [`schluessel`] | ed25519 erzeugen, laden, übernehmen, vergleichen | Dateien |
//! | [`hostschluessel`] | TOFU gegen eigene und `~/.ssh/known_hosts` | Dateien |
//! | [`anmeldung`] | der Ablauf: Schlüssel, erste Einrichtung, Hinweise | Dateien + Netz |
//! | [`client`] | der einzige Ort mit Netzverkehr (russh, russh-sftp) | Sitzung |
//! | [`format`] | reine Funktionen: Daten → Text | keiner |
//! | [`server`] | die sechs MCP-Werkzeuge | Verbindungen aus der Umgebung |
//!
//! Zugangsdaten stehen nie in einer Antwort: `Debug` zeigt von [`verbindung::Verbindung`]
//! nur, *ob* ein Passwort gesetzt ist, und das sudo-Passwort geht ausschließlich
//! über stdin des entfernten Befehls.

pub mod anmeldung;
pub mod client;
pub mod format;
pub mod hostschluessel;
pub mod pfade;
pub mod schluessel;
pub mod server;
pub mod verbindung;
pub mod werkzeug;

pub use server::SshServer;
pub use werkzeug::{Ssh, TITEL};

pub use quiver_tools_kern::{starten, Profil, Werkzeug, Zugang};
