// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! `quiver-email` – E-Mail über IMAP und SMTP als MCP-Tool (SPEC §5,
//! Aufgabe 04).
//!
//! Aufbau wie bei `quiver-outline`, nur mit zwei Protokollen:
//!
//! | Modul | Aufgabe |
//! |---|---|
//! | [`werkzeug`] | `Werkzeug`-Beschreibung: Felder, Vorgaben, `testen` |
//! | [`domain`] | Daten und reine Regeln (Adressen, Threading, Suche) |
//! | [`fehler`] | [`MailFehler`] – nie mit Passwort |
//! | [`postfach`] | `Postfach` (Naht) und `async-imap` mit `tokio-rustls` |
//! | [`versand`] | `Versand` (Naht) und `lettre` mit rustls |
//! | [`format`] | reine Funktionen: Daten → knapper Text |
//! | [`server`] | die zwölf MCP-Werkzeuge über stdio |
//!
//! Den Zugang trägt der User im quiver-Fenster ein (SPEC §2); der Kern liest
//! ihn aus der Umgebung (`EMAIL_ADRESSE`, `EMAIL_PASSWORT`, …). Das Passwort
//! liegt in der Tresor-Datei und erscheint in keiner Antwort.

pub mod domain;
pub mod fehler;
pub mod format;
pub mod postfach;
pub mod server;
pub mod versand;
pub mod werkzeug;

pub use domain::{
    Adresse, Anhang, ImapKonfiguration, Konto, Kopfzeilen, KopfzeilenSeite, LoeschErgebnis,
    Nachricht, PostfachInfo, Sicherheit, SmtpKonfiguration, Suchkriterien, VersandErgebnis,
    VersandNachricht,
};
pub use fehler::MailFehler;
pub use postfach::{ImapPostfach, Postfach};
pub use server::{Echt, EmailServer, Fabrik};
pub use versand::{LettreVersand, Versand};
pub use werkzeug::{Email, NAME, TITEL};

pub use quiver_tools_kern::{starten, Profil, Werkzeug, Zugang};
