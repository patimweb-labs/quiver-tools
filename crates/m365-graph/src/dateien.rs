// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! SharePoint und OneDrive (docs/aufgaben/06-graph-erweiterung.md): Sites,
//! Bibliotheken (Drives), Dateien, Ordner und Listen. Nur die Felder, die die
//! Tools zurückgeben – `$select` setzt das Tool passend.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::modelle::Eltern;

/// `GET /sites?search=…` – eine SharePoint-Site.
///
/// Die Id hat die Form `hostname,siteId,webId`; sie ist der einzige stabile
/// Zugang, deshalb nimmt jedes Site-Tool auch den Namen und löst ihn hierüber auf.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Website {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub web_url: Option<String>,
}

/// `GET /sites/{site}/drives` bzw. `GET /me/drive` – eine Bibliothek.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Laufwerk {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// `documentLibrary`, `business`, `personal`, …
    #[serde(default)]
    pub drive_type: Option<String>,
    #[serde(default)]
    pub web_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Ordner {
    #[serde(default)]
    pub child_count: Option<u64>,
}

/// `{"file": {"mimeType": …}}` an einem Laufwerkseintrag (nur bei Dateien).
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DateiArt {
    #[serde(default)]
    pub mime_type: Option<String>,
}

/// Ein Eintrag eines Laufwerks: Datei **oder** Ordner
/// (`GET /drives/{drive}/root/children`, `…/items/{id}/children`).
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LaufwerkEintrag {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub web_url: Option<String>,
    #[serde(default)]
    pub created_date_time: Option<String>,
    #[serde(default)]
    pub last_modified_date_time: Option<String>,
    #[serde(default)]
    pub folder: Option<Ordner>,
    #[serde(default)]
    pub file: Option<DateiArt>,
    #[serde(default)]
    pub parent_reference: Option<Eltern>,
}

impl LaufwerkEintrag {
    /// Ist das ein Ordner (und keine Datei)?
    pub fn ist_ordner(&self) -> bool {
        self.folder.is_some()
    }
}

/// `{"list": {"template": …}}` an einer SharePoint-Liste.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ListenArt {
    #[serde(default)]
    pub template: Option<String>,
}

/// `GET /sites/{site}/lists` – eine SharePoint-Liste.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Liste {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub web_url: Option<String>,
    #[serde(default)]
    pub list: Option<ListenArt>,
}

/// `GET /sites/{site}/lists/{liste}/items?$expand=fields` – ein Listeneintrag.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Listeneintrag {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub web_url: Option<String>,
    #[serde(default)]
    pub created_date_time: Option<String>,
    #[serde(default)]
    pub last_modified_date_time: Option<String>,
    /// Die Spaltenwerte der Liste – roh, weil jede Liste andere Spalten hat.
    #[serde(default)]
    pub fields: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn site_und_laufwerk_lesen_nur_was_da_ist() {
        let site: Website = serde_json::from_value(serde_json::json!({
            "id": "contoso.sharepoint.com,1111,aaaa",
            "displayName": "Projekt Nord",
            "name": "Projekt-Nord",
            "webUrl": "https://contoso.sharepoint.com/sites/projekt-nord",
        }))
        .expect("Modell passt");
        assert_eq!(site.display_name.as_deref(), Some("Projekt Nord"));

        let laufwerk: Laufwerk = serde_json::from_value(serde_json::json!({
            "id": "drive-doku",
            "name": "Dokumente",
            "driveType": "documentLibrary",
        }))
        .expect("Modell passt");
        assert_eq!(laufwerk.drive_type.as_deref(), Some("documentLibrary"));
        assert!(laufwerk.web_url.is_none());
    }

    #[test]
    fn eintrag_unterscheidet_datei_und_ordner() {
        let ordner: LaufwerkEintrag = serde_json::from_value(serde_json::json!({
            "id": "o1",
            "name": "Projekte",
            "folder": {"childCount": 3},
        }))
        .expect("Modell passt");
        assert!(ordner.ist_ordner());
        assert_eq!(ordner.folder.and_then(|f| f.child_count), Some(3));

        let datei: LaufwerkEintrag = serde_json::from_value(serde_json::json!({
            "id": "d1",
            "name": "notiz.md",
            "size": 12,
            "file": {"mimeType": "text/markdown"},
        }))
        .expect("Modell passt");
        assert!(!datei.ist_ordner());
        assert_eq!(datei.size, Some(12));
    }

    #[test]
    fn listeneintrag_traegt_seine_spalten_roh() {
        let eintrag: Listeneintrag = serde_json::from_value(serde_json::json!({
            "id": "1",
            "fields": {"Titel": "Erster", "Status": "offen"},
        }))
        .expect("Modell passt");
        assert_eq!(eintrag.fields.expect("fields")["Titel"], "Erster");
    }
}
