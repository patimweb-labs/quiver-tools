// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Das Werkzeug „Jira“ (SPEC §2, Aufgabe 13).
//!
//! Wie `quiver-devops` hat Jira **mehrere** Zugänge: der User legt im
//! quiver-Fenster Sites an (`Eintrag.mehrfach`), quiver übergibt sie als JSON in
//! `QUIVER_ZUGAENGE`. Die Felder einer Site stehen darum unter `mehrfach` und
//! nicht am Eintrag selbst.

use quiver_tools_kern::zugaenge::UMGEBUNG;
use quiver_tools_kern::{hinweis_zugang, Feld, FeldArt, Profil, Werkzeug};

use crate::client::{Anfrage, Client};
use crate::daten;
use crate::sites::{Sammlung, CA_ZERTIFIKAT, EMAIL, NUR, SCHREIBEN, TOKEN, UNVERSCHLUESSELT, URL};

/// Kurzname: Teil der Werkzeugnamen (`jira_status`) und des Programms.
pub const NAME: &str = "jira";
/// Anzeigename im quiver-Fenster und in MCP.
pub const TITEL: &str = "Jira";
/// Überschrift der Site-Liste im quiver-Fenster.
pub const MEHRFACH_TITEL: &str = "Sites";
/// Bezeichnung eines Elements der Liste.
pub const MEHRFACH_EINZAHL: &str = "Site";

/// Die Adresse, unter der sich ein API-Token erzeugen lässt.
pub const TOKEN_LINK: &str = "https://id.atlassian.com/manage-profile/security/api-tokens";

/// Das Jira-Werkzeug.
#[derive(Debug, Clone, Copy, Default)]
pub struct Jira;

impl Werkzeug for Jira {
    fn name(&self) -> &'static str {
        NAME
    }

    fn titel(&self) -> &'static str {
        TITEL
    }

    fn beschreibung(&self) -> &'static str {
        "Jira Cloud je Site: Vorgänge suchen und lesen, Kommentare, Übergänge, Projekte, Filter, \
         Personen und Verknüpfungsarten ansehen. Anlegen, Ändern, Übergänge, Kommentare und \
         Verknüpfungen nur, wenn der Schalter „Schreiben erlauben“ für die Site an ist – und nur \
         in den Projekten aus dem Feld „Nur diese Projekte“."
    }

    fn felder(&self) -> Vec<Feld> {
        vec![
            Feld {
                name: URL,
                titel: "Adresse",
                art: FeldArt::Url,
                pflicht: true,
                standard: None,
                hilfe: "z. B. https://<firma>.atlassian.net",
                link: None,
            },
            Feld {
                name: EMAIL,
                titel: "E-Mail",
                art: FeldArt::Text,
                pflicht: true,
                standard: None,
                hilfe: "Die E-Mail-Adresse des Atlassian-Kontos, mit dem sich das Tool anmeldet.",
                link: None,
            },
            Feld {
                name: TOKEN,
                titel: "API-Token",
                art: FeldArt::Passwort,
                pflicht: true,
                standard: None,
                hilfe: "In Atlassian unter Konto-Einstellungen → Sicherheit → API-Tokens \
                        erstellen. Das Token wird zusammen mit der E-Mail als Basic-Anmeldung \
                        geschickt.",
                link: Some(("API-Token erstellen", TOKEN_LINK)),
            },
            Feld {
                name: NUR,
                titel: "Nur diese Projekte",
                art: FeldArt::Text,
                pflicht: false,
                standard: None,
                hilfe: "Kommagetrennte Projektschlüssel, z. B. ABC, DEF. Ist das Feld gesetzt, \
                        ändern die verändernden Werkzeuge nur Vorgänge in diesen Projekten. Leer \
                        heißt: alle Projekte, die das Konto sieht.",
                link: None,
            },
            Feld {
                name: UNVERSCHLUESSELT,
                titel: quiver_tools_kern::net::TITEL_UNVERSCHLUESSELT,
                art: FeldArt::Schalter,
                pflicht: false,
                standard: Some("aus"),
                hilfe:
                    "Nur nötig, wenn die Site über http:// erreichbar ist. Ohne diesen Schalter \
                        sind nur 127.0.0.1, localhost und [::1] per http erlaubt.",
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
                hilfe: "Erlaubt Anlegen, Ändern, Übergänge, Kommentare und Verknüpfungen für \
                        diese Site",
                link: None,
            },
        ]
    }

    fn mehrfach(&self) -> Option<(&'static str, &'static str)> {
        Some((MEHRFACH_TITEL, MEHRFACH_EINZAHL))
    }

    /// `quiver-jira testen`: jede eingetragene Site wirklich anmelden.
    async fn testen(&self, _profil: &Profil) -> anyhow::Result<String> {
        let sammlung = Sammlung::aus_umgebung()?;
        if sammlung.ist_leer() {
            anyhow::bail!("{} ({UMGEBUNG} ist leer.)", hinweis_zugang(TITEL));
        }

        let mut erreichbar = Vec::new();
        for site in sammlung.alle() {
            let client = Client::neu(site)?;
            let mich = client.json(&Anfrage::holen("myself")).await?;
            let anzeige = daten::text_oder(&mich, "displayName", "unbekannt");
            erreichbar.push(format!(
                "{} – angemeldet als {} ({})",
                site.name, anzeige, site.url
            ));
        }
        Ok(format!(
            "{} Site(s) erreichbar:\n- {}",
            erreichbar.len(),
            erreichbar.join("\n- ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn das_werkzeug_beschreibt_eine_liste_von_sites() {
        let werkzeug = Jira;
        assert_eq!(werkzeug.name(), "jira");
        assert_eq!(werkzeug.titel(), "Jira");
        assert!(werkzeug.beschreibung().contains("Vorgänge"));
        assert!(werkzeug.beschreibung().contains("Schreiben erlauben"));
        assert!(werkzeug.beschreibung().contains("Nur diese Projekte"));
        assert_eq!(werkzeug.mehrfach(), Some(("Sites", "Site")));

        let felder = werkzeug.felder();
        assert_eq!(
            felder.iter().map(|feld| feld.name).collect::<Vec<_>>(),
            vec![
                "url",
                "email",
                "api_token",
                "nur",
                "unverschluesselt_erlauben",
                "ca_zertifikat",
                "schreiben"
            ]
        );
        assert_eq!(felder[0].art, FeldArt::Url);
        assert!(felder[0].pflicht);
        assert_eq!(felder[1].art, FeldArt::Text);
        assert!(felder[1].pflicht);
        assert_eq!(felder[2].art, FeldArt::Passwort);
        assert!(felder[2].pflicht);
        assert_eq!(felder[3].art, FeldArt::Text);
        assert!(!felder[3].pflicht);
        assert_eq!(felder[4].art, FeldArt::Schalter);
        assert_eq!(felder[4].standard, Some("aus"));
        assert_eq!(felder[5].art, FeldArt::Text);
        assert_eq!(felder[6].art, FeldArt::Schalter);
        assert_eq!(felder[6].standard, Some("aus"));
        assert!(!felder[6].pflicht);
        // Alle Hilfe-Texte erklären das Feld, keiner ist leer.
        assert!(felder.iter().all(|feld| !feld.hilfe.is_empty()));
    }

    #[test]
    fn der_api_token_hat_einen_hilfe_link_auf_die_token_seite() {
        let felder = Jira.felder();
        let token = felder.iter().find(|feld| feld.name == TOKEN).unwrap();
        let (text, url) = token.link.expect("der Token braucht einen Link");
        assert_eq!(text, "API-Token erstellen");
        assert_eq!(url, TOKEN_LINK);
        assert!(url.starts_with("https://id.atlassian.com/"));
    }
}
