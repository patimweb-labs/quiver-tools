// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Text → Atlassian Document Format (ADF) und zurück (Aufgabe 13).
//!
//! Jira Cloud nimmt Beschreibungen und Kommentare nur als ADF an – ein
//! JSON-Baum aus Absätzen, Listen und Textknoten. Menschen und Agenten schreiben
//! aber Text. Dieses Modul übersetzt in beide Richtungen:
//!
//! - [`text_zu_adf`] baut aus Text ein ADF-Dokument: Absätze, Zeilenumbrüche
//!   (harter Umbruch), Aufzählungen, numerierte Listen und Codeblöcke
//!   (```` ``` ````). Mehr wird bewusst nicht gedeutet – kein Markdown. Ein
//!   leerer Listeneintrag oder ein leerer Codeblock wird weggelassen, denn
//!   `text`-Knoten ohne Inhalt sind in ADF ungültig.
//! - [`pruefen`] prüft einen Rumpf gegen die Knoten, die die ADF-Beschreibung
//!   nennt (`doc`, `paragraph`, `text`, `hardBreak`, Listen, `codeBlock` …).
//!   Unbekannte Knoten und leere `text`-Knoten sind ein Fehler, **bevor**
//!   etwas an Jira geht.
//! - [`adf_zu_text`] liest ein ADF-Dokument als Text. **Unbekannte Knoten
//!   verlieren keinen Text**: sie geben ihren `text`, ihr `attrs.text` oder den
//!   Text ihrer Kinder weiter.
//!
//! Alle Funktionen sind rein und ohne Netz – darum gut zu prüfen.

use serde_json::{json, Value};

/// Die Knoten, die die ADF-Beschreibung (Structure of the document) nennt.
///
/// Die OpenAPI-Spezifikation verweist für den Rumpf nur auf diese Beschreibung,
/// sie zählt die Knoten nicht selbst auf; darum steht die Liste hier und wird
/// beim Senden geprüft.
pub const KNOTEN: &[&str] = &[
    // Block-Knoten
    "doc",
    "paragraph",
    "heading",
    "blockquote",
    "codeBlock",
    "rule",
    "panel",
    "expand",
    "nestedExpand",
    "bulletList",
    "orderedList",
    "listItem",
    "taskList",
    "taskItem",
    "decisionList",
    "decisionItem",
    "table",
    "tableRow",
    "tableHeader",
    "tableCell",
    "caption",
    "mediaSingle",
    "mediaGroup",
    "layoutSection",
    "layoutColumn",
    "bodiedExtension",
    "multiBodiedExtension",
    "extension",
    "syncBlock",
    // Inline-Knoten
    "text",
    "hardBreak",
    "mention",
    "emoji",
    "date",
    "status",
    "inlineCard",
    "blockCard",
    "embedCard",
    "media",
    "mediaInline",
    "inlineExtension",
    "placeholder",
];

/// Ein Textabschnitt als ADF-Textknoten.
fn textknoten(text: &str) -> Value {
    json!({ "type": "text", "text": text })
}

/// Ein Absatz aus einer oder mehreren Zeilen; Zeilen werden harte Umbrüche.
fn absatz(zeilen: &[String]) -> Value {
    let mut inhalt = Vec::new();
    for (nummer, zeile) in zeilen.iter().enumerate() {
        if nummer > 0 {
            inhalt.push(json!({ "type": "hardBreak" }));
        }
        inhalt.push(textknoten(zeile));
    }
    json!({ "type": "paragraph", "content": inhalt })
}

/// Ein Listeneintrag mit einem Absatz.
fn listeneintrag(text: &str) -> Value {
    json!({
        "type": "listItem",
        "content": [ { "type": "paragraph", "content": [ textknoten(text) ] } ]
    })
}

/// Eine Liste (`bulletList` oder `orderedList`) aus den gesammelten Einträgen.
fn liste(eintraege: &[String], numeriert: bool) -> Value {
    let art = if numeriert {
        "orderedList"
    } else {
        "bulletList"
    };
    json!({
        "type": art,
        "attrs": { "order": 1 },
        "content": eintraege.iter().map(|eintrag| listeneintrag(eintrag)).collect::<Vec<_>>()
    })
}

/// Der Inhalt eines Codeblocks; die Sprache steht im `attrs.language`.
fn codeblock(sprache: Option<&str>, zeilen: &[String]) -> Value {
    let mut knoten = json!({
        "type": "codeBlock",
        "content": [ textknoten(&zeilen.join("\n")) ]
    });
    match sprache.map(str::trim).filter(|sprache| !sprache.is_empty()) {
        Some(sprache) => knoten["attrs"] = json!({ "language": sprache }),
        None => knoten["attrs"] = json!({}),
    }
    knoten
}

/// Ist die Zeile ein Aufzählungspunkt (`- ` oder `* `)?
fn aufzaehlung(zeile: &str) -> Option<String> {
    zeile
        .strip_prefix("- ")
        .or_else(|| zeile.strip_prefix("* "))
        .map(|rest| rest.trim().to_string())
}

/// Ist die Zeile ein numerierter Punkt (`1. `, `12) `)?
fn numerierung(zeile: &str) -> Option<String> {
    let ziffern = zeile
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    if ziffern.is_empty() {
        return None;
    }
    let rest = &zeile[ziffern.len()..];
    rest.strip_prefix(". ")
        .or_else(|| rest.strip_prefix(") "))
        .map(|rest| rest.trim().to_string())
}

/// Text in ein ADF-Dokument übersetzen.
///
/// ```text
/// Erster Absatz, zweite Zeile   → paragraph mit hardBreak
/// - Punkt                       → bulletList
/// 1. Schritt                    → orderedList
/// ```rust                       → codeBlock mit attrs.language
/// ```
pub fn text_zu_adf(text: &str) -> Value {
    let normalisiert = text.replace("\r\n", "\n").replace('\r', "\n");
    let zeilen: Vec<&str> = normalisiert.lines().collect();

    let mut bloecke: Vec<Value> = Vec::new();
    let mut absatz_zeilen: Vec<String> = Vec::new();
    let mut eintraege: Vec<String> = Vec::new();
    let mut numeriert = false;
    let mut code: Option<(Option<String>, Vec<String>)> = None;

    // Die gesammelten Zeilen eines Absatzes oder einer Liste abschließen.
    macro_rules! abschliessen {
        () => {
            if !absatz_zeilen.is_empty() {
                bloecke.push(absatz(&absatz_zeilen));
                absatz_zeilen.clear();
            }
            if !eintraege.is_empty() {
                bloecke.push(liste(&eintraege, numeriert));
                eintraege.clear();
            }
        };
    }

    for zeile in zeilen {
        // Innerhalb eines Codeblocks zählt jede Zeile wörtlich.
        if let Some((sprache, gesammelt)) = code.as_mut() {
            if zeile.trim_start().starts_with("```") {
                // Ein Codeblock ohne Inhalt wäre ein leerer `text`-Knoten.
                if gesammelt.iter().any(|zeile| !zeile.trim().is_empty()) {
                    bloecke.push(codeblock(sprache.as_deref(), gesammelt));
                }
                code = None;
            } else {
                gesammelt.push(zeile.to_string());
            }
            continue;
        }

        if let Some(sprache) = zeile.trim_start().strip_prefix("```") {
            abschliessen!();
            code = Some((Some(sprache.to_string()), Vec::new()));
            continue;
        }

        if zeile.trim().is_empty() {
            abschliessen!();
            continue;
        }

        if let Some(punkt) = aufzaehlung(zeile) {
            if !absatz_zeilen.is_empty() {
                abschliessen!();
            }
            // `- ` ohne Text ergibt keinen Eintrag (ein leerer `text`-Knoten
            // wäre in ADF ungültig).
            if punkt.is_empty() {
                abschliessen!();
                continue;
            }
            if numeriert && !eintraege.is_empty() {
                bloecke.push(liste(&eintraege, true));
                eintraege.clear();
            }
            numeriert = false;
            eintraege.push(punkt);
            continue;
        }

        if let Some(punkt) = numerierung(zeile) {
            if !absatz_zeilen.is_empty() {
                abschliessen!();
            }
            if punkt.is_empty() {
                abschliessen!();
                continue;
            }
            if !eintraege.is_empty() && !numeriert {
                bloecke.push(liste(&eintraege, false));
                eintraege.clear();
            }
            eintraege.push(punkt);
            numeriert = true;
            continue;
        }

        // Ein Codeblock, der nie geschlossen wurde, endet hier.
        if !eintraege.is_empty() {
            abschliessen!();
        }
        absatz_zeilen.push(zeile.trim().to_string());
    }

    if let Some((sprache, gesammelt)) = code {
        if !gesammelt.iter().all(|zeile| zeile.trim().is_empty()) {
            bloecke.push(codeblock(sprache.as_deref(), &gesammelt));
        }
    }
    abschliessen!();

    if bloecke.is_empty() {
        bloecke.push(json!({ "type": "paragraph", "content": [] }));
    }

    json!({ "type": "doc", "version": 1, "content": bloecke })
}

// ------------------------------------------------------------ ADF prüfen

/// Ein ADF-Rumpf, geprüft gegen die Knoten der ADF-Beschreibung.
///
/// Geprüft wird nur, was wir senden: der Wurzelknoten `doc` mit `version: 1`,
/// jeder Knoten ein Objekt mit **bekanntem** `type`, und ein `text`-Knoten ohne
/// Inhalt ist ungültig. Der Fehlertext nennt den Pfad im Baum.
pub fn pruefen(wert: &Value) -> Result<(), String> {
    let Some(objekt) = wert.as_object() else {
        return Err("Der Rumpf ist kein ADF-Dokument (Objekt erwartet).".to_string());
    };
    if feld(objekt, "type").as_deref() != Some("doc") {
        return Err("Der Rumpf ist kein ADF-Dokument (type = doc fehlt).".to_string());
    }
    if let Some(version) = feld(objekt, "version") {
        if version != "1" {
            return Err(format!("ADF kennt nur version 1, nicht {version}."));
        }
    }
    kinder_pruefen(wert, "doc")
}

/// Prüfen, ob ein Knoten vorkommen darf; rekursiv über die Kinder.
fn kinder_pruefen(wert: &Value, pfad: &str) -> Result<(), String> {
    let Some(objekt) = wert.as_object() else {
        return Err(format!("{pfad}: ein ADF-Knoten muss ein Objekt sein."));
    };
    let Some(art) = feld(objekt, "type") else {
        return Err(format!("{pfad}: dem Knoten fehlt `type`."));
    };
    if !KNOTEN.contains(&art.as_str()) {
        return Err(format!(
            "{pfad}: der ADF-Knoten „{art}“ ist unbekannt (erlaubt: {}).",
            KNOTEN.join(", ")
        ));
    }
    if art == "text" {
        let text = objekt.get("text").and_then(Value::as_str).unwrap_or("");
        if text.is_empty() {
            return Err(format!(
                "{pfad}: ein `text`-Knoten ohne Text ist in ADF ungültig."
            ));
        }
        return Ok(());
    }
    let Some(inhalt) = objekt.get("content") else {
        return Ok(());
    };
    let Some(kinder) = inhalt.as_array() else {
        return Err(format!("{pfad}: `content` muss eine Liste sein."));
    };
    for (nummer, kind) in kinder.iter().enumerate() {
        kinder_pruefen(kind, &format!("{pfad}.content.{nummer}"))?;
    }
    Ok(())
}

// ------------------------------------------------------------- ADF → Text

/// Ein ADF-Dokument (oder einen einzelnen Knoten) als Text lesen.
pub fn adf_zu_text(wert: &Value) -> String {
    let text = inhalt(wert);
    // Leerraum am Zeilenende weg, nie mehr als eine Leerzeile.
    let mut ergebnis = String::with_capacity(text.len());
    for zeile in text.lines() {
        if zeile.trim().is_empty() {
            if !ergebnis.ends_with("\n\n") && !ergebnis.is_empty() {
                ergebnis.push('\n');
            }
            continue;
        }
        ergebnis.push_str(zeile.trim_end());
        ergebnis.push('\n');
    }
    ergebnis.trim_end_matches('\n').to_string()
}

/// Der Text eines Knotens samt Kindern.
fn inhalt(wert: &Value) -> String {
    match wert {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Array(eintraege) => eintraege
            .iter()
            .map(inhalt)
            .filter(|teil| !teil.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n"),
        Value::Object(objekt) => knoten(objekt),
        andere => andere.to_string(),
    }
}

/// Der Text eines einzelnen ADF-Knotens.
fn knoten(objekt: &serde_json::Map<String, Value>) -> String {
    let art = feld(objekt, "type");
    match art.as_deref() {
        Some("text") => feld(objekt, "text").unwrap_or_default(),
        Some("hardBreak") => "\n".to_string(),
        // Knoten, deren Text in `attrs` steht.
        Some("mention") => feld(objekt, "attrs.text")
            .or_else(|| feld(objekt, "attrs.displayName"))
            .or_else(|| feld(objekt, "attrs.id").map(|id| format!("@{id}")))
            .unwrap_or_default(),
        Some("emoji") => feld(objekt, "attrs.text")
            .or_else(|| feld(objekt, "attrs.shortName"))
            .unwrap_or_default(),
        Some("status") => feld(objekt, "attrs.text").unwrap_or_default(),
        Some("inlineCard") | Some("blockCard") => feld(objekt, "attrs.url").unwrap_or_default(),
        Some("media") | Some("mediaInline") => feld(objekt, "attrs.alt")
            .or_else(|| feld(objekt, "attrs.url"))
            .map(|wert| format!("[Anhang: {wert}]"))
            .unwrap_or_else(|| "[Anhang]".to_string()),
        Some("mediaSingle") | Some("mediaGroup") => {
            let kinder = kinder(objekt, "\n");
            if kinder.is_empty() {
                "[Anhang]".to_string()
            } else {
                kinder
            }
        }
        Some("paragraph") => kinder_inline(objekt),
        Some("doc") => kinder_mit(objekt, "\n\n"),
        Some("heading") => {
            let stufe = feld(objekt, "attrs.level")
                .and_then(|wert| wert.parse::<usize>().ok())
                .unwrap_or(1)
                .clamp(1, 6);
            format!("{} {}", "#".repeat(stufe), kinder_inline(objekt))
        }
        Some("codeBlock") => {
            let sprache = feld(objekt, "attrs.language").unwrap_or_default();
            let code = kinder_mit(objekt, "\n");
            format!("```{sprache}\n{code}\n```")
        }
        Some("blockquote") => kinder_mit(objekt, "\n")
            .lines()
            .map(|zeile| format!("> {zeile}"))
            .collect::<Vec<_>>()
            .join("\n"),
        Some("rule") => "---".to_string(),
        Some("bulletList") => liste_text(objekt, false),
        Some("orderedList") => liste_text(objekt, true),
        Some("listItem") => kinder_mit(objekt, "\n"),
        Some("taskList") => liste_text(objekt, false),
        Some("taskItem") => {
            let haken = if feld(objekt, "attrs.state").as_deref() == Some("DONE") {
                "[x]"
            } else {
                "[ ]"
            };
            format!("- {haken} {}", kinder_mit(objekt, " "))
        }
        Some("table") => kinder_mit(objekt, "\n"),
        Some("tableRow") => kinder(objekt, " | "),
        Some("tableHeader") | Some("tableCell") => kinder_mit(objekt, " "),
        Some("panel") => {
            let art = feld(objekt, "attrs.panelType").unwrap_or_else(|| "info".to_string());
            format!("[{art}] {}", kinder_mit(objekt, "\n"))
        }
        Some("expand") | Some("nestedExpand") => kinder_mit(objekt, "\n\n"),
        // Unbekannt: keinen Text verlieren.
        _ => {
            if let Some(text) = feld(objekt, "text") {
                return text;
            }
            if let Some(text) = feld(objekt, "attrs.text") {
                return text;
            }
            let kinder = kinder_mit(objekt, "\n\n");
            if kinder.is_empty() {
                feld(objekt, "attrs.url").unwrap_or_default()
            } else {
                kinder
            }
        }
    }
}

/// Eine Liste als Text: je Eintrag eine Zeile mit Aufzählungszeichen.
fn liste_text(objekt: &serde_json::Map<String, Value>, numeriert: bool) -> String {
    let mut zeilen = Vec::new();
    for (nummer, eintrag) in kinder_liste(objekt).into_iter().enumerate() {
        let gelesen = inhalt(&eintrag);
        let text = gelesen.trim();
        if text.is_empty() {
            continue;
        }
        let zeichen = if numeriert {
            format!("{}.", nummer + 1)
        } else {
            "-".to_string()
        };
        for (stelle, zeile) in text.lines().enumerate() {
            if stelle == 0 {
                zeilen.push(format!("{zeichen} {zeile}"));
            } else {
                zeilen.push(format!("  {zeile}"));
            }
        }
    }
    zeilen.join("\n")
}

/// Die Kinder eines Knotens als Text.
fn kinder(objekt: &serde_json::Map<String, Value>, trenner: &str) -> String {
    kinder_liste(objekt)
        .into_iter()
        .map(|kind| inhalt(&kind))
        .filter(|teil| !teil.trim().is_empty())
        .collect::<Vec<_>>()
        .join(trenner)
}

/// Wie [`kinder`], aber ohne leere Teile zu entfernen (für Absätze).
fn kinder_mit(objekt: &serde_json::Map<String, Value>, trenner: &str) -> String {
    kinder_liste(objekt)
        .into_iter()
        .map(|kind| inhalt(&kind))
        .filter(|teil| !teil.is_empty())
        .collect::<Vec<_>>()
        .join(trenner)
}

/// Wie [`kinder`], aber für Absätze: Zeilenumbrüche bleiben erhalten.
fn kinder_inline(objekt: &serde_json::Map<String, Value>) -> String {
    kinder_liste(objekt)
        .into_iter()
        .map(|kind| inhalt(&kind))
        .collect::<Vec<_>>()
        .join("")
}

/// Die `content`-Liste eines Knotens.
fn kinder_liste(objekt: &serde_json::Map<String, Value>) -> Vec<Value> {
    objekt
        .get("content")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// Ein Wert unter einem Punktpfad in `attrs` oder am Knoten selbst.
fn feld(objekt: &serde_json::Map<String, Value>, pfad: &str) -> Option<String> {
    let (erst, rest) = pfad.split_once('.').unwrap_or((pfad, ""));
    let mut aktuell = objekt.get(erst)?;
    for teil in rest.split('.').filter(|teil| !teil.is_empty()) {
        aktuell = aktuell.get(teil)?;
    }
    match aktuell {
        Value::String(text) => Some(text.clone()),
        Value::Number(zahl) => Some(zahl.to_string()),
        Value::Bool(wahr) => Some(wahr.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absaetze_und_zeilenumbrueche_werden_zu_adf() {
        let adf = text_zu_adf("Erster Absatz\nzweite Zeile\n\nZweiter Absatz");
        assert_eq!(adf["type"], json!("doc"));
        assert_eq!(adf["version"], json!(1));
        let bloecke = adf["content"].as_array().unwrap();
        assert_eq!(bloecke.len(), 2);
        assert_eq!(bloecke[0]["type"], json!("paragraph"));
        assert_eq!(bloecke[0]["content"][0]["text"], json!("Erster Absatz"));
        assert_eq!(bloecke[0]["content"][1]["type"], json!("hardBreak"));
        assert_eq!(bloecke[0]["content"][2]["text"], json!("zweite Zeile"));
        assert_eq!(bloecke[1]["content"][0]["text"], json!("Zweiter Absatz"));
    }

    #[test]
    fn listen_werden_zu_bullet_und_ordered_list() {
        let adf = text_zu_adf("- eins\n- zwei\n\n1. erster\n2. zweiter");
        let bloecke = adf["content"].as_array().unwrap();
        assert_eq!(bloecke.len(), 2);
        assert_eq!(bloecke[0]["type"], json!("bulletList"));
        assert_eq!(bloecke[0]["content"].as_array().unwrap().len(), 2);
        assert_eq!(
            bloecke[0]["content"][1]["content"][0]["content"][0]["text"],
            json!("zwei")
        );
        assert_eq!(bloecke[1]["type"], json!("orderedList"));
        assert_eq!(bloecke[1]["content"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn ein_codeblock_behaelt_seine_zeilen_und_sprache() {
        let adf = text_zu_adf("Vorher\n```rust\nfn main() {}\n// x\n```\nNachher");
        let bloecke = adf["content"].as_array().unwrap();
        assert_eq!(bloecke.len(), 3);
        assert_eq!(bloecke[1]["type"], json!("codeBlock"));
        assert_eq!(bloecke[1]["attrs"]["language"], json!("rust"));
        assert_eq!(
            bloecke[1]["content"][0]["text"],
            json!("fn main() {}\n// x")
        );
        assert_eq!(bloecke[2]["content"][0]["text"], json!("Nachher"));

        // Ohne Sprache bleibt `attrs` leer, der Text steht.
        let adf = text_zu_adf("```\nnur text\n```");
        assert_eq!(adf["content"][0]["attrs"], json!({}));
        assert_eq!(adf["content"][0]["content"][0]["text"], json!("nur text"));
    }

    #[test]
    fn leerer_text_ergibt_einen_leeren_absatz() {
        let adf = text_zu_adf("   \n\n");
        assert_eq!(adf["content"].as_array().unwrap().len(), 1);
        assert_eq!(adf["content"][0]["type"], json!("paragraph"));
        assert_eq!(adf["content"][0]["content"], json!([]));
    }

    #[test]
    fn leere_eintraege_und_leere_codebloecke_verschwinden() {
        // Ein `- ` ohne Text würde sonst einen leeren `text`-Knoten ergeben.
        let adf = text_zu_adf("- \n- echter Eintrag\n\n```\n```");
        prinzip(&adf);
        assert_eq!(adf["content"].as_array().unwrap().len(), 1, "{adf}");
        assert_eq!(adf["content"][0]["type"], json!("bulletList"));
        assert_eq!(
            adf["content"][0]["content"].as_array().unwrap().len(),
            1,
            "{adf}"
        );
    }

    /// Jeder erzeugte Rumpf muss die ADF-Prüfung bestehen.
    fn prinzip(adf: &Value) {
        if let Err(fehler) = pruefen(adf) {
            panic!("{fehler}: {adf}");
        }
    }

    #[test]
    fn jeder_erzeugte_rumpf_besteht_die_adf_pruefung() {
        for text in [
            "",
            "   ",
            "Ein Absatz",
            "Zeile eins\nZeile zwei\n\nNeuer Absatz",
            "- eins\n- zwei",
            "1. erster\n2. zweiter",
            "```rust\nfn main() {}\n```",
            "```\n```",
            "Text\n- Punkt\n```\nCode\n```\nEnde",
            "# keine Überschrift, sondern Text",
            "Kunde's Login & „Anführungszeichen“",
        ] {
            prinzip(&text_zu_adf(text));
        }
    }

    #[test]
    fn die_adf_pruefung_lehnt_unbekannte_und_leere_knoten_ab() {
        // Ein leerer `text`-Knoten ist in ADF ungültig.
        let leer = json!({ "type": "doc", "version": 1, "content": [
            { "type": "paragraph", "content": [ { "type": "text", "text": "" } ] }
        ] });
        let fehler = pruefen(&leer).unwrap_err();
        assert!(fehler.contains("text"), "{fehler}");
        assert!(fehler.contains("content.0.content.0"), "{fehler}");

        // Ein unbekannter Knoten wird abgelehnt.
        let unbekannt = json!({ "type": "doc", "version": 1, "content": [
            { "type": "tabellenZauber", "content": [] }
        ] });
        assert!(pruefen(&unbekannt).unwrap_err().contains("tabellenZauber"));

        // Der Wurzelknoten muss `doc` sein, die Fassung 1 haben.
        assert!(pruefen(&json!({ "type": "paragraph" })).is_err());
        assert!(pruefen(&json!("nur Text")).is_err());
        assert!(pruefen(&json!({ "type": "doc", "version": 2, "content": [] })).is_err());
        assert!(pruefen(&json!({ "type": "doc", "version": 1, "content": {} })).is_err());
        assert!(pruefen(&json!({ "type": "doc", "version": 1, "content": [] })).is_ok());
    }

    #[test]
    fn adf_wird_wieder_text() {
        let adf =
            text_zu_adf("Erster Absatz\nzweite Zeile\n\n- eins\n- zwei\n\n```rust\nfn x() {}\n```");
        let text = adf_zu_text(&adf);
        assert_eq!(
            text,
            "Erster Absatz\nzweite Zeile\n\n- eins\n- zwei\n\n```rust\nfn x() {}\n```"
        );
    }

    #[test]
    fn unbekannte_knoten_verlieren_keinen_text() {
        let unbekannt = json!({
            "type": "voelligNeu",
            "content": [ { "type": "text", "text": "wichtig" } ]
        });
        assert_eq!(adf_zu_text(&unbekannt), "wichtig");

        let mit_attrs = json!({ "type": "neuerKnopf", "attrs": { "text": "Beschriftung" } });
        assert_eq!(adf_zu_text(&mit_attrs), "Beschriftung");

        let doc = json!({
            "type": "doc",
            "version": 1,
            "content": [
                { "type": "paragraph", "content": [
                    { "type": "text", "text": "Hallo " },
                    { "type": "mention", "attrs": { "id": "acc-1", "text": "@Alex" } },
                    { "type": "text", "text": " und " },
                    { "type": "status", "attrs": { "text": "Offen" } }
                ] },
                { "type": "unbekannt", "content": [
                    { "type": "paragraph", "content": [ { "type": "text", "text": "geht nicht verloren" } ] }
                ] }
            ]
        });
        let text = adf_zu_text(&doc);
        assert!(text.contains("Hallo @Alex und Offen"), "{text}");
        assert!(text.contains("geht nicht verloren"), "{text}");
    }

    #[test]
    fn ueberschriften_tabellen_und_zitate_bleiben_lesbar() {
        let doc = json!({
            "type": "doc",
            "version": 1,
            "content": [
                { "type": "heading", "attrs": { "level": 2 }, "content": [ { "type": "text", "text": "Titel" } ] },
                { "type": "table", "content": [
                    { "type": "tableRow", "content": [
                        { "type": "tableHeader", "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": "A" } ] } ] },
                        { "type": "tableCell", "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": "B" } ] } ] }
                    ] }
                ] },
                { "type": "blockquote", "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": "zitiert" } ] } ] }
            ]
        });
        let text = adf_zu_text(&doc);
        assert!(text.contains("## Titel"), "{text}");
        assert!(text.contains("A | B"), "{text}");
        assert!(text.contains("> zitiert"), "{text}");
    }

    #[test]
    fn ein_anhang_und_ein_aufgabeneintrag_bleiben_sichtbar() {
        let doc = json!({
            "type": "doc",
            "version": 1,
            "content": [
                { "type": "mediaSingle", "content": [
                    { "type": "media", "attrs": { "alt": "screenshot.png" } }
                ] },
                { "type": "taskList", "content": [
                    { "type": "taskItem", "attrs": { "state": "DONE" }, "content": [
                        { "type": "text", "text": "erledigt" }
                    ] }
                ] }
            ]
        });
        let text = adf_zu_text(&doc);
        assert!(text.contains("[Anhang: screenshot.png]"), "{text}");
        assert!(text.contains("- [x] erledigt"), "{text}");
    }

    #[test]
    fn kaputte_und_leere_werte_sind_leiser_text() {
        assert_eq!(adf_zu_text(&Value::Null), "");
        assert_eq!(adf_zu_text(&json!("nur ein Text")), "nur ein Text");
        assert_eq!(
            adf_zu_text(&json!({ "type": "paragraph", "content": [] })),
            ""
        );
        assert_eq!(
            adf_zu_text(&json!({ "type": "doc", "content": [ { "type": "rule" } ] })),
            "---"
        );
    }
}
