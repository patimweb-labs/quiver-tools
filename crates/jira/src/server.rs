// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der MCP-Server: 17 Werkzeuge über stdio (Aufgabe 13).
//!
//! Zwölf lesende Werkzeuge (`read_only_hint`) und fünf verändernde
//! (`destructive_hint`, brauchen den Schalter `schreiben`) – dazu aus dem Kern
//! `jira_profile`. Die Sites kommen aus `QUIVER_ZUGAENGE` (mehrere je Werkzeug).
//! Fehlt die Liste, antwortet jedes Werkzeug mit dem Hinweis auf das
//! quiver-Fenster – **kein** MCP-Fehler. Jedes Werkzeug nimmt `verbindung`; bei
//! genau einer Site darf der Name fehlen.
//!
//! Vier Regeln stehen **vor** jeder Anfrage (Aufgaben 12 und 13):
//!
//! | Regel | Wirkung |
//! |---|---|
//! | `schreiben` je Site (Standard aus) | verändernde Werkzeuge antworten ohne Anfrage |
//! | `nur` (Projektschlüssel) | Schreiben außerhalb wird ohne verändernde Anfrage abgelehnt; der Schlüssel kommt vom Server |
//! | E-Mail und Token nur in der Kopfzeile | keine Antwort, kein Fehler und kein Log nennt sie |
//! | Folgeseite nur am eigenen Host | fremder Host wird ohne Anfrage abgelehnt |

use std::any::Any;

use quiver_tools_kern::mcp::profile_tool;
use quiver_tools_kern::{antwort, hinweis_zugang, OhneProfil, Zugang};
use rmcp::handler::server::tool::schema_for_type;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, JsonObject, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool, ToolAnnotations,
};
use rmcp::schemars::JsonSchema;
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::adf;
use crate::client::{self, Anfrage, Client};
use crate::daten;
use crate::fehler::JiraFehler;
use crate::format;
use crate::jql;
use crate::sites::{AuswahlFehler, SammelFehler, Sammlung, Site};
use crate::uebergang::{self, Wahl};
use crate::werkzeug::{MEHRFACH_EINZAHL, MEHRFACH_TITEL, TITEL};

/// `GET myself`: wer ist angemeldet.
const MICH: &str = "myself";

/// Was ein Werkzeug zurückgibt.
type Ergebnis = Result<CallToolResult, McpError>;

/// Der Jira-Server: die Sites und die Werkzeugliste.
pub struct JiraServer {
    /// Der Zugang des Kerns; bei `mehrfach` ohne Felder, aber mit Titel.
    zugang: Zugang,
    sites: Result<Sammlung, SammelFehler>,
}

impl JiraServer {
    /// Der Normalfall: alles aus der Umgebung (`QUIVER_ZUGAENGE`).
    pub fn neu(zugang: Zugang) -> Self {
        JiraServer {
            zugang,
            sites: Sammlung::aus_umgebung(),
        }
    }

    /// Feste Teile (Tests und Sonderfälle), ohne Prozessumgebung.
    pub fn mit(zugang: Zugang, sites: Result<Sammlung, SammelFehler>) -> Self {
        JiraServer { zugang, sites }
    }

    /// Alle Werkzeuge, wie `tools/list` sie meldet.
    pub fn werkzeuge(&self) -> Vec<Tool> {
        let mut alle = self.lesende_werkzeuge();
        alle.extend(self.schreibende_werkzeuge());
        // `jira_profile` kommt aus dem Kern: bei jedem Tool dasselbe lesende
        // Werkzeug.
        alle.push(profile_tool(&self.zugang));
        alle
    }

    /// Die zwölf lesenden Werkzeuge.
    fn lesende_werkzeuge(&self) -> Vec<Tool> {
        vec![
            lesend::<StatusParam>(
                "jira_status",
                "Sites",
                "Listet die im quiver-Fenster eingetragenen Jira-Sites: Name, Adresse, erlaubte \
                 Projekte (Feld „nur“) und ob „schreiben“ an ist. Öffnet keine Verbindung, zeigt \
                 weder E-Mail noch Token.",
            ),
            lesend::<VerbindungParam>(
                "jira_testen",
                "Anmeldung prüfen",
                "Meldet sich mit E-Mail und API-Token an und gibt zurück, wer angemeldet ist. \
                 Ohne `verbindung` prüft es alle Sites. Ein abgelehnter Token wird nicht \
                 wiederholt.",
            ),
            lesend::<ProjekteParam>(
                "jira_projekte",
                "Projekte",
                "Listet die Projekte der Site: Schlüssel, Name, Typ und Leitung. `anzahl` ist \
                 standardmäßig 25 und höchstens 100; Folgeseiten am selben Host werden geladen.",
            ),
            lesend::<SucheParam>(
                "jira_suchen",
                "Vorgänge suchen",
                "Sucht Vorgänge. Entweder mit fertiger `jql` oder bequem über `projekt`, \
                 `status`, `zugewiesen_an_mich` und `text`; daraus wird die JQL gebaut. \
                 `anzahl` ist standardmäßig 25 und höchstens 100. Gibt Schlüssel, Typ, Status, \
                 Titel und Zuweisung zurück und die Seite für den nächsten Aufruf (`seite`).",
            ),
            lesend::<VorgangParam>(
                "jira_vorgang",
                "Vorgang",
                "Ein Vorgang mit Feldern, Beschreibung (ADF als Text), Verknüpfungen, \
                 Unteraufgaben und Eltern. Lange Texte werden über `max_zeichen` gekürzt \
                 (Standard 4000).",
            ),
            lesend::<ListeParam>(
                "jira_kommentare",
                "Kommentare",
                "Alle Kommentare eines Vorgangs mit Autor, Datum, Kommentar-ID und Text (gekürzt \
                 über `max_zeichen`, Standard 4000).",
            ),
            lesend::<VorgangParam>(
                "jira_uebergaenge",
                "Übergänge",
                "Die möglichen Workflow-Übergänge eines Vorgangs mit ID und Zielstatus. Vor \
                 `jira_uebergang` aufrufen.",
            ),
            lesend::<FelderParam>(
                "jira_felder_zum_anlegen",
                "Felder zum Anlegen",
                "Die Vorgangstypen eines Projekts und – mit `vorgangstyp` – die Felder dieses \
                 Typs: Pflichtfelder und erlaubte Werte. Vor `jira_anlegen` aufrufen.",
            ),
            lesend::<FilterlisteParam>(
                "jira_filter_liste",
                "Filter",
                "Sucht gespeicherte Filter (eigene und geteilte) nach Namen und nennt ihre JQL.",
            ),
            lesend::<FilterParam>(
                "jira_filter",
                "Filter lesen",
                "Ein gespeicherter Filter mit Name, JQL, Beschreibung und Besitzer.",
            ),
            lesend::<VerbindungParam>(
                "jira_verknuepfungsarten",
                "Verknüpfungsarten",
                "Die Verknüpfungsarten der Site (z. B. Blocks) mit ID und der Bedeutung in beide \
                 Richtungen. Vor `jira_verknuepfen` aufrufen.",
            ),
            lesend::<PersonParam>(
                "jira_person_suchen",
                "Person suchen",
                "Sucht Atlassian-Konten nach Name oder E-Mail und nennt die `accountId` – die \
                 braucht `jira_anlegen`/`jira_aendern` für die Zuweisung.",
            ),
        ]
    }

    /// Die fünf verändernden Werkzeuge (brauchen `schreiben`).
    fn schreibende_werkzeuge(&self) -> Vec<Tool> {
        vec![
            schreibend::<AnlegenParam>(
                "jira_anlegen",
                "Vorgang anlegen",
                "Legt einen Vorgang an: Projekt, Vorgangstyp, Titel, optional Beschreibung, \
                 Elternvorgang (für eine Unteraufgabe), Zuweisung und weitere Felder. Gesendet \
                 wird der Projektschlüssel, den Jira nennt. Braucht den Schalter „schreiben“ und \
                 ein Projekt aus dem Feld „nur“; ein Elternvorgang muss ebenfalls darin liegen.",
            ),
            schreibend::<AendernParam>(
                "jira_aendern",
                "Vorgang ändern",
                "Ändert Felder eines Vorgangs: beliebige Felder als Paare (Wert als JSON, also auch \
                 Listen wie `labels` oder Objekte wie `priority`) oder bequem `titel`, \
                 `beschreibung` und `zugewiesen`. `project` lässt sich nicht ändern; ein doppelt \
                 angegebenes Feld (`summary` und `titel`) wird abgelehnt. `parent` nur, wenn der \
                 Elternvorgang laut Jira im Feld „nur“ liegt. Den Status ändert nur \
                 `jira_uebergang`.",
            ),
            schreibend::<UebergangParam>(
                "jira_uebergang",
                "Übergang ausführen",
                "Führt einen Workflow-Übergang aus, den `jira_uebergaenge` für diesen Vorgang nennt \
                 (ID oder Name, optional mit Kommentar) und meldet den neuen Status, den es danach \
                 am Vorgang liest. Ein Übergang, den Jira nicht anbietet, wird nicht versucht.",
            ),
            schreibend::<KommentarParam>(
                "jira_kommentieren",
                "Vorgang kommentieren",
                "Schreibt einen Kommentar an einen Vorgang.",
            ),
            schreibend::<VerknuepfenParam>(
                "jira_verknuepfen",
                "Vorgänge verknüpfen",
                "Verknüpft zwei Vorgänge: `vorgang` ist die eine Seite, `ziel` die andere. Gelesen \
                 wird die Beziehung in Richtung der Verknüpfungsart: bei „Blocks“ gilt „vorgang \
                 blocks ziel“, bei „Relates“ „vorgang relates to ziel“ – `vorgang` steht also im \
                 outward-Text von `jira_verknuepfungsarten`, `ziel` im inward-Text. Nach dem \
                 Anlegen liest das Tool die Verknüpfung am Vorgang und meldet sie so, wie Jira sie \
                 zeigt; weicht sie ab, sagt es das deutlich. Beide Vorgänge werden gegen das Feld \
                 „nur“ geprüft.",
            ),
        ]
    }

    /// Ein Werkzeug aufrufen. Auch der Weg, den die Tests nehmen.
    pub async fn aufrufen(&self, name: &str, argumente: Option<JsonObject>) -> Ergebnis {
        match name {
            "jira_status" => self.status(argumente),
            "jira_testen" => self.testen(argumente).await,
            "jira_projekte" => self.projekte(argumente).await,
            "jira_suchen" => self.suchen(argumente).await,
            "jira_vorgang" => self.vorgang(argumente).await,
            "jira_kommentare" => self.kommentare(argumente).await,
            "jira_uebergaenge" => self.uebergaenge(argumente).await,
            "jira_felder_zum_anlegen" => self.felder_zum_anlegen(argumente).await,
            "jira_filter_liste" => self.filter_liste(argumente).await,
            "jira_filter" => self.filter(argumente).await,
            "jira_verknuepfungsarten" => self.verknuepfungsarten(argumente).await,
            "jira_person_suchen" => self.person_suchen(argumente).await,
            "jira_anlegen" => self.anlegen(argumente).await,
            "jira_aendern" => self.aendern(argumente).await,
            "jira_uebergang" => self.uebergang(argumente).await,
            "jira_kommentieren" => self.kommentieren(argumente).await,
            "jira_verknuepfen" => self.verknuepfen(argumente).await,
            "jira_profile" => self.profile(argumente),
            sonst => Err(McpError::invalid_params(
                format!("Unbekanntes Werkzeug „{sonst}“."),
                None,
            )),
        }
    }

    /// `jira_profile`: bei einer Site-Liste sind „die Angaben“ die Sites.
    fn profile(&self, roh: Option<JsonObject>) -> Ergebnis {
        self.status(roh)
    }

    /// Die Liste der Sites oder ein Fehler ohne ihren Inhalt.
    fn sammlung(&self) -> Result<&Sammlung, McpError> {
        self.sites.as_ref().map_err(|fehler| {
            McpError::invalid_params(
                format!("{fehler} – bitte im quiver-Fenster unter Tools → {TITEL} prüfen."),
                None,
            )
        })
    }

    /// Die gewünschte Site; fehlt die Liste, kommt der Hinweis.
    fn waehlen(&self, name: Option<&str>) -> Result<Site, OhneProfil> {
        let sammlung = self.sammlung().map_err(OhneProfil::Fehler)?;
        if sammlung.ist_leer() {
            return Err(OhneProfil::Hinweis(antwort(hinweis_zugang(TITEL))));
        }
        sammlung
            .waehlen(name)
            .cloned()
            .map_err(|fehler: AuswahlFehler| {
                OhneProfil::Fehler(McpError::invalid_params(fehler.to_string(), None))
            })
    }

    /// Die Site – aber nur, wenn der Schalter „schreiben“ an ist.
    ///
    /// Ist er aus, kommt der Hinweis **ohne** Anfrage an Jira.
    fn schreiben_erlaubt(&self, name: Option<&str>) -> Result<Site, OhneProfil> {
        let site = self.waehlen(name)?;
        if !site.schreiben_erlaubt() {
            return Err(OhneProfil::Hinweis(antwort(format::schreiben_aus(
                &site.name,
            ))));
        }
        Ok(site)
    }

    /// Ein Client für diese Site, oder ein MCP-Fehler ohne Token.
    fn client(site: &Site) -> Result<Client, McpError> {
        Client::neu(site).map_err(fehler_zu_mcp)
    }

    /// Liest ein **Projekt vom Server** und prüft es gegen das Feld `nur`.
    ///
    /// Den Schlüssel glauben wir nicht dem Agenten: er kommt aus
    /// `GET project/{key}` und wird genau so gesendet. Liegt das Projekt
    /// außerhalb von `nur`, kommt der Hinweis statt einer Änderung.
    async fn projekt_lesen(
        &self,
        client: &Client,
        site: &Site,
        projekt: &str,
    ) -> Result<String, OhneProfil> {
        let pfad = pfadwert(projekt, "projekt").map_err(OhneProfil::Fehler)?;
        let gelesen = client
            .json(&Anfrage::holen(format!("project/{pfad}")))
            .await
            .map_err(|fehler| OhneProfil::Fehler(fehler_zu_mcp(fehler)))?;
        let key = daten::text(&gelesen, "key").ok_or_else(|| {
            OhneProfil::Fehler(McpError::internal_error(
                format!(
                    "Jira nennt zu „{projekt}“ keinen Projektschlüssel – bitte den Schlüssel in \
                     Jira prüfen."
                ),
                None,
            ))
        })?;
        if site.erlaubt_projekt(&key) {
            Ok(key)
        } else {
            Err(OhneProfil::Hinweis(antwort(format::nur_verletzt(
                &site.name, &key, &site.nur,
            ))))
        }
    }

    /// Prüft einen **Vorgang** gegen das Feld `nur`.
    ///
    /// Das Projekt kommt aus `GET issue/…?fields=project`, nicht aus dem
    /// Schlüssel, den der Agent genannt hat. Ohne `nur` entfällt die Anfrage.
    async fn nur_vorgang(&self, client: &Client, site: &Site, key: &str) -> Result<(), OhneProfil> {
        if site.nur.is_empty() {
            return Ok(());
        }
        let pfad = pfadwert(key, "key").map_err(OhneProfil::Fehler)?;
        let gelesen = client
            .json(&Anfrage::holen(format!("issue/{pfad}")).mit_abfrage("fields", "project"))
            .await
            .map_err(|fehler| OhneProfil::Fehler(fehler_zu_mcp(fehler)))?;
        let projekt = daten::text(&gelesen, "fields.project.key").ok_or_else(|| {
            OhneProfil::Fehler(McpError::internal_error(
                "Das Projekt des Vorgangs fehlt in der Antwort – bitte `jira_vorgang` prüfen."
                    .to_string(),
                None,
            ))
        })?;
        if site.erlaubt_projekt(&projekt) {
            Ok(())
        } else {
            Err(OhneProfil::Hinweis(antwort(format::nur_verletzt(
                &site.name, &projekt, &site.nur,
            ))))
        }
    }

    // ------------------------------------------------------------- lesend

    fn status(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: StatusParam = argumente(roh)?;
        let sammlung = self.sammlung()?;
        if sammlung.ist_leer() {
            return Ok(antwort(hinweis_zugang(TITEL)));
        }
        let gewaehlt: Vec<Site> = match p.verbindung.as_deref() {
            Some(name) => vec![sammlung
                .waehlen(Some(name))
                .map_err(|fehler| McpError::invalid_params(fehler.to_string(), None))?
                .clone()],
            None => sammlung.alle().to_vec(),
        };
        Ok(antwort(format::status(&gewaehlt)))
    }

    async fn testen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: VerbindungParam = argumente(roh)?;
        let sammlung = self.sammlung()?;
        if sammlung.ist_leer() {
            return Ok(antwort(hinweis_zugang(TITEL)));
        }
        // Ohne Namen: alle Sites prüfen. Mit Namen: genau die eine.
        let sites: Vec<Site> = match p.verbindung.as_deref() {
            Some(name) => vec![sammlung
                .waehlen(Some(name))
                .map_err(|fehler| McpError::invalid_params(fehler.to_string(), None))?
                .clone()],
            None => sammlung.alle().to_vec(),
        };

        let mut zeilen = Vec::new();
        for site in &sites {
            let client = Self::client(site)?;
            let mich = client
                .json(&Anfrage::holen(MICH))
                .await
                .map_err(fehler_zu_mcp)?;
            zeilen.push(format::testen(site, &mich));
        }
        Ok(antwort(zeilen.join("\n\n")))
    }

    async fn projekte(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: ProjekteParam = argumente(roh)?;
        let site = match self.waehlen(p.verbindung.as_deref()) {
            Ok(site) => site,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let anzahl = anzahl_der_liste(p.anzahl);
        let client = client_or_mcp(&site)?;

        // Folgeseiten: nur am eingetragenen Host (der Client prüft `next`).
        let mut eintraege: Vec<Value> = Vec::new();
        let mut gesamt: Option<i64> = None;
        let mut naechste: Option<String> = None;
        let mut weiter = false;
        for _ in 0..20 {
            let anfrage = match &naechste {
                Some(link) => Anfrage::holen(link.clone()),
                None => {
                    Anfrage::holen("project/search").mit_abfrage("maxResults", anzahl.to_string())
                }
            };
            let geantwortet = client.json(&anfrage).await.map_err(fehler_zu_mcp)?;
            if gesamt.is_none() {
                gesamt = daten::zahl(&geantwortet, "total");
            }
            let werte = daten::liste(&geantwortet, "values");
            eintraege.extend(werte.iter().cloned());
            let letzte = daten::wahr(&geantwortet, "isLast") || werte.is_empty();
            naechste = daten::text(&geantwortet, "next");
            weiter = !letzte;
            if letzte || naechste.is_none() || eintraege.len() >= anzahl {
                break;
            }
        }
        if eintraege.len() > anzahl {
            eintraege.truncate(anzahl);
            weiter = true;
        }
        Ok(antwort(format::projekte(
            &site.name, &eintraege, gesamt, weiter,
        )))
    }

    async fn suchen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: SucheParam = argumente(roh)?;
        let site = match self.waehlen(p.verbindung.as_deref()) {
            Ok(site) => site,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let anzahl = anzahl_der_liste(p.anzahl);

        let abfrage = match p
            .jql
            .as_deref()
            .map(str::trim)
            .filter(|jql| !jql.is_empty())
        {
            Some(jql) => jql.to_string(),
            None => jql::bauen(
                &site.nur,
                &jql::Komfort {
                    projekt: p.projekt.as_deref(),
                    status: p.status.as_deref(),
                    zugewiesen_an_mich: p.zugewiesen_an_mich.unwrap_or(false),
                    text: p.text.as_deref(),
                },
            ),
        };

        // `POST search/jql` nimmt nur begrenzte Abfragen (Spezifikation):
        // eine reine `order by`-Klausel lehnt Jira ab. Das sagen wir vorher.
        if !jql::ist_begrenzt(&abfrage) {
            return Ok(antwort(format::suche_unbegrenzt(&site.name)));
        }

        let mut rumpf = json!({
            "jql": abfrage,
            "maxResults": anzahl,
            "fields": [
                "summary", "status", "issuetype", "assignee", "priority", "updated", "project"
            ],
        });
        if let Some(seite) = p
            .seite
            .as_deref()
            .map(str::trim)
            .filter(|seite| !seite.is_empty())
        {
            rumpf["nextPageToken"] = json!(seite);
        }

        let client = client_or_mcp(&site)?;
        let geantwortet = client
            .json(&Anfrage::post("search/jql", rumpf.to_string()))
            .await
            .map_err(fehler_zu_mcp)?;
        let eintraege = daten::liste(&geantwortet, "issues");
        let naechste = daten::text(&geantwortet, "nextPageToken");
        Ok(antwort(format::suchen(
            eintraege,
            None,
            naechste.as_deref(),
        )))
    }

    async fn vorgang(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: VorgangParam = argumente(roh)?;
        let site = match self.waehlen(p.verbindung.as_deref()) {
            Ok(site) => site,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let key = pflicht(&p.key, "key")?;
        let client = client_or_mcp(&site)?;
        let geantwortet = client
            .json(&Anfrage::holen(format!("issue/{}", pfadwert(key, "key")?)))
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::vorgang(
            &site,
            key,
            &geantwortet,
            zeichen(p.max_zeichen),
        )))
    }

    async fn kommentare(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: ListeParam = argumente(roh)?;
        let site = match self.waehlen(p.verbindung.as_deref()) {
            Ok(site) => site,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let key = pflicht(&p.key, "key")?;
        let anzahl = anzahl_der_liste(p.anzahl);
        let client = client_or_mcp(&site)?;
        let geantwortet = client
            .json(
                &Anfrage::holen(format!("issue/{}/comment", pfadwert(key, "key")?))
                    .mit_abfrage("maxResults", anzahl.to_string())
                    .mit_abfrage("orderBy", "created"),
            )
            .await
            .map_err(fehler_zu_mcp)?;
        let mut text = format::kommentare(key, &geantwortet, zeichen(p.max_zeichen));
        if let Some(gesamt) = daten::zahl(&geantwortet, "total") {
            if gesamt > daten::liste(&geantwortet, "comments").len() as i64 {
                text.push_str("\n… es gibt weitere Kommentare.");
            }
        }
        Ok(antwort(text))
    }

    async fn uebergaenge(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: VorgangParam = argumente(roh)?;
        let site = match self.waehlen(p.verbindung.as_deref()) {
            Ok(site) => site,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let key = pflicht(&p.key, "key")?;
        let client = client_or_mcp(&site)?;
        let geantwortet = client
            .json(&Anfrage::holen(format!(
                "issue/{}/transitions",
                pfadwert(key, "key")?
            )))
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::uebergaenge(key, &geantwortet)))
    }

    async fn felder_zum_anlegen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: FelderParam = argumente(roh)?;
        let site = match self.waehlen(p.verbindung.as_deref()) {
            Ok(site) => site,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let projekt = pflicht(&p.projekt, "projekt")?;
        let client = client_or_mcp(&site)?;
        let basis = format!(
            "issue/createmeta/{}/issuetypes",
            pfadwert(projekt, "projekt")?
        );

        let Some(roh_typ) = p
            .vorgangstyp
            .as_deref()
            .map(str::trim)
            .filter(|typ| !typ.is_empty())
        else {
            // Ohne Typ: die Liste der Vorgangstypen.
            let geantwortet = client
                .json(&Anfrage::holen(&basis))
                .await
                .map_err(fehler_zu_mcp)?;
            return Ok(antwort(format::vorgangstypen(projekt, &geantwortet)));
        };

        // Mit Typ: Name oder ID. Eine ID erkennen wir an den Ziffern.
        let (typ_id, typ_name) = if roh_typ.chars().all(|zeichen| zeichen.is_ascii_digit()) {
            (roh_typ.to_string(), roh_typ.to_string())
        } else {
            let liste = client
                .json(&Anfrage::holen(&basis))
                .await
                .map_err(fehler_zu_mcp)?;
            // Die Spezifikation nennt die Liste `issueTypes`.
            let treffer = daten::liste(&liste, "issueTypes")
                .iter()
                .find(|typ| {
                    daten::text(typ, "name")
                        .map(|name| name.eq_ignore_ascii_case(roh_typ))
                        .unwrap_or(false)
                })
                .ok_or_else(|| {
                    McpError::invalid_params(
                        format!(
                            "Den Vorgangstyp „{roh_typ}“ gibt es in {projekt} nicht – bitte \
                             `jira_felder_zum_anlegen` ohne `vorgangstyp` aufrufen."
                        ),
                        None,
                    )
                })?;
            (
                daten::text_oder(treffer, "id", ""),
                daten::text_oder(treffer, "name", roh_typ),
            )
        };

        let geantwortet = client
            .json(&Anfrage::holen(format!(
                "{basis}/{}",
                pfadwert(&typ_id, "vorgangstyp")?
            )))
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::felder_zum_anlegen(
            projekt,
            &typ_name,
            &geantwortet,
        )))
    }

    async fn filter_liste(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: FilterlisteParam = argumente(roh)?;
        let site = match self.waehlen(p.verbindung.as_deref()) {
            Ok(site) => site,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let anzahl = anzahl_der_liste(p.anzahl);
        let mut anfrage =
            Anfrage::holen("filter/search").mit_abfrage("maxResults", anzahl.to_string());
        if let Some(name) = p
            .name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            anfrage = anfrage.mit_abfrage("filterName", name);
        }
        let client = client_or_mcp(&site)?;
        let geantwortet = client.json(&anfrage).await.map_err(fehler_zu_mcp)?;
        let mut text = format::filter_liste(&geantwortet, zeichen(p.max_zeichen));
        let eintraege = daten::liste(&geantwortet, "values");
        if !daten::wahr(&geantwortet, "isLast")
            && daten::wert(&geantwortet, "isLast").is_some()
            && !eintraege.is_empty()
        {
            text.push_str("\n… es gibt weitere Filter.");
        }
        Ok(antwort(text))
    }

    async fn filter(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: FilterParam = argumente(roh)?;
        let site = match self.waehlen(p.verbindung.as_deref()) {
            Ok(site) => site,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let id = pflicht(&p.id, "id")?;
        let client = client_or_mcp(&site)?;
        let geantwortet = client
            .json(&Anfrage::holen(format!("filter/{}", pfadwert(id, "id")?)))
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::filter(
            &geantwortet,
            zeichen(p.max_zeichen),
        )))
    }

    async fn verknuepfungsarten(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: VerbindungParam = argumente(roh)?;
        let site = match self.waehlen(p.verbindung.as_deref()) {
            Ok(site) => site,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let client = client_or_mcp(&site)?;
        let geantwortet = client
            .json(&Anfrage::holen("issueLinkType"))
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::verknuepfungsarten(&geantwortet)))
    }

    async fn person_suchen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: PersonParam = argumente(roh)?;
        let site = match self.waehlen(p.verbindung.as_deref()) {
            Ok(site) => site,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let suche = pflicht(&p.suche, "suche")?;
        let anzahl = anzahl_der_liste(p.anzahl);
        let client = client_or_mcp(&site)?;
        let geantwortet = client
            .json(
                &Anfrage::holen("user/search")
                    .mit_abfrage("query", suche)
                    .mit_abfrage("maxResults", anzahl.to_string()),
            )
            .await
            .map_err(fehler_zu_mcp)?;
        let mut text = format::personen(&geantwortet);
        if daten::liste(&geantwortet, "").len() >= anzahl {
            text.push_str("\n… möglicherweise weitere Personen (Suche genauer fassen).");
        }
        Ok(antwort(text))
    }

    // ---------------------------------------------------------- schreibend

    async fn anlegen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: AnlegenParam = argumente(roh)?;
        let site = match self.schreiben_erlaubt(p.verbindung.as_deref()) {
            Ok(site) => site,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let projekt = pflicht(&p.projekt, "projekt")?;
        let typ = pflicht(&p.vorgangstyp, "vorgangstyp")?;
        let titel = pflicht(&p.titel, "titel")?;
        let client = client_or_mcp(&site)?;

        // Freie Felder zuerst: sie dürfen die Felder mit eigenem Parameter
        // nicht doppelt setzen.
        let mut felder = serde_json::Map::new();
        for (name, wert) in &p.felder {
            let name = pflicht(name, "felder")?;
            let eigen = match name.to_ascii_lowercase().as_str() {
                "project" => Some("`projekt`"),
                "issuetype" => Some("`vorgangstyp`"),
                "summary" => Some("`titel`"),
                "description" => Some("`beschreibung`"),
                "assignee" => Some("`zugewiesen`"),
                "parent" => Some("`eltern`"),
                _ => None,
            };
            if let Some(eigen) = eigen {
                return Err(McpError::invalid_params(
                    format!(
                        "`{name}` ist doppelt angegeben – dafür gibt es {eigen}. Bitte nur eines \
                         von beiden nennen."
                    ),
                    None,
                ));
            }
            felder.insert(name.to_string(), wert.clone());
        }

        // Den Projektschlüssel vom Server lesen und genau ihn senden.
        let projekt_key = match self.projekt_lesen(&client, &site, projekt).await {
            Ok(key) => key,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        felder.insert("project".into(), json!({ "key": projekt_key }));
        felder.insert(
            "issuetype".into(),
            if typ.chars().all(|zeichen| zeichen.is_ascii_digit()) {
                json!({ "id": typ })
            } else {
                json!({ "name": typ })
            },
        );
        felder.insert("summary".into(), json!(titel));
        if let Some(beschreibung) = beschreibung(&p.beschreibung)? {
            felder.insert("description".into(), beschreibung);
        }
        if let Some(eltern) = p
            .eltern
            .as_deref()
            .map(str::trim)
            .filter(|key| !key.is_empty())
        {
            // Ein Elternvorgang außerhalb von `nur` ist nicht erlaubt.
            if let Err(ohne) = self.nur_vorgang(&client, &site, eltern).await {
                return ohne.in_ergebnis();
            }
            felder.insert("parent".into(), json!({ "key": eltern }));
        }
        if let Some(zugewiesen) = p
            .zugewiesen
            .as_deref()
            .map(str::trim)
            .filter(|konto| !konto.is_empty())
        {
            felder.insert("assignee".into(), json!({ "accountId": zugewiesen }));
        }

        let geantwortet = client
            .json(&Anfrage::post(
                "issue",
                json!({ "fields": felder }).to_string(),
            ))
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::angelegt(&site, &geantwortet)))
    }

    async fn aendern(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: AendernParam = argumente(roh)?;
        let site = match self.schreiben_erlaubt(p.verbindung.as_deref()) {
            Ok(site) => site,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let key = pflicht(&p.key, "key")?;
        let client = client_or_mcp(&site)?;

        if let Err(ohne) = self.nur_vorgang(&client, &site, key).await {
            return ohne.in_ergebnis();
        }

        let mut felder = serde_json::Map::new();
        for (name, wert) in &p.felder {
            let name = pflicht(name, "felder")?;
            if name.eq_ignore_ascii_case("project") {
                return Err(McpError::invalid_params(
                    "`project` lässt sich nicht ändern – bitte einen neuen Vorgang in dem \
                     gewünschten Projekt anlegen."
                        .to_string(),
                    None,
                ));
            }
            felder.insert(name.to_string(), wert.clone());
        }
        // `description` als Text wird zu ADF; ein ADF-Objekt wird geprüft.
        if let Some(wert) = felder.get("description").cloned() {
            felder.insert("description".into(), beschreibung_wert(&wert)?);
        }
        if let Some(titel) = p
            .titel
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
        {
            feld_setzen(&mut felder, "summary", json!(titel), "`titel`")?;
        }
        if let Some(wert) = beschreibung(&p.beschreibung)? {
            feld_setzen(&mut felder, "description", wert, "`beschreibung`")?;
        }
        if let Some(zugewiesen) = p
            .zugewiesen
            .as_deref()
            .map(str::trim)
            .filter(|konto| !konto.is_empty())
        {
            feld_setzen(
                &mut felder,
                "assignee",
                json!({ "accountId": zugewiesen }),
                "`zugewiesen`",
            )?;
        }
        if felder.is_empty() {
            return Err(McpError::invalid_params(
                "Bitte `felder`, `titel`, `beschreibung` oder `zugewiesen` angeben.".to_string(),
                None,
            ));
        }

        // Ein Elternvorgang muss laut Server im Feld `nur` liegen; ein Text
        // (`"ABC-1"`) wird zur Form, die Jira erwartet (`{"key": …}`).
        // Nur der **geprüfte** Schlüssel geht an Jira: eine Id (auch neben einem
        // Schlüssel) oder eine andere Schreibweise ginge an `nur` vorbei.
        if felder
            .keys()
            .any(|name| name != "parent" && name.eq_ignore_ascii_case("parent"))
        {
            return Err(McpError::invalid_params(
                "Das Feld heißt `parent` (klein geschrieben).".to_string(),
                None,
            ));
        }
        if let Some(wert) = felder.get("parent").cloned() {
            if !wert.is_null() {
                let eltern = eltern_key(&wert).ok_or_else(|| {
                    McpError::invalid_params(
                        "`parent` ist der Schlüssel des Elternvorgangs (Text oder \
                         {\"key\": …}) oder null – es wurde nichts geändert."
                            .to_string(),
                        None,
                    )
                })?;
                pfadwert(&eltern, "parent")?;
                if let Err(ohne) = self.nur_vorgang(&client, &site, &eltern).await {
                    return ohne.in_ergebnis();
                }
                felder.insert("parent".into(), json!({ "key": eltern }));
            }
        }

        let geaendert: Vec<String> = felder.keys().cloned().collect();
        client
            .anfrage(&Anfrage::put(
                format!("issue/{}", pfadwert(key, "key")?),
                json!({ "fields": felder }).to_string(),
            ))
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::geaendert(key, &geaendert)))
    }

    async fn uebergang(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: UebergangParam = argumente(roh)?;
        let site = match self.schreiben_erlaubt(p.verbindung.as_deref()) {
            Ok(site) => site,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let key = pflicht(&p.key, "key")?;
        let gewaehlt = pflicht(&p.uebergang, "uebergang")?;
        let client = client_or_mcp(&site)?;
        let pfad = pfadwert(key, "key")?;

        if let Err(ohne) = self.nur_vorgang(&client, &site, key).await {
            return ohne.in_ergebnis();
        }

        // Nur Übergänge ausführen, die Jira für diesen Vorgang anbietet.
        let liste = client
            .json(&Anfrage::holen(format!("issue/{pfad}/transitions")))
            .await
            .map_err(fehler_zu_mcp)?;
        let angeboten = daten::liste(&liste, "transitions");
        let (id, name) = match uebergang::waehlen(angeboten, gewaehlt) {
            Wahl::Eindeutig(gefunden) => (
                daten::text_oder(gefunden, "id", gewaehlt),
                daten::text_oder(gefunden, "name", gewaehlt),
            ),
            Wahl::Mehrdeutig(kandidaten) => {
                return Ok(antwort(uebergang::mehrdeutig(key, gewaehlt, &kandidaten)))
            }
            Wahl::Unbekannt => return Ok(antwort(uebergang::unbekannt(key, gewaehlt, angeboten))),
        };

        let mut rumpf = json!({ "transition": { "id": id } });
        if let Some(kommentar) = p
            .kommentar
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
        {
            let rumpf_adf = adf::text_zu_adf(kommentar);
            adf::pruefen(&rumpf_adf).map_err(|fehler| {
                McpError::invalid_params(
                    format!("Der Kommentar taugt nicht als ADF: {fehler}"),
                    None,
                )
            })?;
            rumpf["update"] = json!({ "comment": [ { "add": { "body": rumpf_adf } } ] });
        }

        client
            .anfrage(&Anfrage::post(
                format!("issue/{pfad}/transitions"),
                rumpf.to_string(),
            ))
            .await
            .map_err(fehler_zu_mcp)?;

        // Den neuen Status am Vorgang lesen – nicht raten.
        let neuer_status = match client
            .json(&Anfrage::holen(format!("issue/{pfad}")).mit_abfrage("fields", "status"))
            .await
        {
            Ok(gelesen) => daten::text(&gelesen, "fields.status.name"),
            Err(_) => None,
        };
        Ok(antwort(format::uebergang(
            key,
            &id,
            &name,
            neuer_status.as_deref(),
        )))
    }

    async fn kommentieren(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: KommentarParam = argumente(roh)?;
        let site = match self.schreiben_erlaubt(p.verbindung.as_deref()) {
            Ok(site) => site,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let key = pflicht(&p.key, "key")?;
        let text = pflicht(&p.text, "text")?;
        let client = client_or_mcp(&site)?;

        if let Err(ohne) = self.nur_vorgang(&client, &site, key).await {
            return ohne.in_ergebnis();
        }

        let rumpf_adf = adf::text_zu_adf(text);
        adf::pruefen(&rumpf_adf).map_err(|fehler| {
            McpError::invalid_params(format!("Der Kommentar taugt nicht als ADF: {fehler}"), None)
        })?;
        let geantwortet = client
            .json(&Anfrage::post(
                format!("issue/{}/comment", pfadwert(key, "key")?),
                json!({ "body": rumpf_adf }).to_string(),
            ))
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::kommentiert(key, &geantwortet)))
    }

    async fn verknuepfen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: VerknuepfenParam = argumente(roh)?;
        let site = match self.schreiben_erlaubt(p.verbindung.as_deref()) {
            Ok(site) => site,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let vorgang = pflicht(&p.vorgang, "vorgang")?;
        let ziel = pflicht(&p.ziel, "ziel")?;
        let art = pflicht(&p.art, "art")?;
        if vorgang.eq_ignore_ascii_case(ziel) {
            return Err(McpError::invalid_params(
                "Zwei verschiedene Vorgänge angeben – ein Vorgang lässt sich nicht mit sich \
                 selbst verknüpfen."
                    .to_string(),
                None,
            ));
        }
        let client = client_or_mcp(&site)?;

        // Beide Vorgänge gegen `nur` prüfen (Projekt jeweils vom Server).
        for key in [vorgang, ziel] {
            pfadwert(key, "vorgang/ziel")?;
            if let Err(ohne) = self.nur_vorgang(&client, &site, key).await {
                return ohne.in_ergebnis();
            }
        }

        let mut rumpf = json!({
            "type": { "name": art },
            // `vorgang` ist die outward-Seite: „vorgang <outward> ziel“.
            "outwardIssue": { "key": vorgang },
            "inwardIssue": { "key": ziel },
        });
        if let Some(kommentar) = p
            .kommentar
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
        {
            let rumpf_adf = adf::text_zu_adf(kommentar);
            adf::pruefen(&rumpf_adf).map_err(|fehler| {
                McpError::invalid_params(
                    format!("Der Kommentar taugt nicht als ADF: {fehler}"),
                    None,
                )
            })?;
            rumpf["comment"] = json!({ "body": rumpf_adf });
        }
        // Angelegt ist angelegt: ab hier zählt nur noch die Wahrheit von Jira.
        client
            .anfrage(&Anfrage::post("issueLink", rumpf.to_string()))
            .await
            .map_err(fehler_zu_mcp)?;

        // Die Verknüpfung am Vorgang lesen und so melden, wie Jira sie zeigt.
        match client
            .json(&Anfrage::holen(format!(
                "issue/{}",
                pfadwert(vorgang, "vorgang")?
            )))
            .await
        {
            Ok(gelesen) => Ok(antwort(format::verknuepft(vorgang, ziel, art, &gelesen))),
            Err(fehler) => Ok(antwort(format::verknuepft_ungelesen(
                vorgang,
                ziel,
                art,
                &fehler.to_string(),
            ))),
        }
    }
}

// ---------------------------------------------------------------- Helfer

/// Ein Werkzeug, das nichts ändert (`read_only_hint`, AGENTS.md §8).
fn lesend<P: JsonSchema + Any>(
    name: &'static str,
    titel: &'static str,
    beschreibung: &'static str,
) -> Tool {
    Tool::new(name, beschreibung, schema_for_type::<P>())
        .with_title(titel)
        .with_annotations(ToolAnnotations::from_raw(
            Some(titel.to_string()),
            Some(true),
            Some(false),
            Some(true),
            Some(false),
        ))
}

/// Ein veränderndes Werkzeug (`destructive_hint`, Aufgabe 13).
fn schreibend<P: JsonSchema + Any>(
    name: &'static str,
    titel: &'static str,
    beschreibung: &'static str,
) -> Tool {
    Tool::new(name, beschreibung, schema_for_type::<P>())
        .with_title(titel)
        .with_annotations(ToolAnnotations::from_raw(
            Some(titel.to_string()),
            Some(false),
            Some(true),
            Some(false),
            Some(false),
        ))
}

/// Die Argumente eines Aufrufs als typisierte Parameter.
fn argumente<T: DeserializeOwned>(roh: Option<JsonObject>) -> Result<T, McpError> {
    let wert = Value::Object(roh.unwrap_or_default());
    serde_json::from_value(wert).map_err(|fehler| {
        McpError::invalid_params(format!("Die Parameter passen nicht: {fehler}"), None)
    })
}

/// Ein Pflichttext, der nicht nur aus Leerraum besteht.
fn pflicht<'a>(wert: &'a str, name: &str) -> Result<&'a str, McpError> {
    let wert = wert.trim();
    if wert.is_empty() {
        return Err(McpError::invalid_params(
            format!("`{name}` darf nicht leer sein."),
            None,
        ));
    }
    Ok(wert)
}

/// Ein Pflichtwert für einen Pfad: leer, `.` und `..` werden **vor** der
/// Anfrage abgelehnt, danach wird kodiert.
fn pfadwert(wert: &str, name: &str) -> Result<String, McpError> {
    let wert = pflicht(wert, name)?;
    client::pfadwert(wert, name).map_err(fehler_zu_mcp)
}

/// Ein Feld setzen – ein doppelt angegebenes Feld wird abgelehnt.
fn feld_setzen(
    felder: &mut serde_json::Map<String, Value>,
    name: &str,
    wert: Value,
    herkunft: &str,
) -> Result<(), McpError> {
    if felder.contains_key(name) {
        return Err(McpError::invalid_params(
            format!(
                "`{name}` ist doppelt angegeben – einmal in `felder` und einmal als {herkunft}. \
                 Bitte nur eines von beiden nennen."
            ),
            None,
        ));
    }
    felder.insert(name.to_string(), wert);
    Ok(())
}

/// Die Beschreibung aus dem eigenen Parameter: Text → ADF, geprüft.
fn beschreibung(roh: &Option<String>) -> Result<Option<Value>, McpError> {
    let Some(text) = roh
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
    else {
        return Ok(None);
    };
    let adf_wert = adf::text_zu_adf(text);
    adf::pruefen(&adf_wert).map_err(|fehler| {
        McpError::invalid_params(
            format!("Die Beschreibung taugt nicht als ADF: {fehler}"),
            None,
        )
    })?;
    Ok(Some(adf_wert))
}

/// Ein `description`-Wert aus `felder`: Text wird zu ADF, ADF wird geprüft.
fn beschreibung_wert(wert: &Value) -> Result<Value, McpError> {
    let adf_wert = match wert {
        Value::String(text) => adf::text_zu_adf(text),
        andere => andere.clone(),
    };
    adf::pruefen(&adf_wert).map_err(|fehler| {
        McpError::invalid_params(format!("`description` taugt nicht als ADF: {fehler}"), None)
    })?;
    Ok(adf_wert)
}

/// Der Schlüssel eines Elternvorgangs – als Text (`ABC-1`) oder Objekt.
fn eltern_key(wert: &Value) -> Option<String> {
    match wert {
        Value::String(key) => Some(key.trim().to_string()).filter(|key| !key.is_empty()),
        Value::Object(objekt) if objekt.len() == 1 => objekt
            .get("key")
            .and_then(Value::as_str)
            .map(|key| key.trim().to_string())
            .filter(|key| !key.is_empty()),
        _ => None,
    }
}

/// Ein Client oder ein MCP-Fehler ohne Zugangsdaten.
fn client_or_mcp(site: &Site) -> Result<Client, McpError> {
    JiraServer::client(site)
}

/// Wie viele Einträge eine Liste zeigt: Standard 25, höchstens 100.
fn anzahl_der_liste(wert: Option<u32>) -> usize {
    wert.unwrap_or(format::ANZAHL_STANDARD as u32)
        .clamp(1, format::ANZAHL_MAX as u32) as usize
}

/// Wie viele Zeichen ein Text behält: Standard 4000, höchstens 100 000.
fn zeichen(wert: Option<u32>) -> usize {
    wert.unwrap_or(format::MAX_ZEICHEN as u32)
        .clamp(100, 100_000) as usize
}

/// Aus einem Jira-Fehler wird ein MCP-Fehler – mit Verbindung, nie mit Token.
fn fehler_zu_mcp(fehler: JiraFehler) -> McpError {
    let text = fehler.to_string();
    match fehler {
        JiraFehler::Anmeldung { .. }
        | JiraFehler::Rechte { .. }
        | JiraFehler::Http { .. }
        | JiraFehler::FremderHost { .. }
        | JiraFehler::Wert { .. } => McpError::invalid_params(text, None),
        _ => McpError::internal_error(text, None),
    }
}

// ------------------------------------------------------------- Parameter

/// Parameter von `jira_status`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct StatusParam {
    /// Nur diese Site zeigen. Ohne Angabe: alle.
    #[serde(default)]
    pub verbindung: Option<String>,
}

/// Parameter von `jira_testen`, `jira_projekte` und `jira_verknuepfungsarten`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct VerbindungParam {
    /// Site; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
}

/// Parameter von `jira_projekte`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ProjekteParam {
    /// Site; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Höchstens so viele Projekte (Standard 25, höchstens 100).
    #[serde(default)]
    pub anzahl: Option<u32>,
}

/// Parameter von `jira_suchen`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct SucheParam {
    /// Site; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Fertige JQL. Ersetzt `projekt`, `status`, `zugewiesen_an_mich` und `text`.
    #[serde(default)]
    pub jql: Option<String>,
    /// Projektschlüssel, z. B. `ABC`. Ohne Angabe gelten die erlaubten Projekte.
    #[serde(default)]
    pub projekt: Option<String>,
    /// Status, z. B. `In Progress`.
    #[serde(default)]
    pub status: Option<String>,
    /// Nur Vorgänge, die mir zugewiesen sind.
    #[serde(default)]
    pub zugewiesen_an_mich: Option<bool>,
    /// Freitext (`text ~`).
    #[serde(default)]
    pub text: Option<String>,
    /// Höchstens so viele Treffer (Standard 25, höchstens 100).
    #[serde(default)]
    pub anzahl: Option<u32>,
    /// Seite aus der vorigen Antwort (`nextPageToken`).
    #[serde(default)]
    pub seite: Option<String>,
}

/// Parameter von `jira_vorgang` und `jira_uebergaenge`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct VorgangParam {
    /// Site; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Vorgangsschlüssel, z. B. `ABC-1`.
    pub key: String,
    /// Höchstens so viele Zeichen je Text (Standard 4000).
    #[serde(default)]
    pub max_zeichen: Option<u32>,
}

/// Parameter der Werkzeuge, die einen Vorgang und eine Anzahl brauchen.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ListeParam {
    /// Site; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Vorgangsschlüssel, z. B. `ABC-1`.
    pub key: String,
    /// Höchstens so viele Einträge (Standard 25, höchstens 100).
    #[serde(default)]
    pub anzahl: Option<u32>,
    /// Höchstens so viele Zeichen je Text (Standard 4000).
    #[serde(default)]
    pub max_zeichen: Option<u32>,
}

/// Parameter von `jira_felder_zum_anlegen`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct FelderParam {
    /// Site; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Projektschlüssel, z. B. `ABC`.
    pub projekt: String,
    /// Vorgangstyp (Name oder ID). Ohne Angabe kommen die Typen des Projekts.
    #[serde(default)]
    pub vorgangstyp: Option<String>,
}

/// Parameter von `jira_filter_liste`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct FilterlisteParam {
    /// Site; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Teil des Filternamens.
    #[serde(default)]
    pub name: Option<String>,
    /// Höchstens so viele Filter (Standard 25, höchstens 100).
    #[serde(default)]
    pub anzahl: Option<u32>,
    /// Höchstens so viele Zeichen je Text (Standard 4000).
    #[serde(default)]
    pub max_zeichen: Option<u32>,
}

/// Parameter von `jira_filter`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct FilterParam {
    /// Site; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Filter-ID aus `jira_filter_liste`.
    pub id: String,
    /// Höchstens so viele Zeichen je Text (Standard 4000).
    #[serde(default)]
    pub max_zeichen: Option<u32>,
}

/// Parameter von `jira_person_suchen`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct PersonParam {
    /// Site; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Name oder E-Mail.
    pub suche: String,
    /// Höchstens so viele Treffer (Standard 25, höchstens 100).
    #[serde(default)]
    pub anzahl: Option<u32>,
}

/// Parameter von `jira_anlegen`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct AnlegenParam {
    /// Site; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Projektschlüssel, z. B. `ABC`.
    pub projekt: String,
    /// Vorgangstyp (Name oder ID, siehe `jira_felder_zum_anlegen`).
    pub vorgangstyp: String,
    /// Titel (Pflicht).
    pub titel: String,
    /// Beschreibung als Text; wird zu ADF.
    #[serde(default)]
    pub beschreibung: Option<String>,
    /// Elternvorgang – damit wird der neue Vorgang eine Unteraufgabe.
    #[serde(default)]
    pub eltern: Option<String>,
    /// Zugewiesenes Konto (`accountId` aus `jira_person_suchen`).
    #[serde(default)]
    pub zugewiesen: Option<String>,
    /// Weitere Felder als Paare; der Wert ist **JSON**, also auch eine Liste
    /// (`labels`) oder ein Objekt (`priority`). `project`, `issuetype`,
    /// `summary`, `description`, `assignee` und `parent` haben eigene Parameter.
    #[serde(default)]
    pub felder: std::collections::BTreeMap<String, Value>,
}

/// Parameter von `jira_aendern`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct AendernParam {
    /// Site; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Vorgangsschlüssel, z. B. `ABC-1`.
    pub key: String,
    /// Felder als Paare; der Wert ist **JSON**, also auch eine Liste (`labels`)
    /// oder ein Objekt (`priority`). `project` ist nicht änderbar; doppelt
    /// angegebene Felder (`summary` und `titel`) werden abgelehnt.
    #[serde(default)]
    pub felder: std::collections::BTreeMap<String, Value>,
    /// Kurzform für `summary`.
    #[serde(default)]
    pub titel: Option<String>,
    /// Kurzform für `description` (Text, wird zu ADF).
    #[serde(default)]
    pub beschreibung: Option<String>,
    /// Kurzform für `assignee` (`accountId`).
    #[serde(default)]
    pub zugewiesen: Option<String>,
}

/// Parameter von `jira_uebergang`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct UebergangParam {
    /// Site; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Vorgangsschlüssel, z. B. `ABC-1`.
    pub key: String,
    /// Übergangs-ID oder -Name aus `jira_uebergaenge`.
    pub uebergang: String,
    /// Kommentar, der beim Übergang mitgeschrieben wird.
    #[serde(default)]
    pub kommentar: Option<String>,
}

/// Parameter von `jira_kommentieren`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct KommentarParam {
    /// Site; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Vorgangsschlüssel, z. B. `ABC-1`.
    pub key: String,
    /// Kommentartext; wird zu ADF.
    pub text: String,
}

/// Parameter von `jira_verknuepfen`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct VerknuepfenParam {
    /// Site; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Vorgang, der die Beziehung ausspricht: er steht im **outward**-Text der
    /// Verknüpfungsart. Bei `art` = „Blocks“ gilt also „vorgang blocks ziel“.
    pub vorgang: String,
    /// Ziel der Beziehung: es steht im **inward**-Text der Verknüpfungsart
    /// (bei „Blocks“: „ziel is blocked by vorgang“).
    pub ziel: String,
    /// Verknüpfungsart (Name aus `jira_verknuepfungsarten`, z. B. `Blocks`).
    pub art: String,
    /// Kommentar, der mit der Verknüpfung geschrieben wird.
    #[serde(default)]
    pub kommentar: Option<String>,
}

// -------------------------------------------------------------- MCP-Anker

impl ServerHandler for JiraServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(quiver_tools_kern::mcp::server_info(&self.zugang))
            .with_instructions(format!(
                "Jira Cloud über die REST-API v3: Vorgänge suchen und lesen, Kommentare, \
                 Übergänge, Projekte, Filter, Personen und Verknüpfungsarten ansehen. Verändernd \
                 gehen Vorgänge anlegen und ändern, Übergänge, Kommentare und Verknüpfungen – \
                 aber nur, wenn der Schalter „schreiben“ für die Site an ist (Standard: aus). Ist \
                 er aus, antworten die verändernden Werkzeuge mit dem Hinweis und ohne Anfrage an \
                 Jira. Ist das Feld „nur“ gesetzt, sind Änderungen außerhalb dieser Projekte \
                 gesperrt; das Projekt liest das Tool vom Server. Die {MEHRFACH_TITEL} \
                 ({MEHRFACH_EINZAHL}s) legt der User im quiver-Fenster unter Tools → {TITEL} an; \
                 jedes Werkzeug nimmt `verbindung`, bei genau einer darf der Name fehlen. Fehlt \
                 die Liste, sagen die Werkzeuge das als Text."
            ))
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(self.werkzeuge()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        self.aufrufen(request.name.as_ref(), request.arguments)
            .await
            .map(CallToolResponse::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::werkzeug::Jira;
    use quiver_tools_kern::zugaenge::Zugang as Eintrag;
    use serde_json::json;

    fn eintrag(name: &str, schreiben: bool, nur: Option<&str>) -> Eintrag {
        let mut werte: std::collections::BTreeMap<String, String> = [
            ("url", "https://firma.atlassian.net"),
            ("email", "alex@example.test"),
            ("api_token", "token-4711"),
            ("schreiben", if schreiben { "an" } else { "aus" }),
        ]
        .into_iter()
        .map(|(feld, wert)| (feld.to_string(), wert.to_string()))
        .collect();
        if let Some(nur) = nur {
            werte.insert("nur".to_string(), nur.to_string());
        }
        Eintrag {
            name: name.to_string(),
            werte,
        }
    }

    fn server_mit(eintraege: Vec<Eintrag>) -> JiraServer {
        JiraServer::mit(
            Zugang::mit_umgebung(&Jira, |_| None),
            Ok(Sammlung::aus_zugaengen(eintraege).unwrap()),
        )
    }

    fn ohne_sites() -> JiraServer {
        JiraServer::mit(
            Zugang::mit_umgebung(&Jira, |_| None),
            Ok(Sammlung::default()),
        )
    }

    fn text_von(ergebnis: Ergebnis) -> String {
        let ergebnis = ergebnis.expect("unerwarteter Fehler");
        ergebnis.content[0]
            .as_text()
            .expect("kein Text")
            .text
            .clone()
    }

    fn aufruf(server: &JiraServer, name: &str, argumente: Value) -> Ergebnis {
        let laufzeit = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let objekt = argumente.as_object().cloned().unwrap_or_default();
        laufzeit.block_on(server.aufrufen(name, Some(objekt)))
    }

    #[test]
    fn alle_werkzeuge_tragen_den_richtigen_hinweis() {
        let server = ohne_sites();
        let werkzeuge = server.werkzeuge();
        assert_eq!(werkzeuge.len(), 18, "17 Werkzeuge plus jira_profile");

        let schreibend = [
            "jira_anlegen",
            "jira_aendern",
            "jira_uebergang",
            "jira_kommentieren",
            "jira_verknuepfen",
        ];

        for werkzeug in &werkzeuge {
            let hinweise = werkzeug.annotations.as_ref().expect("Hinweise fehlen");
            let name = werkzeug.name.as_ref();
            let destruktiv = schreibend.contains(&name);
            assert_eq!(hinweise.read_only_hint, Some(!destruktiv), "{name}");
            assert_eq!(hinweise.destructive_hint, Some(destruktiv), "{name}");
            if name == "jira_profile" {
                assert!(hinweise.title.is_some(), "{name} hat keinen Titel");
            } else {
                assert!(werkzeug.title.is_some(), "{name} hat keinen Titel");
            }
            let eigenschaften = &werkzeug.input_schema["properties"];
            if name != "jira_profile" {
                assert!(
                    eigenschaften.get("verbindung").is_some(),
                    "{name} kennt kein `verbindung`: {eigenschaften:?}"
                );
            }
            // Jedes Schema ist ein Objekt – sonst lehnen manche Clients die Liste ab.
            assert_eq!(
                werkzeug.input_schema["type"], "object",
                "{name}: kein Objekt-Schema"
            );
        }

        let namen: Vec<&str> = werkzeuge.iter().map(|w| w.name.as_ref()).collect();
        assert_eq!(namen[0], "jira_status");
        assert_eq!(namen[11], "jira_person_suchen");
        assert_eq!(namen[12], "jira_anlegen");
        assert_eq!(namen[16], "jira_verknuepfen");
        assert_eq!(namen[17], "jira_profile");
    }

    #[test]
    fn ohne_sites_kommt_der_hinweis_und_kein_fehler() {
        let server = ohne_sites();
        for (name, argumente) in [
            ("jira_status", json!({})),
            ("jira_testen", json!({})),
            ("jira_projekte", json!({})),
            ("jira_suchen", json!({ "text": "Login" })),
            ("jira_vorgang", json!({ "key": "ABC-1" })),
            (
                "jira_anlegen",
                json!({ "projekt": "ABC", "vorgangstyp": "Bug", "titel": "x" }),
            ),
        ] {
            let text = text_von(aufruf(&server, name, argumente));
            assert!(text.contains("Zugang fehlt"), "{name}: {text}");
            assert!(text.contains("Tools → Jira"), "{name}: {text}");
        }
    }

    #[test]
    fn das_profil_nennt_die_sites_ohne_token() {
        let server = server_mit(vec![eintrag("firma", false, Some("ABC"))]);
        let text = text_von(aufruf(&server, "jira_profile", json!({})));
        assert!(text.contains("firma"), "{text}");
        assert!(text.contains("nur ABC"), "{text}");
        assert!(text.contains("Schreiben aus"), "{text}");
        assert!(!text.contains("token-4711"), "{text}");
        assert!(!text.contains("alex@example.test"), "{text}");
    }

    #[test]
    fn mehrere_sites_verlangen_einen_namen() {
        let server = server_mit(vec![
            eintrag("firma", false, None),
            eintrag("kunde", false, None),
        ]);
        let text = text_von(aufruf(&server, "jira_status", json!({})));
        assert!(text.contains("firma"), "{text}");
        assert!(text.contains("kunde"), "{text}");

        let fehler = aufruf(&server, "jira_projekte", json!({})).unwrap_err();
        assert!(
            fehler.message.contains("mehrere Sites"),
            "{}",
            fehler.message
        );

        let fehler = aufruf(
            &server,
            "jira_projekte",
            json!({ "verbindung": "gibtsnicht" }),
        )
        .unwrap_err();
        assert!(fehler.message.contains("gibtsnicht"), "{}", fehler.message);
    }

    #[test]
    fn schreiben_aus_antwortet_bei_allen_fuenf_ohne_anfrage() {
        let server = server_mit(vec![eintrag("firma", false, None)]);
        for (name, argumente) in [
            (
                "jira_anlegen",
                json!({ "projekt": "ABC", "vorgangstyp": "Bug", "titel": "x" }),
            ),
            ("jira_aendern", json!({ "key": "ABC-1", "titel": "neu" })),
            (
                "jira_uebergang",
                json!({ "key": "ABC-1", "uebergang": "11" }),
            ),
            (
                "jira_kommentieren",
                json!({ "key": "ABC-1", "text": "hallo" }),
            ),
            (
                "jira_verknuepfen",
                json!({ "vorgang": "ABC-1", "ziel": "ABC-2", "art": "Blocks" }),
            ),
        ] {
            let text = text_von(aufruf(&server, name, argumente));
            assert!(
                text.contains("Schreiben ist für firma aus"),
                "{name}: {text}"
            );
        }
    }

    #[test]
    fn leere_pflichtparameter_und_unbekannte_werkzeuge_sind_parameterfehler() {
        let server = server_mit(vec![eintrag("firma", true, None)]);
        let fehler = aufruf(
            &server,
            "jira_anlegen",
            json!({ "projekt": "ABC", "vorgangstyp": "  ", "titel": "x" }),
        )
        .unwrap_err();
        assert!(fehler.message.contains("vorgangstyp"), "{}", fehler.message);

        let fehler = aufruf(&server, "jira_gibtsnicht", json!({})).unwrap_err();
        assert!(
            fehler.message.contains("jira_gibtsnicht"),
            "{}",
            fehler.message
        );

        let fehler = aufruf(&server, "jira_vorgang", json!({ "key": 1 })).unwrap_err();
        assert!(fehler.message.contains("Parameter"), "{}", fehler.message);

        let fehler = aufruf(&server, "jira_aendern", json!({ "key": "ABC-1" })).unwrap_err();
        assert!(fehler.message.contains("felder"), "{}", fehler.message);

        let fehler = aufruf(
            &server,
            "jira_verknuepfen",
            json!({ "vorgang": "ABC-1", "ziel": "ABC-1", "art": "Blocks" }),
        )
        .unwrap_err();
        assert!(fehler.message.contains("selbst"), "{}", fehler.message);
    }

    #[test]
    fn die_helfer_begrenzen_anzahl_und_zeichen() {
        assert_eq!(anzahl_der_liste(None), format::ANZAHL_STANDARD);
        assert_eq!(anzahl_der_liste(Some(0)), 1);
        assert_eq!(anzahl_der_liste(Some(10)), 10);
        assert_eq!(anzahl_der_liste(Some(5000)), format::ANZAHL_MAX);

        assert_eq!(zeichen(None), format::MAX_ZEICHEN);
        assert_eq!(zeichen(Some(5000)), 5000);
        assert_eq!(zeichen(Some(1)), 100);
    }

    #[test]
    fn eine_kaputte_site_liste_ist_ein_parameterfehler() {
        let server = JiraServer::mit(
            Zugang::mit_umgebung(&Jira, |_| None),
            Sammlung::mit_umgebung(|_| Some("[{\"name\":".to_string())),
        );
        let fehler = aufruf(&server, "jira_status", json!({})).unwrap_err();
        assert!(
            fehler.message.contains("QUIVER_ZUGAENGE"),
            "{}",
            fehler.message
        );
    }
}
