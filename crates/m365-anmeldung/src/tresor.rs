// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Wo das Refresh-Token liegt (SPEC.md §2): eigene verschlüsselte Datei
//! ([`DateiTresor`]), sonst nur im Speicher.

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::Tresor;

pub use crate::dateitresor::DateiTresor;

impl DateiTresor {
    /// Der Tresor in `<Datenordner>/quiver-m365/tresor` (macOS: `~/Library/Application Support`, Linux: `~/.local/share`, Windows: `%APPDATA%`).
    ///
    /// Unter `cfg(test)` liegt er in einem temporären Ordner je Prozess: kein
    /// Unit-Test kommt so an die echte Tresor-Datei des Users (Aufgabe 11).
    pub fn standard() -> Self {
        DateiTresor::im_ordner(zustandsordner())
    }
}

/// Der Ordner des Standard-Tresors.
#[cfg(not(test))]
fn zustandsordner() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("quiver-m365")
        .join("tresor")
}

/// Im Test ein temporärer Ordner je Prozess – nie der echte Tresor.
#[cfg(test)]
fn zustandsordner() -> PathBuf {
    std::env::temp_dir().join(format!("quiver-m365-test-{}", std::process::id()))
}

impl Tresor for DateiTresor {
    fn lesen(&self, schluessel: &str) -> anyhow::Result<Option<String>> {
        DateiTresor::lesen(self, schluessel)
    }
    fn schreiben(&self, schluessel: &str, wert: &str) -> anyhow::Result<()> {
        DateiTresor::schreiben(self, schluessel, wert)
    }
    fn loeschen(&self, schluessel: &str) -> anyhow::Result<()> {
        DateiTresor::loeschen(self, schluessel)
    }
}

/// Refresh-Token nur im Arbeitsspeicher (Tests, Entwicklung, `M365_TRESOR=speicher`).
pub struct SpeicherTresor {
    werte: Mutex<HashMap<String, String>>,
}

impl SpeicherTresor {
    pub fn neu() -> Self {
        Self {
            werte: Mutex::new(HashMap::new()),
        }
    }

    fn fach(&self) -> std::sync::MutexGuard<'_, HashMap<String, String>> {
        // Ein vergifteter Sperrhahn enthält nur Klartext-Schlüssel, keine Logik.
        self.werte
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner())
    }
}

impl Default for SpeicherTresor {
    fn default() -> Self {
        Self::neu()
    }
}

impl fmt::Debug for SpeicherTresor {
    /// Nie Werte, nur die Schlüssel.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let schluessel: Vec<String> = {
            let fach = self.fach();
            fach.keys().cloned().collect()
        };
        f.debug_struct("SpeicherTresor")
            .field("schluessel", &schluessel)
            .finish()
    }
}

impl Tresor for SpeicherTresor {
    fn lesen(&self, schluessel: &str) -> anyhow::Result<Option<String>> {
        Ok(self.fach().get(schluessel).cloned())
    }

    fn schreiben(&self, schluessel: &str, wert: &str) -> anyhow::Result<()> {
        self.fach().insert(schluessel.to_string(), wert.to_string());
        Ok(())
    }

    fn loeschen(&self, schluessel: &str) -> anyhow::Result<()> {
        self.fach().remove(schluessel);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Wächter: `DateiTresor::standard()` zeigt im Test nie unter `HOME` – sonst
    /// schriebe ein Unit-Test in den echten Tresor des Users (Aufgabe 11).
    #[test]
    fn der_standard_tresor_liegt_im_test_nicht_unter_home() {
        let ordner = DateiTresor::standard().ordner().to_path_buf();
        for name in ["HOME", "USERPROFILE"] {
            let Some(heim) = std::env::var_os(name).filter(|wert| !wert.is_empty()) else {
                continue;
            };
            let heim = PathBuf::from(heim);
            assert!(
                !ordner.starts_with(&heim),
                "{ordner:?} liegt unter {name}={heim:?}"
            );
        }
        assert!(ordner.starts_with(std::env::temp_dir()), "{ordner:?}");
    }
}
