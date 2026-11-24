// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der MCP-Server: 15 Werkzeuge über stdio (Aufgabe 15).
//!
//! Elf lesende Werkzeuge (`read_only_hint`) und drei verändernde
//! (`destructive_hint`, brauchen den Schalter `schreiben`) – dazu aus dem Kern
//! `zephyr_profile`. Die Zugänge kommen aus `QUIVER_ZUGAENGE` (mehrere je
//! Werkzeug). Fehlt die Liste, antwortet jedes Werkzeug mit dem Hinweis auf das
//! quiver-Fenster – **kein** MCP-Fehler. Jedes Werkzeug nimmt `verbindung`; bei
//! genau einem Zugang darf der Name fehlen.
//!
//! Drei Regeln stehen **vor** jeder verändernden Anfrage (Aufgabe 15):
//!
//! | Regel | Wirkung |
//! |---|---|
//! | `schreiben` je Zugang (Standard aus) | verändernde Werkzeuge antworten ohne Anfrage |
//! | `nur` (Projektschlüssel) | Schreiben außerhalb wird abgelehnt, ohne verändernde Anfrage |
//! | Projekt vom Server | der Schlüssel kommt aus `GET /projects/{id}`, nicht vom Agenten |
//!
//! Zwei Regeln gelten für **jedes** gelesene Feld: es steht in der OpenAPI-Datei
//! (Verweise tragen nur `id` und `self`), und Pfadwerte sind geprüft
//! ([`client::pfadwert`]).

use std::any::Any;

use quiver_tools_kern::mcp::profile_tool;
use quiver_tools_kern::{antwort, hinweis_zugang, OhneProfil, Zugang as KernZugang};
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

use crate::client::{self, pfadwert, Anfrage, Client, Methode};
use crate::daten;
use crate::fehler::ZephyrFehler;
use crate::format;
use crate::namen::{self, Namen};
use crate::werkzeug::{MEHRFACH_EINZAHL, MEHRFACH_TITEL, TITEL};
use crate::zugaenge::{AuswahlFehler, SammelFehler, Sammlung, Zugang};

/// Was ein Werkzeug zurückgibt.
type Ergebnis = Result<CallToolResult, McpError>;

/// So viele Seiten zählt `zephyr_testzyklus` höchstens (100 je Seite).
const ZAEHL_SEITEN: u32 = 5;

/// Der Zephyr-Server: die Zugänge und die Werkzeugliste.
pub struct ZephyrServer {
    /// Der Zugang des Kerns; bei `mehrfach` ohne Felder, aber mit Titel.
    zugang: KernZugang,
    zugaenge: Result<Sammlung, SammelFehler>,
}

impl ZephyrServer {
    /// Der Normalfall: alles aus der Umgebung (`QUIVER_ZUGAENGE`).
    pub fn neu(zugang: KernZugang) -> Self {
        ZephyrServer {
            zugang,
            zugaenge: Sammlung::aus_umgebung(),
        }
    }

    /// Feste Teile (Tests und Sonderfälle), ohne Prozessumgebung.
    pub fn mit(zugang: KernZugang, zugaenge: Result<Sammlung, SammelFehler>) -> Self {
        ZephyrServer { zugang, zugaenge }
    }

    /// Alle Werkzeuge, wie `tools/list` sie meldet.
    pub fn werkzeuge(&self) -> Vec<Tool> {
        let mut alle = self.lesende_werkzeuge();
        alle.extend(self.schreibende_werkzeuge());
        // `zephyr_profile` kommt aus dem Kern: bei jedem Tool dasselbe lesende
        // Werkzeug.
        alle.push(profile_tool(&self.zugang));
        alle
    }

    /// Die elf lesenden Werkzeuge.
    fn lesende_werkzeuge(&self) -> Vec<Tool> {
        vec![
            lesend::<StatusParam>(
                "zephyr_status",
                "Zugänge",
                "Listet die im quiver-Fenster eingetragenen Zephyr-Zugänge: Name, Basisadresse, \
                 Standardprojekt, erlaubte Projekte und ob „schreiben“ an ist. Öffnet keine \
                 Verbindung, zeigt kein Token.",
            ),
            lesend::<VerbindungParam>(
                "zephyr_testen",
                "Token prüfen",
                "Prüft mit `/projects?maxResults=1`, ob das API-Token gilt, und nennt die Zahl der \
                 sichtbaren Projekte. Zephyr nennt keinen Benutzernamen. Ohne `verbindung` prüft \
                 es alle Zugänge.",
            ),
            lesend::<ProjektlisteParam>(
                "zephyr_projekte",
                "Projekte",
                "Listet die Projekte mit Schlüssel und Id: `anzahl` (Standard 25, höchstens 100) \
                 und `ab` (Startposition).",
            ),
            lesend::<ListenParam>(
                "zephyr_testfaelle",
                "Testfälle",
                "Listet die Testfälle eines Projekts: Schlüssel, Id, Name, Status, Priorität und \
                 Ordner-Id. `projekt` fehlt nur, wenn der Zugang eines nennt.",
            ),
            lesend::<TestfallParam>(
                "zephyr_testfall",
                "Testfall",
                "Ein Testfall mit Objektiv, Vorbedingung und Schritten (`/teststeps`) oder \
                 Skripttext (`/testscript`). Lange Texte werden über `max_zeichen` gekürzt \
                 (Standard 4000).",
            ),
            lesend::<ListenParam>(
                "zephyr_testzyklen",
                "Testzyklen",
                "Listet die Testzyklen eines Projekts: Schlüssel, Id, Name, Status und geplanten \
                 Zeitraum.",
            ),
            lesend::<ZyklusParam>(
                "zephyr_testzyklus",
                "Testzyklus",
                "Ein Testzyklus mit der Zahl der Ausführungen je Status. Gezählt werden höchstens \
                 500 Ausführungen; fehlen welche, steht das dabei.",
            ),
            lesend::<AusfuehrungenParam>(
                "zephyr_ausfuehrungen",
                "Ausführungen",
                "Listet die Ausführungen eines Projekts, optional nur die eines Testzyklus: Id, \
                 Schlüssel, Testfall-Id, Status, Zyklus und Umgebung.",
            ),
            lesend::<AusfuehrungParam>(
                "zephyr_ausfuehrung",
                "Ausführung",
                "Eine Ausführung mit Status, Zeitraum, Dauer, Art und Kommentar (gekürzt über \
                 `max_zeichen`, Standard 4000).",
            ),
            lesend::<StatuslisteParam>(
                "zephyr_status_liste",
                "Status",
                "Die erlaubten Status eines Projekts. `art` wählt die Statusart: `TEST_EXECUTION` \
                 (Standard, für Ergebnisse), `TEST_CASE`, `TEST_CYCLE` oder `TEST_PLAN`.",
            ),
            lesend::<OrdnerParam>(
                "zephyr_ordner",
                "Ordner",
                "Die Ordner eines Projekts, mit `art` `TEST_CASE` (Standard), `TEST_PLAN` oder \
                 `TEST_CYCLE`.",
            ),
        ]
    }

    /// Die drei verändernden Werkzeuge (brauchen `schreiben`).
    fn schreibende_werkzeuge(&self) -> Vec<Tool> {
        vec![
            schreibend::<TestfallAnlegenParam>(
                "zephyr_testfall_anlegen",
                "Testfall anlegen",
                "Legt einen Testfall an: Projekt, Name, optional Objektiv, Vorbedingung und Ordner. \
                 Das Projekt wird vorher mit `GET /projects/{id}` geprüft. Braucht den Schalter \
                 „schreiben“.",
            ),
            schreibend::<ZyklusHinzufuegenParam>(
                "zephyr_zyklus_testfaelle_hinzufuegen",
                "Testfälle einem Zyklus zuordnen",
                "Ordnet einem Testzyklus Testfälle zu: je Testfall entsteht eine Ausführung. \
                 `status` ist Pflicht (oder der Standard aus `/statuses`); jeder Testfall wird \
                 vorher auf Existenz und Projekt geprüft. Die Testfälle laufen **nacheinander**; \
                 bricht einer ab, nennt die Antwort die angelegten, den gescheiterten und die \
                 nicht versuchten. Nichts wird wiederholt.",
            ),
            schreibend::<ErgebnisParam>(
                "zephyr_ergebnis_eintragen",
                "Ergebnis eintragen",
                "Trägt einen Status und optional einen Kommentar an einer Ausführung ein. Der \
                 Statusname wird gegen `/statuses` (`statusType=TEST_EXECUTION`) geprüft; die \
                 Ausführung wird vorher gelesen, und der gelesene Wert von `automated` geht mit.",
            ),
        ]
    }

    /// Ein Werkzeug aufrufen. Auch der Weg, den die Tests nehmen.
    pub async fn aufrufen(&self, name: &str, argumente: Option<JsonObject>) -> Ergebnis {
        match name {
            "zephyr_status" => self.status(argumente),
            "zephyr_testen" => self.testen(argumente).await,
            "zephyr_projekte" => self.projekte(argumente).await,
            "zephyr_testfaelle" => self.testfaelle(argumente).await,
            "zephyr_testfall" => self.testfall(argumente).await,
            "zephyr_testzyklen" => self.testzyklen(argumente).await,
            "zephyr_testzyklus" => self.testzyklus(argumente).await,
            "zephyr_ausfuehrungen" => self.ausfuehrungen(argumente).await,
            "zephyr_ausfuehrung" => self.ausfuehrung(argumente).await,
            "zephyr_status_liste" => self.status_liste(argumente).await,
            "zephyr_ordner" => self.ordner(argumente).await,
            "zephyr_testfall_anlegen" => self.testfall_anlegen(argumente).await,
            "zephyr_zyklus_testfaelle_hinzufuegen" => {
                self.zyklus_testfaelle_hinzufuegen(argumente).await
            }
            "zephyr_ergebnis_eintragen" => self.ergebnis_eintragen(argumente).await,
            "zephyr_profile" => self.profile(argumente),
            sonst => Err(McpError::invalid_params(
                format!("Unbekanntes Werkzeug „{sonst}“."),
                None,
            )),
        }
    }

    /// `zephyr_profile`: bei einer Zugangsliste sind „die Angaben“ die Zugänge.
    fn profile(&self, roh: Option<JsonObject>) -> Ergebnis {
        self.status(roh)
    }

    /// Die Liste der Zugänge oder ein Fehler ohne ihren Inhalt.
    fn sammlung(&self) -> Result<&Sammlung, McpError> {
        self.zugaenge.as_ref().map_err(|fehler| {
            McpError::invalid_params(
                format!("{fehler} – bitte im quiver-Fenster unter Tools → {TITEL} prüfen."),
                None,
            )
        })
    }

    /// Der gewünschte Zugang; fehlt die Liste, kommt der Hinweis.
    fn waehlen(&self, name: Option<&str>) -> Result<Zugang, OhneProfil> {
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

    /// Der Zugang – aber nur, wenn der Schalter „schreiben“ an ist.
    ///
    /// Ist er aus, kommt der Hinweis **ohne** Anfrage an Zephyr.
    fn schreiben_erlaubt(&self, name: Option<&str>) -> Result<Zugang, OhneProfil> {
        let zugang = self.waehlen(name)?;
        if !zugang.schreiben_erlaubt() {
            return Err(OhneProfil::Hinweis(antwort(format::schreiben_aus(
                &zugang.name,
            ))));
        }
        Ok(zugang)
    }

    /// Das Projekt: aus dem Aufruf, sonst aus dem Zugang, sonst ein Hinweis.
    fn projekt(&self, zugang: &Zugang, genannt: Option<&str>) -> Result<String, OhneProfil> {
        zugang
            .projekt(genannt)
            .ok_or_else(|| OhneProfil::Hinweis(antwort(format::projekt_fehlt(&zugang.name))))
    }

    /// `nur`: darf in diesem Projekt geschrieben werden?
    fn nur_pruefen(&self, zugang: &Zugang, projekt: &str) -> Result<(), OhneProfil> {
        if zugang.projekt_erlaubt(projekt) {
            return Ok(());
        }
        Err(OhneProfil::Hinweis(antwort(format::nur_verboten(
            &zugang.name,
            projekt,
            &zugang.nur,
        ))))
    }

    /// Ein Client für diesen Zugang, oder ein MCP-Fehler ohne Token.
    fn client(zugang: &Zugang) -> Result<Client, McpError> {
        Client::neu(zugang).map_err(fehler_zu_mcp)
    }

    // ------------------------------------------------------------- lesend

    fn status(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: StatusParam = argumente(roh)?;
        let sammlung = self.sammlung()?;
        if sammlung.ist_leer() {
            return Ok(antwort(hinweis_zugang(TITEL)));
        }
        let gewaehlt: Vec<Zugang> = match p.verbindung.as_deref() {
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
        // Ohne Namen: alle Zugänge prüfen. Mit Namen: genau den einen.
        let zugaenge: Vec<Zugang> = match p.verbindung.as_deref() {
            Some(name) => vec![sammlung
                .waehlen(Some(name))
                .map_err(|fehler| McpError::invalid_params(fehler.to_string(), None))?
                .clone()],
            None => sammlung.alle().to_vec(),
        };

        let mut zeilen = Vec::new();
        for zugang in &zugaenge {
            let client = Self::client(zugang)?;
            let geantwortet = client
                .json(&Anfrage::holen("projects").mit_abfrage("maxResults", "1"))
                .await
                .map_err(fehler_zu_mcp)?;
            zeilen.push(format::testen(zugang, &geantwortet));
        }
        Ok(antwort(format!(
            "{} Zugang/Zugänge erreichbar:\n- {}",
            zeilen.len(),
            zeilen.join("\n- ")
        )))
    }

    async fn projekte(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: ProjektlisteParam = argumente(roh)?;
        let zugang = match self.waehlen(p.verbindung.as_deref()) {
            Ok(zugang) => zugang,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let client = Self::client(&zugang)?;
        let start = ab(p.ab);
        let baum = client
            .json(
                &Anfrage::holen("projects")
                    .mit_abfrage("maxResults", anzahl(p.anzahl).to_string())
                    .mit_abfrage("startAt", start.to_string()),
            )
            .await
            .map_err(fehler_zu_mcp)?;
        client.folgeseite_pruefen(&baum).map_err(fehler_zu_mcp)?;
        Ok(antwort(format::projekte(&baum, start)))
    }

    async fn testfaelle(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: ListenParam = argumente(roh)?;
        let zugang = match self.waehlen(p.verbindung.as_deref()) {
            Ok(zugang) => zugang,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let projekt = match self.projekt(&zugang, p.projekt.as_deref()) {
            Ok(projekt) => projekt,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let start = ab(p.ab);
        let client = Self::client(&zugang)?;
        let baum = client
            .json(
                &Anfrage::holen("testcases")
                    .mit_abfrage("projectKey", &projekt)
                    .mit_abfrage("maxResults", anzahl(p.anzahl).to_string())
                    .mit_abfrage("startAt", start.to_string()),
            )
            .await
            .map_err(fehler_zu_mcp)?;
        client.folgeseite_pruefen(&baum).map_err(fehler_zu_mcp)?;

        let mut namen = Namen::neu();
        // Status und Priorität sind Verweise: ihre Namen kommen aus eigenen
        // Endpunkten – und nur, wenn die Liste sie überhaupt nennt.
        if verweis_vorhanden(&baum, "status") {
            namen
                .status_laden(&client, &projekt, namen::STATUS_TESTFALL)
                .await
                .map_err(fehler_zu_mcp)?;
        }
        if verweis_vorhanden(&baum, "priority") {
            namen
                .prio_laden(&client, &projekt)
                .await
                .map_err(fehler_zu_mcp)?;
        }
        Ok(antwort(format::testfaelle(&baum, start, &namen, &projekt)))
    }

    async fn testfall(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: TestfallParam = argumente(roh)?;
        let zugang = match self.waehlen(p.verbindung.as_deref()) {
            Ok(zugang) => zugang,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let schluessel = pfadwert(&p.schluessel, "schluessel").map_err(fehler_zu_mcp)?;
        let client = Self::client(&zugang)?;
        let baum = client
            .json(&Anfrage::holen(format!("testcases/{schluessel}")))
            .await
            .map_err(fehler_zu_mcp)?;

        let mut namen = Namen::neu();
        let projekt = namen
            .projekt_key(&client, baum.get("project").unwrap_or(&Value::Null))
            .await
            .map_err(fehler_zu_mcp)?
            .ok_or_else(|| {
                McpError::internal_error(
                    "Die Antwort zum Testfall nennt kein lesbares Projekt – es wurde nichts \
                     geändert."
                        .to_string(),
                    None,
                )
            })?;

        // Ein Testfall hat entweder Schritte oder ein Skript. `/testscript`
        // antwortet mit 422, wenn Schritte da sind, `/teststeps` mit 422, wenn
        // es ein Skript ist. Beides ist eine Auskunft, kein Fehler.
        let skript = match client
            .json(&Anfrage::holen(format!(
                "testcases/{schluessel}/testscript"
            )))
            .await
        {
            Ok(wert) => Some(wert),
            Err(fehler) if fehler.ist_nicht_verarbeitbar() => None,
            Err(fehler) => return Err(fehler_zu_mcp(fehler)),
        };
        let schritte = if skript.is_some() {
            Vec::new()
        } else {
            match client
                .json(
                    &Anfrage::holen(format!("testcases/{schluessel}/teststeps"))
                        .mit_abfrage("maxResults", format::MAX_ANZAHL.to_string()),
                )
                .await
            {
                Ok(wert) => daten::liste(&wert, "values").to_vec(),
                Err(fehler) if fehler.ist_nicht_verarbeitbar() => Vec::new(),
                Err(fehler) => return Err(fehler_zu_mcp(fehler)),
            }
        };

        namen
            .status_laden(&client, &projekt, namen::STATUS_TESTFALL)
            .await
            .map_err(fehler_zu_mcp)?;
        namen
            .prio_laden(&client, &projekt)
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::testfall(
            &baum,
            skript.as_ref(),
            &schritte,
            zeichen(p.max_zeichen),
            &namen,
            &projekt,
        )))
    }

    async fn testzyklen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: ListenParam = argumente(roh)?;
        let zugang = match self.waehlen(p.verbindung.as_deref()) {
            Ok(zugang) => zugang,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let projekt = match self.projekt(&zugang, p.projekt.as_deref()) {
            Ok(projekt) => projekt,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let start = ab(p.ab);
        let client = Self::client(&zugang)?;
        let baum = client
            .json(
                &Anfrage::holen("testcycles")
                    .mit_abfrage("projectKey", &projekt)
                    .mit_abfrage("maxResults", anzahl(p.anzahl).to_string())
                    .mit_abfrage("startAt", start.to_string()),
            )
            .await
            .map_err(fehler_zu_mcp)?;
        client.folgeseite_pruefen(&baum).map_err(fehler_zu_mcp)?;

        let mut namen = Namen::neu();
        if verweis_vorhanden(&baum, "status") {
            namen
                .status_laden(&client, &projekt, namen::STATUS_ZYKLUS)
                .await
                .map_err(fehler_zu_mcp)?;
        }
        Ok(antwort(format::testzyklen(&baum, start, &namen, &projekt)))
    }

    async fn testzyklus(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: ZyklusParam = argumente(roh)?;
        let zugang = match self.waehlen(p.verbindung.as_deref()) {
            Ok(zugang) => zugang,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let schluessel = pfadwert(&p.schluessel, "schluessel").map_err(fehler_zu_mcp)?;
        let client = Self::client(&zugang)?;
        let zyklus = client
            .json(&Anfrage::holen(format!("testcycles/{schluessel}")))
            .await
            .map_err(fehler_zu_mcp)?;

        // Das Projekt kommt aus dem Verweis der Antwort, nicht aus dem Aufruf.
        let mut namen = Namen::neu();
        let projekt = namen
            .projekt_key(&client, zyklus.get("project").unwrap_or(&Value::Null))
            .await
            .map_err(fehler_zu_mcp)?
            .ok_or_else(|| {
                McpError::internal_error(
                    "Die Antwort zum Testzyklus nennt kein lesbares Projekt – es wurde nichts \
                     geändert."
                        .to_string(),
                    None,
                )
            })?;
        // `testCycle` ist ein **Schlüssel**-Filter: der Agent darf auch eine Id
        // angegeben haben, gesendet wird der Schlüssel aus der Antwort.
        let zyklus_key =
            daten::text(&zyklus, "key").unwrap_or_else(|| p.schluessel.trim().to_string());

        let mut ausfuehrungen = Vec::new();
        let mut gesamt = None;
        let mut start = 0u32;
        for _ in 0..ZAEHL_SEITEN {
            let seite = client
                .json(
                    &Anfrage::holen("testexecutions")
                        .mit_abfrage("projectKey", &projekt)
                        .mit_abfrage("testCycle", &zyklus_key)
                        .mit_abfrage("maxResults", "100")
                        .mit_abfrage("startAt", start.to_string()),
                )
                .await
                .map_err(fehler_zu_mcp)?;
            client.folgeseite_pruefen(&seite).map_err(fehler_zu_mcp)?;
            if let Some(gesamt_neu) = daten::gesamt(&seite) {
                gesamt = Some(gesamt_neu);
            }
            let werte = daten::liste(&seite, "values");
            if werte.is_empty() {
                break;
            }
            ausfuehrungen.extend(werte.iter().cloned());
            if !daten::mehr(&seite, start, werte.len()) {
                break;
            }
            start += werte.len() as u32;
        }

        namen
            .status_laden(&client, &projekt, namen::STATUS_ZYKLUS)
            .await
            .map_err(fehler_zu_mcp)?;
        if !ausfuehrungen.is_empty() {
            namen
                .status_laden(&client, &projekt, namen::STATUS_AUSFUEHRUNG)
                .await
                .map_err(fehler_zu_mcp)?;
        }
        Ok(antwort(format::testzyklus(
            &zyklus,
            &ausfuehrungen,
            gesamt,
            &namen,
            &projekt,
        )))
    }

    async fn ausfuehrungen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: AusfuehrungenParam = argumente(roh)?;
        let zugang = match self.waehlen(p.verbindung.as_deref()) {
            Ok(zugang) => zugang,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let projekt = match self.projekt(&zugang, p.projekt.as_deref()) {
            Ok(projekt) => projekt,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let start = ab(p.ab);
        let mut anfrage = Anfrage::holen("testexecutions")
            .mit_abfrage("projectKey", &projekt)
            .mit_abfrage("maxResults", anzahl(p.anzahl).to_string())
            .mit_abfrage("startAt", start.to_string());
        if let Some(zyklus) = p.zyklus.as_deref().map(str::trim).filter(|z| !z.is_empty()) {
            // Der Zyklus ist ein Filterwert, kein Pfad: kodiert wird er trotzdem.
            anfrage = anfrage.mit_abfrage("testCycle", zyklus);
        }
        let client = Self::client(&zugang)?;
        let baum = client.json(&anfrage).await.map_err(fehler_zu_mcp)?;
        client.folgeseite_pruefen(&baum).map_err(fehler_zu_mcp)?;

        let mut namen = Namen::neu();
        if verweis_vorhanden(&baum, "testExecutionStatus") {
            namen
                .status_laden(&client, &projekt, namen::STATUS_AUSFUEHRUNG)
                .await
                .map_err(fehler_zu_mcp)?;
        }
        namen_aufloesen(&client, &mut namen, daten::liste(&baum, "values")).await?;
        Ok(antwort(format::ausfuehrungen(
            &baum, start, &namen, &projekt,
        )))
    }

    async fn ausfuehrung(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: AusfuehrungParam = argumente(roh)?;
        let zugang = match self.waehlen(p.verbindung.as_deref()) {
            Ok(zugang) => zugang,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let kennung = pfadwert(&p.ausfuehrung, "ausfuehrung").map_err(fehler_zu_mcp)?;
        let client = Self::client(&zugang)?;
        let baum = client
            .json(&Anfrage::holen(format!("testexecutions/{kennung}")))
            .await
            .map_err(fehler_zu_mcp)?;

        let mut namen = Namen::neu();
        let projekt = namen
            .projekt_key(&client, baum.get("project").unwrap_or(&Value::Null))
            .await
            .map_err(fehler_zu_mcp)?
            .ok_or_else(|| {
                McpError::internal_error(
                    "Die Antwort zur Ausführung nennt kein lesbares Projekt.".to_string(),
                    None,
                )
            })?;
        namen
            .status_laden(&client, &projekt, namen::STATUS_AUSFUEHRUNG)
            .await
            .map_err(fehler_zu_mcp)?;
        namen_aufloesen(&client, &mut namen, std::slice::from_ref(&baum)).await?;
        Ok(antwort(format::ausfuehrung(
            &baum,
            zeichen(p.max_zeichen),
            &namen,
            &projekt,
        )))
    }

    async fn status_liste(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: StatuslisteParam = argumente(roh)?;
        let zugang = match self.waehlen(p.verbindung.as_deref()) {
            Ok(zugang) => zugang,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let projekt = match self.projekt(&zugang, p.projekt.as_deref()) {
            Ok(projekt) => projekt,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let art = status_art(p.art.as_deref())?;
        let start = ab(p.ab);
        let client = Self::client(&zugang)?;
        let baum = client
            .json(
                &Anfrage::holen("statuses")
                    .mit_abfrage("projectKey", &projekt)
                    .mit_abfrage("statusType", &art)
                    .mit_abfrage("maxResults", anzahl(p.anzahl).to_string())
                    .mit_abfrage("startAt", start.to_string()),
            )
            .await
            .map_err(fehler_zu_mcp)?;
        client.folgeseite_pruefen(&baum).map_err(fehler_zu_mcp)?;
        Ok(antwort(format::status_liste(&baum, start)))
    }

    async fn ordner(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: OrdnerParam = argumente(roh)?;
        let zugang = match self.waehlen(p.verbindung.as_deref()) {
            Ok(zugang) => zugang,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let projekt = match self.projekt(&zugang, p.projekt.as_deref()) {
            Ok(projekt) => projekt,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let art = ordner_art(p.art.as_deref())?;
        let start = ab(p.ab);
        let client = Self::client(&zugang)?;
        let baum = client
            .json(
                &Anfrage::holen("folders")
                    .mit_abfrage("projectKey", &projekt)
                    .mit_abfrage("folderType", art)
                    .mit_abfrage("maxResults", anzahl(p.anzahl).to_string())
                    .mit_abfrage("startAt", start.to_string()),
            )
            .await
            .map_err(fehler_zu_mcp)?;
        client.folgeseite_pruefen(&baum).map_err(fehler_zu_mcp)?;
        Ok(antwort(format::ordner(&baum, start)))
    }

    // ---------------------------------------------------------- schreibend

    async fn testfall_anlegen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: TestfallAnlegenParam = argumente(roh)?;
        let zugang = match self.schreiben_erlaubt(p.verbindung.as_deref()) {
            Ok(zugang) => zugang,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let name = pflicht(&p.name, "name")?;
        let gewuenscht = match self.projekt(&zugang, p.projekt.as_deref()) {
            Ok(projekt) => projekt,
            Err(ohne) => return ohne.in_ergebnis(),
        };

        let client = Self::client(&zugang)?;
        // Nicht dem Agenten glauben: `GET /projects/{id-or-key}` nennt den
        // Schlüssel des Servers – und ob es das Projekt überhaupt gibt.
        let projekt = self
            .projekt_holen(&client, &zugang, &gewuenscht, "angelegt")
            .await?;
        if let Err(ohne) = self.nur_pruefen(&zugang, &projekt) {
            return ohne.in_ergebnis();
        }

        let mut rumpf = json!({ "projectKey": projekt, "name": name });
        if let Some(wert) = p
            .objektiv
            .as_deref()
            .map(str::trim)
            .filter(|w| !w.is_empty())
        {
            rumpf["objective"] = json!(wert);
        }
        if let Some(wert) = p
            .vorbedingung
            .as_deref()
            .map(str::trim)
            .filter(|w| !w.is_empty())
        {
            rumpf["precondition"] = json!(wert);
        }
        if let Some(ordner) = p.ordner {
            rumpf["folderId"] = json!(ordner);
        }

        let angelegt = client
            .json(&Anfrage::post("testcases", rumpf.to_string()))
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::testfall_angelegt(
            &angelegt, name, &projekt,
        )))
    }

    async fn zyklus_testfaelle_hinzufuegen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: ZyklusHinzufuegenParam = argumente(roh)?;
        let zugang = match self.schreiben_erlaubt(p.verbindung.as_deref()) {
            Ok(zugang) => zugang,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let zyklus = pfadwert(&p.zyklus, "zyklus").map_err(fehler_zu_mcp)?;
        let testfaelle = testfaelle_pruefen(&p.testfaelle)?;

        let client = Self::client(&zugang)?;
        // Projekt und Status kommen aus Antworten des Servers.
        let gelesen = client
            .json(&Anfrage::holen(format!("testcycles/{zyklus}")))
            .await
            .map_err(fehler_zu_mcp)?;
        let mut namen = Namen::neu();
        let projekt = namen
            .projekt_key(&client, gelesen.get("project").unwrap_or(&Value::Null))
            .await
            .map_err(fehler_zu_mcp)?
            .ok_or_else(|| {
                McpError::internal_error(
                    "Die Antwort zum Testzyklus nennt kein lesbares Projekt – es wurde nichts \
                     angelegt."
                        .to_string(),
                    None,
                )
            })?;
        // Gesendet wird der Schlüssel **des Servers**: der Agent darf auch eine
        // Id angegeben haben.
        let zyklus_key = daten::text(&gelesen, "key").ok_or_else(|| {
            McpError::internal_error(
                "Die Antwort zum Testzyklus nennt keinen Schlüssel – es wurde nichts angelegt."
                    .to_string(),
                None,
            )
        })?;
        if let Err(ohne) = self.nur_pruefen(&zugang, &projekt) {
            return ohne.in_ergebnis();
        }

        // `statusName` ist Pflicht. Erlaubt ist der genannte Status oder – nur
        // wenn `/statuses` einen Standard nennt – dieser Standard.
        namen
            .status_laden(&client, &projekt, namen::STATUS_AUSFUEHRUNG)
            .await
            .map_err(fehler_zu_mcp)?;
        let status = match p
            .status
            .as_deref()
            .map(str::trim)
            .filter(|status| !status.is_empty())
        {
            Some(gesucht) => namen
                .status_namen(namen::STATUS_AUSFUEHRUNG, &projekt)
                .into_iter()
                .find(|name| name.eq_ignore_ascii_case(gesucht))
                .ok_or_else(|| unbekannter_status(&namen, &projekt, gesucht))?,
            None => namen
                .status_standard(namen::STATUS_AUSFUEHRUNG, &projekt)
                .ok_or_else(|| {
                    McpError::invalid_params(
                        format!(
                            "`status` fehlt und /statuses nennt für {projekt} keinen Standard. \
                             Erlaubt sind: {}. Es wurde nichts angelegt.",
                            statusliste(&namen, &projekt)
                        ),
                        None,
                    )
                })?,
        };
        let umgebung = p
            .umgebung
            .as_deref()
            .map(str::trim)
            .filter(|umgebung| !umgebung.is_empty())
            .map(str::to_string);

        // Nacheinander, nichts wiederholen: ob ein Aufruf ankam, weiß nur der
        // Server. Jeder Testfall wird **vorher** geprüft (Existenz und Projekt).
        let mut angelegt: Vec<(String, Value)> = Vec::new();
        let mut gescheitert: Option<(String, String)> = None;
        let mut nicht_versucht: Vec<String> = Vec::new();
        for (nummer, testfall) in testfaelle.iter().enumerate() {
            let schluessel = match pfadwert(testfall, "testfaelle") {
                Ok(schluessel) => schluessel,
                Err(fehler) => {
                    gescheitert = Some((testfall.clone(), fehler.to_string()));
                    nicht_versucht = testfaelle[nummer + 1..].to_vec();
                    break;
                }
            };
            // Der Schlüssel des Servers – nicht der Text des Agenten.
            let testfall_key = match self
                .testfall_im_projekt(&client, &mut namen, &schluessel, &projekt)
                .await
            {
                Ok(testfall_key) => testfall_key,
                Err(fehler) => {
                    gescheitert = Some((testfall.clone(), fehler));
                    nicht_versucht = testfaelle[nummer + 1..].to_vec();
                    break;
                }
            };

            let mut rumpf = json!({
                "projectKey": projekt,
                "testCaseKey": testfall_key,
                "testCycleKey": zyklus_key,
                "statusName": status,
            });
            if let Some(umgebung) = &umgebung {
                rumpf["environmentName"] = json!(umgebung);
            }
            match client
                .json(&Anfrage::post("testexecutions", rumpf.to_string()))
                .await
            {
                Ok(antwort) => angelegt.push((testfall.clone(), antwort)),
                Err(fehler) => {
                    gescheitert = Some((testfall.clone(), fehler.to_string()));
                    nicht_versucht = testfaelle[nummer + 1..].to_vec();
                    break;
                }
            }
        }

        Ok(antwort(format::zyklus_hinzugefuegt(
            &zyklus_key,
            &angelegt,
            gescheitert
                .as_ref()
                .map(|(testfall, fehler)| (testfall.as_str(), fehler.clone())),
            &nicht_versucht,
        )))
    }

    async fn ergebnis_eintragen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: ErgebnisParam = argumente(roh)?;
        let zugang = match self.schreiben_erlaubt(p.verbindung.as_deref()) {
            Ok(zugang) => zugang,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let kennung = pfadwert(&p.ausfuehrung, "ausfuehrung").map_err(fehler_zu_mcp)?;
        let status = pflicht(&p.status, "status")?;

        let client = Self::client(&zugang)?;
        // Vorher lesen: das Projekt steht im Verweis der Antwort, und
        // `automated` (Standard `true`) muss mit.
        let gelesen = client
            .json(&Anfrage::holen(format!("testexecutions/{kennung}")))
            .await
            .map_err(fehler_zu_mcp)?;
        let mut namen = Namen::neu();
        let projekt = namen
            .projekt_key(&client, gelesen.get("project").unwrap_or(&Value::Null))
            .await
            .map_err(fehler_zu_mcp)?
            .ok_or_else(|| {
                McpError::internal_error(
                    "Die Antwort zur Ausführung nennt kein lesbares Projekt – es wurde nichts \
                     geändert."
                        .to_string(),
                    None,
                )
            })?;
        if let Err(ohne) = self.nur_pruefen(&zugang, &projekt) {
            return ohne.in_ergebnis();
        }

        // Unbekannter Status: Meldung mit den erlaubten Namen, keine Änderung.
        namen
            .status_laden(&client, &projekt, namen::STATUS_AUSFUEHRUNG)
            .await
            .map_err(fehler_zu_mcp)?;
        let status_name = namen
            .status_namen(namen::STATUS_AUSFUEHRUNG, &projekt)
            .into_iter()
            .find(|name| name.eq_ignore_ascii_case(status))
            .ok_or_else(|| unbekannter_status(&namen, &projekt, status))?;

        let id = daten::text(&gelesen, "id").unwrap_or_else(|| kennung.clone());
        let anzeige = format::ausfuehrung_kennung(&gelesen);
        let rumpf = format::aktualisierung(&gelesen, &status_name, p.kommentar.as_deref());
        let pfad = format!(
            "testexecutions/{}",
            pfadwert(&id, "ausfuehrung").map_err(fehler_zu_mcp)?
        );

        client
            .anfrage(&Anfrage::neu(Methode::Put, pfad).mit_inhalt(rumpf.to_string()))
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::ergebnis_gesetzt(
            &anzeige,
            &status_name,
            p.kommentar.as_deref(),
            daten::wert(&gelesen, "automated").and_then(Value::as_bool),
        )))
    }

    // ------------------------------------------------------------- innen

    /// Den Projektschlüssel **des Servers** holen (`GET /projects/{id-or-key}`).
    ///
    /// Das prüft zugleich, ob es das Projekt gibt. Was der Agent geschrieben
    /// hat, gilt nicht: der Schlüssel kommt aus der Antwort.
    async fn projekt_holen(
        &self,
        client: &Client,
        zugang: &Zugang,
        gewuenscht: &str,
        tat: &str,
    ) -> Result<String, McpError> {
        let pfad = format!(
            "projects/{}",
            pfadwert(gewuenscht, "projekt").map_err(fehler_zu_mcp)?
        );
        match client.json(&Anfrage::holen(pfad)).await {
            Ok(baum) => daten::text(&baum, "key").ok_or_else(|| {
                McpError::internal_error(
                    format!(
                        "Die Antwort zu „{gewuenscht}“ nennt keinen Projektschlüssel – es wurde \
                         nichts {tat}."
                    ),
                    None,
                )
            }),
            Err(fehler) if fehler.ist_nicht_gefunden() => Err(McpError::invalid_params(
                format!(
                    "Das Projekt „{gewuenscht}“ gibt es im Zugang „{}“ nicht (GET /projects/{} \
                     antwortet 404). Es wurde nichts {tat}.",
                    zugang.name,
                    client::kodieren(gewuenscht.trim()),
                ),
                None,
            )),
            Err(fehler) => Err(fehler_zu_mcp(fehler)),
        }
    }

    /// Prüft einen Testfall für die Zuordnung: gibt es ihn, und gehört er in
    /// dieses Projekt (`nur` gilt damit auch für ihn)?
    ///
    /// Liefert den Testfallschlüssel **des Servers** zurück.
    async fn testfall_im_projekt(
        &self,
        client: &Client,
        namen: &mut Namen,
        schluessel: &str,
        projekt: &str,
    ) -> Result<String, String> {
        let baum = match client
            .json(&Anfrage::holen(format!("testcases/{schluessel}")))
            .await
        {
            Ok(baum) => baum,
            Err(fehler) if fehler.ist_nicht_gefunden() => {
                return Err("den Testfall gibt es nicht (HTTP 404) – nichts angelegt".to_string())
            }
            Err(fehler) => return Err(fehler.to_string()),
        };
        let seines = namen
            .projekt_key(client, baum.get("project").unwrap_or(&Value::Null))
            .await
            .map_err(|fehler| fehler.to_string())?;
        match seines {
            Some(seines) if seines.eq_ignore_ascii_case(projekt) => daten::text(&baum, "key")
                .ok_or_else(|| {
                    "die Antwort zum Testfall nennt keinen Schlüssel – nichts angelegt".to_string()
                }),
            Some(seines) => Err(format!(
                "der Testfall gehört zum Projekt {seines}, nicht zu {projekt} – nichts angelegt"
            )),
            None => Err(
                "die Antwort zum Testfall nennt kein lesbares Projekt – nichts angelegt"
                    .to_string(),
            ),
        }
    }
}

/// Löst die Verweise einer Ausführungsliste auf (`environment`, `testCycle`).
///
/// Jede Id wird über [`Namen`] nur einmal gefragt, auch wenn sie in mehreren
/// Einträgen steht.
async fn namen_aufloesen(
    client: &Client,
    namen: &mut Namen,
    ausfuehrungen: &[Value],
) -> Result<(), McpError> {
    for ausfuehrung in ausfuehrungen {
        if let Some(umgebung) = ausfuehrung
            .get("environment")
            .filter(|wert| !wert.is_null())
        {
            namen
                .umgebung_name(client, umgebung)
                .await
                .map_err(fehler_zu_mcp)?;
        }
        if let Some(zyklus) = ausfuehrung.get("testCycle").filter(|wert| !wert.is_null()) {
            namen
                .zyklus_key(client, zyklus)
                .await
                .map_err(fehler_zu_mcp)?;
        }
    }
    Ok(())
}

/// Nennt die Liste mindestens einen Eintrag mit diesem Verweis?
fn verweis_vorhanden(baum: &Value, feld: &str) -> bool {
    daten::liste(baum, "values")
        .iter()
        .any(|eintrag| eintrag.get(feld).is_some_and(|wert| !wert.is_null()))
}

/// Die erlaubten Statusnamen als Text; `(keine)`, wenn es keine gibt.
fn statusliste(namen: &Namen, projekt: &str) -> String {
    let liste = namen.status_namen(namen::STATUS_AUSFUEHRUNG, projekt);
    if liste.is_empty() {
        "(keine)".to_string()
    } else {
        liste.join(", ")
    }
}

/// Ein Status, den der Server nicht kennt – mit den erlaubten Namen.
fn unbekannter_status(namen: &Namen, projekt: &str, gesucht: &str) -> McpError {
    let zusatz = if namen.status_vollstaendig(namen::STATUS_AUSFUEHRUNG, projekt) {
        ""
    } else {
        " (die Liste war länger, als gelesen wurde)"
    };
    McpError::invalid_params(
        format!(
            "Den Status „{gesucht}“ gibt es im Projekt {projekt} für Ausführungen nicht. Erlaubt \
             sind: {}{zusatz}. Es wurde nichts geändert.",
            statusliste(namen, projekt)
        ),
        None,
    )
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

/// Ein veränderndes Werkzeug (`destructive_hint`, Aufgabe 15).
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

/// Wie viele Einträge eine Liste zeigt: Standard 25, höchstens 100.
fn anzahl(wert: Option<u32>) -> u32 {
    wert.unwrap_or(format::STANDARD_ANZAHL)
        .clamp(1, format::MAX_ANZAHL)
}

/// Die Startposition einer Liste: `startAt` erlaubt höchstens 1 000 000.
fn ab(wert: Option<u32>) -> u32 {
    wert.unwrap_or(0).min(1_000_000)
}

/// Die Ordnerart prüfen: `TEST_CASE`, `TEST_PLAN` oder `TEST_CYCLE`.
fn ordner_art(wert: Option<&str>) -> Result<String, McpError> {
    art_pruefen(
        wert,
        "TEST_CASE",
        &["TEST_CASE", "TEST_PLAN", "TEST_CYCLE"],
        "art",
    )
}

/// Die Statusart prüfen: alle vier Arten aus `StatusType`.
fn status_art(wert: Option<&str>) -> Result<String, McpError> {
    art_pruefen(
        wert,
        namen::STATUS_AUSFUEHRUNG,
        &[
            namen::STATUS_TESTFALL,
            namen::STATUS_PLAN,
            namen::STATUS_ZYKLUS,
            namen::STATUS_AUSFUEHRUNG,
        ],
        "art",
    )
}

/// Eine von mehreren erlaubten Arten, groß geschrieben.
fn art_pruefen(
    wert: Option<&str>,
    standard: &str,
    erlaubt: &[&str],
    feld: &str,
) -> Result<String, McpError> {
    let art = wert
        .map(str::trim)
        .filter(|art| !art.is_empty())
        .map(str::to_ascii_uppercase)
        .unwrap_or_else(|| standard.to_string());
    if erlaubt.contains(&art.as_str()) {
        return Ok(art);
    }
    Err(McpError::invalid_params(
        format!("`{feld}` muss {} sein, nicht „{art}“.", erlaubt.join(", ")),
        None,
    ))
}

/// Die Testfallschlüssel einer Zuordnung prüfen (mindestens einer, keiner leer).
fn testfaelle_pruefen(roh: &[String]) -> Result<Vec<String>, McpError> {
    let mut liste = Vec::with_capacity(roh.len());
    for eintrag in roh {
        let wert = eintrag.trim();
        if wert.is_empty() {
            return Err(McpError::invalid_params(
                "In `testfaelle` steht ein leerer Schlüssel.".to_string(),
                None,
            ));
        }
        liste.push(wert.to_string());
    }
    if liste.is_empty() {
        return Err(McpError::invalid_params(
            "Bitte mindestens einen Testfall in `testfaelle` angeben.".to_string(),
            None,
        ));
    }
    Ok(liste)
}

/// Aus einem Zephyr-Fehler wird ein MCP-Fehler – mit Verbindung, nie mit Token.
fn fehler_zu_mcp(fehler: ZephyrFehler) -> McpError {
    let text = fehler.to_string();
    match fehler {
        ZephyrFehler::Anmeldung { .. }
        | ZephyrFehler::Rechte { .. }
        | ZephyrFehler::Http { .. }
        | ZephyrFehler::Weiterleitung { .. }
        | ZephyrFehler::FremdeFolgeseite { .. }
        | ZephyrFehler::Parameter { .. } => McpError::invalid_params(text, None),
        _ => McpError::internal_error(text, None),
    }
}

// ------------------------------------------------------------- Parameter

/// Parameter von `zephyr_status`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct StatusParam {
    /// Nur diesen Zugang zeigen. Ohne Angabe: alle.
    #[serde(default)]
    pub verbindung: Option<String>,
}

/// Parameter von `zephyr_testen`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct VerbindungParam {
    /// Zugang; bei genau einem darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
}

/// Parameter von `zephyr_projekte`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ProjektlisteParam {
    /// Zugang; bei genau einem darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Höchstens so viele Einträge (Standard 25, höchstens 100).
    #[serde(default)]
    pub anzahl: Option<u32>,
    /// Startposition in der Liste (Standard 0).
    #[serde(default)]
    pub ab: Option<u32>,
}

/// Parameter der Werkzeuge, die eine Liste eines Projekts zeigen.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ListenParam {
    /// Zugang; bei genau einem darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Projekt; ohne Angabe gilt das Projekt des Zugangs.
    #[serde(default)]
    pub projekt: Option<String>,
    /// Höchstens so viele Einträge (Standard 25, höchstens 100).
    #[serde(default)]
    pub anzahl: Option<u32>,
    /// Startposition in der Liste (Standard 0).
    #[serde(default)]
    pub ab: Option<u32>,
}

/// Parameter von `zephyr_testfall`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct TestfallParam {
    /// Zugang; bei genau einem darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Testfallschlüssel, z. B. `QA-T1`.
    pub schluessel: String,
    /// Höchstens so viele Zeichen je Text (Standard 4000).
    #[serde(default)]
    pub max_zeichen: Option<u32>,
}

/// Parameter von `zephyr_testzyklus`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ZyklusParam {
    /// Zugang; bei genau einem darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Testzyklusschlüssel oder Id, z. B. `QA-R1` oder `31`.
    pub schluessel: String,
}

/// Parameter von `zephyr_ausfuehrungen`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct AusfuehrungenParam {
    /// Zugang; bei genau einem darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Projekt; ohne Angabe gilt das Projekt des Zugangs.
    #[serde(default)]
    pub projekt: Option<String>,
    /// Nur Ausführungen dieses Testzyklus (Schlüssel, z. B. `QA-R1` – kein
    /// Filter über die Id).
    #[serde(default)]
    pub zyklus: Option<String>,
    /// Höchstens so viele Einträge (Standard 25, höchstens 100).
    #[serde(default)]
    pub anzahl: Option<u32>,
    /// Startposition in der Liste (Standard 0).
    #[serde(default)]
    pub ab: Option<u32>,
}

/// Parameter von `zephyr_ausfuehrung`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct AusfuehrungParam {
    /// Zugang; bei genau einem darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Id oder Schlüssel der Ausführung, z. B. `101` oder `QA-E1`.
    pub ausfuehrung: String,
    /// Höchstens so viele Zeichen je Text (Standard 4000).
    #[serde(default)]
    pub max_zeichen: Option<u32>,
}

/// Parameter von `zephyr_status_liste`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct StatuslisteParam {
    /// Zugang; bei genau einem darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Projekt; ohne Angabe gilt das Projekt des Zugangs.
    #[serde(default)]
    pub projekt: Option<String>,
    /// `TEST_EXECUTION` (Standard), `TEST_CASE`, `TEST_CYCLE` oder `TEST_PLAN`.
    #[serde(default)]
    pub art: Option<String>,
    /// Höchstens so viele Einträge (Standard 25, höchstens 100).
    #[serde(default)]
    pub anzahl: Option<u32>,
    /// Startposition in der Liste (Standard 0).
    #[serde(default)]
    pub ab: Option<u32>,
}

/// Parameter von `zephyr_ordner`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct OrdnerParam {
    /// Zugang; bei genau einem darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Projekt; ohne Angabe gilt das Projekt des Zugangs.
    #[serde(default)]
    pub projekt: Option<String>,
    /// `TEST_CASE` (Standard), `TEST_PLAN` oder `TEST_CYCLE`.
    #[serde(default)]
    pub art: Option<String>,
    /// Höchstens so viele Einträge (Standard 25, höchstens 100).
    #[serde(default)]
    pub anzahl: Option<u32>,
    /// Startposition in der Liste (Standard 0).
    #[serde(default)]
    pub ab: Option<u32>,
}

/// Parameter von `zephyr_testfall_anlegen`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct TestfallAnlegenParam {
    /// Zugang; bei genau einem darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Projekt; ohne Angabe gilt das Projekt des Zugangs.
    #[serde(default)]
    pub projekt: Option<String>,
    /// Name des Testfalls (Pflicht).
    pub name: String,
    /// Objektiv (Ziel des Tests).
    #[serde(default)]
    pub objektiv: Option<String>,
    /// Vorbedingung.
    #[serde(default)]
    pub vorbedingung: Option<String>,
    /// Ordner-Id aus `zephyr_ordner`.
    #[serde(default)]
    pub ordner: Option<u32>,
}

/// Parameter von `zephyr_zyklus_testfaelle_hinzufuegen`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ZyklusHinzufuegenParam {
    /// Zugang; bei genau einem darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Testzyklusschlüssel oder Id, z. B. `QA-R1` oder `31`.
    pub zyklus: String,
    /// Testfallschlüssel; je Testfall entsteht eine Ausführung.
    pub testfaelle: Vec<String>,
    /// Status der Ausführung; Pflicht, außer `/statuses` nennt einen Standard.
    #[serde(default)]
    pub status: Option<String>,
    /// Umgebung, z. B. `Test`.
    #[serde(default)]
    pub umgebung: Option<String>,
}

/// Parameter von `zephyr_ergebnis_eintragen`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ErgebnisParam {
    /// Zugang; bei genau einem darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Id oder Schlüssel der Ausführung, z. B. `101` oder `QA-E1`.
    pub ausfuehrung: String,
    /// Statusname aus `zephyr_status_liste`, z. B. `Pass`.
    pub status: String,
    /// Kommentar zum Ergebnis.
    #[serde(default)]
    pub kommentar: Option<String>,
}

// -------------------------------------------------------------- MCP-Anker

impl ServerHandler for ZephyrServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(quiver_tools_kern::mcp::server_info(&self.zugang))
            .with_instructions(format!(
                "Zephyr Scale Cloud über die REST-API v2: Testfälle, Testzyklen, Ausführungen, \
                 Status und Ordner lesen. Verändernd gehen einen Testfall anlegen, Testfälle einem \
                 Zyklus zuordnen und ein Ergebnis eintragen – aber nur, wenn der Schalter \
                 „schreiben“ für den Zugang an ist (Standard: aus). Ist er aus, antworten die \
                 verändernden Werkzeuge mit dem Hinweis und ohne Anfrage an Zephyr. Ist `nur` \
                 gesetzt, wird außerhalb dieser Projekte nicht geschrieben; den Projektschlüssel \
                 liest das Werkzeug vom Server, nicht aus dem Aufruf. Die {MEHRFACH_TITEL} \
                 (ein Element heißt {MEHRFACH_EINZAHL}) legt der User im quiver-Fenster unter \
                 Tools → {TITEL} an; jedes Werkzeug nimmt `verbindung`, bei genau einem darf der \
                 Name fehlen. Fehlt die Liste, sagen die Werkzeuge das als Text."
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
    use crate::werkzeug::Zephyr;
    use quiver_tools_kern::zugaenge::Zugang as Eintrag;

    fn eintrag(name: &str, projekt: Option<&str>, schreiben: bool) -> Eintrag {
        let mut werte: std::collections::BTreeMap<String, String> = [
            ("basis_url", "https://api.example.test/v2"),
            ("api_token", "token-4711"),
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

    fn server_mit(eintraege: Vec<Eintrag>) -> ZephyrServer {
        ZephyrServer::mit(
            KernZugang::mit_umgebung(&Zephyr, |_| None),
            Ok(Sammlung::aus_zugaengen(eintraege).unwrap()),
        )
    }

    fn ohne_zugaenge() -> ZephyrServer {
        ZephyrServer::mit(
            KernZugang::mit_umgebung(&Zephyr, |_| None),
            Ok(Sammlung::default()),
        )
    }

    fn aufruf(server: &ZephyrServer, name: &str, argumente: Value) -> Ergebnis {
        let laufzeit = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let objekt = argumente.as_object().cloned().unwrap_or_default();
        laufzeit.block_on(server.aufrufen(name, Some(objekt)))
    }

    fn text_von(ergebnis: Ergebnis) -> String {
        let ergebnis = ergebnis.expect("unerwarteter Fehler");
        ergebnis.content[0]
            .as_text()
            .expect("kein Text")
            .text
            .clone()
    }

    #[test]
    fn alle_werkzeuge_tragen_den_richtigen_hinweis() {
        let server = ohne_zugaenge();
        let werkzeuge = server.werkzeuge();
        assert_eq!(werkzeuge.len(), 15, "14 Werkzeuge plus zephyr_profile");

        let schreibend = [
            "zephyr_testfall_anlegen",
            "zephyr_zyklus_testfaelle_hinzufuegen",
            "zephyr_ergebnis_eintragen",
        ];

        for werkzeug in &werkzeuge {
            let hinweise = werkzeug.annotations.as_ref().expect("Hinweise fehlen");
            let name = werkzeug.name.as_ref();
            let destruktiv = schreibend.contains(&name);
            assert_eq!(hinweise.read_only_hint, Some(!destruktiv), "{name}");
            assert_eq!(hinweise.destructive_hint, Some(destruktiv), "{name}");
            if name == "zephyr_profile" {
                assert!(hinweise.title.is_some(), "{name} hat keinen Titel");
            } else {
                assert!(werkzeug.title.is_some(), "{name} hat keinen Titel");
            }
            // Jedes eigene Werkzeug kennt `verbindung`; `zephyr_profile` nicht.
            let eigenschaften = &werkzeug.input_schema["properties"];
            if name != "zephyr_profile" {
                assert!(
                    eigenschaften.get("verbindung").is_some(),
                    "{name} kennt kein `verbindung`: {eigenschaften:?}"
                );
            }
            assert_eq!(werkzeug.input_schema["type"], json!("object"), "{name}");
        }

        // Die Namen stehen in einer festen Reihenfolge: erst lesend, dann
        // verändernd, zuletzt das Profil.
        let namen: Vec<&str> = werkzeuge.iter().map(|w| w.name.as_ref()).collect();
        assert_eq!(namen[0], "zephyr_status");
        assert_eq!(namen[10], "zephyr_ordner");
        assert_eq!(namen[11], "zephyr_testfall_anlegen");
        assert_eq!(namen[13], "zephyr_ergebnis_eintragen");
        assert_eq!(namen[14], "zephyr_profile");
    }

    #[test]
    fn ohne_zugaenge_kommt_der_hinweis_und_kein_fehler() {
        let server = ohne_zugaenge();
        for (name, argumente) in [
            ("zephyr_status", json!({})),
            ("zephyr_testen", json!({})),
            ("zephyr_projekte", json!({})),
            ("zephyr_testfaelle", json!({})),
            ("zephyr_testfall", json!({ "schluessel": "QA-T1" })),
            ("zephyr_ausfuehrungen", json!({"projekt": "QA"})),
            ("zephyr_testfall_anlegen", json!({ "name": "Neu" })),
        ] {
            let text = text_von(aufruf(&server, name, argumente));
            assert!(text.contains("Zugang fehlt"), "{name}: {text}");
            assert!(text.contains("Tools → Zephyr"), "{name}: {text}");
        }
    }

    #[test]
    fn ohne_projekt_kommt_ein_hinweis_ohne_anfrage() {
        let server = server_mit(vec![eintrag("kunde", None, true)]);
        let text = text_von(aufruf(&server, "zephyr_testfaelle", json!({})));
        assert!(text.contains("Projekt fehlt"), "{text}");
        assert!(text.contains("`projekt` angeben"), "{text}");
    }

    #[test]
    fn das_profil_nennt_die_zugaenge_ohne_token() {
        let server = server_mit(vec![eintrag("kunde", Some("QA"), false)]);
        let text = text_von(aufruf(&server, "zephyr_profile", json!({})));
        assert!(text.contains("kunde"), "{text}");
        assert!(text.contains("Schreiben aus"), "{text}");
        assert!(!text.contains("token-4711"), "{text}");
    }

    #[test]
    fn mehrere_zugaenge_verlangen_einen_namen() {
        let server = server_mit(vec![
            eintrag("kunde", Some("QA"), true),
            eintrag("ops", None, false),
        ]);
        let text = text_von(aufruf(&server, "zephyr_status", json!({})));
        assert!(text.contains("kunde"), "{text}");
        assert!(text.contains("ops"), "{text}");
        assert!(!text.contains("token-4711"), "{text}");

        let fehler = aufruf(&server, "zephyr_projekte", json!({})).unwrap_err();
        assert!(
            fehler.message.contains("mehrere Zugänge"),
            "{}",
            fehler.message
        );

        let fehler = aufruf(
            &server,
            "zephyr_projekte",
            json!({ "verbindung": "gibtsnicht" }),
        )
        .unwrap_err();
        assert!(fehler.message.contains("gibtsnicht"), "{}", fehler.message);
    }

    #[test]
    fn schreiben_aus_antwortet_ohne_anfrage() {
        let server = server_mit(vec![eintrag("kunde", Some("QA"), false)]);
        for (name, argumente) in [
            ("zephyr_testfall_anlegen", json!({ "name": "Neu" })),
            (
                "zephyr_zyklus_testfaelle_hinzufuegen",
                json!({ "zyklus": "QA-R1", "testfaelle": ["QA-T1"] }),
            ),
            (
                "zephyr_ergebnis_eintragen",
                json!({ "ausfuehrung": "101", "status": "Pass" }),
            ),
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
        let server = server_mit(vec![eintrag("kunde", Some("QA"), true)]);
        let fehler =
            aufruf(&server, "zephyr_testfall_anlegen", json!({ "name": "  " })).unwrap_err();
        assert!(fehler.message.contains("name"), "{}", fehler.message);

        let fehler = aufruf(&server, "zephyr_gibtsnicht", json!({})).unwrap_err();
        assert!(
            fehler.message.contains("zephyr_gibtsnicht"),
            "{}",
            fehler.message
        );

        let fehler = aufruf(&server, "zephyr_testfall", json!({ "schluessel": 3 })).unwrap_err();
        assert!(fehler.message.contains("Parameter"), "{}", fehler.message);

        let fehler = aufruf(
            &server,
            "zephyr_zyklus_testfaelle_hinzufuegen",
            json!({ "zyklus": "QA-R1", "testfaelle": [] }),
        )
        .unwrap_err();
        assert!(fehler.message.contains("mindestens"), "{}", fehler.message);

        let fehler = aufruf(&server, "zephyr_ordner", json!({ "art": "quatsch" })).unwrap_err();
        assert!(fehler.message.contains("TEST_CASE"), "{}", fehler.message);

        let fehler =
            aufruf(&server, "zephyr_status_liste", json!({ "art": "quatsch" })).unwrap_err();
        assert!(
            fehler.message.contains("TEST_EXECUTION"),
            "{}",
            fehler.message
        );

        // `.` und `..` sind keine Schlüssel: der Fehler kommt ohne Anfrage.
        for falsch in [".", "..", "  "] {
            let fehler =
                aufruf(&server, "zephyr_testfall", json!({ "schluessel": falsch })).unwrap_err();
            assert!(
                fehler.message.contains("nichts gesendet"),
                "{falsch}: {}",
                fehler.message
            );
        }
    }

    #[test]
    fn die_helfer_rechnen_richtig() {
        assert_eq!(zeichen(None), format::MAX_ZEICHEN);
        assert_eq!(zeichen(Some(5000)), 5000);
        assert_eq!(zeichen(Some(1)), 100);
        assert_eq!(anzahl(None), format::STANDARD_ANZAHL);
        assert_eq!(anzahl(Some(500)), format::MAX_ANZAHL);
        assert_eq!(anzahl(Some(0)), 1);
        assert_eq!(ab(None), 0);
        assert_eq!(ab(Some(25)), 25);
        // `startAt` der OpenAPI endet bei 1 000 000.
        assert_eq!(ab(Some(9_999_999)), 1_000_000);

        assert_eq!(ordner_art(None).unwrap(), "TEST_CASE");
        assert_eq!(ordner_art(Some("test_plan")).unwrap(), "TEST_PLAN");
        assert_eq!(ordner_art(Some("TEST_CYCLE")).unwrap(), "TEST_CYCLE");
        assert!(ordner_art(Some("quatsch")).is_err());

        // Ohne `art` sind es die Status der Ausführungen (für Ergebnisse).
        assert_eq!(status_art(None).unwrap(), namen::STATUS_AUSFUEHRUNG);
        assert_eq!(status_art(Some("test_case")).unwrap(), "TEST_CASE");
        assert_eq!(status_art(Some("TEST_PLAN")).unwrap(), "TEST_PLAN");
        assert!(status_art(Some("quatsch")).is_err());

        assert_eq!(
            testfaelle_pruefen(&[" QA-T1 ".to_string()]).unwrap(),
            vec!["QA-T1"]
        );
        assert!(testfaelle_pruefen(&[]).is_err());
        assert!(testfaelle_pruefen(&["  ".to_string()]).is_err());
    }

    #[test]
    fn ein_verweis_wird_nur_erkannt_wenn_er_dasteht() {
        let mit = json!({ "values": [ { "id": 1 }, { "status": { "id": 2 } } ] });
        assert!(verweis_vorhanden(&mit, "status"));
        assert!(!verweis_vorhanden(&mit, "environment"));
        let ohne = json!({ "values": [ { "status": null } ] });
        assert!(!verweis_vorhanden(&ohne, "status"));
    }

    #[test]
    fn eine_kaputte_zugangsliste_ist_ein_parameterfehler() {
        let server = ZephyrServer::mit(
            KernZugang::mit_umgebung(&Zephyr, |_| None),
            Sammlung::mit_umgebung(|_| Some("[{\"name\":".to_string())),
        );
        let fehler = aufruf(&server, "zephyr_status", json!({})).unwrap_err();
        assert!(
            fehler.message.contains("QUIVER_ZUGAENGE"),
            "{}",
            fehler.message
        );
    }
}
