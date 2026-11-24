// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Das Werkzeug „Excalidraw“ (Aufgabe 17).
//!
//! Es gibt **keine** Zugangsdaten: das Werkzeug arbeitet auf Dateien in einem
//! Arbeitsordner. Der User trägt im quiver-Fenster nur zwei Dinge ein: den
//! Ordner (Pflicht) und ob geschrieben werden darf (Standard: an).

use quiver_diagramm::ablage::Ablage;
use quiver_tools_kern::{Feld, FeldArt, Profil, Werkzeug};

use crate::datei::ENDUNG;

/// Kurzname: Teil der Werkzeugnamen (`excalidraw_status`) und des Programms.
pub const NAME: &str = "excalidraw";
/// Anzeigename im quiver-Fenster und in MCP.
pub const TITEL: &str = "Excalidraw";
/// Feld „Arbeitsordner“.
pub const ORDNER: &str = "ordner";
/// Feld „Schreiben erlauben“.
pub const SCHREIBEN: &str = "schreiben";

/// Das Excalidraw-Werkzeug.
#[derive(Debug, Clone, Copy, Default)]
pub struct Excalidraw;

impl Werkzeug for Excalidraw {
    fn name(&self) -> &'static str {
        NAME
    }

    fn titel(&self) -> &'static str {
        TITEL
    }

    fn beschreibung(&self) -> &'static str {
        "Excalidraw-Dateien lokal im Arbeitsordner: aus einer Beschreibung (Knoten, Kanten, \
         Gruppen) erzeugen, lesen, Elemente ändern, hinzufügen und entfernen, als Bild (SVG/PNG) \
         ansehen und in den Arbeitsordner exportieren. Alles ohne Anmeldung und ohne Netz; \
         geöffnet wird die Datei von Hand auf excalidraw.com."
    }

    fn felder(&self) -> Vec<Feld> {
        vec![
            Feld {
                name: ORDNER,
                titel: "Arbeitsordner",
                art: FeldArt::Text,
                pflicht: true,
                standard: None,
                hilfe: "Voller Pfad, z. B. ~/Diagramme. Der Ordner wird angelegt; alle Pfade im \
                        Werkzeug sind relativ dazu und verlassen ihn nie.",
                link: None,
            },
            Feld {
                name: SCHREIBEN,
                titel: "Schreiben erlauben",
                art: FeldArt::Schalter,
                pflicht: false,
                standard: Some("an"),
                hilfe: "Erlaubt Anlegen, Ändern, Hinzufügen, Entfernen und Exportieren im \
                        Arbeitsordner. Ist der Schalter aus, ändern die Werkzeuge nichts.",
                link: None,
            },
        ]
    }

    /// `quiver-excalidraw testen`: den Arbeitsordner öffnen und zählen.
    async fn testen(&self, profil: &Profil) -> anyhow::Result<String> {
        let ordner = ordner(profil).ok_or_else(|| {
            anyhow::anyhow!(
                "{} (Feld „{ORDNER}“ fehlt.)",
                quiver_tools_kern::hinweis_zugang(TITEL)
            )
        })?;
        let ablage = Ablage::neu(ordner, ENDUNG)?;
        let dateien = ablage.liste()?;
        let schalter = if schreiben_an(profil) { "an" } else { "aus" };
        Ok(format!(
            "Arbeitsordner {} ist bereit: {} Datei(en) mit .{ENDUNG}, Schreiben ist {schalter}.",
            ablage.wurzel().display(),
            dateien.len()
        ))
    }
}

/// Der Arbeitsordner aus dem Zugang.
pub fn ordner(profil: &Profil) -> Option<&str> {
    profil
        .werte
        .get(ORDNER)
        .map(|wert| wert.trim())
        .filter(|wert| !wert.is_empty())
}

/// Ist der Schalter „schreiben“ an? Fehlt er, gilt sein Standard (an).
pub fn schreiben_an(profil: &Profil) -> bool {
    match profil.werte.get(SCHREIBEN) {
        None => true,
        Some(wert) => !matches!(
            wert.trim().to_ascii_lowercase().as_str(),
            "aus" | "nein" | "false" | "0" | "off"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn profil(werte: &[(&str, &str)]) -> Profil {
        let mut profil = Profil::leer();
        profil.werte = werte
            .iter()
            .map(|(name, wert)| ((*name).to_string(), (*wert).to_string()))
            .collect::<BTreeMap<_, _>>();
        profil
    }

    #[test]
    fn das_werkzeug_hat_genau_zwei_felder_und_keine_zugaenge() {
        let werkzeug = Excalidraw;
        assert_eq!(werkzeug.name(), "excalidraw");
        assert_eq!(werkzeug.titel(), "Excalidraw");
        assert!(
            werkzeug.mehrfach().is_none(),
            "ein Arbeitsordner, keine Liste"
        );
        let felder = werkzeug.felder();
        assert_eq!(
            felder.iter().map(|feld| feld.name).collect::<Vec<_>>(),
            vec!["ordner", "schreiben"]
        );
        assert_eq!(felder[0].art, FeldArt::Text);
        assert!(felder[0].pflicht);
        assert_eq!(felder[1].art, FeldArt::Schalter);
        assert_eq!(felder[1].standard, Some("an"));
        assert!(felder.iter().all(|feld| !feld.hilfe.is_empty()));
    }

    #[test]
    fn der_schalter_ist_ohne_angabe_an() {
        assert!(schreiben_an(&profil(&[("ordner", "/tmp/x")])));
        assert!(schreiben_an(&profil(&[("schreiben", "an")])));
        assert!(!schreiben_an(&profil(&[("schreiben", "aus")])));
        assert!(!schreiben_an(&profil(&[("schreiben", " false ")])));
        assert!(!schreiben_an(&profil(&[("schreiben", "nein")])));
        assert!(schreiben_an(&profil(&[("schreiben", "ja")])));
    }

    #[test]
    fn der_ordner_wird_getrimmt_und_leer_ist_nichts() {
        assert_eq!(ordner(&profil(&[("ordner", " /tmp/x ")])), Some("/tmp/x"));
        assert_eq!(ordner(&profil(&[("ordner", "   ")])), None);
        assert_eq!(ordner(&profil(&[])), None);
    }

    #[test]
    fn ohne_ordner_sagt_testen_was_fehlt() {
        let laufzeit = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let fehler = laufzeit
            .block_on(Excalidraw.testen(&profil(&[])))
            .unwrap_err();
        assert!(fehler.to_string().contains("quiver-Fenster"), "{fehler}");
    }
}
