// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Wo das Refresh-Token liegt (Aufgabe 06): Schlüsselbund des Geräts, sonst nur
//! im Arbeitsspeicher.
//!
//! Der Zugang selbst kommt als Umgebung aus quiver; hier liegt nur, was das
//! Werkzeug selbst behalten muss, damit es angemeldet bleibt: das (rotierende)
//! Refresh-Token und die einmalige Client-Registrierung.
//!
//! Dienst ist `quiver-zep`, Schlüssel ist die MCP-Adresse. Kein Wert dieses
//! Moduls erscheint je in einem Log oder einer Antwort.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Mutex;

/// Name des Dienstes im Schlüsselbund (Aufgabe 06).
pub const DIENST: &str = "quiver-zep";

/// Der Schlüssel des Refresh-Tokens: die MCP-Adresse, wie der User sie einträgt.
pub fn schluessel_refresh(url: &str) -> String {
    url.to_string()
}

/// Der Schlüssel der Client-Registrierung (DCR): kein Geheimnis, aber es gehört
/// zur Installation und nicht in eine Antwort.
pub fn schluessel_client(url: &str) -> String {
    format!("client-id:{url}")
}

/// Der Tresor, den die Umgebung wählt.
///
/// Normalfall ist der Schlüsselbund. `ZEP_TRESOR=speicher` legt die Werte nur in
/// den Arbeitsspeicher – für Testläufe und für einen Lauf ohne Schlüsselbund;
/// damit kommt kein Test je an den echten Schlüsselbund.
pub fn aus_umgebung() -> std::sync::Arc<dyn Tresor> {
    match std::env::var("ZEP_TRESOR").ok().as_deref() {
        Some("speicher") => std::sync::Arc::new(SpeicherTresor::neu()),
        _ => std::sync::Arc::new(Schluesselbund::neu()),
    }
}

/// Wo Werte liegen, die das Werkzeug über einen Neustart hinweg braucht.
pub trait Tresor: Send + Sync {
    fn lesen(&self, schluessel: &str) -> anyhow::Result<Option<String>>;
    fn schreiben(&self, schluessel: &str, wert: &str) -> anyhow::Result<()>;
    fn loeschen(&self, schluessel: &str) -> anyhow::Result<()>;
}

/// Refresh-Token und Client-Id im Schlüsselbund des Betriebssystems.
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

/// Werte nur im Arbeitsspeicher: für Tests und für einen Lauf ohne
/// Schlüsselbund. Nichts davon überlebt den Prozess.
pub struct SpeicherTresor {
    werte: Mutex<BTreeMap<String, String>>,
}

impl SpeicherTresor {
    pub fn neu() -> Self {
        Self {
            werte: Mutex::new(BTreeMap::new()),
        }
    }

    fn fach(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, String>> {
        // Ein vergifteter Sperrhahn enthält nur Text, keine Logik.
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

    const URL: &str = "https://zep.example.test/mandant/next/mcp";

    #[test]
    fn der_speicher_traegt_und_vergisst() {
        let tresor = SpeicherTresor::neu();
        assert_eq!(tresor.lesen(&schluessel_refresh(URL)).unwrap(), None);
        tresor.schreiben(&schluessel_refresh(URL), "rt-1").unwrap();
        assert_eq!(
            tresor.lesen(&schluessel_refresh(URL)).unwrap().as_deref(),
            Some("rt-1")
        );
        tresor.loeschen(&schluessel_refresh(URL)).unwrap();
        assert_eq!(tresor.lesen(&schluessel_refresh(URL)).unwrap(), None);
    }

    #[test]
    fn schluessel_sind_die_adresse_und_eine_eigene_fuer_die_client_id() {
        assert_eq!(schluessel_refresh(URL), URL);
        assert_eq!(schluessel_client(URL), format!("client-id:{URL}"));
        assert_ne!(schluessel_refresh(URL), schluessel_client(URL));
    }

    #[test]
    fn debug_nennt_nur_schluessel_und_nie_werte() {
        let tresor = SpeicherTresor::neu();
        tresor
            .schreiben(&schluessel_refresh(URL), "rt-nur-im-test")
            .unwrap();
        let text = format!("{tresor:?}");
        assert!(text.contains(URL));
        assert!(!text.contains("rt-nur-im-test"));
        assert!(format!("{:?}", SpeicherTresor::neu()).contains("schluessel"));
    }
}
