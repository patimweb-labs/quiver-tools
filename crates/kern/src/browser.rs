// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Wie die Einrichtungsseite den Browser öffnet (SPEC §4).
//!
//! Ein Tool öffnet die Seite einmal; danach arbeitet der User im Browser. Im
//! Test darf nichts aufgehen, deshalb ist das Öffnen hinter einem Trait.

/// Öffnet eine Adresse im Browser des Users.
pub trait Browser: Send + Sync {
    fn oeffnen(&self, adresse: &str) -> anyhow::Result<()>;
}

/// Der echte Browser (`open`).
pub struct EchterBrowser;

impl Browser for EchterBrowser {
    fn oeffnen(&self, adresse: &str) -> anyhow::Result<()> {
        open::that_detached(adresse)?;
        Ok(())
    }
}

/// Attrappe für Tests: öffnet nichts und merkt sich die Adresse.
///
/// Die Tests warten auf [`Merker::adresse`], damit sie nicht raten müssen,
/// unter welcher Adresse die Seite läuft.
pub struct Merker {
    adresse: tokio::sync::watch::Sender<String>,
}

impl Default for Merker {
    fn default() -> Self {
        Self::neu()
    }
}

impl Merker {
    pub fn neu() -> Self {
        let (adresse, _) = tokio::sync::watch::channel(String::new());
        Self { adresse }
    }

    /// Wartet, bis die Seite geöffnet wurde, und gibt die Adresse zurück.
    pub async fn adresse(&self) -> String {
        let mut empfang = self.adresse.subscribe();
        let gelesen = empfang.wait_for(|wert| !wert.is_empty()).await;
        match gelesen {
            Ok(wert) => wert.clone(),
            // Der Sender lebt in `self`, dieser Zweig tritt nicht ein.
            Err(_) => String::new(),
        }
    }

    /// Die zuletzt geöffnete Adresse, falls schon eine geöffnet wurde.
    pub fn zuletzt(&self) -> Option<String> {
        let wert = self.adresse.borrow().clone();
        (!wert.is_empty()).then_some(wert)
    }
}

impl Browser for Merker {
    fn oeffnen(&self, adresse: &str) -> anyhow::Result<()> {
        self.adresse.send_replace(adresse.to_string());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn merker_gibt_die_adresse_zurueck() {
        let merker = Merker::neu();
        assert_eq!(merker.zuletzt(), None);

        merker.oeffnen("http://127.0.0.1:1234/e/abc/").unwrap();
        assert_eq!(merker.adresse().await, "http://127.0.0.1:1234/e/abc/");
        assert_eq!(
            merker.zuletzt().as_deref(),
            Some("http://127.0.0.1:1234/e/abc/")
        );
    }
}
