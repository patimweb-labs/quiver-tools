// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Welcher Übergang ist gemeint? (Aufgabe 13, KORREKTUREN)
//!
//! `jira_uebergang` darf **nur** Übergänge ausführen, die
//! `GET issue/{key}/transitions` nennt. Der Agent gibt eine ID oder einen Namen;
//! dieses Modul entscheidet, welcher Übergang gemeint ist:
//!
//! - **ID** (Ziffern) muss exakt passen,
//! - **Name** ohne Groß-/Kleinschreibung,
//! - mehrere Treffer sind **mehrdeutig** → die Kandidaten werden genannt,
//! - kein Treffer → die vorhandenen Übergänge werden genannt.
//!
//! Die Funktionen sind rein: kein Netz, kein Zustand – und darum gut zu prüfen.

use serde_json::Value;

use crate::daten;

/// Das Ergebnis der Suche nach dem gewählten Übergang.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wahl<'a> {
    /// Genau einer passt.
    Eindeutig(&'a Value),
    /// Mehrere passen – der Agent muss die ID nennen.
    Mehrdeutig(Vec<&'a Value>),
    /// Keiner passt.
    Unbekannt,
}

/// Der gewählte Übergang aus der Liste von `GET …/transitions`.
pub fn waehlen<'a>(liste: &'a [Value], gewaehlt: &str) -> Wahl<'a> {
    let gewaehlt = gewaehlt.trim();
    if gewaehlt.is_empty() {
        return Wahl::Unbekannt;
    }
    let per_id: Vec<&Value> = liste
        .iter()
        .filter(|uebergang| daten::text(uebergang, "id").as_deref() == Some(gewaehlt))
        .collect();
    let treffer = if per_id.is_empty() {
        liste
            .iter()
            .filter(|uebergang| {
                daten::text(uebergang, "name")
                    .map(|name| name.eq_ignore_ascii_case(gewaehlt))
                    .unwrap_or(false)
            })
            .collect::<Vec<_>>()
    } else {
        per_id
    };
    match treffer.as_slice() {
        [] => Wahl::Unbekannt,
        [einzig] => Wahl::Eindeutig(einzig),
        mehrere => Wahl::Mehrdeutig(mehrere.to_vec()),
    }
}

/// Ein Übergang als `21 Fertig → Done`.
pub fn zeile(uebergang: &Value) -> String {
    format!(
        "{} {} → {}",
        daten::text_oder(uebergang, "id", "?"),
        daten::text_oder(uebergang, "name", "(ohne Namen)"),
        daten::text_oder(uebergang, "to.name", "?")
    )
}

/// Der Fehlertext, wenn der gewählte Übergang nicht in der Liste steht.
pub fn unbekannt(key: &str, gewaehlt: &str, liste: &[Value]) -> String {
    if liste.is_empty() {
        return format!(
            "Für {key} nennt Jira keinen Übergang – mit dem Status des Kontos ist hier nichts zu \
             tun. Es wurde nichts geändert."
        );
    }
    format!(
        "Den Übergang „{gewaehlt}“ nennt Jira für {key} nicht – es wurde nichts geändert. \
         Möglich sind:\n- {}",
        liste.iter().map(zeile).collect::<Vec<_>>().join("\n- ")
    )
}

/// Der Fehlertext, wenn mehrere Übergänge denselben Namen tragen.
pub fn mehrdeutig(key: &str, gewaehlt: &str, kandidaten: &[&Value]) -> String {
    format!(
        "Der Name „{gewaehlt}“ passt für {key} auf {} Übergänge – bitte die ID nennen, es wurde \
         nichts geändert:\n- {}",
        kandidaten.len(),
        kandidaten
            .iter()
            .map(|uebergang| zeile(uebergang))
            .collect::<Vec<_>>()
            .join("\n- ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn liste() -> Vec<Value> {
        vec![
            json!({ "id": "11", "name": "Start Progress", "to": { "name": "In Progress" } }),
            json!({ "id": "21", "name": "Fertig", "to": { "name": "Done" } }),
            json!({ "id": "31", "name": "Fertig", "to": { "name": "Closed" } }),
            json!({ "id": "41", "name": "11", "to": { "name": "Wieder offen" } }),
        ]
    }

    #[test]
    fn die_id_geht_vor_dem_namen() {
        let liste = liste();
        let Wahl::Eindeutig(gefunden) = waehlen(&liste, "11") else {
            panic!("11 ist nicht eindeutig");
        };
        assert_eq!(
            daten::text(gefunden, "to.name").as_deref(),
            Some("In Progress")
        );

        // Ein Name, der wie eine ID aussieht, wird von der ID überstimmt.
        let Wahl::Eindeutig(gefunden) = waehlen(&liste, "41") else {
            panic!("41 ist nicht eindeutig");
        };
        assert_eq!(
            daten::text(gefunden, "to.name").as_deref(),
            Some("Wieder offen")
        );
    }

    #[test]
    fn der_name_gilt_ohne_gross_kleinschreibung() {
        let liste = liste();
        let Wahl::Mehrdeutig(kandidaten) = waehlen(&liste, "fertig") else {
            panic!("„fertig“ ist mehrdeutig");
        };
        assert_eq!(kandidaten.len(), 2);
        assert!(mehrdeutig("ABC-1", "fertig", &kandidaten).contains("21 Fertig → Done"));

        let Wahl::Unbekannt = waehlen(&liste, "Gibt es nicht") else {
            panic!("unbekannt erwartet");
        };
        let text = unbekannt("ABC-1", "Gibt es nicht", &liste);
        assert!(text.contains("nichts geändert"), "{text}");
        assert!(text.contains("11 Start Progress → In Progress"), "{text}");
        assert!(text.contains("31 Fertig → Closed"), "{text}");
        assert_eq!(waehlen(&liste, "  "), Wahl::Unbekannt);
    }

    #[test]
    fn ohne_uebergaenge_ist_die_meldung_kurz() {
        let text = unbekannt("ABC-1", "11", &[]);
        assert!(text.contains("keinen Übergang"), "{text}");
        assert!(text.contains("nichts geändert"), "{text}");
    }
}
