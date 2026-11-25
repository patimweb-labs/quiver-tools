// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der MCP-Server: 13 Werkzeuge über stdio (Aufgabe 14).
//!
//! Neun lesende Werkzeuge (`read_only_hint`) und vier verändernde
//! (`destructive_hint`, brauchen den Schalter `schreiben`) – dazu aus dem Kern
//! `confluence_profile`. Die Sites kommen aus `QUIVER_ZUGAENGE` (mehrere je
//! Werkzeug). Fehlt die Liste, antwortet jedes Werkzeug mit dem Hinweis auf das
//! quiver-Fenster – **kein** MCP-Fehler. Jedes Werkzeug nimmt `verbindung`; bei
//! genau einer Site darf der Name fehlen.
//!
//! Fünf Regeln stehen **vor** jeder Anfrage (Aufgabe 12):
//!
//! | Regel | Wirkung |
//! |---|---|
//! | `schreiben` je Site (Standard aus) | verändernde Werkzeuge antworten ohne Anfrage |
//! | `nur` je Site | Schreiben außerhalb der Bereiche wird ohne Änderung abgelehnt |
//! | Bereich immer vom Server | der Schlüssel kommt aus der Seite, nie vom Agenten |
//! | `version` Pflicht beim Ändern | weicht sie ab oder kommt 409: „neu lesen“ |
//! | Werte im Pfad kodiert | `/`, `?`, `#` öffnen keinen anderen Endpunkt |
//!
//! Dazu die Korrekturen nach der Abnahme: leere Werte und Punkt-Segmente werden
//! abgelehnt, `anhaengen` bricht ohne alten Inhalt ab, ein fehlender Titel wird
//! nie zu „(ohne Titel)“, `status` gilt nur laut Spezifikation, eine Elternseite
//! muss im selben Bereich liegen, und Labels gehen über die ältere API.

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

use crate::client::{self, Anfrage, Client, API, BENUTZER, LABEL, SUCHE};
use crate::cql;
use crate::daten;
use crate::fehler::Fehler;
use crate::format;
use crate::html;
use crate::sites::{AuswahlFehler, SammelFehler, Sammlung, Site};
use crate::werkzeug::{MEHRFACH_EINZAHL, MEHRFACH_TITEL, TITEL};

/// Was ein Werkzeug zurückgibt.
type Ergebnis = Result<CallToolResult, McpError>;

/// Der Confluence-Server: die Sites und die Werkzeugliste.
pub struct ConfluenceServer {
    /// Der Zugang des Kerns; bei `mehrfach` ohne Felder, aber mit Titel.
    zugang: Zugang,
    sites: Result<Sammlung, SammelFehler>,
}

impl ConfluenceServer {
    /// Der Normalfall: alles aus der Umgebung (`QUIVER_ZUGAENGE`).
    pub fn neu(zugang: Zugang) -> Self {
        ConfluenceServer {
            zugang,
            sites: Sammlung::aus_umgebung(),
        }
    }

    /// Feste Teile (Tests und Sonderfälle), ohne Prozessumgebung.
    pub fn mit(zugang: Zugang, sites: Result<Sammlung, SammelFehler>) -> Self {
        ConfluenceServer { zugang, sites }
    }

    /// Alle Werkzeuge, wie `tools/list` sie meldet.
    pub fn werkzeuge(&self) -> Vec<Tool> {
        let mut alle = self.lesende_werkzeuge();
        alle.extend(self.schreibende_werkzeuge());
        // `confluence_profile` kommt aus dem Kern: bei jedem Tool dasselbe
        // lesende Werkzeug.
        alle.push(profile_tool(&self.zugang));
        alle
    }

    /// Die neun lesenden Werkzeuge.
    fn lesende_werkzeuge(&self) -> Vec<Tool> {
        vec![
            lesend::<VerbindungParam>(
                "confluence_status",
                "Sites",
                "Listet die im quiver-Fenster eingetragenen Confluence-Sites: Name, Adresse, \
                 freigegebene Bereiche und ob „schreiben“ an ist. Öffnet keine Verbindung, zeigt \
                 weder Token noch E-Mail.",
            ),
            lesend::<VerbindungParam>(
                "confluence_testen",
                "Anmeldung prüfen",
                "Meldet sich mit E-Mail und API-Token an und gibt zurück, wer angemeldet ist. \
                 Ohne `verbindung` prüft es alle Sites. Ein abgelehntes Token wird nicht \
                 wiederholt.",
            ),
            lesend::<BereicheParam>(
                "confluence_bereiche",
                "Bereiche",
                "Listet die Bereiche (Spaces) einer Site: Schlüssel, Name, Id, Art. `schluessel` \
                 filtert auf einzelne Schlüssel (kommagetrennt). `anzahl` ist standardmäßig 25 und \
                 höchstens 100.",
            ),
            lesend::<SucheParam>(
                "confluence_suchen",
                "Suchen (CQL)",
                "Sucht Inhalte. Entweder mit fertigem `cql` oder bequem über `text` und `bereich`; \
                 daraus wird eine CQL-Abfrage gebaut, maskiert und über die Suche der älteren API \
                 geschickt. Gibt Id, Typ, Titel und Bereich zurück.",
            ),
            lesend::<SeiteParam>(
                "confluence_seite",
                "Seite",
                "Eine Seite per `id` oder per `bereich` und `titel`. Der Inhalt kommt als Text \
                 (Storage-Format gelesen); `als_storage` gibt das Original. Lange Texte werden \
                 über `max_zeichen` gekürzt (Standard 4000).",
            ),
            lesend::<ListeParam>(
                "confluence_unterseiten",
                "Unterseiten",
                "Die Unterseiten einer Seite mit Id, Titel und Status. `anzahl` ist standardmäßig \
                 25 und höchstens 100.",
            ),
            lesend::<KommentareParam>(
                "confluence_kommentare",
                "Kommentare",
                "Die Fußkommentare einer Seite mit Autor-Id, Datum und Text (gekürzt über \
                 `max_zeichen`, Standard 4000).",
            ),
            lesend::<ListeParam>(
                "confluence_anhaenge",
                "Anhänge",
                "Die Anhänge einer Seite: Name, Medientyp, Größe und Version. Nur die Liste – \
                 heruntergeladen wird nichts.",
            ),
            lesend::<ListeParam>(
                "confluence_labels",
                "Labels",
                "Die Labels einer Seite mit Präfix und Name.",
            ),
        ]
    }

    /// Die vier verändernden Werkzeuge (brauchen `schreiben`).
    fn schreibende_werkzeuge(&self) -> Vec<Tool> {
        vec![
            schreibend::<AnlegenParam>(
                "confluence_seite_anlegen",
                "Seite anlegen",
                "Legt eine Seite in einem Bereich an: Titel, Text und optional eine Elternseite. \
                 `text` wird maskiert ins Storage-Format gesetzt, `storage` unverändert übernommen \
                 (genau eines von beiden). Braucht den Schalter „schreiben“.",
            ),
            schreibend::<AendernParam>(
                "confluence_seite_aendern",
                "Seite ändern",
                "Ändert Titel und Inhalt einer Seite. `version` ist Pflicht: weicht sie von der \
                 Version auf dem Server ab (oder antwortet er mit 409), wird nichts überschrieben. \
                 Titel und Status bleiben, wenn sie nicht angegeben sind; `anhaengen` hängt an den \
                 vorhandenen Inhalt an, statt ihn zu ersetzen.",
            ),
            schreibend::<KommentierenParam>(
                "confluence_kommentieren",
                "Kommentieren",
                "Schreibt einen Fußkommentar an eine Seite. `text` wird maskiert ins \
                 Storage-Format gesetzt, `storage` unverändert übernommen (genau eines von beiden).",
            ),
            schreibend::<LabelParam>(
                "confluence_label_hinzufuegen",
                "Label hinzufügen",
                "Hängt ein globales Label an eine Seite. Vorhandene Labels bleiben stehen.",
            ),
        ]
    }

    /// Ein Werkzeug aufrufen. Auch der Weg, den die Tests nehmen.
    pub async fn aufrufen(&self, name: &str, argumente: Option<JsonObject>) -> Ergebnis {
        match name {
            "confluence_status" => self.status(argumente),
            "confluence_testen" => self.testen(argumente).await,
            "confluence_bereiche" => self.bereiche(argumente).await,
            "confluence_suchen" => self.suchen(argumente).await,
            "confluence_seite" => self.seite(argumente).await,
            "confluence_unterseiten" => self.unterseiten(argumente).await,
            "confluence_kommentare" => self.kommentare(argumente).await,
            "confluence_anhaenge" => self.anhaenge(argumente).await,
            "confluence_labels" => self.labels(argumente).await,
            "confluence_seite_anlegen" => self.seite_anlegen(argumente).await,
            "confluence_seite_aendern" => self.seite_aendern(argumente).await,
            "confluence_kommentieren" => self.kommentieren(argumente).await,
            "confluence_label_hinzufuegen" => self.label_hinzufuegen(argumente).await,
            "confluence_profile" => self.profile(argumente),
            sonst => Err(McpError::invalid_params(
                format!("Unbekanntes Werkzeug „{sonst}“."),
                None,
            )),
        }
    }

    /// `confluence_profile`: bei einer Site-Liste sind „die Angaben“ die Sites.
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
    /// Ist er aus, kommt der Hinweis **ohne** Anfrage an Confluence.
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

    /// Den Bereich vom Server holen: Id und Schlüssel.
    ///
    /// Der Agent nennt nur einen Schlüssel; was er darf, entscheidet der
    /// Schlüssel, den Confluence zurückgibt (Aufgabe 12, Sicherheit 6). Der
    /// erste Treffer zählt nur, wenn sein Schlüssel dem gesuchten entspricht –
    /// sonst wäre die Freigabe die des falschen Bereichs (Korrekturen).
    async fn bereich_holen(client: &Client, schluessel: &str) -> Result<(String, String), Fehler> {
        let schluessel = schluessel.trim();
        let antwort = client
            .json(
                &Anfrage::holen(format!("{API}/spaces"))
                    .mit_abfrage("keys", schluessel)
                    .mit_abfrage("limit", "1"),
            )
            .await?;
        let Some(bereich) = daten::liste(&antwort, "results").first() else {
            return Err(Fehler::Antwort {
                text: format!(
                    "Den Bereich „{schluessel}“ gibt es nicht – die Schlüssel zeigt \
                     confluence_bereiche."
                ),
            });
        };
        let id = daten::text(bereich, "id").ok_or_else(|| Fehler::Antwort {
            text: format!("Der Bereich „{schluessel}“ hat keine Id in der Antwort."),
        })?;
        let key = daten::text(bereich, "key").ok_or_else(|| Fehler::Antwort {
            text: format!(
                "Der Bereich „{schluessel}“ nennt keinen Schlüssel in der Antwort – es wurde \
                 nichts geändert."
            ),
        })?;
        if !key.eq_ignore_ascii_case(schluessel) {
            return Err(Fehler::Antwort {
                text: format!(
                    "Die Antwort auf „{schluessel}“ nennt den Bereich „{key}“ – es wurde nichts \
                     geändert."
                ),
            });
        }
        Ok((id, key))
    }

    /// Den Bereichsschlüssel einer Seite vom Server holen (`Seite → Bereich`).
    ///
    /// Fehlt der Schlüssel in der Antwort, ist das ein Fehler – die Id ist
    /// kein Schlüssel und darf nicht als Freigabe durchgehen (Korrekturen).
    async fn bereich_von_seite(client: &Client, seite: &Value) -> Result<String, Fehler> {
        let space_id = daten::text(seite, "spaceId").ok_or_else(|| Fehler::Antwort {
            text: "Die Seite nennt keinen Bereich in der Antwort.".to_string(),
        })?;
        let antwort = client
            .json(&Anfrage::holen(format!(
                "{API}/spaces/{}",
                client::pfad_wert(&space_id)?
            )))
            .await?;
        daten::text(&antwort, "key").ok_or_else(|| Fehler::Antwort {
            text: format!(
                "Der Bereich {space_id} der Seite nennt keinen Schlüssel in der Antwort – es \
                 wurde nichts geändert."
            ),
        })
    }

    /// Der Bereich muss im Feld `nur` freigegeben sein – ohne Änderung sonst.
    fn bereich_erlaubt(site: &Site, schluessel: &str) -> Result<(), OhneProfil> {
        if site.bereich_erlaubt(schluessel) {
            Ok(())
        } else {
            Err(OhneProfil::Hinweis(antwort(format::fremder_bereich(
                &site.name, schluessel,
            ))))
        }
    }

    // ------------------------------------------------------------- lesend

    fn status(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: VerbindungParam = argumente(roh)?;
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
            let geantwortet = client
                .json(&Anfrage::holen(BENUTZER))
                .await
                .map_err(fehler_zu_mcp)?;
            zeilen.push(format::testen(site, &geantwortet));
        }
        Ok(antwort(zeilen.join("\n\n")))
    }

    async fn bereiche(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: BereicheParam = argumente(roh)?;
        let site = match self.waehlen(p.verbindung.as_deref()) {
            Ok(site) => site,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let anzahl = zaehlen(p.anzahl);
        let mut abfrage = vec![("limit".to_string(), anzahl.to_string())];
        for schluessel in schluessel_liste(p.schluessel.as_deref()) {
            abfrage.push(("keys".to_string(), schluessel));
        }

        let client = Self::client(&site)?;
        let auszug = client
            .liste(&format!("{API}/spaces"), abfrage, anzahl)
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::bereiche(&auszug)))
    }

    async fn suchen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: SucheParam = argumente(roh)?;
        let site = match self.waehlen(p.verbindung.as_deref()) {
            Ok(site) => site,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let anzahl = zaehlen(p.anzahl);

        let cql = match p
            .cql
            .as_deref()
            .map(str::trim)
            .filter(|cql| !cql.is_empty())
        {
            Some(cql) => cql.to_string(),
            None => {
                let komfort = cql::Komfort {
                    text: p.text.as_deref(),
                    bereich: p.bereich.as_deref(),
                };
                match cql::bauen(&komfort) {
                    Some(cql) => cql,
                    None => {
                        return Err(McpError::invalid_params(
                            "Bitte `cql`, `text` oder `bereich` angeben.".to_string(),
                            None,
                        ))
                    }
                }
            }
        };

        let client = Self::client(&site)?;
        let auszug = client
            .liste(
                SUCHE,
                vec![
                    ("cql".to_string(), cql),
                    ("limit".to_string(), anzahl.to_string()),
                ],
                anzahl,
            )
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::suche(&auszug)))
    }

    async fn seite(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: SeiteParam = argumente(roh)?;
        let site = match self.waehlen(p.verbindung.as_deref()) {
            Ok(site) => site,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let client = Self::client(&site)?;

        let baum = match p.id.as_deref().map(str::trim).filter(|id| !id.is_empty()) {
            Some(id) => client
                .json(
                    &Anfrage::holen(format!("{API}/pages/{}", kennung(id)?))
                        .mit_abfrage("body-format", "storage"),
                )
                .await
                .map_err(fehler_zu_mcp)?,
            None => {
                let bereich = pflicht_opt(&p.bereich, "bereich")?;
                let titel = pflicht_opt(&p.titel, "titel")?;
                let (space_id, _) = Self::bereich_holen(&client, bereich)
                    .await
                    .map_err(fehler_zu_mcp)?;
                let treffer = client
                    .json(
                        &Anfrage::holen(format!("{API}/pages"))
                            .mit_abfrage("space-id", space_id)
                            .mit_abfrage("title", titel)
                            .mit_abfrage("body-format", "storage")
                            .mit_abfrage("limit", "2"),
                    )
                    .await
                    .map_err(fehler_zu_mcp)?;
                match daten::liste(&treffer, "results").first() {
                    Some(seite) => seite.clone(),
                    None => {
                        return Ok(antwort(format!(
                            "Keine Seite „{titel}“ im Bereich {bereich} gefunden."
                        )))
                    }
                }
            }
        };

        let bereich = Self::bereich_von_seite(&client, &baum)
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::seite(
            &baum,
            &site.url,
            Some(&bereich),
            p.als_storage.unwrap_or(false),
            zeichen(p.max_zeichen),
        )))
    }

    async fn unterseiten(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: ListeParam = argumente(roh)?;
        let site = match self.waehlen(p.verbindung.as_deref()) {
            Ok(site) => site,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let id = pflicht(&p.id, "id")?;
        let anzahl = zaehlen(p.anzahl);

        let client = Self::client(&site)?;
        let auszug = client
            .liste(
                &format!("{API}/pages/{}/direct-children", kennung(id)?),
                vec![("limit".to_string(), anzahl.to_string())],
                anzahl,
            )
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::unterseiten(id, &auszug)))
    }

    async fn kommentare(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: KommentareParam = argumente(roh)?;
        let site = match self.waehlen(p.verbindung.as_deref()) {
            Ok(site) => site,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let id = pflicht(&p.id, "id")?;
        let anzahl = zaehlen(p.anzahl);

        let client = Self::client(&site)?;
        let auszug = client
            .liste(
                &format!("{API}/pages/{}/footer-comments", kennung(id)?),
                vec![
                    ("body-format".to_string(), "storage".to_string()),
                    ("limit".to_string(), anzahl.to_string()),
                ],
                anzahl,
            )
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::kommentare(
            id,
            &auszug,
            zeichen(p.max_zeichen),
        )))
    }

    async fn anhaenge(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: ListeParam = argumente(roh)?;
        let site = match self.waehlen(p.verbindung.as_deref()) {
            Ok(site) => site,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let id = pflicht(&p.id, "id")?;
        let anzahl = zaehlen(p.anzahl);

        let client = Self::client(&site)?;
        let auszug = client
            .liste(
                &format!("{API}/pages/{}/attachments", kennung(id)?),
                vec![("limit".to_string(), anzahl.to_string())],
                anzahl,
            )
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::anhaenge(id, &auszug)))
    }

    async fn labels(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: ListeParam = argumente(roh)?;
        let site = match self.waehlen(p.verbindung.as_deref()) {
            Ok(site) => site,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let id = pflicht(&p.id, "id")?;
        let anzahl = zaehlen(p.anzahl);

        let client = Self::client(&site)?;
        let auszug = client
            .liste(
                &format!("{API}/pages/{}/labels", kennung(id)?),
                vec![("limit".to_string(), anzahl.to_string())],
                anzahl,
            )
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::labels(id, &auszug)))
    }

    // ---------------------------------------------------------- schreibend

    async fn seite_anlegen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: AnlegenParam = argumente(roh)?;
        let site = match self.schreiben_erlaubt(p.verbindung.as_deref()) {
            Ok(site) => site,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let bereich = pflicht_opt(&p.bereich, "bereich")?;
        let titel = pflicht(&p.titel, "titel")?;
        let inhalt = inhalt(&p.text, &p.storage)?;

        let client = Self::client(&site)?;
        let (space_id, schluessel) = Self::bereich_holen(&client, bereich)
            .await
            .map_err(fehler_zu_mcp)?;
        if let Err(ohne) = Self::bereich_erlaubt(&site, &schluessel) {
            return ohne.in_ergebnis();
        }

        let mut rumpf = json!({
            "spaceId": space_id,
            "status": "current",
            "title": titel,
            "body": { "representation": "storage", "value": inhalt },
        });
        if let Some(eltern) = p.eltern.as_deref().map(str::trim).filter(|e| !e.is_empty()) {
            // Eine Elternseite aus einem anderen Bereich gibt es in Confluence
            // nicht – erst lesen, dann (nicht) anlegen (Korrekturen).
            let eltern_anfrage = Anfrage::holen(format!(
                "{API}/pages/{}",
                client::pfad_wert(eltern).map_err(fehler_zu_mcp)?
            ));
            let eltern_seite = client.json(&eltern_anfrage).await.map_err(fehler_zu_mcp)?;
            let eltern_space = daten::text(&eltern_seite, "spaceId").ok_or_else(|| {
                McpError::internal_error(
                    format!(
                        "Die Elternseite {eltern} nennt keinen Bereich in der Antwort – es wurde \
                         nichts angelegt."
                    ),
                    None,
                )
            })?;
            if eltern_space != space_id {
                return Ok(antwort(format::eltern_fremd(
                    eltern,
                    &schluessel,
                    &eltern_space,
                )));
            }
            rumpf["parentId"] = json!(eltern);
        }

        let geantwortet = client
            .json(&Anfrage::post(format!("{API}/pages"), rumpf.to_string()))
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::seite_angelegt(&geantwortet, &site.url)))
    }

    async fn seite_aendern(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: AendernParam = argumente(roh)?;
        let site = match self.schreiben_erlaubt(p.verbindung.as_deref()) {
            Ok(site) => site,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let id = pflicht(&p.id, "id")?;
        let inhalt = inhalt(&p.text, &p.storage)?;
        // `version` ist eine ganze Zahl ab 1 – 0 oder ein Überlauf wären keine
        // echte Version (Korrekturen).
        if p.version < 1 {
            return Err(McpError::invalid_params(
                "`version` muss die gelesene Version der Seite sein (ganze Zahl ab 1).".to_string(),
                None,
            ));
        }
        // `status` nur laut Spezifikation: `current` oder `draft`.
        let status_gewuenscht = p
            .status
            .as_deref()
            .map(str::trim)
            .filter(|status| !status.is_empty());
        if let Some(status) = status_gewuenscht {
            if status != "current" && status != "draft" {
                return Err(McpError::invalid_params(
                    "`status` ist „current“ oder „draft“ – es wurde nichts geändert.".to_string(),
                    None,
                ));
            }
        }

        let client = Self::client(&site)?;
        let seite = client
            .json(
                &Anfrage::holen(format!("{API}/pages/{}", kennung(id)?))
                    .mit_abfrage("body-format", "storage"),
            )
            .await
            .map_err(fehler_zu_mcp)?;

        // Die gelesene Version ist Pflicht; weicht sie ab, wird nichts
        // überschrieben (Aufgabe 14).
        let server_version = daten::zahl(&seite, "version.number").ok_or_else(|| {
            McpError::internal_error(
                "Die Version der Seite fehlt in der Antwort – bitte `confluence_seite` neu lesen."
                    .to_string(),
                None,
            )
        })?;
        if server_version < 1 {
            return Err(McpError::internal_error(
                "Die Version der Seite ist keine Zahl ab 1 – es wurde nichts geändert.".to_string(),
                None,
            ));
        }
        let neue_version = server_version.checked_add(1).ok_or_else(|| {
            McpError::internal_error(
                "Die Version der Seite lässt sich nicht erhöhen – es wurde nichts geändert."
                    .to_string(),
                None,
            )
        })?;
        let passt = i64::try_from(p.version).map(|version| version == server_version);
        if passt != Ok(true) {
            return Ok(antwort(format::neu_lesen(id)));
        }

        if !site.nur.is_empty() {
            let schluessel = Self::bereich_von_seite(&client, &seite)
                .await
                .map_err(fehler_zu_mcp)?;
            if let Err(ohne) = Self::bereich_erlaubt(&site, &schluessel) {
                return ohne.in_ergebnis();
            }
        }

        let anhaengen = p.anhaengen.unwrap_or(false);
        let neu =
            match (anhaengen, daten::text(&seite, "body.storage.value")) {
                // Ohne den alten Inhalt würde ein Anhängen die Seite leeren –
                // darum kein PUT (Korrekturen).
                (true, None) => return Err(McpError::internal_error(
                    "Die Seite nennt keinen Inhalt in der Antwort – ein Anhängen würde den alten \
                     Inhalt ersetzen. Es wurde nichts geändert."
                        .to_string(),
                    None,
                )),
                // Der alte Inhalt bleibt bytegleich, der neue kommt danach.
                (true, Some(alt)) => format!("{alt}{inhalt}"),
                (false, _) => inhalt,
            };
        // Nie „(ohne Titel)“ senden: fehlt der Titel, wird nichts geschrieben.
        let titel = match p
            .titel
            .as_deref()
            .map(str::trim)
            .filter(|titel| !titel.is_empty())
        {
            Some(titel) => titel.to_string(),
            None => daten::text(&seite, "title").ok_or_else(|| {
                McpError::internal_error(
                    "Die Seite nennt keinen Titel in der Antwort – es wurde nichts geändert."
                        .to_string(),
                    None,
                )
            })?,
        };
        // Ohne ausdrückliche Angabe bleibt der Status: eine veröffentlichte
        // Seite wird nie unbeabsichtigt zum Entwurf.
        let status = match status_gewuenscht {
            Some(status) => status.to_string(),
            None => {
                let gelesen = daten::text(&seite, "status").ok_or_else(|| {
                    McpError::internal_error(
                        "Die Seite nennt keinen Status in der Antwort – es wurde nichts geändert."
                            .to_string(),
                        None,
                    )
                })?;
                if gelesen != "current" && gelesen != "draft" {
                    return Err(McpError::internal_error(
                        format!("Die Seite hat den Status „{gelesen}“ – es wurde nichts geändert."),
                        None,
                    ));
                }
                gelesen
            }
        };

        let rumpf = json!({
            "id": id,
            "status": status,
            "title": titel,
            "body": { "representation": "storage", "value": neu },
            "version": { "number": neue_version },
        });
        match client
            .json(&Anfrage::put(
                format!("{API}/pages/{}", kennung(id)?),
                rumpf.to_string(),
            ))
            .await
        {
            Ok(_) => Ok(antwort(format::seite_geaendert(
                id,
                &titel,
                neue_version,
                anhaengen,
            ))),
            // Der Server hat inzwischen eine andere Version: nicht überschreiben.
            Err(Fehler::Http { status: 409, .. }) => Ok(antwort(format::neu_lesen(id))),
            Err(fehler) => Err(fehler_zu_mcp(fehler)),
        }
    }

    async fn kommentieren(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: KommentierenParam = argumente(roh)?;
        let site = match self.schreiben_erlaubt(p.verbindung.as_deref()) {
            Ok(site) => site,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let id = pflicht(&p.id, "id")?;
        let inhalt = inhalt(&p.text, &p.storage)?;

        let client = Self::client(&site)?;
        let seite = client
            .json(&Anfrage::holen(format!("{API}/pages/{}", kennung(id)?)))
            .await
            .map_err(fehler_zu_mcp)?;
        if !site.nur.is_empty() {
            let schluessel = Self::bereich_von_seite(&client, &seite)
                .await
                .map_err(fehler_zu_mcp)?;
            if let Err(ohne) = Self::bereich_erlaubt(&site, &schluessel) {
                return ohne.in_ergebnis();
            }
        }

        let rumpf = json!({
            "pageId": id,
            "body": { "representation": "storage", "value": inhalt },
        });
        let geantwortet = client
            .json(&Anfrage::post(
                format!("{API}/footer-comments"),
                rumpf.to_string(),
            ))
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::kommentiert(id, &geantwortet)))
    }

    async fn label_hinzufuegen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: LabelParam = argumente(roh)?;
        let site = match self.schreiben_erlaubt(p.verbindung.as_deref()) {
            Ok(site) => site,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let id = pflicht(&p.id, "id")?;
        let label = pflicht(&p.label, "label")?;

        let client = Self::client(&site)?;
        let seite = client
            .json(&Anfrage::holen(format!("{API}/pages/{}", kennung(id)?)))
            .await
            .map_err(fehler_zu_mcp)?;
        if !site.nur.is_empty() {
            let schluessel = Self::bereich_von_seite(&client, &seite)
                .await
                .map_err(fehler_zu_mcp)?;
            if let Err(ohne) = Self::bereich_erlaubt(&site, &schluessel) {
                return ohne.in_ergebnis();
            }
        }

        // Labels schreibt nur die ältere API (`POST /wiki/rest/api/content/{id}/label`);
        // v2 kennt dort ausschließlich `GET` (Korrekturen).
        // Ein leerer Rumpf (204) ist Erfolg – darum `anfrage`, nicht `json`.
        client
            .anfrage(&Anfrage::post(
                format!("{LABEL}/{}/label", kennung(id)?),
                json!([{ "prefix": "global", "name": label }]).to_string(),
            ))
            .await
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format::label_angelegt(id, label)))
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

/// Ein veränderndes Werkzeug (`destructive_hint`, Aufgabe 14).
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

/// Ein Wert für den Pfad: kodiert, leer und Punkt-Segmente abgelehnt.
fn kennung(wert: &str) -> Result<String, McpError> {
    client::pfad_wert(wert).map_err(fehler_zu_mcp)
}

/// Wie [`pflicht`], aber für ein optionales Feld.
fn pflicht_opt<'a>(wert: &'a Option<String>, name: &str) -> Result<&'a str, McpError> {
    match wert.as_deref().map(str::trim).filter(|w| !w.is_empty()) {
        Some(wert) => Ok(wert),
        None => Err(McpError::invalid_params(
            format!("`{name}` darf nicht leer sein."),
            None,
        )),
    }
}

/// Der Inhalt einer Seite: genau eines von `text` und `storage`.
///
/// `text` wird **maskiert** ins Storage-Format gesetzt, `storage` unverändert
/// übernommen (Aufgabe 14).
fn inhalt(text: &Option<String>, storage: &Option<String>) -> Result<String, McpError> {
    let text = text.as_deref().filter(|text| !text.trim().is_empty());
    let storage = storage
        .as_deref()
        .filter(|storage| !storage.trim().is_empty());
    match (text, storage) {
        (Some(_), Some(_)) => Err(McpError::invalid_params(
            "Bitte entweder `text` oder `storage` angeben, nicht beides.".to_string(),
            None,
        )),
        (Some(text), None) => Ok(html::aus_text(text)),
        (None, Some(storage)) => Ok(storage.to_string()),
        (None, None) => Err(McpError::invalid_params(
            "Bitte `text` (wird maskiert) oder `storage` (unverändert) angeben.".to_string(),
            None,
        )),
    }
}

/// Eine Zahl aus einem optionalen Feld, mit Vorgabe und Grenzen.
fn zaehlen(wert: Option<u32>) -> usize {
    wert.unwrap_or(format::ANZAHL as u32)
        .clamp(1, format::MAX_ANZAHL as u32) as usize
}

/// Zeichen für einen Text: Standard 4000, höchstens 100 000.
fn zeichen(wert: Option<u32>) -> usize {
    wert.unwrap_or(format::MAX_ZEICHEN as u32)
        .clamp(100, 100_000) as usize
}

/// Eine kommagetrennte Liste von Schlüsseln, ohne Leerraum.
fn schluessel_liste(wert: Option<&str>) -> Vec<String> {
    wert.map(|wert| {
        wert.split(',')
            .map(str::trim)
            .filter(|teil| !teil.is_empty())
            .map(str::to_string)
            .collect()
    })
    .unwrap_or_default()
}

/// Aus einem Confluence-Fehler wird ein MCP-Fehler – mit Site, nie mit Token.
fn fehler_zu_mcp(fehler: Fehler) -> McpError {
    let text = fehler.to_string();
    match fehler {
        Fehler::Anmeldung { .. }
        | Fehler::Rechte { .. }
        | Fehler::Http { .. }
        | Fehler::FremderHost { .. }
        | Fehler::Adresse { .. }
        | Fehler::Pfad { .. } => McpError::invalid_params(text, None),
        _ => McpError::internal_error(text, None),
    }
}

// ------------------------------------------------------------- Parameter

/// Parameter von `confluence_status`, `confluence_testen` und `confluence_profile`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct VerbindungParam {
    /// Site; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
}

/// Parameter von `confluence_bereiche`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct BereicheParam {
    /// Site; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Nur diese Bereichsschlüssel, kommagetrennt (z. B. `ENG,OPS`).
    #[serde(default)]
    pub schluessel: Option<String>,
    /// Höchstens so viele Bereiche (Standard 25, höchstens 100).
    #[serde(default)]
    pub anzahl: Option<u32>,
}

/// Parameter von `confluence_suchen`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct SucheParam {
    /// Site; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Fertige CQL-Abfrage. Ersetzt `text` und `bereich`.
    #[serde(default)]
    pub cql: Option<String>,
    /// Freitext im Inhalt (`text ~ "…"`).
    #[serde(default)]
    pub text: Option<String>,
    /// Bereichsschlüssel (`space = "…"`).
    #[serde(default)]
    pub bereich: Option<String>,
    /// Höchstens so viele Treffer (Standard 25, höchstens 100).
    #[serde(default)]
    pub anzahl: Option<u32>,
}

/// Parameter von `confluence_seite`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct SeiteParam {
    /// Site; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Seiten-Id. Ohne Id werden `bereich` und `titel` gebraucht.
    #[serde(default)]
    pub id: Option<String>,
    /// Bereichsschlüssel (z. B. `ENG`) für die Suche über den Titel.
    #[serde(default)]
    pub bereich: Option<String>,
    /// Titel der Seite für die Suche über `bereich`.
    #[serde(default)]
    pub titel: Option<String>,
    /// Das Storage-Format unverändert zeigen statt als Text.
    #[serde(default)]
    pub als_storage: Option<bool>,
    /// Höchstens so viele Zeichen Inhalt (Standard 4000).
    #[serde(default)]
    pub max_zeichen: Option<u32>,
}

/// Parameter von `confluence_unterseiten`, `confluence_anhaenge` und
/// `confluence_labels`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ListeParam {
    /// Site; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Seiten-Id.
    pub id: String,
    /// Höchstens so viele Einträge (Standard 25, höchstens 100).
    #[serde(default)]
    pub anzahl: Option<u32>,
}

/// Parameter von `confluence_kommentare`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct KommentareParam {
    /// Site; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Seiten-Id.
    pub id: String,
    /// Höchstens so viele Kommentare (Standard 25, höchstens 100).
    #[serde(default)]
    pub anzahl: Option<u32>,
    /// Höchstens so viele Zeichen je Kommentar (Standard 4000).
    #[serde(default)]
    pub max_zeichen: Option<u32>,
}

/// Parameter von `confluence_seite_anlegen`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct AnlegenParam {
    /// Site; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Bereichsschlüssel, in dem die Seite entsteht (Pflicht).
    #[serde(default)]
    pub bereich: Option<String>,
    /// Titel der neuen Seite.
    pub titel: String,
    /// Inhalt als Text; wird maskiert ins Storage-Format gesetzt.
    #[serde(default)]
    pub text: Option<String>,
    /// Inhalt als Storage-Format; wird unverändert übernommen.
    #[serde(default)]
    pub storage: Option<String>,
    /// Elternseite, unter der die Seite hängt.
    #[serde(default)]
    pub eltern: Option<String>,
}

/// Parameter von `confluence_seite_aendern`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct AendernParam {
    /// Site; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Seiten-Id.
    pub id: String,
    /// Die zuletzt gelesene Version der Seite (Pflicht, z. B. aus `confluence_seite`).
    pub version: u64,
    /// Neuer Titel; ohne Angabe bleibt der Titel.
    #[serde(default)]
    pub titel: Option<String>,
    /// Neuer Inhalt als Text; wird maskiert ins Storage-Format gesetzt.
    #[serde(default)]
    pub text: Option<String>,
    /// Neuer Inhalt als Storage-Format; wird unverändert übernommen.
    #[serde(default)]
    pub storage: Option<String>,
    /// An den vorhandenen Inhalt anhängen statt ihn zu ersetzen.
    #[serde(default)]
    pub anhaengen: Option<bool>,
    /// Neuer Status (`current` oder `draft`); ohne Angabe bleibt der Status.
    #[serde(default)]
    pub status: Option<String>,
}

/// Parameter von `confluence_kommentieren`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct KommentierenParam {
    /// Site; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Seiten-Id.
    pub id: String,
    /// Kommentar als Text; wird maskiert ins Storage-Format gesetzt.
    #[serde(default)]
    pub text: Option<String>,
    /// Kommentar als Storage-Format; wird unverändert übernommen.
    #[serde(default)]
    pub storage: Option<String>,
}

/// Parameter von `confluence_label_hinzufuegen`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct LabelParam {
    /// Site; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Seiten-Id.
    pub id: String,
    /// Das Label (ohne Präfix, es wird `global`).
    pub label: String,
}

// -------------------------------------------------------------- MCP-Anker

impl ServerHandler for ConfluenceServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(quiver_tools_kern::mcp::server_info(&self.zugang))
            .with_instructions(format!(
                "Confluence Cloud über die REST-API v2 (Suche über CQL in der älteren API): \
                 Bereiche, Seiten, Unterseiten, Kommentare, Anhänge und Labels lesen, Seiten und \
                 Kommentare suchen. Verändernd gehen Seite anlegen und ändern, Kommentar und Label \
                 schreiben – aber nur, wenn der Schalter „schreiben“ für die Site an ist \
                 (Standard: aus). Ist er aus, antworten die verändernden Werkzeuge mit dem Hinweis \
                 und ohne Anfrage an Confluence. Beim Ändern ist `version` Pflicht: weicht sie ab \
                 oder meldet der Server 409, wird nichts überschrieben. Das Feld „nur“ begrenzt \
                 das Schreiben auf bestimmte Bereiche; den Schlüssel liest das Werkzeug vom \
                 Server. Die {MEHRFACH_TITEL} ({MEHRFACH_EINZAHL}s) legt der User im quiver-Fenster \
                 unter Tools → {TITEL} an; jedes Werkzeug nimmt `verbindung`, bei genau einer darf \
                 der Name fehlen. Fehlt die Liste, sagen die Werkzeuge das als Text."
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
    use crate::werkzeug::Confluence;
    use quiver_tools_kern::zugaenge::Zugang as Eintrag;
    use serde_json::json;

    fn eintrag(name: &str, nur: Option<&str>, schreiben: bool) -> Eintrag {
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

    fn server_mit(eintraege: Vec<Eintrag>) -> ConfluenceServer {
        ConfluenceServer::mit(
            Zugang::mit_umgebung(&Confluence, |_| None),
            Ok(Sammlung::aus_zugaengen(eintraege).unwrap()),
        )
    }

    fn ohne_sites() -> ConfluenceServer {
        ConfluenceServer::mit(
            Zugang::mit_umgebung(&Confluence, |_| None),
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

    fn aufruf(server: &ConfluenceServer, name: &str, argumente: Value) -> Ergebnis {
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
        assert_eq!(werkzeuge.len(), 14, "13 Werkzeuge plus confluence_profile");

        let schreibend = [
            "confluence_seite_anlegen",
            "confluence_seite_aendern",
            "confluence_kommentieren",
            "confluence_label_hinzufuegen",
        ];

        for werkzeug in &werkzeuge {
            let hinweise = werkzeug.annotations.as_ref().expect("Hinweise fehlen");
            let name = werkzeug.name.as_ref();
            let destruktiv = schreibend.contains(&name);
            assert_eq!(hinweise.read_only_hint, Some(!destruktiv), "{name}");
            assert_eq!(hinweise.destructive_hint, Some(destruktiv), "{name}");
            if name == "confluence_profile" {
                assert!(hinweise.title.is_some(), "{name} hat keinen Titel");
            } else {
                assert!(werkzeug.title.is_some(), "{name} hat keinen Titel");
            }
            // Jedes eigene Werkzeug kennt `verbindung`.
            let eigenschaften = &werkzeug.input_schema["properties"];
            if name != "confluence_profile" {
                assert!(
                    eigenschaften.get("verbindung").is_some(),
                    "{name} kennt kein `verbindung`: {eigenschaften:?}"
                );
            }
            // Jedes Schema ist ein Objekt – sonst lehnen manche Clients die
            // ganze Liste ab.
            assert_eq!(werkzeug.input_schema["type"], json!("object"), "{name}");
        }

        // Die Namen stehen in einer festen Reihenfolge: erst lesend, dann
        // verändernd, zuletzt das Profil.
        let namen: Vec<&str> = werkzeuge.iter().map(|w| w.name.as_ref()).collect();
        assert_eq!(namen[0], "confluence_status");
        assert_eq!(namen[8], "confluence_labels");
        assert_eq!(namen[9], "confluence_seite_anlegen");
        assert_eq!(namen[12], "confluence_label_hinzufuegen");
        assert_eq!(namen[13], "confluence_profile");
    }

    #[test]
    fn ohne_sites_kommt_der_hinweis_und_kein_fehler() {
        let server = ohne_sites();
        for (name, argumente) in [
            ("confluence_status", json!({})),
            ("confluence_testen", json!({})),
            ("confluence_bereiche", json!({})),
            ("confluence_suchen", json!({ "text": "Anmeldung" })),
            ("confluence_seite", json!({ "id": "123" })),
            ("confluence_labels", json!({ "id": "123" })),
            (
                "confluence_seite_anlegen",
                json!({ "bereich": "ENG", "titel": "x", "text": "y" }),
            ),
        ] {
            let text = text_von(aufruf(&server, name, argumente));
            assert!(text.contains("Zugang fehlt"), "{name}: {text}");
            assert!(text.contains("Tools → Confluence"), "{name}: {text}");
        }
    }

    #[test]
    fn das_profil_nennt_die_sites_ohne_token_und_ohne_email() {
        let server = server_mit(vec![eintrag("firma", Some("ENG"), false)]);
        let text = text_von(aufruf(&server, "confluence_profile", json!({})));
        assert!(text.contains("firma"), "{text}");
        assert!(text.contains("Bereiche ENG"), "{text}");
        assert!(text.contains("Schreiben aus"), "{text}");
        assert!(!text.contains("token-4711"), "{text}");
        assert!(!text.contains("alex@example.test"), "{text}");
    }

    #[test]
    fn mehrere_sites_verlangen_einen_namen() {
        let server = server_mit(vec![
            eintrag("firma", None, true),
            eintrag("kunde", None, false),
        ]);
        let text = text_von(aufruf(&server, "confluence_status", json!({})));
        assert!(text.contains("firma"), "{text}");
        assert!(text.contains("kunde"), "{text}");
        assert!(!text.contains("token-4711"), "{text}");

        let fehler = aufruf(&server, "confluence_bereiche", json!({})).unwrap_err();
        assert!(
            fehler.message.contains("mehrere Sites"),
            "{}",
            fehler.message
        );

        let fehler = aufruf(
            &server,
            "confluence_bereiche",
            json!({ "verbindung": "gibtsnicht" }),
        )
        .unwrap_err();
        assert!(fehler.message.contains("gibtsnicht"), "{}", fehler.message);
    }

    #[test]
    fn schreiben_aus_antwortet_ohne_anfrage() {
        let server = server_mit(vec![eintrag("firma", None, false)]);
        for (name, argumente) in [
            (
                "confluence_seite_anlegen",
                json!({ "bereich": "ENG", "titel": "Neu", "text": "x" }),
            ),
            (
                "confluence_seite_aendern",
                json!({ "id": "123", "version": 3, "text": "x" }),
            ),
            (
                "confluence_kommentieren",
                json!({ "id": "123", "text": "hallo" }),
            ),
            (
                "confluence_label_hinzufuegen",
                json!({ "id": "123", "label": "anmeldung" }),
            ),
        ] {
            let text = text_von(aufruf(&server, name, argumente));
            assert!(
                text.contains("Schreiben ist für firma aus"),
                "{name}: {text}"
            );
            assert!(
                text.contains("im quiver-Fenster einschalten"),
                "{name}: {text}"
            );
        }
    }

    #[test]
    fn leere_pflichtparameter_und_unbekannte_werkzeuge_sind_parameterfehler() {
        let server = server_mit(vec![eintrag("firma", None, true)]);

        let fehler = aufruf(
            &server,
            "confluence_seite_anlegen",
            json!({ "bereich": "  ", "titel": "x", "text": "y" }),
        )
        .unwrap_err();
        assert!(fehler.message.contains("bereich"), "{}", fehler.message);

        let fehler = aufruf(&server, "confluence_gibtsnicht", json!({})).unwrap_err();
        assert!(
            fehler.message.contains("confluence_gibtsnicht"),
            "{}",
            fehler.message
        );

        let fehler =
            aufruf(&server, "confluence_seite", json!({ "als_storage": true })).unwrap_err();
        assert!(fehler.message.contains("bereich"), "{}", fehler.message);

        let fehler = aufruf(&server, "confluence_kommentare", json!({ "id": 12 })).unwrap_err();
        assert!(fehler.message.contains("Parameter"), "{}", fehler.message);
    }

    #[test]
    fn inhalt_ist_genau_eines_von_text_und_storage() {
        assert_eq!(
            inhalt(&Some("Hallo & Welt".into()), &None).unwrap(),
            "<p>Hallo &amp; Welt</p>"
        );
        assert_eq!(
            inhalt(&None, &Some("<p>roh</p>".into())).unwrap(),
            "<p>roh</p>"
        );
        // Beides: Fehler; nichts: Fehler; Leerraum zählt als fehlend.
        assert!(inhalt(&Some("x".into()), &Some("y".into())).is_err());
        assert!(inhalt(&None, &None).is_err());
        assert!(inhalt(&Some("   ".into()), &None).is_err());
        assert_eq!(
            inhalt(&Some("  y  ".into()), &None).unwrap(),
            "<p>  y  </p>"
        );
    }

    #[test]
    fn die_helfer_rechnen_richtig() {
        assert_eq!(zaehlen(None), format::ANZAHL);
        assert_eq!(zaehlen(Some(0)), 1);
        assert_eq!(zaehlen(Some(10)), 10);
        assert_eq!(zaehlen(Some(5000)), format::MAX_ANZAHL);
        assert_eq!(zeichen(None), format::MAX_ZEICHEN);
        assert_eq!(zeichen(Some(5000)), 5000);
        assert_eq!(zeichen(Some(1)), 100);
        assert_eq!(
            schluessel_liste(Some(" ENG , ops ")),
            vec!["ENG".to_string(), "ops".to_string()]
        );
        assert!(schluessel_liste(None).is_empty());
        assert!(schluessel_liste(Some(" , ")).is_empty());
    }

    #[test]
    fn eine_kaputte_site_liste_ist_ein_parameterfehler() {
        let server = ConfluenceServer::mit(
            Zugang::mit_umgebung(&Confluence, |_| None),
            Sammlung::mit_umgebung(|_| Some("[{\"name\":".to_string())),
        );
        let fehler = aufruf(&server, "confluence_status", json!({})).unwrap_err();
        assert!(
            fehler.message.contains("QUIVER_ZUGAENGE"),
            "{}",
            fehler.message
        );
    }
}
