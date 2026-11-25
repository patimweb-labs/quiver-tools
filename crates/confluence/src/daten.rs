// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! JSON → Werte (Aufgabe 14).
//!
//! Confluence antwortet je Endpunkt mit einer anderen Form: Listen stehen unter
//! `results` (v2), `results`/`value` (v1), ein Benutzer ist mal ein Text, mal
//! ein Objekt mit `displayName`. Dieses Modul liest solche Antworten mit
//! **einem** Satz Helfer. Es ist rein und ohne Zustand.

use serde_json::Value;

/// Der Wert unter einem Punktpfad: `"version.number"`, `"results.0.id"`.
pub fn wert<'a>(baum: &'a Value, pfad: &str) -> Option<&'a Value> {
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
/// Zeichenketten und Zahlen werden übernommen, ein Objekt über sein
/// `displayName` (so liefert Confluence Benutzer).
pub fn text(baum: &Value, pfad: &str) -> Option<String> {
    match wert(baum, pfad)? {
        Value::String(wert) => {
            let wert = wert.trim();
            (!wert.is_empty()).then(|| wert.to_string())
        }
        Value::Number(zahl) => Some(zahl.to_string()),
        Value::Bool(wahr) => Some(wahr.to_string()),
        objekt @ Value::Object(_) => ["displayName", "publicName", "value"]
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
/// Confluence verpackt Listen als `{ results: [ … ] }` (v2 und v1-Suche); die
/// nackte Liste und die Verpackung `{ value: [ … ] }` werden auch verstanden.
/// Fehlt der Pfad, ist die Liste leer.
pub fn liste<'a>(baum: &'a Value, pfad: &str) -> &'a [Value] {
    const LEER: &[Value] = &[];
    match wert(baum, pfad) {
        Some(Value::Array(eintraege)) => eintraege,
        Some(verpackung @ Value::Object(_)) => match verpackung.get("results") {
            Some(Value::Array(eintraege)) => eintraege,
            _ => match verpackung.get("value") {
                Some(Value::Array(eintraege)) => eintraege,
                _ => LEER,
            },
        },
        _ => LEER,
    }
}

/// Ein Datum aus ISO-Text auf `JJJJ-MM-TT` (oder `JJJJ-MM-TT hh:mm`).
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
    // Mitternacht ist bei Anlege- und Änderungszeiten nur Rauschen.
    if zeit.len() == 5 && zeit.as_bytes()[2] == b':' && zeit != "00:00" {
        Some(format!("{tag} {zeit}"))
    } else {
        Some(tag.to_string())
    }
}

/// Die ersten Zeichen eines Textes, für knappe Antworten.
pub fn kurz(text: &str) -> String {
    let sauber = text.trim();
    let gekuerzt: String = sauber.chars().take(120).collect();
    if sauber.chars().count() > 120 {
        format!("{} …", gekuerzt.trim_end())
    } else {
        gekuerzt
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ein_punktpfad_findet_werte() {
        let baum = json!({
            "id": "123",
            "version": { "number": 3, "authorId": "abc" },
            "author": { "displayName": "Alex" },
            "results": [ { "id": "1" }, { "id": "2" } ]
        });
        assert_eq!(text(&baum, "id").as_deref(), Some("123"));
        assert_eq!(zahl(&baum, "version.number"), Some(3));
        assert_eq!(text(&baum, "author").as_deref(), Some("Alex"));
        assert_eq!(text(&baum, "results.1.id").as_deref(), Some("2"));
        assert_eq!(text(&baum, "gibtsnicht"), None);
        assert_eq!(text_oder(&baum, "gibtsnicht", "-"), "-");
        assert!(!wahr(&baum, "id"));
    }

    #[test]
    fn listen_werden_mit_und_ohne_verpackung_gelesen() {
        assert_eq!(liste(&json!({ "results": [1, 2] }), "results").len(), 2);
        assert_eq!(liste(&json!({ "value": [1, 2, 3] }), "value").len(), 3);
        assert_eq!(liste(&json!([1, 2, 3]), "").len(), 3);
        assert!(liste(&json!({}), "results").is_empty());
        // Eine Verpackung ohne Pfad liest `results`.
        assert_eq!(liste(&json!({ "results": [1] }), "").len(), 1);
    }

    #[test]
    fn daten_werden_gekuerzt() {
        assert_eq!(
            datum(Some("2026-11-20T10:15:00.123Z".into())).as_deref(),
            Some("2026-11-20 10:15")
        );
        assert_eq!(
            datum(Some("2026-11-20T00:00:00Z".into())).as_deref(),
            Some("2026-11-20")
        );
        assert_eq!(datum(Some("kurz".into())).as_deref(), Some("kurz"));
        assert_eq!(datum(None), None);
    }

    #[test]
    fn kurz_kuerzt_an_zeichen() {
        assert_eq!(kurz("  hallo  "), "hallo");
        let lang = "ä".repeat(200);
        let gekuerzt = kurz(&lang);
        assert!(gekuerzt.chars().count() <= 123, "{gekuerzt}");
        assert!(gekuerzt.ends_with('…'));
    }
}
