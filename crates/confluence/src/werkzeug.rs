// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Das Werkzeug „Confluence“ (Aufgabe 14).
//!
//! Wie `quiver-devops` pflegt der User **mehrere** Sites: der User legt sie im
//! quiver-Fenster an (`Eintrag.mehrfach` = „Sites“/„Site“), quiver übergibt sie
//! als JSON in `QUIVER_ZUGAENGE`. Die Felder einer Site stehen darum unter
//! `mehrfach` und nicht am Eintrag selbst.

use quiver_tools_kern::zugaenge::UMGEBUNG;
use quiver_tools_kern::{hinweis_zugang, Feld, FeldArt, Profil, Werkzeug};

use crate::client::{Anfrage, Client, BENUTZER};
use crate::daten;
use crate::sites::{Sammlung, CA_ZERTIFIKAT, EMAIL, NUR, SCHREIBEN, TOKEN, UNVERSCHLUESSELT, URL};

/// Kurzname: Teil der Werkzeugnamen (`confluence_status`) und des Programms.
pub const NAME: &str = "confluence";
/// Anzeigename im quiver-Fenster und in MCP.
pub const TITEL: &str = "Confluence";
/// Überschrift der Site-Liste im quiver-Fenster.
pub const MEHRFACH_TITEL: &str = "Sites";
/// Bezeichnung eines Elements der Liste.
pub const MEHRFACH_EINZAHL: &str = "Site";
/// Hilfe-Link des API-Tokens: dieselbe Seite wie bei Jira (Aufgabe 13).
pub const TOKEN_LINK: &str = "https://id.atlassian.com/manage-profile/security/api-tokens";

/// Das Confluence-Werkzeug.
#[derive(Debug, Clone, Copy, Default)]
pub struct Confluence;

impl Werkzeug for Confluence {
    fn name(&self) -> &'static str {
        NAME
    }

    fn titel(&self) -> &'static str {
        TITEL
    }

    fn beschreibung(&self) -> &'static str {
        "Confluence Cloud je Site: Bereiche, Seiten (Storage-Format als Text), Unterseiten, \
         Kommentare, Anhänge (nur Liste), Labels und die Suche über CQL lesen. Anlegen und Ändern \
         (Seite, Kommentar, Label) nur, wenn der Schalter „schreiben“ für die Site an ist."
    }

    fn felder(&self) -> Vec<Feld> {
        vec![
            Feld {
                name: URL,
                titel: "Adresse",
                art: FeldArt::Url,
                pflicht: true,
                standard: None,
                hilfe: "z. B. https://firma.atlassian.net – ohne /wiki",
                link: None,
            },
            Feld {
                name: EMAIL,
                titel: "E-Mail",
                art: FeldArt::Text,
                pflicht: true,
                standard: None,
                hilfe: "Die E-Mail des Atlassian-Kontos, mit dem das Token erstellt wurde",
                link: None,
            },
            Feld {
                name: TOKEN,
                titel: "API-Token",
                art: FeldArt::Passwort,
                pflicht: true,
                standard: None,
                hilfe: "Auf der Atlassian-Seite „API-Tokens“ erstellen; das Token gilt für alle \
                        Atlassian-Produkte des Kontos.",
                link: Some(("API-Token erstellen", TOKEN_LINK)),
            },
            Feld {
                name: NUR,
                titel: "Nur diese Bereiche",
                art: FeldArt::Text,
                pflicht: false,
                standard: None,
                hilfe: "Bereichsschlüssel, kommagetrennt (z. B. ENG,OPS). Leer heißt: alle \
                        Bereiche. Gilt nur fürs Schreiben.",
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
                hilfe: "Erlaubt Anlegen und Ändern (Seite, Kommentar, Label) für diese Site",
                link: None,
            },
        ]
    }

    fn mehrfach(&self) -> Option<(&'static str, &'static str)> {
        Some((MEHRFACH_TITEL, MEHRFACH_EINZAHL))
    }

    /// `quiver-confluence testen`: jede eingetragene Site wirklich anmelden.
    async fn testen(&self, _profil: &Profil) -> anyhow::Result<String> {
        let sammlung = Sammlung::aus_umgebung()?;
        if sammlung.ist_leer() {
            anyhow::bail!("{} ({UMGEBUNG} ist leer.)", hinweis_zugang(TITEL));
        }

        let mut erreichbar = Vec::new();
        for site in sammlung.alle() {
            let client = Client::neu(site)?;
            let antwort = client.json(&Anfrage::holen(BENUTZER)).await?;
            let anzeige = daten::text(&antwort, "displayName")
                .or_else(|| daten::text(&antwort, "publicName"))
                .unwrap_or_else(|| "unbekannt".to_string());
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
        let werkzeug = Confluence;
        assert_eq!(werkzeug.name(), "confluence");
        assert_eq!(werkzeug.titel(), "Confluence");
        assert!(werkzeug.beschreibung().contains("CQL"));
        assert!(werkzeug.beschreibung().contains("schreiben"));
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
        assert!(felder[3].hilfe.contains("kommagetrennt"));
        assert_eq!(felder[4].art, FeldArt::Schalter);
        assert_eq!(felder[5].art, FeldArt::Text);
        assert_eq!(felder[6].art, FeldArt::Schalter);
        assert_eq!(felder[6].standard, Some("aus"));
        // Alle Hilfe-Texte erklären das Feld, keiner ist leer.
        assert!(felder.iter().all(|feld| !feld.hilfe.is_empty()));
    }

    #[test]
    fn das_token_hat_den_gleichen_hilfe_link_wie_jira() {
        let felder = Confluence.felder();
        let token = felder.iter().find(|feld| feld.name == TOKEN).unwrap();
        let (text, url) = token.link.expect("das Token braucht einen Link");
        assert_eq!(text, "API-Token erstellen");
        assert_eq!(
            url,
            "https://id.atlassian.com/manage-profile/security/api-tokens"
        );
    }
}
