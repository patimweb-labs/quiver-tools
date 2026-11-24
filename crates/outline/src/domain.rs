// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Daten, die Outline liefert (SPEC §6).
//!
//! Alles hier ist nur Form: `serde` beschreibt, was ankommt, und die
//! Ausgabe-Funktionen in [`crate::format`] machen daraus Text. Kein Feld hält
//! einen Zustand, den es auf der Leitung nicht gibt.

use serde::Deserialize;

/// Ein Benutzer, wie Outline ihn an einer Seite oder einem Kommentar nennt.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Nutzer {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
}

/// Der Arbeitsbereich (die Instanz), in der der Schlüssel gilt.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Arbeitsbereich {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

/// Antwort von `auth.info`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Info {
    #[serde(default)]
    pub user: Option<Nutzer>,
    #[serde(default)]
    pub team: Option<Arbeitsbereich>,
}

/// Eine Sammlung – die oberste Ebene eines Wikis.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Sammlung {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, rename = "description")]
    pub beschreibung: Option<String>,
}

impl Sammlung {
    /// Der Name, oder ein Platzhalter, falls Outline keinen liefert.
    pub fn titel(&self) -> &str {
        if self.name.is_empty() {
            "Ohne Namen"
        } else {
            &self.name
        }
    }
}

/// Ein Dokument. Je nach Endpunkt mit oder ohne `text`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Dokument {
    #[serde(default)]
    pub id: String,
    #[serde(default, rename = "urlId")]
    pub url_id: Option<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default, rename = "collectionId")]
    pub sammlung_id: Option<String>,
    #[serde(default, rename = "parentDocumentId")]
    pub eltern_id: Option<String>,
    #[serde(default)]
    pub template: bool,
    #[serde(default, rename = "createdAt")]
    pub erstellt_am: Option<String>,
    #[serde(default, rename = "updatedAt")]
    pub geaendert_am: Option<String>,
    #[serde(default, rename = "archivedAt")]
    pub archiviert_am: Option<String>,
    #[serde(default, rename = "deletedAt")]
    pub geloescht_am: Option<String>,
    #[serde(default, rename = "publishedAt")]
    pub veroeffentlicht_am: Option<String>,
    #[serde(default, rename = "updatedBy")]
    pub geaendert_von: Option<Nutzer>,
    /// Markdown-Rumpf; leer, wo Outline ihn nicht mitschickt.
    #[serde(default)]
    pub text: String,
}

impl Dokument {
    /// Der Titel, oder ein Platzhalter.
    pub fn titel(&self) -> &str {
        if self.title.is_empty() {
            "Ohne Titel"
        } else {
            &self.title
        }
    }

    /// Die Kennung, unter der das Dokument sicher zu lesen ist.
    pub fn kennung(&self) -> &str {
        if self.id.is_empty() {
            self.url_id.as_deref().unwrap_or("")
        } else {
            &self.id
        }
    }

    /// Ein Zustand in einem Wort: gelöscht, archiviert, Entwurf – oder keiner.
    pub fn zustand(&self) -> Option<&'static str> {
        if self.geloescht_am.is_some() {
            Some("gelöscht")
        } else if self.archiviert_am.is_some() {
            Some("archiviert")
        } else if self.veroeffentlicht_am.is_none() {
            Some("Entwurf")
        } else {
            None
        }
    }
}

/// Ein Treffer aus `documents.search`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct Treffer {
    #[serde(default)]
    pub ranking: Option<f64>,
    #[serde(default)]
    pub context: String,
    #[serde(default)]
    pub document: Dokument,
}

/// Ein Knoten des Dokumentbaums (`collections.documents`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Knoten {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub children: Vec<Knoten>,
}

impl Knoten {
    /// Der Titel, oder ein Platzhalter.
    pub fn titel(&self) -> &str {
        if self.title.is_empty() {
            "Ohne Titel"
        } else {
            &self.title
        }
    }

    /// Diesen Knoten und alle Nachfahren.
    pub fn alle(&self) -> Vec<&Knoten> {
        let mut raus = vec![self];
        for kind in &self.children {
            raus.extend(kind.alle());
        }
        raus
    }
}

/// Antwort von `documents.move`: die betroffenen Dokumente.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Verschoben {
    #[serde(default)]
    pub documents: Vec<Dokument>,
}

/// Alle Knoten eines Baums in Lesereihenfolge (Tiefensuche, Eltern zuerst).
pub fn baum_knoten(wurzeln: &[Knoten]) -> Vec<&Knoten> {
    let mut raus = Vec::new();
    for wurzel in wurzeln {
        raus.extend(wurzel.alle());
    }
    raus
}

/// Ein Kommentar. `data` ist Outlines ProseMirror-Dokument; `text` fehlt je
/// nach Ausbaustufe des Servers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Kommentar {
    #[serde(default)]
    pub id: String,
    #[serde(default, rename = "documentId")]
    pub dokument_id: Option<String>,
    #[serde(default, rename = "parentCommentId")]
    pub eltern_id: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
    #[serde(default, rename = "createdAt")]
    pub erstellt_am: Option<String>,
    #[serde(default, rename = "resolvedAt")]
    pub erledigt_am: Option<String>,
    #[serde(default, rename = "createdBy")]
    pub erstellt_von: Option<Nutzer>,
}

impl Kommentar {
    /// Der Text des Kommentars als Prosa.
    pub fn inhalt(&self) -> String {
        match self.text.as_deref() {
            Some(text) if !text.is_empty() => text.to_string(),
            _ => reiner_text(self.data.as_ref()).trim().to_string(),
        }
    }
}

/// ProseMirror zu Prosa: Textknoten aneinander, Absätze mit Zeilenumbruch.
///
/// Ohne diesen Schritt läse der Agent einen Knotenbaum statt eines Kommentars.
pub fn reiner_text(knoten: Option<&serde_json::Value>) -> String {
    match knoten {
        None | Some(serde_json::Value::Null) => String::new(),
        Some(serde_json::Value::String(text)) => text.clone(),
        Some(serde_json::Value::Object(karte)) => {
            if karte.get("type").and_then(|art| art.as_str()) == Some("text") {
                return karte
                    .get("text")
                    .and_then(|text| text.as_str())
                    .unwrap_or_default()
                    .to_string();
            }
            let kinder: Vec<String> = karte
                .get("content")
                .and_then(|inhalt| inhalt.as_array())
                .map(|kinder| {
                    kinder
                        .iter()
                        .map(|kind| reiner_text(Some(kind)))
                        .filter(|teil| !teil.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            let zusammen = kinder.concat();
            if ist_block(karte.get("type").and_then(|art| art.as_str())) {
                format!("{zusammen}\n")
            } else {
                zusammen
            }
        }
        Some(_) => String::new(),
    }
}

fn ist_block(art: Option<&str>) -> bool {
    matches!(
        art,
        Some(
            "paragraph"
                | "heading"
                | "blockquote"
                | "code_block"
                | "code_fence"
                | "list_item"
                | "checkbox_item"
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn dokument_kennt_seinen_zustand() {
        let mut dokument = Dokument::default();
        assert_eq!(dokument.zustand(), Some("Entwurf"));
        dokument.veroeffentlicht_am = Some("2026-01-01T00:00:00Z".to_string());
        assert_eq!(dokument.zustand(), None);
        dokument.archiviert_am = Some("2026-02-01T00:00:00Z".to_string());
        assert_eq!(dokument.zustand(), Some("archiviert"));
        dokument.geloescht_am = Some("2026-03-01T00:00:00Z".to_string());
        assert_eq!(dokument.zustand(), Some("gelöscht"));
    }

    #[test]
    fn titel_und_kennung_haben_einen_platzhalter() {
        let dokument = Dokument::default();
        assert_eq!(dokument.titel(), "Ohne Titel");
        assert_eq!(dokument.kennung(), "");

        let dokument = Dokument {
            url_id: Some("abc".to_string()),
            title: "Titel".to_string(),
            ..Dokument::default()
        };
        assert_eq!(dokument.titel(), "Titel");
        assert_eq!(dokument.kennung(), "abc");
    }

    #[test]
    fn outline_felder_kommen_in_camel_case_an() {
        let dokument: Dokument = serde_json::from_value(json!({
            "id": "d1",
            "urlId": "slug",
            "title": "Titel",
            "collectionId": "c1",
            "parentDocumentId": "d0",
            "updatedAt": "2026-01-01T10:11:12.000Z",
            "updatedBy": { "name": "Ada" },
            "template": true
        }))
        .unwrap();
        assert_eq!(dokument.sammlung_id.as_deref(), Some("c1"));
        assert_eq!(dokument.eltern_id.as_deref(), Some("d0"));
        assert_eq!(
            dokument.geaendert_am.as_deref(),
            Some("2026-01-01T10:11:12.000Z")
        );
        assert_eq!(
            dokument.geaendert_von.and_then(|wer| wer.name).as_deref(),
            Some("Ada")
        );
        assert!(dokument.template);
    }

    #[test]
    fn baum_knoten_geht_tiefen_zuerst() {
        let baum: Vec<Knoten> = serde_json::from_value(json!([
            { "id": "a", "title": "A", "children": [{ "id": "a1", "title": "A1", "children": [] }] },
            { "id": "b", "title": "B", "children": [] }
        ]))
        .unwrap();
        let ids: Vec<&str> = baum_knoten(&baum).iter().map(|k| k.id.as_str()).collect();
        assert_eq!(ids, ["a", "a1", "b"]);
    }

    #[test]
    fn kommentar_text_gewinnt_sonst_prosemirror() {
        let mit_text: Kommentar = serde_json::from_value(json!({
            "id": "k1", "text": "Kurz", "data": { "type": "doc" }
        }))
        .unwrap();
        assert_eq!(mit_text.inhalt(), "Kurz");

        let ohne_text: Kommentar = serde_json::from_value(json!({
            "id": "k2",
            "data": {
                "type": "doc",
                "content": [
                    { "type": "paragraph", "content": [{ "type": "text", "text": "Hallo" }] },
                    { "type": "paragraph", "content": [{ "type": "text", "text": "Welt" }] }
                ]
            }
        }))
        .unwrap();
        assert_eq!(ohne_text.inhalt(), "Hallo\nWelt");
    }

    #[test]
    fn reiner_text_kommt_mit_leeren_werten_zurecht() {
        assert_eq!(reiner_text(None), "");
        assert_eq!(reiner_text(Some(&json!(null))), "");
        assert_eq!(reiner_text(Some(&json!("direkt"))), "direkt");
        assert_eq!(reiner_text(Some(&json!({ "content": [] }))), "");
    }
}
