// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Verweise aus Antworten in Namen übersetzen (Aufgabe 15 b).
//!
//! Die OpenAPI-Datei von Zephyr Scale ist hier eindeutig: ein Verweis
//! (`ProjectLink`, `StatusLink`, `PriorityLink`, `FolderLink`,
//! `EnvironmentLink`, `TestCycleLink`) trägt **nur** `id` und `self` – keinen
//! `key` und keinen `name`. Wer „Status: Draft“ oder „Projekt: QA“ schreiben
//! will, muss den Endpunkt fragen, den das Schema dafür nennt:
//!
//! | Verweis | Endpunkt | gelesenes Feld |
//! |---|---|---|
//! | `status` / `testExecutionStatus` | `GET /statuses?projectKey=…&statusType=…` | `name`, `default` |
//! | `priority` | `GET /priorities?projectKey=…` | `name` |
//! | `project` | `GET /projects/{id}` | `key` |
//! | `environment` | `GET /environments/{id}` | `name` |
//! | `testCycle` | `GET /testcycles/{id}` | `key` |
//!
//! Dieses Modul merkt sich jede Antwort: **je Id wird pro Werkzeugaufruf
//! einmal** gefragt, nie zweimal. Es ist der einzige Ort, an dem die
//! Zephyr-Antworten zusätzliche Aufrufe auslösen.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::client::{pfadwert, Anfrage, Client};
use crate::daten;
use crate::fehler::ZephyrFehler;

/// Statusart der Testfälle (`statusType` in `/statuses`).
pub const STATUS_TESTFALL: &str = "TEST_CASE";
/// Statusart der Testzyklen.
pub const STATUS_ZYKLUS: &str = "TEST_CYCLE";
/// Statusart der Ausführungen (das ist der Status eines Ergebnisses).
pub const STATUS_AUSFUEHRUNG: &str = "TEST_EXECUTION";
/// Statusart der Testpläne.
pub const STATUS_PLAN: &str = "TEST_PLAN";

/// So viele Verweise löst ein Aufruf höchstens einzeln auf.
///
/// Jeder Verweis mit eigener Id kostet eine Anfrage. Bei sehr langen Listen
/// bleibt der Verweis darum als Id stehen, statt hunderte Anfragen auszulösen.
pub const VERWEISE_HOECHSTENS: usize = 50;

/// Die Status eines Projekts einer Art, in der Reihenfolge des Servers.
#[derive(Debug, Clone, Default)]
struct StatusListe {
    /// `(id, name, default)`.
    eintraege: Vec<(String, String, bool)>,
    /// Wurden alle Seiten gelesen?
    vollstaendig: bool,
}

/// Was ein Aufruf bereits vom Server geholt hat.
#[derive(Debug, Default)]
pub struct Namen {
    /// `"ART:projekt"` → Statusliste.
    status: BTreeMap<String, StatusListe>,
    /// Projekt → (Prioritäts-Id → Name).
    prio_listen: BTreeMap<String, BTreeMap<String, String>>,
    /// Projekt-Id → Schlüssel (oder `None`, wenn es das Projekt nicht gibt).
    projekte: BTreeMap<String, Option<String>>,
    /// Umgebungs-Id → Name.
    umgebungen: BTreeMap<String, Option<String>>,
    /// Zyklus-Id → Schlüssel.
    zyklen: BTreeMap<String, Option<String>>,
}

impl Namen {
    /// Ein leerer Speicher.
    pub fn neu() -> Namen {
        Namen::default()
    }

    /// Die Status eines Projekts einer Art laden – einmal je Projekt und Art.
    ///
    /// Alle Seiten werden gelesen (bis [`crate::client::SEITEN_HOECHSTENS`]),
    /// damit ein Name auf Seite 2 nicht als „gibt es nicht“ gilt.
    pub async fn status_laden(
        &mut self,
        client: &Client,
        projekt: &str,
        art: &str,
    ) -> Result<(), ZephyrFehler> {
        let schluessel = format!("{art}:{projekt}");
        if self.status.contains_key(&schluessel) {
            return Ok(());
        }
        let (eintraege, _gesamt, vollstaendig) = client
            .alle(
                "statuses",
                vec![
                    ("projectKey".to_string(), projekt.to_string()),
                    ("statusType".to_string(), art.to_string()),
                ],
            )
            .await?;
        let liste = StatusListe {
            eintraege: eintraege
                .iter()
                .filter_map(|status| {
                    let id = daten::verweis_id(status)?;
                    let name = daten::text(status, "name")?;
                    Some((id, name, daten::wahr(status, "default")))
                })
                .collect(),
            vollstaendig,
        };
        self.status.insert(schluessel, liste);
        Ok(())
    }

    /// Der Name eines Status-Verweises, falls er geladen ist.
    pub fn status_name(&self, art: &str, projekt: &str, verweis: &Value) -> Option<String> {
        let id = daten::verweis_id(verweis)?;
        self.status
            .get(&format!("{art}:{projekt}"))?
            .eintraege
            .iter()
            .find(|(eintrag, _, _)| *eintrag == id)
            .map(|(_, name, _)| name.clone())
    }

    /// Alle erlaubten Statusnamen eines Projekts einer Art.
    pub fn status_namen(&self, art: &str, projekt: &str) -> Vec<String> {
        self.status
            .get(&format!("{art}:{projekt}"))
            .map(|liste| {
                liste
                    .eintraege
                    .iter()
                    .map(|(_, name, _)| name.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Der Name des Status, den der Server als Standard nennt.
    ///
    /// Zephyr darf einen Standard nennen (`default: true`); nur dann darf ein
    /// Werkzeug ihn einsetzen, statt ihn zu verlangen.
    pub fn status_standard(&self, art: &str, projekt: &str) -> Option<String> {
        self.status
            .get(&format!("{art}:{projekt}"))?
            .eintraege
            .iter()
            .find(|(_, _, standard)| *standard)
            .map(|(_, name, _)| name.clone())
    }

    /// Wurden die Status dieses Projekts vollständig gelesen?
    pub fn status_vollstaendig(&self, art: &str, projekt: &str) -> bool {
        self.status
            .get(&format!("{art}:{projekt}"))
            .map(|liste| liste.vollstaendig)
            .unwrap_or(false)
    }

    /// Die Prioritäten eines Projekts laden – einmal je Projekt.
    pub async fn prio_laden(&mut self, client: &Client, projekt: &str) -> Result<(), ZephyrFehler> {
        if self.prio_listen.contains_key(projekt) {
            return Ok(());
        }
        let (eintraege, _gesamt, _vollstaendig) = client
            .alle(
                "priorities",
                vec![("projectKey".to_string(), projekt.to_string())],
            )
            .await?;
        let mut liste = BTreeMap::new();
        for prio_eintrag in &eintraege {
            if let (Some(id), Some(name)) = (
                daten::verweis_id(prio_eintrag),
                daten::text(prio_eintrag, "name"),
            ) {
                liste.insert(id, name);
            }
        }
        self.prio_listen.insert(projekt.to_string(), liste);
        Ok(())
    }

    /// Der Name einer Priorität, falls geladen.
    pub fn prio_name(&self, projekt: &str, verweis: &Value) -> Option<String> {
        let id = daten::verweis_id(verweis)?;
        self.prio_listen.get(projekt)?.get(&id).cloned()
    }

    /// Der Projektschlüssel zu einem Projekt-Verweis (`GET /projects/{id}`).
    ///
    /// Das ist der Weg, den `nur` nimmt: der Schlüssel kommt aus der Antwort,
    /// nicht aus dem Aufruf des Agenten.
    pub async fn projekt_key(
        &mut self,
        client: &Client,
        verweis: &Value,
    ) -> Result<Option<String>, ZephyrFehler> {
        let Some(id) = daten::verweis_id(verweis) else {
            return Ok(None);
        };
        if let Some(wert) = self.projekte.get(&id) {
            return Ok(wert.clone());
        }
        let wert = self.einzeln(client, "projects", &id, "key").await?;
        self.projekte.insert(id, wert.clone());
        Ok(wert)
    }

    /// Der Name einer Umgebung (`GET /environments/{id}`).
    pub async fn umgebung_name(
        &mut self,
        client: &Client,
        verweis: &Value,
    ) -> Result<Option<String>, ZephyrFehler> {
        let Some(id) = daten::verweis_id(verweis) else {
            return Ok(None);
        };
        if let Some(wert) = self.umgebungen.get(&id) {
            return Ok(wert.clone());
        }
        if self.umgebungen.len() >= VERWEISE_HOECHSTENS {
            return Ok(None);
        }
        let wert = self.einzeln(client, "environments", &id, "name").await?;
        self.umgebungen.insert(id, wert.clone());
        Ok(wert)
    }

    /// Der Schlüssel eines Testzyklus (`GET /testcycles/{id}`).
    pub async fn zyklus_key(
        &mut self,
        client: &Client,
        verweis: &Value,
    ) -> Result<Option<String>, ZephyrFehler> {
        let Some(id) = daten::verweis_id(verweis) else {
            return Ok(None);
        };
        if let Some(wert) = self.zyklen.get(&id) {
            return Ok(wert.clone());
        }
        if self.zyklen.len() >= VERWEISE_HOECHSTENS {
            return Ok(None);
        }
        let wert = self.einzeln(client, "testcycles", &id, "key").await?;
        self.zyklen.insert(id, wert.clone());
        Ok(wert)
    }

    /// Der bereits geladene Schlüssel eines Zyklus-Verweises.
    pub fn zyklus_key_gelesen(&self, verweis: &Value) -> Option<String> {
        let id = daten::verweis_id(verweis)?;
        self.zyklen.get(&id).cloned().flatten()
    }

    /// Der bereits geladene Name einer Umgebung.
    pub fn umgebung_name_gelesen(&self, verweis: &Value) -> Option<String> {
        let id = daten::verweis_id(verweis)?;
        self.umgebungen.get(&id).cloned().flatten()
    }

    /// Nur für Tests: eine Statusliste setzen.
    #[cfg(test)]
    pub fn setze_status(&mut self, art: &str, projekt: &str, eintraege: &[(i64, &str, bool)]) {
        self.status.insert(
            format!("{art}:{projekt}"),
            StatusListe {
                eintraege: eintraege
                    .iter()
                    .map(|(id, name, standard)| (id.to_string(), name.to_string(), *standard))
                    .collect(),
                vollstaendig: true,
            },
        );
    }

    /// Nur für Tests: Prioritäten setzen.
    #[cfg(test)]
    pub fn setze_prio(&mut self, projekt: &str, eintraege: &[(i64, &str)]) {
        self.prio_listen.insert(
            projekt.to_string(),
            eintraege
                .iter()
                .map(|(id, name)| (id.to_string(), name.to_string()))
                .collect(),
        );
    }

    /// Nur für Tests: eine Umgebung setzen.
    #[cfg(test)]
    pub fn setze_umgebung(&mut self, id: &str, name: Option<&str>) {
        self.umgebungen
            .insert(id.to_string(), name.map(str::to_string));
    }

    /// Nur für Tests: einen Zyklus setzen.
    #[cfg(test)]
    pub fn setze_zyklus(&mut self, id: &str, key: Option<&str>) {
        self.zyklen.insert(id.to_string(), key.map(str::to_string));
    }

    /// `GET /<pfad>/<id>` und ein Feld daraus; `404` heißt „gibt es nicht“.
    async fn einzeln(
        &self,
        client: &Client,
        pfad: &str,
        id: &str,
        feld: &str,
    ) -> Result<Option<String>, ZephyrFehler> {
        let ziel = format!("{pfad}/{}", pfadwert(id, "id")?);
        match client.json(&Anfrage::holen(ziel)).await {
            Ok(baum) => Ok(daten::text(&baum, feld)),
            Err(fehler) if fehler.ist_nicht_gefunden() => Ok(None),
            Err(fehler) => Err(fehler),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ein_verweis_wird_ueber_seine_id_gesucht() {
        let mut namen = Namen::neu();
        namen.status.insert(
            format!("{STATUS_AUSFUEHRUNG}:QA"),
            StatusListe {
                eintraege: vec![
                    ("1".into(), "Pass".into(), false),
                    ("2".into(), "Fail".into(), true),
                ],
                vollstaendig: true,
            },
        );
        let verweis = json!({ "id": 1, "self": "https://x/statuses/1" });
        assert_eq!(
            namen
                .status_name(STATUS_AUSFUEHRUNG, "QA", &verweis)
                .as_deref(),
            Some("Pass")
        );
        assert_eq!(
            namen.status_standard(STATUS_AUSFUEHRUNG, "QA").as_deref(),
            Some("Fail")
        );
        assert_eq!(
            namen.status_namen(STATUS_AUSFUEHRUNG, "QA"),
            vec!["Pass", "Fail"]
        );
        assert!(namen.status_vollstaendig(STATUS_AUSFUEHRUNG, "QA"));

        // Ein anderer Status (andere Art oder anderes Projekt) ist nicht dabei.
        assert_eq!(namen.status_name(STATUS_TESTFALL, "QA", &verweis), None);
        assert_eq!(namen.status_name(STATUS_AUSFUEHRUNG, "OPS", &verweis), None);
        assert_eq!(
            namen.status_name(STATUS_AUSFUEHRUNG, "QA", &json!({})),
            None
        );
        assert_eq!(namen.status_standard(STATUS_AUSFUEHRUNG, "OPS"), None);
        assert!(namen.status_namen(STATUS_TESTFALL, "QA").is_empty());
        assert!(!namen.status_vollstaendig(STATUS_TESTFALL, "QA"));
    }

    #[test]
    fn eine_prioritaet_haengt_am_projekt() {
        let mut namen = Namen::neu();
        namen.prio_listen.insert(
            "QA".into(),
            [("2".to_string(), "High".to_string())]
                .into_iter()
                .collect(),
        );
        let verweis = json!({ "id": 2, "self": "https://x/priorities/2" });
        assert_eq!(namen.prio_name("QA", &verweis).as_deref(), Some("High"));
        assert_eq!(namen.prio_name("OPS", &verweis), None);
        assert_eq!(namen.prio_name("QA", &json!({})), None);
    }
}
