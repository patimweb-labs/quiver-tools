// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der MCP-Server: 14 Werkzeuge über stdio (SPEC §6).
//!
//! Es gibt genau einen Zugang (aus der Umgebung, SPEC §2); jedes Werkzeug
//! antwortet ohne Zugang mit dem Hinweistext des Kerns – **kein** MCP-Fehler.
//! Fehler von Outline werden zu MCP-Fehlern mit Status und Text, nie mit dem
//! API-Schlüssel.

use std::any::Any;
use std::collections::BTreeSet;
use std::path::Path;

use quiver_tools_kern::mcp::{profile_antwort, profile_tool};
use quiver_tools_kern::{antwort, zugang, OhneProfil, Profil, Zugang};
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
use serde_json::{json, Map, Value};

use crate::client::{Client, OutlineFehler, Seiten};
use crate::domain::{baum_knoten, Dokument, Knoten, Kommentar, Sammlung, Treffer, Verschoben};
use crate::{datei, format};

/// Was ein Werkzeug zurückgibt.
type Ergebnis = Result<CallToolResult, McpError>;

/// Der Outline-Server: der Zugang und die Werkzeugliste.
pub struct OutlineServer {
    zugang: Zugang,
}

impl OutlineServer {
    /// Ein Server über dem Zugang eines Werkzeugs.
    pub fn neu(zugang: Zugang) -> Self {
        OutlineServer { zugang }
    }

    /// Alle Werkzeuge, wie `tools/list` sie meldet.
    ///
    /// `outline_profile` kommt aus dem Kern: es ist bei jedem Tool dasselbe
    /// lesende Werkzeug (SPEC §2).
    pub fn werkzeuge(&self) -> Vec<Tool> {
        vec![
            lese_werkzeug::<SammlungenParam>(
                "outline_collections",
                "Sammlungen",
                "Listet die Sammlungen (oberste Abschnitte) des Outline mit ihren ids. Der Einstieg, wenn eine Sammlungs-id für outline_structure, outline_list oder outline_create gebraucht wird.",
            ),
            lese_werkzeug::<StrukturParam>(
                "outline_structure",
                "Dokumentbaum",
                "Zeigt den verschachtelten Dokumentbaum einer Sammlung (Titel und ids, Eltern und Kinder). Zeigt, wie ein Wiki-Abschnitt aufgebaut ist, bevor eine Seite angelegt oder verschoben wird.",
            ),
            lese_werkzeug::<SuchParam>(
                "outline_search",
                "Suche",
                "Volltextsuche im Outline-Wiki: Treffer mit Textausschnitt und id, den ganzen Text holt outline_read. Zwei oder drei eigentümliche Wörter suchen besser als ein ganzer Satz.",
            ),
            lese_werkzeug::<ListenParam>(
                "outline_list",
                "Dokumente auflisten",
                "Listet Dokumente ohne Suchbegriff – standardmäßig die zuletzt geänderten. Filter nach Sammlung, nach Elterndokument oder die eigenen Entwürfe. Für die gezielte Suche outline_search nehmen.",
            ),
            lese_werkzeug::<LeseParam>(
                "outline_read",
                "Seite lesen",
                "Liest den Markdown-Rumpf einer Seite über ihre id (aus outline_search, outline_list oder outline_structure). Optional die Kindseiten auflisten oder die Seite als Markdown-Datei ablegen.",
            ),
            lese_werkzeug::<KommentareParam>(
                "outline_comments",
                "Kommentare lesen",
                "Liest die Kommentare einer Seite, Antworten unter ihrem Elternkommentar. Nützlich für offene Fragen und Rückmeldungen, die nie in den Seitentext gewandert sind.",
            ),
            schreib_werkzeug::<ErstellenParam>(
                "outline_create",
                "Seite anlegen",
                "Legt eine neue Seite aus Markdown an – in einer Sammlung oder unter einer bestehenden Seite. Vorher suchen: eine vorhandene Seite zu ändern ist meist besser als eine zweite, fast gleiche anzulegen.",
            ),
            schreib_werkzeug::<AendernParam>(
                "outline_update",
                "Seite ändern",
                "Ändert eine Seite: Rumpf ersetzen, einen Abschnitt anhängen oder voranstellen, umbenennen, einen Entwurf veröffentlichen. Beim Ersetzen die Seite vorher lesen, damit nichts verloren geht.",
            ),
            schreib_werkzeug::<VerschiebenParam>(
                "outline_move",
                "Seite verschieben",
                "Verschiebt eine Seite in eine andere Sammlung oder unter eine andere Elternseite; Kindseiten wandern mit. `to_top_level` hebt eine verschachtelte Seite an den Anfang ihrer Sammlung.",
            ),
            schreib_werkzeug::<ArchivParam>(
                "outline_archive",
                "Seite archivieren",
                "Archiviert eine Seite (umkehrbar, verschwindet aus der Sammlung), holt eine archivierte oder gelöschte Seite zurück oder macht aus einer Seite wieder einen Entwurf. Löschen nur, wenn die Seite wirklich weg soll.",
            ),
            schreib_werkzeug::<LoeschenParam>(
                "outline_delete",
                "Seite löschen",
                "Legt eine Seite in den Papierkorb, von wo outline_archive sie zurückholt. Mit `permanent: true` wird sie endgültig gelöscht – nur, wenn das ausdrücklich verlangt ist. Archivieren ist meist die bessere Wahl.",
            ),
            schreib_werkzeug::<KommentarParam>(
                "outline_comment",
                "Kommentar schreiben",
                "Schreibt einen Kommentar an eine Seite oder antwortet in einem Kommentarstrang. Für eine Frage oder einen Hinweis an die Menschen, denen die Seite gehört – statt sie still zu ändern.",
            ),
            schreib_werkzeug::<ExportParam>(
                "outline_export",
                "Seiten exportieren",
                "Schreibt eine Seite (optional mit Kindern) oder eine ganze Sammlung als Markdown-Dateien in einen Ordner. Holt Wiki-Inhalte in ein Repository, ohne jede Seite durch das Modell zu schicken.",
            ),
            profile_tool(&self.zugang),
        ]
    }

    /// Ein Werkzeug aufrufen. Auch der Weg, den die Tests nehmen.
    pub async fn aufrufen(&self, name: &str, argumente: Option<JsonObject>) -> Ergebnis {
        match name {
            "outline_collections" => self.sammlungen(argumente).await,
            "outline_structure" => self.struktur(argumente).await,
            "outline_search" => self.suche(argumente).await,
            "outline_list" => self.liste(argumente).await,
            "outline_read" => self.lesen(argumente).await,
            "outline_comments" => self.kommentare(argumente).await,
            "outline_create" => self.erstellen(argumente).await,
            "outline_update" => self.aendern(argumente).await,
            "outline_move" => self.verschieben(argumente).await,
            "outline_archive" => self.archivieren(argumente).await,
            "outline_delete" => self.loeschen(argumente).await,
            "outline_comment" => self.kommentieren(argumente).await,
            "outline_export" => self.exportieren(argumente).await,
            "outline_profile" => profile_antwort(&self.zugang),
            sonst => Err(McpError::invalid_params(
                format!("Unbekanntes Werkzeug „{sonst}“."),
                None,
            )),
        }
    }

    /// Der Zugang oder die fertige Antwort (Hinweis oder Fehler).
    fn hole_profil(&self) -> Result<Profil, OhneProfil> {
        zugang(self.zugang.titel(), self.zugang.profil())
    }

    // ------------------------------------------------------------- lesend

    async fn sammlungen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: SammlungenParam = argumente(roh)?;
        let profil = match self.hole_profil() {
            Ok(profil) => profil,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let ergebnis = client(&profil)?
            .aufrufen::<Vec<Sammlung>>(
                "collections.list",
                json!({
                    "limit": p.limit.unwrap_or(50),
                    "offset": p.offset.unwrap_or(0),
                }),
            )
            .await
            .map_err(mcp_fehler)?;
        Ok(mit_weiteren(
            format::sammlungen(&ergebnis.daten),
            ergebnis.seiten.as_ref(),
            ergebnis.daten.len(),
        ))
    }

    async fn struktur(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: StrukturParam = argumente(roh)?;
        if p.collection_id.trim().is_empty() {
            return Err(parameter_fehler(
                "outline_structure braucht eine collection_id.",
            ));
        }
        let profil = match self.hole_profil() {
            Ok(profil) => profil,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let client = client(&profil)?;
        let baum = client
            .aufrufen::<Vec<Knoten>>(
                "collections.documents",
                json!({ "id": p.collection_id.trim() }),
            )
            .await
            .map_err(mcp_fehler)?;

        // Der Baum-Endpunkt nennt die Sammlung nicht; der Name ist Kosmetik und
        // darf den Aufruf nie scheitern lassen.
        let name = match client
            .aufrufen::<Vec<Sammlung>>("collections.list", json!({ "limit": 100 }))
            .await
        {
            Ok(sammlungen) => sammlungen
                .daten
                .into_iter()
                .find(|sammlung| sammlung.id == p.collection_id.trim())
                .map(|sammlung| sammlung.name),
            Err(_) => None,
        };
        Ok(antwort(format::struktur(&baum.daten, name.as_deref())))
    }

    async fn suche(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: SuchParam = argumente(roh)?;
        let suchbegriff = p.query.trim().to_string();
        if suchbegriff.is_empty() {
            return Err(parameter_fehler(
                "Die Suche braucht einen Suchbegriff (`query`).",
            ));
        }
        if let Some(filter) = &p.status_filter {
            let ungueltig: Vec<&str> = filter
                .iter()
                .map(String::as_str)
                .filter(|wert| !["draft", "archived", "published"].contains(wert))
                .collect();
            if !ungueltig.is_empty() {
                return Err(parameter_fehler(format!(
                    "Unzulässiger status_filter: {}. Erlaubt: draft, archived, published.",
                    ungueltig.join(", ")
                )));
            }
        }
        if let Some(filter) = p.date_filter.as_deref() {
            if !["day", "week", "month", "year"].contains(&filter) {
                return Err(parameter_fehler(format!(
                    "Unzulässiger date_filter „{filter}“. Erlaubt: day, week, month, year."
                )));
            }
        }

        let profil = match self.hole_profil() {
            Ok(profil) => profil,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let ergebnis = client(&profil)?
            .aufrufen::<Vec<Treffer>>("documents.search", suchkriterien(&p, &suchbegriff))
            .await
            .map_err(mcp_fehler)?;
        Ok(antwort(format::suche(
            &ergebnis.daten,
            &suchbegriff,
            ergebnis.seiten.as_ref(),
        )))
    }

    async fn liste(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: ListenParam = argumente(roh)?;
        let sortierung = p.sort.clone().unwrap_or_else(|| "updatedAt".to_string());
        if !["updatedAt", "createdAt", "title", "index"].contains(&sortierung.as_str()) {
            return Err(parameter_fehler(format!(
                "Unzulässige sortierung „{sortierung}“. Erlaubt: updatedAt, createdAt, title, index."
            )));
        }
        let richtung = p
            .direction
            .clone()
            .unwrap_or_else(|| "DESC".to_string())
            .to_uppercase();
        if richtung != "ASC" && richtung != "DESC" {
            return Err(parameter_fehler(format!(
                "Unzulässige Richtung „{richtung}“. Erlaubt: ASC, DESC."
            )));
        }

        let profil = match self.hole_profil() {
            Ok(profil) => profil,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let entwuerfe = p.drafts.unwrap_or(false);
        let methode = if entwuerfe {
            "documents.drafts"
        } else {
            "documents.list"
        };
        let rumpf = rumpf(vec![
            ("limit", Some(json!(p.limit.unwrap_or(25)))),
            ("offset", Some(json!(p.offset.unwrap_or(0)))),
            ("sort", Some(json!(sortierung))),
            ("direction", Some(json!(richtung))),
            (
                "collectionId",
                (!entwuerfe)
                    .then(|| p.collection_id.clone())
                    .flatten()
                    .map(|wert| json!(wert)),
            ),
            (
                "parentDocumentId",
                (!entwuerfe)
                    .then(|| p.parent_document_id.clone())
                    .flatten()
                    .map(|wert| json!(wert)),
            ),
        ]);
        let ueberschrift = if entwuerfe {
            "Eigene Entwürfe".to_string()
        } else if let Some(eltern) = p.parent_document_id.as_deref() {
            format!("Kinder von {eltern}")
        } else if let Some(sammlung) = p.collection_id.as_deref() {
            format!("Dokumente in Sammlung {sammlung}")
        } else {
            "Zuletzt geänderte Dokumente".to_string()
        };

        let ergebnis = client(&profil)?
            .aufrufen::<Vec<Dokument>>(methode, rumpf)
            .await
            .map_err(mcp_fehler)?;
        Ok(antwort(format::liste(
            &ergebnis.daten,
            &ueberschrift,
            ergebnis.seiten.as_ref(),
        )))
    }

    async fn lesen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: LeseParam = argumente(roh)?;
        if p.id.trim().is_empty() {
            return Err(parameter_fehler("outline_read braucht eine id."));
        }
        let profil = match self.hole_profil() {
            Ok(profil) => profil,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let client = client(&profil)?;
        let seite = client
            .aufrufen::<Dokument>("documents.info", json!({ "id": p.id.trim() }))
            .await
            .map_err(mcp_fehler)?
            .daten;

        let mut teile = vec![format::dokument(
            &seite,
            p.max_zeichen.unwrap_or(8000) as usize,
        )];

        if let Some(ordner) = p.save_dir.as_deref() {
            let dateiname = format!("{}.md", datei::slug(seite.titel()));
            let markdown = format!("# {}\n\n{}\n", seite.titel(), seite.text);
            let pfad = datei::schreiben(Path::new(ordner), &dateiname, &markdown)
                .map_err(|fehler| McpError::internal_error(fehler.to_string(), None))?;
            teile.push(format!("Vollständig gespeichert: {}", pfad.display()));
        }

        if p.include_children.unwrap_or(false) {
            let kinder = client
                .aufrufen::<Vec<Dokument>>(
                    "documents.list",
                    json!({ "parentDocumentId": seite.kennung(), "limit": 50 }),
                )
                .await
                .map_err(mcp_fehler)?;
            teile.push(format::liste(
                &kinder.daten,
                "Kindseiten",
                kinder.seiten.as_ref(),
            ));
        }

        Ok(antwort(teile.join("\n\n")))
    }

    async fn kommentare(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: KommentareParam = argumente(roh)?;
        if p.document_id.trim().is_empty() {
            return Err(parameter_fehler(
                "outline_comments braucht eine document_id.",
            ));
        }
        let profil = match self.hole_profil() {
            Ok(profil) => profil,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let ergebnis = client(&profil)?
            .aufrufen::<Vec<Kommentar>>(
                "comments.list",
                json!({
                    "documentId": p.document_id.trim(),
                    "limit": p.limit.unwrap_or(50),
                    "offset": p.offset.unwrap_or(0),
                }),
            )
            .await
            .map_err(mcp_fehler)?;
        Ok(mit_weiteren(
            format::kommentare(&ergebnis.daten, p.document_id.trim()),
            ergebnis.seiten.as_ref(),
            ergebnis.daten.len(),
        ))
    }

    // ----------------------------------------------------------- schreibend

    async fn erstellen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: ErstellenParam = argumente(roh)?;
        if p.title.trim().is_empty() {
            return Err(parameter_fehler("outline_create braucht einen Titel."));
        }
        let veroeffentlichen = p.publish.unwrap_or(true);
        let hat_ziel = p.collection_id.is_some() || p.parent_document_id.is_some();
        if veroeffentlichen && !hat_ziel {
            return Err(parameter_fehler(
                "Eine veröffentlichte Seite braucht eine collection_id oder eine parent_document_id. Sammlungen mit outline_collections finden – oder `publish: false` für einen Entwurf setzen.",
            ));
        }

        let profil = match self.hole_profil() {
            Ok(profil) => profil,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let rumpf = rumpf(vec![
            ("title", Some(json!(p.title.trim()))),
            ("text", Some(json!(p.text.unwrap_or_default()))),
            ("collectionId", p.collection_id.map(|wert| json!(wert))),
            (
                "parentDocumentId",
                p.parent_document_id.map(|wert| json!(wert)),
            ),
            ("templateId", p.template_id.map(|wert| json!(wert))),
            ("icon", p.icon.map(|wert| json!(wert))),
            ("publish", Some(json!(veroeffentlichen))),
        ]);
        let seite = client(&profil)?
            .aufrufen::<Dokument>("documents.create", rumpf)
            .await
            .map_err(mcp_fehler)?
            .daten;
        Ok(antwort(format::gespeichert(&seite, "erstellt")))
    }

    async fn aendern(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: AendernParam = argumente(roh)?;
        if p.id.trim().is_empty() {
            return Err(parameter_fehler("outline_update braucht eine id."));
        }
        let modus = p
            .mode
            .clone()
            .unwrap_or_else(|| "replace".to_string())
            .to_lowercase();
        if !["replace", "append", "prepend"].contains(&modus.as_str()) {
            return Err(parameter_fehler(format!(
                "Unzulässiger Modus „{modus}“. Erlaubt: replace (Standard), append, prepend."
            )));
        }
        if p.text.is_none() && p.title.is_none() && p.icon.is_none() && p.publish.is_none() {
            return Err(parameter_fehler(
                "Nichts zu ändern: mindestens eines von text, title, icon oder publish angeben.",
            ));
        }

        let profil = match self.hole_profil() {
            Ok(profil) => profil,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let client = client(&profil)?;
        let id = p.id.trim();

        // Anhängen und Voranstellen brauchen den bestehenden Text; so muss der
        // Agent die Seite nicht erst lesen und wieder mitschicken.
        let text = match (p.text, modus.as_str()) {
            (Some(text), "append") => {
                let bestand = bestand(&client, id).await?;
                Some(format!("{}\n\n{}\n", bestand.trim_end(), text))
            }
            (Some(text), "prepend") => {
                let bestand = bestand(&client, id).await?;
                Some(format!("{text}\n\n{}\n", bestand.trim_start()))
            }
            (text, _) => text,
        };

        let rumpf = rumpf(vec![
            ("id", Some(json!(id))),
            ("text", text.map(|wert| json!(wert))),
            ("title", p.title.map(|wert| json!(wert))),
            ("icon", p.icon.map(|wert| json!(wert))),
            ("publish", p.publish.map(|wert| json!(wert))),
        ]);
        let seite = client
            .aufrufen::<Dokument>("documents.update", rumpf)
            .await
            .map_err(mcp_fehler)?
            .daten;
        Ok(antwort(format!(
            "{}\nModus: {modus}",
            format::gespeichert(&seite, "geändert")
        )))
    }

    async fn verschieben(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: VerschiebenParam = argumente(roh)?;
        if p.id.trim().is_empty() {
            return Err(parameter_fehler("outline_move braucht eine id."));
        }
        let an_die_wurzel = p.to_top_level.unwrap_or(false);
        if p.collection_id.is_none() && p.parent_document_id.is_none() && !an_die_wurzel {
            return Err(parameter_fehler(
                "Ziel fehlt: collection_id, parent_document_id oder `to_top_level: true` angeben.",
            ));
        }
        if p.parent_document_id.is_some() && an_die_wurzel {
            return Err(parameter_fehler(
                "`parent_document_id` und `to_top_level` widersprechen sich.",
            ));
        }

        let profil = match self.hole_profil() {
            Ok(profil) => profil,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let client = client(&profil)?;

        // documents.move braucht eine Sammlung, wenn die Elternseite fällt.
        let sammlung = match (&p.collection_id, an_die_wurzel) {
            (Some(sammlung), _) => Some(sammlung.clone()),
            (None, true) => {
                client
                    .aufrufen::<Dokument>("documents.info", json!({ "id": p.id.trim() }))
                    .await
                    .map_err(mcp_fehler)?
                    .daten
                    .sammlung_id
            }
            (None, false) => None,
        };

        let rumpf = rumpf(vec![
            ("id", Some(json!(p.id.trim()))),
            ("collectionId", sammlung.clone().map(|wert| json!(wert))),
            (
                "parentDocumentId",
                if an_die_wurzel {
                    Some(Value::Null)
                } else {
                    p.parent_document_id.clone().map(|wert| json!(wert))
                },
            ),
            ("index", p.index.map(|wert| json!(wert))),
        ]);
        let ergebnis = client
            .aufrufen::<Verschoben>("documents.move", rumpf)
            .await
            .map_err(mcp_fehler)?;

        let ziel = if an_die_wurzel {
            format!(
                "an den Anfang von Sammlung {}",
                sammlung.as_deref().unwrap_or("(unbekannt)")
            )
        } else if let Some(eltern) = p.parent_document_id.as_deref() {
            format!("unter Dokument {eltern}")
        } else {
            format!(
                "in Sammlung {}",
                sammlung.as_deref().unwrap_or("(unbekannt)")
            )
        };
        Ok(antwort(format!(
            "Dokument {} verschoben: {ziel}. {} Dokument(e) betroffen.",
            p.id.trim(),
            ergebnis.daten.documents.len()
        )))
    }

    async fn archivieren(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: ArchivParam = argumente(roh)?;
        if p.id.trim().is_empty() {
            return Err(parameter_fehler("outline_archive braucht eine id."));
        }
        let gewuenscht = p
            .action
            .clone()
            .unwrap_or_else(|| "archive".to_string())
            .to_lowercase();
        let (methode, wort) = match gewuenscht.as_str() {
            "archive" => ("documents.archive", "archiviert"),
            "restore" | "unarchive" => ("documents.restore", "wiederhergestellt"),
            "unpublish" => ("documents.unpublish", "zurückgezogen (jetzt ein Entwurf)"),
            sonst => {
                return Err(parameter_fehler(format!(
                    "Unbekannte Aktion „{sonst}“. Erlaubt: archive, restore, unpublish."
                )));
            }
        };

        let profil = match self.hole_profil() {
            Ok(profil) => profil,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let ergebnis = client(&profil)?
            .aufrufen::<Value>(methode, json!({ "id": p.id.trim() }))
            .await
            .map_err(mcp_fehler)?;
        let titel = als_dokument(&ergebnis.daten)
            .map(|seite| seite.titel().to_string())
            .unwrap_or_else(|| p.id.trim().to_string());
        Ok(antwort(format!(
            "Dokument {titel} {wort}.\nId: {}",
            p.id.trim()
        )))
    }

    async fn loeschen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: LoeschenParam = argumente(roh)?;
        if p.id.trim().is_empty() {
            return Err(parameter_fehler("outline_delete braucht eine id."));
        }
        let endgueltig = p.permanent.unwrap_or(false);

        let profil = match self.hole_profil() {
            Ok(profil) => profil,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let client = client(&profil)?;
        let id = p.id.trim();

        // Den Titel vor dem Löschen holen, damit die Antwort die Seite nennt
        // und nicht nur eine Kennung.
        let titel = match client
            .aufrufen::<Dokument>("documents.info", json!({ "id": id }))
            .await
        {
            Ok(antwort) => antwort.daten.titel().to_string(),
            Err(_) => id.to_string(),
        };
        client
            .aufrufen::<Value>(
                "documents.delete",
                json!({ "id": id, "permanent": endgueltig }),
            )
            .await
            .map_err(mcp_fehler)?;

        let text = if endgueltig {
            format!("„{titel}“ ({id}) endgültig gelöscht. Das lässt sich nicht rückgängig machen.")
        } else {
            format!(
                "„{titel}“ ({id}) liegt jetzt im Papierkorb. Zurückholen mit outline_archive und action „restore“."
            )
        };
        Ok(antwort(text))
    }

    async fn kommentieren(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: KommentarParam = argumente(roh)?;
        if p.document_id.trim().is_empty() {
            return Err(parameter_fehler(
                "outline_comment braucht eine document_id.",
            ));
        }
        let text = p.text.trim().to_string();
        if text.is_empty() {
            return Err(parameter_fehler("outline_comment braucht einen Text."));
        }

        let profil = match self.hole_profil() {
            Ok(profil) => profil,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let art = if p.parent_comment_id.is_some() {
            "Antwort"
        } else {
            "Kommentar"
        };
        // Ältere Outline-Versionen prüfen `data` (ProseMirror), neuere `text`;
        // beides mitzuschicken macht den Aufruf in beiden Fällen gültig.
        let rumpf = rumpf(vec![
            ("documentId", Some(json!(p.document_id.trim()))),
            (
                "parentCommentId",
                p.parent_comment_id.map(|wert| json!(wert)),
            ),
            ("text", Some(json!(text))),
            (
                "data",
                Some(json!({
                    "type": "doc",
                    "content": [{
                        "type": "paragraph",
                        "content": [{ "type": "text", "text": text }]
                    }]
                })),
            ),
        ]);
        let kommentar = client(&profil)?
            .aufrufen::<Kommentar>("comments.create", rumpf)
            .await
            .map_err(mcp_fehler)?
            .daten;
        Ok(antwort(format!(
            "{art} an Dokument {} geschrieben.\nKommentar-Id: {}",
            p.document_id.trim(),
            kommentar.id
        )))
    }

    async fn exportieren(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: ExportParam = argumente(roh)?;
        if p.target_dir.trim().is_empty() {
            return Err(parameter_fehler(
                "outline_export braucht einen Zielordner (`target_dir`).",
            ));
        }
        match (&p.document_id, &p.collection_id) {
            (None, None) => {
                return Err(parameter_fehler(
                    "Entweder document_id (eine Seite) oder collection_id (eine Sammlung) angeben.",
                ));
            }
            (Some(_), Some(_)) => {
                return Err(parameter_fehler(
                    "document_id und collection_id schließen einander aus.",
                ));
            }
            _ => {}
        }

        let profil = match self.hole_profil() {
            Ok(profil) => profil,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let client = client(&profil)?;

        // Welche Seiten sollen in Dateien landen?
        let mut ziele: Vec<(String, String)> = Vec::new();
        if let Some(sammlung) = p.collection_id.as_deref() {
            let baum = client
                .aufrufen::<Vec<Knoten>>("collections.documents", json!({ "id": sammlung }))
                .await
                .map_err(mcp_fehler)?
                .daten;
            for knoten in baum_knoten(&baum) {
                ziele.push((knoten.id.clone(), knoten.titel().to_string()));
            }
        } else if let Some(id) = p.document_id.as_deref() {
            let seite = client
                .aufrufen::<Dokument>("documents.info", json!({ "id": id }))
                .await
                .map_err(mcp_fehler)?
                .daten;
            ziele.push((seite.kennung().to_string(), seite.titel().to_string()));

            if p.include_children.unwrap_or(false) {
                if let Some(sammlung) = seite.sammlung_id.as_deref() {
                    let baum = client
                        .aufrufen::<Vec<Knoten>>("collections.documents", json!({ "id": sammlung }))
                        .await
                        .map_err(mcp_fehler)?
                        .daten;
                    if let Some(knoten) = baum_knoten(&baum)
                        .into_iter()
                        .find(|knoten| knoten.id == seite.kennung())
                    {
                        for kind in baum_knoten(&knoten.children) {
                            ziele.push((kind.id.clone(), kind.titel().to_string()));
                        }
                    }
                }
            }
        }

        let grenze = p.limit.unwrap_or(50) as usize;
        let anzahl = ziele.len();
        let ordner = Path::new(p.target_dir.trim());
        let mut geschrieben: Vec<String> = Vec::new();
        let mut gescheitert: Vec<(String, String)> = Vec::new();
        let mut benutzt: BTreeSet<String> = BTreeSet::new();

        for (id, titel) in ziele.iter().take(grenze) {
            let markdown = match client
                .aufrufen::<String>("documents.export", json!({ "id": id }))
                .await
            {
                Ok(antwort) => antwort.daten,
                Err(fehler) => {
                    gescheitert.push((id.clone(), fehler.to_string()));
                    continue;
                }
            };
            let mut stamm = datei::slug(titel);
            // Wikis sind voll von Seiten namens „Übersicht“: keine darf eine
            // andere überschreiben.
            if !benutzt.insert(stamm.clone()) {
                stamm = format!("{stamm}-{}", kuerzen_roh(id, 8));
                benutzt.insert(stamm.clone());
            }
            match datei::schreiben(ordner, &format!("{stamm}.md"), &markdown) {
                Ok(pfad) => geschrieben.push(pfad.display().to_string()),
                Err(fehler) => gescheitert.push((id.clone(), fehler.to_string())),
            }
        }

        let mut zeilen = vec![format!(
            "{} Dokument(e) nach {} exportiert:",
            geschrieben.len(),
            ordner.display()
        )];
        zeilen.extend(geschrieben.iter().map(|pfad| format!("- {pfad}")));
        if anzahl > grenze {
            zeilen.push(format!(
                "\n{} weitere(s) Dokument(e) nicht exportiert (Grenze {grenze}).",
                anzahl - grenze
            ));
        }
        if !gescheitert.is_empty() {
            zeilen.push(format!(
                "\n{} Dokument(e) fehlgeschlagen:",
                gescheitert.len()
            ));
            zeilen.extend(
                gescheitert
                    .iter()
                    .map(|(id, fehler)| format!("- {id}: {fehler}")),
            );
        }
        Ok(antwort(zeilen.join("\n")))
    }
}

// ---------------------------------------------------------------- Helfer

/// Ein Werkzeug, das nichts ändert.
fn lese_werkzeug<P: JsonSchema + Any>(
    name: &'static str,
    titel: &'static str,
    beschreibung: &'static str,
) -> Tool {
    werkzeug::<P>(name, titel, beschreibung, Some(true), Some(false))
}

/// Ein Werkzeug, das etwas ändert (`destructive_hint`, AGENTS.md §8).
fn schreib_werkzeug<P: JsonSchema + Any>(
    name: &'static str,
    titel: &'static str,
    beschreibung: &'static str,
) -> Tool {
    werkzeug::<P>(name, titel, beschreibung, Some(false), Some(true))
}

fn werkzeug<P: JsonSchema + Any>(
    name: &'static str,
    titel: &'static str,
    beschreibung: &'static str,
    nur_lesen: Option<bool>,
    zerstoerend: Option<bool>,
) -> Tool {
    Tool::new(name, beschreibung, schema_for_type::<P>())
        .with_title(titel)
        .with_annotations(ToolAnnotations::from_raw(
            Some(titel.to_string()),
            nur_lesen,
            zerstoerend,
            nur_lesen,
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

/// Ein Fehler von Outline als MCP-Fehler (ohne Geheimnis, siehe [`Client`]).
fn mcp_fehler(fehler: OutlineFehler) -> McpError {
    McpError::internal_error(fehler.to_string(), None)
}

fn parameter_fehler(text: impl Into<String>) -> McpError {
    McpError::invalid_params(text.into(), None)
}

fn client(profil: &Profil) -> Result<Client, McpError> {
    Client::aus_profil(profil).map_err(mcp_fehler)
}

/// Der bestehende Markdown-Rumpf einer Seite (für append/prepend).
async fn bestand(client: &Client, id: &str) -> Result<String, McpError> {
    Ok(client
        .aufrufen::<Dokument>("documents.info", json!({ "id": id }))
        .await
        .map_err(mcp_fehler)?
        .daten
        .text)
}

/// Ein Antworttext plus der Hinweis, wie es weitergeht.
fn mit_weiteren(text: String, seiten: Option<&Seiten>, anzahl: usize) -> CallToolResult {
    antwort(format!("{text}{}", format::weitere(seiten, anzahl)))
}

/// Ein Rumpf aus Paaren; `None` fällt weg (Outline mag keine leeren Felder).
fn rumpf(paare: Vec<(&str, Option<Value>)>) -> Value {
    let mut karte = Map::new();
    for (name, wert) in paare {
        if let Some(wert) = wert {
            karte.insert(name.to_string(), wert);
        }
    }
    Value::Object(karte)
}

fn als_dokument(wert: &Value) -> Option<Dokument> {
    serde_json::from_value(wert.clone()).ok()
}

fn kuerzen_roh(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

fn suchkriterien(p: &SuchParam, suchbegriff: &str) -> Value {
    rumpf(vec![
        ("query", Some(json!(suchbegriff))),
        ("limit", Some(json!(p.limit.unwrap_or(15)))),
        ("offset", Some(json!(p.offset.unwrap_or(0)))),
        (
            "collectionId",
            p.collection_id.clone().map(|wert| json!(wert)),
        ),
        ("documentId", p.document_id.clone().map(|wert| json!(wert))),
        (
            "statusFilter",
            p.status_filter.clone().map(|wert| json!(wert)),
        ),
        ("dateFilter", p.date_filter.clone().map(|wert| json!(wert))),
    ])
}

// ------------------------------------------------------------- Parameter

/// Parameter von `outline_collections`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct SammlungenParam {
    /// Wie viele Sammlungen zurückkommen. Standard 50.
    #[serde(default)]
    pub limit: Option<u32>,
    /// Wie viele Sammlungen übersprungen werden (Seitenweise blättern). Standard 0.
    #[serde(default)]
    pub offset: Option<u32>,
}

/// Parameter von `outline_structure`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct StrukturParam {
    /// Kennung der Sammlung; steht in outline_collections.
    pub collection_id: String,
}

/// Parameter von `outline_search`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct SuchParam {
    /// Suchwörter. Eigentümliche Substantive suchen besser als ganze Fragen.
    pub query: String,
    /// Wie viele Treffer zurückkommen. Standard 15.
    #[serde(default)]
    pub limit: Option<u32>,
    /// Wie viele Treffer übersprungen werden. Standard 0.
    #[serde(default)]
    pub offset: Option<u32>,
    /// Nur in dieser Sammlung suchen.
    #[serde(default)]
    pub collection_id: Option<String>,
    /// Nur diese Seite und ihre Kinder durchsuchen.
    #[serde(default)]
    pub document_id: Option<String>,
    /// Nur Seiten in diesen Zuständen: draft, archived, published. Standard: veröffentlichte.
    #[serde(default)]
    pub status_filter: Option<Vec<String>>,
    /// Nur Seiten, die in diesem Zeitraum geändert wurden: day, week, month, year.
    #[serde(default)]
    pub date_filter: Option<String>,
}

/// Parameter von `outline_list`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ListenParam {
    /// Nur Dokumente in dieser Sammlung.
    #[serde(default)]
    pub collection_id: Option<String>,
    /// Nur direkte Kinder dieser Seite.
    #[serde(default)]
    pub parent_document_id: Option<String>,
    /// Statt einer Liste die eigenen unveröffentlichten Entwürfe. Standard false.
    #[serde(default)]
    pub drafts: Option<bool>,
    /// Wie viele Dokumente zurückkommen. Standard 25.
    #[serde(default)]
    pub limit: Option<u32>,
    /// Wie viele Dokumente übersprungen werden. Standard 0.
    #[serde(default)]
    pub offset: Option<u32>,
    /// Sortierfeld: updatedAt (Standard), createdAt, title oder index.
    #[serde(default)]
    pub sort: Option<String>,
    /// ASC oder DESC. Standard DESC.
    #[serde(default)]
    pub direction: Option<String>,
}

/// Parameter von `outline_read`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct LeseParam {
    /// Kennung des Dokuments (uuid) oder die Kürzung aus einer Outline-URL.
    pub id: String,
    /// Rumpf nach so vielen Zeichen kürzen. Standard 8000, 0 heißt ungekürzt.
    #[serde(default)]
    pub max_zeichen: Option<u32>,
    /// Auch die direkten Kindseiten auflisten. Standard false.
    #[serde(default)]
    pub include_children: Option<bool>,
    /// Die Seite zusätzlich als Markdown-Datei in diesem Ordner ablegen.
    #[serde(default)]
    pub save_dir: Option<String>,
}

/// Parameter von `outline_comments`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct KommentareParam {
    /// Kennung des Dokuments.
    pub document_id: String,
    /// Wie viele Kommentare zurückkommen. Standard 50.
    #[serde(default)]
    pub limit: Option<u32>,
    /// Wie viele Kommentare übersprungen werden. Standard 0.
    #[serde(default)]
    pub offset: Option<u32>,
}

/// Parameter von `outline_create`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ErstellenParam {
    /// Titel der Seite. Lieber genau und auffindbar als lang.
    pub title: String,
    /// Markdown-Rumpf. Den Titel nicht als Überschrift wiederholen – Outline zeigt ihn getrennt.
    #[serde(default)]
    pub text: Option<String>,
    /// Sammlung, in der die Seite oben liegt. Nötig, wenn keine parent_document_id angegeben ist.
    #[serde(default)]
    pub collection_id: Option<String>,
    /// Die Seite unter dieser Seite einhängen.
    #[serde(default)]
    pub parent_document_id: Option<String>,
    /// Die Seite aus dieser Vorlage anlegen.
    #[serde(default)]
    pub template_id: Option<String>,
    /// Emoji oder Symbolname neben dem Titel.
    #[serde(default)]
    pub icon: Option<String>,
    /// Sofort veröffentlichen. Standard true; false legt einen persönlichen Entwurf an.
    #[serde(default)]
    pub publish: Option<bool>,
}

/// Parameter von `outline_update`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct AendernParam {
    /// Kennung des Dokuments.
    pub id: String,
    /// Markdown, das geschrieben, angehängt oder vorangestellt wird.
    #[serde(default)]
    pub text: Option<String>,
    /// Wie `text` wirkt: replace (Standard, ersetzt alles), append (ans Ende), prepend (an den Anfang).
    #[serde(default)]
    pub mode: Option<String>,
    /// Neuer Titel.
    #[serde(default)]
    pub title: Option<String>,
    /// Neues Emoji oder Symbol.
    #[serde(default)]
    pub icon: Option<String>,
    /// Einen Entwurf veröffentlichen.
    #[serde(default)]
    pub publish: Option<bool>,
}

/// Parameter von `outline_move`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct VerschiebenParam {
    /// Kennung des zu verschiebenden Dokuments.
    pub id: String,
    /// Zielsammlung.
    #[serde(default)]
    pub collection_id: Option<String>,
    /// Ziel-Elternseite (hängt das Dokument darunter ein).
    #[serde(default)]
    pub parent_document_id: Option<String>,
    /// Aus der aktuellen Elternseite lösen und an den Anfang der Sammlung stellen. Standard false.
    #[serde(default)]
    pub to_top_level: Option<bool>,
    /// Platz unter den neuen Geschwistern (0 = erstes).
    #[serde(default)]
    pub index: Option<u32>,
}

/// Parameter von `outline_archive`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ArchivParam {
    /// Kennung des Dokuments.
    pub id: String,
    /// archive (Standard), restore oder unpublish.
    #[serde(default)]
    pub action: Option<String>,
}

/// Parameter von `outline_delete`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct LoeschenParam {
    /// Kennung des Dokuments.
    pub id: String,
    /// Endgültig löschen statt in den Papierkorb legen. Nicht umkehrbar. Standard false.
    #[serde(default)]
    pub permanent: Option<bool>,
}

/// Parameter von `outline_comment`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct KommentarParam {
    /// Kennung des Dokuments, das kommentiert wird.
    pub document_id: String,
    /// Text des Kommentars.
    pub text: String,
    /// In diesem Kommentarstrang antworten.
    #[serde(default)]
    pub parent_comment_id: Option<String>,
}

/// Parameter von `outline_export`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ExportParam {
    /// Ordner für die .md-Dateien. Absolute Pfade sind am sichersten.
    pub target_dir: String,
    /// Eine Seite exportieren. Schließt collection_id aus.
    #[serde(default)]
    pub document_id: Option<String>,
    /// Alle Seiten dieser Sammlung exportieren.
    #[serde(default)]
    pub collection_id: Option<String>,
    /// Mit document_id auch die verschachtelten Kindseiten exportieren. Standard false.
    #[serde(default)]
    pub include_children: Option<bool>,
    /// Höchstens so viele Dokumente schreiben. Standard 50.
    #[serde(default)]
    pub limit: Option<u32>,
}

// -------------------------------------------------------------- MCP-Anker

impl ServerHandler for OutlineServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(quiver_tools_kern::mcp::server_info(&self.zugang))
            .with_instructions(
                "Outline-Wiki: Sammlungen und Seiten lesen, durchsuchen, anlegen, ändern, \
                 verschieben, archivieren, löschen und kommentieren. Den Zugang trägt der User \
                 im quiver-Fenster unter Tools → Outline-Wiki ein; fehlt er, sagen die \
                 Werkzeuge das als Text."
                    .to_string(),
            )
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

    #[test]
    fn lesende_und_schreibende_werkzeuge_tragen_die_richtigen_hinweise() {
        let server = OutlineServer::neu(ohne_zugang());
        let werkzeuge = server.werkzeuge();
        assert_eq!(werkzeuge.len(), 14);

        let namen: Vec<&str> = werkzeuge.iter().map(|w| w.name.as_ref()).collect();
        for erwartet in [
            "outline_collections",
            "outline_structure",
            "outline_search",
            "outline_list",
            "outline_read",
            "outline_comments",
            "outline_profile",
            "outline_create",
            "outline_update",
            "outline_move",
            "outline_archive",
            "outline_delete",
            "outline_comment",
            "outline_export",
        ] {
            assert!(namen.contains(&erwartet), "{erwartet} fehlt");
        }

        let lesend = [
            "outline_collections",
            "outline_structure",
            "outline_search",
            "outline_list",
            "outline_read",
            "outline_comments",
            "outline_profile",
        ];
        for werkzeug in &werkzeuge {
            let hinweise = werkzeug.annotations.as_ref().expect("Hinweise fehlen");
            if lesend.contains(&werkzeug.name.as_ref()) {
                assert_eq!(hinweise.read_only_hint, Some(true), "{}", werkzeug.name);
                assert_eq!(hinweise.destructive_hint, Some(false), "{}", werkzeug.name);
            } else {
                assert_eq!(hinweise.read_only_hint, Some(false), "{}", werkzeug.name);
                assert_eq!(hinweise.destructive_hint, Some(true), "{}", werkzeug.name);
            }
            assert!(!werkzeug.description.as_deref().unwrap_or("").is_empty());
            // Es gibt nur einen Zugang: kein Werkzeug nimmt mehr `profil`.
            let eigenschaften = werkzeug.input_schema.get("properties");
            assert!(
                !eigenschaften.is_some_and(|werte| werte.get("profil").is_some()),
                "{} kennt noch `profil`: {:?}",
                werkzeug.name,
                werkzeug.input_schema
            );
        }
    }

    #[test]
    fn ein_unbekanntes_werkzeug_ist_ein_parameterfehler() {
        let server = OutlineServer::neu(ohne_zugang());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let fehler = runtime
            .block_on(server.aufrufen("outline_gibtsnicht", None))
            .unwrap_err();
        assert!(fehler.message.contains("outline_gibtsnicht"));
    }

    #[test]
    fn ohne_zugang_kommt_der_hinweis_und_kein_fehler() {
        let server = OutlineServer::neu(ohne_zugang());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let ergebnis = runtime
            .block_on(server.aufrufen("outline_collections", Some(JsonObject::new())))
            .unwrap();
        let text = ergebnis.content[0].as_text().unwrap().text.clone();
        assert!(text.contains("Zugang fehlt"), "{text}");
        assert!(text.contains("Outline-Wiki"), "{text}");
    }

    /// Ein Zugang ohne Umgebung – nichts gesetzt, kein Schlüsselbund.
    fn ohne_zugang() -> Zugang {
        Zugang::mit_umgebung(&crate::Outline, |_| None)
    }
}
