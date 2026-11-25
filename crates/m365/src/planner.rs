// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Planner und To Do für die Antworten (docs/aufgaben/06-graph-erweiterung.md).
//!
//! Reine Funktionen ohne Netz: Fortschritt prüfen, Fälligkeiten in die Form
//! bringen, die Graph erwartet, und Aufgaben knapp als JSON ausgeben.
//!
//! Planner rechnet in UTC: `dueDateTime` ist ein Zeitpunkt mit `Z`. Ein Datum
//! ohne Uhrzeit wird deshalb zu `…T00:00:00Z`. To Do will stattdessen Ortzeit
//! **und** Zone – dafür gibt es [`todo_faelligkeit`].

use jiff::Timestamp;
use m365_graph::{Bucket, Plan, PlannerAufgabe, ToDoAufgabe, ToDoListe};
use serde_json::{json, Map, Value};

use crate::zeit::Zeitzone;

/// Planner verlangt genau einen dieser drei Werte im Board.
const FORTSCHRITTE: &[u32] = &[0, 50, 100];

/// `fortschritt` prüfen: 0 bis 100, wie Planner es kennt (0, 50, 100).
pub fn fortschritt_pruefen(fortschritt: Option<u32>) -> Result<Option<u32>, String> {
    match fortschritt {
        None => Ok(None),
        Some(wert) if wert <= 100 => Ok(Some(wert)),
        Some(wert) => Err(format!(
            "fortschritt {wert} ist zu groß – erlaubt sind 0 (neu), 50 (in Arbeit) und 100 (fertig)"
        )),
    }
}

/// Ist `fortschritt` einer der drei Werte, die Planner im Board anzeigt?
pub fn ist_board_fortschritt(fortschritt: u32) -> bool {
    FORTSCHRITTE.contains(&fortschritt)
}

/// `fortschritt` als Wort für die Antwort.
pub fn fortschritt_text(fortschritt: Option<u32>) -> Option<&'static str> {
    match fortschritt? {
        0 => Some("neu"),
        1..=99 => Some("in Arbeit"),
        _ => Some("fertig"),
    }
}

/// Ein Datum oder ein Zeitpunkt → der Zeitpunkt, den Planner erwartet.
///
/// `2026-11-30` → `2026-11-30T00:00:00Z` (UTC, wie Planner rechnet);
/// `2026-11-30T12:00:00+01:00` → `2026-11-30T11:00:00Z`.
pub fn faelligkeit(roh: &str) -> Result<String, String> {
    let text = roh.trim();
    if text.is_empty() {
        return Err("Fälligkeit fehlt (erwartet z. B. 2026-11-30)".to_string());
    }
    if let Ok(zeitpunkt) = text.parse::<Timestamp>() {
        return Ok(zeitpunkt.to_string());
    }
    let datum = text
        .parse::<jiff::civil::Date>()
        .map_err(|_| format!("Fälligkeit {text:?} ist kein Datum und kein Zeitpunkt (erwartet z. B. 2026-11-30 oder 2026-11-30T12:00:00+01:00)"))?;
    Ok(format!("{datum}T00:00:00Z"))
}

/// Fälligkeit für To Do: Ortzeit in `zone` **und** der Zonenname.
pub fn todo_faelligkeit(roh: &str, zone: &Zeitzone) -> Result<Value, String> {
    let text = roh.trim();
    if text.is_empty() {
        return Err("Fälligkeit fehlt (erwartet z. B. 2026-11-30)".to_string());
    }
    if let Ok(datum) = text.parse::<jiff::civil::Date>() {
        return Ok(json!({
            "dateTime": format!("{datum}T00:00:00"),
            "timeZone": zone.name(),
        }));
    }
    let ortzeit = zone.eingabe(text)?;
    Ok(json!({"dateTime": ortzeit, "timeZone": zone.name()}))
}

/// Wer einer Aufgabe zugewiesen ist: Graph erwartet ein Objekt je Benutzer-Id.
pub fn zuweisung(jetzt: Timestamp) -> Value {
    json!({
        "@odata.type": "#microsoft.graph.plannerAssignment",
        "orderDateTime": jetzt.to_string(),
    })
}

/// Ist die Aufgabe jemandem zugewiesen?
pub fn zugewiesen_an(aufgabe: &PlannerAufgabe) -> Vec<String> {
    match aufgabe.assignments.as_ref().and_then(Value::as_object) {
        Some(karte) => karte
            .iter()
            .filter(|(_, wert)| wert.get("assignedDateTime").is_some() || wert.is_object())
            .map(|(id, _)| id.clone())
            .collect(),
        None => Vec::new(),
    }
}

/// Der Plan als knappes JSON.
pub fn plan_json(plan: &Plan) -> Value {
    json!({
        "id": plan.id,
        "titel": plan.title,
        "gruppe": plan.owner,
        "angelegt": plan.created_date_time,
    })
}

/// Der Bucket als knappes JSON.
pub fn bucket_json(bucket: &Bucket) -> Value {
    json!({"id": bucket.id, "name": bucket.name})
}

/// Die Planner-Aufgabe als knappes JSON; `bucket` ist der Name, wenn bekannt.
pub fn aufgabe_json(aufgabe: &PlannerAufgabe, bucket: Option<&str>, zone: &Zeitzone) -> Value {
    let mut wert = json!({
        "id": aufgabe.id,
        "titel": aufgabe.title,
        "plan": aufgabe.plan_id,
        "bucket": bucket.map(str::to_string).or_else(|| aufgabe.bucket_id.clone()),
        "fortschritt": aufgabe.percent_complete,
        "status": fortschritt_text(aufgabe.percent_complete),
        "prioritaet": aufgabe.priority,
        "faellig": aufgabe
            .due_date_time
            .as_deref()
            .and_then(|zeit| zone.zeitpunkt(zeit, None)),
        "erledigt_am": aufgabe
            .completed_date_time
            .as_deref()
            .and_then(|zeit| zone.zeitpunkt(zeit, None)),
        "zugewiesen": zugewiesen_an(aufgabe),
    });
    if let Some(etag) = aufgabe.etag.as_deref() {
        wert["etag"] = json!(etag);
    }
    wert
}

/// Die To-Do-Liste als knappes JSON.
pub fn todo_liste_json(liste: &ToDoListe) -> Value {
    json!({
        "id": liste.id,
        "name": liste.display_name,
        "art": liste.wellknown_list_name,
    })
}

/// Ein To-Do-Zeitpunkt (`dateTime` + `timeZone`) als RFC 3339 mit Versatz.
fn todo_zeit(zeit: Option<&m365_graph::Zeitpunkt>, zone: &Zeitzone) -> Option<String> {
    let zeit = zeit?;
    let roh = zeit.date_time.as_deref()?;
    zone.zeitpunkt(roh, zeit.time_zone.as_deref())
}

/// Die To-Do-Aufgabe als knappes JSON.
pub fn todo_aufgabe_json(aufgabe: &ToDoAufgabe, zone: &Zeitzone) -> Value {
    json!({
        "id": aufgabe.id,
        "titel": aufgabe.title,
        "status": aufgabe.status,
        "wichtigkeit": aufgabe.importance,
        "faellig": todo_zeit(aufgabe.due_date_time.as_ref(), zone),
        "erledigt_am": todo_zeit(aufgabe.completed_date_time.as_ref(), zone),
        "text": aufgabe
            .body
            .as_ref()
            .and_then(|koerper| koerper.content.clone())
            .map(|inhalt| inhalt.trim().to_string())
            .filter(|inhalt| !inhalt.is_empty()),
    })
}

/// Der Status, den To Do für „erledigt“ bzw. „offen“ kennt.
pub fn todo_status(erledigt: bool) -> &'static str {
    if erledigt {
        "completed"
    } else {
        "notStarted"
    }
}

/// Eine Bucket-Liste nach Namen durchsuchen (Id **oder** Name).
pub fn bucket_finden<'a>(buckets: &'a [Bucket], referenz: &str) -> Result<&'a Bucket, String> {
    let referenz = referenz.trim();
    if let Some(bucket) = buckets.iter().find(|bucket| bucket.id == referenz) {
        return Ok(bucket);
    }
    let klein = referenz.to_lowercase();
    let genau: Vec<&Bucket> = buckets
        .iter()
        .filter(|bucket| {
            bucket
                .name
                .as_deref()
                .is_some_and(|name| name.to_lowercase() == klein)
        })
        .collect();
    match genau.len() {
        1 => Ok(genau[0]),
        0 => {
            let teil: Vec<&Bucket> = buckets
                .iter()
                .filter(|bucket| {
                    bucket
                        .name
                        .as_deref()
                        .is_some_and(|name| name.to_lowercase().contains(&klein))
                })
                .collect();
            match teil.len() {
                1 => Ok(teil[0]),
                0 => Err(format!(
                    "Bucket {referenz:?} nicht gefunden – bitte `planner_aufgaben` mit dem Plan aufrufen"
                )),
                _ => Err(format!(
                    "Bucket {referenz:?} ist mehrdeutig – bitte die Id nennen"
                )),
            }
        }
        _ => Err(format!(
            "Bucket {referenz:?} ist mehrdeutig – bitte die Id nennen"
        )),
    }
}

/// Die Zuordnungen als Karte, damit ein PATCH nur die genannten Felder schickt.
pub fn patch_koerper(felder: Vec<(&str, Value)>) -> Value {
    let mut karte = Map::new();
    for (name, wert) in felder {
        karte.insert(name.to_string(), wert);
    }
    Value::Object(karte)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fortschritt_wird_geprueft() {
        assert_eq!(fortschritt_pruefen(None), Ok(None));
        assert_eq!(fortschritt_pruefen(Some(0)), Ok(Some(0)));
        assert_eq!(fortschritt_pruefen(Some(50)), Ok(Some(50)));
        assert_eq!(fortschritt_pruefen(Some(100)), Ok(Some(100)));
        assert!(fortschritt_pruefen(Some(101)).is_err());
        assert!(ist_board_fortschritt(50));
        assert!(!ist_board_fortschritt(25));
    }

    #[test]
    fn fortschritt_wird_ein_wort() {
        assert_eq!(fortschritt_text(Some(0)), Some("neu"));
        assert_eq!(fortschritt_text(Some(50)), Some("in Arbeit"));
        assert_eq!(fortschritt_text(Some(100)), Some("fertig"));
        assert_eq!(fortschritt_text(None), None);
    }

    #[test]
    fn faelligkeit_wird_ein_utc_zeitpunkt() {
        assert_eq!(
            faelligkeit("2026-11-30").expect("Datum"),
            "2026-11-30T00:00:00Z"
        );
        assert_eq!(
            faelligkeit(" 2026-11-30T12:00:00+01:00 ").expect("Zeitpunkt"),
            "2026-11-30T11:00:00Z"
        );
        assert!(faelligkeit("").is_err());
        assert!(faelligkeit("morgen").is_err());
        assert!(faelligkeit("2026-02-30").is_err());
    }

    #[test]
    fn todo_faelligkeit_traegt_die_zone() {
        let zone = Zeitzone::neu("Europe/Berlin");
        let wert = todo_faelligkeit("2026-11-30", &zone).expect("Datum");
        assert_eq!(wert["dateTime"], "2026-11-30T00:00:00");
        assert_eq!(wert["timeZone"], "Europe/Berlin");

        let wert = todo_faelligkeit("2026-11-30T10:00:00Z", &zone).expect("Zeitpunkt");
        assert_eq!(wert["dateTime"], "2026-11-30T11:00:00");
        assert!(todo_faelligkeit("morgen", &zone).is_err());
    }

    #[test]
    fn zuweisungen_werden_gelesen() {
        let aufgabe: PlannerAufgabe = serde_json::from_value(json!({
            "id": "task-1",
            "assignments": {
                "u-bert": {"@odata.type": "#microsoft.graph.plannerAssignment", "orderDateTime": "2026-11-27T09:00:00Z"}
            },
        }))
        .expect("Modell passt");
        assert_eq!(zugewiesen_an(&aufgabe), vec!["u-bert"]);

        let leer = PlannerAufgabe::default();
        assert!(zugewiesen_an(&leer).is_empty());
    }

    #[test]
    fn buckets_loesen_sich_ueber_id_und_name_auf() {
        let buckets = vec![
            Bucket {
                id: "b1".to_string(),
                name: Some("Zu erledigen".to_string()),
                ..Bucket::default()
            },
            Bucket {
                id: "b2".to_string(),
                name: Some("Erledigt".to_string()),
                ..Bucket::default()
            },
        ];
        assert_eq!(bucket_finden(&buckets, "b2").expect("Id").id, "b2");
        assert_eq!(bucket_finden(&buckets, "erledigt").expect("Name").id, "b2");
        assert!(bucket_finden(&buckets, "Quatsch").is_err());
        assert!(bucket_finden(&[], "b1").is_err());
    }

    #[test]
    fn aufgabe_json_traegt_etag_und_zeit() {
        let zone = Zeitzone::neu("Europe/Berlin");
        let aufgabe = PlannerAufgabe {
            id: "task-1".to_string(),
            title: Some("Angebot".to_string()),
            percent_complete: Some(50),
            due_date_time: Some("2026-11-30T00:00:00Z".to_string()),
            etag: Some("W/\"1\"".to_string()),
            ..PlannerAufgabe::default()
        };
        let wert = aufgabe_json(&aufgabe, Some("Zu erledigen"), &zone);
        assert_eq!(wert["titel"], "Angebot");
        assert_eq!(wert["status"], "in Arbeit");
        assert_eq!(wert["bucket"], "Zu erledigen");
        assert_eq!(wert["faellig"], "2026-11-30T01:00:00+01:00");
        assert_eq!(wert["etag"], "W/\"1\"");
    }

    #[test]
    fn todo_status_und_json() {
        assert_eq!(todo_status(true), "completed");
        assert_eq!(todo_status(false), "notStarted");

        let zone = Zeitzone::neu("Europe/Berlin");
        let aufgabe = ToDoAufgabe {
            id: "todo-1".to_string(),
            title: Some("Rechnung".to_string()),
            status: Some("notStarted".to_string()),
            body: Some(m365_graph::Koerper {
                content_type: Some("text".to_string()),
                content: Some("  Bis Freitag  ".to_string()),
            }),
            ..ToDoAufgabe::default()
        };
        let wert = todo_aufgabe_json(&aufgabe, &zone);
        assert_eq!(wert["titel"], "Rechnung");
        assert_eq!(wert["text"], "Bis Freitag");
    }
}
