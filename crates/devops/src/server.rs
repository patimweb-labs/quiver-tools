// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der MCP-Server: 26 Werkzeuge über stdio (Aufgabe 09).
//!
//! Achtzehn lesende Werkzeuge (`read_only_hint`) und acht verändernde
//! (`destructive_hint`, brauchen den Schalter `schreiben`) – dazu aus dem Kern
//! `devops_profile`. Die Organisationen kommen aus `QUIVER_ZUGAENGE` (mehrere je
//! Werkzeug). Fehlt die Liste, antwortet jedes Werkzeug mit dem Hinweis auf das
//! quiver-Fenster – **kein** MCP-Fehler. Jedes Werkzeug nimmt `verbindung`; bei
//! genau einer Organisation darf der Name fehlen.
//!
//! Drei Regeln stehen **vor** jeder Anfrage (Aufgabe 09):
//!
//! | Regel | Wirkung |
//! |---|---|
//! | `schreiben` je Organisation (Standard aus) | verändernde Werkzeuge antworten ohne Anfrage |
//! | Projekt aus dem Zugang oder je Aufruf | fehlt beides, kommt ein Hinweis ohne Anfrage |
//! | PAT nur in der Kopfzeile | keine Antwort, kein Fehler und kein Log nennt ihn |

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

use crate::client::{self, verbindungsdaten, Anfrage, Client, Methode, API_KOMMENTARE};
use crate::daten;
use crate::fehler::DevopsFehler;
use crate::format;
use crate::organisationen::{AuswahlFehler, Organisation, SammelFehler, Sammlung};
use crate::werkzeug::{MEHRFACH_EINZAHL, MEHRFACH_TITEL, TITEL};
use crate::wiql;

/// `GET _apis/projects`.
const PROJEKTE: &str = "_apis/projects";

/// Artikel-Link einer Pull-Request-Verknüpfung am Arbeitsitem.
const ARTEFAKT_LINK: &str = "ArtifactLink";

/// Was ein Werkzeug zurückgibt.
type Ergebnis = Result<CallToolResult, McpError>;

/// Der DevOps-Server: die Organisationen und die Werkzeugliste.
pub struct DevopsServer {
    /// Der Zugang des Kerns; bei `mehrfach` ohne Felder, aber mit Titel.
    zugang: Zugang,
    organisationen: Result<Sammlung, SammelFehler>,
}

impl DevopsServer {
    /// Der Normalfall: alles aus der Umgebung (`QUIVER_ZUGAENGE`).
    pub fn neu(zugang: Zugang) -> Self {
        DevopsServer {
            zugang,
            organisationen: Sammlung::aus_umgebung(),
        }
    }

    /// Feste Teile (Tests und Sonderfälle), ohne Prozessumgebung.
    pub fn mit(zugang: Zugang, organisationen: Result<Sammlung, SammelFehler>) -> Self {
        DevopsServer {
            zugang,
            organisationen,
        }
    }

    /// Alle Werkzeuge, wie `tools/list` sie meldet.
    pub fn werkzeuge(&self) -> Vec<Tool> {
        let mut alle = self.lesende_werkzeuge();
        alle.extend(self.schreibende_werkzeuge());
        // `devops_profile` kommt aus dem Kern: bei jedem Tool dasselbe lesende
        // Werkzeug.
        alle.push(profile_tool(&self.zugang));
        alle
    }

    /// Die achtzehn lesenden Werkzeuge.
    fn lesende_werkzeuge(&self) -> Vec<Tool> {
        vec![
            lesend::<StatusParam>(
                "devops_status",
                "Organisationen",
                "Listet die im quiver-Fenster eingetragenen Azure-DevOps-Organisationen: Name, \
                 Adresse, Standardprojekt und ob „schreiben“ an ist. Öffnet keine Verbindung, \
                 zeigt keinen PAT.",
            ),
            lesend::<VerbindungParam>(
                "devops_testen",
                "Anmeldung prüfen",
                "Meldet sich mit dem PAT an und gibt zurück, wer angemeldet ist. Ohne \
                 `verbindung` prüft es alle Organisationen. Ein abgelehnter PAT wird nicht \
                 wiederholt.",
            ),
            lesend::<VerbindungParam>(
                "devops_projekte",
                "Projekte",
                "Listet die Projekte der Organisation: Name, Zustand, Sichtbarkeit.",
            ),
            lesend::<SucheParam>(
                "devops_workitems_suchen",
                "Arbeitsitems suchen",
                "Sucht Arbeitsitems. Entweder mit fertigem `wiql` oder bequem über `text`, \
                 `status`, `typ` und `zugewiesen_an_mich`; daraus wird die WIQL gebaut. \
                 `top` ist standardmäßig 50 und höchstens 200. Gibt ID, Typ, Status, Titel und \
                 Zuweisung zurück.",
            ),
            lesend::<ItemParam>(
                "devops_workitem",
                "Arbeitsitem",
                "Ein Arbeitsitem mit Feldern, Beschreibung, Akzeptanzkriterien und Links. Lange \
                 Texte werden über `max_zeichen` gekürzt (Standard 4000).",
            ),
            lesend::<ItemParam>(
                "devops_workitem_kommentare",
                "Kommentare",
                "Alle Kommentare eines Arbeitsitems mit Autor, Datum und Text (gekürzt über \
                 `max_zeichen`, Standard 4000).",
            ),
            lesend::<ProjektParam>(
                "devops_workitem_typen",
                "Arbeitsitem-Typen",
                "Die Arbeitsitem-Typen des Projekts mit Kurzname und den erlaubten Zuständen. Vor \
                 dem Anlegen fragen, welche Typen es gibt.",
            ),
            lesend::<IterationenParam>(
                "devops_iterationen",
                "Iterationen",
                "Die Iterationen (Sprints) eines Teams mit Pfad, Zeitraum und Zeitfenster. Mit \
                 `aktuell` nur die laufende. `team` ist Pflicht.",
            ),
            lesend::<IterationsitemsParam>(
                "devops_iteration_workitems",
                "Arbeitsitems einer Iteration",
                "Die Arbeitsitems einer Iteration mit ID, Typ, Status, Titel und Zuweisung.",
            ),
            lesend::<ProjektParam>(
                "devops_repos",
                "Repositories",
                "Die Git-Repositories des Projekts mit Standardbranch und Größe.",
            ),
            lesend::<BranchesParam>(
                "devops_branches",
                "Branches",
                "Die Branches eines Repositories mit kurzem SHA und Ersteller.",
            ),
            lesend::<PullRequestsParam>(
                "devops_pull_requests",
                "Pull Requests",
                "Pull Requests des Projekts (oder eines Repositories). `status` ist \
                 standardmäßig active; `ersteller` filtert nach dem Anzeigenamen.",
            ),
            lesend::<PullRequestParam>(
                "devops_pull_request",
                "Pull Request",
                "Ein Pull Request mit Beschreibung, Prüfern und Stimmen, verknüpften \
                 Arbeitsitems und höchstens 20 Commits.",
            ),
            lesend::<PullRequestParam>(
                "devops_pull_request_threads",
                "Pull-Request-Threads",
                "Die Kommentar-Threads eines Pull Requests ohne die System-Threads von DevOps \
                 (Abstimmungen, Pushes).",
            ),
            lesend::<ProjektParam>(
                "devops_pipelines",
                "Pipelines",
                "Die YAML-Pipelines des Projekts mit Name und Ordner.",
            ),
            lesend::<LaeufeParam>(
                "devops_laeufe",
                "Läufe",
                "Die letzten Läufe (Builds) eines Projekts, optional je Pipeline und Branch. \
                 `top` ist standardmäßig 10.",
            ),
            lesend::<LaufParam>(
                "devops_lauf",
                "Lauf",
                "Ein Lauf mit Zustand, Zeiten und der Zeitachse; genannt werden die \
                 fehlgeschlagenen Schritte samt Log-Nummer.",
            ),
            lesend::<LogParam>(
                "devops_lauf_log",
                "Lauf-Log",
                "Das Ende des Logs eines Schritts, standardmäßig die letzten 200 Zeilen. Die \
                 Log-Nummer steht in `devops_lauf`.",
            ),
        ]
    }

    /// Die acht verändernden Werkzeuge (brauchen `schreiben`).
    fn schreibende_werkzeuge(&self) -> Vec<Tool> {
        vec![
            schreibend::<AnlegenParam>(
                "devops_workitem_anlegen",
                "Arbeitsitem anlegen",
                "Legt ein Arbeitsitem an: Typ, Titel, optional Beschreibung, Zuweisung, Bereich, \
                 Iteration und ein Eltern-Arbeitsitem. Braucht den Schalter „schreiben“.",
            ),
            schreibend::<AendernParam>(
                "devops_workitem_aendern",
                "Arbeitsitem ändern",
                "Ändert Felder eines Arbeitsitems: beliebige Felder als Liste `{feld, wert}` oder \
                 bequem `status`, `titel`, `zugewiesen`.",
            ),
            schreibend::<KommentarParam>(
                "devops_workitem_kommentieren",
                "Arbeitsitem kommentieren",
                "Schreibt einen Kommentar an ein Arbeitsitem.",
            ),
            schreibend::<VerknuepfenParam>(
                "devops_workitem_verknuepfen",
                "Arbeitsitems verknüpfen",
                "Verknüpft zwei Arbeitsitems (art `eltern`, `kind` oder `verwandt`) oder entfernt \
                 den Link wieder (`entfernen`).",
            ),
            schreibend::<PrAnlegenParam>(
                "devops_pull_request_anlegen",
                "Pull Request anlegen",
                "Legt einen Pull Request an: Quelle, Ziel, Titel, Beschreibung, Entwurf und \
                 optional Arbeitsitems, die verknüpft werden.",
            ),
            schreibend::<PrKommentarParam>(
                "devops_pull_request_kommentieren",
                "Pull Request kommentieren",
                "Schreibt einen neuen Kommentar-Thread an einen Pull Request oder antwortet mit \
                 `thread` in einem bestehenden Thread.",
            ),
            schreibend::<AbstimmenParam>(
                "devops_pull_request_abstimmen",
                "Pull Request abstimmen",
                "Stimmt über einen Pull Request ab: `approve`, `approve-with-suggestions`, `wait`, \
                 `reject` oder `reset`. Die Stimme gilt für den angemeldeten Benutzer.",
            ),
            schreibend::<PipelineParam>(
                "devops_pipeline_starten",
                "Pipeline starten",
                "Startet eine Pipeline auf einem Branch, mit Variablen und Vorlagenparametern.",
            ),
        ]
    }

    /// Ein Werkzeug aufrufen. Auch der Weg, den die Tests nehmen.
    pub async fn aufrufen(&self, name: &str, argumente: Option<JsonObject>) -> Ergebnis {
        match name {
            "devops_status" => self.status(argumente),
            "devops_testen" => self.testen(argumente).await,
            "devops_projekte" => self.projekte(argumente).await,
            "devops_workitems_suchen" => self.workitems_suchen(argumente).await,
            "devops_workitem" => self.workitem(argumente).await,
            "devops_workitem_kommentare" => self.workitem_kommentare(argumente).await,
            "devops_workitem_typen" => self.workitem_typen(argumente).await,
            "devops_iterationen" => self.iterationen(argumente).await,
            "devops_iteration_workitems" => self.iteration_workitems(argumente).await,
            "devops_repos" => self.repos(argumente).await,
            "devops_branches" => self.branches(argumente).await,
            "devops_pull_requests" => self.pull_requests(argumente).await,
            "devops_pull_request" => self.pull_request(argumente).await,
            "devops_pull_request_threads" => self.pull_request_threads(argumente).await,
            "devops_pipelines" => self.pipelines(argumente).await,
            "devops_laeufe" => self.laeufe(argumente).await,
            "devops_lauf" => self.lauf(argumente).await,
            "devops_lauf_log" => self.lauf_log(argumente).await,
            "devops_workitem_anlegen" => self.workitem_anlegen(argumente).await,
            "devops_workitem_aendern" => self.workitem_aendern(argumente).await,
            "devops_workitem_kommentieren" => self.workitem_kommentieren(argumente).await,
            "devops_workitem_verknuepfen" => self.workitem_verknuepfen(argumente).await,
            "devops_pull_request_anlegen" => self.pull_request_anlegen(argumente).await,
            "devops_pull_request_kommentieren" => self.pull_request_kommentieren(argumente).await,
            "devops_pull_request_abstimmen" => self.pull_request_abstimmen(argumente).await,
            "devops_pipeline_starten" => self.pipeline_starten(argumente).await,
            "devops_profile" => self.profile(argumente),
            sonst => Err(McpError::invalid_params(
                format!("Unbekanntes Werkzeug „{sonst}“."),
                None,
            )),
        }
    }

    /// `devops_profile`: bei einer Organisationsliste sind „die Angaben“ die
    /// Organisationen.
    fn profile(&self, roh: Option<JsonObject>) -> Ergebnis {
        self.status(roh)
    }

    /// Die Liste der Organisationen oder ein Fehler ohne ihren Inhalt.
    fn sammlung(&self) -> Result<&Sammlung, McpError> {
        self.organisationen.as_ref().map_err(|fehler| {
            McpError::invalid_params(
                format!("{fehler} – bitte im quiver-Fenster unter Tools → {TITEL} prüfen."),
                None,
            )
        })
    }

    /// Die gewünschte Organisation; fehlt die Liste, kommt der Hinweis.
    fn waehlen(&self, name: Option<&str>) -> Result<Organisation, OhneProfil> {
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

    /// Die Organisation – aber nur, wenn der Schalter „schreiben“ an ist.
    ///
    /// Ist er aus, kommt der Hinweis **ohne** Anfrage an Azure DevOps.
    fn schreiben_erlaubt(&self, name: Option<&str>) -> Result<Organisation, OhneProfil> {
        let organisation = self.waehlen(name)?;
        if !organisation.schreiben_erlaubt() {
            return Err(OhneProfil::Hinweis(antwort(format::schreiben_aus(
                &organisation.name,
            ))));
        }
        Ok(organisation)
    }

    /// Das Projekt: aus dem Aufruf, sonst aus dem Zugang, sonst ein Hinweis.
    fn projekt(
        &self,
        organisation: &Organisation,
        genannt: Option<&str>,
    ) -> Result<String, OhneProfil> {
        organisation
            .projekt(genannt)
            .ok_or_else(|| OhneProfil::Hinweis(antwort(format::projekt_fehlt(&organisation.name))))
    }

    /// Ein Client für diese Organisation, oder ein MCP-Fehler ohne PAT.
    fn client(organisation: &Organisation) -> Result<Client, McpError> {
        Client::neu(organisation).map_err(fehler_zu_mcp)
    }

    /// Die Felder mehrerer Arbeitsitems holen (`workitemsbatch`).
    async fn batch(
        &self,
        client: &Client,
        projekt: &str,
        ids: &[i64],
    ) -> Result<Value, DevopsFehler> {
        if ids.is_empty() {
            return Ok(json!({ "count": 0, "value": [] }));
        }
        let rumpf = json!({
            "ids": ids,
            "fields": wiql::FELDER,
            "errorPolicy": "omit",
        });
        client
            .json(&Anfrage::post(
                format!("{}/_apis/wit/workitemsbatch", client::kodieren(projekt)),
                rumpf.to_string(),
            ))
            .await
    }

    // ------------------------------------------------------------- lesend

    fn status(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: StatusParam = argumente(roh)?;
        let sammlung = self.sammlung()?;
        if sammlung.ist_leer() {
            return Ok(antwort(hinweis_zugang(TITEL)));
        }
        let gewaehlt: Vec<Organisation> = match p.verbindung.as_deref() {
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
        // Ohne Namen: alle Organisationen prüfen. Mit Namen: genau die eine.
        let organisationen: Vec<Organisation> = match p.verbindung.as_deref() {
            Some(name) => vec![sammlung
                .waehlen(Some(name))
                .map_err(|fehler| McpError::invalid_params(fehler.to_string(), None))?
                .clone()],
            None => sammlung.alle().to_vec(),
        };

        let mut zeilen = Vec::new();
        for organisation in &organisationen {
            let client = Self::client(organisation)?;
            // Der Endpunkt ist Vorschau: der Aufruf kommt aus dem Client, damit
            // die `api-version` nirgends vergessen werden kann.
            let geantwortet = client
                .json(&verbindungsdaten())
                .await
                .map_err(fehler_zu_mcp)?;
            zeilen.push(format::testen(organisation, &geantwortet));
        }
        Ok(antwort(zeilen.join("\n\n")))
    }

    async fn projekte(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: VerbindungParam = argumente(roh)?;
        let organisation = match self.waehlen(p.verbindung.as_deref()) {
            Ok(organisation) => organisation,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let client = Self::client(&organisation)?;
        let geantwortet = client
            .json(&Anfrage::holen(PROJEKTE))
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::projekte(&organisation.name, &geantwortet)))
    }

    async fn workitems_suchen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: SucheParam = argumente(roh)?;
        let organisation = match self.waehlen(p.verbindung.as_deref()) {
            Ok(organisation) => organisation,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let projekt = match self.projekt(&organisation, p.projekt.as_deref()) {
            Ok(projekt) => projekt,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let top = wiql::top(p.top);
        let abfrage = match p
            .wiql
            .as_deref()
            .map(str::trim)
            .filter(|wiql| !wiql.is_empty())
        {
            Some(wiql) => wiql.to_string(),
            None => wiql::bauen(
                &projekt,
                &wiql::Komfort {
                    text: p.text.as_deref(),
                    zugewiesen_an_mich: p.zugewiesen_an_mich.unwrap_or(false),
                    status: p.status.as_deref(),
                    typ: p.typ.as_deref(),
                },
            ),
        };

        let client = Self::client(&organisation)?;
        let treffer = client
            .json(
                &Anfrage::post(
                    format!("{}/_apis/wit/wiql", client::kodieren(&projekt)),
                    json!({ "query": abfrage }).to_string(),
                )
                .mit_abfrage("$top", top.to_string()),
            )
            .await
            .map_err(fehler_zu_mcp)?;

        let ids: Vec<i64> = daten::liste(&treffer, "workItems")
            .iter()
            .filter_map(|eintrag| daten::zahl(eintrag, "id"))
            .take(top)
            .collect();
        let items = self
            .batch(&client, &projekt, &ids)
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::arbeitsitems(
            &format!("Arbeitsitems in {projekt}"),
            &items,
        )))
    }

    async fn workitem(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: ItemParam = argumente(roh)?;
        let organisation = match self.waehlen(p.verbindung.as_deref()) {
            Ok(organisation) => organisation,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let client = Self::client(&organisation)?;
        let geantwortet = client
            .json(
                &Anfrage::holen(format!("_apis/wit/workitems/{}", p.id))
                    .mit_abfrage("$expand", "relations"),
            )
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::arbeitsitem(
            &geantwortet,
            zeichen(p.max_zeichen),
        )))
    }

    async fn workitem_kommentare(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: ItemParam = argumente(roh)?;
        let organisation = match self.waehlen(p.verbindung.as_deref()) {
            Ok(organisation) => organisation,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let projekt = match self.projekt(&organisation, p.projekt.as_deref()) {
            Ok(projekt) => projekt,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let client = Self::client(&organisation)?;
        let geantwortet = client
            .json(
                &Anfrage::holen(format!(
                    "{}/_apis/wit/workItems/{}/comments",
                    client::kodieren(&projekt),
                    p.id
                ))
                .mit_abfrage("api-version", API_KOMMENTARE)
                .mit_abfrage("$top", "200"),
            )
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::kommentare(
            p.id,
            &geantwortet,
            zeichen(p.max_zeichen),
        )))
    }

    async fn workitem_typen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: ProjektParam = argumente(roh)?;
        let organisation = match self.waehlen(p.verbindung.as_deref()) {
            Ok(organisation) => organisation,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let projekt = match self.projekt(&organisation, p.projekt.as_deref()) {
            Ok(projekt) => projekt,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let client = Self::client(&organisation)?;
        let geantwortet = client
            .json(&Anfrage::holen(format!(
                "{}/_apis/wit/workitemtypes",
                client::kodieren(&projekt)
            )))
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::arbeitsitem_typen(&geantwortet)))
    }

    async fn iterationen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: IterationenParam = argumente(roh)?;
        let organisation = match self.waehlen(p.verbindung.as_deref()) {
            Ok(organisation) => organisation,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let projekt = match self.projekt(&organisation, p.projekt.as_deref()) {
            Ok(projekt) => projekt,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let Some(team) = team(&p.team) else {
            return Ok(antwort(format::team_fehlt()));
        };

        let mut anfrage = Anfrage::holen(format!(
            "{}/{}/_apis/work/teamsettings/iterations",
            client::kodieren(&projekt),
            client::kodieren(&team)
        ));
        if p.aktuell.unwrap_or(false) {
            anfrage = anfrage.mit_abfrage("$timeframe", "current");
        }
        let client = Self::client(&organisation)?;
        let geantwortet = client.json(&anfrage).await.map_err(fehler_zu_mcp)?;
        Ok(antwort(format::iterationen(&geantwortet)))
    }

    async fn iteration_workitems(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: IterationsitemsParam = argumente(roh)?;
        let organisation = match self.waehlen(p.verbindung.as_deref()) {
            Ok(organisation) => organisation,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let projekt = match self.projekt(&organisation, p.projekt.as_deref()) {
            Ok(projekt) => projekt,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let Some(team) = team(&p.team) else {
            return Ok(antwort(format::team_fehlt()));
        };
        let iteration = pflicht(&p.iteration, "iteration")?;

        let client = Self::client(&organisation)?;
        let geantwortet = client
            .json(&Anfrage::holen(format!(
                "{}/{}/_apis/work/teamsettings/iterations/{}/workitems",
                client::kodieren(&projekt),
                client::kodieren(&team),
                client::kodieren(iteration)
            )))
            .await
            .map_err(fehler_zu_mcp)?;
        let ids: Vec<i64> = daten::liste(&geantwortet, "workItemRelations")
            .iter()
            .filter_map(|relation| daten::zahl(relation, "target.id"))
            .collect();
        let items = self
            .batch(&client, &projekt, &ids)
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::arbeitsitems(
            &format!("Arbeitsitems der Iteration {iteration} ({team})"),
            &items,
        )))
    }

    async fn repos(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: ProjektParam = argumente(roh)?;
        let organisation = match self.waehlen(p.verbindung.as_deref()) {
            Ok(organisation) => organisation,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let projekt = match self.projekt(&organisation, p.projekt.as_deref()) {
            Ok(projekt) => projekt,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let client = Self::client(&organisation)?;
        let geantwortet = client
            .json(&Anfrage::holen(format!(
                "{}/_apis/git/repositories",
                client::kodieren(&projekt)
            )))
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::repos(&geantwortet)))
    }

    async fn branches(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: BranchesParam = argumente(roh)?;
        let organisation = match self.waehlen(p.verbindung.as_deref()) {
            Ok(organisation) => organisation,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let projekt = match self.projekt(&organisation, p.projekt.as_deref()) {
            Ok(projekt) => projekt,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let repo = pflicht(&p.repo, "repo")?;
        let client = Self::client(&organisation)?;
        let geantwortet = client
            .json(
                &Anfrage::holen(format!(
                    "{}/_apis/git/repositories/{}/refs",
                    client::kodieren(&projekt),
                    client::kodieren(repo)
                ))
                .mit_abfrage("filter", "heads/"),
            )
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::branches(repo, &geantwortet)))
    }

    async fn pull_requests(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: PullRequestsParam = argumente(roh)?;
        let organisation = match self.waehlen(p.verbindung.as_deref()) {
            Ok(organisation) => organisation,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let projekt = match self.projekt(&organisation, p.projekt.as_deref()) {
            Ok(projekt) => projekt,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let status = p
            .status
            .as_deref()
            .map(str::trim)
            .filter(|status| !status.is_empty())
            .unwrap_or("active");

        // `ersteller` filtert lokal: DevOps kennt in der Abfrage nur eine
        // Ersteller-ID, keinen Anzeigenamen.
        let anfrage = match p.repo.as_deref().map(str::trim).filter(|r| !r.is_empty()) {
            Some(repo) => Anfrage::holen(format!(
                "{}/_apis/git/repositories/{}/pullrequests",
                client::kodieren(&projekt),
                client::kodieren(repo)
            )),
            None => Anfrage::holen(format!(
                "{}/_apis/git/pullrequests",
                client::kodieren(&projekt)
            )),
        }
        .mit_abfrage("searchCriteria.status", status)
        .mit_abfrage("$top", p.top.unwrap_or(100).clamp(1, 500).to_string());

        let client = Self::client(&organisation)?;
        let mut geantwortet = client.json(&anfrage).await.map_err(fehler_zu_mcp)?;
        if let Some(ersteller) = p.ersteller.as_deref().map(str::trim) {
            if !ersteller.is_empty() {
                geantwortet = filtern_nach_ersteller(&geantwortet, ersteller);
            }
        }
        Ok(antwort(format::pull_requests(&geantwortet)))
    }

    async fn pull_request(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: PullRequestParam = argumente(roh)?;
        let organisation = match self.waehlen(p.verbindung.as_deref()) {
            Ok(organisation) => organisation,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let projekt = match self.projekt(&organisation, p.projekt.as_deref()) {
            Ok(projekt) => projekt,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let client = Self::client(&organisation)?;
        let pr = client
            .json(&Anfrage::holen(format!(
                "{}/_apis/git/pullrequests/{}",
                client::kodieren(&projekt),
                p.id
            )))
            .await
            .map_err(fehler_zu_mcp)?;

        // Das Repo steht im Pull Request; ohne Repo gibt es keine Commits.
        let repo = p
            .repo
            .as_deref()
            .map(str::trim)
            .filter(|repo| !repo.is_empty())
            .map(str::to_string)
            .or_else(|| daten::repo_von_pull_request(&pr));
        let (commits, items) = match repo {
            Some(repo) => {
                let basis = format!(
                    "{}/_apis/git/repositories/{}/pullRequests/{}",
                    client::kodieren(&projekt),
                    client::kodieren(&repo),
                    p.id
                );
                let commits = client
                    .json(&Anfrage::holen(format!("{basis}/commits")).mit_abfrage("$top", "20"))
                    .await
                    .map_err(fehler_zu_mcp)?;
                let items = client
                    .json(&Anfrage::holen(format!("{basis}/workitems")))
                    .await
                    .map_err(fehler_zu_mcp)?;
                (commits, items)
            }
            None => (json!({ "value": [] }), json!({ "value": [] })),
        };
        Ok(antwort(format::pull_request(
            &pr,
            &commits,
            &items,
            zeichen(p.max_zeichen),
        )))
    }

    async fn pull_request_threads(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: PullRequestParam = argumente(roh)?;
        let organisation = match self.waehlen(p.verbindung.as_deref()) {
            Ok(organisation) => organisation,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let projekt = match self.projekt(&organisation, p.projekt.as_deref()) {
            Ok(projekt) => projekt,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let client = Self::client(&organisation)?;
        let repo = match p.repo.as_deref().map(str::trim).filter(|r| !r.is_empty()) {
            Some(repo) => repo.to_string(),
            None => {
                // Ohne `repo`: aus dem Pull Request holen.
                let pr = client
                    .json(&Anfrage::holen(format!(
                        "{}/_apis/git/pullrequests/{}",
                        client::kodieren(&projekt),
                        p.id
                    )))
                    .await
                    .map_err(fehler_zu_mcp)?;
                match daten::repo_von_pull_request(&pr) {
                    Some(repo) => repo,
                    None => {
                        return Err(McpError::invalid_params(
                            "Das Repository des Pull Requests ist unbekannt – bitte `repo` \
                             angeben."
                                .to_string(),
                            None,
                        ))
                    }
                }
            }
        };
        let geantwortet = client
            .json(&Anfrage::holen(format!(
                "{}/_apis/git/repositories/{}/pullRequests/{}/threads",
                client::kodieren(&projekt),
                client::kodieren(&repo),
                p.id
            )))
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::threads(
            p.id,
            &geantwortet,
            zeichen(p.max_zeichen),
        )))
    }

    async fn pipelines(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: ProjektParam = argumente(roh)?;
        let organisation = match self.waehlen(p.verbindung.as_deref()) {
            Ok(organisation) => organisation,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let projekt = match self.projekt(&organisation, p.projekt.as_deref()) {
            Ok(projekt) => projekt,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let client = Self::client(&organisation)?;
        let geantwortet = client
            .json(&Anfrage::holen(format!(
                "{}/_apis/pipelines",
                client::kodieren(&projekt)
            )))
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::pipelines(&geantwortet)))
    }

    async fn laeufe(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: LaeufeParam = argumente(roh)?;
        let organisation = match self.waehlen(p.verbindung.as_deref()) {
            Ok(organisation) => organisation,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let projekt = match self.projekt(&organisation, p.projekt.as_deref()) {
            Ok(projekt) => projekt,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let mut anfrage =
            Anfrage::holen(format!("{}/_apis/build/builds", client::kodieren(&projekt)))
                .mit_abfrage("$top", p.top.unwrap_or(10).clamp(1, 200).to_string());
        if let Some(pipeline) = p.pipeline {
            anfrage = anfrage.mit_abfrage("definitions", pipeline.to_string());
        }
        if let Some(branch) = p.branch.as_deref().map(str::trim) {
            if !branch.is_empty() {
                anfrage = anfrage.mit_abfrage("branchName", ref_name(branch));
            }
        }
        let client = Self::client(&organisation)?;
        let geantwortet = client.json(&anfrage).await.map_err(fehler_zu_mcp)?;
        Ok(antwort(format::laeufe(&geantwortet)))
    }

    async fn lauf(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: LaufParam = argumente(roh)?;
        let organisation = match self.waehlen(p.verbindung.as_deref()) {
            Ok(organisation) => organisation,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let projekt = match self.projekt(&organisation, p.projekt.as_deref()) {
            Ok(projekt) => projekt,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let client = Self::client(&organisation)?;
        let basis = format!(
            "{}/_apis/build/builds/{}",
            client::kodieren(&projekt),
            p.lauf
        );
        let lauf = client
            .json(&Anfrage::holen(&basis))
            .await
            .map_err(fehler_zu_mcp)?;
        let zeitachse = client
            .json(&Anfrage::holen(format!("{basis}/timeline")))
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::lauf(&lauf, Some(&zeitachse))))
    }

    async fn lauf_log(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: LogParam = argumente(roh)?;
        let organisation = match self.waehlen(p.verbindung.as_deref()) {
            Ok(organisation) => organisation,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let projekt = match self.projekt(&organisation, p.projekt.as_deref()) {
            Ok(projekt) => projekt,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let client = Self::client(&organisation)?;
        let geantwortet = client
            .anfrage(
                &Anfrage::holen(format!(
                    "{}/_apis/build/builds/{}/logs/{}",
                    client::kodieren(&projekt),
                    p.lauf,
                    p.log
                ))
                // Ein Log kommt als Text, nicht als JSON.
                .mit_annahme("text/plain, */*"),
            )
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::lauf_log(
            p.lauf,
            p.log,
            &geantwortet.text,
            zeilen(p.zeilen),
        )))
    }

    // ---------------------------------------------------------- schreibend

    async fn workitem_anlegen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: AnlegenParam = argumente(roh)?;
        let organisation = match self.schreiben_erlaubt(p.verbindung.as_deref()) {
            Ok(organisation) => organisation,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let projekt = match self.projekt(&organisation, p.projekt.as_deref()) {
            Ok(projekt) => projekt,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let typ = pflicht(&p.typ, "typ")?;
        let titel = pflicht(&p.titel, "titel")?;

        let mut felder = vec![("System.Title", titel.to_string())];
        if let Some(wert) = p.beschreibung.as_deref().map(str::trim) {
            if !wert.is_empty() {
                felder.push(("System.Description", wert.to_string()));
            }
        }
        if let Some(wert) = p.zugewiesen.as_deref().map(str::trim) {
            if !wert.is_empty() {
                felder.push(("System.AssignedTo", wert.to_string()));
            }
        }
        if let Some(wert) = p.bereich.as_deref().map(str::trim) {
            if !wert.is_empty() {
                felder.push(("System.AreaPath", wert.to_string()));
            }
        }
        if let Some(wert) = p.iteration.as_deref().map(str::trim) {
            if !wert.is_empty() {
                felder.push(("System.IterationPath", wert.to_string()));
            }
        }

        let mut rumpf: Vec<Value> = felder
            .into_iter()
            .map(|(feld, wert)| patch("add", &format!("/fields/{feld}"), json!(wert)))
            .collect();
        if let Some(eltern) = p.eltern {
            rumpf.push(patch(
                "add",
                "/relations/-",
                json!({
                    "rel": "System.LinkTypes.Hierarchy-Reverse",
                    "url": format!("{}/_apis/wit/workItems/{eltern}", organisation.url),
                    "attributes": { "comment": "Eltern" },
                }),
            ));
        }

        let client = Self::client(&organisation)?;
        let geantwortet = client
            .json(
                &Anfrage::neu(
                    Methode::Post,
                    format!(
                        "{}/_apis/wit/workitems/${}",
                        client::kodieren(&projekt),
                        client::kodieren(typ)
                    ),
                )
                .mit_inhalt(Value::Array(rumpf).to_string())
                .mit_kopf("Content-Type", client::JSON_PATCH),
            )
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::item_angelegt(&geantwortet)))
    }

    async fn workitem_aendern(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: AendernParam = argumente(roh)?;
        let organisation = match self.schreiben_erlaubt(p.verbindung.as_deref()) {
            Ok(organisation) => organisation,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let projekt = match self.projekt(&organisation, p.projekt.as_deref()) {
            Ok(projekt) => projekt,
            Err(ohne) => return ohne.in_ergebnis(),
        };

        let mut felder: Vec<(String, String)> = Vec::new();
        for eintrag in &p.felder {
            let feld = pflicht(&eintrag.feld, "felder.feld")?.to_string();
            felder.push((feld, eintrag.wert.clone()));
        }
        for (feld, wert) in [
            ("System.Title", &p.titel),
            ("System.State", &p.status),
            ("System.AssignedTo", &p.zugewiesen),
        ] {
            if let Some(wert) = wert.as_deref().map(str::trim).filter(|w| !w.is_empty()) {
                felder.push((feld.to_string(), wert.to_string()));
            }
        }
        if felder.is_empty() {
            return Err(McpError::invalid_params(
                "Bitte `felder`, `status`, `titel` oder `zugewiesen` angeben.".to_string(),
                None,
            ));
        }

        let rumpf: Vec<Value> = felder
            .iter()
            .map(|(feld, wert)| patch("replace", &format!("/fields/{feld}"), json!(wert)))
            .collect();
        let client = Self::client(&organisation)?;
        client
            .anfrage(&Anfrage::patch(
                format!(
                    "{}/_apis/wit/workitems/{}",
                    client::kodieren(&projekt),
                    p.id
                ),
                Value::Array(rumpf).to_string(),
            ))
            .await
            .map_err(fehler_zu_mcp)?;

        let beschreibung: Vec<String> = felder
            .iter()
            .map(|(feld, wert)| format!("{feld} → {wert}"))
            .collect();
        Ok(antwort(format::item_geaendert(p.id, &beschreibung)))
    }

    async fn workitem_kommentieren(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: KommentarParam = argumente(roh)?;
        let organisation = match self.schreiben_erlaubt(p.verbindung.as_deref()) {
            Ok(organisation) => organisation,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let projekt = match self.projekt(&organisation, p.projekt.as_deref()) {
            Ok(projekt) => projekt,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let text = pflicht(&p.text, "text")?;
        let client = Self::client(&organisation)?;
        let geantwortet = client
            .json(
                &Anfrage::post(
                    format!(
                        "{}/_apis/wit/workItems/{}/comments",
                        client::kodieren(&projekt),
                        p.id
                    ),
                    json!({ "text": text }).to_string(),
                )
                .mit_abfrage("api-version", API_KOMMENTARE),
            )
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::kommentar_angelegt(p.id, &geantwortet)))
    }

    async fn workitem_verknuepfen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: VerknuepfenParam = argumente(roh)?;
        let organisation = match self.schreiben_erlaubt(p.verbindung.as_deref()) {
            Ok(organisation) => organisation,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let projekt = match self.projekt(&organisation, p.projekt.as_deref()) {
            Ok(projekt) => projekt,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let rel = link_typ(&p.art).ok_or_else(|| {
            McpError::invalid_params(
                format!(
                    "`art` muss eltern, kind oder verwandt sein, nicht „{}“.",
                    p.art
                ),
                None,
            )
        })?;
        let client = Self::client(&organisation)?;
        let url = format!(
            "{}/_apis/wit/workItems/{}",
            client::kodieren(&projekt),
            p.id
        );

        let entfernen = p.entfernen.unwrap_or(false);
        let rumpf = if entfernen {
            // Zum Entfernen muss der Index des Links bekannt sein.
            let item = client
                .json(&Anfrage::holen(&url).mit_abfrage("$expand", "relations"))
                .await
                .map_err(fehler_zu_mcp)?;
            let index = daten::liste(&item, "relations")
                .iter()
                .position(|relation| {
                    daten::text(relation, "rel").as_deref() == Some(rel)
                        && daten::text(relation, "url")
                            .map(|ziel| ziel.ends_with(&format!("/workItems/{}", p.ziel)))
                            .unwrap_or(false)
                })
                .ok_or_else(|| {
                    McpError::invalid_params(
                        format!(
                            "An #{0} hängt kein Link „{1}“ zu #{2}.",
                            p.id, p.art, p.ziel
                        ),
                        None,
                    )
                })?;
            vec![json!({ "op": "remove", "path": format!("/relations/{index}") })]
        } else {
            vec![patch(
                "add",
                "/relations/-",
                json!({
                    "rel": rel,
                    "url": format!("{}/_apis/wit/workItems/{}", organisation.url, p.ziel),
                }),
            )]
        };

        client
            .anfrage(&Anfrage::patch(url, Value::Array(rumpf).to_string()))
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::verknuepft(p.id, &p.art, p.ziel, entfernen)))
    }

    async fn pull_request_anlegen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: PrAnlegenParam = argumente(roh)?;
        let organisation = match self.schreiben_erlaubt(p.verbindung.as_deref()) {
            Ok(organisation) => organisation,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let projekt = match self.projekt(&organisation, p.projekt.as_deref()) {
            Ok(projekt) => projekt,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let repo = pflicht(&p.repo, "repo")?;
        let titel = pflicht(&p.titel, "titel")?;
        let quelle = pflicht(&p.quelle, "quelle")?;
        let ziel = pflicht(&p.ziel, "ziel")?;

        let mut rumpf = json!({
            "sourceRefName": ref_name(quelle),
            "targetRefName": ref_name(ziel),
            "title": titel,
        });
        if let Some(beschreibung) = p.beschreibung.as_deref().map(str::trim) {
            if !beschreibung.is_empty() {
                rumpf["description"] = json!(beschreibung);
            }
        }
        if p.entwurf.unwrap_or(false) {
            rumpf["isDraft"] = json!(true);
        }

        let client = Self::client(&organisation)?;
        let pr = client
            .json(&Anfrage::post(
                format!(
                    "{}/_apis/git/repositories/{}/pullrequests",
                    client::kodieren(&projekt),
                    client::kodieren(repo)
                ),
                rumpf.to_string(),
            ))
            .await
            .map_err(fehler_zu_mcp)?;

        let verknuepft = if p.arbeitsitems.is_empty() {
            0
        } else {
            self.arbeitsitems_verknuepfen(&client, &projekt, &pr, &p.arbeitsitems)
                .await?
        };

        let mut text = format::pull_request_angelegt(&pr);
        if verknuepft > 0 {
            text.push_str(&format!("\n{verknuepft} Arbeitsitem(s) verknüpft."));
        }
        Ok(antwort(text))
    }

    /// Arbeitsitems an einen Pull Request hängen (`ArtifactLink`).
    ///
    /// DevOps braucht dafür Projekt- und Repo-GUID im Artefakt-Pfad; beide
    /// stehen in der Antwort des Pull Requests.
    async fn arbeitsitems_verknuepfen(
        &self,
        client: &Client,
        projekt: &str,
        pr: &Value,
        items: &[i64],
    ) -> Result<usize, McpError> {
        let projekt_id = daten::text(pr, "repository.project.id").ok_or_else(|| {
            McpError::internal_error(
                "Die Projekt-ID des Pull Requests fehlt in der Antwort – Arbeitsitems wurden \
                 nicht verknüpft."
                    .to_string(),
                None,
            )
        })?;
        let repo_id = daten::text(pr, "repository.id").ok_or_else(|| {
            McpError::internal_error(
                "Die Repo-ID des Pull Requests fehlt in der Antwort – Arbeitsitems wurden nicht \
                 verknüpft."
                    .to_string(),
                None,
            )
        })?;
        let nummer = daten::text(pr, "pullRequestId").unwrap_or_default();
        let artefakt = format!("vstfs:///Git/PullRequestId/{projekt_id}%2F{repo_id}%2F{nummer}");

        let mut verknuepft = 0;
        for id in items {
            let rumpf = json!([{
                "op": "add",
                "path": "/relations/-",
                "value": {
                    "rel": ARTEFAKT_LINK,
                    "url": artefakt,
                    "attributes": { "name": "Pull Request" },
                },
            }]);
            client
                .anfrage(&Anfrage::patch(
                    format!("{}/_apis/wit/workitems/{}", client::kodieren(projekt), id),
                    rumpf.to_string(),
                ))
                .await
                .map_err(fehler_zu_mcp)?;
            verknuepft += 1;
        }
        Ok(verknuepft)
    }

    async fn pull_request_kommentieren(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: PrKommentarParam = argumente(roh)?;
        let organisation = match self.schreiben_erlaubt(p.verbindung.as_deref()) {
            Ok(organisation) => organisation,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let projekt = match self.projekt(&organisation, p.projekt.as_deref()) {
            Ok(projekt) => projekt,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let repo = pflicht(&p.repo, "repo")?;
        let text = pflicht(&p.text, "text")?;
        let basis = format!(
            "{}/_apis/git/repositories/{}/pullRequests/{}/threads",
            client::kodieren(&projekt),
            client::kodieren(repo),
            p.id
        );

        let client = Self::client(&organisation)?;
        let geantwortet = match p.thread {
            Some(thread) => {
                client
                    .json(&Anfrage::post(
                        format!("{basis}/{thread}/comments"),
                        json!({ "content": text, "commentType": 1, "parentCommentId": 0 })
                            .to_string(),
                    ))
                    .await
                    .map_err(fehler_zu_mcp)?;
                format::pr_kommentiert(p.id, Some(thread.to_string()), true)
            }
            None => {
                let thread = client
                    .json(&Anfrage::post(
                        basis,
                        json!({
                            "comments": [ { "content": text, "commentType": 1, "parentCommentId": 0 } ],
                            "status": 1,
                        })
                        .to_string(),
                    ))
                    .await
                    .map_err(fehler_zu_mcp)?;
                format::pr_kommentiert(p.id, daten::text(&thread, "id"), false)
            }
        };
        Ok(antwort(geantwortet))
    }

    async fn pull_request_abstimmen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: AbstimmenParam = argumente(roh)?;
        let organisation = match self.schreiben_erlaubt(p.verbindung.as_deref()) {
            Ok(organisation) => organisation,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let projekt = match self.projekt(&organisation, p.projekt.as_deref()) {
            Ok(projekt) => projekt,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let repo = pflicht(&p.repo, "repo")?;
        let stimme = p
            .stimme
            .as_deref()
            .map(str::trim)
            .filter(|stimme| !stimme.is_empty())
            .unwrap_or("approve");
        let wert = stimme_wert(stimme).ok_or_else(|| {
            McpError::invalid_params(
                format!(
                    "`stimme` muss approve, approve-with-suggestions, wait, reject oder reset \
                     sein, nicht „{stimme}“."
                ),
                None,
            )
        })?;

        let client = Self::client(&organisation)?;
        // Die Stimme gilt für den angemeldeten Benutzer – seine ID kommt aus
        // `connectionData`. Antwortet der Dienst dort mit einem Fehler (400 bei
        // falscher `api-version`), bricht der Aufruf hier ab; der Fehler läuft
        // nie als Benutzer-ID in den Pfad des `PUT`.
        let daten = client
            .json(&verbindungsdaten())
            .await
            .map_err(fehler_zu_mcp)?;
        let benutzer = daten::text(&daten, "authenticatedUser.id").ok_or_else(|| {
            McpError::internal_error(
                "Die Benutzer-ID fehlt in der Antwort von connectionData – bitte \
                 `devops_testen` prüfen."
                    .to_string(),
                None,
            )
        })?;

        client
            .json(
                &Anfrage::neu(
                    Methode::Put,
                    format!(
                        "{}/_apis/git/repositories/{}/pullRequests/{}/reviewers/{}",
                        client::kodieren(&projekt),
                        client::kodieren(repo),
                        p.id,
                        client::kodieren(&benutzer)
                    ),
                )
                .mit_inhalt(json!({ "vote": wert }).to_string()),
            )
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::abgestimmt(
            p.id,
            format::stimme_anzeige(wert),
        )))
    }

    async fn pipeline_starten(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: PipelineParam = argumente(roh)?;
        let organisation = match self.schreiben_erlaubt(p.verbindung.as_deref()) {
            Ok(organisation) => organisation,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let projekt = match self.projekt(&organisation, p.projekt.as_deref()) {
            Ok(projekt) => projekt,
            Err(ohne) => return ohne.in_ergebnis(),
        };

        let mut rumpf = json!({});
        if let Some(branch) = p.branch.as_deref().map(str::trim).filter(|b| !b.is_empty()) {
            rumpf["resources"] = json!({
                "repositories": { "self": { "refName": ref_name(branch) } }
            });
        }
        if !p.variablen.is_empty() {
            let variablen: serde_json::Map<String, Value> = p
                .variablen
                .iter()
                .map(|(name, wert)| (name.clone(), json!({ "value": wert })))
                .collect();
            rumpf["variables"] = Value::Object(variablen);
        }
        if !p.parameter.is_empty() {
            let parameter: serde_json::Map<String, Value> = p
                .parameter
                .iter()
                .map(|(name, wert)| (name.clone(), json!(wert)))
                .collect();
            rumpf["templateParameters"] = Value::Object(parameter);
        }

        let client = Self::client(&organisation)?;
        let lauf = client
            .json(&Anfrage::post(
                format!(
                    "{}/_apis/pipelines/{}/runs",
                    client::kodieren(&projekt),
                    p.pipeline
                ),
                rumpf.to_string(),
            ))
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::lauf_gestartet(&lauf)))
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

/// Ein veränderndes Werkzeug (`destructive_hint`, Aufgabe 09).
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

/// Eine Zahl aus einem optionalen Feld, mit Vorgabe und Grenzen.
fn zeichen(wert: Option<u32>) -> usize {
    wert.unwrap_or(format::MAX_ZEICHEN as u32)
        .clamp(100, 100_000) as usize
}

/// Zeilen für ein Log: Standard 200, höchstens 5000.
fn zeilen(wert: Option<u32>) -> usize {
    wert.unwrap_or(format::MAX_ZEILEN as u32).clamp(1, 5000) as usize
}

/// Ein optionales Team, das nicht nur aus Leerraum besteht.
fn team(wert: &Option<String>) -> Option<String> {
    wert.as_deref()
        .map(str::trim)
        .filter(|team| !team.is_empty())
        .map(str::to_string)
}

/// Ein Branch-Name mit `refs/heads/`, falls er noch keins hat.
fn ref_name(branch: &str) -> String {
    let branch = branch.trim();
    if branch.starts_with("refs/") {
        branch.to_string()
    } else {
        format!("refs/heads/{branch}")
    }
}

/// Eine JSON-Patch-Operation mit Wert.
fn patch(op: &str, pfad: &str, wert: Value) -> Value {
    json!({ "op": op, "path": pfad, "value": wert })
}

/// Die Relation zu einer Kurzform (`eltern`, `kind`, `verwandt`).
fn link_typ(art: &str) -> Option<&'static str> {
    match art.trim().to_ascii_lowercase().as_str() {
        "eltern" | "parent" => Some("System.LinkTypes.Hierarchy-Reverse"),
        "kind" | "child" => Some("System.LinkTypes.Hierarchy-Forward"),
        "verwandt" | "related" => Some("System.LinkTypes.Related"),
        _ => None,
    }
}

/// Der Zahlenwert einer Stimme.
fn stimme_wert(stimme: &str) -> Option<i64> {
    match stimme.trim().to_ascii_lowercase().as_str() {
        "approve" => Some(10),
        "approve-with-suggestions" => Some(5),
        "wait" | "waiting-for-author" => Some(-5),
        "reject" => Some(-10),
        "reset" => Some(0),
        _ => None,
    }
}

/// Eine Pull-Request-Liste auf einen Ersteller filtern (Anzeigename, Teiltreffer).
fn filtern_nach_ersteller(baum: &Value, ersteller: &str) -> Value {
    let gesucht = ersteller.trim().to_lowercase();
    let gefiltert: Vec<Value> = daten::liste(baum, "value")
        .iter()
        .filter(|pr| {
            daten::text(pr, "createdBy.displayName")
                .map(|name| name.to_lowercase().contains(&gesucht))
                .unwrap_or(false)
        })
        .cloned()
        .collect();
    json!({ "count": gefiltert.len(), "value": gefiltert })
}

/// Aus einem DevOps-Fehler wird ein MCP-Fehler – mit Verbindung, nie mit PAT.
fn fehler_zu_mcp(fehler: DevopsFehler) -> McpError {
    let text = fehler.to_string();
    match fehler {
        DevopsFehler::Anmeldung { .. } | DevopsFehler::Http { .. } => {
            McpError::invalid_params(text, None)
        }
        _ => McpError::internal_error(text, None),
    }
}

// ------------------------------------------------------------- Parameter

/// Parameter von `devops_status`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct StatusParam {
    /// Nur diese Organisation zeigen. Ohne Angabe: alle.
    #[serde(default)]
    pub verbindung: Option<String>,
}

/// Parameter von `devops_testen` und `devops_projekte`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct VerbindungParam {
    /// Organisation; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
}

/// Parameter der Werkzeuge, die nur ein Projekt brauchen.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ProjektParam {
    /// Organisation; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Projekt; ohne Angabe gilt das Projekt des Zugangs.
    #[serde(default)]
    pub projekt: Option<String>,
}

/// Parameter von `devops_workitems_suchen`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct SucheParam {
    /// Organisation; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Projekt; ohne Angabe gilt das Projekt des Zugangs.
    #[serde(default)]
    pub projekt: Option<String>,
    /// Fertige WIQL-Abfrage. Ersetzt `text`, `status`, `typ` und `zugewiesen_an_mich`.
    #[serde(default)]
    pub wiql: Option<String>,
    /// Freitext im Titel (`CONTAINS`).
    #[serde(default)]
    pub text: Option<String>,
    /// Nur Arbeitsitems, die mir zugewiesen sind (`@Me`).
    #[serde(default)]
    pub zugewiesen_an_mich: Option<bool>,
    /// Status, z. B. `Active` oder `Closed`.
    #[serde(default)]
    pub status: Option<String>,
    /// Arbeitsitem-Typ, z. B. `Bug` oder `User Story`.
    #[serde(default)]
    pub typ: Option<String>,
    /// Höchstens so viele Treffer (Standard 50, höchstens 200).
    #[serde(default)]
    pub top: Option<u32>,
}

/// Parameter von `devops_workitem`, `devops_workitem_kommentare` und den
/// Pull-Request-Werkzeugen mit einer ID.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ItemParam {
    /// Organisation; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Projekt; ohne Angabe gilt das Projekt des Zugangs.
    #[serde(default)]
    pub projekt: Option<String>,
    /// Arbeitsitem-Nummer.
    pub id: u64,
    /// Höchstens so viele Zeichen je Text (Standard 4000).
    #[serde(default)]
    pub max_zeichen: Option<u32>,
}

/// Parameter von `devops_iterationen`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct IterationenParam {
    /// Organisation; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Projekt; ohne Angabe gilt das Projekt des Zugangs.
    #[serde(default)]
    pub projekt: Option<String>,
    /// Team des Projekts (Pflicht).
    #[serde(default)]
    pub team: Option<String>,
    /// Nur die laufende Iteration.
    #[serde(default)]
    pub aktuell: Option<bool>,
}

/// Parameter von `devops_iteration_workitems`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct IterationsitemsParam {
    /// Organisation; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Projekt; ohne Angabe gilt das Projekt des Zugangs.
    #[serde(default)]
    pub projekt: Option<String>,
    /// Team des Projekts (Pflicht).
    #[serde(default)]
    pub team: Option<String>,
    /// Iterations-ID aus `devops_iterationen`.
    pub iteration: String,
}

/// Parameter von `devops_branches`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct BranchesParam {
    /// Organisation; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Projekt; ohne Angabe gilt das Projekt des Zugangs.
    #[serde(default)]
    pub projekt: Option<String>,
    /// Repository-Name oder -ID.
    pub repo: String,
}

/// Parameter von `devops_pull_requests`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct PullRequestsParam {
    /// Organisation; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Projekt; ohne Angabe gilt das Projekt des Zugangs.
    #[serde(default)]
    pub projekt: Option<String>,
    /// Nur dieses Repository (Name oder ID).
    #[serde(default)]
    pub repo: Option<String>,
    /// `active` (Standard), `abandoned`, `completed` oder `all`.
    #[serde(default)]
    pub status: Option<String>,
    /// Nur Pull Requests dieses Erstellers (Anzeigename, Teiltreffer).
    #[serde(default)]
    pub ersteller: Option<String>,
    /// Höchstens so viele Treffer (Standard 100).
    #[serde(default)]
    pub top: Option<u32>,
}

/// Parameter von `devops_pull_request` und `devops_pull_request_threads`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct PullRequestParam {
    /// Organisation; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Projekt; ohne Angabe gilt das Projekt des Zugangs.
    #[serde(default)]
    pub projekt: Option<String>,
    /// Repository; ohne Angabe aus dem Pull Request.
    #[serde(default)]
    pub repo: Option<String>,
    /// Pull-Request-Nummer.
    pub id: u64,
    /// Höchstens so viele Zeichen je Text (Standard 4000).
    #[serde(default)]
    pub max_zeichen: Option<u32>,
}

/// Parameter von `devops_laeufe`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct LaeufeParam {
    /// Organisation; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Projekt; ohne Angabe gilt das Projekt des Zugangs.
    #[serde(default)]
    pub projekt: Option<String>,
    /// Nur Läufe dieser Pipeline.
    #[serde(default)]
    pub pipeline: Option<u32>,
    /// Nur Läufe dieses Branches (`main` oder `refs/heads/main`).
    #[serde(default)]
    pub branch: Option<String>,
    /// Höchstens so viele Läufe (Standard 10).
    #[serde(default)]
    pub top: Option<u32>,
}

/// Parameter von `devops_lauf`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct LaufParam {
    /// Organisation; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Projekt; ohne Angabe gilt das Projekt des Zugangs.
    #[serde(default)]
    pub projekt: Option<String>,
    /// Lauf-Nummer (Build-ID).
    pub lauf: u64,
}

/// Parameter von `devops_lauf_log`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct LogParam {
    /// Organisation; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Projekt; ohne Angabe gilt das Projekt des Zugangs.
    #[serde(default)]
    pub projekt: Option<String>,
    /// Lauf-Nummer (Build-ID).
    pub lauf: u64,
    /// Log-Nummer aus `devops_lauf`.
    pub log: u64,
    /// So viele Zeilen vom Ende (Standard 200).
    #[serde(default)]
    pub zeilen: Option<u32>,
}

/// Ein Feld und sein neuer Wert.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct FeldWert {
    /// Feldname, z. B. `System.Priority` oder `Microsoft.VSTS.Common.Severity`.
    pub feld: String,
    /// Neuer Wert.
    pub wert: String,
}

/// Parameter von `devops_workitem_anlegen`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct AnlegenParam {
    /// Organisation; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Projekt; ohne Angabe gilt das Projekt des Zugangs.
    #[serde(default)]
    pub projekt: Option<String>,
    /// Arbeitsitem-Typ, z. B. `Bug` oder `User Story` (siehe `devops_workitem_typen`).
    pub typ: String,
    /// Titel (Pflicht).
    pub titel: String,
    /// Beschreibung (HTML oder Text).
    #[serde(default)]
    pub beschreibung: Option<String>,
    /// Zugewiesener Benutzer (E-Mail oder Anzeigename).
    #[serde(default)]
    pub zugewiesen: Option<String>,
    /// Bereichspfad, z. B. `PROJEKT\Web`.
    #[serde(default)]
    pub bereich: Option<String>,
    /// Iterationspfad, z. B. `PROJEKT\Sprint 1`.
    #[serde(default)]
    pub iteration: Option<String>,
    /// Eltern-Arbeitsitem, wird als Link gesetzt.
    #[serde(default)]
    pub eltern: Option<i64>,
}

/// Parameter von `devops_workitem_aendern`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct AendernParam {
    /// Organisation; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Projekt; ohne Angabe gilt das Projekt des Zugangs.
    #[serde(default)]
    pub projekt: Option<String>,
    /// Arbeitsitem-Nummer.
    pub id: u64,
    /// Felder als Liste `{feld, wert}`.
    #[serde(default)]
    pub felder: Vec<FeldWert>,
    /// Kurzform für `System.State`.
    #[serde(default)]
    pub status: Option<String>,
    /// Kurzform für `System.Title`.
    #[serde(default)]
    pub titel: Option<String>,
    /// Kurzform für `System.AssignedTo`.
    #[serde(default)]
    pub zugewiesen: Option<String>,
}

/// Parameter von `devops_workitem_kommentieren`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct KommentarParam {
    /// Organisation; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Projekt; ohne Angabe gilt das Projekt des Zugangs.
    #[serde(default)]
    pub projekt: Option<String>,
    /// Arbeitsitem-Nummer.
    pub id: u64,
    /// Kommentartext.
    pub text: String,
}

/// Parameter von `devops_workitem_verknuepfen`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct VerknuepfenParam {
    /// Organisation; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Projekt; ohne Angabe gilt das Projekt des Zugangs.
    #[serde(default)]
    pub projekt: Option<String>,
    /// Arbeitsitem-Nummer, von dem der Link ausgeht.
    pub id: u64,
    /// Ziel-Arbeitsitem.
    pub ziel: u64,
    /// Art des Links: `eltern`, `kind` oder `verwandt`.
    #[serde(default)]
    pub art: String,
    /// `true` entfernt den Link wieder.
    #[serde(default)]
    pub entfernen: Option<bool>,
}

/// Parameter von `devops_pull_request_anlegen`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct PrAnlegenParam {
    /// Organisation; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Projekt; ohne Angabe gilt das Projekt des Zugangs.
    #[serde(default)]
    pub projekt: Option<String>,
    /// Repository-Name oder -ID.
    pub repo: String,
    /// Quellbranch (`feature/login` oder `refs/heads/feature/login`).
    pub quelle: String,
    /// Zielbranch (`main` oder `refs/heads/main`).
    pub ziel: String,
    /// Titel (Pflicht).
    pub titel: String,
    /// Beschreibung (Markdown).
    #[serde(default)]
    pub beschreibung: Option<String>,
    /// Arbeitsitems, die verknüpft werden.
    #[serde(default)]
    pub arbeitsitems: Vec<i64>,
    /// Als Entwurf anlegen.
    #[serde(default)]
    pub entwurf: Option<bool>,
}

/// Parameter von `devops_pull_request_kommentieren`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct PrKommentarParam {
    /// Organisation; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Projekt; ohne Angabe gilt das Projekt des Zugangs.
    #[serde(default)]
    pub projekt: Option<String>,
    /// Repository-Name oder -ID.
    pub repo: String,
    /// Pull-Request-Nummer.
    pub id: u64,
    /// Kommentartext.
    pub text: String,
    /// Thread, in dem geantwortet wird. Ohne Angabe: neuer Thread.
    #[serde(default)]
    pub thread: Option<u32>,
}

/// Parameter von `devops_pull_request_abstimmen`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct AbstimmenParam {
    /// Organisation; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Projekt; ohne Angabe gilt das Projekt des Zugangs.
    #[serde(default)]
    pub projekt: Option<String>,
    /// Repository-Name oder -ID.
    pub repo: String,
    /// Pull-Request-Nummer.
    pub id: u64,
    /// `approve` (Standard), `approve-with-suggestions`, `wait`, `reject` oder `reset`.
    #[serde(default)]
    pub stimme: Option<String>,
}

/// Parameter von `devops_pipeline_starten`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct PipelineParam {
    /// Organisation; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Projekt; ohne Angabe gilt das Projekt des Zugangs.
    #[serde(default)]
    pub projekt: Option<String>,
    /// Pipeline-Nummer (siehe `devops_pipelines`).
    pub pipeline: u32,
    /// Branch (`main` oder `refs/heads/main`).
    #[serde(default)]
    pub branch: Option<String>,
    /// Build-Variablen als Paare.
    #[serde(default)]
    pub variablen: std::collections::BTreeMap<String, String>,
    /// Vorlagenparameter der YAML-Pipeline als Paare.
    #[serde(default)]
    pub parameter: std::collections::BTreeMap<String, String>,
}

// -------------------------------------------------------------- MCP-Anker

impl ServerHandler for DevopsServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(quiver_tools_kern::mcp::server_info(&self.zugang))
            .with_instructions(format!(
                "Azure DevOps über die REST-API 7.1: Arbeitsitems suchen und lesen, Kommentare, \
                 Iterationen, Repos, Branches, Pull Requests, Pipelines und Läufe ansehen. \
                 Verändernd gehen Arbeitsitems anlegen und ändern, Kommentare, Links, Pull \
                 Requests, Abstimmungen und Pipeline-Läufe – aber nur, wenn der Schalter \
                 „schreiben“ für die Organisation an ist (Standard: aus). Ist er aus, antworten \
                 die verändernden Werkzeuge mit dem Hinweis und ohne Anfrage an DevOps. Die \
                 {MEHRFACH_TITEL} ({MEHRFACH_EINZAHL}en) legt der User im quiver-Fenster unter \
                 Tools → {TITEL} an; jedes Werkzeug nimmt `verbindung`, bei genau einer darf der \
                 Name fehlen. Das Projekt kommt aus dem Zugang oder aus `projekt`. Fehlt die \
                 Liste, sagen die Werkzeuge das als Text."
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
    use crate::werkzeug::Devops;
    use quiver_tools_kern::zugaenge::Zugang as Eintrag;
    use serde_json::json;

    fn eintrag(name: &str, projekt: Option<&str>, schreiben: bool) -> Eintrag {
        let mut werte: std::collections::BTreeMap<String, String> = [
            ("url", "https://dev.azure.com/org"),
            ("pat", "pat-4711"),
            ("schreiben", if schreiben { "an" } else { "aus" }),
        ]
        .into_iter()
        .map(|(feld, wert)| (feld.to_string(), wert.to_string()))
        .collect();
        if let Some(projekt) = projekt {
            werte.insert("projekt".to_string(), projekt.to_string());
        }
        Eintrag {
            name: name.to_string(),
            werte,
        }
    }

    fn server_mit(eintraege: Vec<Eintrag>) -> DevopsServer {
        DevopsServer::mit(
            Zugang::mit_umgebung(&Devops, |_| None),
            Ok(Sammlung::aus_zugaengen(eintraege).unwrap()),
        )
    }

    fn ohne_organisationen() -> DevopsServer {
        DevopsServer::mit(
            Zugang::mit_umgebung(&Devops, |_| None),
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

    fn aufruf(server: &DevopsServer, name: &str, argumente: Value) -> Ergebnis {
        let laufzeit = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let objekt = argumente.as_object().cloned().unwrap_or_default();
        laufzeit.block_on(server.aufrufen(name, Some(objekt)))
    }

    #[test]
    fn alle_werkzeuge_tragen_den_richtigen_hinweis() {
        let server = ohne_organisationen();
        let werkzeuge = server.werkzeuge();
        assert_eq!(werkzeuge.len(), 27, "26 Werkzeuge plus devops_profile");

        let schreibend = [
            "devops_workitem_anlegen",
            "devops_workitem_aendern",
            "devops_workitem_kommentieren",
            "devops_workitem_verknuepfen",
            "devops_pull_request_anlegen",
            "devops_pull_request_kommentieren",
            "devops_pull_request_abstimmen",
            "devops_pipeline_starten",
        ];

        for werkzeug in &werkzeuge {
            let hinweise = werkzeug.annotations.as_ref().expect("Hinweise fehlen");
            let name = werkzeug.name.as_ref();
            let destruktiv = schreibend.contains(&name);
            assert_eq!(hinweise.read_only_hint, Some(!destruktiv), "{name}");
            assert_eq!(hinweise.destructive_hint, Some(destruktiv), "{name}");
            if name == "devops_profile" {
                assert!(hinweise.title.is_some(), "{name} hat keinen Titel");
            } else {
                assert!(werkzeug.title.is_some(), "{name} hat keinen Titel");
            }
            // Jedes eigene Werkzeug kennt `verbindung`; `devops_status` und
            // `devops_profile` dürfen es als einziges Feld tragen.
            let eigenschaften = &werkzeug.input_schema["properties"];
            if name != "devops_profile" {
                assert!(
                    eigenschaften.get("verbindung").is_some(),
                    "{name} kennt kein `verbindung`: {eigenschaften:?}"
                );
            }
        }

        // Die Namen stehen in einer festen Reihenfolge: erst lesend, dann
        // verändernd, zuletzt das Profil.
        let namen: Vec<&str> = werkzeuge.iter().map(|w| w.name.as_ref()).collect();
        assert_eq!(namen[0], "devops_status");
        assert_eq!(namen[17], "devops_lauf_log");
        assert_eq!(namen[18], "devops_workitem_anlegen");
        assert_eq!(namen[25], "devops_pipeline_starten");
        assert_eq!(namen[26], "devops_profile");
    }

    #[test]
    fn ohne_organisationen_kommt_der_hinweis_und_kein_fehler() {
        let server = ohne_organisationen();
        for (name, argumente) in [
            ("devops_status", json!({})),
            ("devops_testen", json!({})),
            ("devops_projekte", json!({})),
            ("devops_workitems_suchen", json!({ "text": "Login" })),
            ("devops_workitem", json!({ "id": 1 })),
            ("devops_repos", json!({})),
            (
                "devops_workitem_anlegen",
                json!({ "typ": "Bug", "titel": "x" }),
            ),
        ] {
            let text = text_von(aufruf(&server, name, argumente));
            assert!(text.contains("Zugang fehlt"), "{name}: {text}");
            assert!(text.contains("Tools → DevOps"), "{name}: {text}");
        }
    }

    #[test]
    fn ohne_projekt_kommt_ein_hinweis_ohne_anfrage() {
        let server = server_mit(vec![eintrag("kunde", None, true)]);
        let text = text_von(aufruf(&server, "devops_repos", json!({})));
        assert!(text.contains("Projekt fehlt"), "{text}");
        assert!(text.contains("`projekt` angeben"), "{text}");
    }

    #[test]
    fn ohne_team_kommt_ein_hinweis() {
        let server = server_mit(vec![eintrag("kunde", Some("PROJEKT"), true)]);
        let text = text_von(aufruf(&server, "devops_iterationen", json!({})));
        assert!(text.contains("Team fehlt"), "{text}");
    }

    #[test]
    fn das_profil_nennt_die_organisationen_ohne_pat() {
        let server = server_mit(vec![eintrag("beispiel", Some("PROJEKT"), false)]);
        let text = text_von(aufruf(&server, "devops_profile", json!({})));
        assert!(text.contains("beispiel"), "{text}");
        assert!(text.contains("Schreiben aus"), "{text}");
        assert!(!text.contains("pat-4711"), "{text}");
    }

    #[test]
    fn mehrere_organisationen_verlangen_einen_namen() {
        let server = server_mit(vec![
            eintrag("beispiel", Some("PROJEKT"), true),
            eintrag("kunde", None, false),
        ]);
        let text = text_von(aufruf(&server, "devops_status", json!({})));
        assert!(text.contains("beispiel"), "{text}");
        assert!(text.contains("kunde"), "{text}");
        assert!(!text.contains("pat-4711"), "{text}");

        let fehler = aufruf(&server, "devops_repos", json!({})).unwrap_err();
        assert!(
            fehler.message.contains("mehrere Organisationen"),
            "{}",
            fehler.message
        );

        let fehler = aufruf(
            &server,
            "devops_repos",
            json!({ "verbindung": "gibtsnicht" }),
        )
        .unwrap_err();
        assert!(fehler.message.contains("gibtsnicht"), "{}", fehler.message);
    }

    #[test]
    fn schreiben_aus_antwortet_ohne_anfrage() {
        let server = server_mit(vec![eintrag("kunde", Some("PROJEKT"), false)]);
        for (name, argumente) in [
            (
                "devops_workitem_anlegen",
                json!({ "typ": "Bug", "titel": "x" }),
            ),
            (
                "devops_workitem_aendern",
                json!({ "id": 1, "status": "Closed" }),
            ),
            (
                "devops_workitem_kommentieren",
                json!({ "id": 1, "text": "hallo" }),
            ),
            (
                "devops_workitem_verknuepfen",
                json!({ "id": 1, "ziel": 2, "art": "verwandt" }),
            ),
            (
                "devops_pull_request_anlegen",
                json!({ "repo": "r", "quelle": "a", "ziel": "b", "titel": "t" }),
            ),
            (
                "devops_pull_request_kommentieren",
                json!({ "repo": "r", "id": 1, "text": "t" }),
            ),
            (
                "devops_pull_request_abstimmen",
                json!({ "repo": "r", "id": 1, "stimme": "approve" }),
            ),
            ("devops_pipeline_starten", json!({ "pipeline": 7 })),
        ] {
            let text = text_von(aufruf(&server, name, argumente));
            assert!(
                text.contains("Schreiben ist für kunde aus"),
                "{name}: {text}"
            );
        }
    }

    #[test]
    fn leere_pflichtparameter_und_unbekannte_werkzeuge_sind_parameterfehler() {
        let server = server_mit(vec![eintrag("org", Some("PROJEKT"), true)]);
        let fehler = aufruf(
            &server,
            "devops_workitem_anlegen",
            json!({ "typ": "  ", "titel": "x" }),
        )
        .unwrap_err();
        assert!(fehler.message.contains("typ"), "{}", fehler.message);

        let fehler = aufruf(&server, "devops_gibtsnicht", json!({})).unwrap_err();
        assert!(
            fehler.message.contains("devops_gibtsnicht"),
            "{}",
            fehler.message
        );

        let fehler = aufruf(&server, "devops_workitem", json!({ "id": "abc" })).unwrap_err();
        assert!(fehler.message.contains("Parameter"), "{}", fehler.message);

        // Ein veränderndes Werkzeug ohne Felder sagt, was fehlt.
        let fehler = aufruf(&server, "devops_workitem_aendern", json!({ "id": 1 })).unwrap_err();
        assert!(fehler.message.contains("felder"), "{}", fehler.message);

        let fehler = aufruf(
            &server,
            "devops_workitem_verknuepfen",
            json!({ "id": 1, "ziel": 2, "art": "quatsch" }),
        )
        .unwrap_err();
        assert!(fehler.message.contains("eltern"), "{}", fehler.message);

        let fehler = aufruf(
            &server,
            "devops_pull_request_abstimmen",
            json!({ "repo": "r", "id": 1, "stimme": "vielleicht" }),
        )
        .unwrap_err();
        assert!(fehler.message.contains("approve"), "{}", fehler.message);
    }

    #[test]
    fn die_helfer_rechnen_richtig() {
        assert_eq!(zeichen(None), format::MAX_ZEICHEN);
        assert_eq!(zeichen(Some(5000)), 5000);
        assert_eq!(zeichen(Some(1)), 100);
        assert_eq!(zeilen(None), format::MAX_ZEILEN);
        assert_eq!(zeilen(Some(0)), 1);

        assert_eq!(ref_name("main"), "refs/heads/main");
        assert_eq!(ref_name("refs/heads/main"), "refs/heads/main");
        assert_eq!(ref_name(" feature/x "), "refs/heads/feature/x");

        assert_eq!(team(&None), None);
        assert_eq!(team(&Some("  ".into())), None);
        assert_eq!(team(&Some(" Team A ".into())).as_deref(), Some("Team A"));

        assert_eq!(
            link_typ("Eltern"),
            Some("System.LinkTypes.Hierarchy-Reverse")
        );
        assert_eq!(link_typ("kind"), Some("System.LinkTypes.Hierarchy-Forward"));
        assert_eq!(link_typ("verwandt"), Some("System.LinkTypes.Related"));
        assert_eq!(link_typ("quatsch"), None);

        assert_eq!(stimme_wert("approve"), Some(10));
        assert_eq!(stimme_wert("approve-with-suggestions"), Some(5));
        assert_eq!(stimme_wert("wait"), Some(-5));
        assert_eq!(stimme_wert("reject"), Some(-10));
        assert_eq!(stimme_wert("reset"), Some(0));
        assert_eq!(stimme_wert("vielleicht"), None);

        assert_eq!(
            patch("add", "/fields/System.Title", json!("x")),
            json!({ "op": "add", "path": "/fields/System.Title", "value": "x" })
        );
    }

    #[test]
    fn der_ersteller_filter_arbeitet_lokal() {
        let liste = json!({ "value": [
            { "pullRequestId": 1, "createdBy": { "displayName": "Alex Beispiel" } },
            { "pullRequestId": 2, "createdBy": { "displayName": "Anna" } }
        ] });
        let gefiltert = filtern_nach_ersteller(&liste, "beisp");
        assert_eq!(daten::liste(&gefiltert, "value").len(), 1);
        assert_eq!(
            daten::text(&gefiltert, "value.0.pullRequestId").as_deref(),
            Some("1")
        );
        assert!(daten::liste(&filtern_nach_ersteller(&liste, "niemand"), "value").is_empty());
    }

    #[test]
    fn eine_kaputte_organisationsliste_ist_ein_parameterfehler() {
        let server = DevopsServer::mit(
            Zugang::mit_umgebung(&Devops, |_| None),
            Sammlung::mit_umgebung(|_| Some("[{\"name\":".to_string())),
        );
        let fehler = aufruf(&server, "devops_status", json!({})).unwrap_err();
        assert!(
            fehler.message.contains("QUIVER_ZUGAENGE"),
            "{}",
            fehler.message
        );
    }
}
