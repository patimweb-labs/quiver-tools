// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Wo die Geheimnisse liegen (SPEC §3): Schlüsselbund im Betrieb, Speicher im Test.
//!
//! Ein Geheimnis ist immer (Dienst, Schlüssel) → Wert. Der Dienst ist
//! `quiver-<werkzeug>`, der Schlüssel `<profil>/<feld>`. Kein Fehler dieses
//! Moduls enthält je einen Wert.

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

/// Was beim Tresor schiefgehen kann. Enthält nie einen gespeicherten Wert.
#[derive(Debug, thiserror::Error)]
pub enum TresorFehler {
    #[error("Der Schlüsselbund ist nicht verfügbar: {0}")]
    NichtVerfuegbar(String),
    #[error("Der Schlüsselbund hat den Zugriff verweigert.")]
    Verweigert,
    #[error("Der Schlüsselbund meldet einen Fehler: {0}")]
    Sonstig(String),
}

impl From<keyring::Error> for TresorFehler {
    fn from(fehler: keyring::Error) -> Self {
        match fehler {
            // Die Meldungen von `keyring` nennen nur Dienst und Schlüssel, nie den Wert.
            keyring::Error::PlatformFailure(ursache) => {
                TresorFehler::NichtVerfuegbar(ursache.to_string())
            }
            keyring::Error::NoStorageAccess(_) | keyring::Error::NoEntry => {
                TresorFehler::Verweigert
            }
            sonstig => TresorFehler::Sonstig(sonstig.to_string()),
        }
    }
}

/// Ablage für Geheimnisse.
pub trait Tresor: Send + Sync {
    /// Wert lesen; `Ok(None)`, wenn nichts hinterlegt ist.
    fn lesen(&self, dienst: &str, schluessel: &str) -> Result<Option<String>, TresorFehler>;
    /// Wert hinterlegen (überschreibt).
    fn schreiben(&self, dienst: &str, schluessel: &str, wert: &str) -> Result<(), TresorFehler>;
    /// Wert entfernen; kein Eintrag ist kein Fehler.
    fn loeschen(&self, dienst: &str, schluessel: &str) -> Result<(), TresorFehler>;
}

/// Der Schlüsselbund des Systems (macOS Schlüsselbund, Windows Anmeldeinformationen,
/// Linux Secret Service).
pub struct Schluesselbund;

impl Tresor for Schluesselbund {
    fn lesen(&self, dienst: &str, schluessel: &str) -> Result<Option<String>, TresorFehler> {
        match eintrag(dienst, schluessel)?.get_password() {
            Ok(wert) => Ok(Some(wert)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(fehler) => Err(fehler.into()),
        }
    }

    fn schreiben(&self, dienst: &str, schluessel: &str, wert: &str) -> Result<(), TresorFehler> {
        eintrag(dienst, schluessel)?.set_password(wert)?;
        Ok(())
    }

    fn loeschen(&self, dienst: &str, schluessel: &str) -> Result<(), TresorFehler> {
        match eintrag(dienst, schluessel)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(fehler) => Err(fehler.into()),
        }
    }
}

fn eintrag(dienst: &str, schluessel: &str) -> Result<keyring::Entry, TresorFehler> {
    keyring::Entry::new(dienst, schluessel).map_err(TresorFehler::from)
}

/// Ablage im Arbeitsspeicher – für Tests und für `QUIVER_TOOLS_TRESOR=speicher`.
///
/// Der Speicher ist prozessweit: zwei Griffe sehen dieselben Werte. Nur so
/// kann ein Test ein Profil speichern und über einen zweiten Griff lesen.
#[derive(Debug, Default, Clone, Copy)]
pub struct SpeicherTresor;

fn speicher() -> &'static Mutex<BTreeMap<(String, String), String>> {
    static SPEICHER: OnceLock<Mutex<BTreeMap<(String, String), String>>> = OnceLock::new();
    SPEICHER.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn gesperrt() -> std::sync::MutexGuard<'static, BTreeMap<(String, String), String>> {
    // Ein vergifteter Speicher ist ein Programmierfehler, kein Datenverlust:
    // die Karte bleibt benutzbar.
    speicher().lock().unwrap_or_else(|gift| gift.into_inner())
}

impl SpeicherTresor {
    /// Alles vergessen (nur für Tests).
    pub fn leeren() {
        gesperrt().clear();
    }

    /// Wie viele Geheimnisse gerade liegen (nur für Tests).
    pub fn anzahl() -> usize {
        gesperrt().len()
    }
}

impl Tresor for SpeicherTresor {
    fn lesen(&self, dienst: &str, schluessel: &str) -> Result<Option<String>, TresorFehler> {
        Ok(gesperrt()
            .get(&(dienst.to_string(), schluessel.to_string()))
            .cloned())
    }

    fn schreiben(&self, dienst: &str, schluessel: &str, wert: &str) -> Result<(), TresorFehler> {
        gesperrt().insert(
            (dienst.to_string(), schluessel.to_string()),
            wert.to_string(),
        );
        Ok(())
    }

    fn loeschen(&self, dienst: &str, schluessel: &str) -> Result<(), TresorFehler> {
        gesperrt().remove(&(dienst.to_string(), schluessel.to_string()));
        Ok(())
    }
}

/// Name der Umgebungsvariablen, die den Tresor wählt (SPEC §3).
pub const TRESOR_UMGEBUNG: &str = "QUIVER_TOOLS_TRESOR";

/// Der Tresor für diesen Lauf: `QUIVER_TOOLS_TRESOR=speicher` → Speicher,
/// sonst der Schlüsselbund des Systems.
pub fn aus_umgebung() -> std::sync::Arc<dyn Tresor> {
    match std::env::var(TRESOR_UMGEBUNG) {
        Ok(wert) if wert.eq_ignore_ascii_case("speicher") => {
            tracing::warn!(
                "Tresor im Arbeitsspeicher ({}={}): Geheimnisse überleben den Lauf nicht.",
                TRESOR_UMGEBUNG,
                wert
            );
            std::sync::Arc::new(SpeicherTresor)
        }
        _ => std::sync::Arc::new(Schluesselbund),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIENST: &str = "quiver-test-tresor";

    #[test]
    fn speicher_merkt_schreibt_und_loescht() {
        let tresor = SpeicherTresor;
        assert_eq!(tresor.lesen(DIENST, "a/b").unwrap(), None);

        tresor.schreiben(DIENST, "a/b", "geheim").unwrap();
        assert_eq!(
            tresor.lesen(DIENST, "a/b").unwrap().as_deref(),
            Some("geheim")
        );

        // Ein zweiter Griff sieht denselben Wert.
        assert_eq!(
            SpeicherTresor.lesen(DIENST, "a/b").unwrap().as_deref(),
            Some("geheim")
        );

        tresor.loeschen(DIENST, "a/b").unwrap();
        assert_eq!(tresor.lesen(DIENST, "a/b").unwrap(), None);

        // Löschen ohne Eintrag ist kein Fehler.
        tresor.loeschen(DIENST, "a/b").unwrap();
    }

    #[test]
    fn fehlertext_nennt_keinen_wert() {
        let fehler = TresorFehler::from(keyring::Error::NoEntry);
        let text = fehler.to_string();
        assert!(!text.contains("geheim"));
        assert_eq!(text, "Der Schlüsselbund hat den Zugriff verweigert.");
    }
}
