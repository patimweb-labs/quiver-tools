// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Das Werkzeug „Zephyr“ (Aufgabe 15).
//!
//! Wie `quiver-devops` hat Zephyr **mehrere** Zugänge: der User legt im
//! quiver-Fenster Zugänge an (`Eintrag.mehrfach`), quiver übergibt sie als JSON
//! in `QUIVER_ZUGAENGE`. Die Felder eines Zugangs stehen darum unter `mehrfach`
//! und nicht am Eintrag selbst.

use quiver_tools_kern::zugaenge::UMGEBUNG;
use quiver_tools_kern::{hinweis_zugang, Feld, FeldArt, Profil, Werkzeug};

use crate::client::{Anfrage, Client};
use crate::format;
use crate::zugaenge::{
    Sammlung, BASIS_URL, CA_ZERTIFIKAT, NUR, PROJEKT, SCHREIBEN, STANDARD_BASIS_URL, TOKEN,
    UNVERSCHLUESSELT,
};

/// Kurzname: Teil der Werkzeugnamen (`zephyr_status`) und des Programms.
pub const NAME: &str = "zephyr";
/// Anzeigename im quiver-Fenster und in MCP.
pub const TITEL: &str = "Zephyr";
/// Überschrift der Zugangsliste im quiver-Fenster.
pub const MEHRFACH_TITEL: &str = "Zugänge";
/// Bezeichnung eines Elements der Liste.
pub const MEHRFACH_EINZAHL: &str = "Zugang";

/// Das Zephyr-Werkzeug.
#[derive(Debug, Clone, Copy, Default)]
pub struct Zephyr;

impl Werkzeug for Zephyr {
    fn name(&self) -> &'static str {
        NAME
    }

    fn titel(&self) -> &'static str {
        TITEL
    }

    fn beschreibung(&self) -> &'static str {
        "Zephyr Scale Cloud (REST v2): Testfälle, Testzyklen und Ausführungen lesen, Status und \
         Ordner ansehen. Verändernd: einen Testfall anlegen, Testfälle einem Zyklus zuordnen und \
         ein Ergebnis eintragen – aber nur, wenn der Schalter „Schreiben erlauben“ für den Zugang \
         an ist."
    }

    fn felder(&self) -> Vec<Feld> {
        vec![
            Feld {
                name: BASIS_URL,
                titel: "Basisadresse",
                art: FeldArt::Url,
                pflicht: false,
                standard: Some(STANDARD_BASIS_URL),
                hilfe:
                    "Adresse von Zephyr Scale Cloud (REST v2). Normalerweise unverändert lassen.",
                link: None,
            },
            Feld {
                name: TOKEN,
                titel: "API-Token",
                art: FeldArt::Passwort,
                pflicht: true,
                standard: None,
                hilfe: "Im Jira-Profil unter „Zephyr API Access Tokens“ erzeugen. Das Token geht \
                        nur an die Basisadresse und steht in keiner Antwort.",
                link: None,
            },
            Feld {
                name: PROJEKT,
                titel: "Standardprojekt",
                art: FeldArt::Text,
                pflicht: false,
                standard: None,
                hilfe: "Projektschlüssel, z. B. QA. Leer heißt: das Projekt wird bei jedem Aufruf \
                        angegeben",
                link: None,
            },
            Feld {
                name: NUR,
                titel: "Nur diese Projekte",
                art: FeldArt::Text,
                pflicht: false,
                standard: None,
                hilfe: "Kommagetrennte Projektschlüssel. Leer heißt: alle. Gilt fürs Lesen \
                        und fürs Schreiben.",
                link: None,
            },
            Feld {
                name: UNVERSCHLUESSELT,
                titel: quiver_tools_kern::net::TITEL_UNVERSCHLUESSELT,
                art: FeldArt::Schalter,
                pflicht: false,
                standard: Some("aus"),
                hilfe: "Nur nötig, wenn die Basisadresse über http:// erreichbar ist. Ohne diesen \
                        Schalter sind nur 127.0.0.1, localhost und [::1] per http erlaubt.",
                link: None,
            },
            Feld {
                name: CA_ZERTIFIKAT,
                titel: quiver_tools_kern::net::TITEL_CA_ZERTIFIKAT,
                art: FeldArt::Text,
                pflicht: false,
                standard: None,
                hilfe: "Pfad einer PEM-Datei für ein eigenes CA-Zertifikat; leer heißt: Prüfung \
                        gegen den Zertifikatsspeicher des Systems.",
                link: None,
            },
            Feld {
                name: SCHREIBEN,
                titel: "Schreiben erlauben",
                art: FeldArt::Schalter,
                pflicht: false,
                standard: Some("aus"),
                hilfe: "Erlaubt Anlegen, Zuordnen und Ergebnisse (Testfälle, Ausführungen) für \
                        diesen Zugang",
                link: None,
            },
        ]
    }

    fn mehrfach(&self) -> Option<(&'static str, &'static str)> {
        Some((MEHRFACH_TITEL, MEHRFACH_EINZAHL))
    }

    /// `quiver-zephyr testen`: jeden eingetragenen Zugang wirklich anmelden.
    ///
    /// Zephyr nennt keinen Benutzernamen; geprüft wird darum, ob das Token gilt
    /// und welche Projekte sichtbar sind (`/projects?maxResults=1`).
    async fn testen(&self, _profil: &Profil) -> anyhow::Result<String> {
        let sammlung = Sammlung::aus_umgebung()?;
        if sammlung.ist_leer() {
            anyhow::bail!("{} ({UMGEBUNG} ist leer.)", hinweis_zugang(TITEL));
        }

        let mut erreichbar = Vec::new();
        for zugang in sammlung.alle() {
            let client =
                Client::neu(zugang).map_err(|fehler| anyhow::anyhow!(fehler.to_string()))?;
            let antwort = client
                .json(&Anfrage::holen("projects").mit_abfrage("maxResults", "1"))
                .await
                .map_err(|fehler| anyhow::anyhow!(fehler.to_string()))?;
            erreichbar.push(format::testen(zugang, &antwort));
        }
        Ok(format!(
            "{} Zugang/Zugänge erreichbar:\n- {}",
            erreichbar.len(),
            erreichbar.join("\n- ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn das_werkzeug_beschreibt_eine_liste_von_zugaengen() {
        let werkzeug = Zephyr;
        assert_eq!(werkzeug.name(), "zephyr");
        assert_eq!(werkzeug.titel(), "Zephyr");
        assert!(werkzeug.beschreibung().contains("Testzyklen"));
        assert!(werkzeug.beschreibung().contains("Schreiben erlauben"));
        assert_eq!(werkzeug.mehrfach(), Some(("Zugänge", "Zugang")));

        let felder = werkzeug.felder();
        assert_eq!(
            felder.iter().map(|feld| feld.name).collect::<Vec<_>>(),
            vec![
                "basis_url",
                "api_token",
                "projekt",
                "nur",
                "unverschluesselt_erlauben",
                "ca_zertifikat",
                "schreiben"
            ]
        );
        // Reihenfolge und Art wie in der Aufgabe vorgegeben.
        assert_eq!(felder[0].art, FeldArt::Url);
        assert!(!felder[0].pflicht);
        assert_eq!(felder[0].standard, Some(STANDARD_BASIS_URL));
        assert_eq!(felder[1].art, FeldArt::Passwort);
        assert!(felder[1].pflicht);
        assert!(felder[1].hilfe.contains("Zephyr API Access Tokens"));
        assert_eq!(felder[2].art, FeldArt::Text);
        assert!(!felder[2].pflicht);
        assert_eq!(felder[3].art, FeldArt::Text);
        assert!(!felder[3].pflicht);
        assert!(felder[3].hilfe.contains("Projektschlüssel"));
        assert_eq!(felder[4].art, FeldArt::Schalter);
        assert_eq!(felder[4].standard, Some("aus"));
        assert_eq!(felder[5].art, FeldArt::Text);
        assert_eq!(felder[6].art, FeldArt::Schalter);
        assert_eq!(felder[6].standard, Some("aus"));
        assert!(felder[6].hilfe.contains("Ergebnisse"));
        // Alle Hilfe-Texte erklären das Feld, keiner ist leer.
        assert!(felder.iter().all(|feld| !feld.hilfe.is_empty()));
    }
}
