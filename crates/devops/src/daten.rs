// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! JSON → Werte (Aufgabe 09).
//!
//! Azure DevOps antwortet je Endpunkt mit einer anderen Form: mal `{ value: [ … ] }`,
//! mal `{ comments: [ … ] }`, mal `{ workItemRelations: [ … ] }`, und ein Benutzer
//! ist mal ein Text, mal ein Objekt mit `displayName`. Dieses Modul liest solche
//! Antworten mit **einem** Satz Helfer, statt für jeden Endpunkt ein eigenes
//! Schema zu pflegen. Es ist rein und ohne Zustand.

use serde_json::Value;

/// Der Wert unter einem Punktpfad: `"fields.System.Title"`, `"value.0.id"`.
pub fn wert<'a>(baum: &'a Value, pfad: &str) -> Option<&'a Value> {
    // Arbeitsitem-Felder heißen `System.Title` und enthalten selbst Punkte:
    // nach `fields.` ist der Rest **ein** Schlüssel, kein Pfad.
    if let Some(feld) = pfad.strip_prefix("fields.") {
        return baum.get("fields")?.get(feld).filter(|wert| !wert.is_null());
    }
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
/// `displayName` (so liefert DevOps Benutzer und Zustände).
pub fn text(baum: &Value, pfad: &str) -> Option<String> {
    match wert(baum, pfad)? {
        Value::String(wert) => {
            let wert = wert.trim();
            (!wert.is_empty()).then(|| wert.to_string())
        }
        Value::Number(zahl) => Some(zahl.to_string()),
        Value::Bool(wahr) => Some(wahr.to_string()),
        objekt @ Value::Object(_) => ["displayName", "name", "$value"]
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
/// DevOps verpackt Listen meist als `{ count, value: [ … ] }`; beides – die
/// nackte Liste und die Verpackung – wird verstanden. Fehlt der Pfad, ist die
/// Liste leer.
pub fn liste<'a>(baum: &'a Value, pfad: &str) -> &'a [Value] {
    const LEER: &[Value] = &[];
    match wert(baum, pfad) {
        Some(Value::Array(eintraege)) => eintraege,
        Some(verpackung @ Value::Object(_)) => match verpackung.get("value") {
            Some(Value::Array(eintraege)) => eintraege,
            _ => LEER,
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
    // Mitternacht ist bei Iterationen und Fälligkeiten nur Rauschen.
    if zeit.len() == 5 && zeit.as_bytes()[2] == b':' && zeit != "00:00" {
        Some(format!("{tag} {zeit}"))
    } else {
        Some(tag.to_string())
    }
}

/// Der Name eines Repos aus einer Pull-Request-Antwort.
pub fn repo_von_pull_request(baum: &Value) -> Option<String> {
    text(baum, "repository.name").or_else(|| text(baum, "repository.id"))
}

/// Ist das ein System-Thread (Abstimmung, Push, Statuswechsel)?
///
/// DevOps legt solche Threads selbst an; sie tragen eine Eigenschaft
/// (`CodeReviewThreadType`) und haben für einen Agenten keinen Wert.
pub fn ist_system_thread(thread: &Value) -> bool {
    thread.get("properties").is_some_and(|eigenschaften| {
        eigenschaften
            .get("CodeReviewThreadType")
            .is_some_and(|typ| !typ.is_null())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ein_punktpfad_findet_werte() {
        let baum = json!({
            "id": 123,
            "fields": { "System.Title": "Login kaputt", "System.AssignedTo": { "displayName": "Patrick" } },
            "value": [ { "id": 1 }, { "id": 2 } ]
        });
        assert_eq!(
            text(&baum, "fields.System.Title").as_deref(),
            Some("Login kaputt")
        );
        assert_eq!(
            text(&baum, "fields.System.AssignedTo").as_deref(),
            Some("Patrick")
        );
        assert_eq!(zahl(&baum, "id"), Some(123));
        assert_eq!(zahl(&baum, "value.1.id"), Some(2));
        assert_eq!(text(&baum, "gibtsnicht"), None);
        assert_eq!(text(&baum, "fields.System.Fehlt"), None);
        assert_eq!(text_oder(&baum, "gibtsnicht", "-"), "-");
        assert!(!wahr(&baum, "id"));
    }

    #[test]
    fn listen_werden_mit_und_ohne_verpackung_gelesen() {
        let verpackt = json!({ "count": 2, "value": [1, 2] });
        assert_eq!(liste(&verpackt, "value").len(), 2);
        assert_eq!(liste(&verpackt, "").len(), 2);

        let nackt = json!([1, 2, 3]);
        assert_eq!(liste(&nackt, "").len(), 3);

        let fehlt = json!({});
        assert!(liste(&fehlt, "value").is_empty());
        assert!(liste(&fehlt, "value").is_empty());
    }

    #[test]
    fn daten_werden_gekuerzt() {
        assert_eq!(
            datum(Some("2026-11-20T10:15:00.123Z".into())).as_deref(),
            Some("2026-11-20 10:15")
        );
        assert_eq!(
            datum(Some("2026-11-20".into())).as_deref(),
            Some("2026-11-20")
        );
        assert_eq!(datum(Some("kurz".into())).as_deref(), Some("kurz"));
        assert_eq!(datum(None), None);
    }

    #[test]
    fn system_threads_werden_erkannt() {
        let system = json!({ "id": 1, "properties": { "CodeReviewThreadType": { "$value": "VoteUpdate" } } });
        let mensch = json!({ "id": 2, "comments": [ { "content": "hi" } ] });
        assert!(ist_system_thread(&system));
        assert!(!ist_system_thread(&mensch));
    }

    #[test]
    fn das_repo_kommt_aus_dem_pull_request() {
        let pr = json!({ "repository": { "name": "scoda-web", "id": "abc" } });
        assert_eq!(repo_von_pull_request(&pr).as_deref(), Some("scoda-web"));
        assert_eq!(repo_von_pull_request(&json!({})), None);
    }
}
