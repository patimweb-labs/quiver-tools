// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Teams-Modelle (docs/aufgaben/05-teams.md): nur die Felder, die die Tools
//! zurückgeben – `$select` setzt das Tool passend.
//!
//! Alle Ids und Namen sind optional, weil Graph je Endpunkt unterschiedlich viel
//! liefert (`/me/chats` ohne `$expand=members` hat keine Mitglieder usw.).

use serde::{Deserialize, Serialize};

use crate::modelle::Koerper;

/// `GET /me/joinedTeams`, `GET /teams/{team}`
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Team {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

/// `GET /teams/{team}/channels`
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Kanal {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub membership_type: Option<String>,
    #[serde(default)]
    pub web_url: Option<String>,
}

/// Ein Mitglied eines Teams, eines Kanals oder eines Chats – Graph liefert
/// dieselben Felder für alle drei (`aadUserConversationMember`).
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Mitglied {
    /// Die Mitgliedschafts-Id (nicht die User-Id).
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub roles: Option<Vec<String>>,
}

/// `chat.viewpoint` – die Lesemarke des angemeldeten Users für diesen Chat.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Sicht {
    #[serde(default)]
    pub last_message_read_date_time: Option<String>,
    #[serde(default)]
    pub is_hidden: Option<bool>,
}

/// `chat.lastMessagePreview`
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Vorschau {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub created_date_time: Option<String>,
    #[serde(default)]
    pub body: Option<Koerper>,
    #[serde(default)]
    pub from: Option<Absender>,
}

/// Eine Person, die in einer Nachricht steht (`from`, `mention.user`).
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Identitaet {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
}

/// `{"user": {…}}` bzw. `{"application": {…}}` an einer Nachricht.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Absender {
    #[serde(default)]
    pub user: Option<Identitaet>,
    #[serde(default)]
    pub application: Option<Identitaet>,
}

/// `GET /me/chats` (`$expand=members,lastMessagePreview`)
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Chat {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub topic: Option<String>,
    #[serde(default)]
    pub chat_type: Option<String>,
    #[serde(default)]
    pub members: Option<Vec<Mitglied>>,
    #[serde(default)]
    pub last_message_preview: Option<Vorschau>,
    #[serde(default)]
    pub viewpoint: Option<Sicht>,
    #[serde(default)]
    pub web_url: Option<String>,
}

/// Ein Anhang einer Nachricht – für die Antwort reicht der Name.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Anhang {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub content_url: Option<String>,
}

/// Eine Reaktion auf eine Nachricht.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Reaktion {
    #[serde(default)]
    pub reaction_type: Option<String>,
    #[serde(default)]
    pub created_date_time: Option<String>,
    #[serde(default)]
    pub user: Option<Identitaet>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Erwaehnter {
    #[serde(default)]
    pub user: Option<Identitaet>,
}

/// Eine `@`-Erwähnung in einer Nachricht.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Erwaehnung {
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(default)]
    pub mention_text: Option<String>,
    #[serde(default)]
    pub mentioned: Option<Erwaehnter>,
}

/// Wo eine Kanalnachricht lebt (bei Suchtreffern mitgeliefert).
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct KanalIdentitaet {
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub channel_id: Option<String>,
}

/// Eine Teams-Nachricht (Chat, Kanalbeitrag, Antwort oder Suchtreffer).
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TeamsNachricht {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub reply_to_id: Option<String>,
    /// `message`, `systemEventMessage`, …
    #[serde(default)]
    pub message_type: Option<String>,
    #[serde(default)]
    pub created_date_time: Option<String>,
    #[serde(default)]
    pub last_modified_date_time: Option<String>,
    /// Gesetzt, wenn die Nachricht gelöscht ist.
    #[serde(default)]
    pub deleted_date_time: Option<String>,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub importance: Option<String>,
    #[serde(default)]
    pub web_url: Option<String>,
    #[serde(default)]
    pub from: Option<Absender>,
    #[serde(default)]
    pub body: Option<Koerper>,
    #[serde(default)]
    pub attachments: Option<Vec<Anhang>>,
    #[serde(default)]
    pub mentions: Option<Vec<Erwaehnung>>,
    #[serde(default)]
    pub reactions: Option<Vec<Reaktion>>,
    /// Nur bei Suchtreffern: der Chat, in dem der Treffer liegt.
    #[serde(default)]
    pub chat_id: Option<String>,
    /// Nur bei Suchtreffern: Team und Kanal, in dem der Treffer liegt.
    #[serde(default)]
    pub channel_identity: Option<KanalIdentitaet>,
}

// ---------------------------------------------------------------------------
// Microsoft Search (`POST /search/query`, `entityTypes: ["chatMessage"]`)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SucheAntwort {
    #[serde(default)]
    pub value: Vec<SucheAntwortBlock>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SucheAntwortBlock {
    #[serde(default)]
    pub hits_containers: Vec<TrefferBehaelter>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TrefferBehaelter {
    #[serde(default)]
    pub hits: Vec<Treffer>,
    #[serde(default)]
    pub total: Option<u64>,
    #[serde(default)]
    pub more_results_available: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Treffer {
    #[serde(default)]
    pub hit_id: Option<String>,
    #[serde(default)]
    pub rank: Option<i64>,
    /// Zusammenfassung mit `<c0>`-Markierungen um die Suchwörter.
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub resource: Option<TeamsNachricht>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nachricht_liest_nur_was_da_ist() {
        let nachricht: TeamsNachricht = serde_json::from_value(serde_json::json!({
            "id": "n1",
            "messageType": "message",
            "createdDateTime": "2026-11-27T09:00:00Z",
            "deletedDateTime": null,
            "from": {"user": {"id": "u1", "displayName": "Anna"}},
            "body": {"contentType": "html", "content": "<p>Hallo</p>"},
            "attachments": [{"id": "a1", "name": "Bericht.pdf"}],
        }))
        .expect("Modell passt");

        assert_eq!(nachricht.id, "n1");
        assert_eq!(
            nachricht
                .from
                .as_ref()
                .and_then(|absender| absender.user.as_ref())
                .and_then(|user| user.display_name.as_deref()),
            Some("Anna")
        );
        assert_eq!(
            nachricht
                .attachments
                .as_ref()
                .and_then(|liste| liste.first())
                .and_then(|anhang| anhang.name.as_deref()),
            Some("Bericht.pdf")
        );
    }

    #[test]
    fn chat_ohne_mitglieder_bleibt_lesbar() {
        let chat: Chat = serde_json::from_value(serde_json::json!({
            "id": "19:chat@thread.v2",
            "chatType": "oneOnOne",
        }))
        .expect("Modell passt");
        assert_eq!(chat.chat_type.as_deref(), Some("oneOnOne"));
        assert!(chat.members.is_none());
    }

    #[test]
    fn suchtreffer_tragen_ihren_ort() {
        let antwort: SucheAntwort = serde_json::from_value(serde_json::json!({
            "value": [{
                "hitsContainers": [{
                    "hits": [{
                        "hitId": "h1",
                        "summary": "Ein <c0>Treffer</c0>",
                        "resource": {
                            "id": "n1",
                            "createdDateTime": "2026-11-27T09:00:00Z",
                            "chatId": "19:chat@thread.v2",
                        },
                    }],
                    "total": 1,
                }],
            }],
        }))
        .expect("Modell passt");
        let treffer = &antwort.value[0].hits_containers[0].hits[0];
        let resource = treffer.resource.as_ref().expect("resource");
        assert_eq!(resource.chat_id.as_deref(), Some("19:chat@thread.v2"));
        assert_eq!(treffer.summary.as_deref(), Some("Ein <c0>Treffer</c0>"));
    }
}
