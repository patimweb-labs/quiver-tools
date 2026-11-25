// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Namen → Ids (docs/aufgaben/06-graph-erweiterung.md, Regeln wie bei Teams).
//!
//! Die Tools nehmen Namen an, Graph will Ids. Aufgelöst wird über die Listen;
//! passen mehrere, nennt das Tool die Kandidaten und **tut nichts**. Diese
//! Helfer sind für die neuen Tools da; `teams_tools` bringt seine eigene,
//! unveränderte Fassung mit (Aufgabe 05 bleibt unangetastet).

use m365_anmeldung::Anmeldung;
use m365_graph::{GraphClient, Profil, Sammlung};

use crate::server::{ungueltig_tool, ToolFehler};

/// Ein aufgelöster Treffer: Id für Graph, Name für die Antwort.
#[derive(Debug, Clone)]
pub(crate) struct Anzeige {
    pub id: String,
    pub name: String,
}

impl Anzeige {
    pub fn neu(id: impl Into<String>, name: Option<&str>) -> Self {
        let id = id.into();
        let name = name
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(&id)
            .to_string();
        Self { id, name }
    }

    pub fn text(&self) -> String {
        if self.name == self.id {
            self.id.clone()
        } else {
            format!("{} ({})", self.name, self.id)
        }
    }
}

/// Sieht der Text nach einer GUID aus?
pub(crate) fn ist_guid(text: &str) -> bool {
    let text = text.trim();
    text.len() == 36
        && text.chars().enumerate().all(|(stelle, zeichen)| {
            if matches!(stelle, 8 | 13 | 18 | 23) {
                zeichen == '-'
            } else {
                zeichen.is_ascii_hexdigit()
            }
        })
}

/// Sieht der Text nach einer Graph-Id aus (GUID, Site-Id, Drive-Id)?
///
/// Site-Ids haben die Form `hostname,siteId,webId`, Drive- und Item-Ids sind
/// lange Zeichenketten ohne Leerzeichen. Ein Name wie „Projekt Nord“ fällt
/// dadurch heraus.
pub(crate) fn ist_graph_id(text: &str) -> bool {
    let text = text.trim();
    ist_guid(text)
        || (text.len() >= 20
            && !text.contains(' ')
            && !text.contains('/')
            && text.contains(|zeichen: char| zeichen.is_ascii_digit()))
}

pub(crate) fn mehrdeutig(art: &str, referenz: &str, treffer: &[&Anzeige]) -> ToolFehler {
    let liste = treffer
        .iter()
        .take(8)
        .map(|treffer| treffer.text())
        .collect::<Vec<String>>()
        .join(", ");
    ungueltig_tool(format!(
        "{art} {referenz:?} ist mehrdeutig – {} Treffer: {liste}. Bitte die Id nennen; es wurde nichts geändert.",
        treffer.len()
    ))
}

/// Genau ein Treffer über Id, genauer Name, sonst Name-Teil.
pub(crate) fn waehlen(
    kandidaten: Vec<Anzeige>,
    referenz: &str,
    art: &str,
) -> Result<Anzeige, ToolFehler> {
    let referenz = referenz.trim();
    let klein = referenz.to_lowercase();

    if let Some(treffer) = kandidaten
        .iter()
        .find(|kandidat| kandidat.id.eq_ignore_ascii_case(referenz))
    {
        return Ok(treffer.clone());
    }

    let genau: Vec<&Anzeige> = kandidaten
        .iter()
        .filter(|kandidat| kandidat.name.to_lowercase() == klein)
        .collect();
    match genau.len() {
        1 => return Ok(genau[0].clone()),
        0 => {}
        _ => return Err(mehrdeutig(art, referenz, &genau)),
    }

    let teil: Vec<&Anzeige> = kandidaten
        .iter()
        .filter(|kandidat| kandidat.name.to_lowercase().contains(&klein))
        .collect();
    match teil.len() {
        1 => Ok(teil[0].clone()),
        0 => Err(ungueltig_tool(format!(
            "{art} {referenz:?} nicht gefunden. Bitte die Liste abrufen und die Id nennen."
        ))),
        _ => Err(mehrdeutig(art, referenz, &teil)),
    }
}

/// Eine Person aus `GET /users`.
#[derive(Debug, Clone)]
pub(crate) struct Person {
    pub id: String,
    pub name: String,
    pub mail: Option<String>,
    pub upn: Option<String>,
}

impl Person {
    pub fn text(&self) -> String {
        match (&self.mail, &self.upn) {
            (Some(mail), _) => format!("{} <{mail}> ({})", self.name, self.id),
            (None, Some(upn)) => format!("{} <{upn}> ({})", self.name, self.id),
            _ => format!("{} ({})", self.name, self.id),
        }
    }
}

/// Personen über Name, Mail oder UPN suchen (`GET /users?$search=…`).
pub(crate) async fn personen_suchen<A: Anmeldung>(
    graph: &GraphClient<A>,
    suche: &str,
    anzahl: u32,
) -> Result<Vec<Person>, ToolFehler> {
    let abfrage = format!("\"displayName:{suche}\" OR \"mail:{suche}\"");
    let liste: Sammlung<Profil> = graph
        .get_mit_kopf(
            "/users",
            &[
                ("$search", abfrage),
                (
                    "$select",
                    "id,displayName,mail,userPrincipalName".to_string(),
                ),
                ("$top", anzahl.to_string()),
            ],
            &[("ConsistencyLevel", "eventual")],
        )
        .await?;
    Ok(liste
        .value
        .into_iter()
        .filter_map(|profil| {
            let id = profil.id?;
            Some(Person {
                name: profil.display_name.unwrap_or_else(|| id.clone()),
                mail: profil.mail,
                upn: profil.user_principal_name,
                id,
            })
        })
        .collect())
}

/// Eine Person eindeutig finden; mehrere Treffer ergeben keine Aktion.
pub(crate) async fn person_finden<A: Anmeldung>(
    graph: &GraphClient<A>,
    referenz: &str,
) -> Result<Person, ToolFehler> {
    let referenz = referenz.trim();
    if referenz.is_empty() {
        return Err(ungueltig_tool("Person fehlt"));
    }
    let personen = personen_suchen(graph, referenz, 10).await?;
    let klein = referenz.to_lowercase();
    let identisch: Vec<&Person> = personen
        .iter()
        .filter(|person| {
            person.id.eq_ignore_ascii_case(referenz)
                || person
                    .mail
                    .as_deref()
                    .is_some_and(|mail| mail.eq_ignore_ascii_case(referenz))
                || person
                    .upn
                    .as_deref()
                    .is_some_and(|upn| upn.eq_ignore_ascii_case(referenz))
                || person.name.to_lowercase() == klein
        })
        .collect();
    match identisch.len() {
        1 => return Ok(identisch[0].clone()),
        0 => {}
        _ => return Err(personen_mehrdeutig(referenz, &identisch)),
    }
    let teil: Vec<&Person> = personen
        .iter()
        .filter(|person| person.name.to_lowercase().contains(&klein))
        .collect();
    if teil.len() == 1 {
        return Ok(teil[0].clone());
    }
    let kandidaten: Vec<&Person> = if teil.is_empty() {
        personen.iter().collect()
    } else {
        teil
    };
    match kandidaten.len() {
        0 => Err(ungueltig_tool(format!(
            "Person {referenz:?} nicht gefunden. Bitte E-Mail oder UPN nennen."
        ))),
        _ => Err(personen_mehrdeutig(referenz, &kandidaten)),
    }
}

fn personen_mehrdeutig(referenz: &str, treffer: &[&Person]) -> ToolFehler {
    let liste = treffer
        .iter()
        .take(8)
        .map(|person| person.text())
        .collect::<Vec<String>>()
        .join(", ");
    ungueltig_tool(format!(
        "Person {referenz:?} ist mehrdeutig – {} Treffer: {liste}. Bitte E-Mail oder UPN nennen; es wurde nichts geändert.",
        treffer.len()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fehlertext(fehler: ToolFehler) -> String {
        match fehler {
            ToolFehler::Fehler(fehler) => format!("{fehler}"),
            ToolFehler::Hinweis(text) => text,
        }
    }

    #[test]
    fn ids_werden_erkannt() {
        assert!(ist_guid("12345678-1234-1234-1234-123456789abc"));
        assert!(!ist_guid("Entwicklung"));
        assert!(ist_graph_id("contoso.sharepoint.com,1111,aaaa"));
        assert!(ist_graph_id("01ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"));
        assert!(!ist_graph_id("Projekt Nord"));
        assert!(!ist_graph_id("Projekte/Nord"));
        assert!(!ist_graph_id("Notizen"));
    }

    #[test]
    fn namen_loesen_genau_dann_teilweise_auf() {
        let kandidaten = vec![
            Anzeige::neu("s1", Some("Projekt Nord")),
            Anzeige::neu("s2", Some("Projekt Süd")),
            Anzeige::neu("s3", Some("Projekt Nord Intern")),
        ];
        assert_eq!(
            waehlen(kandidaten.clone(), "s2", "Site").expect("Id").name,
            "Projekt Süd"
        );
        assert_eq!(
            waehlen(kandidaten.clone(), "projekt süd", "Site")
                .expect("genau")
                .id,
            "s2"
        );
        assert_eq!(
            waehlen(kandidaten.clone(), "Intern", "Site")
                .expect("Teil")
                .id,
            "s3"
        );
        assert_eq!(
            waehlen(kandidaten.clone(), "Projekt Nord", "Site")
                .expect("genauer Name gewinnt")
                .id,
            "s1"
        );

        let text =
            fehlertext(waehlen(kandidaten.clone(), "Projekt", "Site").expect_err("mehrdeutig"));
        assert!(text.contains("mehrdeutig"), "{text}");
        assert!(text.contains("Projekt Nord (s1)"), "{text}");
        assert!(text.contains("nichts geändert"), "{text}");

        assert!(waehlen(kandidaten, "Quatsch", "Site").is_err());
    }

    #[test]
    fn eine_id_ohne_namen_zeigt_sich_selbst() {
        let anzeige = Anzeige::neu("drive-doku", None);
        assert_eq!(anzeige.text(), "drive-doku");
        assert_eq!(
            Anzeige::neu("drive-doku", Some("Dokumente")).text(),
            "Dokumente (drive-doku)"
        );
    }
}
