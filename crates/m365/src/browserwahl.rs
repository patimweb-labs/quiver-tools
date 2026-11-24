// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Darf `quiver-m365` im MCP-Betrieb einen Browser öffnen? (Aufgabe 03)
//!
//! Standard: **nein**. Die Anmeldung löst der User im quiver-Fenster aus
//! (Knopf „Anmelden“), nie ein Agent über einen Tool-Aufruf. Wer `quiver-m365`
//! ohne quiver benutzt, wählt mit `M365_BROWSER=erlaubt` das alte Verhalten.

use m365_anmeldung::{Browser, KeinBrowser, OeffneBrowser};

/// Der Browser aus der Umgebung, als ein Typ (die Anmeldung ist generisch).
pub enum BrowserWahl {
    /// `M365_BROWSER=erlaubt`: der Browser-Ablauf aus einem Tool heraus.
    Erlaubt(OeffneBrowser),
    /// Standard: kein Browser, Anmeldung nur über `quiver-m365 anmelden`.
    Aus(KeinBrowser),
}

impl BrowserWahl {
    /// Die reine Regel: `erlaubt` (Groß-/Kleinschreibung egal) → Browser,
    /// alles andere → keiner.
    pub fn aus_wert(roh: Option<&str>) -> Self {
        match roh.map(|wert| wert.trim().to_ascii_lowercase()) {
            Some(wert) if wert == "erlaubt" => Self::Erlaubt(OeffneBrowser::neu()),
            _ => Self::Aus(KeinBrowser::neu()),
        }
    }

    /// `M365_BROWSER` aus der Umgebung.
    pub fn aus_umgebung() -> Self {
        let roh = crate::umgebung::wert("M365_BROWSER");
        Self::aus_wert(roh.as_deref())
    }

    /// Darf ein Browser aufgehen?
    pub fn erlaubt(&self) -> bool {
        matches!(self, Self::Erlaubt(_))
    }

    /// Nur für Tests und Logs: welcher Betrieb ist es?
    pub fn name(&self) -> &'static str {
        match self {
            Self::Erlaubt(_) => "erlaubt",
            Self::Aus(_) => "aus",
        }
    }
}

impl Browser for BrowserWahl {
    fn oeffnen(&self, url: &str) -> anyhow::Result<()> {
        match self {
            Self::Erlaubt(browser) => browser.oeffnen(url),
            Self::Aus(browser) => browser.oeffnen(url),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ohne `M365_BROWSER` bleibt der Browser zu: die Anmeldung kommt aus quiver.
    #[test]
    fn standard_ist_kein_browser() {
        assert!(!BrowserWahl::aus_wert(None).erlaubt());
        assert!(!BrowserWahl::aus_wert(Some("")).erlaubt());
        assert!(!BrowserWahl::aus_wert(Some("nein")).erlaubt());
        assert_eq!(BrowserWahl::aus_wert(None).name(), "aus");
    }

    #[test]
    fn erlaubt_schaltet_den_browser_ein() {
        for wert in ["erlaubt", " Erlaubt ", "ERLAUBT"] {
            let wahl = BrowserWahl::aus_wert(Some(wert));
            assert!(wahl.erlaubt(), "{wert}");
            assert_eq!(wahl.name(), "erlaubt");
        }
    }
}
