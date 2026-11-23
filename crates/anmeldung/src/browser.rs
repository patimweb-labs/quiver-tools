// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der echte Browser (SPEC.md §2): `open::that` – und [`KeinBrowser`] für den
//! Betrieb, in dem die Anmeldung aus dem quiver-Fenster kommt (Aufgabe 03).

use crate::Browser;

/// Öffnet eine URL im Standardbrowser des Systems.
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

/// Ein Browser, der nichts öffnet: er liefert einen eigenen Fehler.
///
/// Im MCP-Betrieb öffnet `quiver-m365` nie von selbst einen Browser; die
/// Anmeldung löst der User im quiver-Fenster aus (Aufgabe 03). `M365_BROWSER=erlaubt`
/// schaltet das alte Verhalten wieder ein.
#[derive(Debug, Clone, Copy, Default)]
pub struct KeinBrowser;

impl KeinBrowser {
    pub fn neu() -> Self {
        Self
    }
}

impl Browser for KeinBrowser {
    /// Die URL landet bewusst **nicht** im Fehlertext (sie trägt `state` und
    /// `code_challenge`, beides gehört nicht in ein Log).
    fn oeffnen(&self, _url: &str) -> anyhow::Result<()> {
        anyhow::bail!("kein Browser in diesem Betrieb – Anmeldung im quiver-Fenster")
    }
}
