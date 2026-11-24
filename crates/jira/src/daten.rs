// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! JSON → Werte (Aufgabe 13).
//!
//! Jira antwortet je Endpunkt mit einer anderen Verpackung: mal `{ values: [ … ] }`
//! (`/project/search`), mal `{ issues: [ … ] }` (`/search/jql`), mal
//! `{ comments: [ … ] }`, mal eine nackte Liste (`/user/search`). Ein Feld ist
//! meist ein Text, manchmal ein Objekt mit `name` oder `displayName`. Dieses
//! Modul liest solche Antworten mit **einem** Satz Helfer, statt für jeden
//! Endpunkt ein eigenes Schema zu pflegen. Es ist rein und ohne Zustand.

use serde_json::Value;

/// Der Wert unter einem Punktpfad: `"fields.summary"`, `"issues.0.key"`.
pub fn wert<'a>(baum: &'a Value, pfad: &str) -> Option<&'a Value> {
    // Jira-Felder heißen `summary` oder `customfield_10010` und stehen unter
    // `fields`; sie enthalten selbst keinen Punkt. `fields.status.name` ist
    // darum ein normaler Pfad.
    let mut aktuell = baum;
    for teil in pfad.split('.').filter(|teil| !teil.is_empty()) {
        aktuell = match teil.parse::<usize>() {
            Ok(index) => aktuell.get(index)?,
            Err(_) => aktuell.get(teil)?,
        };
    }
    (!aktuell.is_null()).then_some(aktuell)
}

/// Ein Text unter einem Punktpfad.
///
/// Zeichenketten und Zahlen werden übernommen, ein Objekt über sein `name` bzw.
/// `displayName` (so liefert Jira Status, Typen und Benutzer).
pub fn text(baum: &Value, pfad: &str) -> Option<String> {
    match wert(baum, pfad)? {
        Value::String(wert) => {
            let wert = wert.trim();
            (!wert.is_empty()).then(|| wert.to_string())
        }
        Value::Number(zahl) => Some(zahl.to_string()),
        Value::Bool(wahr) => Some(wahr.to_string()),
        objekt @ Value::Object(_) => ["name", "displayName", "value", "key"]
            .iter()
            .find_map(|feld| text(objekt, feld)),
        _ => None,
    }
}

/// Wie [`text`], aber mit Vorgabe.
pub fn text_oder(baum: &Value, pfad: &str, vorgabe: &str) -> String {
    text(baum, pfad).unwrap_or_else(|| vorgabe.to_string())
}

/// Eine Zahl unter einem Punktpfad (auch als Zeichenkette).
pub fn zahl(baum: &Value, pfad: &str) -> Option<i64> {
    match wert(baum, pfad)? {
        Value::Number(zahl) => zahl.as_i64(),
        Value::String(wert) => wert.trim().parse().ok(),
        _ => None,
    }
}

/// Eine Wahrheit unter einem Punktpfad.
pub fn wahr(baum: &Value, pfad: &str) -> bool {
    matches!(wert(baum, pfad), Some(Value::Bool(true)))
}

/// Eine Liste unter einem Punktpfad.
///
/// Verstanden werden die nackte Liste, eine Verpackung mit `value`/`values`
/// und – wenn der Pfad selbst fehlt – die gängigen Listenschlüssel der API.
/// Fehlt alles, ist die Liste leer.
pub fn liste<'a>(baum: &'a Value, pfad: &str) -> &'a [Value] {
    const LEER: &[Value] = &[];
    let Some(wert) = wert(baum, pfad) else {
        if pfad.is_empty() {
            return verkappung(baum);
        }
        return LEER;
    };
    match wert {
        Value::Array(eintraege) => eintraege,
        verpackung @ Value::Object(_) => verkappung(verpackung),
        _ => LEER,
    }
}

/// Die Liste in einer Verpackung (`value`, `values`, `…s`) oder in der Antwort.
fn verkappung(baum: &Value) -> &[Value] {
    const LEER: &[Value] = &[];
    match baum {
        Value::Array(eintraege) => eintraege,
        Value::Object(_) => [
            "value",
            "values",
            "issues",
            "comments",
            "transitions",
            "issueLinkTypes",
            "fields",
        ]
        .iter()
        .find_map(|schluessel| match baum.get(schluessel) {
            Some(Value::Array(eintraege)) => Some(eintraege.as_slice()),
            _ => None,
        })
        .unwrap_or(LEER),
        _ => LEER,
    }
}

/// Ein Datum aus Jira-Text auf `JJJJ-MM-TT` (oder `JJJJ-MM-TT hh:mm`).
///
/// Jira liefert `2026-11-20T10:15:00.000+0000`; ein reines Datum
/// (`2026-11-30`) bleibt, wie es ist.
pub fn datum(text: Option<String>) -> Option<String> {
    let text = text?;
    let text = text.trim();
    if text.len() < 10 {
        return Some(text.to_string());
    }
    let tag = &text[..10];
    if !tag
        .chars()
        .all(|zeichen| zeichen.is_ascii_digit() || zeichen == '-')
    {
        return Some(text.to_string());
    }
    let zeit = text.get(11..16).unwrap_or_default();
    if zeit.len() == 5 && zeit.as_bytes()[2] == b':' && zeit != "00:00" {
        Some(format!("{tag} {zeit}"))
    } else {
        Some(tag.to_string())
    }
}

/// Eine Liste von Namen (`labels`, `components`) als Text.
pub fn namen(baum: &Value, pfad: &str) -> Vec<String> {
    liste(baum, pfad)
        .iter()
        .filter_map(|eintrag| match eintrag {
            Value::String(name) => Some(name.clone()),
            objekt @ Value::Object(_) => text(objekt, "").or_else(|| text(objekt, "name")),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ein_punktpfad_findet_werte() {
        let baum = json!({
            "key": "ABC-1",
            "fields": { "summary": "Login kaputt", "status": { "name": "Offen" },
                        "customfield_10010": 3 },
            "issues": [ { "key": "ABC-1" }, { "key": "ABC-2" } ]
        });
        assert_eq!(
            text(&baum, "fields.summary").as_deref(),
            Some("Login kaputt")
        );
        assert_eq!(text(&baum, "fields.status").as_deref(), Some("Offen"));
        assert_eq!(zahl(&baum, "fields.customfield_10010"), Some(3));
        assert_eq!(text(&baum, "issues.1.key").as_deref(), Some("ABC-2"));
        assert_eq!(text(&baum, "gibtsnicht"), None);
        assert_eq!(text(&baum, "fields.gibtsnicht"), None);
        assert_eq!(text_oder(&baum, "gibtsnicht", "-"), "-");
        assert!(!wahr(&baum, "key"));
    }

    #[test]
    fn listen_werden_mit_und_ohne_verpackung_gelesen() {
        let verpackt = json!({ "isLast": true, "values": [1, 2] });
        assert_eq!(liste(&verpackt, "values").len(), 2);
        assert_eq!(liste(&verpackt, "").len(), 2);

        let nackt = json!([1, 2, 3]);
        assert_eq!(liste(&nackt, "").len(), 3);

        assert_eq!(liste(&json!({ "issues": [1] }), "").len(), 1);
        assert!(liste(&json!({}), "values").is_empty());
        assert!(liste(&json!({}), "").is_empty());
    }

    #[test]
    fn daten_werden_gekuerzt() {
        assert_eq!(
            datum(Some("2026-11-20T10:15:00.000+0000".into())).as_deref(),
            Some("2026-11-20 10:15")
        );
        assert_eq!(
            datum(Some("2026-11-20T00:00:00.000+0000".into())).as_deref(),
            Some("2026-11-20")
        );
        assert_eq!(
            datum(Some("2026-11-30".into())).as_deref(),
            Some("2026-11-30")
        );
        assert_eq!(datum(Some("kurz".into())).as_deref(), Some("kurz"));
        assert_eq!(datum(None), None);
    }

    #[test]
    fn namen_lesen_zeichenketten_und_objekte() {
        let baum = json!({ "fields": { "labels": ["login", "bug"] } });
        assert_eq!(
            namen(&baum, "fields.labels"),
            vec!["login".to_string(), "bug".to_string()]
        );

        let baum = json!({ "components": [ { "name": "Web" }, { "name": "API" } ] });
        assert_eq!(
            namen(&baum, "components"),
            vec!["Web".to_string(), "API".to_string()]
        );
        assert!(namen(&json!({}), "components").is_empty());
    }
}
