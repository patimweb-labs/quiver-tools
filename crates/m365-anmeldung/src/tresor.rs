// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Wo das Refresh-Token liegt (SPEC.md §2): eigene verschlüsselte Datei
//! ([`DateiTresor`]), sonst nur im Speicher.

use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

use crate::Tresor;

pub use crate::dateitresor::DateiTresor;

impl DateiTresor {
    /// Der Tresor in `<Datenordner>/quiver-m365/tresor` (macOS: `~/Library/Application Support`, Linux: `~/.local/share`, Windows: `%APPDATA%`).
    pub fn standard() -> Self {
        DateiTresor::im_ordner(
            dirs::data_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join("quiver-m365")
                .join("tresor"),
        )
    }
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
