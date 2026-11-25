// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! JQL aus den Komfort-Parametern bauen (Aufgabe 13).
//!
//! `jira_suchen` nimmt entweder eine fertige `jql` oder die bequemen Felder
//! `projekt`, `status`, `zugewiesen_an_mich` und `text`, aus denen dieses Modul
//! eine Abfrage baut. Zeichenketten werden **korrekt maskiert**: in JQL steht
//! eine Zeichenkette in einfachen Anführungszeichen, ein `'` darin wird mit
//! Backslash maskiert (`\'`), ein Backslash selbst verdoppelt (`\\`).
//!
//! `POST search/jql` verlangt laut Spezifikation eine **begrenzte** Abfrage
//! („bounded query“): `order by key desc` allein wird abgelehnt. Darum gibt es
//! [`ist_begrenzt`]; die Suche fragt Jira gar nicht erst ohne Einschränkung.
//!
//! Die Funktionen sind rein: kein Netz, kein Zustand – und darum gut zu prüfen.

/// Die bequemen Felder einer Suche. Leere Angaben zählen als fehlend.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Komfort<'a> {
    /// Projektschlüssel, z. B. `ABC`. Fehlt er, gelten die erlaubten Projekte
    /// des Zugangs (Feld `nur`).
    pub projekt: Option<&'a str>,
    /// Status, z. B. `In Progress`.
    pub status: Option<&'a str>,
    /// Nur Vorgänge, die mir zugewiesen sind (`currentUser()`).
    pub zugewiesen_an_mich: bool,
    /// Freitext, gesucht über `text ~`.
    pub text: Option<&'a str>,
}

impl Komfort<'_> {
    /// Steht in den Komfort-Feldern überhaupt etwas?
    pub fn ist_leer(&self) -> bool {
        !self.zugewiesen_an_mich
            && [self.projekt, self.status, self.text]
                .iter()
                .all(|wert| wert.map(str::trim).unwrap_or_default().is_empty())
    }
}

/// Eine Zeichenkette als JQL-Literal: in einfachen Anführungszeichen, maskiert.
pub fn zeichenkette(wert: &str) -> String {
    let mut maskiert = String::with_capacity(wert.len() + 2);
    maskiert.push('\'');
    for zeichen in wert.chars() {
        match zeichen {
            '\\' => maskiert.push_str("\\\\"),
            '\'' => maskiert.push_str("\\'"),
            sonst => maskiert.push(sonst),
        }
    }
    maskiert.push('\'');
    maskiert
}

/// Eine Abfrage aus dem Projekt und den Komfort-Feldern.
///
/// ```text
/// project = 'ABC' AND status = 'In Progress' AND assignee = currentUser()
///   AND text ~ 'Login' ORDER BY updated DESC
/// ```
///
/// Ist kein Projekt genannt, aber der Zugang erlaubt nur bestimmte Projekte
/// (`nur`), werden diese mit `project IN (…)` eingegrenzt. Bleibt keine
/// Einschränkung übrig, enthält die Abfrage nur die Sortierung – dann lehnt
/// Jira sie ab (siehe [`ist_begrenzt`], `jira_suchen` sagt es vorher).
pub fn bauen(erlaubte: &[String], komfort: &Komfort<'_>) -> String {
    let mut bedingungen: Vec<String> = Vec::new();

    let projekt = komfort
        .projekt
        .map(str::trim)
        .filter(|projekt| !projekt.is_empty());
    match projekt {
        Some(projekt) => bedingungen.push(format!("project = {}", zeichenkette(projekt))),
        None if !erlaubte.is_empty() => bedingungen.push(format!(
            "project IN ({})",
            erlaubte
                .iter()
                .map(|projekt| zeichenkette(projekt))
                .collect::<Vec<_>>()
                .join(", ")
        )),
        None => {}
    }

    if let Some(status) = komfort
        .status
        .map(str::trim)
        .filter(|status| !status.is_empty())
    {
        bedingungen.push(format!("status = {}", zeichenkette(status)));
    }
    if komfort.zugewiesen_an_mich {
        bedingungen.push("assignee = currentUser()".to_string());
    }
    if let Some(suche) = komfort
        .text
        .map(str::trim)
        .filter(|suche| !suche.is_empty())
    {
        bedingungen.push(format!("text ~ {}", zeichenkette(suche)));
    }

    let mut abfrage = bedingungen.join(" AND ");
    if !abfrage.is_empty() {
        abfrage.push(' ');
    }
    abfrage.push_str("ORDER BY updated DESC");
    abfrage
}

/// Verlangt Jira für diese Abfrage eine Einschränkung, die sie nicht hat?
///
/// `POST search/jql` nimmt nur **begrenzte** Abfragen: eine `order by`-Klausel
/// ohne Bedingung (`order by key desc`) wird mit `400` abgelehnt. Begrenzt ist
/// eine Abfrage, wenn vor der Sortierung noch etwas steht.
pub fn ist_begrenzt(jql: &str) -> bool {
    !ohne_sortierung(jql).trim().is_empty()
}

/// Die Abfrage ohne ihre `order by`-Klausel.
fn ohne_sortierung(jql: &str) -> &str {
    // Nur ASCII klein schreiben: die Byte-Stellen bleiben so dieselben.
    let klein = jql.to_ascii_lowercase();
    let schluessel = "order by";
    let mut suche = 0;
    while let Some(stelle) = klein[suche..].find(schluessel) {
        let absolut = suche + stelle;
        let davor = absolut == 0 || klein.as_bytes()[absolut - 1].is_ascii_whitespace();
        let danach = klein
            .as_bytes()
            .get(absolut + schluessel.len())
            .map(|byte| byte.is_ascii_whitespace())
            .unwrap_or(true);
        if davor && danach {
            return &jql[..absolut];
        }
        suche = absolut + 1;
    }
    jql
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ohne_komfort_bleibt_die_sortierung() {
        let abfrage = bauen(&[], &Komfort::default());
        assert_eq!(abfrage, "ORDER BY updated DESC");
        assert!(Komfort::default().ist_leer());
        // Ohne Einschränkung lehnt `search/jql` ab – das sagt die Suche vorher.
        assert!(!ist_begrenzt(&abfrage));
    }

    #[test]
    fn alle_komfort_felder_stehen_in_der_abfrage() {
        let komfort = Komfort {
            projekt: Some("ABC"),
            status: Some("In Progress"),
            zugewiesen_an_mich: true,
            text: Some("Login"),
        };
        let abfrage = bauen(&[], &komfort);
        assert_eq!(
            abfrage,
            "project = 'ABC' AND status = 'In Progress' AND assignee = currentUser() \
             AND text ~ 'Login' ORDER BY updated DESC"
        );
        assert!(!komfort.ist_leer());
        assert!(ist_begrenzt(&abfrage));
        // Reines JQL: kein `SELECT`, kein `WHERE`.
        assert!(!abfrage.contains("SELECT"), "{abfrage}");
        assert!(!abfrage.contains("WHERE"), "{abfrage}");
    }

    #[test]
    fn nur_eine_einschraenkung_macht_die_abfrage_begrenzt() {
        assert!(ist_begrenzt("project = ABC"));
        assert!(ist_begrenzt("project = ABC ORDER BY created DESC"));
        assert!(ist_begrenzt("assignee = currentUser() order by key"));
        assert!(ist_begrenzt("text ~ 'order by'"));
        assert!(!ist_begrenzt("ORDER BY key DESC"));
        assert!(!ist_begrenzt("  order by key desc "));
        assert!(!ist_begrenzt(""));
        assert!(!ist_begrenzt("   "));
    }

    #[test]
    fn zeichenketten_werden_maskiert() {
        assert_eq!(zeichenkette("ABC"), "'ABC'");
        // Ein Apostroph darf die Abfrage nicht zerlegen.
        assert_eq!(zeichenkette("Kunde's Login"), "'Kunde\\'s Login'");
        // Ein Backslash wird verdoppelt, sonst maskiert er das nächste Zeichen.
        assert_eq!(zeichenkette("C:\\temp"), "'C:\\\\temp'");
        assert_eq!(zeichenkette("a\\'b"), "'a\\\\\\'b'");
    }

    #[test]
    fn erlaubte_projekte_grenzen_eine_suche_ohne_projekt_ein() {
        let erlaubte = vec!["ABC".to_string(), "DEF".to_string()];
        let abfrage = bauen(&erlaubte, &Komfort::default());
        assert!(abfrage.contains("project IN ('ABC', 'DEF')"), "{abfrage}");

        // Ein genanntes Projekt ersetzt die Liste.
        let komfort = Komfort {
            projekt: Some("XYZ"),
            ..Komfort::default()
        };
        let abfrage = bauen(&erlaubte, &komfort);
        assert!(abfrage.contains("project = 'XYZ'"), "{abfrage}");
        assert!(!abfrage.contains("project IN"), "{abfrage}");
    }

    #[test]
    fn leerraum_gilt_als_fehlend() {
        let komfort = Komfort {
            projekt: Some("   "),
            status: Some(""),
            zugewiesen_an_mich: false,
            text: Some("  "),
        };
        assert!(komfort.ist_leer());
        let abfrage = bauen(&[], &komfort);
        assert_eq!(abfrage, "ORDER BY updated DESC");
        assert!(!ist_begrenzt(&abfrage));
    }
}
