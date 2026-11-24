// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! OneNote (docs/aufgaben/06-graph-erweiterung.md): Notizbücher, Abschnitte,
//! Seiten. Seiteninhalt kommt als HTML (`GET …/pages/{id}/content`); die
//! Umwandlung in Text macht `quiver-m365::onenote`.

use serde::{Deserialize, Serialize};

/// `GET /me/onenote/notebooks`
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Notizbuch {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub web_url: Option<String>,
    #[serde(default)]
    pub created_date_time: Option<String>,
    #[serde(default)]
    pub last_modified_date_time: Option<String>,
}

/// `GET /me/onenote/sections`, `GET /me/onenote/notebooks/{id}/sections`
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Abschnitt {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub web_url: Option<String>,
    #[serde(default)]
    pub created_date_time: Option<String>,
    #[serde(default)]
    pub last_modified_date_time: Option<String>,
}

/// `GET /me/onenote/sections/{abschnitt}/pages`, `GET /me/onenote/pages/{id}`
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Seite {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub web_url: Option<String>,
    #[serde(default)]
    pub created_date_time: Option<String>,
    #[serde(default)]
    pub last_modified_date_time: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seite_und_abschnitt_lesen_namen_und_link() {
        let abschnitt: Abschnitt = serde_json::from_value(serde_json::json!({
            "id": "sec-1",
            "displayName": "Besprechungen",
            "webUrl": "https://example/onenote/sec-1",
        }))
        .expect("Modell passt");
        assert_eq!(abschnitt.display_name.as_deref(), Some("Besprechungen"));

        let seite: Seite = serde_json::from_value(serde_json::json!({
            "id": "page-1",
            "title": "Jour fixe 27.11.",
            "lastModifiedDateTime": "2026-11-27T09:00:00Z",
        }))
        .expect("Modell passt");
        assert_eq!(seite.title.as_deref(), Some("Jour fixe 27.11."));
        assert!(seite.web_url.is_none());
    }
}
