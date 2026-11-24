// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Planner- und To-Do-Tools (docs/aufgaben/06-graph-erweiterung.md).
//!
//! Planner verlangt bei jeder Änderung `If-Match` mit dem ETag der Aufgabe.
//! Ohne ETag gibt es **keinen** PATCH – lieber eine Meldung als ein blindes
//! Überschreiben. To Do kommt ohne ETag aus.
//!
//! Namen (Plan, Bucket, Liste, Aufgabe) dürfen Ids ersetzen; mehrdeutig →
//! Kandidaten und keine Aktion.

use jiff::Timestamp;
use m365_anmeldung::Anmeldung;
use m365_graph::{Bucket, Plan, PlannerAufgabe, Sammlung, ToDoAufgabe, ToDoListe};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::ErrorData;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::aufloesen::{ist_graph_id, ist_guid, person_finden, waehlen, Anzeige};
use crate::planner::{
    aufgabe_json, bucket_finden, bucket_json, faelligkeit, fortschritt_pruefen, patch_koerper,
    plan_json, todo_aufgabe_json, todo_faelligkeit, todo_liste_json, todo_status, zuweisung,
};
use crate::server::{als_json, ergebnis_erweitert, ungueltig_tool, Quiver, ToolFehler};

/// So viele Pläne/Listen holt eine Namenssuche höchstens.
const HOECHSTENS_LISTE: usize = 200;
/// `$select` für Planner-Aufgaben (inklusive ETag).
const AUFGABE_FELDER: &str =
    "id,planId,bucketId,title,percentComplete,priority,dueDateTime,completedDateTime,createdDateTime,assignments";
/// `$select` für To-Do-Aufgaben.
const TODO_FELDER: &str =
    "id,title,status,importance,dueDateTime,completedDateTime,createdDateTime,lastModifiedDateTime,body";

/// `GET /groups?$search=…` – nur Id und Name.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Gruppe {
    #[serde(default)]
    id: String,
    #[serde(default)]
    display_name: Option<String>,
}

// ---------------------------------------------------------------------------
// Parameter der Tools
// ---------------------------------------------------------------------------

/// Parameter für `planner_plaene`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct PlannerPlaeneAnfrage {
    /// Nur die Pläne dieser Gruppe (Id oder Name). Ohne Angabe alle Pläne des Users.
    #[serde(default)]
    pub gruppe: Option<String>,
    /// Wie viele Pläne (Standard 20, höchstens 50).
    #[serde(default)]
    pub anzahl: Option<u32>,
}

/// Parameter für `planner_aufgaben`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct PlannerAufgabenAnfrage {
    /// Plan als Id oder Titel. Ohne Angabe alle Aufgaben des Users (`/me/planner/tasks`).
    #[serde(default)]
    pub plan: Option<String>,
    /// Nur Aufgaben dieses Buckets (Id oder Name).
    #[serde(default)]
    pub bucket: Option<String>,
    /// Wie viele Aufgaben (Standard 20, höchstens 50).
    #[serde(default)]
    pub anzahl: Option<u32>,
}

/// Parameter für `planner_aufgabe_anlegen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct PlannerAufgabeAnlegenAnfrage {
    /// Plan als Id oder Titel.
    pub plan: String,
    /// Titel der Aufgabe.
    pub titel: String,
    /// Bucket als Id oder Name. Ohne Angabe der erste Bucket des Plans.
    #[serde(default)]
    pub bucket: Option<String>,
    /// Fälligkeit als Datum (`2026-11-30`) oder Zeitpunkt (`2026-11-30T12:00:00+01:00`).
    #[serde(default)]
    pub faellig: Option<String>,
    /// Person, der die Aufgabe zugewiesen wird (Name, E-Mail, UPN oder Id).
    #[serde(default)]
    pub zugewiesen: Option<String>,
}

/// Parameter für `planner_aufgabe_aendern`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct PlannerAufgabeAendernAnfrage {
    /// Id der Aufgabe (aus `planner_aufgaben`).
    pub aufgabe: String,
    /// Fortschritt: 0 (neu), 50 (in Arbeit) oder 100 (fertig).
    #[serde(default)]
    pub fortschritt: Option<u32>,
    /// Neue Fälligkeit als Datum oder Zeitpunkt.
    #[serde(default)]
    pub faellig: Option<String>,
    /// Neue Zuweisung (Name, E-Mail, UPN oder Id).
    #[serde(default)]
    pub zugewiesen: Option<String>,
}

/// Parameter für `todo_aufgaben`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TodoAufgabenAnfrage {
    /// Liste als Id oder Name (aus `todo_listen`).
    pub liste: String,
    /// Nur offene Aufgaben (Standard `false`: alle).
    #[serde(default)]
    pub nur_offen: Option<bool>,
    /// Wie viele Aufgaben (Standard 20, höchstens 50).
    #[serde(default)]
    pub anzahl: Option<u32>,
}

/// Parameter für `todo_aufgabe_anlegen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TodoAufgabeAnlegenAnfrage {
    /// Liste als Id oder Name.
    pub liste: String,
    /// Titel der Aufgabe.
    pub titel: String,
    /// Fälligkeit als Datum (`2026-11-30`) oder Zeitpunkt mit Versatz.
    #[serde(default)]
    pub faellig: Option<String>,
    /// Wichtigkeit: `low`, `normal` oder `high` (Standard `normal`).
    #[serde(default)]
    pub wichtigkeit: Option<String>,
    /// Freitext zur Aufgabe (optional).
    #[serde(default)]
    pub text: Option<String>,
}

/// Parameter für `todo_aufgabe_erledigen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TodoAufgabeErledigenAnfrage {
    /// Liste als Id oder Name.
    pub liste: String,
    /// Aufgabe als Id oder Titel.
    pub aufgabe: String,
    /// `true` (Standard): erledigen. `false`: wieder öffnen.
    #[serde(default)]
    pub erledigt: Option<bool>,
}

fn anzahl_mit(anzahl: Option<u32>, standard: u32) -> u32 {
    anzahl.unwrap_or(standard).clamp(1, 50)
}

// ---------------------------------------------------------------------------
// Die Tools
// ---------------------------------------------------------------------------

#[tool_router(router = planner_router, vis = "pub(crate)")]
impl<A: Anmeldung + 'static> Quiver<A> {
    /// Die Pläne des Users – insgesamt oder je Gruppe.
    #[tool(
        description = "Planner-Pläne auflisten (Id, Titel, Gruppe) – alle des Users oder die einer Gruppe.",
        annotations(read_only_hint = true)
    )]
    pub async fn planner_plaene(
        &self,
        Parameters(anfrage): Parameters<PlannerPlaeneAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis_erweitert(self.planner_plaene_lauf(anfrage).await)
    }

    /// Aufgaben eines Plans oder alle eigenen, mit Buckets.
    #[tool(
        description = "Planner-Aufgaben lesen: je Plan (mit Bucket-Namen) oder alle eigenen Aufgaben.",
        annotations(read_only_hint = true)
    )]
    pub async fn planner_aufgaben(
        &self,
        Parameters(anfrage): Parameters<PlannerAufgabenAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis_erweitert(self.planner_aufgaben_lauf(anfrage).await)
    }

    /// Eine Planner-Aufgabe anlegen.
    #[tool(
        description = "Eine Planner-Aufgabe anlegen. Verändert das Board des Plans.",
        annotations(destructive_hint = true)
    )]
    pub async fn planner_aufgabe_anlegen(
        &self,
        Parameters(anfrage): Parameters<PlannerAufgabeAnlegenAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis_erweitert(self.planner_aufgabe_anlegen_lauf(anfrage).await)
    }

    /// Eine Planner-Aufgabe ändern (Fortschritt, Fälligkeit, Zuweisung).
    #[tool(
        description = "Eine Planner-Aufgabe ändern: Fortschritt, Fälligkeit, Zuweisung. Nutzt das ETag der Aufgabe (If-Match) und ändert sonst nichts.",
        annotations(destructive_hint = true)
    )]
    pub async fn planner_aufgabe_aendern(
        &self,
        Parameters(anfrage): Parameters<PlannerAufgabeAendernAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis_erweitert(self.planner_aufgabe_aendern_lauf(anfrage).await)
    }

    /// Die To-Do-Listen des Users.
    #[tool(
        description = "Microsoft-To-Do-Listen auflisten (Id und Name).",
        annotations(read_only_hint = true)
    )]
    pub async fn todo_listen(&self) -> Result<String, ErrorData> {
        ergebnis_erweitert(self.todo_listen_lauf().await)
    }

    /// Die Aufgaben einer To-Do-Liste.
    #[tool(
        description = "Aufgaben einer Microsoft-To-Do-Liste lesen – optional nur die offenen.",
        annotations(read_only_hint = true)
    )]
    pub async fn todo_aufgaben(
        &self,
        Parameters(anfrage): Parameters<TodoAufgabenAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis_erweitert(self.todo_aufgaben_lauf(anfrage).await)
    }

    /// Eine To-Do-Aufgabe anlegen.
    #[tool(
        description = "Eine Aufgabe in einer Microsoft-To-Do-Liste anlegen. Verändert die Liste.",
        annotations(destructive_hint = true)
    )]
    pub async fn todo_aufgabe_anlegen(
        &self,
        Parameters(anfrage): Parameters<TodoAufgabeAnlegenAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis_erweitert(self.todo_aufgabe_anlegen_lauf(anfrage).await)
    }

    /// Eine To-Do-Aufgabe erledigen (oder wieder öffnen).
    #[tool(
        description = "Eine To-Do-Aufgabe als erledigt markieren – oder wieder öffnen. Verändert die Liste.",
        annotations(destructive_hint = true)
    )]
    pub async fn todo_aufgabe_erledigen(
        &self,
        Parameters(anfrage): Parameters<TodoAufgabeErledigenAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis_erweitert(self.todo_aufgabe_erledigen_lauf(anfrage).await)
    }
}

// ---------------------------------------------------------------------------
// Auflösen
// ---------------------------------------------------------------------------

impl<A: Anmeldung + 'static> Quiver<A> {
    async fn gruppe_finden(&self, referenz: &str) -> Result<Anzeige, ToolFehler> {
        let referenz = referenz.trim();
        if referenz.is_empty() {
            return Err(ungueltig_tool("gruppe fehlt"));
        }
        if ist_guid(referenz) {
            return Ok(Anzeige::neu(referenz, None));
        }
        let gruppen: Sammlung<Gruppe> = self
            .graph
            .get_mit_kopf(
                "/groups",
                &[
                    ("$search", format!("\"displayName:{referenz}\"")),
                    ("$select", "id,displayName".to_string()),
                    ("$top", HOECHSTENS_LISTE.to_string()),
                ],
                &[("ConsistencyLevel", "eventual")],
            )
            .await?;
        let kandidaten: Vec<Anzeige> = gruppen
            .value
            .into_iter()
            .map(|gruppe| Anzeige::neu(gruppe.id, gruppe.display_name.as_deref()))
            .collect();
        waehlen(kandidaten, referenz, "Gruppe")
    }

    async fn plan_finden(&self, referenz: &str) -> Result<Anzeige, ToolFehler> {
        let referenz = referenz.trim();
        if referenz.is_empty() {
            return Err(ungueltig_tool("plan fehlt"));
        }
        if ist_graph_id(referenz) {
            return Ok(Anzeige::neu(referenz, None));
        }
        let plaene = self.plaene_laden(None, HOECHSTENS_LISTE as u32).await?;
        let kandidaten: Vec<Anzeige> = plaene
            .into_iter()
            .map(|plan| Anzeige::neu(plan.id, plan.title.as_deref()))
            .collect();
        waehlen(kandidaten, referenz, "Plan")
    }

    async fn plaene_laden(
        &self,
        gruppe: Option<&Anzeige>,
        anzahl: u32,
    ) -> Result<Vec<Plan>, ToolFehler> {
        let pfad = match gruppe {
            Some(gruppe) => format!("/groups/{}/planner/plans", gruppe.id),
            None => "/me/planner/plans".to_string(),
        };
        Ok(self
            .graph
            .get_seiten::<Plan>(
                &pfad,
                &[
                    ("$select", "id,title,owner,createdDateTime".to_string()),
                    ("$top", anzahl.to_string()),
                ],
                anzahl as usize,
            )
            .await?)
    }

    async fn buckets_laden(&self, plan_id: &str) -> Result<Vec<Bucket>, ToolFehler> {
        Ok(self
            .graph
            .get_seiten::<Bucket>(
                &format!("/planner/plans/{plan_id}/buckets"),
                &[("$select", "id,name,planId".to_string())],
                HOECHSTENS_LISTE,
            )
            .await?)
    }

    async fn todo_liste_finden(&self, referenz: &str) -> Result<Anzeige, ToolFehler> {
        let referenz = referenz.trim();
        if referenz.is_empty() {
            return Err(ungueltig_tool("liste fehlt"));
        }
        if ist_graph_id(referenz) {
            return Ok(Anzeige::neu(referenz, None));
        }
        let listen = self.todo_listen_laden().await?;
        let kandidaten: Vec<Anzeige> = listen
            .into_iter()
            .map(|liste| Anzeige::neu(liste.id, liste.display_name.as_deref()))
            .collect();
        waehlen(kandidaten, referenz, "To-Do-Liste")
    }

    async fn todo_listen_laden(&self) -> Result<Vec<ToDoListe>, ToolFehler> {
        Ok(self
            .graph
            .get_seiten::<ToDoListe>(
                "/me/todo/lists",
                &[
                    ("$select", "id,displayName,wellknownListName".to_string()),
                    ("$top", HOECHSTENS_LISTE.to_string()),
                ],
                HOECHSTENS_LISTE,
            )
            .await?)
    }

    async fn todo_aufgaben_laden(
        &self,
        liste: &Anzeige,
        anzahl: u32,
    ) -> Result<Vec<ToDoAufgabe>, ToolFehler> {
        Ok(self
            .graph
            .get_seiten::<ToDoAufgabe>(
                &format!("/me/todo/lists/{}/tasks", liste.id),
                &[
                    ("$select", TODO_FELDER.to_string()),
                    ("$top", anzahl.to_string()),
                ],
                anzahl as usize,
            )
            .await?)
    }

    async fn todo_aufgabe_finden(
        &self,
        liste: &Anzeige,
        referenz: &str,
    ) -> Result<Anzeige, ToolFehler> {
        let referenz = referenz.trim();
        if referenz.is_empty() {
            return Err(ungueltig_tool("aufgabe fehlt"));
        }
        if ist_graph_id(referenz) {
            return Ok(Anzeige::neu(referenz, None));
        }
        let aufgaben = self.todo_aufgaben_laden(liste, 50).await?;
        let kandidaten: Vec<Anzeige> = aufgaben
            .into_iter()
            .map(|aufgabe| Anzeige::neu(aufgabe.id, aufgabe.title.as_deref()))
            .collect();
        waehlen(kandidaten, referenz, "To-Do-Aufgabe")
    }
}

// ---------------------------------------------------------------------------
// Umsetzung der Tools
// ---------------------------------------------------------------------------

impl<A: Anmeldung + 'static> Quiver<A> {
    async fn planner_plaene_lauf(
        &self,
        anfrage: PlannerPlaeneAnfrage,
    ) -> Result<String, ToolFehler> {
        let anzahl = anzahl_mit(anfrage.anzahl, 20);
        let gruppe = match anfrage
            .gruppe
            .as_deref()
            .map(str::trim)
            .filter(|gruppe| !gruppe.is_empty())
        {
            Some(gruppe) => Some(self.gruppe_finden(gruppe).await?),
            None => None,
        };
        let plaene = self
            .plaene_laden(gruppe.as_ref(), anzahl)
            .await?
            .into_iter()
            .take(anzahl as usize)
            .map(|plan| plan_json(&plan))
            .collect::<Vec<Value>>();
        als_json(&plaene).map_err(ToolFehler::from)
    }

    async fn planner_aufgaben_lauf(
        &self,
        anfrage: PlannerAufgabenAnfrage,
    ) -> Result<String, ToolFehler> {
        let anzahl = anzahl_mit(anfrage.anzahl, 20);
        let plan = match anfrage
            .plan
            .as_deref()
            .map(str::trim)
            .filter(|plan| !plan.is_empty())
        {
            Some(plan) => Some(self.plan_finden(plan).await?),
            None => None,
        };

        let (pfad, buckets) = match &plan {
            Some(plan) => (
                format!("/planner/plans/{}/tasks", plan.id),
                self.buckets_laden(&plan.id).await?,
            ),
            None => ("/me/planner/tasks".to_string(), Vec::new()),
        };
        let aufgaben = self
            .graph
            .get_seiten::<PlannerAufgabe>(
                &pfad,
                &[
                    ("$select", AUFGABE_FELDER.to_string()),
                    ("$top", anzahl.to_string()),
                ],
                anzahl as usize,
            )
            .await?;

        // `bucket` grenzt ein, wenn einer genannt ist.
        let filter = anfrage
            .bucket
            .as_deref()
            .map(str::trim)
            .filter(|bucket| !bucket.is_empty());
        let mut gewaehlt = None;
        if let Some(filter) = filter {
            let gewaehlt_bucket = match bucket_finden(&buckets, filter) {
                Ok(bucket) => bucket,
                Err(fehler) => return Err(ungueltig_tool(fehler)),
            };
            gewaehlt = Some(gewaehlt_bucket.id.clone());
        }

        let liste: Vec<Value> = aufgaben
            .iter()
            .filter(|aufgabe| match &gewaehlt {
                Some(id) => aufgabe.bucket_id.as_deref() == Some(id.as_str()),
                None => true,
            })
            .map(|aufgabe| {
                let name = aufgabe.bucket_id.as_deref().and_then(|id| {
                    buckets
                        .iter()
                        .find(|bucket| bucket.id == id)
                        .and_then(|bucket| bucket.name.clone())
                });
                aufgabe_json(aufgabe, name.as_deref(), &self.zeitzone)
            })
            .collect();

        let buckets: Vec<Value> = buckets.iter().map(bucket_json).collect();
        als_json(&json!({"aufgaben": liste, "buckets": buckets})).map_err(ToolFehler::from)
    }

    async fn planner_aufgabe_anlegen_lauf(
        &self,
        anfrage: PlannerAufgabeAnlegenAnfrage,
    ) -> Result<String, ToolFehler> {
        let titel = anfrage.titel.trim();
        if titel.is_empty() {
            return Err(ungueltig_tool("titel fehlt"));
        }
        let plan = self.plan_finden(&anfrage.plan).await?;
        let buckets = self.buckets_laden(&plan.id).await?;
        let bucket = match anfrage
            .bucket
            .as_deref()
            .map(str::trim)
            .filter(|bucket| !bucket.is_empty())
        {
            Some(referenz) => bucket_finden(&buckets, referenz)
                .map_err(ungueltig_tool)?
                .clone(),
            None => buckets.first().cloned().ok_or_else(|| {
                ungueltig_tool(format!(
                    "Plan {:?} hat keinen Bucket – bitte im Planner einen anlegen",
                    plan.name
                ))
            })?,
        };

        let mut body = json!({
            "planId": plan.id,
            "bucketId": bucket.id,
            "title": titel,
        });
        if let Some(faellig) = anfrage
            .faellig
            .as_deref()
            .map(str::trim)
            .filter(|faellig| !faellig.is_empty())
        {
            body["dueDateTime"] = json!(faelligkeit(faellig).map_err(ungueltig_tool)?);
        }
        if let Some(referenz) = anfrage
            .zugewiesen
            .as_deref()
            .map(str::trim)
            .filter(|referenz| !referenz.is_empty())
        {
            let person = person_finden(&self.graph, referenz).await?;
            body["assignments"] = json!({person.id.clone(): zuweisung(Timestamp::now())});
        }

        let neu: PlannerAufgabe = self.graph.post("/planner/tasks", &body).await?;
        let mut antwort = aufgabe_json(&neu, bucket.name.as_deref(), &self.zeitzone);
        antwort["angelegt"] = json!(true);
        als_json(&antwort).map_err(ToolFehler::from)
    }

    async fn planner_aufgabe_aendern_lauf(
        &self,
        anfrage: PlannerAufgabeAendernAnfrage,
    ) -> Result<String, ToolFehler> {
        let aufgabe = anfrage.aufgabe.trim();
        if aufgabe.is_empty() {
            return Err(ungueltig_tool("aufgabe fehlt"));
        }

        let mut felder: Vec<(&str, Value)> = Vec::new();
        if let Some(fortschritt) =
            fortschritt_pruefen(anfrage.fortschritt).map_err(ungueltig_tool)?
        {
            felder.push(("percentComplete", json!(fortschritt)));
        }
        if let Some(faellig) = anfrage
            .faellig
            .as_deref()
            .map(str::trim)
            .filter(|faellig| !faellig.is_empty())
        {
            felder.push((
                "dueDateTime",
                json!(faelligkeit(faellig).map_err(ungueltig_tool)?),
            ));
        }
        if let Some(referenz) = anfrage
            .zugewiesen
            .as_deref()
            .map(str::trim)
            .filter(|referenz| !referenz.is_empty())
        {
            let person = person_finden(&self.graph, referenz).await?;
            felder.push((
                "assignments",
                json!({person.id.clone(): zuweisung(Timestamp::now())}),
            ));
        }
        if felder.is_empty() {
            return Err(ungueltig_tool(
                "nichts zu ändern – bitte fortschritt, faellig oder zugewiesen nennen",
            ));
        }

        // Planner verlangt `If-Match` mit dem aktuellen ETag. Erst lesen, dann
        // ändern – und ohne ETag gar nicht ändern.
        let aktuell: PlannerAufgabe = self
            .graph
            .get(
                &format!("/planner/tasks/{aufgabe}"),
                &[("$select", AUFGABE_FELDER.to_string())],
            )
            .await?;
        let Some(etag) = aktuell.etag.clone() else {
            return Err(ungueltig_tool(format!(
                "Planner-Aufgabe {aufgabe:?} kommt ohne ETag – ohne If-Match ändert Planner nichts. Es wurde nichts geändert."
            )));
        };

        let geaendert: PlannerAufgabe = self
            .graph
            .patch_mit_kopf(
                &format!("/planner/tasks/{aufgabe}"),
                &patch_koerper(felder),
                &[("If-Match", etag.as_str())],
            )
            .await?;

        let mut antwort = aufgabe_json(&geaendert, None, &self.zeitzone);
        antwort["geaendert"] = json!(true);
        als_json(&antwort).map_err(ToolFehler::from)
    }

    async fn todo_listen_lauf(&self) -> Result<String, ToolFehler> {
        let listen = self.todo_listen_laden().await?;
        let liste: Vec<Value> = listen.iter().map(todo_liste_json).collect();
        als_json(&liste).map_err(ToolFehler::from)
    }

    async fn todo_aufgaben_lauf(&self, anfrage: TodoAufgabenAnfrage) -> Result<String, ToolFehler> {
        let liste = self.todo_liste_finden(&anfrage.liste).await?;
        let anzahl = anzahl_mit(anfrage.anzahl, 20);
        let nur_offen = anfrage.nur_offen.unwrap_or(false);
        let aufgaben = self
            .todo_aufgaben_laden(&liste, anzahl)
            .await?
            .into_iter()
            .filter(|aufgabe| !nur_offen || aufgabe.status.as_deref() != Some("completed"))
            .take(anzahl as usize)
            .map(|aufgabe| todo_aufgabe_json(&aufgabe, &self.zeitzone))
            .collect::<Vec<Value>>();
        als_json(&json!({"liste": liste.text(), "aufgaben": aufgaben})).map_err(ToolFehler::from)
    }

    async fn todo_aufgabe_anlegen_lauf(
        &self,
        anfrage: TodoAufgabeAnlegenAnfrage,
    ) -> Result<String, ToolFehler> {
        let titel = anfrage.titel.trim();
        if titel.is_empty() {
            return Err(ungueltig_tool("titel fehlt"));
        }
        let liste = self.todo_liste_finden(&anfrage.liste).await?;

        let wichtigkeit = match anfrage
            .wichtigkeit
            .as_deref()
            .map(str::trim)
            .filter(|wert| !wert.is_empty())
        {
            None => None,
            Some(wert) => match wert.to_lowercase().as_str() {
                "low" | "niedrig" => Some("low"),
                "normal" => Some("normal"),
                "high" | "hoch" => Some("high"),
                anderes => {
                    return Err(ungueltig_tool(format!(
                    "wichtigkeit {anderes:?} kenne ich nicht – erlaubt sind low, normal und high"
                )))
                }
            },
        };

        let mut body = json!({"title": titel});
        if let Some(wichtigkeit) = wichtigkeit {
            body["importance"] = json!(wichtigkeit);
        }
        if let Some(faellig) = anfrage
            .faellig
            .as_deref()
            .map(str::trim)
            .filter(|faellig| !faellig.is_empty())
        {
            body["dueDateTime"] =
                todo_faelligkeit(faellig, &self.zeitzone).map_err(ungueltig_tool)?;
        }
        if let Some(text) = anfrage
            .text
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
        {
            body["body"] = json!({"content": text, "contentType": "text"});
        }

        let neu: ToDoAufgabe = self
            .graph
            .post(&format!("/me/todo/lists/{}/tasks", liste.id), &body)
            .await?;
        let mut antwort = todo_aufgabe_json(&neu, &self.zeitzone);
        antwort["liste"] = json!(liste.text());
        antwort["angelegt"] = json!(true);
        als_json(&antwort).map_err(ToolFehler::from)
    }

    async fn todo_aufgabe_erledigen_lauf(
        &self,
        anfrage: TodoAufgabeErledigenAnfrage,
    ) -> Result<String, ToolFehler> {
        let erledigt = anfrage.erledigt.unwrap_or(true);
        let liste = self.todo_liste_finden(&anfrage.liste).await?;
        let aufgabe = self.todo_aufgabe_finden(&liste, &anfrage.aufgabe).await?;
        let geaendert: ToDoAufgabe = self
            .graph
            .patch_mit_kopf(
                &format!("/me/todo/lists/{}/tasks/{}", liste.id, aufgabe.id),
                &json!({"status": todo_status(erledigt)}),
                &[],
            )
            .await?;
        let mut antwort = todo_aufgabe_json(&geaendert, &self.zeitzone);
        antwort["liste"] = json!(liste.text());
        antwort["erledigt"] = json!(erledigt);
        als_json(&antwort).map_err(ToolFehler::from)
    }
}
