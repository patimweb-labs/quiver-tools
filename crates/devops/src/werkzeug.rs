// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Das Werkzeug „DevOps“ (SPEC §2, Aufgabe 09).
//!
//! Wie `quiver-ssh` und `quiver-abap` hat DevOps **mehrere** Zugänge: der User
//! legt im quiver-Fenster Organisationen an (`Eintrag.mehrfach`), quiver übergibt
//! sie als JSON in `QUIVER_ZUGAENGE`. Die Felder einer Organisation stehen darum
//! unter `mehrfach` und nicht am Eintrag selbst.

use quiver_tools_kern::zugaenge::UMGEBUNG;
use quiver_tools_kern::{hinweis_zugang, Feld, FeldArt, Profil, Werkzeug};

use crate::client::{verbindungsdaten, Client};
use crate::daten;
use crate::organisationen::{
    Sammlung, CA_ZERTIFIKAT, PAT, PROJEKT, SCHREIBEN, UNVERSCHLUESSELT, URL,
};

/// Kurzname: Teil der Werkzeugnamen (`devops_status`) und des Programms.
pub const NAME: &str = "devops";
/// Anzeigename im quiver-Fenster und in MCP.
pub const TITEL: &str = "DevOps";
/// Überschrift der Organisationsliste im quiver-Fenster.
pub const MEHRFACH_TITEL: &str = "Organisationen";
/// Bezeichnung eines Elements der Liste.
pub const MEHRFACH_EINZAHL: &str = "Organisation";

/// Das DevOps-Werkzeug.
#[derive(Debug, Clone, Copy, Default)]
pub struct Devops;

impl Werkzeug for Devops {
    fn name(&self) -> &'static str {
        NAME
    }

    fn titel(&self) -> &'static str {
        TITEL
    }

    fn beschreibung(&self) -> &'static str {
        "Azure DevOps je Organisation: Arbeitsitems, Kommentare, Iterationen, Repos, Branches, \
         Pull Requests, Pipelines und Läufe lesen. Anlegen und Ändern (Arbeitsitems, Kommentare, \
         Links, Pull Requests, Abstimmungen, Pipeline-Läufe) nur, wenn der Schalter „Schreiben \
         erlauben“ für die Organisation an ist."
    }

    fn felder(&self) -> Vec<Feld> {
        vec![
            Feld {
                name: URL,
                titel: "Adresse",
                art: FeldArt::Url,
                pflicht: true,
                standard: None,
                hilfe: "z. B. https://dev.azure.com/<firma> oder ein Azure DevOps Server \
                        https://tfs.example.test/tfs/DefaultCollection",
                link: None,
            },
            Feld {
                name: PROJEKT,
                titel: "Standardprojekt",
                art: FeldArt::Text,
                pflicht: false,
                standard: None,
                hilfe: "Leer heißt: das Projekt wird bei jedem Aufruf angegeben",
                link: None,
            },
            Feld {
                name: PAT,
                titel: "Personal Access Token",
                art: FeldArt::Passwort,
                pflicht: true,
                standard: None,
                hilfe: "In DevOps unter Benutzereinstellungen → Personal access tokens erzeugen. \
                        Rechte nach Bedarf (Work Items, Code, Build: lesen bzw. schreiben).",
                link: Some(("PAT erstellen", "${url}/_usersSettings/tokens")),
            },
            Feld {
                name: UNVERSCHLUESSELT,
                titel: quiver_tools_kern::net::TITEL_UNVERSCHLUESSELT,
                art: FeldArt::Schalter,
                pflicht: false,
                standard: Some("aus"),
                hilfe: "Nur nötig, wenn die Organisation über http:// erreichbar ist. Ohne diesen \
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
                hilfe: "Erlaubt Anlegen und Ändern (Work Items, Pull Requests, Kommentare, \
                        Pipelines starten) für diese Organisation",
                link: None,
            },
        ]
    }

    fn mehrfach(&self) -> Option<(&'static str, &'static str)> {
        Some((MEHRFACH_TITEL, MEHRFACH_EINZAHL))
    }

    /// `quiver-devops testen`: jede eingetragene Organisation wirklich anmelden.
    async fn testen(&self, _profil: &Profil) -> anyhow::Result<String> {
        let sammlung = Sammlung::aus_umgebung()?;
        if sammlung.ist_leer() {
            anyhow::bail!("{} ({UMGEBUNG} ist leer.)", hinweis_zugang(TITEL));
        }

        let mut erreichbar = Vec::new();
        for organisation in sammlung.alle() {
            let client = Client::neu(organisation)?;
            // `connectionData` ist ein Vorschau-Endpunkt; der Aufruf kommt aus
            // dem Client, damit die `api-version` nicht hier vergessen wird.
            let antwort = client.json(&verbindungsdaten()).await?;
            let anzeige = daten::text(&antwort, "authenticatedUser.displayName")
                .or_else(|| daten::text(&antwort, "authenticatedUser.providerDisplayName"))
                .unwrap_or_else(|| "unbekannt".to_string());
            erreichbar.push(format!(
                "{} – angemeldet als {} ({})",
                organisation.name, anzeige, organisation.url
            ));
        }
        Ok(format!(
            "{} Organisation(en) erreichbar:\n- {}",
            erreichbar.len(),
            erreichbar.join("\n- ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn das_werkzeug_beschreibt_eine_liste_von_organisationen() {
        let werkzeug = Devops;
        assert_eq!(werkzeug.name(), "devops");
        assert_eq!(werkzeug.titel(), "DevOps");
        assert!(werkzeug.beschreibung().contains("Pull Requests"));
        assert!(werkzeug.beschreibung().contains("Schreiben erlauben"));
        assert_eq!(
            werkzeug.mehrfach(),
            Some(("Organisationen", "Organisation"))
        );

        let felder = werkzeug.felder();
        assert_eq!(
            felder.iter().map(|feld| feld.name).collect::<Vec<_>>(),
            vec![
                "url",
                "projekt",
                "pat",
                "unverschluesselt_erlauben",
                "ca_zertifikat",
                "schreiben"
            ]
        );
        // Reihenfolge und Art wie in der Aufgabe vorgegeben.
        assert_eq!(felder[0].art, FeldArt::Url);
        assert!(felder[0].pflicht);
        assert_eq!(felder[1].art, FeldArt::Text);
        assert!(!felder[1].pflicht);
        assert_eq!(felder[2].art, FeldArt::Passwort);
        assert!(felder[2].pflicht);
        assert!(felder[2].hilfe.contains("Personal access tokens"));
        assert_eq!(felder[3].art, FeldArt::Schalter);
        assert_eq!(felder[3].standard, Some("aus"));
        assert_eq!(felder[4].art, FeldArt::Text);
        assert_eq!(felder[5].art, FeldArt::Schalter);
        assert_eq!(felder[5].standard, Some("aus"));
        assert!(felder[5].hilfe.contains("Pipelines starten"));
        // Alle Hilfe-Texte erklären das Feld, keiner ist leer.
        assert!(felder.iter().all(|feld| !feld.hilfe.is_empty()));
    }

    #[test]
    fn der_pat_hat_einen_hilfe_link_auf_die_token_seite() {
        let felder = Devops.felder();
        let pat = felder.iter().find(|feld| feld.name == PAT).unwrap();
        let (text, url) = pat.link.expect("der PAT braucht einen Link");
        assert_eq!(text, "PAT erstellen");
        // `${url}` setzt quiver aus dem Feld `url` zusammen.
        assert_eq!(url, "${url}/_usersSettings/tokens");
    }
}
