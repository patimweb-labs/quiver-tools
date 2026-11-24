// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der Sperrzustand der Routine (Aufgabe 06, „Sperrschutz").
//!
//! ZEP sperrt ein Konto nach fünf Fehlversuchen. Darum: je Fehlschlag der
//! Routine **ein** Versuch, danach ist die Routine gesperrt – auch über einen
//! Neustart hinweg, weil der Zustand in einer Datei liegt. Frei wird sie erst
//! wieder durch eine erfolgreiche Anmeldung über den Knopf (den Befehl
//! `quiver-zep anmelden`).
//!
//! In der Datei steht nur ein Grund und ein Zeitstempel, **nie** ein Wert aus
//! dem Zugang.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Der Sperrzustand als kleine Datei.
#[derive(Debug, Clone)]
pub struct Sperre {
    pfad: PathBuf,
}

/// Was in der Datei steht.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct Zustand {
    /// Warum gesperrt wurde – ein Satz, der dem User weiterhilft.
    grund: String,
    /// Sekunden seit dem 1.1.1970 (nur zur Information).
    zeit: u64,
}

impl Sperre {
    /// Die Sperre im Zustandsordner des Werkzeugs.
    pub fn im_ordner(ordner: impl AsRef<Path>) -> Sperre {
        Sperre {
            pfad: ordner.as_ref().join("routine-gesperrt.json"),
        }
    }

    /// Die Sperre in genau dieser Datei (Tests, Sonderfälle).
    pub fn aus_datei(pfad: impl Into<PathBuf>) -> Sperre {
        Sperre { pfad: pfad.into() }
    }

    /// Der Pfad der Datei.
    pub fn pfad(&self) -> &Path {
        &self.pfad
    }

    /// Der Grund, falls die Routine gesperrt ist.
    ///
    /// Eine unlesbare Datei gilt als gesperrt: im Zweifel keinen weiteren
    /// Anmeldeversuch wagen.
    pub fn grund(&self) -> Option<String> {
        let roh = std::fs::read_to_string(&self.pfad).ok()?;
        if roh.trim().is_empty() {
            return None;
        }
        match serde_json::from_str::<Zustand>(&roh) {
            Ok(zustand) => Some(zustand.grund),
            Err(_) => Some("Die Routine ist gesperrt (Zustand unlesbar).".to_string()),
        }
    }

    /// Ist die Routine gesperrt?
    pub fn ist_gesperrt(&self) -> bool {
        self.grund().is_some()
    }

    /// Sperren – nach einem fehlgeschlagenen Versuch. Gibt den Pfad zurück,
    /// damit eine Meldung sagen kann, wo der Zustand liegt.
    pub fn sperren(&self, grund: &str) -> anyhow::Result<()> {
        if let Some(ordner) = self.pfad.parent() {
            std::fs::create_dir_all(ordner)?;
        }
        let zustand = Zustand {
            grund: grund.to_string(),
            zeit: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|dauer| dauer.as_secs())
                .unwrap_or_default(),
        };
        std::fs::write(&self.pfad, serde_json::to_vec_pretty(&zustand)?)?;
        Ok(())
    }

    /// Die Sperre aufheben (nach einer erfolgreichen Anmeldung).
    pub fn loesen(&self) -> anyhow::Result<()> {
        match std::fs::remove_file(&self.pfad) {
            Ok(()) => Ok(()),
            Err(fehler) if fehler.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(fehler) => Err(fehler.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sperren_und_loesen_ueber_die_datei() {
        let ordner = tempfile::tempdir().unwrap();
        let sperre = Sperre::im_ordner(ordner.path());
        assert!(!sperre.ist_gesperrt());
        assert_eq!(sperre.grund(), None);

        sperre
            .sperren("Das Passwort wurde abgelehnt.")
            .expect("sperren schlägt fehl");
        assert!(sperre.ist_gesperrt());
        assert_eq!(
            sperre.grund().as_deref(),
            Some("Das Passwort wurde abgelehnt.")
        );
        // Eine zweite Sperre liest denselben Zustand (wie nach einem Neustart).
        let nochmal = Sperre::aus_datei(sperre.pfad().to_path_buf());
        assert!(nochmal.ist_gesperrt());

        sperre.loesen().expect("loesen schlägt fehl");
        assert!(!sperre.ist_gesperrt());
        // Zweimal lösen ist kein Fehler.
        sperre.loesen().expect("loesen schlägt fehl");
    }

    #[test]
    fn eine_kaputte_datei_gilt_als_gesperrt() {
        let ordner = tempfile::tempdir().unwrap();
        let sperre = Sperre::im_ordner(ordner.path());
        std::fs::write(sperre.pfad(), "{kein json").unwrap();
        assert!(sperre.ist_gesperrt());
        assert!(sperre.grund().unwrap().contains("unlesbar"));

        std::fs::write(sperre.pfad(), "   ").unwrap();
        assert!(!sperre.ist_gesperrt(), "leere Datei ist keine Sperre");
    }

    #[test]
    fn die_datei_enthaelt_keinen_zugang() {
        let ordner = tempfile::tempdir().unwrap();
        let sperre = Sperre::im_ordner(ordner.path());
        sperre.sperren("Ein Versuch, dann Pause.").unwrap();
        let inhalt = std::fs::read_to_string(sperre.pfad()).unwrap();
        assert!(inhalt.contains("Ein Versuch, dann Pause."));
        assert!(!inhalt.contains("passwort"));
        assert_eq!(
            sperre.pfad().file_name().unwrap().to_str().unwrap(),
            "routine-gesperrt.json"
        );
    }
}
