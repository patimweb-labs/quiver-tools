// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der MCP-Server: neunzehn lesende Werkzeuge über stdio (Aufgabe 07).
//!
//! Die Systeme kommen aus `QUIVER_ZUGAENGE` (mehrere je Werkzeug). Fehlt die
//! Liste, antwortet jedes Werkzeug mit dem Hinweis auf das quiver-Fenster –
//! **kein** MCP-Fehler. Jedes Werkzeug nimmt `system`; bei genau einem System
//! darf der Name fehlen.
//!
//! Kein Werkzeug verändert etwas im SAP-System: `read_only_hint` steht bei
//! allen auf `true`, auch bei Syntax-Check, Unit-Tests und ATC – die rechnen
//! nur.

use std::any::Any;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
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
use serde_json::Value;

use crate::adt::client::{Anfrage, Client, Methode};
use crate::adt::daten::{self, Objekt};
use crate::adt::AdtFehler;
use crate::format;
use crate::systeme::{AuswahlFehler, SammelFehler, Sammlung, System};
use crate::werkzeug::{MEHRFACH_EINZAHL, MEHRFACH_TITEL, TITEL};

/// Die Endpunkte, abgelesen bei `abap-adt-api` (`build/api/*.js`).
const SUCHE: &str = "/sap/bc/adt/repository/informationsystem/search";
const VERWENDUNGEN: &str = "/sap/bc/adt/repository/informationsystem/usageReferences";
const OBJEKTARTEN: &str = "/sap/bc/adt/repository/informationsystem/objecttypes";
const KNOTEN: &str = "/sap/bc/adt/repository/nodestructure";
const DATENVORSCHAU_DDIC: &str = "/sap/bc/adt/datapreview/ddic";
const DATENVORSCHAU_SQL: &str = "/sap/bc/adt/datapreview/freestyle";
const SYNTAX: &str = "/sap/bc/adt/checkruns";
const UNIT: &str = "/sap/bc/adt/abapunit/testruns";
const ATC_LAUF: &str = "/sap/bc/adt/atc/runs";
const ATC_LISTE: &str = "/sap/bc/adt/atc/worklists";
const DUMPS: &str = "/sap/bc/adt/runtime/dumps";
const TRACES: &str = "/sap/bc/adt/runtime/traces/abaptraces";
const TRANSPORTE: &str = "/sap/bc/adt/cts/transportrequests";
const DISCOVERY: &str = "/sap/bc/adt/core/discovery";

/// So viele Zeichen kommen höchstens aus `abap_read`.
const MAX_ZEICHEN_STANDARD: u32 = 20_000;
/// So viele Zeilen kommen höchstens aus `abap_table`/`abap_query`.
const MAX_ZEILEN_STANDARD: u32 = 100;
/// Mehr Zeilen gibt es nicht.
const MAX_ZEILEN_OBEN: u32 = 1_000;
/// So viele Treffer holt die Suche beim Auflösen eines Namens.
const AUFLOESEN_TREFFER: u32 = 20;

/// Was ein Werkzeug zurückgibt.
type Ergebnis = Result<CallToolResult, McpError>;

/// Der ABAP-Server: die Systeme und die Werkzeugliste.
pub struct AbapServer {
    /// Der Zugang des Kerns; bei `mehrfach` ohne Felder, aber mit Titel.
    zugang: Zugang,
    systeme: Result<Sammlung, SammelFehler>,
}

impl AbapServer {
    /// Der Normalfall: alles aus der Umgebung (`QUIVER_ZUGAENGE`).
    pub fn neu(zugang: Zugang) -> Self {
        AbapServer {
            zugang,
            systeme: Sammlung::aus_umgebung(),
        }
    }

    /// Feste Teile (Tests und Sonderfälle), ohne Prozessumgebung.
    pub fn mit(zugang: Zugang, systeme: Result<Sammlung, SammelFehler>) -> Self {
        AbapServer { zugang, systeme }
    }

    /// Alle Werkzeuge, wie `tools/list` sie meldet.
    pub fn werkzeuge(&self) -> Vec<Tool> {
        vec![
            lesend::<StatusParam>(
                "abap_status",
                "Systeme",
                "Listet die im quiver-Fenster eingetragenen SAP-Systeme: Name, Benutzer@Host, \
                 Mandant, Sprache und ob das Serverzertifikat geprüft wird. Öffnet keine \
                 Verbindung, zeigt kein Passwort.",
            ),
            lesend::<VerbindungParam>(
                "abap_test_connection",
                "Verbindung prüfen",
                "Meldet sich am SAP-System an und gibt zurück, wie viele ADT-Bereiche es meldet. \
                 Ohne `system` prüft es alle eingetragenen Systeme. Ein abgelehntes Passwort wird \
                 nicht wiederholt (SAP sperrt den Benutzer).",
            ),
            lesend::<SuchParam>(
                "abap_search",
                "Objekte suchen",
                "Sucht ABAP-Objekte nach Namen (Platzhalter wie `ZCL_*` gehen auf den meisten \
                 Systemen). Liefert Name, Typ, Paket, Beschreibung und die Objekt-URL für \
                 abap_read und abap_object_structure.",
            ),
            lesend::<ObjektParam>(
                "abap_object_structure",
                "Objektstruktur",
                "Zeigt Kennwerte (Typ, Beschreibung, Verantwortlicher, Sprache) und die Includes \
                 eines Objekts samt Adresse des Hauptquelltexts. Entweder `name` (mit optionalem \
                 `type`) oder `objectUrl` angeben.",
            ),
            lesend::<LesenParam>(
                "abap_read",
                "Quelltext lesen",
                "Liest den Hauptquelltext eines Objekts (Klasse, Programm, Funktionsgruppe, \
                 Interface …). Mit `max_zeichen` gekürzt (Standard 20000).",
            ),
            lesend::<ObjektParam>(
                "abap_where_used",
                "Wo verwendet",
                "Zeigt, welche Objekte ein Objekt verwenden (Wo-eingesetzt-Liste). Entweder `name` \
                 oder `objectUrl` angeben.",
            ),
            lesend::<NurSystemParam>(
                "abap_object_types",
                "Objektarten",
                "Listet die Objektarten, nach denen die Suche filtern kann (z. B. CLAS, PROG, FUGR, \
                 INTF) – als Hinweis für `objectType` von abap_search.",
            ),
            lesend::<KnotenParam>(
                "abap_node_contents",
                "Paketinhalt",
                "Zeigt den Inhalt eines Pakets oder einer Funktionsgruppe. `parentType` ist meist \
                 `DEVC/K` (Paket), `parentName` der Paketname; `$TMP` sind die lokalen Objekte.",
            ),
            lesend::<TabellenParam>(
                "abap_table",
                "Tabelle ansehen",
                "Zeigt die ersten Zeilen einer DDIC-Tabelle oder Sicht. `filter` ist eine \
                 SQL-Bedingung ohne WHERE, z. B. `CARRID = 'LH'`. Höchstens 1000 Zeilen.",
            ),
            lesend::<AbfrageParam>(
                "abap_query",
                "SQL-Abfrage",
                "Führt eine freie ABAP-SQL-Abfrage aus, z. B. `SELECT carrid, connid FROM sflight`. \
                 Nur SELECT. Höchstens 1000 Zeilen. Auf Produktionssystemen sparsam verwenden.",
            ),
            lesend::<SyntaxParam>(
                "abap_syntax_check",
                "Syntax-Check",
                "Prüft einen Quelltext auf Syntaxfehler. Ohne `source` wird der gespeicherte \
                 Hauptquelltext geprüft; mit `source` der übergebene – gespeichert wird nichts.",
            ),
            lesend::<TestParam>(
                "abap_unit_test",
                "Unit-Tests",
                "Führt die ABAP-Unit-Tests einer Klasse oder eines Programms aus. Standardmäßig \
                 laufen nur kurze, harmlose Tests; gefährliche, kritische und längere Tests \
                 schaltet man einzeln dazu.",
            ),
            lesend::<AtcParam>(
                "abap_atc",
                "ATC-Prüfung",
                "Startet eine ATC-Prüfung (ABAP Test Cockpit) für ein Objekt und gibt die Befunde \
                 mit Priorität, Prüfung und Zeile zurück. `variant` ist die Prüfvariante (Standard \
                 `DEFAULT`).",
            ),
            lesend::<DumpParam>(
                "abap_dumps",
                "Kurzspeicherauszüge",
                "Listet die Kurzspeicherauszüge (Dumps) des Systems, optional gefiltert über \
                 `query` (z. B. ein Programmname).",
            ),
            lesend::<BenutzerParam>(
                "abap_traces",
                "Aufzeichnungen",
                "Listet die Performance-Aufzeichnungen eines Benutzers (Standard: der angemeldete \
                 Benutzer) mit Kennung, Zustand und Laufzeit.",
            ),
            lesend::<TraceParam>(
                "abap_trace_hitlist",
                "Aufzeichnung auswerten",
                "Wertet eine Aufzeichnung aus: Hot Spots (Trefferliste) und der aggregierte \
                 Aufrufbaum der Aussagen. `traceId` stammt aus abap_traces.",
            ),
            lesend::<BenutzerParam>(
                "abap_transports",
                "Transporte",
                "Listet die Transportaufträge eines Benutzers (Standard: der angemeldete Benutzer) \
                 nach Ziel und Auftragsnummer.",
            ),
            lesend::<TransportParam>(
                "abap_transport_details",
                "Transport ansehen",
                "Zeigt einen Auftrag mit Status, Inhaber, Aufgaben und den enthaltenen Objekten. \
                 `transportNumber` stammt aus abap_transports.",
            ),
            lesend::<TextelementParam>(
                "abap_text_elements",
                "Textelemente",
                "Liest die Textelemente eines Objekts: `symbols` (Textsymbole), `selections` \
                 (Selectionstexte) oder `headings` (Überschriften). Standard `symbols`.",
            ),
        ]
    }

    /// Ein Werkzeug aufrufen. Auch der Weg, den die Tests nehmen.
    pub async fn aufrufen(&self, name: &str, argumente: Option<JsonObject>) -> Ergebnis {
        match name {
            "abap_status" => self.status(argumente),
            "abap_test_connection" => self.test_verbindung(argumente).await,
            "abap_search" => self.suche(argumente).await,
            "abap_object_structure" => self.struktur(argumente).await,
            "abap_read" => self.lesen(argumente).await,
            "abap_where_used" => self.verwendungen(argumente).await,
            "abap_object_types" => self.objektarten(argumente).await,
            "abap_node_contents" => self.knoten(argumente).await,
            "abap_table" => self.tabelle(argumente).await,
            "abap_query" => self.sql(argumente).await,
            "abap_syntax_check" => self.syntaxcheck(argumente).await,
            "abap_unit_test" => self.unittests(argumente).await,
            "abap_atc" => self.atc(argumente).await,
            "abap_dumps" => self.dumps(argumente).await,
            "abap_traces" => self.traces(argumente).await,
            "abap_trace_hitlist" => self.trace_auswerten(argumente).await,
            "abap_transports" => self.transporte(argumente).await,
            "abap_transport_details" => self.transportdetails(argumente).await,
            "abap_text_elements" => self.textelemente(argumente).await,
            sonst => Err(McpError::invalid_params(
                format!("Unbekanntes Werkzeug „{sonst}“."),
                None,
            )),
        }
    }

    /// Die Liste der Systeme oder ein Fehler ohne ihren Inhalt.
    fn sammlung(&self) -> Result<&Sammlung, McpError> {
        self.systeme.as_ref().map_err(|fehler| {
            McpError::invalid_params(
                format!("{fehler} – bitte im quiver-Fenster unter Tools → {TITEL} prüfen."),
                None,
            )
        })
    }

    /// Die gewünschte Verbindung; fehlt die Liste, kommt der Hinweis.
    fn waehlen(&self, name: Option<&str>) -> Result<System, OhneProfil> {
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

    /// Ein Client für dieses System, oder ein MCP-Fehler ohne Geheimnis.
    fn client(system: &System) -> Result<Client, McpError> {
        Client::neu(system).map_err(|fehler| fehler_zu_mcp(system, fehler))
    }

    // ------------------------------------------------------------- lesend

    fn status(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: StatusParam = argumente(roh)?;
        let sammlung = self.sammlung()?;
        if sammlung.ist_leer() {
            return Ok(antwort(hinweis_zugang(TITEL)));
        }
        let gewaehlt: Vec<&System> = match p.system.as_deref() {
            Some(name) => vec![sammlung
                .waehlen(Some(name))
                .map_err(|fehler| McpError::invalid_params(fehler.to_string(), None))?],
            None => sammlung.alle().iter().collect(),
        };
        let kopien: Vec<System> = gewaehlt.into_iter().cloned().collect();
        Ok(antwort(format::status(&kopien)))
    }

    async fn test_verbindung(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: VerbindungParam = argumente(roh)?;
        let sammlung = self.sammlung()?;
        if sammlung.ist_leer() {
            return Ok(antwort(hinweis_zugang(TITEL)));
        }
        // Ohne Namen: alle Systeme prüfen. Mit Namen: genau das eine.
        let systeme: Vec<System> = match p.system.as_deref() {
            Some(name) => vec![sammlung
                .waehlen(Some(name))
                .map_err(|fehler| McpError::invalid_params(fehler.to_string(), None))?
                .clone()],
            None => sammlung.alle().to_vec(),
        };

        let mut zeilen = Vec::new();
        for system in &systeme {
            let client = Self::client(system)?;
            let antwort = client
                .anfrage(&Anfrage::holen(DISCOVERY))
                .await
                .map_err(|fehler| fehler_zu_mcp(system, fehler))?;
            let bereiche = daten::discovery_titel(&antwort.text).unwrap_or_default();
            zeilen.push(format::verbindung(system, &bereiche));
        }
        Ok(antwort(zeilen.join("\n\n")))
    }

    async fn suche(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: SuchParam = argumente(roh)?;
        let system = match self.waehlen(p.system.as_deref()) {
            Ok(system) => system,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let abfrage = pflicht(&p.query, "query")?;
        let max = p.max.unwrap_or(100).clamp(1, 500);

        let client = Self::client(&system)?;
        let mut anfrage = Anfrage::holen(SUCHE)
            .mit_annahme("application/*")
            .mit_abfrage("operation", "quickSearch")
            .mit_abfrage("query", abfrage)
            .mit_abfrage("maxResults", max.to_string());
        if let Some(art) = p.object_type.as_deref().map(str::trim) {
            if !art.is_empty() {
                // SAP nutzt nur den ersten Teil, z. B. `CLAS` statt `CLAS/OC`.
                let art = art.split('/').next().unwrap_or(art);
                anfrage = anfrage.mit_abfrage("objectType", art);
            }
        }

        let xml = client
            .holen(&anfrage)
            .await
            .map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        let treffer = daten::treffer(&xml).map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        Ok(antwort(format::suche(abfrage, &treffer)))
    }

    async fn struktur(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: ObjektParam = argumente(roh)?;
        let system = match self.waehlen(p.system.as_deref()) {
            Ok(system) => system,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let object = self.aufloesen(&system, &p).await?;
        Ok(antwort(format::objektstruktur(&object)))
    }

    async fn lesen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: LesenParam = argumente(roh)?;
        let system = match self.waehlen(p.system.as_deref()) {
            Ok(system) => system,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let objekt_param = ObjektParam {
            system: p.system.clone(),
            name: p.name.clone(),
            object_url: p.object_url.clone(),
            typ: p.typ.clone(),
        };
        let object = self.aufloesen(&system, &objekt_param).await?;
        if object.typ.starts_with("DEVC") {
            return Ok(antwort(format!(
                "{} ist ein Paket und hat keinen Quelltext – bitte abap_node_contents mit \
                 parentType DEVC/K und parentName {} verwenden.",
                object.name, object.name
            )));
        }

        let client = Self::client(&system)?;
        let quelle = client
            .holen(
                &Anfrage::holen(&object.quelltext_url)
                    .mit_annahme("text/plain")
                    .mit_abfrage("version", "active"),
            )
            .await
            .map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        let max_zeichen = p.max_zeichen.unwrap_or(MAX_ZEICHEN_STANDARD).max(1) as usize;
        Ok(antwort(format::quelltext(
            &object.name,
            &object.quelltext_url,
            &quelle,
            max_zeichen,
        )))
    }

    async fn verwendungen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: ObjektParam = argumente(roh)?;
        let system = match self.waehlen(p.system.as_deref()) {
            Ok(system) => system,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let object = self.aufloesen(&system, &p).await?;

        let rumpf = "<?xml version=\"1.0\" encoding=\"ASCII\"?>
  <usagereferences:usageReferenceRequest xmlns:usagereferences=\"http://www.sap.com/adt/ris/usageReferences\">
    <usagereferences:affectedObjects/>
  </usagereferences:usageReferenceRequest>";
        let client = Self::client(&system)?;
        let xml = client
            .holen(
                &Anfrage::senden(Methode::Post, VERWENDUNGEN, rumpf)
                    .mit_annahme("application/*")
                    .mit_kopf("Content-Type", "application/*")
                    .mit_abfrage("uri", object.quelltext_url.clone()),
            )
            .await
            .map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        let verwendungen =
            daten::verwendungen(&xml).map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        Ok(antwort(format::verwendungen(&verwendungen)))
    }

    async fn objektarten(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: NurSystemParam = argumente(roh)?;
        let system = match self.waehlen(p.system.as_deref()) {
            Ok(system) => system,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let client = Self::client(&system)?;
        let xml = client
            .holen(
                &Anfrage::holen(OBJEKTARTEN)
                    .mit_annahme("application/*")
                    .mit_abfrage("maxItemCount", "999")
                    .mit_abfrage("name", "*")
                    .mit_abfrage("data", "usedByProvider"),
            )
            .await
            .map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        let arten = daten::objektarten(&xml).map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        Ok(antwort(format::objektarten(&arten)))
    }

    async fn knoten(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: KnotenParam = argumente(roh)?;
        let system = match self.waehlen(p.system.as_deref()) {
            Ok(system) => system,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let parent_art = pflicht(&p.parent_type, "parentType")?;

        let mut anfrage = Anfrage::senden(Methode::Post, KNOTEN, String::new())
            .mit_annahme("application/*")
            .mit_abfrage("parent_type", parent_art)
            .mit_abfrage("withShortDescriptions", "true");
        if let Some(name) = p.parent_name.as_deref().map(str::trim) {
            if !name.is_empty() {
                anfrage = anfrage.mit_abfrage("parent_name", name);
            }
        }

        let client = Self::client(&system)?;
        let xml = client
            .holen(&anfrage)
            .await
            .map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        let knoten = daten::baumknoten(&xml).map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        Ok(antwort(format::baumknoten(&knoten)))
    }

    async fn tabelle(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: TabellenParam = argumente(roh)?;
        let system = match self.waehlen(p.system.as_deref()) {
            Ok(system) => system,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let tabelle = pflicht(&p.table, "table")?.to_uppercase();
        let zeilen = zeilen(&p.max_zeilen);
        let filter = p
            .filter
            .as_deref()
            .map(str::trim)
            .filter(|filter| !filter.is_empty())
            .map(|filter| format!("WHERE {filter}"))
            .unwrap_or_default();

        let client = Self::client(&system)?;
        let xml = client
            .holen(
                &Anfrage::senden(Methode::Post, DATENVORSCHAU_DDIC, filter)
                    .mit_annahme("application/*")
                    .mit_kopf("Content-Type", "text/plain")
                    .mit_abfrage("rowNumber", zeilen.to_string())
                    .mit_abfrage("ddicEntityName", tabelle),
            )
            .await
            .map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        let abfrage = daten::abfrage(&xml).map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        Ok(antwort(format::abfrage(&abfrage)))
    }

    async fn sql(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: AbfrageParam = argumente(roh)?;
        let system = match self.waehlen(p.system.as_deref()) {
            Ok(system) => system,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let sql = pflicht(&p.sql, "sql")?;
        let zeilen = zeilen(&p.max_zeilen);

        let client = Self::client(&system)?;
        let xml = client
            .holen(
                &Anfrage::senden(Methode::Post, DATENVORSCHAU_SQL, sql)
                    .mit_annahme("application/*")
                    .mit_kopf("Content-Type", "text/plain")
                    .mit_abfrage("rowNumber", zeilen.to_string()),
            )
            .await
            .map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        let abfrage = daten::abfrage(&xml).map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        Ok(antwort(format::abfrage(&abfrage)))
    }

    async fn syntaxcheck(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: SyntaxParam = argumente(roh)?;
        let system = match self.waehlen(p.system.as_deref()) {
            Ok(system) => system,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let objekt_param = ObjektParam {
            system: p.system.clone(),
            name: p.name.clone(),
            object_url: p.object_url.clone(),
            typ: p.typ.clone(),
        };
        let object = self.aufloesen(&system, &objekt_param).await?;
        let client = Self::client(&system)?;

        // Ohne `source` wird der gespeicherte Quelltext geprüft.
        let quelle = match p.source.clone() {
            Some(quelle) => quelle,
            None => client
                .holen(
                    &Anfrage::holen(&object.quelltext_url)
                        .mit_annahme("text/plain")
                        .mit_abfrage("version", "active"),
                )
                .await
                .map_err(|fehler| fehler_zu_mcp(&system, fehler))?,
        };

        let rumpf = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>
  <chkrun:checkObjectList xmlns:chkrun=\"http://www.sap.com/adt/checkrun\" xmlns:adtcore=\"http://www.sap.com/adt/core\">
  <chkrun:checkObject adtcore:uri=\"{quelle_url}\" chkrun:version=\"active\">
    <chkrun:artifacts>
      <chkrun:artifact chkrun:contentType=\"text/plain; charset=utf-8\" chkrun:uri=\"{quelle_url}\">
        <chkrun:content>{inhalt}</chkrun:content>
      </chkrun:artifact>
    </chkrun:artifacts>
  </chkrun:checkObject>
</chkrun:checkObjectList>",
            quelle_url = xml_attribut(&object.quelltext_url),
            inhalt = BASE64.encode(quelle.as_bytes()),
        );

        let xml = client
            .holen(
                &Anfrage::senden(Methode::Post, SYNTAX, rumpf)
                    .mit_annahme("application/*")
                    .mit_kopf("Content-Type", "application/*")
                    .mit_abfrage("reporters", "abapCheckRun"),
            )
            .await
            .map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        let meldungen =
            daten::syntaxmeldungen(&xml).map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        Ok(antwort(format::syntax(&meldungen)))
    }

    async fn unittests(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: TestParam = argumente(roh)?;
        let system = match self.waehlen(p.system.as_deref()) {
            Ok(system) => system,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let objekt_param = ObjektParam {
            system: p.system.clone(),
            name: p.name.clone(),
            object_url: p.object_url.clone(),
            typ: p.typ.clone(),
        };
        let object = self.aufloesen(&system, &objekt_param).await?;

        let ja = |wert: Option<bool>| {
            if wert.unwrap_or(false) {
                "true"
            } else {
                "false"
            }
        };
        let rumpf = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>
  <aunit:runConfiguration xmlns:aunit=\"http://www.sap.com/adt/aunit\">
  <external>
    <coverage active=\"false\"/>
  </external>
  <options>
    <uriType value=\"semantic\"/>
    <testDeterminationStrategy sameProgram=\"true\" assignedTests=\"false\"/>
    <testRiskLevels harmless=\"true\" dangerous=\"{dangerous}\" critical=\"{critical}\"/>
    <testDurations short=\"true\" medium=\"{medium}\" long=\"{long}\"/>
    <withNavigationUri enabled=\"true\"/>
  </options>
  <adtcore:objectSets xmlns:adtcore=\"http://www.sap.com/adt/core\">
    <objectSet kind=\"inclusive\">
      <adtcore:objectReferences>
        <adtcore:objectReference adtcore:uri=\"{uri}\"/>
      </adtcore:objectReferences>
    </objectSet>
  </adtcore:objectSets>
</aunit:runConfiguration>",
            dangerous = ja(p.dangerous),
            critical = ja(p.critical),
            medium = ja(p.medium),
            long = ja(p.long),
            uri = xml_attribut(&object.objekt_url),
        );

        let client = Self::client(&system)?;
        let xml = client
            .holen(
                &Anfrage::senden(Methode::Post, UNIT, rumpf)
                    .mit_annahme("application/*")
                    .mit_kopf("Content-Type", "application/*"),
            )
            .await
            .map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        let klassen = daten::testklassen(&xml).map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        Ok(antwort(format::unit_tests(&klassen)))
    }

    async fn atc(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: AtcParam = argumente(roh)?;
        let system = match self.waehlen(p.system.as_deref()) {
            Ok(system) => system,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let objekt_param = ObjektParam {
            system: p.system.clone(),
            name: p.name.clone(),
            object_url: p.object_url.clone(),
            typ: p.typ.clone(),
        };
        let object = self.aufloesen(&system, &objekt_param).await?;
        let variante = p
            .variant
            .as_deref()
            .map(str::trim)
            .filter(|variante| !variante.is_empty())
            .unwrap_or("DEFAULT");
        let max_befunde = p.max_results.unwrap_or(100).clamp(1, 1_000);

        let rumpf = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<atc:run maximumVerdicts=\"{max_befunde}\" xmlns:atc=\"http://www.sap.com/adt/atc\">
  <objectSets xmlns:adtcore=\"http://www.sap.com/adt/core\">
    <objectSet kind=\"inclusive\">
      <adtcore:objectReferences>
        <adtcore:objectReference adtcore:uri=\"{uri}\"/>
      </adtcore:objectReferences>
    </objectSet>
  </objectSets>
</atc:run>",
            uri = xml_attribut(&object.objekt_url),
        );

        let client = Self::client(&system)?;
        let xml = client
            .holen(
                &Anfrage::senden(Methode::Post, ATC_LAUF, rumpf)
                    .mit_annahme("application/xml")
                    .mit_kopf("Content-Type", "application/xml")
                    .mit_abfrage("worklistId", variante),
            )
            .await
            .map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        let lauf = daten::atc_lauf(&xml).map_err(|fehler| fehler_zu_mcp(&system, fehler))?;

        let liste = client
            .holen(
                &Anfrage::holen(format!("{ATC_LISTE}/{lauf}"))
                    .mit_annahme("application/atc.worklist.v1+xml")
                    .mit_abfrage("includeExemptedFindings", "false"),
            )
            .await
            .map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        let befunde =
            daten::atc_befunde(&liste).map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        Ok(antwort(format::atc(&befunde)))
    }

    async fn dumps(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: DumpParam = argumente(roh)?;
        let system = match self.waehlen(p.system.as_deref()) {
            Ok(system) => system,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let mut anfrage = Anfrage::holen(DUMPS).mit_annahme("application/atom+xml;type=feed");
        if let Some(abfrage) = p.query.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
            anfrage = anfrage.mit_abfrage("$query", abfrage);
        }

        let client = Self::client(&system)?;
        let xml = client
            .holen(&anfrage)
            .await
            .map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        let dumps = daten::dumps(&xml).map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        Ok(antwort(format::dumps(&dumps)))
    }

    async fn traces(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: BenutzerParam = argumente(roh)?;
        let system = match self.waehlen(p.system.as_deref()) {
            Ok(system) => system,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let benutzer = benutzer(&p.user, &system);

        let client = Self::client(&system)?;
        let xml = client
            .holen(&Anfrage::holen(TRACES).mit_abfrage("user", benutzer))
            .await
            .map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        let aufzeichnungen =
            daten::aufzeichnungen(&xml).map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        Ok(antwort(format::aufzeichnungen(&aufzeichnungen)))
    }

    async fn trace_auswerten(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: TraceParam = argumente(roh)?;
        let system = match self.waehlen(p.system.as_deref()) {
            Ok(system) => system,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let kennung = pflicht(&p.trace_id, "traceId")?;
        let pfad = if kennung.starts_with(TRACES) {
            kennung.to_string()
        } else {
            format!("{TRACES}/{kennung}")
        };
        let systemereignisse = if p.with_system_events.unwrap_or(false) {
            "true"
        } else {
            "false"
        };

        let client = Self::client(&system)?;
        let hitlist = client
            .holen(
                &Anfrage::holen(format!("{pfad}/hitlist"))
                    .mit_abfrage("withSystemEvents", systemereignisse),
            )
            .await
            .map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        let aussagen = client
            .holen(
                &Anfrage::holen(format!("{pfad}/statements"))
                    .mit_annahme(
                        "application/vnd.sap.adt.runtime.traces.abaptraces.aggcalltree+xml, \
                         application/xml",
                    )
                    .mit_abfrage("withSystemEvents", systemereignisse),
            )
            .await
            .map_err(|fehler| fehler_zu_mcp(&system, fehler))?;

        let hotspots =
            daten::hotspots(&hitlist).map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        let aussagen =
            daten::aussagen(&aussagen).map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        Ok(antwort(format::trace_auswertung(&hotspots, &aussagen)))
    }

    async fn transporte(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: BenutzerParam = argumente(roh)?;
        let system = match self.waehlen(p.system.as_deref()) {
            Ok(system) => system,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let benutzer = benutzer(&p.user, &system);

        let client = Self::client(&system)?;
        let xml = client
            .holen(
                &Anfrage::holen(TRANSPORTE)
                    .mit_abfrage("user", benutzer)
                    .mit_abfrage("targets", "true"),
            )
            .await
            .map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        let transporte =
            daten::transporte(&xml).map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        Ok(antwort(format::transporte(&transporte)))
    }

    async fn transportdetails(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: TransportParam = argumente(roh)?;
        let system = match self.waehlen(p.system.as_deref()) {
            Ok(system) => system,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let nummer = pflicht(&p.transport_number, "transportNumber")?.to_uppercase();

        let client = Self::client(&system)?;
        let xml = client
            .holen(
                &Anfrage::holen(format!("{TRANSPORTE}/{nummer}"))
                    .mit_annahme("application/vnd.sap.adt.transportorganizer.v1+xml"),
            )
            .await
            .map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        let detail =
            daten::transportdetails(&xml).map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        Ok(antwort(format::transportdetails(&detail)))
    }

    async fn textelemente(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: TextelementParam = argumente(roh)?;
        let system = match self.waehlen(p.system.as_deref()) {
            Ok(system) => system,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let objekt_param = ObjektParam {
            system: p.system.clone(),
            name: p.name.clone(),
            object_url: p.object_url.clone(),
            typ: p.typ.clone(),
        };
        let object = self.aufloesen(&system, &objekt_param).await?;
        let kategorie = p
            .category
            .as_deref()
            .map(str::trim)
            .filter(|kategorie| !kategorie.is_empty())
            .unwrap_or("symbols")
            .to_lowercase();
        if !matches!(kategorie.as_str(), "symbols" | "selections" | "headings") {
            return Err(McpError::invalid_params(
                format!(
                    "`category` muss symbols, selections oder headings sein, nicht „{kategorie}“."
                ),
                None,
            ));
        }

        let url = daten::textelemente_url(&object.typ, &object.name);
        let client = Self::client(&system)?;
        let abruf = client
            .anfrage(
                &Anfrage::holen(format!("{url}/source/{kategorie}")).mit_annahme(format!(
                    "application/vnd.sap.adt.textelements.{kategorie}.v1"
                )),
            )
            .await;
        // Ein Objekt ohne Textelemente antwortet mit 404 – das ist kein Fehler.
        let inhalt = match abruf {
            Ok(ergebnis) => ergebnis.text,
            Err(AdtFehler::Http { status: 404, .. }) => String::new(),
            Err(fehler) => return Err(fehler_zu_mcp(&system, fehler)),
        };
        let elemente = daten::textelemente(&inhalt);
        Ok(antwort(format::textelemente(
            &elemente,
            &kategorie,
            &object.name,
        )))
    }

    // ------------------------------------------------------------- intern

    /// Ein Objekt auflösen: über die Adresse oder über Name und Typ.
    async fn aufloesen(&self, system: &System, p: &ObjektParam) -> Result<Objekt, McpError> {
        let client = Self::client(system)?;

        let objekt_url = match p
            .object_url
            .as_deref()
            .map(str::trim)
            .filter(|url| !url.is_empty())
        {
            Some(url) => url.to_string(),
            None => {
                let name = match p.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
                    Some(name) => name,
                    None => {
                        return Err(McpError::invalid_params(
                            "Bitte `name` oder `objectUrl` angeben.".to_string(),
                            None,
                        ))
                    }
                };
                let mut anfrage = Anfrage::holen(SUCHE)
                    .mit_annahme("application/*")
                    .mit_abfrage("operation", "quickSearch")
                    .mit_abfrage("query", name)
                    .mit_abfrage("maxResults", AUFLOESEN_TREFFER.to_string());
                if let Some(art) = p.typ.as_deref().map(str::trim).filter(|a| !a.is_empty()) {
                    let art = art.split('/').next().unwrap_or(art);
                    anfrage = anfrage.mit_abfrage("objectType", art);
                }
                let xml = client
                    .holen(&anfrage)
                    .await
                    .map_err(|fehler| fehler_zu_mcp(system, fehler))?;
                let treffer =
                    daten::treffer(&xml).map_err(|fehler| fehler_zu_mcp(system, fehler))?;
                let gesucht = name.to_lowercase();
                treffer
                    .iter()
                    .find(|treffer| treffer.name.to_lowercase() == gesucht)
                    .or_else(|| treffer.first())
                    .map(|treffer| treffer.uri.clone())
                    .filter(|uri| !uri.is_empty())
                    .ok_or_else(|| {
                        McpError::invalid_params(
                            format!("Kein Objekt gefunden, das zu „{name}“ passt."),
                            None,
                        )
                    })?
            }
        };

        let xml = client
            .holen(&Anfrage::holen(&objekt_url).mit_annahme("application/*"))
            .await
            .map_err(|fehler| fehler_zu_mcp(system, fehler))?;
        daten::objekt(&xml, &objekt_url).map_err(|fehler| fehler_zu_mcp(system, fehler))
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

/// Die Zeilenzahl einer Abfrage: Standard 100, höchstens 1000.
fn zeilen(wert: &Option<u32>) -> u32 {
    wert.unwrap_or(MAX_ZEILEN_STANDARD)
        .clamp(1, MAX_ZEILEN_OBEN)
}

/// Der Benutzer für Traces und Transporte: der genannte, sonst der angemeldete.
fn benutzer(genannt: &Option<String>, system: &System) -> String {
    genannt
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(|name| name.to_uppercase())
        .unwrap_or_else(|| system.benutzer.to_uppercase())
}

/// Ein Text für ein XML-Attribut: nur die fünf Zeichen, die XML nicht erlaubt.
fn xml_attribut(wert: &str) -> String {
    wert.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Aus einem ADT-Fehler wird ein MCP-Fehler – mit Systemnamen, nie mit Geheimnis.
fn fehler_zu_mcp(system: &System, fehler: AdtFehler) -> McpError {
    let text = format!("{}: {fehler}", system.name);
    match fehler {
        AdtFehler::Anmeldung { .. } | AdtFehler::Csrf { .. } => {
            McpError::invalid_params(text, None)
        }
        _ => McpError::internal_error(text, None),
    }
}

// ------------------------------------------------------------- Parameter

/// Parameter von `abap_status`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct StatusParam {
    /// Nur dieses System zeigen. Ohne Angabe: alle.
    #[serde(default)]
    pub system: Option<String>,
}

/// Parameter von `abap_test_connection`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct VerbindungParam {
    /// Nur dieses System prüfen. Ohne Angabe: alle.
    #[serde(default)]
    pub system: Option<String>,
}

/// Parameter von `abap_search`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct SuchParam {
    /// System; bei genau einem darf der Name fehlen.
    #[serde(default)]
    pub system: Option<String>,
    /// Suchmuster, z. B. `ZCL_*` oder `ZCL_MY_CLASS`.
    pub query: String,
    /// Objektart als Filter; der erste Teil genügt, z. B. `CLAS`, `PROG`, `FUGR`.
    #[serde(default, rename = "objectType")]
    pub object_type: Option<String>,
    /// Wie viele Treffer höchstens zurückkommen. Standard 100.
    #[serde(default)]
    pub max: Option<u32>,
}

/// Parameter der Werkzeuge, die ein Objekt brauchen.
#[derive(Debug, Default, Clone, Deserialize, JsonSchema)]
pub struct ObjektParam {
    /// System; bei genau einem darf der Name fehlen.
    #[serde(default)]
    pub system: Option<String>,
    /// Objektname, z. B. `ZCL_MY_CLASS`.
    #[serde(default)]
    pub name: Option<String>,
    /// Objekt-URL, z. B. `/sap/bc/adt/oo/classes/zcl_my_class`. Geht vor `name`.
    #[serde(default, rename = "objectUrl")]
    pub object_url: Option<String>,
    /// Art als Hinweis für die Suche, z. B. `CLAS/OC`.
    #[serde(default)]
    pub typ: Option<String>,
}

/// Parameter von `abap_read`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct LesenParam {
    /// System; bei genau einem darf der Name fehlen.
    #[serde(default)]
    pub system: Option<String>,
    /// Objektname, z. B. `ZCL_MY_CLASS`.
    #[serde(default)]
    pub name: Option<String>,
    /// Objekt-URL. Geht vor `name`.
    #[serde(default, rename = "objectUrl")]
    pub object_url: Option<String>,
    /// Art als Hinweis für die Suche, z. B. `CLAS/OC`.
    #[serde(default)]
    pub typ: Option<String>,
    /// Quelltext nach so vielen Zeichen kürzen. Standard 20000.
    #[serde(default)]
    pub max_zeichen: Option<u32>,
}

/// Parameter von `abap_object_types`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct NurSystemParam {
    /// System; bei genau einem darf der Name fehlen.
    #[serde(default)]
    pub system: Option<String>,
}

/// Parameter von `abap_node_contents`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct KnotenParam {
    /// System; bei genau einem darf der Name fehlen.
    #[serde(default)]
    pub system: Option<String>,
    /// Art des Elternknotens: `DEVC/K` (Paket), `PROG/P`, `FUGR/F`, `PROG/PI`.
    #[serde(rename = "parentType")]
    pub parent_type: String,
    /// Name des Elternknotens, z. B. `Z_MY_PACKAGE` oder `$TMP`.
    #[serde(default, rename = "parentName")]
    pub parent_name: Option<String>,
}

/// Parameter von `abap_table`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct TabellenParam {
    /// System; bei genau einem darf der Name fehlen.
    #[serde(default)]
    pub system: Option<String>,
    /// Tabellen- oder Sichtname, z. B. `SFLIGHT`.
    pub table: String,
    /// Höchstens so viele Zeilen. Standard 100, höchstens 1000.
    #[serde(default)]
    pub max_zeilen: Option<u32>,
    /// Bedingung ohne WHERE, z. B. `CARRID = 'LH'`.
    #[serde(default)]
    pub filter: Option<String>,
}

/// Parameter von `abap_query`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct AbfrageParam {
    /// System; bei genau einem darf der Name fehlen.
    #[serde(default)]
    pub system: Option<String>,
    /// ABAP-SQL-SELECT, z. B. `SELECT carrid, connid FROM sflight`.
    pub sql: String,
    /// Höchstens so viele Zeilen. Standard 100, höchstens 1000.
    #[serde(default)]
    pub max_zeilen: Option<u32>,
}

/// Parameter von `abap_syntax_check`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct SyntaxParam {
    /// System; bei genau einem darf der Name fehlen.
    #[serde(default)]
    pub system: Option<String>,
    /// Objektname, z. B. `ZCL_MY_CLASS`.
    #[serde(default)]
    pub name: Option<String>,
    /// Objekt-URL. Geht vor `name`.
    #[serde(default, rename = "objectUrl")]
    pub object_url: Option<String>,
    /// Art als Hinweis für die Suche.
    #[serde(default)]
    pub typ: Option<String>,
    /// Zu prüfender Quelltext. Ohne Angabe wird der gespeicherte geprüft.
    #[serde(default)]
    pub source: Option<String>,
}

/// Parameter von `abap_unit_test`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct TestParam {
    /// System; bei genau einem darf der Name fehlen.
    #[serde(default)]
    pub system: Option<String>,
    /// Klassen- oder Programmname.
    #[serde(default)]
    pub name: Option<String>,
    /// Objekt-URL. Geht vor `name`.
    #[serde(default, rename = "objectUrl")]
    pub object_url: Option<String>,
    /// Art als Hinweis für die Suche.
    #[serde(default)]
    pub typ: Option<String>,
    /// Auch gefährliche Tests. Standard false.
    #[serde(default)]
    pub dangerous: Option<bool>,
    /// Auch kritische Tests. Standard false.
    #[serde(default)]
    pub critical: Option<bool>,
    /// Auch mittellange Tests. Standard false.
    #[serde(default)]
    pub medium: Option<bool>,
    /// Auch lange Tests. Standard false.
    #[serde(default)]
    pub long: Option<bool>,
}

/// Parameter von `abap_atc`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct AtcParam {
    /// System; bei genau einem darf der Name fehlen.
    #[serde(default)]
    pub system: Option<String>,
    /// Objektname.
    #[serde(default)]
    pub name: Option<String>,
    /// Objekt-URL. Geht vor `name`.
    #[serde(default, rename = "objectUrl")]
    pub object_url: Option<String>,
    /// Art als Hinweis für die Suche.
    #[serde(default)]
    pub typ: Option<String>,
    /// Prüfvariante. Standard `DEFAULT`.
    #[serde(default)]
    pub variant: Option<String>,
    /// Höchstens so viele Befunde. Standard 100.
    #[serde(default, rename = "maxResults")]
    pub max_results: Option<u32>,
}

/// Parameter von `abap_dumps`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct DumpParam {
    /// System; bei genau einem darf der Name fehlen.
    #[serde(default)]
    pub system: Option<String>,
    /// Filter, z. B. ein Programmname. Ohne Angabe: alle.
    #[serde(default)]
    pub query: Option<String>,
}

/// Parameter von `abap_traces` und `abap_transports`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct BenutzerParam {
    /// System; bei genau einem darf der Name fehlen.
    #[serde(default)]
    pub system: Option<String>,
    /// Benutzer. Ohne Angabe: der angemeldete Benutzer.
    #[serde(default)]
    pub user: Option<String>,
}

/// Parameter von `abap_trace_hitlist`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct TraceParam {
    /// System; bei genau einem darf der Name fehlen.
    #[serde(default)]
    pub system: Option<String>,
    /// Kennung aus abap_traces, ganz oder als Kurzform.
    #[serde(rename = "traceId")]
    pub trace_id: String,
    /// Auch Systemereignisse auswerten. Standard false.
    #[serde(default, rename = "withSystemEvents")]
    pub with_system_events: Option<bool>,
}

/// Parameter von `abap_transport_details`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct TransportParam {
    /// System; bei genau einem darf der Name fehlen.
    #[serde(default)]
    pub system: Option<String>,
    /// Auftragsnummer, z. B. `DEVK900123`.
    #[serde(rename = "transportNumber")]
    pub transport_number: String,
}

/// Parameter von `abap_text_elements`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct TextelementParam {
    /// System; bei genau einem darf der Name fehlen.
    #[serde(default)]
    pub system: Option<String>,
    /// Objektname.
    #[serde(default)]
    pub name: Option<String>,
    /// Objekt-URL. Geht vor `name`.
    #[serde(default, rename = "objectUrl")]
    pub object_url: Option<String>,
    /// Art als Hinweis für die Suche.
    #[serde(default)]
    pub typ: Option<String>,
    /// `symbols`, `selections` oder `headings`. Standard `symbols`.
    #[serde(default)]
    pub category: Option<String>,
}

// -------------------------------------------------------------- MCP-Anker

impl ServerHandler for AbapServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(quiver_tools_kern::mcp::server_info(&self.zugang))
            .with_instructions(format!(
                "ABAP über SAP ADT: Objekte suchen und lesen, Tabellen und freie SELECTs ansehen, \
                 Syntax prüfen, Unit-Tests und ATC laufen lassen, Dumps, Traces und Transporte \
                 ansehen. Alle Werkzeuge sind lesend – nichts wird im SAP-System geändert. Die \
                 {MEHRFACH_TITEL} ({MEHRFACH_EINZAHL}e) legt der User im quiver-Fenster unter \
                 Tools → {TITEL} an; jedes Werkzeug nimmt `system`, bei genau einem System darf der \
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
    use quiver_tools_kern::Zugang;
    use serde_json::json;

    use crate::Abap;

    /// Ein Server ohne Systeme.
    fn ohne_systeme() -> AbapServer {
        AbapServer::mit(
            Zugang::mit_umgebung(&Abap, |_| None),
            Ok(Sammlung::default()),
        )
    }

    fn aufruf(server: &AbapServer, name: &str, argumente: Value) -> Ergebnis {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let objekt = argumente.as_object().cloned().unwrap_or_default();
        runtime.block_on(server.aufrufen(name, Some(objekt)))
    }

    fn system() -> System {
        System {
            name: "dev-100".into(),
            benutzer: "DEVELOPER".into(),
            url: "https://sap.example.test:44300".into(),
            mandant: "100".into(),
            sprache: "DE".into(),
            passwort: "geheim-4711".into(),
            zertifikat_pruefen: true,
        }
    }

    #[test]
    fn alle_werkzeuge_sind_lesend_und_nehmen_ein_system() {
        let server = ohne_systeme();
        let werkzeuge = server.werkzeuge();
        assert_eq!(werkzeuge.len(), 19);
        for werkzeug in &werkzeuge {
            let hinweise = werkzeug.annotations.as_ref().expect("Hinweise fehlen");
            assert_eq!(hinweise.read_only_hint, Some(true), "{}", werkzeug.name);
            assert_eq!(hinweise.destructive_hint, Some(false), "{}", werkzeug.name);
            let eigenschaften = &werkzeug.input_schema["properties"];
            assert!(
                eigenschaften.get("system").is_some(),
                "{} kennt kein `system`: {eigenschaften:?}",
                werkzeug.name
            );
        }
    }

    #[test]
    fn ohne_systeme_kommt_der_hinweis_und_kein_fehler() {
        let server = ohne_systeme();
        for (name, argumente) in [
            ("abap_status", json!({})),
            ("abap_test_connection", json!({})),
            ("abap_search", json!({ "query": "Z*" })),
            ("abap_read", json!({ "name": "ZCL_DEMO" })),
            ("abap_table", json!({ "table": "SFLIGHT" })),
        ] {
            let ergebnis = aufruf(&server, name, argumente).unwrap();
            let text = ergebnis.content[0].as_text().unwrap().text.clone();
            assert!(text.contains("Zugang fehlt"), "{name}: {text}");
            assert!(text.contains("Tools → ABAP"), "{name}: {text}");
        }
    }

    /// Ein Server mit genau einem System: Parameterprüfungen laufen ohne Netz.
    fn mit_einem_system() -> AbapServer {
        let zugang = quiver_tools_kern::zugaenge::Zugang {
            name: "dev-100".into(),
            werte: [
                ("benutzer", "DEVELOPER"),
                ("url", "https://sap-dev.example.test:44300"),
                ("mandant", "100"),
                ("passwort", "geheim-4711"),
            ]
            .into_iter()
            .map(|(feld, wert)| (feld.to_string(), wert.to_string()))
            .collect(),
        };
        AbapServer::mit(
            Zugang::mit_umgebung(&Abap, |_| None),
            Ok(Sammlung::aus_zugaengen(vec![zugang]).unwrap()),
        )
    }

    #[test]
    fn leere_pflichtparameter_und_unbekannte_werkzeuge_sind_parameterfehler() {
        let server = mit_einem_system();
        let fehler = aufruf(&server, "abap_search", json!({ "query": "  " })).unwrap_err();
        assert!(fehler.message.contains("query"), "{}", fehler.message);

        let fehler = aufruf(&server, "abap_gibtsnicht", json!({})).unwrap_err();
        assert!(
            fehler.message.contains("abap_gibtsnicht"),
            "{}",
            fehler.message
        );

        let fehler = aufruf(&server, "abap_search", json!({ "query": 42 })).unwrap_err();
        assert!(fehler.message.contains("Parameter"), "{}", fehler.message);
    }

    #[test]
    fn mehrere_systeme_verlangen_einen_namen() {
        let sammlung = Sammlung::aus_zugaengen(vec![
            quiver_tools_kern::zugaenge::Zugang {
                name: "dev-100".into(),
                werte: [
                    ("benutzer", "DEVELOPER"),
                    ("url", "https://sap-dev.example.test:44300"),
                    ("mandant", "100"),
                    ("passwort", "geheim-4711"),
                ]
                .into_iter()
                .map(|(feld, wert)| (feld.to_string(), wert.to_string()))
                .collect(),
            },
            quiver_tools_kern::zugaenge::Zugang {
                name: "qas-200".into(),
                werte: [
                    ("benutzer", "DEVELOPER"),
                    ("url", "https://sap-qas.example.test:44300"),
                    ("mandant", "200"),
                    ("passwort", "geheim-4712"),
                ]
                .into_iter()
                .map(|(feld, wert)| (feld.to_string(), wert.to_string()))
                .collect(),
            },
        ])
        .unwrap();
        let server = AbapServer::mit(Zugang::mit_umgebung(&Abap, |_| None), Ok(sammlung));

        let ergebnis = aufruf(&server, "abap_status", json!({})).unwrap();
        let text = ergebnis.content[0].as_text().unwrap().text.clone();
        assert!(text.contains("dev-100"), "{text}");
        assert!(text.contains("qas-200"), "{text}");
        assert!(!text.contains("geheim-4711"), "{text}");

        let fehler = aufruf(&server, "abap_search", json!({ "query": "Z*" })).unwrap_err();
        assert!(
            fehler.message.contains("mehrere Systeme"),
            "{}",
            fehler.message
        );

        let fehler = aufruf(
            &server,
            "abap_search",
            json!({ "query": "Z*", "system": "gibtsnicht" }),
        )
        .unwrap_err();
        assert!(fehler.message.contains("gibtsnicht"), "{}", fehler.message);
    }

    #[test]
    fn die_helfer_rechnen_richtig() {
        assert_eq!(zeilen(&None), 100);
        assert_eq!(zeilen(&Some(0)), 1);
        assert_eq!(zeilen(&Some(5000)), 1000);
        assert_eq!(benutzer(&None, &system()), "DEVELOPER");
        assert_eq!(benutzer(&Some("  sap*  ".into()), &system()), "SAP*");
        assert_eq!(
            xml_attribut("/a?b=1&c=\"x\"<y>"),
            "/a?b=1&amp;c=&quot;x&quot;&lt;y&gt;"
        );
        let fehler = fehler_zu_mcp(
            &system(),
            AdtFehler::Anmeldung {
                system: "dev-100".into(),
            },
        );
        assert!(fehler.message.contains("dev-100"), "{}", fehler.message);
        assert!(
            !fehler.message.contains("geheim-4711"),
            "{}",
            fehler.message
        );
    }

    #[test]
    fn eine_kaputte_systemliste_ist_ein_parameterfehler() {
        let server = AbapServer::mit(
            Zugang::mit_umgebung(&Abap, |_| None),
            Sammlung::mit_umgebung(|_| Some("[{\"name\":".to_string())),
        );
        let fehler = aufruf(&server, "abap_status", json!({})).unwrap_err();
        assert!(
            fehler.message.contains("QUIVER_ZUGAENGE"),
            "{}",
            fehler.message
        );
    }
}
