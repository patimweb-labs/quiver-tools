// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der MCP-Server: 25 Werkzeuge über stdio (Aufgaben 07 und 08).
//!
//! Neunzehn lesende Werkzeuge (Teil 1) und sechs schreibende (Teil 2) – dazu
//! aus dem Kern `abap_profile`. Die Systeme kommen aus `QUIVER_ZUGAENGE`
//! (mehrere je Werkzeug). Fehlt die Liste, antwortet jedes Werkzeug mit dem
//! Hinweis auf das quiver-Fenster – **kein** MCP-Fehler. Jedes Werkzeug nimmt
//! `system`; bei genau einem System darf der Name fehlen.
//!
//! Die lesenden Werkzeuge tragen `read_only_hint` und arbeiten zustandslos
//! (auch Syntax-Check, Unit-Tests und ATC – die rechnen nur). Die schreibenden
//! tragen `destructive_hint` (`abap_lock`/`abap_unlock` nur „nicht lesend“) und
//! laufen über **eine** stateful Sitzung je System ([`crate::adt::sitzung`]),
//! weil ein Lock-Handle nur dort gilt.
//!
//! Drei Regeln stehen **vor** jeder Anfrage an SAP
//! ([`crate::schreiben`]): der Schalter `schreiben` je System (Standard aus),
//! der Kundennamensraum (`Z*`, `Y*`, `/…/`) und `source` bis 2 MB.

use std::any::Any;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
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
use serde_json::Value;

use crate::adt::client::{Anfrage, Client, Methode};
use crate::adt::daten::{self, Objekt};
use crate::adt::sitzung::{Sitzung, Sitzungen, FREIGABE_FRIST};
use crate::adt::xml::attribut_wert as xml_attribut;
use crate::adt::AdtFehler;
use crate::format;
use crate::schreiben::{self, Kategorie};
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
/// Aktivieren (`POST`, `api/activate.js`).
const AKTIVIERUNG: &str = "/sap/bc/adt/activation";

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

/// Der ABAP-Server: die Systeme, die Werkzeugliste und die Schreibsitzungen.
pub struct AbapServer {
    /// Der Zugang des Kerns; bei `mehrfach` ohne Felder, aber mit Titel.
    zugang: Zugang,
    systeme: Result<Sammlung, SammelFehler>,
    /// Je System eine stateful Sitzung, erst beim ersten Schreibaufruf angelegt.
    sitzungen: Arc<Sitzungen>,
    /// Hängt der SIGTERM-Wächter schon?
    ende_beobachtet: Arc<AtomicBool>,
}

impl AbapServer {
    /// Der Normalfall: alles aus der Umgebung (`QUIVER_ZUGAENGE`).
    pub fn neu(zugang: Zugang) -> Self {
        AbapServer {
            zugang,
            systeme: Sammlung::aus_umgebung(),
            sitzungen: Arc::new(Sitzungen::default()),
            ende_beobachtet: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Feste Teile (Tests und Sonderfälle), ohne Prozessumgebung.
    pub fn mit(zugang: Zugang, systeme: Result<Sammlung, SammelFehler>) -> Self {
        AbapServer {
            zugang,
            systeme,
            sitzungen: Arc::new(Sitzungen::default()),
            ende_beobachtet: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Beim Ende des Prozesses alle Locks freigeben, höchstens 5 s.
    fn ende_aufraeumen(&self) {
        let sitzungen = self.sitzungen.mit_locks();
        if sitzungen.is_empty() {
            return;
        }
        crate::adt::sitzung::freigeben_mit_frist(sitzungen);
    }

    /// Die Sitzung dieses Systems (wird beim ersten Mal angelegt).
    fn sitzung(&self, system: &System) -> Result<Arc<Sitzung>, McpError> {
        self.sitzungen
            .hole(system)
            .map_err(|fehler| fehler_zu_mcp(system, fehler))?;
        // Nach dem ersten Schreibaufruf auf SIGTERM hören – dann gehen die
        // Locks auch beim Beenden durch ein Signal frei.
        self.beobachte_ende();
        self.sitzungen
            .vorhandene(&system.name)
            .ok_or_else(|| McpError::internal_error("Die Sitzung ließ sich nicht anlegen.", None))
    }

    /// Auf `SIGTERM` achten: Locks freigeben und gehen (höchstens 5 s).
    ///
    /// Nur auf Unix; auf anderen Plattformen bleibt das normale Ende, für das
    /// [`Drop`] aufräumt.
    #[cfg(unix)]
    fn beobachte_ende(&self) {
        if self.ende_beobachtet.swap(true, Ordering::SeqCst) {
            return;
        }
        let sitzungen = self.sitzungen.clone();
        let Ok(laufzeit) = tokio::runtime::Handle::try_current() else {
            return;
        };
        laufzeit.spawn(async move {
            let Ok(mut signal) =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            else {
                return;
            };
            signal.recv().await;
            let offene = sitzungen.mit_locks();
            let _ = tokio::time::timeout(FREIGABE_FRIST, async {
                for sitzung in &offene {
                    sitzung.alle_entsperren().await;
                }
            })
            .await;
            // 128 + SIGTERM, wie eine Shell es erwartet.
            std::process::exit(143);
        });
    }

    #[cfg(not(unix))]
    fn beobachte_ende(&self) {}

    /// Alle Werkzeuge, wie `tools/list` sie meldet.
    pub fn werkzeuge(&self) -> Vec<Tool> {
        let mut alle = self.lesende_werkzeuge();
        alle.extend(self.schreibende_werkzeuge());
        // `abap_profile` kommt aus dem Kern: bei jedem Tool dasselbe lesende
        // Werkzeug.
        alle.push(profile_tool(&self.zugang));
        alle
    }

    /// Die neunzehn lesenden Werkzeuge (Teil 1).
    fn lesende_werkzeuge(&self) -> Vec<Tool> {
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

    /// Die sechs schreibenden Werkzeuge (Teil 2, Aufgabe 08).
    fn schreibende_werkzeuge(&self) -> Vec<Tool> {
        vec![
            aendernd::<SperrParam>(
                "abap_lock",
                "Objekt sperren",
                "Sperrt ein Objekt zum Bearbeiten und gibt den Lock-Handle zurück. Der Handle gilt \
                 nur in dieser Sitzung und muss an abap_write gehen oder mit abap_unlock \
                 freigegeben werden. Nur Objekte im Kundennamensraum (Z*, Y*, /…/); der Schalter \
                 „schreiben“ muss für das System an sein.",
            ),
            aendernd::<EntsperrParam>(
                "abap_unlock",
                "Objekt freigeben",
                "Gibt eine Sperre wieder frei. `lockHandle` stammt aus abap_lock und muss aus \
                 dieser Sitzung kommen.",
            ),
            schreibend::<SchreibParam>(
                "abap_write",
                "Quelltext schreiben",
                "Schreibt (speichert) den vollständigen Quelltext eines Objekts. Ohne \
                 `lockHandle` wird gesperrt, geschrieben und wieder entsperrt; mit `lockHandle` \
                 bleibt die Sperre stehen. Höchstens 2 MB, nur Kundennamensraum, nur bei \
                 eingeschaltetem „schreiben“. Danach mit abap_activate aktivieren.",
            ),
            schreibend::<AktivierParam>(
                "abap_activate",
                "Objekt aktivieren",
                "Aktiviert ein Objekt nach dem Schreiben und gibt die Meldungen mit Typ, Zeile und \
                 Text zurück (höchstens 50). `preauditRequested` ist standardmäßig an.",
            ),
            schreibend::<AnlegenParam>(
                "abap_create_object",
                "Objekt anlegen",
                "Legt ein neues Objekt an: CLAS/OC, INTF/OI, PROG/P, PROG/I, FUGR/F, TABL/DT, \
                 DTEL/DE, DOMA/DD oder DDLS/DF. Das Paket ist `$TMP` oder ein Kundenpaket; der \
                 Name muss im Kundennamensraum liegen. Gibt die Adresse des neuen Objekts zurück.",
            ),
            schreibend::<TextelementeSchreibenParam>(
                "abap_set_text_elements",
                "Textelemente schreiben",
                "Schreibt Textelemente eines Objekts: `symbols` (Schlüssel mit drei Zeichen, \
                 optional `maxLength`), `selections` oder `headings`. Ohne `lockHandle` wird \
                 gesperrt, geschrieben und entsperrt.",
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
            "abap_lock" => self.sperren(argumente).await,
            "abap_unlock" => self.entsperren(argumente).await,
            "abap_write" => self.schreiben(argumente).await,
            "abap_activate" => self.aktivieren(argumente).await,
            "abap_create_object" => self.anlegen(argumente).await,
            "abap_set_text_elements" => self.textelemente_schreiben(argumente).await,
            "abap_profile" => self.profile(argumente),
            sonst => Err(McpError::invalid_params(
                format!("Unbekanntes Werkzeug „{sonst}“."),
                None,
            )),
        }
    }

    /// `abap_profile`: bei einer Systemliste sind „die Angaben“ die Systeme.
    ///
    /// Ein einzelnes `Profil` gibt es nur ohne `mehrfach`; hier antwortet das
    /// Werkzeug darum mit derselben Liste wie `abap_status` – ohne Passwort.
    fn profile(&self, roh: Option<JsonObject>) -> Ergebnis {
        self.status(roh)
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
        let roh_filter = p
            .filter
            .as_deref()
            .map(str::trim)
            .filter(|filter| !filter.is_empty());
        if let Some(filter) = roh_filter {
            // Kein `;`, kein Kommentarzeichen – der Filter steht hinter einem
            // WHERE, das das Werkzeug selbst baut (Härtung Paket 4).
            crate::sql::filter(filter)
                .map_err(|fehler| McpError::invalid_params(fehler.to_string(), None))?;
        }
        let filter = roh_filter
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
        // Nur ein einzelnes SELECT, ohne Kette, ohne INTO und ohne FOR UPDATE
        // (Härtung Paket 4). Geprüft wird vor jeder Anfrage an das System.
        crate::sql::nur_select(sql)
            .map_err(|fehler| McpError::invalid_params(fehler.to_string(), None))?;
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

    // ------------------------------------------------------------ schreibend

    /// `abap_lock`: sperren und den Handle in dieser Sitzung merken.
    async fn sperren(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: SperrParam = argumente(roh)?;
        let system = match self.schreiben_erlaubt(p.system.as_deref()) {
            Ok(system) => system,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let objekt_param = p.objekt_param();
        self.namensraum_pruefen(&objekt_param)?;
        let modus = p
            .access_mode
            .as_deref()
            .map(str::trim)
            .filter(|modus| !modus.is_empty())
            .unwrap_or("MODIFY")
            .to_uppercase();

        let objekt = self.aufloesen_zum_schreiben(&system, &objekt_param).await?;
        let sitzung = self.sitzung(&system)?;
        let sperre = sitzung
            .sperren(&objekt.objekt_url, &modus)
            .await
            .map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        sitzung.merken(&objekt.objekt_url, &objekt.name, &sperre.handle);
        Ok(antwort(format::sperre(
            &objekt.name,
            &objekt.objekt_url,
            &sperre,
        )))
    }

    /// `abap_unlock`: eine Sperre dieser Sitzung freigeben.
    async fn entsperren(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: EntsperrParam = argumente(roh)?;
        let system = match self.schreiben_erlaubt(p.system.as_deref()) {
            Ok(system) => system,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let objekt_param = p.objekt_param();
        self.namensraum_pruefen(&objekt_param)?;
        let handle = pflicht(&p.lock_handle, "lockHandle")?.to_string();

        // Erst prüfen, ob der Handle aus dieser Sitzung stammt – ohne Anfrage.
        let sitzung = self.sitzung(&system)?;
        if !self.ist_bekannter_handle(&sitzung, &p.name, &p.object_url, &handle) {
            return Err(unbekannter_handle());
        }
        let objekt = self.aufloesen_zum_schreiben(&system, &objekt_param).await?;
        sitzung
            .entsperren(&objekt.objekt_url, &handle)
            .await
            .map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        Ok(antwort(format::entsperrt(&objekt.name, &objekt.objekt_url)))
    }

    /// `abap_write`: Quelltext speichern, Sperre wie `finally` behandeln.
    async fn schreiben(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: SchreibParam = argumente(roh)?;
        let system = match self.schreiben_erlaubt(p.system.as_deref()) {
            Ok(system) => system,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let objekt_param = p.objekt_param();
        self.namensraum_pruefen(&objekt_param)?;
        if p.source.len() > schreiben::MAX_QUELLE {
            return Err(McpError::invalid_params(
                format!(
                    "`source` ist zu groß ({} Bytes); höchstens {} Bytes (2 MB) gehen in einem \
                     Aufruf.",
                    p.source.len(),
                    schreiben::MAX_QUELLE
                ),
                None,
            ));
        }

        let sitzung = self.sitzung(&system)?;
        let uebergeben = uebergebener_handle(p.lock_handle.as_deref());
        if let Some(handle) = &uebergeben {
            if !self.ist_bekannter_handle(&sitzung, &p.name, &p.object_url, handle) {
                return Err(unbekannter_handle());
            }
        }

        let objekt = self.aufloesen_zum_schreiben(&system, &objekt_param).await?;
        let (handle, selbst_gesperrt) = self
            .handle_besorgen(&sitzung, &system, &objekt, uebergeben)
            .await?;
        let ergebnis = self
            .quelle_schreiben(
                &sitzung,
                &system,
                &objekt,
                &p.source,
                p.transport.as_deref(),
                &handle,
            )
            .await;
        // Wie `finally` in der Vorlage: auch nach einem gescheiterten Schreiben
        // wird die selbst geholte Sperre freigegeben.
        let freigegeben = if selbst_gesperrt {
            self.freigeben(&sitzung, &objekt, &handle).await
        } else {
            false
        };
        ergebnis?;
        Ok(antwort(format::geschrieben(
            &objekt.name,
            p.source.chars().count(),
            freigegeben,
        )))
    }

    /// `abap_activate`: aktivieren und die Meldungen zeigen.
    async fn aktivieren(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: AktivierParam = argumente(roh)?;
        let system = match self.schreiben_erlaubt(p.system.as_deref()) {
            Ok(system) => system,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let objekt_param = p.objekt_param();
        self.namensraum_pruefen(&objekt_param)?;
        let vorpruefung = p.preaudit_requested.unwrap_or(true);

        let objekt = self.aufloesen_zum_schreiben(&system, &objekt_param).await?;
        let sitzung = self.sitzung(&system)?;
        let rumpf =
            schreiben::aktivierung_rumpf(&objekt.objekt_url, &objekt.name, &objekt.quelltext_url);
        let geantwortet = sitzung
            .anfrage(
                &Anfrage::senden(Methode::Post, AKTIVIERUNG, rumpf)
                    .mit_annahme("application/*")
                    .mit_kopf("Content-Type", "application/xml")
                    .mit_abfrage("method", "activate")
                    .mit_abfrage(
                        "preauditRequested",
                        if vorpruefung { "true" } else { "false" },
                    ),
            )
            .await
            .map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        let ergebnis = daten::aktivierung(&geantwortet.text)
            .map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        Ok(antwort(format::aktivierung(&objekt.name, &ergebnis)))
    }

    /// `abap_create_object`: ein neues Objekt im Kundenpaket anlegen.
    async fn anlegen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: AnlegenParam = argumente(roh)?;
        let system = match self.schreiben_erlaubt(p.system.as_deref()) {
            Ok(system) => system,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let art = schreiben::anlegbar(&p.object_type).ok_or_else(|| {
            McpError::invalid_params(
                format!(
                    "Die Objektart „{}“ legt dieses Werkzeug nicht an; möglich sind: {}.",
                    p.object_type,
                    schreiben::anlegbare_arten()
                ),
                None,
            )
        })?;
        let name = pflicht(&p.name, "name")?;
        if !schreiben::im_kundennamensraum(name) {
            return Err(McpError::invalid_params(format::namensraum(name), None));
        }
        let paket = pflicht(&p.parent_name, "parentName")?;
        if !schreiben::paket_erlaubt(paket) {
            return Err(McpError::invalid_params(
                format!(
                    "Das Paket „{paket}“ ist nicht erlaubt – erlaubt sind `$TMP` und \
                     Kundenpakete (Z*, Y*, /…/)."
                ),
                None,
            ));
        }
        let beschreibung = pflicht(&p.description, "description")?;

        let rumpf = schreiben::anlege_rumpf(
            art,
            name,
            paket,
            beschreibung,
            &system.sprache,
            &system.benutzer,
        );
        let anfrage = mit_transport(
            Anfrage::senden(Methode::Post, schreiben::anlege_pfad(art), rumpf)
                .mit_annahme("application/*")
                .mit_kopf("Content-Type", "application/*"),
            p.transport.as_deref(),
        );

        let sitzung = self.sitzung(&system)?;
        let geantwortet = sitzung
            .anfrage(&anfrage)
            .await
            .map_err(|fehler| fehler_zu_mcp(&system, fehler))?;
        let url = geantwortet
            .ort
            .filter(|ort| !ort.trim().is_empty())
            .unwrap_or_else(|| schreiben::neue_objekt_url(art, name));
        Ok(antwort(format::neues_objekt(art.typ, name, paket, &url)))
    }

    /// `abap_set_text_elements`: Textelemente schreiben.
    async fn textelemente_schreiben(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: TextelementeSchreibenParam = argumente(roh)?;
        let system = match self.schreiben_erlaubt(p.system.as_deref()) {
            Ok(system) => system,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let objekt_param = p.objekt_param();
        self.namensraum_pruefen(&objekt_param)?;
        let kategorie = Kategorie::aus_text(&p.category).ok_or_else(|| {
            McpError::invalid_params(
                format!(
                    "`category` muss symbols, selections oder headings sein, nicht „{}“.",
                    p.category
                ),
                None,
            )
        })?;
        let elemente: Vec<schreiben::Element> = p
            .elements
            .iter()
            .map(|element| schreiben::Element {
                id: element.id.clone(),
                text: element.text.clone(),
                max_laenge: element.max_length,
            })
            .collect();
        let rumpf = schreiben::textelemente_rumpf(&elemente, kategorie)
            .map_err(|text| McpError::invalid_params(text, None))?;

        let sitzung = self.sitzung(&system)?;
        let uebergeben = uebergebener_handle(p.lock_handle.as_deref());
        if let Some(handle) = &uebergeben {
            if !self.ist_bekannter_handle(&sitzung, &p.name, &p.object_url, handle) {
                return Err(unbekannter_handle());
            }
        }

        let objekt = self.aufloesen_zum_schreiben(&system, &objekt_param).await?;
        let medientyp = kategorie.medientyp();
        let url = format!(
            "{}/source/{}",
            daten::textelemente_url(&objekt.typ, &objekt.name),
            kategorie.name()
        );
        let (handle, selbst_gesperrt) = self
            .handle_besorgen(&sitzung, &system, &objekt, uebergeben)
            .await?;
        let anfrage = mit_transport(
            Anfrage::senden(Methode::Put, url, rumpf)
                .mit_annahme(medientyp.clone())
                .mit_kopf("Content-Type", format!("{medientyp}; charset=UTF-8"))
                .mit_abfrage("lockHandle", handle.clone()),
            p.transport.as_deref(),
        );
        let ergebnis = sitzung
            .anfrage(&anfrage)
            .await
            .map(|_| ())
            .map_err(|fehler| fehler_zu_mcp(&system, fehler));
        let freigegeben = if selbst_gesperrt {
            self.freigeben(&sitzung, &objekt, &handle).await
        } else {
            false
        };
        ergebnis?;
        Ok(antwort(format::textelemente_geschrieben(
            &objekt.name,
            kategorie.name(),
            elemente.len(),
            freigegeben,
        )))
    }

    /// Das System – aber nur, wenn der Schalter „schreiben“ an ist.
    ///
    /// Ist er aus, kommt der Hinweis **ohne** Anfrage an SAP.
    fn schreiben_erlaubt(&self, name: Option<&str>) -> Result<System, OhneProfil> {
        let system = self.waehlen(name)?;
        if !system.schreiben_erlaubt() {
            return Err(OhneProfil::Hinweis(antwort(format::schreiben_aus(
                &system.name,
            ))));
        }
        Ok(system)
    }

    /// Der Objektname im Kundennamensraum – ohne Anfrage an SAP geprüft.
    ///
    /// **Jede** Angabe muss passen: `name` und der Name aus `objectUrl`. Sonst
    /// ließe sich mit `name: "ZFOO"` plus `objectUrl` auf ein SAP-Objekt die
    /// Regel umgehen (geschrieben wird an die URL).
    fn namensraum_pruefen(&self, p: &ObjektParam) -> Result<(), McpError> {
        let namen = namen_ohne_anfrage(p.name.as_deref(), p.object_url.as_deref());
        if namen.is_empty() {
            return Err(McpError::invalid_params(
                "Bitte `name` oder `objectUrl` angeben.".to_string(),
                None,
            ));
        }
        for name in namen {
            if !schreiben::im_kundennamensraum(&name) {
                return Err(McpError::invalid_params(format::namensraum(&name), None));
            }
        }
        Ok(())
    }

    /// Auflösen für einen Schreibvorgang: nur ein **exakter** Treffer zählt
    /// (kein „erster Treffer“), und der Name, den SAP für das Objekt meldet,
    /// muss ebenfalls im Kundennamensraum liegen.
    async fn aufloesen_zum_schreiben(
        &self,
        system: &System,
        p: &ObjektParam,
    ) -> Result<Objekt, McpError> {
        let objekt = self.aufloesen(system, p).await?;
        let gemeldet = [
            Some(objekt.name.clone()),
            schreiben::name_aus_url(&objekt.objekt_url),
        ];
        for name in gemeldet.into_iter().flatten() {
            if !schreiben::im_kundennamensraum(&name) {
                return Err(McpError::invalid_params(format::namensraum(&name), None));
            }
        }
        if let Some(gesucht) = p.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
            if p.object_url
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
                && !objekt.name.eq_ignore_ascii_case(gesucht)
            {
                return Err(McpError::invalid_params(
                    format!(
                        "„{gesucht}“ ist nicht eindeutig gefunden (SAP meldet „{}“) – bitte \
                         `objectUrl` angeben.",
                        objekt.name
                    ),
                    None,
                ));
            }
        }
        Ok(objekt)
    }

    /// Der Handle für einen Schreibvorgang: der bereits geprüfte übergebene
    /// oder ein frisch geholter. Das zweite Feld sagt, ob **wir** gesperrt haben
    /// und darum wieder freigeben müssen.
    async fn handle_besorgen(
        &self,
        sitzung: &Sitzung,
        system: &System,
        objekt: &Objekt,
        uebergeben: Option<String>,
    ) -> Result<(String, bool), McpError> {
        match uebergeben {
            Some(handle) => Ok((handle, false)),
            None => {
                let sperre = sitzung
                    .sperren(&objekt.objekt_url, "MODIFY")
                    .await
                    .map_err(|fehler| fehler_zu_mcp(system, fehler))?;
                sitzung.merken(&objekt.objekt_url, &objekt.name, &sperre.handle);
                Ok((sperre.handle, true))
            }
        }
    }

    /// Stammt dieser Handle aus dieser Sitzung? Ohne Anfrage an SAP.
    fn ist_bekannter_handle(
        &self,
        sitzung: &Sitzung,
        name: &Option<String>,
        object_url: &Option<String>,
        handle: &str,
    ) -> bool {
        sitzung
            .handle_fuer(name.as_deref(), object_url.as_deref())
            .as_deref()
            == Some(handle)
    }

    /// Eine selbst geholte Sperre freigeben. Fehler nur ins Log – der
    /// Schreibvorgang ist wichtiger als das Aufräumen.
    async fn freigeben(&self, sitzung: &Sitzung, objekt: &Objekt, handle: &str) -> bool {
        match sitzung.entsperren(&objekt.objekt_url, handle).await {
            Ok(()) => true,
            Err(fehler) => {
                tracing::warn!(
                    system = %sitzung.system().name,
                    objekt = %objekt.objekt_url,
                    "Sperre nach dem Schreiben nicht freigegeben: {fehler}"
                );
                false
            }
        }
    }

    /// Den Quelltext in den Hauptinclude schreiben (`setObjectSource`).
    async fn quelle_schreiben(
        &self,
        sitzung: &Sitzung,
        system: &System,
        objekt: &Objekt,
        quelle: &str,
        transport: Option<&str>,
        handle: &str,
    ) -> Result<(), McpError> {
        // XML-Quelltexte (z. B. CDS) gehen als `application/*`, alles andere als
        // Klartext – dieselbe Wahl wie in der Vorlage.
        let typ = if quelle.trim_start().starts_with("<?xml") {
            "application/*"
        } else {
            "text/plain; charset=utf-8"
        };
        let anfrage = mit_transport(
            Anfrage::senden(Methode::Put, &objekt.quelltext_url, quelle)
                .mit_annahme("text/plain")
                .mit_kopf("Content-Type", typ)
                .mit_abfrage("lockHandle", handle),
            transport,
        );
        sitzung
            .anfrage(&anfrage)
            .await
            .map(|_| ())
            .map_err(|fehler| fehler_zu_mcp(system, fehler))
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

/// Ein Werkzeug, das nichts liest, aber auch nichts zerstört: Sperren und
/// Freigeben (`read_only_hint` falsch, `destructive_hint` falsch).
fn aendernd<P: JsonSchema + Any>(
    name: &'static str,
    titel: &'static str,
    beschreibung: &'static str,
) -> Tool {
    Tool::new(name, beschreibung, schema_for_type::<P>())
        .with_title(titel)
        .with_annotations(ToolAnnotations::from_raw(
            Some(titel.to_string()),
            Some(false),
            Some(false),
            Some(false),
            Some(false),
        ))
}

/// Ein schreibendes Werkzeug (`destructive_hint`, Aufgabe 08).
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

/// Der Objektname aus `name` oder `objectUrl` – **ohne** Anfrage an SAP.
fn namen_ohne_anfrage(name: Option<&str>, object_url: Option<&str>) -> Vec<String> {
    let mut namen = Vec::new();
    if let Some(name) = name.map(str::trim).filter(|name| !name.is_empty()) {
        namen.push(name.to_string());
    }
    if let Some(name) = object_url.and_then(schreiben::name_aus_url) {
        namen.push(name);
    }
    namen
}

/// Die Meldung zu einem Handle, der nicht aus dieser Sitzung stammt.
fn unbekannter_handle() -> McpError {
    McpError::invalid_params(
        "Lock-Handle unbekannt – erst abap_lock aufrufen. Ein Handle gilt nur in der Sitzung, \
         die ihn geholt hat."
            .to_string(),
        None,
    )
}

/// Der übergebene Lock-Handle, falls einer dabei ist.
fn uebergebener_handle(handle: Option<&str>) -> Option<String> {
    handle
        .map(str::trim)
        .filter(|handle| !handle.is_empty())
        .map(str::to_string)
}

/// Einen Transportauftrag als `corrNr` anhängen, falls einer genannt ist.
fn mit_transport(anfrage: Anfrage, transport: Option<&str>) -> Anfrage {
    match transport
        .map(str::trim)
        .filter(|transport| !transport.is_empty())
    {
        Some(transport) => anfrage.mit_abfrage("corrNr", transport),
        None => anfrage,
    }
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

/// Aus einem ADT-Fehler wird ein MCP-Fehler – mit Systemnamen, nie mit Geheimnis.
fn fehler_zu_mcp(system: &System, fehler: AdtFehler) -> McpError {
    let text = format!("{}: {fehler}", system.name);
    match fehler {
        AdtFehler::Anmeldung { .. }
        | AdtFehler::Csrf { .. }
        | AdtFehler::SitzungAbgelaufen { .. } => McpError::invalid_params(text, None),
        _ => McpError::internal_error(text, None),
    }
}

/// Beim Ende des Prozesses (stdin zu, normales Ende) alle Locks freigeben.
impl Drop for AbapServer {
    fn drop(&mut self) {
        self.ende_aufraeumen();
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

// ------------------------------------------------------- schreibend (Teil 2)

/// Parameter von `abap_lock`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct SperrParam {
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
    #[serde(default, alias = "type")]
    pub typ: Option<String>,
    /// Sperrmodus: `MODIFY` (Standard) oder `EDIT`.
    #[serde(default, rename = "accessMode")]
    pub access_mode: Option<String>,
}

impl SperrParam {
    fn objekt_param(&self) -> ObjektParam {
        ObjektParam {
            system: self.system.clone(),
            name: self.name.clone(),
            object_url: self.object_url.clone(),
            typ: self.typ.clone(),
        }
    }
}

/// Parameter von `abap_unlock`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct EntsperrParam {
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
    #[serde(default, alias = "type")]
    pub typ: Option<String>,
    /// Der Handle aus `abap_lock`.
    #[serde(rename = "lockHandle")]
    pub lock_handle: String,
}

impl EntsperrParam {
    fn objekt_param(&self) -> ObjektParam {
        ObjektParam {
            system: self.system.clone(),
            name: self.name.clone(),
            object_url: self.object_url.clone(),
            typ: self.typ.clone(),
        }
    }
}

/// Parameter von `abap_write`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct SchreibParam {
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
    #[serde(default, alias = "type")]
    pub typ: Option<String>,
    /// Der vollständige neue Quelltext (höchstens 2 MB).
    pub source: String,
    /// Transportauftrag, z. B. `DEVK900123`.
    #[serde(default)]
    pub transport: Option<String>,
    /// Handle aus `abap_lock`. Ohne Angabe wird gesperrt und wieder entsperrt.
    #[serde(default, rename = "lockHandle")]
    pub lock_handle: Option<String>,
}

impl SchreibParam {
    fn objekt_param(&self) -> ObjektParam {
        ObjektParam {
            system: self.system.clone(),
            name: self.name.clone(),
            object_url: self.object_url.clone(),
            typ: self.typ.clone(),
        }
    }
}

/// Parameter von `abap_activate`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct AktivierParam {
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
    #[serde(default, alias = "type")]
    pub typ: Option<String>,
    /// Vorprüfung anfordern. Standard `true`.
    #[serde(default, rename = "preauditRequested")]
    pub preaudit_requested: Option<bool>,
}

impl AktivierParam {
    fn objekt_param(&self) -> ObjektParam {
        ObjektParam {
            system: self.system.clone(),
            name: self.name.clone(),
            object_url: self.object_url.clone(),
            typ: self.typ.clone(),
        }
    }
}

/// Parameter von `abap_create_object`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct AnlegenParam {
    /// System; bei genau einem darf der Name fehlen.
    #[serde(default)]
    pub system: Option<String>,
    /// Objektart: `CLAS/OC`, `INTF/OI`, `PROG/P`, `PROG/I`, `FUGR/F`, `TABL/DT`,
    /// `DTEL/DE`, `DOMA/DD` oder `DDLS/DF`.
    #[serde(rename = "objectType")]
    pub object_type: String,
    /// Name des neuen Objekts, z. B. `ZCL_NEU`.
    pub name: String,
    /// Paket, in das angelegt wird (`$TMP` für lokal, sonst Z*, Y*, /…/).
    #[serde(rename = "parentName")]
    pub parent_name: String,
    /// Beschreibung des Objekts.
    pub description: String,
    /// Transportauftrag.
    #[serde(default)]
    pub transport: Option<String>,
}

/// Ein Textelement, wie der Agent es übergibt.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct ElementParam {
    /// Schlüssel, z. B. `001` oder `LISTHEADER`.
    pub id: String,
    /// Der Text.
    pub text: String,
    /// Höchstlänge (nur Textsymbole).
    #[serde(default, rename = "maxLength")]
    pub max_length: Option<u32>,
}

/// Parameter von `abap_set_text_elements`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct TextelementeSchreibenParam {
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
    #[serde(default, alias = "type")]
    pub typ: Option<String>,
    /// `symbols`, `selections` oder `headings`.
    pub category: String,
    /// Die zu schreibenden Elemente.
    pub elements: Vec<ElementParam>,
    /// Transportauftrag.
    #[serde(default)]
    pub transport: Option<String>,
    /// Handle aus `abap_lock`. Ohne Angabe wird gesperrt und wieder entsperrt.
    #[serde(default, rename = "lockHandle")]
    pub lock_handle: Option<String>,
}

impl TextelementeSchreibenParam {
    fn objekt_param(&self) -> ObjektParam {
        ObjektParam {
            system: self.system.clone(),
            name: self.name.clone(),
            object_url: self.object_url.clone(),
            typ: self.typ.clone(),
        }
    }
}

// -------------------------------------------------------------- MCP-Anker

impl ServerHandler for AbapServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(quiver_tools_kern::mcp::server_info(&self.zugang))
            .with_instructions(format!(
                "ABAP über SAP ADT: Objekte suchen und lesen, Tabellen und freie SELECTs ansehen, \
                 Syntax prüfen, Unit-Tests und ATC laufen lassen, Dumps, Traces und Transporte \
                 ansehen. Schreibend gehen Sperren, Quelltext schreiben, Aktivieren, Anlegen und \
                 Textelemente – aber nur für Objekte im Kundennamensraum (Z*, Y*, /…/) und nur, \
                 wenn der Schalter „schreiben“ für das System an ist (Standard: aus). Ist er aus, \
                 antworten die schreibenden Werkzeuge mit dem Hinweis und ohne Anfrage an SAP. \
                 Die {MEHRFACH_TITEL} ({MEHRFACH_EINZAHL}e) legt der User im quiver-Fenster unter \
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

    /// Der Text einer Antwort.
    fn text_von(ergebnis: CallToolResult) -> String {
        ergebnis.content[0]
            .as_text()
            .expect("keine Textantwort")
            .text
            .clone()
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
            schreiben: true,
            ca_zertifikat: None,
        }
    }

    #[test]
    fn alle_werkzeuge_nehmen_ein_system_und_tragen_den_richtigen_hinweis() {
        let server = ohne_systeme();
        let werkzeuge = server.werkzeuge();
        assert_eq!(werkzeuge.len(), 26, "25 Werkzeuge plus abap_profile");

        let schreibend = [
            "abap_write",
            "abap_activate",
            "abap_create_object",
            "abap_set_text_elements",
        ];
        let aendernd = ["abap_lock", "abap_unlock"];

        for werkzeug in &werkzeuge {
            let hinweise = werkzeug.annotations.as_ref().expect("Hinweise fehlen");
            let name = werkzeug.name.as_ref();
            let destruktiv = schreibend.contains(&name);
            let allein_aendernd = aendernd.contains(&name);
            let lesend = !destruktiv && !allein_aendernd;
            assert_eq!(hinweise.read_only_hint, Some(lesend), "{name}");
            assert_eq!(hinweise.destructive_hint, Some(destruktiv), "{name}");
            // Alle Werkzeuge dieses Tools tragen einen Titel – die 25 eigenen
            // direkt, `abap_profile` aus dem Kern in den Hinweisen.
            if name == "abap_profile" {
                assert!(hinweise.title.is_some(), "{name} hat keinen Titel");
            } else {
                assert!(werkzeug.title.is_some(), "{name} hat keinen Titel");
            }
        }

        // Jedes Werkzeug außer `abap_profile` und `abap_create_object` nimmt
        // `system`; create_object bekommt es auch (nur nicht als Objekt).
        for werkzeug in &werkzeuge {
            let eigenschaften = &werkzeug.input_schema["properties"];
            if werkzeug.name.as_ref() == "abap_profile" {
                continue;
            }
            assert!(
                eigenschaften.get("system").is_some(),
                "{} kennt kein `system`: {eigenschaften:?}",
                werkzeug.name
            );
        }

        // Die Namen stehen in einer festen Reihenfolge: erst lesend, dann
        // schreibend, zuletzt das Profil.
        let namen: Vec<&str> = werkzeuge.iter().map(|w| w.name.as_ref()).collect();
        assert_eq!(namen[19], "abap_lock");
        assert_eq!(namen[24], "abap_set_text_elements");
        assert_eq!(namen[25], "abap_profile");
    }

    #[test]
    fn das_profil_nennt_bei_einer_liste_die_systeme_ohne_passwort() {
        let server = mit_einem_system();
        let text = text_von(aufruf(&server, "abap_profile", json!({})).unwrap());
        assert!(text.contains("dev-100"), "{text}");
        assert!(text.contains("Schreiben aus"), "{text}");
        assert!(!text.contains("geheim-4711"), "{text}");

        // Ohne Systeme bleibt es beim Hinweis, kein Fehler.
        let text = text_von(aufruf(&ohne_systeme(), "abap_profile", json!({})).unwrap());
        assert!(text.contains("Zugang fehlt"), "{text}");
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
            ("abap_lock", json!({ "name": "ZCL_DEMO" })),
            (
                "abap_write",
                json!({ "name": "ZCL_DEMO", "source": "REPORT x." }),
            ),
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
