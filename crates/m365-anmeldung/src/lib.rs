// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Anmeldung an Microsoft Entra ID im Browser (SPEC.md §2):
//! Authorization Code + PKCE, Rückleitung über `http://127.0.0.1:<port>`.
//!
//! Die Schnittstelle legt der Architekt fest; die Umsetzung steht in
//! `docs/aufgaben/01-anmeldung.md`.

use std::time::Duration;

/// Woher die Anmeldung ihre Werte bekommt (aus der Umgebung, SPEC.md §3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Konfig {
    pub client_id: String,
    /// Tenant-ID oder `organizations`.
    pub tenant: String,
    pub scopes: Vec<String>,
    /// `https://login.microsoftonline.com` oder die Attrappe im Test.
    pub authority: String,
    /// Wie lange ein Tool-Aufruf auf die Anmeldung im Browser wartet (Standard 45 s).
    pub warten: Duration,
    /// Wie lange der Lauscher im Hintergrund höchstens offen bleibt (Standard 5 min).
    pub lauscher_hoechstens: Duration,
}

/// Eingebaute Client-ID der einen Multi-Tenant-Registrierung (SPEC.md §2a).
///
/// Kein User und keine Firma muss etwas registrieren; eine eigene Registrierung
/// ist möglich, `M365_CLIENT_ID` überschreibt diesen Wert.
pub const STANDARD_CLIENT_ID: &str = "9ae8e0db-e05f-4db8-adde-83d70d416eeb";

pub const STANDARD_SCOPES: &[&str] = &[
    "User.Read",
    "Mail.ReadWrite",
    "Mail.Send",
    "Calendars.ReadWrite",
    "Files.Read.All",
    // Teams (docs/aufgaben/05-teams.md): delegiert, als der User.
    "Team.ReadBasic.All",
    "Channel.ReadBasic.All",
    "ChannelMessage.Read.All",
    "ChannelMessage.Send",
    "Chat.ReadWrite",
    "User.ReadBasic.All",
    // SharePoint/OneDrive, Planner/To Do, Kontakte/Personen, OneNote
    // (docs/aufgaben/06-graph-erweiterung.md): delegiert, als der User.
    "Sites.ReadWrite.All",
    "Files.ReadWrite.All",
    "Tasks.ReadWrite",
    "Group.Read.All",
    "Contacts.ReadWrite",
    "People.Read",
    "Notes.ReadWrite.All",
    "offline_access",
];

/// Wo das Refresh-Token liegt. Schlüssel: `<tenant>/<client_id>`.
pub trait Tresor: Send + Sync {
    fn lesen(&self, schluessel: &str) -> anyhow::Result<Option<String>>;
    fn schreiben(&self, schluessel: &str, wert: &str) -> anyhow::Result<()>;
    fn loeschen(&self, schluessel: &str) -> anyhow::Result<()>;
}

/// Öffnet eine URL im Browser. Echt: `open::that`. Im Test: ruft die URL direkt auf.
pub trait Browser: Send + Sync {
    fn oeffnen(&self, url: &str) -> anyhow::Result<()>;
}

/// Ergebnis von [`Anmeldung::access_token`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// Gültiges Access-Token (nie loggen, nie in Antworten).
    Bereit(String),
    /// Der Browser ist offen, die Anmeldung läuft noch. Das Tool sagt dem Agenten,
    /// er solle den Aufruf wiederholen.
    WartetAufBrowser,
}

#[derive(Debug, thiserror::Error)]
pub enum AnmeldeFehler {
    #[error("Anmeldung abgelehnt: {0}")]
    Abgelehnt(String),
    #[error("Anmeldung abgelaufen, bitte erneut versuchen")]
    Abgelaufen,
    #[error("state passt nicht – Anmeldung verworfen")]
    StateFalsch,
    /// Es gibt kein gültiges Refresh-Token und der Browser-Ablauf ist
    /// abgeschaltet (MCP-Betrieb, Aufgabe 03): Der User meldet sich im
    /// quiver-Fenster an, nicht aus einem Tool-Aufruf heraus.
    #[error("Nicht angemeldet")]
    NichtAngemeldet,
    /// Der Token-Endpunkt verlangt eine neue Zustimmung (`AADSTS65001`,
    /// `consent_required`) – typisch, wenn ein neuer Scope wie Teams dazukam.
    /// Kein Browser, das Refresh-Token bleibt: der User klickt im quiver-Fenster
    /// erneut auf „Anmelden“ (Aufgabe 05).
    #[error("Zustimmung fehlt")]
    ZustimmungFehlt,
    #[error("{0}")]
    Sonstiges(#[from] anyhow::Error),
}

/// Die Fassade (Umsetzung: Aufgabe 01). Klonbar, intern geteilt.
pub trait Anmeldung: Send + Sync {
    /// Gültiges Access-Token: aus dem Speicher, sonst per Refresh-Token, sonst
    /// Browser öffnen und bis `Konfig::warten` auf die Rückleitung warten.
    fn access_token(
        &self,
    ) -> impl std::future::Future<Output = Result<Token, AnmeldeFehler>> + Send;
    /// Nach einem 401 von Graph: Access-Token verwerfen (das nächste
    /// `access_token` erneuert per Refresh-Token).
    fn verwerfen(&self);
    /// Refresh-Token löschen.
    fn abmelden(&self) -> anyhow::Result<()>;
}

// ---------------------------------------------------------------------------
// Umsetzung (Aufgabe 01). Die Schnittstelle oben bleibt unverändert; hier
// werden nur die Module bekannt gemacht, die sie umsetzen.
// ---------------------------------------------------------------------------

mod anmeldung;
mod browser;
mod dateitresor;
mod fluss;
mod konfig;
mod tresor;

pub use anmeldung::EntraAnmeldung;
pub use browser::{KeinBrowser, OeffneBrowser};
pub use tresor::{DateiTresor, SpeicherTresor};
