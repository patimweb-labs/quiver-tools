// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Wo das Refresh-Token liegt (SPEC.md §2): Schlüsselbund, sonst nur im Speicher.

use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

use crate::Tresor;

/// Name des Dienstes im Schlüsselbund. Schlüssel ist `<tenant>/<client_id>`.
pub const DIENST: &str = "quiver-m365";

/// Refresh-Token im Schlüsselbund des Betriebssystems.
pub struct Schluesselbund {
    dienst: String,
}

impl Schluesselbund {
    pub fn neu() -> Self {
        Self {
            dienst: DIENST.to_string(),
        }
    }
}

impl Default for Schluesselbund {
    fn default() -> Self {
        Self::neu()
    }
}

impl fmt::Debug for Schluesselbund {
    /// Nie Werte, nur der Dienst.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Schluesselbund")
            .field("dienst", &self.dienst)
            .finish()
    }
}

impl Tresor for Schluesselbund {
    fn lesen(&self, schluessel: &str) -> anyhow::Result<Option<String>> {
        let eintrag = keyring::Entry::new(&self.dienst, schluessel)?;
        match eintrag.get_password() {
            Ok(wert) => Ok(Some(wert)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(fehler) => Err(fehler.into()),
        }
    }

    fn schreiben(&self, schluessel: &str, wert: &str) -> anyhow::Result<()> {
        keyring::Entry::new(&self.dienst, schluessel)?.set_password(wert)?;
        Ok(())
    }

    fn loeschen(&self, schluessel: &str) -> anyhow::Result<()> {
        let eintrag = keyring::Entry::new(&self.dienst, schluessel)?;
        match eintrag.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(fehler) => Err(fehler.into()),
        }
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
