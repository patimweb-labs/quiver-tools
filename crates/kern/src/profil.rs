// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der eine Zugang eines Werkzeugs (SPEC §2).
//!
//! Es gibt kein Profil in Datei oder Tresor mehr: quiver legt die Werte aus dem
//! Bibliothekseintrag als Umgebung ab, [`crate::Zugang`] liest sie und bildet
//! daraus dieses [`Profil`]. Geheime Werte stehen darum nur im Arbeitsspeicher.

use std::collections::BTreeMap;

/// Was fehlt, um einen Zugang zu bilden. Enthält nie einen Wert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ProfilFehler {
    /// Mindestens ein Pflichtfeld ist nicht gesetzt (leere Werte zählen als fehlend).
    #[error("Zugang fehlt.")]
    Keins,
}

/// Der Zugang eines Werkzeugs. `Debug` zeigt Geheimnisse nie.
#[derive(Clone, PartialEq, Eq)]
pub struct Profil {
    /// Name des Zugangs; aus der Umgebung ist das immer `standard`.
    pub name: String,
    /// Nicht-geheime Werte (Adresse, Host, Auswahl …).
    pub werte: BTreeMap<String, String>,
    /// Geheime Werte (Feldart `Passwort`); sie verlassen dieses Modul nicht.
    pub geheim: BTreeMap<String, String>,
}

impl std::fmt::Debug for Profil {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Profil")
            .field("name", &self.name)
            .field("werte", &self.werte)
            .field("geheim", &self.geheim.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl Profil {
    /// Ein leerer Zugang mit dem Namen `standard`.
    pub fn leer() -> Self {
        Profil {
            name: crate::umgebung::PROFIL.to_string(),
            werte: BTreeMap::new(),
            geheim: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_zeigt_geheimnisse_nur_als_namen() {
        let mut profil = Profil::leer();
        profil
            .geheim
            .insert("api_key".to_string(), "ol_api_nur_im_test_4711".to_string());
        let text = format!("{profil:?}");
        assert!(text.contains("api_key"));
        assert!(!text.contains("ol_api_nur_im_test_4711"));
    }

    #[test]
    fn leer_heisst_standard() {
        let profil = Profil::leer();
        assert_eq!(profil.name, "standard");
        assert!(profil.werte.is_empty());
        assert!(profil.geheim.is_empty());
    }
}
