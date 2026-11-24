// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! CQL aus den Komfort-Parametern bauen (Aufgabe 14).
//!
//! `confluence_suchen` nimmt entweder ein fertiges `cql` oder die bequemen
//! Felder `text` und `bereich`, aus denen dieses Modul eine Abfrage baut. Beide
//! Werte kommen vom Agenten und landen in einer Zeichenkette, die Confluence
//! auswertet – darum werden sie **maskiert**: `"` und `\` bekommen einen
//! Backslash, Zeilenumbrüche werden zu Leerzeichen. Ohne das könnte ein Wert
//! die Abfrage zerlegen (Aufgabe 12, Sicherheit 4).
//!
//! Die Funktion ist rein: kein Netz, kein Zustand.

/// Die bequemen Felder einer Suche. Leere Angaben zählen als fehlend.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Komfort<'a> {
    /// Freitext, gesucht im Inhalt (`text ~ "…"`).
    pub text: Option<&'a str>,
    /// Bereichsschlüssel (`space = "…"`).
    pub bereich: Option<&'a str>,
}

impl Komfort<'_> {
    /// Steht in den Komfort-Feldern überhaupt etwas?
    pub fn ist_leer(&self) -> bool {
        [self.text, self.bereich]
            .iter()
            .all(|wert| wert.map(str::trim).unwrap_or_default().is_empty())
    }
}

/// Einen Wert für eine CQL-Zeichenkette maskieren.
pub fn maskieren(wert: &str) -> String {
    let mut ergebnis = String::with_capacity(wert.len());
    for zeichen in wert.trim().chars() {
        match zeichen {
            '\\' => ergebnis.push_str("\\\\"),
            '"' => ergebnis.push_str("\\\""),
            '\n' | '\r' | '\t' => ergebnis.push(' '),
            sonst => ergebnis.push(sonst),
        }
    }
    ergebnis
}

/// Ein Wert als CQL-Zeichenkette: `"…"`, maskiert.
pub fn zeichenkette(wert: &str) -> String {
    format!("\"{}\"", maskieren(wert))
}

/// Eine CQL-Abfrage aus den Komfort-Feldern; `None`, wenn beide leer sind.
///
/// ```text
/// space = "ENG" AND text ~ "Anmeldung"
/// ```
pub fn bauen(komfort: &Komfort<'_>) -> Option<String> {
    let mut teile = Vec::new();
    if let Some(bereich) = komfort
        .bereich
        .map(str::trim)
        .filter(|bereich| !bereich.is_empty())
    {
        teile.push(format!("space = {}", zeichenkette(bereich)));
    }
    if let Some(suche) = komfort
        .text
        .map(str::trim)
        .filter(|suche| !suche.is_empty())
    {
        teile.push(format!("text ~ {}", zeichenkette(suche)));
    }
    if teile.is_empty() {
        return None;
    }
    Some(teile.join(" AND "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ohne_komfort_gibt_es_keine_abfrage() {
        assert!(Komfort::default().ist_leer());
        assert_eq!(bauen(&Komfort::default()), None);
        let leer = Komfort {
            text: Some("  "),
            bereich: Some(""),
        };
        assert!(leer.ist_leer());
        assert_eq!(bauen(&leer), None);
    }

    #[test]
    fn alle_komfort_felder_stehen_in_der_abfrage() {
        let komfort = Komfort {
            text: Some("Anmeldung"),
            bereich: Some("ENG"),
        };
        assert_eq!(
            bauen(&komfort).as_deref(),
            Some("space = \"ENG\" AND text ~ \"Anmeldung\"")
        );
        assert!(!komfort.ist_leer());

        assert_eq!(
            bauen(&Komfort {
                text: Some("Anmeldung"),
                bereich: None
            })
            .as_deref(),
            Some("text ~ \"Anmeldung\"")
        );
        assert_eq!(
            bauen(&Komfort {
                text: None,
                bereich: Some(" ENG ")
            })
            .as_deref(),
            Some("space = \"ENG\"")
        );
    }

    #[test]
    fn anfuehrungszeichen_und_backslash_werden_maskiert() {
        let komfort = Komfort {
            text: Some("Kunde sagt \"Hallo\""),
            bereich: Some("ENG"),
        };
        assert_eq!(
            bauen(&komfort).as_deref(),
            Some("space = \"ENG\" AND text ~ \"Kunde sagt \\\"Hallo\\\"\"")
        );

        assert_eq!(maskieren("a\\b"), "a\\\\b");
        assert_eq!(maskieren("a\nb"), "a b");
        // Ein Anführungszeichen beendet die Zeichenkette nicht mehr.
        assert_eq!(
            zeichenkette("x\" OR space = \"OPS"),
            "\"x\\\" OR space = \\\"OPS\""
        );
    }
}
