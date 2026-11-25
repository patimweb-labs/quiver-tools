// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! WIQL aus den Komfort-Parametern bauen (Aufgabe 09).
//!
//! `devops_workitems_suchen` nimmt entweder eine fertige `wiql` oder die
//! bequemen Felder `text`, `zugewiesen_an_mich`, `status` und `typ`, aus denen
//! dieses Modul eine Abfrage baut. Die Funktion ist rein: kein Netz, kein
//! Zustand – und darum gut zu prüfen.

/// Die bequemen Felder einer Suche. Leere Angaben zählen als fehlend.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Komfort<'a> {
    /// Freitext, gesucht im Titel (`CONTAINS`).
    pub text: Option<&'a str>,
    /// Nur Arbeitsitems, die mir zugewiesen sind (`@Me`).
    pub zugewiesen_an_mich: bool,
    /// Status, z. B. `Active`, `Closed`.
    pub status: Option<&'a str>,
    /// Arbeitsitem-Typ, z. B. `Bug`, `User Story`.
    pub typ: Option<&'a str>,
}

impl Komfort<'_> {
    /// Steht in den Komfort-Feldern überhaupt etwas?
    pub fn ist_leer(&self) -> bool {
        !self.zugewiesen_an_mich
            && [self.text, self.status, self.typ]
                .iter()
                .all(|wert| wert.map(str::trim).unwrap_or_default().is_empty())
    }
}

/// Die Felder, die die Suche vom Server zurückhaben will.
pub const FELDER: [&str; 8] = [
    "System.Id",
    "System.WorkItemType",
    "System.Title",
    "System.State",
    "System.AssignedTo",
    "System.AreaPath",
    "System.IterationPath",
    "System.ChangedDate",
];

/// So viele Treffer holt eine Suche höchstens (Standard 50, höchstens 200).
pub fn top(wert: Option<u32>) -> usize {
    wert.unwrap_or(50).clamp(1, 200) as usize
}

/// Das Projekt in einer WIQL-Abfrage: in Anführungszeichen, `'` verdoppelt.
fn text(wert: &str) -> String {
    format!("'{}'", wert.trim().replace('\'', "''"))
}

/// Eine WIQL-Abfrage aus dem Projekt und den Komfort-Feldern.
///
/// ```text
/// SELECT [System.Id] FROM WorkItems
///   WHERE [System.TeamProject] = 'PROJEKT' AND [System.State] = 'Active'
///   ORDER BY [System.ChangedDate] DESC
/// ```
pub fn bauen(projekt: &str, komfort: &Komfort<'_>) -> String {
    let mut bedingungen = vec![format!("[System.TeamProject] = {}", text(projekt))];

    if let Some(typ) = komfort.typ.map(str::trim).filter(|typ| !typ.is_empty()) {
        bedingungen.push(format!("[System.WorkItemType] = {}", text(typ)));
    }
    if let Some(status) = komfort
        .status
        .map(str::trim)
        .filter(|status| !status.is_empty())
    {
        bedingungen.push(format!("[System.State] = {}", text(status)));
    }
    if komfort.zugewiesen_an_mich {
        bedingungen.push("[System.AssignedTo] = @Me".to_string());
    }
    if let Some(suche) = komfort
        .text
        .map(str::trim)
        .filter(|suche| !suche.is_empty())
    {
        bedingungen.push(format!("[System.Title] CONTAINS {}", text(suche)));
    }

    format!(
        "SELECT [System.Id] FROM WorkItems WHERE {} ORDER BY [System.ChangedDate] DESC",
        bedingungen.join(" AND ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ohne_komfort_bleibt_das_projekt() {
        let abfrage = bauen("PROJEKT", &Komfort::default());
        assert_eq!(
            abfrage,
            "SELECT [System.Id] FROM WorkItems WHERE [System.TeamProject] = 'PROJEKT' \
             ORDER BY [System.ChangedDate] DESC"
        );
        assert!(Komfort::default().ist_leer());
    }

    #[test]
    fn alle_komfort_felder_stehen_in_der_abfrage() {
        let komfort = Komfort {
            text: Some("Login"),
            zugewiesen_an_mich: true,
            status: Some("Active"),
            typ: Some("Bug"),
        };
        let abfrage = bauen("PROJEKT", &komfort);
        assert!(
            abfrage.contains("[System.WorkItemType] = 'Bug'"),
            "{abfrage}"
        );
        assert!(abfrage.contains("[System.State] = 'Active'"), "{abfrage}");
        assert!(abfrage.contains("[System.AssignedTo] = @Me"), "{abfrage}");
        assert!(
            abfrage.contains("[System.Title] CONTAINS 'Login'"),
            "{abfrage}"
        );
        assert!(abfrage.starts_with("SELECT [System.Id] FROM WorkItems WHERE "));
        assert!(abfrage.ends_with("ORDER BY [System.ChangedDate] DESC"));
        assert!(!komfort.ist_leer());
    }

    #[test]
    fn leerraum_und_anfuehrungszeichen_werden_behandelt() {
        let komfort = Komfort {
            text: Some("   "),
            zugewiesen_an_mich: false,
            status: Some(""),
            typ: Some("  "),
        };
        assert!(komfort.ist_leer());
        let abfrage = bauen("PROJEKT", &komfort);
        assert!(!abfrage.contains("CONTAINS"), "{abfrage}");
        assert!(!abfrage.contains("[System.State]"), "{abfrage}");

        // Ein Apostroph im Suchtext darf die Abfrage nicht zerlegen.
        let komfort = Komfort {
            text: Some("Kunde's Login"),
            ..Komfort::default()
        };
        assert!(bauen("PROJEKT", &komfort).contains("CONTAINS 'Kunde''s Login'"));
    }

    #[test]
    fn top_ist_begrenzt() {
        assert_eq!(top(None), 50);
        assert_eq!(top(Some(0)), 1);
        assert_eq!(top(Some(10)), 10);
        assert_eq!(top(Some(5000)), 200);
    }
}
