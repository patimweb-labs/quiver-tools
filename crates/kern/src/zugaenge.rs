// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Mehrere gleichartige Zugänge eines Werkzeugs (Achse `Eintrag.mehrfach`).
//!
//! Bei den meisten Tools gibt es genau einen Zugang (siehe [`crate::Zugang`]).
//! Ein Werkzeug wie `quiver-ssh` pflegt stattdessen eine Liste benannter
//! Verbindungen: der User legt sie im quiver-Fenster an, quiver übergibt sie
//! beim Start **alle** als JSON in [`UMGEBUNG`]:
//!
//! ```text
//! [{"name":"web-01","werte":{"host":"…","port":"22","benutzer":"…"}}]
//! ```
//!
//! Dieses Modul liest diese Liste. Es kennt weder quiver noch den
//! Schlüsselbund; es liest eine Umgebungsvariable, wie es [`crate::Zugang`] für
//! die Felder eines einzelnen Zugangs tut.

use std::collections::BTreeMap;

use serde::Deserialize;

/// Name der Umgebungsvariable, in der quiver alle Zugänge übergibt
/// (`quiver_domain::MEHRFACH_UMGEBUNG`).
pub const UMGEBUNG: &str = "QUIVER_ZUGAENGE";

/// Ein Zugang aus der Liste, so wie quiver ihn übergibt.
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct Zugang {
    /// Name, den der User vergeben hat, z. B. `web-01`.
    pub name: String,
    /// Die Werte der Felder aus `Werkzeug::mehrfach()`; fehlende Felder fehlen.
    #[serde(default)]
    pub werte: BTreeMap<String, String>,
}

impl std::fmt::Debug for Zugang {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Die Werte können Geheimnisse enthalten; nur die Namen sind harmlos.
        f.debug_struct("Zugang")
            .field("name", &self.name)
            .field("gesetzt", &self.werte.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl Zugang {
    /// Der Wert eines Feldes; leer oder nur Leerraum zählt als fehlend.
    pub fn wert(&self, feld: &str) -> Option<&str> {
        self.werte
            .get(feld)
            .map(|wert| wert.trim())
            .filter(|wert| !wert.is_empty())
    }
}

/// Was beim Lesen der Liste schiefgehen kann. Enthält bewusst keinen Inhalt:
/// die Umgebungsvariable kann Geheimnisse enthalten.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ZugaengeFehler {
    /// `QUIVER_ZUGAENGE` ist gesetzt, aber kein gültiges JSON.
    #[error("Die Zugänge in QUIVER_ZUGAENGE sind kein gültiges JSON.")]
    Kaputt,
}

/// Alle Zugänge aus der echten Umgebung des Prozesses.
///
/// Fehlt die Variable (oder ist sie leer), gibt es keine Zugänge. Kaputtes
/// JSON ist ein Fehler ohne Inhalt.
pub fn zugaenge() -> Result<Vec<Zugang>, ZugaengeFehler> {
    mit_umgebung(|name| std::env::var(name).ok())
}

/// Alle Zugänge aus einer beliebigen Quelle (Tests, Sonderfälle).
///
/// `lesen` bekommt den Namen der Umgebungsvariablen ([`UMGEBUNG`]) und liefert
/// ihren Wert, falls gesetzt.
pub fn mit_umgebung<F>(lesen: F) -> Result<Vec<Zugang>, ZugaengeFehler>
where
    F: Fn(&str) -> Option<String>,
{
    let Some(roh) = lesen(UMGEBUNG) else {
        return Ok(Vec::new());
    };
    if roh.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(&roh).map_err(|_| ZugaengeFehler::Kaputt)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aus(roh: Option<&str>) -> Result<Vec<Zugang>, ZugaengeFehler> {
        mit_umgebung(|name| {
            assert_eq!(name, UMGEBUNG);
            roh.map(str::to_string)
        })
    }

    #[test]
    fn ohne_umgebung_gibt_es_keine_zugaenge() {
        assert_eq!(aus(None).unwrap(), Vec::new());
        assert_eq!(aus(Some("   ")).unwrap(), Vec::new());
    }

    #[test]
    fn die_liste_wird_gelesen() {
        let liste = aus(Some(
            r#"[{"name":"web-01","werte":{"host":"web.example.test","port":"22","anmelde_passwort":"geheim-4711"}},
                {"name":"db-02","werte":{"host":"db.example.test"}}]"#,
        ))
        .unwrap();

        assert_eq!(liste.len(), 2);
        assert_eq!(liste[0].name, "web-01");
        assert_eq!(liste[0].wert("host"), Some("web.example.test"));
        assert_eq!(liste[1].name, "db-02");
        assert_eq!(liste[1].wert("port"), None);
        // Fehlende Werte sind kein Fehler.
        assert_eq!(liste[1].werte.len(), 1);
    }

    #[test]
    fn leerraum_gilt_als_fehlend() {
        let liste = aus(Some(r#"[{"name":"a","werte":{"host":"  "}}]"#)).unwrap();
        assert_eq!(liste[0].wert("host"), None);
    }

    #[test]
    fn kaputtes_json_ist_ein_fehler_ohne_inhalt() {
        let fehler = aus(Some(r#"[{"name":"a","werte":{"passwort":"geheim-4711"}"#)).unwrap_err();
        assert_eq!(fehler, ZugaengeFehler::Kaputt);
        let text = fehler.to_string();
        assert!(!text.contains("geheim-4711"), "{text}");
        assert!(!text.contains("passwort"), "{text}");
    }

    #[test]
    fn debug_zeigt_werte_nur_als_namen() {
        let liste = aus(Some(r#"[{"name":"a","werte":{"passwort":"geheim-4711"}}]"#)).unwrap();
        let text = format!("{:?}", liste[0]);
        assert!(text.contains("passwort"), "{text}");
        assert!(!text.contains("geheim-4711"), "{text}");
    }
}
