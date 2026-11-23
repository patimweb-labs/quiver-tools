// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Wo das Refresh-Token liegt (SPEC.md §2): Schlüsselbund oder – mit
//! `M365_TRESOR=speicher` – nur im Arbeitsspeicher.

use m365_anmeldung::{Schluesselbund, SpeicherTresor, Tresor};

/// Die Wahl aus der Umgebung, als ein Typ (die Anmeldung ist generisch).
pub enum TresorWahl {
    /// Schlüsselbund des Betriebssystems (Standard).
    Schluesselbund(Schluesselbund),
    /// Nur im Arbeitsspeicher (Tests, Entwicklung, `M365_TRESOR=speicher`).
    Speicher(SpeicherTresor),
}

impl TresorWahl {
    /// Die reine Regel: `speicher` (Groß-/Kleinschreibung egal) → Speicher,
    /// alles andere → Schlüsselbund.
    pub fn aus_wert(roh: Option<&str>) -> Self {
        match roh.map(|wert| wert.trim().to_ascii_lowercase()) {
            Some(wert) if wert == "speicher" => Self::Speicher(SpeicherTresor::neu()),
            _ => Self::Schluesselbund(Schluesselbund::neu()),
        }
    }

    /// `M365_TRESOR` aus der Umgebung.
    pub fn aus_umgebung() -> Self {
        let roh = crate::umgebung::wert("M365_TRESOR");
        Self::aus_wert(roh.as_deref())
    }

    /// Nur für Tests und Logs: welcher Tresor ist es?
    pub fn name(&self) -> &'static str {
        match self {
            Self::Schluesselbund(_) => "schluesselbund",
            Self::Speicher(_) => "speicher",
        }
    }
}

impl Tresor for TresorWahl {
    fn lesen(&self, schluessel: &str) -> anyhow::Result<Option<String>> {
        match self {
            Self::Schluesselbund(tresor) => tresor.lesen(schluessel),
            Self::Speicher(tresor) => tresor.lesen(schluessel),
        }
    }

    fn schreiben(&self, schluessel: &str, wert: &str) -> anyhow::Result<()> {
        match self {
            Self::Schluesselbund(tresor) => tresor.schreiben(schluessel, wert),
            Self::Speicher(tresor) => tresor.schreiben(schluessel, wert),
        }
    }

    fn loeschen(&self, schluessel: &str) -> anyhow::Result<()> {
        match self {
            Self::Schluesselbund(tresor) => tresor.loeschen(schluessel),
            Self::Speicher(tresor) => tresor.loeschen(schluessel),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speicher_wird_erkannt() {
        assert_eq!(TresorWahl::aus_wert(Some("speicher")).name(), "speicher");
        assert_eq!(TresorWahl::aus_wert(Some(" Speicher ")).name(), "speicher");
    }

    #[test]
    fn sonst_schluesselbund() {
        assert_eq!(TresorWahl::aus_wert(None).name(), "schluesselbund");
        assert_eq!(TresorWahl::aus_wert(Some("")).name(), "schluesselbund");
        assert_eq!(TresorWahl::aus_wert(Some("datei")).name(), "schluesselbund");
    }

    #[test]
    fn speicher_tresor_haelt_werte() {
        let tresor = TresorWahl::aus_wert(Some("speicher"));
        assert!(tresor.lesen("a/b").expect("lesbar").is_none());
        tresor.schreiben("a/b", "geheim").expect("schreibbar");
        assert_eq!(
            tresor.lesen("a/b").expect("lesbar").as_deref(),
            Some("geheim")
        );
        tresor.loeschen("a/b").expect("loeschbar");
        assert!(tresor.lesen("a/b").expect("lesbar").is_none());
    }
}
