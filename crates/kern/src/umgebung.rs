// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der Zugang eines Werkzeugs aus der Umgebung (SPEC §2).
//!
//! quiver legt die Werte der Felder aus dem Bibliothekseintrag als Umgebung ab:
//! `<NAME>_<FELD>` in Großbuchstaben, z. B. `OUTLINE_URL` und `OUTLINE_API_KEY`.
//! Der Kern liest sie, prüft die Pflichtfelder und bildet daraus das eine Profil
//! `standard`. Ein leerer Wert zählt als nicht gesetzt.

use std::collections::BTreeMap;

use crate::profil::{Profil, ProfilFehler};
use crate::{Feld, FeldArt, Werkzeug};

/// Name des einen Zugangs (SPEC §2).
pub const PROFIL: &str = "standard";

/// Der Name der Umgebungsvariablen eines Feldes: `outline` + `url` → `OUTLINE_URL`.
pub fn schluessel(werkzeug: &str, feld: &str) -> String {
    format!("{}_{}", werkzeug.to_uppercase(), feld.to_uppercase())
}

/// Der Zugang eines Werkzeugs, gelesen aus seiner Umgebung.
#[derive(Clone)]
pub struct Zugang {
    werkzeug: &'static str,
    titel: &'static str,
    felder: Vec<Feld>,
    /// Was die Umgebung gesetzt hat (Feldname → Wert); leere Werte fehlen hier.
    werte: BTreeMap<String, String>,
}

impl std::fmt::Debug for Zugang {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Die Werte können Geheimnisse enthalten; nur die Namen sind harmlos.
        f.debug_struct("Zugang")
            .field("werkzeug", &self.werkzeug)
            .field("titel", &self.titel)
            .field("gesetzt", &self.werte.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl Zugang {
    /// Der Zugang aus der echten Umgebung des Prozesses.
    pub fn neu<W: Werkzeug>(werkzeug: &W) -> Self {
        Self::mit_umgebung(werkzeug, |name| std::env::var(name).ok())
    }

    /// Der Zugang aus einer beliebigen Quelle (Tests, Sonderfälle).
    ///
    /// `lesen` bekommt den Namen der Umgebungsvariablen (`OUTLINE_URL`) und
    /// liefert ihren Wert, falls gesetzt.
    pub fn mit_umgebung<W, F>(werkzeug: &W, lesen: F) -> Self
    where
        W: Werkzeug,
        F: Fn(&str) -> Option<String>,
    {
        let felder = werkzeug.felder();
        let mut werte = BTreeMap::new();
        for feld in &felder {
            let name = schluessel(werkzeug.name(), feld.name);
            if let Some(wert) = lesen(&name) {
                if !wert.trim().is_empty() {
                    werte.insert(feld.name.to_string(), wert);
                }
            }
        }
        Zugang {
            werkzeug: werkzeug.name(),
            titel: werkzeug.titel(),
            felder,
            werte,
        }
    }

    /// Kurzname des Werkzeugs, z. B. `outline`.
    pub fn werkzeug(&self) -> &'static str {
        self.werkzeug
    }

    /// Anzeigename, z. B. „Outline-Wiki“.
    pub fn titel(&self) -> &'static str {
        self.titel
    }

    /// Die Felder des Zugangs.
    pub fn felder(&self) -> &[Feld] {
        &self.felder
    }

    /// Pflichtfelder ohne Wert – in der Reihenfolge der Felder.
    pub fn fehlende(&self) -> Vec<&Feld> {
        self.felder
            .iter()
            .filter(|feld| feld.pflicht && !self.werte.contains_key(feld.name))
            .collect()
    }

    /// Das eine Profil `standard` mit Geheimnissen, oder [`ProfilFehler::Keins`].
    pub fn profil(&self) -> Result<Profil, ProfilFehler> {
        if self
            .felder
            .iter()
            .any(|feld| feld.pflicht && !self.werte.contains_key(feld.name))
        {
            return Err(ProfilFehler::Keins);
        }

        let mut profil = Profil::leer();
        for feld in &self.felder {
            let Some(wert) = self.werte.get(feld.name) else {
                continue;
            };
            let ziel = if feld.art == FeldArt::Passwort {
                &mut profil.geheim
            } else {
                &mut profil.werte
            };
            ziel.insert(feld.name.to_string(), wert.clone());
        }
        Ok(profil)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Attrappe;

    impl Werkzeug for Attrappe {
        fn name(&self) -> &'static str {
            "attrappe"
        }

        fn titel(&self) -> &'static str {
            "Attrappen-Tool"
        }

        fn beschreibung(&self) -> &'static str {
            "Nur für Tests."
        }

        fn felder(&self) -> Vec<Feld> {
            vec![
                Feld {
                    name: "url",
                    titel: "Adresse",
                    art: FeldArt::Url,
                    pflicht: true,
                    standard: None,
                    hilfe: "Mit https://.",
                    link: None,
                },
                Feld {
                    name: "token",
                    titel: "Token",
                    art: FeldArt::Passwort,
                    pflicht: true,
                    standard: None,
                    hilfe: "Steht in den Einstellungen.",
                    link: None,
                },
                Feld {
                    name: "umgebung",
                    titel: "Umgebung",
                    art: FeldArt::Auswahl(vec![("test".to_string(), "Test".to_string())]),
                    pflicht: false,
                    standard: Some("test"),
                    hilfe: "Nur eine Bezeichnung.",
                    link: None,
                },
            ]
        }

        async fn testen(&self, _profil: &Profil) -> anyhow::Result<String> {
            Ok("erreichbar".to_string())
        }
    }

    /// Eine Umgebung als Paare, ohne den Prozess zu verändern.
    fn zugang(werte: &[(&str, &str)]) -> Zugang {
        Zugang::mit_umgebung(&Attrappe, |name| {
            werte
                .iter()
                .find(|(schluessel, _)| *schluessel == name)
                .map(|(_, wert)| (*wert).to_string())
        })
    }

    #[test]
    fn schluessel_sind_gross_und_mit_unterstrich() {
        assert_eq!(schluessel("outline", "url"), "OUTLINE_URL");
        assert_eq!(schluessel("outline", "api_key"), "OUTLINE_API_KEY");
    }

    #[test]
    fn alle_pflichtfelder_gesetzt_ergeben_genau_ein_profil() {
        let zugang = zugang(&[
            ("ATTRAPPE_URL", "https://example.test"),
            ("ATTRAPPE_TOKEN", "geheim-4711"),
            ("ATTRAPPE_UMGEBUNG", "test"),
        ]);
        let profil = zugang.profil().unwrap();
        assert_eq!(profil.name, PROFIL);
        assert_eq!(
            profil.werte.get("url").map(String::as_str),
            Some("https://example.test")
        );
        assert_eq!(
            profil.werte.get("umgebung").map(String::as_str),
            Some("test")
        );
        assert_eq!(
            profil.geheim.get("token").map(String::as_str),
            Some("geheim-4711")
        );
        // Das Geheimnis steht nicht in den offenen Werten.
        assert!(!profil.werte.contains_key("token"));
        // Und `Debug` nennt es nirgends.
        assert!(!format!("{zugang:?}").contains("geheim-4711"));
        assert!(!format!("{profil:?}").contains("geheim-4711"));
    }

    #[test]
    fn ein_leerer_wert_gilt_als_fehlend() {
        let zugang = zugang(&[("ATTRAPPE_URL", "   "), ("ATTRAPPE_TOKEN", "x")]);
        assert_eq!(zugang.profil(), Err(ProfilFehler::Keins));
        let fehlende: Vec<&str> = zugang.fehlende().iter().map(|feld| feld.name).collect();
        assert_eq!(fehlende, vec!["url"]);
    }

    #[test]
    fn ohne_umgebung_fehlt_der_zugang() {
        let zugang = zugang(&[]);
        assert_eq!(zugang.profil(), Err(ProfilFehler::Keins));
        assert_eq!(zugang.fehlende().len(), 2);
        assert_eq!(zugang.titel(), "Attrappen-Tool");
        assert_eq!(zugang.werkzeug(), "attrappe");
        assert_eq!(zugang.felder().len(), 3);
    }
}
