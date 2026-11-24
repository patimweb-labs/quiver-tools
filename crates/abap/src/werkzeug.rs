// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Das Werkzeug „ABAP“ (SPEC §2, Aufgabe 07).
//!
//! Wie `quiver-ssh` hat ABAP **mehrere** Zugänge: der User legt im
//! quiver-Fenster Systeme an (`Eintrag.mehrfach`), quiver übergibt sie als JSON
//! in `QUIVER_ZUGAENGE`. Die Felder eines Systems stehen darum unter `mehrfach`
//! und nicht am Eintrag selbst.

use quiver_tools_kern::zugaenge::UMGEBUNG;
use quiver_tools_kern::{hinweis_zugang, Feld, FeldArt, Profil, Werkzeug};

use crate::adt::client::{Anfrage, Client};
use crate::adt::daten;
use crate::systeme::{
    Sammlung, BENUTZER, MANDANT, PASSWORT, SCHREIBEN, SPRACHE, URL, ZERTIFIKAT_PRUEFEN,
};

/// Kurzname: Teil der Werkzeugnamen (`abap_search`) und des Programms.
pub const NAME: &str = "abap";
/// Anzeigename im quiver-Fenster und in MCP.
pub const TITEL: &str = "ABAP";
/// Überschrift der Systemliste im quiver-Fenster.
pub const MEHRFACH_TITEL: &str = "Systeme";
/// Bezeichnung eines Elements der Liste.
pub const MEHRFACH_EINZAHL: &str = "System";

/// Das ABAP-Werkzeug.
#[derive(Debug, Clone, Copy, Default)]
pub struct Abap;

impl Werkzeug for Abap {
    fn name(&self) -> &'static str {
        NAME
    }

    fn titel(&self) -> &'static str {
        TITEL
    }

    fn beschreibung(&self) -> &'static str {
        "ABAP über SAP ADT: Objekte suchen und lesen, Tabellen abfragen, Syntax, Unit-Tests, ATC, \
         Dumps, Traces und Transporte ansehen. Schreibende Werkzeuge (Sperren, Quelltext \
         schreiben, Aktivieren, Anlegen, Textelemente) nur, wenn der Schalter „schreiben“ im \
         quiver-Fenster für das System an ist."
    }

    fn felder(&self) -> Vec<Feld> {
        vec![
            Feld {
                name: BENUTZER,
                titel: "SAP-Benutzer",
                art: FeldArt::Text,
                pflicht: true,
                standard: None,
                hilfe: "Anmeldename im SAP-System",
                link: None,
            },
            Feld {
                name: URL,
                titel: "ADT-Adresse",
                art: FeldArt::Url,
                pflicht: true,
                standard: None,
                hilfe: "Adresse des ADT-Dienstes, z. B. https://sap-dev.firma.de:44300",
                link: None,
            },
            Feld {
                name: MANDANT,
                titel: "Mandant",
                art: FeldArt::Text,
                pflicht: true,
                standard: None,
                hilfe: "Mandantennummer, z. B. 100",
                link: None,
            },
            Feld {
                name: SPRACHE,
                titel: "Sprache",
                art: FeldArt::Text,
                pflicht: false,
                standard: Some("DE"),
                hilfe: "Anmeldesprache als zweistelliger Schlüssel (DE, EN)",
                link: None,
            },
            Feld {
                name: PASSWORT,
                titel: "SAP-Passwort",
                art: FeldArt::Passwort,
                pflicht: true,
                standard: None,
                hilfe: "Passwort des SAP-Benutzers",
                link: None,
            },
            Feld {
                name: ZERTIFIKAT_PRUEFEN,
                titel: "Serverzertifikat prüfen",
                art: FeldArt::Schalter,
                pflicht: false,
                standard: Some("an"),
                hilfe:
                    "Firmen-CAs erkennt der Client über den Zertifikatsspeicher des Systems. Nur \
                        abschalten, wenn das nicht reicht.",
                link: None,
            },
            Feld {
                name: SCHREIBEN,
                titel: "Schreiben erlauben",
                art: FeldArt::Schalter,
                pflicht: false,
                standard: Some("aus"),
                hilfe:
                    "Erlaubt abap_write, abap_activate, abap_create_object, \
                        abap_set_text_elements und Locks auf diesem System. Für Produktivsysteme aus \
                        lassen.",
                link: None,
            },
        ]
    }

    fn mehrfach(&self) -> Option<(&'static str, &'static str)> {
        Some((MEHRFACH_TITEL, MEHRFACH_EINZAHL))
    }

    /// `quiver-abap testen`: jedes eingetragene System wirklich anmelden.
    async fn testen(&self, _profil: &Profil) -> anyhow::Result<String> {
        let sammlung = Sammlung::aus_umgebung()?;
        if sammlung.ist_leer() {
            anyhow::bail!("{} ({UMGEBUNG} ist leer.)", hinweis_zugang(TITEL));
        }

        let mut erreichbar = Vec::new();
        for system in sammlung.alle() {
            let client = Client::neu(system)?;
            // Der Aufruf holt Token und Cookies und meldet sich dabei an.
            let antwort = client
                .anfrage(&Anfrage::holen("/sap/bc/adt/core/discovery"))
                .await?;
            let bereiche = daten::discovery_titel(&antwort.text).unwrap_or_default();
            erreichbar.push(format!(
                "{} – angemeldet als {} ({} ADT-Bereich(e))",
                system.name,
                system.anmeldung(),
                bereiche.len()
            ));
        }
        Ok(format!(
            "{} System(e) erreichbar:\n- {}",
            erreichbar.len(),
            erreichbar.join("\n- ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn das_werkzeug_beschreibt_eine_liste_von_systemen() {
        let werkzeug = Abap;
        assert_eq!(werkzeug.name(), "abap");
        assert_eq!(werkzeug.titel(), "ABAP");
        assert!(werkzeug.beschreibung().contains("ADT"));
        assert_eq!(werkzeug.mehrfach(), Some(("Systeme", "System")));

        let felder = werkzeug.felder();
        assert_eq!(
            felder.iter().map(|feld| feld.name).collect::<Vec<_>>(),
            vec![
                "benutzer",
                "url",
                "mandant",
                "sprache",
                "passwort",
                "zertifikat_pruefen",
                "schreiben"
            ]
        );
        // Reihenfolge und Art wie in der Aufgabe vorgegeben.
        assert_eq!(felder[0].art, FeldArt::Text);
        assert!(felder[0].pflicht);
        assert_eq!(felder[1].art, FeldArt::Url);
        assert!(felder[1].pflicht);
        assert_eq!(felder[2].art, FeldArt::Text);
        assert!(felder[2].pflicht);
        assert_eq!(felder[3].art, FeldArt::Text);
        assert!(!felder[3].pflicht);
        assert_eq!(felder[3].standard, Some("DE"));
        assert_eq!(felder[4].art, FeldArt::Passwort);
        assert!(felder[4].pflicht);
        assert_eq!(felder[5].art, FeldArt::Schalter);
        assert!(!felder[5].pflicht);
        assert_eq!(felder[5].standard, Some("an"));
        assert!(felder[5].hilfe.contains("Zertifikatsspeicher"));
        // Der Schalter „schreiben“ kommt ans Ende und ist im Zweifel aus.
        assert_eq!(felder[6].art, FeldArt::Schalter);
        assert!(!felder[6].pflicht);
        assert_eq!(felder[6].standard, Some("aus"));
        assert!(felder[6].hilfe.contains("abap_write"));
        assert!(felder[6].hilfe.contains("Produktivsysteme"));
        // Alle Hilfe-Texte erklären das Feld, keiner ist leer.
        assert!(felder.iter().all(|feld| !feld.hilfe.is_empty()));
    }
}
