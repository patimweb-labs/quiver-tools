// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der MCP-Server: draw.io-Werkzeuge über stdio (Aufgabe 18).
//!
//! Vierzehn Werkzeuge plus `drawio_profile` aus dem Kern:
//!
//! | lesend (`read_only_hint`) | verändernd (`destructive_hint`) | keins von beidem |
//! |---|---|---|
//! | `_status`, `_liste`, `_seiten`, `_lesen`, `_bild`, `_link` | `_erstellen`, `_aus_quelltext`, `_elemente_aendern`, `_elemente_hinzufuegen`, `_elemente_entfernen`, `_seite_hinzufuegen`, `_exportieren` | `_oeffnen` |
//!
//! Drei Regeln stehen **vor** jedem Zugriff:
//!
//! | Regel | Wirkung |
//! |---|---|
//! | Arbeitsordner (`ordner`) aus dem Zugang | fehlt er, kommt der Hinweis auf das quiver-Fenster – **kein** MCP-Fehler |
//! | `schreiben` (Standard an) | verändernde Werkzeuge antworten ohne Schreiben, wenn er aus ist |
//! | `stand` aus `drawio_lesen` | Änderungen nur, wenn die Datei noch so ist; sonst „inzwischen geändert – neu lesen“ |
//!
//! Jede Antwort ist kurz und nennt, was der Agent als Nächstes braucht (Id,
//! `stand`, Pfad). Es gibt keine Anmeldung und kein Geheimnis: die Dateien
//! liegen im Arbeitsordner.

use std::any::Any;
use std::sync::Arc;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use quiver_diagramm::ablage::{Ablage, AblageFehler, Inhalt, Stand};
use quiver_diagramm::modell::{Diagramm, Farbe};
use quiver_tools_kern::mcp::profile_tool;
use quiver_tools_kern::{antwort, kuerzen, zugang, OhneProfil, Profil, Zugang};
use rmcp::handler::server::tool::schema_for_type;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, JsonObject,
    ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    ToolAnnotations,
};
use rmcp::schemars::JsonSchema;
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;

use crate::aus_modell;
use crate::bild;
use crate::browser::{Browser, SystemBrowser};
use crate::fehler::FormatFehler;
use crate::format;
use crate::formen::Form;
use crate::link;
use crate::mx::{self, Datei, Lage, Randbehandlung};
use crate::werkzeug::{self, ENDUNG, TITEL};
use crate::xml::Element;

/// Was ein Werkzeug zurückgibt.
type Ergebnis = Result<CallToolResult, McpError>;

/// Ein fehlender Zugang wird zur Hinweis-Antwort (kein MCP-Fehler, SPEC §2).
macro_rules! oder_hinweis {
    ($wert:expr) => {
        match $wert {
            Ok(wert) => wert,
            Err(ohne) => return ohne.in_ergebnis(),
        }
    };
}

/// Der draw.io-Server: der Zugang und der Browser.
pub struct DrawioServer {
    /// Der Zugang des Kerns (Felder `ordner` und `schreiben`).
    zugang: Zugang,
    /// Wer den Browser öffnet – im Test ein Aufzeichner.
    browser: Arc<dyn Browser>,
}

impl DrawioServer {
    /// Der Normalfall: Felder aus der Umgebung, echter Browser.
    pub fn neu(zugang: Zugang) -> Self {
        DrawioServer::mit(zugang, Arc::new(SystemBrowser::neu()))
    }

    /// Fester Browser (Tests und Sonderfälle).
    pub fn mit(zugang: Zugang, browser: Arc<dyn Browser>) -> Self {
        DrawioServer { zugang, browser }
    }

    /// Alle Werkzeuge, wie `tools/list` sie meldet.
    pub fn werkzeuge(&self) -> Vec<Tool> {
        let mut alle = self.lesende_werkzeuge();
        alle.extend(self.schreibende_werkzeuge());
        alle.extend(self.oeffnende_werkzeuge());
        alle.push(profile_tool(&self.zugang));
        alle
    }

    fn lesende_werkzeuge(&self) -> Vec<Tool> {
        vec![
            lesend::<LeerParam>(
                "drawio_status",
                "Zustand",
                "Zeigt den Arbeitsordner, die Zahl der .drawio-Dateien und ob „schreiben“ an ist. \
                 Keine Anmeldung, kein Netz.",
            ),
            lesend::<LeerParam>(
                "drawio_liste",
                "Dateien",
                "Listet die .drawio-Dateien im Arbeitsordner mit Größe; Unterordner werden \
                 mitgezählt.",
            ),
            lesend::<DateiParam>(
                "drawio_seiten",
                "Seiten",
                "Listet die Seiten einer Datei: Nummer, Id, Name, Zellen und ob die Seite \
                 komprimiert gespeichert ist. Der erste Schritt, wenn eine Seiten-Id gebraucht \
                 wird.",
            ),
            lesend::<LesenParam>(
                "drawio_lesen",
                "Lesen",
                "Beschreibt eine Seite knapp: `stand` (für jeden Änderungsaufruf), Seiten-Ids und \
                 je Zelle Id, Art, Form, Elternteil und Text. Mit `roh: true` kommt der Dateitext \
                 selbst, gekürzt über `max_zeichen`.",
            ),
            lesend::<BildParam>(
                "drawio_bild",
                "Bild",
                "Zeichnet die Seite als PNG und gibt es als Bild zurück (eigener Zeichner, kein \
                 Browser). Rechteck, Ellipse, Raute, Zylinder, Notiz, Text, Container und Kanten; \
                 unbekannte Formen werden Rechteck, und die Antwort nennt sie.",
            ),
            lesend::<DateiParam>(
                "drawio_link",
                "Link",
                "Gibt den draw.io-Link als Text: das ganze Diagramm liegt im Fragment der Adresse \
                 und geht nie an einen Server. Höchstens 1 MB; mit drawio_oeffnen öffnet ihn der \
                 Browser.",
            ),
        ]
    }

    fn schreibende_werkzeuge(&self) -> Vec<Tool> {
        vec![
            schreibend::<ErstellenParam>(
                "drawio_erstellen",
                "Erstellen",
                "Legt eine Datei aus einem Modell an (Knoten, Kanten, Gruppen, Richtung); das \
                 Layout rechnet quiver-diagramm. Vorhandene Dateien werden nie überschrieben.",
            ),
            schreibend::<QuelltextParam>(
                "drawio_aus_quelltext",
                "Aus Quelltext",
                "Legt eine Datei aus fertigem draw.io-XML an (ein <mxfile> oder ein \
                 <mxGraphModel>). Geprüft werden Struktur, Zell-Ids, Kanten und Links (nur \
                 http(s)/mailto).",
            ),
            schreibend::<AendernParam>(
                "drawio_elemente_aendern",
                "Elemente ändern",
                "Ändert Text, Farbe, Form, Lage und Verbindung je Zellen-Id. Unbekannte Stile, \
                 Felder und Elemente bleiben stehen. Verlangt den `stand` aus drawio_lesen.",
            ),
            schreibend::<HinzufuegenParam>(
                "drawio_elemente_hinzufuegen",
                "Elemente hinzufügen",
                "Hängt Knoten oder Kanten an (Kante, sobald `quelle`/`ziel` gesetzt ist). Ids \
                 vergibt das Werkzeug, wenn keine genannt ist; Kanten brauchen vorhandene Knoten.",
            ),
            schreibend::<EntfernenParam>(
                "drawio_elemente_entfernen",
                "Elemente entfernen",
                "Entfernt Zellen. Kanten und Kinder gehen nur mit ausdrücklicher Angabe mit: \
                 `kanten: entfernen|loesen`, `kinder: entfernen|behalten`.",
            ),
            schreibend::<SeiteHinzufuegenParam>(
                "drawio_seite_hinzufuegen",
                "Seite hinzufügen",
                "Hängt eine Seite an – aus einem Modell, aus fertigem <mxGraphModel>-XML oder \
                 leer. Verlangt den `stand` aus drawio_lesen.",
            ),
            schreibend::<ExportParam>(
                "drawio_exportieren",
                "Exportieren",
                "Schreibt die Seite als SVG oder PNG in den Arbeitsordner; die Endung entscheidet. \
                 Vorhandene Dateien werden nie überschrieben.",
            ),
        ]
    }

    fn oeffnende_werkzeuge(&self) -> Vec<Tool> {
        vec![oeffnend::<DateiParam>(
            "drawio_oeffnen",
            "Im Browser öffnen",
            "Öffnet genau den selbst gebauten Link im Standardbrowser. Änderungen dort landen \
             NICHT in der lokalen Datei. Ist der Link zu lang für den Startbefehl, nennt die \
             Antwort den Dateipfad statt eines Fehlers.",
        )]
    }

    /// Ein Werkzeug aufrufen. Auch der Weg, den die Tests nehmen.
    pub fn aufrufen(&self, name: &str, roh: Option<JsonObject>) -> Ergebnis {
        match name {
            "drawio_status" => self.status(roh),
            "drawio_liste" => self.liste(roh),
            "drawio_seiten" => self.seiten(roh),
            "drawio_lesen" => self.lesen(roh),
            "drawio_bild" => self.bild(roh),
            "drawio_link" => self.link(roh),
            "drawio_erstellen" => self.erstellen(roh),
            "drawio_aus_quelltext" => self.aus_quelltext(roh),
            "drawio_elemente_aendern" => self.elemente_aendern(roh),
            "drawio_elemente_hinzufuegen" => self.elemente_hinzufuegen(roh),
            "drawio_elemente_entfernen" => self.elemente_entfernen(roh),
            "drawio_seite_hinzufuegen" => self.seite_hinzufuegen(roh),
            "drawio_exportieren" => self.exportieren(roh),
            "drawio_oeffnen" => self.oeffnen(roh),
            "drawio_profile" => {
                let _: LeerParam = argumente(roh)?;
                Ok(quiver_tools_kern::mcp::profile_antwort(&self.zugang)?)
            }
            sonst => Err(McpError::invalid_params(
                format!("Unbekanntes Werkzeug „{sonst}“."),
                None,
            )),
        }
    }

    // ---------------------------------------------------------- Zugang

    fn profil(&self) -> Result<Profil, OhneProfil> {
        zugang(TITEL, self.zugang.profil())
    }

    fn ablage(&self) -> Result<Ablage, OhneProfil> {
        let profil = self.profil()?;
        ablage_aus(&profil)
    }

    fn schreibende_ablage(&self) -> Result<Ablage, OhneProfil> {
        let profil = self.profil()?;
        if !werkzeug::schreiben_erlaubt(&profil) {
            return Err(OhneProfil::Hinweis(antwort(format::schreiben_aus())));
        }
        ablage_aus(&profil)
    }

    // ---------------------------------------------------------- lesend

    fn status(&self, roh: Option<JsonObject>) -> Ergebnis {
        let _: LeerParam = argumente(roh)?;
        let profil = oder_hinweis!(self.profil());
        let ablage = oder_hinweis!(ablage_aus(&profil));
        let dateien = ablage.liste().map_err(ablage_fehler)?;
        Ok(antwort(format::status(
            ablage.wurzel(),
            dateien.len(),
            werkzeug::schreiben_erlaubt(&profil),
        )))
    }

    fn liste(&self, roh: Option<JsonObject>) -> Ergebnis {
        let _: LeerParam = argumente(roh)?;
        let ablage = oder_hinweis!(self.ablage());
        let eintraege = ablage.liste().map_err(ablage_fehler)?;
        Ok(antwort(format::liste(ablage.wurzel(), &eintraege)))
    }

    fn seiten(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: DateiParam = argumente(roh)?;
        let name = datei_name(&p.datei)?;
        let ablage = oder_hinweis!(self.ablage());
        let (_, datei) = datei_lesen(&ablage, name)?;
        let seiten = datei.seiten().map_err(format_fehler)?;
        Ok(antwort(format::seiten(name, &seiten)))
    }

    fn lesen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: LesenParam = argumente(roh)?;
        let name = datei_name(&p.datei)?;
        let ablage = oder_hinweis!(self.ablage());
        let (inhalt, datei) = datei_lesen(&ablage, name)?;

        if p.roh {
            let max = p.max_zeichen.unwrap_or(4000).clamp(200, 200_000);
            let (text, weggelassen) = kuerzen(&inhalt.text, max);
            let mut aus = format!(
                "Datei „{name}“ · {} · stand: {}\n{text}",
                format::groesse(inhalt.groesse),
                inhalt.stand
            );
            if let Some(weggelassen) = weggelassen {
                aus.push_str(&format!(
                    "\n… {weggelassen} Zeichen weggelassen (Grenze {max}; mit `max_zeichen` \
                     erhöhen)."
                ));
            }
            return Ok(antwort(aus));
        }

        let index = datei
            .seite_index(p.seite.as_deref())
            .map_err(format_fehler)?;
        let seiten = datei.seiten().map_err(format_fehler)?;
        let seite = seiten.get(index).ok_or_else(|| {
            McpError::invalid_params(
                "Diese Seite gibt es nicht – bitte drawio_seiten lesen.",
                None,
            )
        })?;
        let modell = datei.modell(index).map_err(format_fehler)?;
        let root = modell
            .kind("root")
            .ok_or_else(|| McpError::invalid_params("Dem Modell fehlt <root>.", None))?;
        let zellen = mx::zellen(root);
        let stand = inhalt.stand.to_string();
        Ok(antwort(format::beschreibung(
            name,
            &stand,
            seite,
            seiten.len(),
            &zellen,
            200,
        )))
    }

    fn bild(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: BildParam = argumente(roh)?;
        let name = datei_name(&p.datei)?;
        let ablage = oder_hinweis!(self.ablage());
        let (_, datei) = datei_lesen(&ablage, name)?;
        let index = datei
            .seite_index(p.seite.as_deref())
            .map_err(format_fehler)?;
        let modell = datei.modell(index).map_err(format_fehler)?;
        let gezeichnet = bild::zeichnung_aus_modell(&modell).map_err(format_fehler)?;
        let png = gezeichnet
            .png(p.max_kante.unwrap_or(1200))
            .map_err(format_fehler)?;
        Ok(CallToolResult::success(vec![
            ContentBlock::image(STANDARD.encode(&png), "image/png"),
            ContentBlock::text(format!("Datei „{name}“: {}", gezeichnet.beschreibung())),
        ]))
    }

    fn link(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: DateiParam = argumente(roh)?;
        let name = datei_name(&p.datei)?;
        let ablage = oder_hinweis!(self.ablage());
        let (_, datei) = datei_lesen(&ablage, name)?;
        let seiten = datei.seiten().map_err(format_fehler)?.len();
        let adresse = link::link(&datei.als_text()).map_err(format_fehler)?;
        Ok(antwort(format!(
            "{}\n{adresse}",
            format::link_hinweis(seiten)
        )))
    }

    // ------------------------------------------------------ verändernd

    fn erstellen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: ErstellenParam = argumente(roh)?;
        let name = datei_name(&p.datei)?;
        let ablage = oder_hinweis!(self.schreibende_ablage());
        let diagramm = diagramm_aus(&p.modell)?;
        let modell = aus_modell::seite_aus_diagramm(&diagramm).map_err(format_fehler)?;
        let seite = p
            .seite
            .as_deref()
            .map(str::trim)
            .filter(|seite| !seite.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| seitenname(name));
        let datei = mx::neue_datei(&seite, modell).map_err(format_fehler)?;
        let text = datei.als_text();
        let stand = ablage.anlegen(name, &text).map_err(ablage_fehler)?;
        let seiten = datei.seiten().map_err(format_fehler)?;
        Ok(antwort(format::erstellt(
            name,
            &stand.to_string(),
            seiten.len(),
            seiten.iter().map(|seite| seite.zellen).sum(),
        )))
    }

    fn aus_quelltext(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: QuelltextParam = argumente(roh)?;
        let name = datei_name(&p.datei)?;
        let ablage = oder_hinweis!(self.schreibende_ablage());
        let datei = bau_aus_quelltext(&p.quelltext, p.seite.as_deref(), name)?;
        let text = datei.als_text();
        let stand = ablage.anlegen(name, &text).map_err(ablage_fehler)?;
        let seiten = datei.seiten().map_err(format_fehler)?;
        Ok(antwort(format::erstellt(
            name,
            &stand.to_string(),
            seiten.len(),
            seiten.iter().map(|seite| seite.zellen).sum(),
        )))
    }

    fn elemente_aendern(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: AendernParam = argumente(roh)?;
        let name = datei_name(&p.datei)?;
        if p.elemente.is_empty() {
            return Err(McpError::invalid_params(
                "`elemente` ist leer – bitte mindestens eine Änderung angeben.".to_string(),
                None,
            ));
        }
        let ablage = oder_hinweis!(self.schreibende_ablage());
        let stand = stand(&p.stand)?;
        let (_, mut datei) = datei_lesen(&ablage, name)?;
        let index = datei
            .seite_index(p.seite.as_deref())
            .map_err(format_fehler)?;
        let mut modell = datei.modell(index).map_err(format_fehler)?;
        let mut root = modell
            .kind("root")
            .ok_or_else(|| McpError::invalid_params("Dem Modell fehlt <root>.", None))?
            .clone();
        let aenderungen = aenderungen(&p.elemente)?;
        let geaendert = mx::aendern(&mut root, &aenderungen).map_err(format_fehler)?;
        mx::root_setzen(&mut modell, root);
        datei.modell_setzen(index, modell).map_err(format_fehler)?;
        let neu = ablage
            .aendern(name, &datei.als_text(), &stand)
            .map_err(ablage_fehler)?;
        Ok(antwort(format::geaendert(
            name,
            &neu.to_string(),
            geaendert.len(),
            "Element(e)",
        )))
    }

    fn elemente_hinzufuegen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: HinzufuegenParam = argumente(roh)?;
        let name = datei_name(&p.datei)?;
        if p.elemente.is_empty() {
            return Err(McpError::invalid_params(
                "`elemente` ist leer – bitte mindestens eine Zelle angeben.".to_string(),
                None,
            ));
        }
        let ablage = oder_hinweis!(self.schreibende_ablage());
        let stand = stand(&p.stand)?;
        let (_, mut datei) = datei_lesen(&ablage, name)?;
        let index = datei
            .seite_index(p.seite.as_deref())
            .map_err(format_fehler)?;
        let mut modell = datei.modell(index).map_err(format_fehler)?;
        let mut root = modell
            .kind("root")
            .ok_or_else(|| McpError::invalid_params("Dem Modell fehlt <root>.", None))?
            .clone();

        let (start_x, start_y) = freie_stelle(&root);
        let mut neue = Vec::new();
        for (nummer, element) in p.elemente.iter().enumerate() {
            let form = match element.form.as_deref() {
                Some(wert) => form(wert)?,
                None => Form::Rechteck,
            };
            let farbe = match element.farbe.as_deref() {
                Some(wert) => Some(farbe(wert)?),
                None => None,
            };
            neue.push(mx::NeueZelle {
                id: element.id.as_deref(),
                text: element.text.as_deref().unwrap_or_default(),
                form,
                farbe,
                lage: Lage::neu(
                    element.x.unwrap_or(start_x + 160.0 * (nummer % 4) as f32),
                    element.y.unwrap_or(start_y + 120.0 * (nummer / 4) as f32),
                    element.breite.unwrap_or(120.0),
                    element.hoehe.unwrap_or(60.0),
                ),
                eltern: element.eltern.as_deref(),
                quelle: element
                    .quelle
                    .as_deref()
                    .filter(|wert| !wert.trim().is_empty()),
                ziel: element
                    .ziel
                    .as_deref()
                    .filter(|wert| !wert.trim().is_empty()),
                gestrichelt: element.gestrichelt.unwrap_or(false),
                pfeil: element.pfeil.unwrap_or(true),
            });
        }
        let ids = mx::hinzufuegen(&mut root, &neue).map_err(format_fehler)?;
        mx::root_setzen(&mut modell, root);
        datei.modell_setzen(index, modell).map_err(format_fehler)?;
        let neu = ablage
            .aendern(name, &datei.als_text(), &stand)
            .map_err(ablage_fehler)?;
        Ok(antwort(format!(
            "{}\nIds: {}",
            format::geaendert(name, &neu.to_string(), ids.len(), "Element(e)"),
            ids.join(", ")
        )))
    }

    fn elemente_entfernen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: EntfernenParam = argumente(roh)?;
        let name = datei_name(&p.datei)?;
        let ablage = oder_hinweis!(self.schreibende_ablage());
        let stand = stand(&p.stand)?;
        let kanten = match p.kanten.as_deref() {
            Some(wert) => Some(Randbehandlung::aus_text(wert, "kanten").map_err(format_fehler)?),
            None => None,
        };
        let kinder = match p.kinder.as_deref() {
            Some(wert) => Some(Randbehandlung::aus_text(wert, "kinder").map_err(format_fehler)?),
            None => None,
        };
        let (_, mut datei) = datei_lesen(&ablage, name)?;
        let index = datei
            .seite_index(p.seite.as_deref())
            .map_err(format_fehler)?;
        let mut modell = datei.modell(index).map_err(format_fehler)?;
        let mut root = modell
            .kind("root")
            .ok_or_else(|| McpError::invalid_params("Dem Modell fehlt <root>.", None))?
            .clone();
        let entfernt = mx::entfernen(
            &mut root,
            &mx::Entfernen {
                ids: &p.ids,
                kanten,
                kinder,
            },
        )
        .map_err(format_fehler)?;
        mx::root_setzen(&mut modell, root);
        datei.modell_setzen(index, modell).map_err(format_fehler)?;
        let neu = ablage
            .aendern(name, &datei.als_text(), &stand)
            .map_err(ablage_fehler)?;
        Ok(antwort(format!(
            "{}\nEntfernt: {}",
            format::geaendert(name, &neu.to_string(), entfernt.len(), "Element(e)"),
            entfernt.join(", ")
        )))
    }

    fn seite_hinzufuegen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: SeiteHinzufuegenParam = argumente(roh)?;
        let name = datei_name(&p.datei)?;
        let ablage = oder_hinweis!(self.schreibende_ablage());
        let stand = stand(&p.stand)?;
        let modell = if let Some(modell) = &p.modell {
            let diagramm = diagramm_aus(modell)?;
            aus_modell::seite_aus_diagramm(&diagramm).map_err(format_fehler)?
        } else if let Some(quelltext) = &p.quelltext {
            modell_aus_quelltext(quelltext)?
        } else {
            mx::leeres_modell()
        };
        let (_, mut datei) = datei_lesen(&ablage, name)?;
        let seitenname = p.name.trim().to_string();
        datei
            .seite_hinzufuegen(&seitenname, p.id.as_deref(), modell)
            .map_err(format_fehler)?;
        let seiten = datei.seiten().map_err(format_fehler)?.len();
        let neu = ablage
            .aendern(name, &datei.als_text(), &stand)
            .map_err(ablage_fehler)?;
        Ok(antwort(format::seite_angehaengt(
            name,
            &seitenname,
            seiten,
            &neu.to_string(),
        )))
    }

    fn exportieren(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: ExportParam = argumente(roh)?;
        let name = datei_name(&p.datei)?;
        let ablage = oder_hinweis!(self.schreibende_ablage());
        let (_, datei) = datei_lesen(&ablage, name)?;
        let index = datei
            .seite_index(p.seite.as_deref())
            .map_err(format_fehler)?;
        let modell = datei.modell(index).map_err(format_fehler)?;
        let gezeichnet = bild::zeichnung_aus_modell(&modell).map_err(format_fehler)?;
        let klein = p.ausgabe.trim().to_ascii_lowercase();
        let (bytes, hinweis) = if klein.ends_with(".svg") {
            (
                gezeichnet.szene.svg().into_bytes(),
                gezeichnet.beschreibung(),
            )
        } else if klein.ends_with(".png") {
            (
                gezeichnet
                    .png(p.max_kante.unwrap_or(1200))
                    .map_err(format_fehler)?,
                gezeichnet.beschreibung(),
            )
        } else {
            return Err(McpError::invalid_params(
                "`ausgabe` braucht die Endung .svg oder .png.".to_string(),
                None,
            ));
        };
        ablage
            .ausgabe_schreiben(p.ausgabe.trim(), &bytes)
            .map_err(ablage_fehler)?;
        Ok(antwort(format::exportiert(
            name,
            p.ausgabe.trim(),
            bytes.len(),
            &hinweis,
        )))
    }

    // -------------------------------------------- im Browser öffnen

    fn oeffnen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: DateiParam = argumente(roh)?;
        let name = datei_name(&p.datei)?;
        let ablage = oder_hinweis!(self.ablage());
        let (_, datei) = datei_lesen(&ablage, name)?;
        let adresse = link::link(&datei.als_text()).map_err(format_fehler)?;
        let pfad = ablage.pfad(name).map_err(ablage_fehler)?;
        if adresse.chars().count() > link::MAX_ADRESSE {
            // Zu lang für den Startbefehl: eine Meldung mit Pfad, kein Fehler.
            return Ok(antwort(format::oeffnen_zu_lang(
                &pfad,
                adresse.chars().count(),
                link::MAX_ADRESSE,
            )));
        }
        self.browser.oeffnen(&adresse).map_err(|fehler| {
            McpError::internal_error(
                format!("Der Standardbrowser ließ sich nicht starten: {fehler}"),
                None,
            )
        })?;
        Ok(antwort(format::oeffnen(ablage.wurzel(), name)))
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

/// Ein veränderndes Werkzeug (`destructive_hint`, AGENTS.md §8).
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

/// Ein Werkzeug, das nach außen wirkt, aber nichts zerstört (`drawio_oeffnen`).
fn oeffnend<P: JsonSchema + Any>(
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
            Some(true),
        ))
}

/// Die Argumente eines Aufrufs als typisierte Parameter.
fn argumente<T: DeserializeOwned>(roh: Option<JsonObject>) -> Result<T, McpError> {
    let wert = Value::Object(roh.unwrap_or_default());
    serde_json::from_value(wert).map_err(|fehler| {
        McpError::invalid_params(format!("Die Parameter passen nicht: {fehler}"), None)
    })
}

/// Ein Dateiname, der nicht leer ist.
fn datei_name(text: &str) -> Result<&str, McpError> {
    let name = text.trim();
    if name.is_empty() {
        return Err(McpError::invalid_params(
            "`datei` fehlt – bitte den Pfad im Arbeitsordner angeben, z. B. „ablauf.drawio“."
                .to_string(),
            None,
        ));
    }
    Ok(name)
}

/// Der Stand, den der Agent gelesen hat.
fn stand(text: &str) -> Result<Stand, McpError> {
    Stand::aus_hex(text).ok_or_else(|| {
        McpError::invalid_params(
            "`stand` ist kein SHA-256 (64 Hexzeichen) – bitte mit drawio_lesen neu holen."
                .to_string(),
            None,
        )
    })
}

fn farbe(wert: &str) -> Result<Farbe, McpError> {
    let gesucht = wert.trim().to_ascii_lowercase();
    Farbe::aus_name(&gesucht).ok_or_else(|| {
        McpError::invalid_params(
            format!(
                "Farbe „{wert}“ kenne ich nicht – erlaubt sind: {}.",
                Farbe::ALLE
                    .iter()
                    .map(|farbe| farbe.name())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            None,
        )
    })
}

fn form(wert: &str) -> Result<Form, McpError> {
    Form::aus_name(wert).ok_or_else(|| {
        McpError::invalid_params(
            format!(
                "Form „{wert}“ kenne ich nicht – erlaubt sind: {}.",
                Form::namen()
            ),
            None,
        )
    })
}

fn diagramm_aus(wert: &Value) -> Result<Diagramm, McpError> {
    Diagramm::aus_json(&wert.to_string()).map_err(|fehler| {
        McpError::invalid_params(
            format!(
                "Das Modell passt nicht: {fehler}. Erwartet wird {{\"richtung\":\"unten\", \
                 \"knoten\":[{{\"id\",\"text\",\"form\"}}],\"kanten\":[…],\"gruppen\":[…]}}."
            ),
            None,
        )
    })
}

fn aenderungen(elemente: &[AenderungParam]) -> Result<Vec<mx::Aenderung<'_>>, McpError> {
    let mut liste = Vec::new();
    for element in elemente {
        let farbe = match element.farbe.as_deref() {
            Some(wert) => Some(farbe(wert)?),
            None => None,
        };
        let form = match element.form.as_deref() {
            Some(wert) => Some(form(wert)?),
            None => None,
        };
        liste.push(mx::Aenderung {
            id: element.id.as_str(),
            text: element.text.as_deref(),
            farbe,
            form,
            lage: mx::LageAenderung {
                x: element.x,
                y: element.y,
                breite: element.breite,
                hoehe: element.hoehe,
            },
            quelle: element.quelle.as_ref().map(|wert| Some(wert.as_str())),
            ziel: element.ziel.as_ref().map(|wert| Some(wert.as_str())),
        });
    }
    Ok(liste)
}

/// Die nächste freie Stelle unter den vorhandenen Knoten.
fn freie_stelle(root: &Element) -> (f32, f32) {
    let mut unten: f32 = 0.0;
    for eintrag in mx::eintraege(root) {
        if !mx::ist_knoten(eintrag) {
            continue;
        }
        let lage = mx::lage(eintrag);
        unten = unten.max(lage.y + lage.hoehe);
    }
    (20.0, if unten > 0.0 { unten + 40.0 } else { 20.0 })
}

/// Der Seitenname aus dem Dateinamen (ohne Endung).
fn seitenname(datei: &str) -> String {
    let ohne_ordner = datei.rsplit('/').next().unwrap_or(datei);
    let ohne_endung = ohne_ordner
        .strip_suffix(".drawio")
        .or_else(|| ohne_ordner.strip_suffix(".DRAWIO"))
        .unwrap_or(ohne_ordner);
    let name = ohne_endung.trim();
    if name.is_empty() {
        "Seite 1".to_string()
    } else {
        name.to_string()
    }
}

fn datei_lesen(ablage: &Ablage, datei: &str) -> Result<(Inhalt, Datei), McpError> {
    let inhalt = ablage.lesen(datei).map_err(ablage_fehler)?;
    let datei = Datei::aus_text(&inhalt.text).map_err(format_fehler)?;
    Ok((inhalt, datei))
}

fn bau_aus_quelltext(quelltext: &str, seite: Option<&str>, datei: &str) -> Result<Datei, McpError> {
    let text = quelltext.trim();
    if text.is_empty() {
        return Err(McpError::invalid_params(
            "`quelltext` ist leer – bitte den draw.io-XML-Inhalt angeben.".to_string(),
            None,
        ));
    }
    let element =
        crate::xml::lesen(text).map_err(|fehler| format_fehler(FormatFehler::from(fehler)))?;
    match element.name.as_str() {
        "mxfile" => Datei::aus_element(element).map_err(format_fehler),
        "mxGraphModel" => {
            mx::pruefe_modell(&element).map_err(format_fehler)?;
            let name = seite
                .map(str::trim)
                .filter(|seite| !seite.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| seitenname(datei));
            mx::neue_datei(&name, element).map_err(format_fehler)
        }
        sonst => Err(McpError::invalid_params(
            format!(
                "Erwartet wird ein <mxfile> oder ein <mxGraphModel>, gefunden wurde <{sonst}>."
            ),
            None,
        )),
    }
}

fn modell_aus_quelltext(quelltext: &str) -> Result<Element, McpError> {
    let element =
        crate::xml::lesen(quelltext).map_err(|fehler| format_fehler(FormatFehler::from(fehler)))?;
    if element.name == "mxfile" {
        // Ganzes Dokument: genau eine Seite ist hier gemeint.
        let datei = Datei::aus_element(element).map_err(format_fehler)?;
        let seiten = datei.seiten().map_err(format_fehler)?;
        if seiten.len() != 1 {
            return Err(McpError::invalid_params(
                format!(
                    "Der Quelltext enthält {} Seiten – hier wird ein <mxGraphModel> oder ein \
                     <mxfile> mit genau einer Seite erwartet.",
                    seiten.len()
                ),
                None,
            ));
        }
        return datei.modell(0).map_err(format_fehler);
    }
    if element.name != "mxGraphModel" {
        return Err(McpError::invalid_params(
            format!(
                "Erwartet wird ein <mxGraphModel> oder ein <mxfile>, gefunden wurde \
                 <{}>.",
                element.name
            ),
            None,
        ));
    }
    mx::pruefe_modell(&element).map_err(format_fehler)?;
    Ok(element)
}

fn ablage_aus(profil: &Profil) -> Result<Ablage, OhneProfil> {
    Ablage::neu(werkzeug::ordner(profil), ENDUNG)
        .map_err(|fehler| OhneProfil::Fehler(ablage_fehler(fehler)))
}

fn ablage_fehler(fehler: AblageFehler) -> McpError {
    McpError::invalid_params(fehler.to_string(), None)
}

fn format_fehler(fehler: FormatFehler) -> McpError {
    McpError::invalid_params(fehler.to_string(), None)
}

// ------------------------------------------------------------- Parameter

/// Werkzeuge ohne Parameter.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct LeerParam {}

/// Parameter aller Werkzeuge, die nur eine Datei brauchen.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct DateiParam {
    /// Pfad im Arbeitsordner, z. B. `ablauf.drawio`.
    pub datei: String,
}

/// Parameter von `drawio_lesen`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct LesenParam {
    /// Pfad im Arbeitsordner.
    pub datei: String,
    /// Seite als Id oder Name; ohne Angabe die erste.
    #[serde(default)]
    pub seite: Option<String>,
    /// Den Dateitext selbst liefern statt der Beschreibung.
    #[serde(default)]
    pub roh: bool,
    /// Höchstzahl Zeichen für `roh` (Standard 4000).
    #[serde(default)]
    pub max_zeichen: Option<usize>,
}

/// Parameter von `drawio_bild`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct BildParam {
    /// Pfad im Arbeitsordner.
    pub datei: String,
    /// Seite als Id oder Name; ohne Angabe die erste.
    #[serde(default)]
    pub seite: Option<String>,
    /// Längste Bildkante in Pixeln (Standard 1200, Grenzen 16–2000).
    #[serde(default)]
    pub max_kante: Option<u32>,
}

/// Parameter von `drawio_erstellen`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ErstellenParam {
    /// Pfad im Arbeitsordner; die Endung `.drawio` ist Pflicht.
    pub datei: String,
    /// Name der ersten Seite; ohne Angabe der Dateiname.
    #[serde(default)]
    pub seite: Option<String>,
    /// Modell: `{"richtung":"unten|rechts","knoten":[{"id","text","form","farbe","gruppe"}],
    /// "kanten":[{"von","nach","text","art"}],"gruppen":[{"id","titel"}]}`.
    #[schemars(with = "serde_json::Map<String, Value>")]
    pub modell: Value,
}

/// Parameter von `drawio_aus_quelltext`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct QuelltextParam {
    /// Pfad im Arbeitsordner; die Endung `.drawio` ist Pflicht.
    pub datei: String,
    /// Name der Seite, wenn der Quelltext nur ein `<mxGraphModel>` ist.
    #[serde(default)]
    pub seite: Option<String>,
    /// Fertiges draw.io-XML: ein `<mxfile>` oder ein `<mxGraphModel>`.
    pub quelltext: String,
}

/// Eine Änderung an einer Zelle.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct AenderungParam {
    /// Id der Zelle (aus `drawio_lesen`).
    pub id: String,
    /// Neuer Text (wird als Text maskiert, nicht als HTML gedeutet).
    #[serde(default)]
    pub text: Option<String>,
    /// Füllfarbe: `blau`, `gruen`, `rot`, `gelb`, `grau`, `violett`, `orange`, `tuerkis`.
    #[serde(default)]
    pub farbe: Option<String>,
    /// Form: `rechteck`, `raute`, `ellipse`, `zylinder`, `notiz`, `text`, `schwimmbahn`.
    #[serde(default)]
    pub form: Option<String>,
    /// Neues x (Knoten).
    #[serde(default)]
    pub x: Option<f32>,
    /// Neues y (Knoten).
    #[serde(default)]
    pub y: Option<f32>,
    /// Neue Breite (Knoten).
    #[serde(default)]
    pub breite: Option<f32>,
    /// Neue Höhe (Knoten).
    #[serde(default)]
    pub hoehe: Option<f32>,
    /// Neuer Startknoten (Kante); leerer Text löst die Verbindung.
    #[serde(default)]
    pub quelle: Option<String>,
    /// Neuer Zielknoten (Kante); leerer Text löst die Verbindung.
    #[serde(default)]
    pub ziel: Option<String>,
}

/// Parameter von `drawio_elemente_aendern`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct AendernParam {
    /// Pfad im Arbeitsordner.
    pub datei: String,
    /// Seite als Id oder Name; ohne Angabe die erste.
    #[serde(default)]
    pub seite: Option<String>,
    /// `stand` aus `drawio_lesen` (oder aus der letzten Änderung).
    pub stand: String,
    /// Die Änderungen.
    pub elemente: Vec<AenderungParam>,
}

/// Eine neue Zelle.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct NeuesParam {
    /// Gewünschte Id; ohne Angabe vergibt das Werkzeug `n1`, `n2`, …
    #[serde(default)]
    pub id: Option<String>,
    /// Beschriftung als Text.
    #[serde(default)]
    pub text: Option<String>,
    /// Form (Standard `rechteck`).
    #[serde(default)]
    pub form: Option<String>,
    /// Füllfarbe aus der festen Liste.
    #[serde(default)]
    pub farbe: Option<String>,
    /// x; ohne Angabe unter den vorhandenen Knoten.
    #[serde(default)]
    pub x: Option<f32>,
    /// y; ohne Angabe unter den vorhandenen Knoten.
    #[serde(default)]
    pub y: Option<f32>,
    /// Breite (Standard 120).
    #[serde(default)]
    pub breite: Option<f32>,
    /// Höhe (Standard 60).
    #[serde(default)]
    pub hoehe: Option<f32>,
    /// Elternteil (Ebene, Gruppe, Container); Standard ist die Ebene `1`.
    #[serde(default)]
    pub eltern: Option<String>,
    /// Startknoten – gesetzt heißt: es wird eine Kante.
    #[serde(default)]
    pub quelle: Option<String>,
    /// Zielknoten.
    #[serde(default)]
    pub ziel: Option<String>,
    /// Gestrichelte Linie (Kante).
    #[serde(default)]
    pub gestrichelt: Option<bool>,
    /// Pfeilspitze am Ziel (Kante, Standard an).
    #[serde(default)]
    pub pfeil: Option<bool>,
}

/// Parameter von `drawio_elemente_hinzufuegen`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct HinzufuegenParam {
    /// Pfad im Arbeitsordner.
    pub datei: String,
    /// Seite als Id oder Name; ohne Angabe die erste.
    #[serde(default)]
    pub seite: Option<String>,
    /// `stand` aus `drawio_lesen`.
    pub stand: String,
    /// Die neuen Zellen.
    pub elemente: Vec<NeuesParam>,
}

/// Parameter von `drawio_elemente_entfernen`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct EntfernenParam {
    /// Pfad im Arbeitsordner.
    pub datei: String,
    /// Seite als Id oder Name; ohne Angabe die erste.
    #[serde(default)]
    pub seite: Option<String>,
    /// `stand` aus `drawio_lesen`.
    pub stand: String,
    /// Die Ids, die weg sollen.
    pub ids: Vec<String>,
    /// Kanten an diesen Zellen: `entfernen` (mit) oder `loesen` (behalten, Verbindung lösen).
    #[serde(default)]
    pub kanten: Option<String>,
    /// Kinder dieser Zellen: `entfernen` (mit) oder `behalten` (hochziehen).
    #[serde(default)]
    pub kinder: Option<String>,
}

/// Parameter von `drawio_seite_hinzufuegen`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct SeiteHinzufuegenParam {
    /// Pfad im Arbeitsordner.
    pub datei: String,
    /// `stand` aus `drawio_lesen`.
    pub stand: String,
    /// Anzeigename der neuen Seite.
    pub name: String,
    /// Gewünschte Seiten-Id; ohne Angabe `seite-1`, `seite-2`, …
    #[serde(default)]
    pub id: Option<String>,
    /// Modell wie bei `drawio_erstellen`; ohne Angabe und ohne `quelltext` wird die Seite leer.
    #[serde(default)]
    #[schemars(with = "Option<serde_json::Map<String, Value>>")]
    pub modell: Option<Value>,
    /// Fertiges `<mxGraphModel>`-XML statt eines Modells.
    #[serde(default)]
    pub quelltext: Option<String>,
}

/// Parameter von `drawio_exportieren`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ExportParam {
    /// Pfad im Arbeitsordner.
    pub datei: String,
    /// Seite als Id oder Name; ohne Angabe die erste.
    #[serde(default)]
    pub seite: Option<String>,
    /// Zielpfad im Arbeitsordner mit der Endung `.svg` oder `.png`.
    pub ausgabe: String,
    /// Längste Bildkante in Pixeln für PNG (Standard 1200).
    #[serde(default)]
    pub max_kante: Option<u32>,
}

// -------------------------------------------------------------- MCP-Anker

impl ServerHandler for DrawioServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(quiver_tools_kern::mcp::server_info(&self.zugang))
            .with_instructions(
                "draw.io-Dateien (.drawio) lokal bearbeiten: Seiten, Zellen, Formen, Farben, \
                 Verbindungen und Stile lesen und ändern, als PNG zeichnen und über einen \
                 #R-Link im Browser öffnen. Es gibt keine Anmeldung und kein Netz; alles \
                 geschieht im Arbeitsordner aus dem quiver-Fenster. Vor jeder Änderung \
                 drawio_lesen (oder drawio_seiten) aufrufen: die Antwort enthält den `stand`, \
                 ohne den nichts geschrieben wird. Unbekannte Elemente, Attribute und Stile \
                 bleiben erhalten; beim Entfernen müssen Kanten (`kanten`) und Kinder (`kinder`) \
                 ausdrücklich genannt werden. Ist der Schalter „schreiben“ aus, antworten die \
                 verändernden Werkzeuge mit einem Hinweis und ohne Schreiben. Änderungen im \
                 Browser landen nicht in der lokalen Datei.",
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
            .map(CallToolResponse::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn der_seitenname_kommt_aus_dem_dateinamen() {
        assert_eq!(seitenname("ablauf.drawio"), "ablauf");
        assert_eq!(seitenname("projekt/ablauf.drawio"), "ablauf");
        assert_eq!(seitenname("ablauf.DRAWIO"), "ablauf");
        assert_eq!(seitenname(".drawio"), "Seite 1");
    }

    #[test]
    fn ein_leerer_dateiname_ist_ein_fehler() {
        assert!(datei_name("  ").is_err());
        assert_eq!(datei_name(" a.drawio ").unwrap(), "a.drawio");
    }

    #[test]
    fn ein_kaputter_stand_ist_ein_fehler() {
        assert!(stand("abc").is_err());
        assert!(stand(&"a".repeat(64)).is_ok());
    }

    #[test]
    fn farben_und_formen_werden_geprueft() {
        assert_eq!(farbe("Blau").unwrap(), Farbe::Blau);
        assert!(farbe("pink").is_err());
        assert_eq!(form("Raute").unwrap(), Form::Raute);
        let fehler = form("kreisel").unwrap_err().to_string();
        assert!(fehler.contains("schwimmbahn"), "{fehler}");
    }

    #[test]
    fn ein_quelltext_mit_deklaration_und_modell_wird_zu_einer_datei() {
        let text = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><mxGraphModel><root>\
                    <mxCell id=\"0\"/><mxCell id=\"1\" parent=\"0\"/>\
                    </root></mxGraphModel>";
        let datei = bau_aus_quelltext(text, Some("Start"), "a.drawio").unwrap();
        let seiten = datei.seiten().unwrap();
        assert_eq!(seiten.len(), 1);
        assert_eq!(seiten[0].name, "Start");
        assert_eq!(seiten[0].zellen, 2);
    }

    #[test]
    fn ein_quelltext_mit_falscher_wurzel_wird_abgelehnt() {
        let fehler = modell_aus_quelltext("<root/>").unwrap_err().to_string();
        assert!(fehler.contains("mxGraphModel"), "{fehler}");
    }

    #[test]
    fn ein_mxfile_mit_einer_seite_wird_als_modell_gelesen() {
        let text = "<mxfile><diagram id=\"a\" name=\"A\"><mxGraphModel><root>\
                    <mxCell id=\"0\"/><mxCell id=\"1\" parent=\"0\"/>\
                    </root></mxGraphModel></diagram></mxfile>";
        let modell = modell_aus_quelltext(text).unwrap();
        assert_eq!(modell.name, "mxGraphModel");
    }
}
