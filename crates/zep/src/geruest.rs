// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Das Gerüst einer Seite – ohne Werte.
//!
//! Bleibt die Routine an einer Seite hängen, die sie nicht kennt (etwa eine
//! Zustimmungsseite, die per JavaScript aufgebaut wird), braucht der Entwickler
//! ihren **Aufbau**, nicht ihren Inhalt. Dieses Modul schreibt ihn auf:
//!
//! - Tags mit **Attributnamen**; Werte nur für `id`, `class`, `type`, `method`,
//!   `action`, `href` und `src` (ohne Query), alles andere als `…`
//! - eingebettetes JSON (`data-page='{…}'`, `<script type="application/json">`)
//!   als Baum aus **Schlüsseln und Typen**, nie Werten
//! - sichtbaren Text nur in Knöpfen, Links und Überschriften, gekürzt
//! - Inline-Skripte nur als Länge und genannte `window.*`-Namen
//!
//! So landet weder ein Token noch ein Name im Bericht.

use crate::html::{entschluesseln, zerlegen};

/// Attribute, deren Wert unbedenklich ist und den Aufbau erklärt.
const OFFENE_ATTRIBUTE: &[&str] = &[
    "id", "class", "type", "method", "action", "href", "src", "role", "name",
];

/// Elemente, deren Text gezeigt wird (Knopfbeschriftungen und Überschriften).
const TEXT_ELEMENTE: &[&str] = &["button", "a", "h1", "h2", "h3", "label", "title"];

/// Elemente ohne Inhalt.
const LEER: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];

/// Höchstens so viele Zeilen – ein Gerüst, kein Abbild.
const MAX_ZEILEN: usize = 600;

/// Das Gerüst einer Seite als Text.
pub fn geruest(html: &str) -> String {
    let mut zeilen: Vec<String> = Vec::new();
    let mut stapel: Vec<String> = Vec::new();
    let mut rest = html;

    while let Some(anfang) = rest.find('<') {
        let text = &rest[..anfang];
        text_zeile(text, &stapel, &mut zeilen);
        rest = &rest[anfang..];

        if let Some(nach) = rest.strip_prefix("<!--") {
            rest = match nach.find("-->") {
                Some(ende) => &nach[ende + 3..],
                None => "",
            };
            continue;
        }
        let Some(ende) = tag_ende(rest) else {
            break;
        };
        let roh = &rest[1..ende];
        rest = &rest[ende + 1..];
        if roh.starts_with('!') || roh.starts_with('?') {
            continue;
        }
        let (name, attribute) = zerlegen(roh);
        if name.is_empty() {
            continue;
        }

        if let Some(zu) = name.strip_prefix('/') {
            if let Some(stelle) = stapel.iter().rposition(|offen| offen == zu) {
                stapel.truncate(stelle);
            }
            continue;
        }

        let tiefe = stapel.len();
        zeilen.push(format!(
            "{}<{}{}>",
            "  ".repeat(tiefe),
            name,
            attribute_text(&attribute)
        ));
        for (attr, wert) in &attribute {
            if let Some(json) = json_von(wert) {
                zeilen.push(format!("{}  [{attr} als JSON]", "  ".repeat(tiefe)));
                json_baum(&json, tiefe + 2, &mut zeilen);
            }
        }

        if name == "script" || name == "style" {
            let schluss = format!("</{name}");
            let ende = finde_ohne_gross(rest, &schluss).unwrap_or(rest.len());
            let inhalt = &rest[..ende];
            if name == "script" {
                skript_zeile(inhalt, &attribute, tiefe + 1, &mut zeilen);
            }
            rest = &rest[ende..];
            continue;
        }

        let selbst_schliessend = roh.trim_end().ends_with('/');
        if !LEER.contains(&name.as_str()) && !selbst_schliessend {
            stapel.push(name);
        }
        if zeilen.len() > MAX_ZEILEN {
            zeilen.push("… (gekürzt)".to_string());
            break;
        }
    }
    zeilen.join("\n")
}

/// Wo endet ein Tag? Ein `>` in Anführungszeichen zählt nicht.
fn tag_ende(text: &str) -> Option<usize> {
    let mut quote: Option<u8> = None;
    for (stelle, byte) in text.bytes().enumerate().skip(1) {
        match (quote, byte) {
            (None, b'"' | b'\'') => quote = Some(byte),
            (Some(offen), _) if offen == byte => quote = None,
            (None, b'>') => return Some(stelle),
            _ => {}
        }
    }
    None
}

fn finde_ohne_gross(text: &str, suche: &str) -> Option<usize> {
    text.to_ascii_lowercase().find(suche)
}

fn attribute_text(attribute: &[(String, String)]) -> String {
    let mut teile = Vec::new();
    for (name, wert) in attribute {
        if OFFENE_ATTRIBUTE.contains(&name.as_str()) && !wert.is_empty() {
            teile.push(format!(" {name}=\"{}\"", offen(wert)));
        } else if wert.is_empty() {
            teile.push(format!(" {name}"));
        } else {
            teile.push(format!(" {name}=…"));
        }
    }
    teile.concat()
}

/// Ein offener Wert: Adressen ohne Query und Fragment, alles gekürzt.
fn offen(wert: &str) -> String {
    let ohne_query = wert.split(['?', '#']).next().unwrap_or("");
    kuerzen(ohne_query, 80)
}

fn kuerzen(text: &str, laenge: usize) -> String {
    let mut ergebnis: String = text.chars().take(laenge).collect();
    if text.chars().count() > laenge {
        ergebnis.push('…');
    }
    ergebnis
}

fn text_zeile(text: &str, stapel: &[String], zeilen: &mut Vec<String>) {
    let Some(oben) = stapel.last() else {
        return;
    };
    let sauber = entschluesseln(
        text.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .as_str(),
    );
    if sauber.is_empty() {
        return;
    }
    let tiefe = stapel.len();
    if TEXT_ELEMENTE.contains(&oben.as_str()) || stapel.iter().any(|tag| tag == "button") {
        zeilen.push(format!("{}„{}“", "  ".repeat(tiefe), kuerzen(&sauber, 40)));
    } else {
        zeilen.push(format!(
            "{}(Text, {} Zeichen)",
            "  ".repeat(tiefe),
            sauber.chars().count()
        ));
    }
}

fn skript_zeile(
    inhalt: &str,
    attribute: &[(String, String)],
    tiefe: usize,
    zeilen: &mut Vec<String>,
) {
    let einrueckung = "  ".repeat(tiefe);
    let art = attribute
        .iter()
        .find(|(name, _)| name == "type")
        .map(|(_, wert)| wert.to_ascii_lowercase())
        .unwrap_or_default();
    if art.contains("json") {
        if let Some(json) = json_von(inhalt) {
            zeilen.push(format!("{einrueckung}[Skript als JSON]"));
            json_baum(&json, tiefe + 1, zeilen);
            return;
        }
    }
    let inhalt = inhalt.trim();
    if inhalt.is_empty() {
        return;
    }
    let mut namen: Vec<String> = Vec::new();
    let mut suche = inhalt;
    while let Some(stelle) = suche.find("window.") {
        let nach = &suche[stelle + 7..];
        let name: String = nach
            .chars()
            .take_while(|zeichen| {
                zeichen.is_ascii_alphanumeric() || *zeichen == '_' || *zeichen == '$'
            })
            .collect();
        if !name.is_empty() && !namen.contains(&name) && namen.len() < 20 {
            namen.push(name);
        }
        suche = nach;
    }
    zeilen.push(format!(
        "{einrueckung}(Skript, {} Zeichen{})",
        inhalt.len(),
        if namen.is_empty() {
            String::new()
        } else {
            format!("; window.{}", namen.join(", window."))
        }
    ));
}

fn json_von(text: &str) -> Option<serde_json::Value> {
    let text = text.trim();
    if !(text.starts_with('{') || text.starts_with('[')) {
        return None;
    }
    serde_json::from_str(text)
        .or_else(|_| serde_json::from_str(&entschluesseln(text)))
        .ok()
}

/// Schlüssel und Typen eines JSON-Werts, nie die Werte selbst.
fn json_baum(wert: &serde_json::Value, tiefe: usize, zeilen: &mut Vec<String>) {
    use serde_json::Value;
    if tiefe > 14 || zeilen.len() > MAX_ZEILEN {
        return;
    }
    let einrueckung = "  ".repeat(tiefe);
    match wert {
        Value::Object(felder) => {
            for (schluessel, inhalt) in felder {
                zeilen.push(format!("{einrueckung}{schluessel}: {}", typ(inhalt)));
                if inhalt.is_object() || inhalt.is_array() {
                    json_baum(inhalt, tiefe + 1, zeilen);
                }
            }
        }
        Value::Array(liste) => {
            if let Some(erstes) = liste.first() {
                zeilen.push(format!("{einrueckung}[0]: {}", typ(erstes)));
                if erstes.is_object() || erstes.is_array() {
                    json_baum(erstes, tiefe + 1, zeilen);
                }
            }
        }
        _ => {}
    }
}

fn typ(wert: &serde_json::Value) -> String {
    use serde_json::Value;
    match wert {
        Value::Null => "null".to_string(),
        Value::Bool(_) => "bool".to_string(),
        Value::Number(_) => "Zahl".to_string(),
        Value::String(text) => format!("Text ({} Zeichen)", text.chars().count()),
        Value::Array(liste) => format!("Liste ({})", liste.len()),
        Value::Object(felder) => format!("Objekt ({})", felder.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GEHEIM: &str = "geheimes-token-4711";

    #[test]
    fn das_geruest_zeigt_aufbau_aber_keine_werte() {
        let html = format!(
            r#"<html><head><title>Zugriff erlauben</title>
            <script>window.__ZEP__ = {{"t":"{GEHEIM}"}};</script>
            <script src="/app.js?v=1"></script></head>
            <body><div id="app" data-page='{{"component":"OAuth/Authorize","props":{{"authToken":"{GEHEIM}","client":{{"name":"quiver-zep"}},"scopes":["mcp:use"]}}}}'></div>
            <input type="hidden" name="_token" value="{GEHEIM}">
            <button type="submit" class="btn">Zulassen</button>
            <p>Hallo Ada Lovelace</p>
            </body></html>"#
        );
        let text = geruest(&html);
        assert!(!text.contains(GEHEIM), "{text}");
        assert!(!text.contains("Ada"), "{text}");
        assert!(text.contains("component: Text"), "{text}");
        assert!(text.contains("authToken: Text (19 Zeichen)"), "{text}");
        assert!(text.contains("scopes: Liste (1)"), "{text}");
        assert!(text.contains("„Zulassen“"), "{text}");
        assert!(text.contains("„Zugriff erlauben“"), "{text}");
        assert!(text.contains("window.__ZEP__"), "{text}");
        assert!(text.contains("src=\"/app.js\""), "{text}");
        assert!(text.contains("name=\"_token\" value=…"), "{text}");
        assert!(text.contains("(Text, 18 Zeichen)"), "{text}");
    }

    #[test]
    fn ein_kaputtes_dokument_bricht_nicht() {
        let text = geruest("<div><a href='x?y=1'>Weiter</a><span");
        assert!(text.contains("„Weiter“"), "{text}");
        assert!(text.contains("href=\"x\""), "{text}");
    }
}
