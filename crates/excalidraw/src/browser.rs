// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Wer den Browser öffnet (Aufgabe 17, Nachtrag „Im Browser öffnen“).
//!
//! `excalidraw_oeffnen` öffnet die **feste** Adresse `https://excalidraw.com`
//! im Standardbrowser und nennt den vollen Pfad der Datei mit dem Hinweis,
//! sie in das Fenster zu ziehen. Es gibt keinen Link, der den Inhalt nur im
//! Fragment trägt (`#json=` lädt hoch) – darum wird **nichts** hochgeladen.
//!
//! Der Browser liegt hinter einem Trait: die Tests öffnen nie einen Browser.

/// Die Adresse, die `excalidraw_oeffnen` öffnet. Fest, ohne Zusatz.
pub const ADRESSE: &str = "https://excalidraw.com";

/// Öffnet eine Adresse im Browser.
pub trait Browser: Send + Sync {
    /// Öffnet `url` im Standardbrowser des Systems.
    fn oeffnen(&self, url: &str) -> anyhow::Result<()>;
}

/// Der Standardbrowser des Systems.
#[derive(Debug, Clone, Copy, Default)]
pub struct StandardBrowser;

impl StandardBrowser {
    /// Ein neuer Standardbrowser.
    pub fn neu() -> Self {
        Self
    }
}

impl Browser for StandardBrowser {
    fn oeffnen(&self, url: &str) -> anyhow::Result<()> {
        // `open` startet den Browser über die Plattform-API; keine Shell, keine
        // Argumente aus dem Aufruf (die Adresse ist fest).
        open::that(url)?;
        Ok(())
    }
}

/// Ein Browser, der nichts öffnet (Tests und Betrieb ohne Fenster).
#[derive(Debug, Clone, Copy, Default)]
pub struct KeinBrowser;

impl KeinBrowser {
    /// Ein Browser, der nichts tut.
    pub fn neu() -> Self {
        Self
    }
}

impl Browser for KeinBrowser {
    fn oeffnen(&self, _url: &str) -> anyhow::Result<()> {
        anyhow::bail!("In diesem Betrieb öffnet das Werkzeug keinen Browser.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn der_stille_browser_oeffnet_nichts() {
        let fehler = KeinBrowser.oeffnen(ADRESSE).unwrap_err();
        assert!(fehler.to_string().contains("keinen Browser"), "{fehler}");
    }

    #[test]
    fn die_adresse_ist_fest_und_ohne_anhang() {
        assert_eq!(ADRESSE, "https://excalidraw.com");
        assert!(!ADRESSE.contains('#'), "kein Fragment, das hochlädt");
    }
}
