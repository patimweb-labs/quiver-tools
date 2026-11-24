// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Graph-Modelle, die die Tools zurückgeben (SPEC.md §4). Nur die Felder,
//! die auch in einer Antwort stehen – `$select` setzt das Tool passend.

use serde::{Deserialize, Serialize};

/// Eine Liste von Graph: `{"value": [...]}`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(bound(deserialize = "T: serde::de::Deserialize<'de>"))]
pub struct Sammlung<T> {
    #[serde(default)]
    pub value: Vec<T>,
    /// Nächste Seite (`@odata.nextLink`), wenn Graph mehr hat.
    #[serde(default, rename = "@odata.nextLink")]
    pub next_link: Option<String>,
}

/// `GET /me`
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Profil {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub mail: Option<String>,
    #[serde(default)]
    pub user_principal_name: Option<String>,
    #[serde(default)]
    pub department: Option<String>,
}

/// `{"emailAddress": {"name": ..., "address": ...}}`
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Empfaenger {
    #[serde(default)]
    pub email_address: Option<Adresse>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Adresse {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub address: Option<String>,
}

/// Der Mail-Text: `{"contentType": "text", "content": ...}`
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Koerper {
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
}

/// `GET /me/messages` bzw. `GET /me/messages/{id}`
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Nachricht {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub from: Option<Empfaenger>,
    #[serde(default)]
    pub to_recipients: Option<Vec<Empfaenger>>,
    #[serde(default)]
    pub cc_recipients: Option<Vec<Empfaenger>>,
    #[serde(default)]
    pub received_date_time: Option<String>,
    #[serde(default)]
    pub body_preview: Option<String>,
    #[serde(default)]
    pub body: Option<Koerper>,
    /// Nur der neue Teil der Mail, ohne den zitierten Verlauf (`uniqueBody`).
    #[serde(default)]
    pub unique_body: Option<Koerper>,
    #[serde(default)]
    pub web_link: Option<String>,
}

/// `{"dateTime": ..., "timeZone": ...}`
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Zeitpunkt {
    #[serde(default)]
    pub date_time: Option<String>,
    #[serde(default)]
    pub time_zone: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Ort {
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Teilnehmer {
    #[serde(default)]
    pub email_address: Option<Adresse>,
    #[serde(rename = "type", default)]
    pub art: Option<String>,
    #[serde(default)]
    pub status: Option<TeilnehmerStatus>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TeilnehmerStatus {
    #[serde(default)]
    pub response: Option<String>,
}

/// `GET /me/calendarView`, `POST /me/events`
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Termin {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub start: Option<Zeitpunkt>,
    #[serde(default)]
    pub end: Option<Zeitpunkt>,
    #[serde(default)]
    pub location: Option<Ort>,
    #[serde(default)]
    pub organizer: Option<Empfaenger>,
    #[serde(default)]
    pub attendees: Option<Vec<Teilnehmer>>,
    #[serde(default)]
    pub is_all_day: Option<bool>,
    #[serde(default)]
    pub body_preview: Option<String>,
    #[serde(default)]
    pub web_link: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Eltern {
    /// z. B. `/drive/root:/Projekte`
    #[serde(default)]
    pub path: Option<String>,
}

/// `GET /me/drive/root/search(q='…')`
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Datei {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub web_url: Option<String>,
    #[serde(default)]
    pub last_modified_date_time: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub parent_reference: Option<Eltern>,
}
