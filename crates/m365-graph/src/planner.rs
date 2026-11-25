// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Microsoft Planner und Microsoft To Do (docs/aufgaben/06-graph-erweiterung.md).
//!
//! Planner verlangt bei jeder Änderung ein `If-Match` mit dem ETag der Aufgabe –
//! deshalb trägt [`PlannerAufgabe`] das Feld `@odata.etag` ausdrücklich mit.
//! To Do kommt ohne ETag aus.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::modelle::{Koerper, Zeitpunkt};

/// `GET /me/planner/plans`, `GET /groups/{gruppe}/planner/plans`
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    /// Die Gruppe, der der Plan gehört (Planner-Pläne hängen immer an einer).
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub created_date_time: Option<String>,
}

/// `GET /planner/plans/{plan}/buckets` – ein „Bucket“ ist eine Spalte im Board.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Bucket {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub plan_id: Option<String>,
}

/// `GET /planner/tasks/{id}`, `GET /planner/plans/{plan}/tasks`, `/me/planner/tasks`
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PlannerAufgabe {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub plan_id: Option<String>,
    #[serde(default)]
    pub bucket_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    /// 0 = neu, 50 = in Arbeit, 100 = fertig.
    #[serde(default)]
    pub percent_complete: Option<u32>,
    #[serde(default)]
    pub priority: Option<u32>,
    #[serde(default)]
    pub due_date_time: Option<String>,
    #[serde(default)]
    pub completed_date_time: Option<String>,
    #[serde(default)]
    pub created_date_time: Option<String>,
    /// `{"<user-id>": {"@odata.type": …, "orderDateTime": …}}`
    #[serde(default)]
    pub assignments: Option<Value>,
    /// Das ETag, das Planner für `If-Match` verlangt.
    #[serde(default, rename = "@odata.etag")]
    pub etag: Option<String>,
}

/// `GET /me/todo/lists`
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ToDoListe {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    /// `defaultList`, `flaggedEmails`, …
    #[serde(default)]
    pub wellknown_list_name: Option<String>,
}

/// `GET /me/todo/lists/{liste}/tasks`
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ToDoAufgabe {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    /// `notStarted`, `inProgress`, `completed`, `waitingOnOthers`, `deferred`.
    #[serde(default)]
    pub status: Option<String>,
    /// `low`, `normal`, `high`.
    #[serde(default)]
    pub importance: Option<String>,
    /// To Do liefert `{"dateTime": …, "timeZone": …}`, nicht eine Zeichenkette.
    #[serde(default)]
    pub due_date_time: Option<Zeitpunkt>,
    #[serde(default)]
    pub completed_date_time: Option<Zeitpunkt>,
    #[serde(default)]
    pub created_date_time: Option<String>,
    #[serde(default)]
    pub last_modified_date_time: Option<String>,
    #[serde(default)]
    pub body: Option<Koerper>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planner_aufgabe_traegt_ihr_etag() {
        let aufgabe: PlannerAufgabe = serde_json::from_value(serde_json::json!({
            "id": "task-1",
            "planId": "plan-1",
            "bucketId": "bucket-1",
            "title": "Angebot schreiben",
            "percentComplete": 50,
            "dueDateTime": "2026-11-30T00:00:00Z",
            "@odata.etag": "W/\"etag-1\"",
        }))
        .expect("Modell passt");
        assert_eq!(aufgabe.percent_complete, Some(50));
        assert_eq!(aufgabe.etag.as_deref(), Some("W/\"etag-1\""));

        let ohne: PlannerAufgabe = serde_json::from_value(serde_json::json!({
            "id": "task-2",
            "title": "Ohne ETag",
        }))
        .expect("Modell passt");
        assert!(ohne.etag.is_none());
    }

    #[test]
    fn todo_aufgabe_liest_status_und_text() {
        let aufgabe: ToDoAufgabe = serde_json::from_value(serde_json::json!({
            "id": "todo-1",
            "title": "Rechnung prüfen",
            "status": "notStarted",
            "importance": "high",
            "body": {"contentType": "text", "content": "Bis Freitag"},
        }))
        .expect("Modell passt");
        assert_eq!(aufgabe.status.as_deref(), Some("notStarted"));
        assert_eq!(
            aufgabe.body.and_then(|koerper| koerper.content).as_deref(),
            Some("Bis Freitag")
        );
    }
}
