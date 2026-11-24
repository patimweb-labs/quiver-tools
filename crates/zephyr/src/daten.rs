// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! JSON → Werte (Aufgabe 15).
//!
//! Zephyr Scale antwortet mit einer festen Hülle für Listen
//! (`{ values: [ … ], total, isLast, next }`) und je Endpunkt mit eigenen
//! Objekten. Dieses Modul liest solche Antworten mit **einem** Satz Helfer,
//! statt für jeden Endpunkt ein eigenes Schema zu pflegen. Es ist rein und
//! ohne Zustand.

use serde_json::Value;

/// Der Wert unter einem Punktpfad: `"project.key"`, `"values.0.id"`.
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
/// Gelesen werden nur Zeichenketten, Zahlen und Wahrheitswerte. Ein Objekt
/// ergibt `None`: was in einem Verweis (`{"id": 11, "self": …}`) steht, liest
/// [`verweis_id`], und ein Name wird dort geholt, wo das Schema ihn nennt.
pub fn text(baum: &Value, pfad: &str) -> Option<String> {
    match wert(baum, pfad)? {
        Value::String(wert) => {
            let wert = wert.trim();
            (!wert.is_empty()).then(|| wert.to_string())
        }
        Value::Number(zahl) => Some(zahl.to_string()),
        Value::Bool(wahr) => Some(wahr.to_string()),
        _ => None,
    }
}

/// Die Id eines Verweises (`ProjectLink`, `StatusLink`, `FolderLink` …) als Text.
///
/// Die OpenAPI-Datei gibt Verweisen nur `id` und `self` – keinen `key` und
/// keinen `name`. Wer mehr braucht, fragt den Endpunkt, den die API dafür
/// nennt (`/projects/{id}`, `/statuses?projectKey=…`, `/environments/{id}` …).
pub fn verweis_id(verweis: &Value) -> Option<String> {
    zahl(verweis, "id").map(|id| id.to_string())
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
/// Zephyr verpackt Listen als `{ values: [ … ] }`; die nackte Liste und die
/// ältere Verpackung `{ value: [ … ] }` werden ebenfalls verstanden. Fehlt der
/// Pfad, ist die Liste leer.
pub fn liste<'a>(baum: &'a Value, pfad: &str) -> &'a [Value] {
    const LEER: &[Value] = &[];
    match wert(baum, pfad) {
        Some(Value::Array(eintraege)) => eintraege,
        Some(verpackung @ Value::Object(_)) => {
            for name in ["values", "value", "items"] {
                if let Some(Value::Array(eintraege)) = verpackung.get(name) {
                    return eintraege;
                }
            }
            LEER
        }
        _ => LEER,
    }
}

/// Wie viele Einträge die Antwort insgesamt hat.
pub fn gesamt(baum: &Value) -> Option<i64> {
    zahl(baum, "total").or_else(|| zahl(baum, "pageInfo.totalElements"))
}

/// Ist das die letzte Seite?
pub fn ist_letzte(baum: &Value) -> Option<bool> {
    wert(baum, "isLast")
        .and_then(Value::as_bool)
        .or_else(|| wert(baum, "pageInfo.isLast").and_then(Value::as_bool))
}

/// Die Adresse der nächsten Seite, falls die Antwort eine nennt.
///
/// `PagedList` nennt sie `next` – nur dieses Feld liest das Werkzeug.
pub fn folgeseite(baum: &Value) -> Option<String> {
    text(baum, "next")
}

/// Gibt es nach dieser Seite noch Einträge?
///
/// `ab` ist die Startposition der Seite, `gezeigt` die Zahl der tatsächlich
/// gelesenen Einträge. `total` und `isLast` entscheiden; fehlen beide, gilt
/// eine genannte Folgeseite als „es gibt mehr“.
pub fn mehr(baum: &Value, ab: u32, gezeigt: usize) -> bool {
    let gelesen = ab as i64 + gezeigt as i64;
    if let Some(gesamt) = gesamt(baum) {
        return gesamt > gelesen;
    }
    if let Some(letzte) = ist_letzte(baum) {
        return !letzte;
    }
    folgeseite(baum).is_some()
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
    // Mitternacht ist bei geplanten Zeiträumen nur Rauschen.
    if zeit.len() == 5 && zeit.as_bytes()[2] == b':' && zeit != "00:00" {
        Some(format!("{tag} {zeit}"))
    } else {
        Some(tag.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ein_punktpfad_findet_werte() {
        let baum = json!({
            "id": 123,
            "project": { "key": "QA", "name": "Qualität" },
            "values": [ { "id": 1 }, { "id": 2 } ]
        });
        assert_eq!(text(&baum, "project.key").as_deref(), Some("QA"));
        assert_eq!(zahl(&baum, "id"), Some(123));
        assert_eq!(zahl(&baum, "values.1.id"), Some(2));
        assert_eq!(text(&baum, "gibtsnicht"), None);
        assert_eq!(text_oder(&baum, "gibtsnicht", "-"), "-");
        assert!(!wahr(&baum, "id"));
        // Ein Verweis hat nur `id` und `self`; `text` macht daraus keinen Namen.
        assert_eq!(text(&baum, "values.0"), None);
        assert_eq!(verweis_id(&baum["values"][0]).as_deref(), Some("1"));
        assert_eq!(verweis_id(&baum["project"]), None);
    }

    #[test]
    fn listen_werden_mit_und_ohne_verpackung_gelesen() {
        assert_eq!(liste(&json!({ "values": [1, 2] }), "").len(), 2);
        assert_eq!(liste(&json!({ "value": [1, 2, 3] }), "").len(), 3);
        assert_eq!(liste(&json!([1, 2, 3]), "").len(), 3);
        assert!(liste(&json!({}), "values").is_empty());
    }

    #[test]
    fn die_seite_sagt_ob_es_mehr_gibt() {
        let mit_total = json!({ "total": 137, "isLast": false, "values": [] });
        assert_eq!(gesamt(&mit_total), Some(137));
        assert_eq!(ist_letzte(&mit_total), Some(false));
        assert!(mehr(&mit_total, 0, 25));
        assert!(!mehr(&mit_total, 0, 137));
        // Mit Startposition zählt die schon gelesene Menge.
        assert!(mehr(&mit_total, 130, 6));
        assert!(!mehr(&mit_total, 130, 7));

        let ohne_total = json!({ "isLast": true, "values": [] });
        assert_eq!(gesamt(&ohne_total), None);
        assert!(!mehr(&ohne_total, 0, 0));

        let mit_next = json!({ "next": "https://api.example.test/v2/testcases?startAt=25" });
        assert_eq!(
            folgeseite(&mit_next).as_deref(),
            Some("https://api.example.test/v2/testcases?startAt=25")
        );
        assert!(mehr(&mit_next, 0, 25));

        // Nur `next` zählt: `_links` kennt die API nicht.
        assert_eq!(
            folgeseite(&json!({ "_links": { "next": { "href": "https://x/y" } } })),
            None
        );
        assert_eq!(folgeseite(&json!({ "next": null })), None);
        assert_eq!(folgeseite(&json!({})), None);
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
}
