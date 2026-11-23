// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Anmeldung an Microsoft Entra ID im Browser (SPEC.md §2):
//! Authorization Code + PKCE, Rückleitung über `http://localhost:<port>`.
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

pub const STANDARD_SCOPES: &[&str] = &[
    "User.Read",
    "Mail.ReadWrite",
    "Mail.Send",
    "Calendars.ReadWrite",
    "Files.Read.All",
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
