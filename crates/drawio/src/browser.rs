// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Wer den Browser öffnet (Aufgabe 18, Nachtrag).
//!
//! `drawio_oeffnen` startet den **Standardbrowser** mit dem selbst gebauten
//! Link. Der Start liegt hinter [`Browser`], damit kein Test je einen Browser
//! öffnet: die Tests setzen einen Aufzeichner ein.
//!
//! Es wird keine Shell gestartet (`open` ruft den Systemdienst direkt), und das
//! Werkzeug öffnet **nie** eine Adresse, die ein Agent genannt hat.

/// Öffnet eine Adresse im Standardbrowser.
pub trait Browser: Send + Sync {
    /// Öffnet genau diese Adresse; ein Fehler ist eine Meldung ohne Adresse.
    fn oeffnen(&self, adresse: &str) -> Result<(), String>;
}

/// Der Standardbrowser des Systems.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemBrowser;

impl SystemBrowser {
    /// Der echte Browser.
    pub fn neu() -> SystemBrowser {
        SystemBrowser
    }
}

impl Browser for SystemBrowser {
    fn oeffnen(&self, adresse: &str) -> Result<(), String> {
        // Keine Shell: `open` übergibt die Adresse direkt an den Systemdienst.
        open::that_detached(adresse).map_err(|fehler| fehler.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ein Browser, der nichts tut – für Tests.
    struct Nichts;

    impl Browser for Nichts {
        fn oeffnen(&self, _adresse: &str) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn der_browser_ist_hinter_dem_trait() {
        let browser: Box<dyn Browser> = Box::new(Nichts);
        assert!(browser.oeffnen("https://example.test").is_ok());
    }
}
