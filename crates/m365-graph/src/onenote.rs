// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! OneNote (docs/aufgaben/06-graph-erweiterung.md): Notizbücher, Abschnitte,
//! Seiten. Seiteninhalt kommt als HTML (`GET …/pages/{id}/content`); die
//! Umwandlung in Text macht `quiver-m365::onenote`.

use serde::{Deserialize, Serialize};

/// Der Link eines Notizbuchs, Abschnitts oder einer Seite (Aufgabe 07/08:
/// `webUrl` gibt es hier **nicht**, der Link steht unter
/// `links.oneNoteWebUrl.href`).
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OneNoteLink {
    #[serde(default)]
    pub href: Option<String>,
}

impl OneNoteLink {
    /// Der Link, wenn Graph einen geliefert hat.
    pub fn href(&self) -> Option<&str> {
        self.href
            .as_deref()
            .map(str::trim)
            .filter(|h| !h.is_empty())
    }
}

/// `links` eines Notizbuchs oder Abschnitts.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OneNoteLinks {
    #[serde(default)]
    pub one_note_web_url: Option<OneNoteLink>,
}

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
    pub links: Option<OneNoteLinks>,
    #[serde(default)]
    pub created_date_time: Option<String>,
    #[serde(default)]
    pub last_modified_date_time: Option<String>,
}

impl Notizbuch {
    /// Der Web-Link – aus `links.oneNoteWebUrl.href` (Aufgabe 07).
    pub fn link(&self) -> Option<&str> {
        self.links
            .as_ref()
            .and_then(|links| links.one_note_web_url.as_ref())
            .and_then(OneNoteLink::href)
            .or(self.web_url.as_deref())
    }
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
    pub links: Option<OneNoteLinks>,
    #[serde(default)]
    pub created_date_time: Option<String>,
    #[serde(default)]
    pub last_modified_date_time: Option<String>,
}

impl Abschnitt {
    /// Der Web-Link – aus `links.oneNoteWebUrl.href` (Aufgabe 07).
    pub fn link(&self) -> Option<&str> {
        self.links
            .as_ref()
            .and_then(|links| links.one_note_web_url.as_ref())
            .and_then(OneNoteLink::href)
            .or(self.web_url.as_deref())
    }
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
    pub links: Option<OneNoteLinks>,
    #[serde(default)]
    pub created_date_time: Option<String>,
    #[serde(default)]
    pub last_modified_date_time: Option<String>,
}

impl Seite {
    /// Der Web-Link – aus `links.oneNoteWebUrl.href` (Aufgabe 08: auch Seiten
    /// kennen kein `webUrl`).
    pub fn link(&self) -> Option<&str> {
        self.links
            .as_ref()
            .and_then(|links| links.one_note_web_url.as_ref())
            .and_then(OneNoteLink::href)
            .or(self.web_url.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seite_und_abschnitt_lesen_namen_und_link() {
        let abschnitt: Abschnitt = serde_json::from_value(serde_json::json!({
            "id": "sec-1",
            "displayName": "Besprechungen",
            "links": {"oneNoteWebUrl": {"href": "https://example/onenote/sec-1"}},
        }))
        .expect("Modell passt");
        assert_eq!(abschnitt.display_name.as_deref(), Some("Besprechungen"));
        assert_eq!(abschnitt.link(), Some("https://example/onenote/sec-1"));

        let notizbuch: Notizbuch = serde_json::from_value(serde_json::json!({
            "id": "nb-1",
            "displayName": "Arbeit",
            "links": {"oneNoteWebUrl": {"href": "https://example/onenote/nb-1"}},
        }))
        .expect("Modell passt");
        assert_eq!(notizbuch.link(), Some("https://example/onenote/nb-1"));

        // Ohne `links` und ohne `webUrl` bleibt der Link leer – kein Fehler.
        let ohne: Notizbuch =
            serde_json::from_value(serde_json::json!({"id": "nb-2"})).expect("Modell passt");
        assert!(ohne.link().is_none());

        // Auch Seiten kennen kein `webUrl` (Aufgabe 08) – der Link steht in `links`.
        let seite: Seite = serde_json::from_value(serde_json::json!({
            "id": "page-1",
            "title": "Jour fixe 27.11.",
            "links": {"oneNoteWebUrl": {"href": "https://example/onenote/page-1"}},
            "lastModifiedDateTime": "2026-11-27T09:00:00Z",
        }))
        .expect("Modell passt");
        assert_eq!(seite.title.as_deref(), Some("Jour fixe 27.11."));
        assert_eq!(seite.link(), Some("https://example/onenote/page-1"));
        assert!(seite.web_url.is_none());
    }
}
