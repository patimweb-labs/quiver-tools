// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Das Werkzeug „draw.io“ (Aufgabe 18).
//!
//! Zwei Felder, kein Geheimnis, keine Anmeldung: ein Arbeitsordner und ein
//! Schalter. Der Arbeitsordner ist Pflicht und absolut (die [`Ablage`] legt ihn
//! an und löst `~` auf); „schreiben“ ist **standardmäßig an**, und auch dann
//! wird nur im Arbeitsordner geschrieben, nie über Vorhandenes und nur mit dem
//! gelesenen `stand` (Aufgabe 16).
//!
//! [`Ablage`]: quiver_diagramm::ablage::Ablage

use quiver_diagramm::ablage::Ablage;
use quiver_tools_kern::{Feld, FeldArt, Profil, Werkzeug};

/// Kurzname: Teil der Werkzeugnamen (`drawio_status`) und des Programms.
pub const NAME: &str = "drawio";
/// Anzeigename im quiver-Fenster und in MCP.
pub const TITEL: &str = "draw.io";
/// Feld des Arbeitsordners.
pub const ORDNER: &str = "ordner";
/// Feld des Schalters „schreiben“.
pub const SCHREIBEN: &str = "schreiben";
/// Die eine erlaubte Dateiendung dieser Ablage.
pub const ENDUNG: &str = "drawio";

/// Der Arbeitsordner aus einem Profil (leer, wenn nicht gesetzt).
pub fn ordner(profil: &Profil) -> &str {
    profil
        .werte
        .get(ORDNER)
        .map(String::as_str)
        .unwrap_or_default()
}

/// Ist „schreiben“ an? Ohne Wert gilt der Standard: an.
pub fn schreiben_erlaubt(profil: &Profil) -> bool {
    !matches!(
        profil
            .werte
            .get(SCHREIBEN)
            .map(|wert| wert.trim().to_ascii_lowercase())
            .as_deref(),
        Some("aus" | "nein" | "false" | "0")
    )
}

/// Das draw.io-Werkzeug.
#[derive(Debug, Clone, Copy, Default)]
pub struct Drawio;

impl Werkzeug for Drawio {
    fn name(&self) -> &'static str {
        NAME
    }

    fn titel(&self) -> &'static str {
        TITEL
    }

    fn beschreibung(&self) -> &'static str {
        "draw.io-Dateien (.drawio) lokal lesen, anlegen und ändern, als Bild zeichnen und im \
         Browser öffnen. Seiten, Zellen, Formen, Farben, Verbindungen und Stile; fremde Formen \
         und unbekannte Felder bleiben erhalten. Keine Anmeldung, kein Netz: alles geschieht in \
         einem Arbeitsordner."
    }

    fn felder(&self) -> Vec<Feld> {
        vec![
            Feld {
                name: ORDNER,
                titel: "Arbeitsordner",
                art: FeldArt::Ordner,
                pflicht: true,
                standard: None,
                hilfe: "In diesem Ordner liegen die Diagramme; Pfade nach außen sind gesperrt.",
                link: None,
            },
            Feld {
                name: SCHREIBEN,
                titel: "Schreiben erlauben",
                art: FeldArt::Schalter,
                pflicht: false,
                standard: Some("an"),
                hilfe: "Erlaubt Anlegen und Ändern im Arbeitsordner – nie über Vorhandenes und \
                        nur mit dem gelesenen `stand`. Lesen, Zeichnen und Öffnen geht immer.",
                link: None,
            },
        ]
    }

    /// `quiver-drawio testen`: Arbeitsordner öffnen, Dateien zählen.
    ///
    /// Es gibt keine Verbindung zu prüfen – draw.io läuft lokal, ohne Konto.
    async fn testen(&self, profil: &Profil) -> anyhow::Result<String> {
        let ablage = Ablage::neu(ordner(profil), ENDUNG)?;
        let dateien = ablage.liste()?;
        Ok(format!(
            "Arbeitsordner {} ist bereit ({} .drawio-Datei(en)), Schreiben ist {}. Keine \
             Anmeldung nötig: draw.io-Dateien liegen lokal.",
            ablage.wurzel().display(),
            dateien.len(),
            if schreiben_erlaubt(profil) {
                "an"
            } else {
                "aus"
            }
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profil(werte: &[(&str, &str)]) -> Profil {
        let mut profil = Profil::leer();
        for (name, wert) in werte {
            profil
                .werte
                .insert((*name).to_string(), (*wert).to_string());
        }
        profil
    }

    #[test]
    fn das_werkzeug_hat_genau_zwei_felder_und_kein_geheimnis() {
        let felder = Drawio.felder();
        assert_eq!(Drawio.name(), "drawio");
        assert_eq!(Drawio.titel(), "draw.io");
        assert_eq!(
            felder.iter().map(|feld| feld.name).collect::<Vec<_>>(),
            vec!["ordner", "schreiben"]
        );
        assert_eq!(felder[0].art, FeldArt::Ordner);
        assert!(felder[0].pflicht);
        assert_eq!(felder[1].art, FeldArt::Schalter);
        assert!(!felder[1].pflicht);
        // Standard an (Aufgabe 16).
        assert_eq!(felder[1].standard, Some("an"));
        // Kein Passwortfeld, also kein Geheimnis in diesem Werkzeug.
        assert!(felder
            .iter()
            .all(|feld| !matches!(feld.art, FeldArt::Passwort)));
        assert!(felder.iter().all(|feld| !feld.hilfe.is_empty()));
    }

    #[test]
    fn schreiben_ist_ohne_wert_an() {
        assert!(schreiben_erlaubt(&profil(&[])));
        assert!(schreiben_erlaubt(&profil(&[("schreiben", "an")])));
        assert!(!schreiben_erlaubt(&profil(&[("schreiben", "aus")])));
        assert!(!schreiben_erlaubt(&profil(&[("schreiben", " AUS ")])));
        assert!(!schreiben_erlaubt(&profil(&[("schreiben", "false")])));
    }

    #[test]
    fn der_ordner_kommt_aus_dem_profil() {
        assert_eq!(ordner(&profil(&[])), "");
        assert_eq!(ordner(&profil(&[("ordner", "/tmp/x")])), "/tmp/x");
    }
}
