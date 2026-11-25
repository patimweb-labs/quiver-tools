// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Das Werkzeug „SSH“ (SPEC §2, Aufgabe 05).
//!
//! Anders als die übrigen Tools hat SSH **mehrere** Zugänge: der User legt im
//! quiver-Fenster Verbindungen an (`Eintrag.mehrfach`), quiver übergibt sie als
//! JSON in `QUIVER_ZUGAENGE`. Die Felder einer Verbindung stehen darum unter
//! `mehrfach` und nicht am Eintrag selbst.

use quiver_tools_kern::zugaenge::UMGEBUNG;
use quiver_tools_kern::{hinweis_zugang, Feld, FeldArt, Profil, Werkzeug};

use crate::anmeldung;
use crate::pfade::Pfade;
use crate::verbindung::{Sammlung, AUSTAUSCHORDNER, BENUTZER, HOST, PASSWORT, PORT, SCHREIBEN};

/// Kurzname: Teil der Werkzeugnamen (`ssh_exec`) und des Programms.
pub const NAME: &str = "ssh";
/// Anzeigename im quiver-Fenster und in MCP.
pub const TITEL: &str = "SSH";
/// Überschrift der Verbindungsliste im quiver-Fenster.
pub const MEHRFACH_TITEL: &str = "Verbindungen";
/// Bezeichnung eines Elements der Liste.
pub const MEHRFACH_EINZAHL: &str = "Verbindung";

/// Das SSH-Werkzeug.
#[derive(Debug, Clone, Copy, Default)]
pub struct Ssh;

impl Werkzeug for Ssh {
    fn name(&self) -> &'static str {
        NAME
    }

    fn titel(&self) -> &'static str {
        TITEL
    }

    fn beschreibung(&self) -> &'static str {
        "SSH-Verbindungen: Befehle ausführen, Dateien übertragen (SFTP), Schlüssel einrichten."
    }

    fn felder(&self) -> Vec<Feld> {
        vec![
            Feld {
                name: BENUTZER,
                titel: "Benutzername",
                art: FeldArt::Text,
                pflicht: true,
                standard: None,
                hilfe: "Anmeldename auf dem entfernten Rechner",
                link: None,
            },
            Feld {
                name: HOST,
                titel: "Hostname",
                art: FeldArt::Text,
                pflicht: true,
                standard: None,
                hilfe: "Name oder IP-Adresse des Servers",
                link: None,
            },
            Feld {
                name: PORT,
                titel: "Port",
                art: FeldArt::Zahl,
                pflicht: false,
                standard: Some("22"),
                hilfe: "Standard 22 – nur ändern, wenn der Server einen anderen Port nutzt",
                link: None,
            },
            Feld {
                name: PASSWORT,
                titel: "Passwort",
                art: FeldArt::Passwort,
                pflicht: false,
                standard: None,
                hilfe: "Für die Anmeldung und für sudo. Bleibt verschlüsselt auf diesem Gerät.",
                link: None,
            },
            Feld {
                name: SCHREIBEN,
                titel: quiver_tools_kern::TITEL_SCHREIBEN,
                art: FeldArt::Schalter,
                pflicht: false,
                standard: Some("aus"),
                hilfe:
                    "Erlaubt Befehle, das Einrichten des Schlüssels sowie Hoch- und Herunterladen. \
                        Standard: aus – ohne diesen Schalter geht nichts an den Server.",
                link: None,
            },
            Feld {
                name: AUSTAUSCHORDNER,
                titel: "Austauschordner",
                art: FeldArt::Ordner,
                pflicht: false,
                standard: None,
                hilfe: "Ordner auf diesem Gerät für das Hoch- und Herunterladen. Ohne dieses Feld \
                        werden keine Dateien übertragen; jede Datei bleibt in diesem Ordner.",
                link: None,
            },
        ]
    }

    fn mehrfach(&self) -> Option<(&'static str, &'static str)> {
        Some((MEHRFACH_TITEL, MEHRFACH_EINZAHL))
    }

    /// `quiver-ssh testen`: jede eingetragene Verbindung wirklich öffnen.
    async fn testen(&self, _profil: &Profil) -> anyhow::Result<String> {
        let pfade = Pfade::aus_umgebung();
        let sammlung = Sammlung::aus_umgebung()?;
        if sammlung.ist_leer() {
            anyhow::bail!("{} ({UMGEBUNG} ist leer.)", hinweis_zugang(TITEL));
        }

        let mut erreichbar = Vec::new();
        for verbindung in sammlung.alle() {
            let bereit = anmeldung::bereit(verbindung, &pfade).await?;
            bereit.sitzung.beenden().await;
            let mut zeile = verbindung.ziel();
            if let Some(hinweis) = bereit.hinweis {
                zeile = format!("{zeile} – {hinweis}");
            }
            erreichbar.push(zeile);
        }
        Ok(format!(
            "{} Verbindung(en) erreichbar:\n- {}",
            erreichbar.len(),
            erreichbar.join("\n- ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn das_werkzeug_beschreibt_eine_liste_von_verbindungen() {
        let werkzeug = Ssh;
        assert_eq!(werkzeug.name(), "ssh");
        assert_eq!(werkzeug.titel(), "SSH");
        assert!(werkzeug.beschreibung().contains("SFTP"));
        assert_eq!(werkzeug.mehrfach(), Some(("Verbindungen", "Verbindung")));

        let felder = werkzeug.felder();
        assert_eq!(felder.len(), 6);
        assert_eq!(
            felder.iter().map(|feld| feld.name).collect::<Vec<_>>(),
            vec![
                "benutzer",
                "host",
                "port",
                "anmelde_passwort",
                "schreiben",
                "austauschordner"
            ]
        );
        // benutzer, host: Pflicht, Text (die Liste im quiver-Fenster zeigt `benutzer@host`).
        assert!(felder[0].pflicht);
        assert_eq!(felder[0].art, FeldArt::Text);
        assert!(felder[1].pflicht);
        assert_eq!(felder[1].art, FeldArt::Text);
        assert_eq!(felder[2].art, FeldArt::Zahl);
        assert_eq!(felder[2].standard, Some("22"));
        assert!(!felder[2].pflicht);
        // Ein Feld für Anmeldung und sudo; ein eigenes sudo-Feld gibt es nicht mehr.
        assert_eq!(felder[3].titel, "Passwort");
        assert_eq!(felder[3].art, FeldArt::Passwort);
        assert!(!felder[3].pflicht);
        assert_eq!(
            felder[3].hilfe,
            "Für die Anmeldung und für sudo. Bleibt verschlüsselt auf diesem Gerät."
        );
        assert!(
            !felder.iter().any(|feld| feld.name == "sudo_passwort"),
            "das alte sudo-Feld steht noch im Eintrag"
        );
        // Der Schreibschalter ist im Zweifel aus; der Austauschordner ist eine Ordnerwahl.
        assert_eq!(felder[4].art, FeldArt::Schalter);
        assert_eq!(felder[4].standard, Some("aus"));
        assert_eq!(felder[5].art, FeldArt::Ordner);
        // Alle Hilfe-Texte erklären das Feld, keiner ist leer.
        assert!(felder.iter().all(|feld| !feld.hilfe.is_empty()));
        // Keine Entwicklersprache in den Hilfetexten.
        for feld in &felder {
            let hilfe = feld.hilfe.to_lowercase();
            assert!(!hilfe.contains("attrappe"), "{}", feld.name);
            assert!(!hilfe.contains("gepusht"), "{}", feld.name);
        }
    }
}
