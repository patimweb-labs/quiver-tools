// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Wer den Browser öffnet (Aufgabe 06, Notfall `anmelden --browser`).
//!
//! Der Normalfall ist die Routine ohne Browser. Nur wenn sie an einer
//! unbekannten Seite scheitert (etwa einer MFA-Abfrage), öffnet der Befehl
//! `quiver-zep anmelden --browser` den Standardbrowser und wartet auf die
//! Rückleitung an `127.0.0.1`.

/// Öffnet eine Adresse im Browser.
pub trait Browser: Send + Sync {
    fn oeffnen(&self, url: &str) -> anyhow::Result<()>;
}

/// Der Standardbrowser des Systems.
#[derive(Debug, Clone, Copy, Default)]
pub struct OeffneBrowser;

impl OeffneBrowser {
    pub fn neu() -> Self {
        Self
    }
}

impl Browser for OeffneBrowser {
    fn oeffnen(&self, url: &str) -> anyhow::Result<()> {
        open::that(url)?;
        Ok(())
    }
}

/// Ein Browser, der nichts öffnet. Im MCP-Betrieb und in Tests ist das der
/// Normalfall: kein Werkzeug-Aufruf startet ungefragt einen Browser.
#[derive(Debug, Clone, Copy, Default)]
pub struct KeinBrowser;

impl KeinBrowser {
    pub fn neu() -> Self {
        Self
    }
}

impl Browser for KeinBrowser {
    /// Die Adresse landet bewusst **nicht** im Fehlertext: sie trägt `state`
    /// und `code_challenge`.
    fn oeffnen(&self, _url: &str) -> anyhow::Result<()> {
        anyhow::bail!("In diesem Betrieb öffnet das Werkzeug keinen Browser (nur mit --browser).")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn der_stille_browser_oeffnet_nichts_und_nennt_keine_adresse() {
        let fehler = KeinBrowser
            .oeffnen("https://example.test/login?state=geheim")
            .unwrap_err();
        let text = fehler.to_string();
        assert!(text.contains("keinen Browser"), "{text}");
        assert!(!text.contains("state"));
        assert!(!text.contains("geheim"));
    }
}
