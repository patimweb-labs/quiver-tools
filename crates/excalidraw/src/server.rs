// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der MCP-Server: elf Werkzeuge über stdio (Aufgabe 17).
//!
//! | Werkzeug | ändert | Hinweis |
//! |---|---|---|
//! | `excalidraw_status` | nein | Arbeitsordner, Anzahl, Schalter |
//! | `excalidraw_liste` | nein | alle Dateien mit Größe und Zeit |
//! | `excalidraw_lesen` | nein | Ids, Arten, Bindungen, `stand`; `roh: true` für den Text |
//! | `excalidraw_bild` | nein | PNG als Bildinhalt, nennt was nicht gezeichnet wurde |
//! | `excalidraw_oeffnen` | nein | öffnet excalidraw.com, lädt nichts hoch |
//! | `excalidraw_erstellen` | ja | aus `modell` (Knoten, Kanten, Gruppen) |
//! | `excalidraw_aus_quelltext` | ja | fertiges Format, streng geprüft |
//! | `excalidraw_elemente_aendern` | ja | Text, Farbe, Form, Lage, Verbindung je Id |
//! | `excalidraw_elemente_hinzufuegen` | ja | Formen, Text, Pfeil, Rahmen, Bildplatzhalter |
//! | `excalidraw_elemente_entfernen` | ja | löst beide Seiten der Bindung auf |
//! | `excalidraw_exportieren` | ja | SVG oder PNG in den Arbeitsordner |
//!
//! Dazu kommt `excalidraw_profile` aus dem Kern. Jedes verändernde Werkzeug
//! braucht den Schalter `schreiben`; ist er aus, kommt der Hinweis und es wird
//! **nichts** geschrieben. Änderungen brauchen den `stand`, den der Agent
//! gelesen hat.

use std::any::Any;
use std::sync::Arc;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use quiver_diagramm::ablage::{Ablage, Inhalt, Stand};
use quiver_diagramm::modell::{Diagramm, Farbe, Form, KanteArt};
use quiver_tools_kern::mcp::profile_tool;
use quiver_tools_kern::{antwort, hinweis_zugang, OhneProfil, Profil, Zugang};
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

use crate::bauen::{self, Vorlage};
use crate::browser::{Browser, StandardBrowser, ADRESSE};
use crate::datei::{Datei, ENDUNG};
use crate::fehler::Fehler;
use crate::werkzeug::{ordner, schreiben_an, ORDNER, SCHREIBEN, TITEL};
use crate::{aendern, bild, farben, format, pruefen};

/// Was ein Werkzeug zurückgibt.
type Ergebnis = Result<CallToolResult, McpError>;

/// Der Excalidraw-Server: Arbeitsordner aus der Umgebung, Browser hinter einem Trait.
pub struct ExcalidrawServer {
    zugang: Zugang,
    browser: Arc<dyn Browser>,
}

impl ExcalidrawServer {
    /// Der Normalfall: Arbeitsordner aus der Umgebung, echter Browser.
    pub fn neu(zugang: Zugang) -> Self {
        ExcalidrawServer {
            zugang,
            browser: Arc::new(StandardBrowser::neu()),
        }
    }

    /// Feste Teile (Tests und Sonderfälle), ohne Prozessumgebung.
    pub fn mit(zugang: Zugang, browser: Arc<dyn Browser>) -> Self {
        ExcalidrawServer { zugang, browser }
    }

    /// Alle Werkzeuge, wie `tools/list` sie meldet.
    pub fn werkzeuge(&self) -> Vec<Tool> {
        let mut alle = vec![
            lesend::<LeerParam>(
                "excalidraw_status",
                "Arbeitsordner",
                "Zeigt den Arbeitsordner, die Anzahl der Dateien und ob der Schalter „schreiben“ \
                 an ist. Öffnet keine Datei.",
            ),
            lesend::<LeerParam>(
                "excalidraw_liste",
                "Dateien",
                "Listet alle .excalidraw-Dateien im Arbeitsordner (auch in Unterordnern) mit Größe \
                 und Änderungszeit.",
            ),
            lesend::<LesenParam>(
                "excalidraw_lesen",
                "Datei lesen",
                "Beschreibt eine Datei knapp: je Element Id, Art, Text, Lage und Bindungen, dazu \
                 der `stand` für Änderungen. Mit `roh: true` kommt der Quelltext (gekürzt).",
            ),
            lesend::<BildParam>(
                "excalidraw_bild",
                "Bild ansehen",
                "Zeichnet die Datei als PNG und gibt es als Bild zurück; die Antwort nennt, was \
                 nicht gezeichnet wurde. Gezeichnet werden rectangle, diamond, ellipse, line, \
                 arrow, text, freedraw und frame; image wird zum Platzhalter.",
            ),
            oeffnend::<OeffnenParam>(
                "excalidraw_oeffnen",
                "Im Browser öffnen",
                "Öffnet https://excalidraw.com im Standardbrowser und nennt den vollen Pfad der \
                 Datei – die Datei wird in das Fenster gezogen. Es wird nichts hochgeladen. Mit \
                 `nur_link: true` wird nichts geöffnet; die Antwort nennt nur Adresse und Pfad.",
            ),
            schreibend::<ErstellenParam>(
                "excalidraw_erstellen",
                "Aus Beschreibung erstellen",
                "Legt eine neue Datei aus `modell` an: `knoten` (id, text, form, farbe?, gruppe?), \
                 `kanten` (von, nach, text?, art) und `gruppen` (id, titel), dazu `richtung`. Das \
                 Layout rechnet die Kiste; Formen bekommen ihren Text als gebundenen Text, Kanten \
                 werden zu Pfeilen mit beiden Bindungen. Überschreibt nie eine vorhandene Datei.",
            ),
            schreibend::<QuelltextParam>(
                "excalidraw_aus_quelltext",
                "Aus Quelltext schreiben",
                "Schreibt eine fertige .excalidraw-Datei aus `quelltext`. Der Text wird streng \
                 geprüft: type „excalidraw“, version 2, alle Pflichtfelder je Element und \
                 Bindungen auf beiden Seiten. Überschreibt nie eine vorhandene Datei.",
            ),
            schreibend::<AendernParam>(
                "excalidraw_elemente_aendern",
                "Elemente ändern",
                "Ändert Elemente je Id: `text`, `farbe`, `form`, `x`/`y`/`breite`/`hoehe` und \
                 `verbindungen` (verbinden oder mit `entfernen: true` lösen). Texte in Formen und \
                 Pfeile an Elementen bleiben beidseitig gebunden; Verschieben zieht sie mit. \
                 Braucht `datei` und den gelesenen `stand`.",
            ),
            schreibend::<HinzufuegenParam>(
                "excalidraw_elemente_hinzufuegen",
                "Elemente hinzufügen",
                "Fügt Elemente hinzu: rectangle, diamond, ellipse, text, line, arrow, frame, \
                 image (Platzhalter). Pfeile nennen `von` und `nach` und bekommen beide Bindungen; \
                 Formen mit `text` bekommen einen gebundenen Text.",
            ),
            schreibend::<EntfernenParam>(
                "excalidraw_elemente_entfernen",
                "Elemente entfernen",
                "Entfernt Elemente über ihre Ids und löst dabei jede Bindung in beide Richtungen \
                 auf: Texte werden frei, Pfeile verlieren das Ende, Rahmen lassen ihre Mitglieder \
                 frei.",
            ),
            schreibend::<ExportParam>(
                "excalidraw_exportieren",
                "Exportieren",
                "Schreibt die Datei als SVG oder PNG in den Arbeitsordner (`ausgabe` endet auf .svg \
                 oder .png). Vorhandene Ausgaben werden nie überschrieben.",
            ),
        ];
        alle.push(profile_tool(&self.zugang));
        alle
    }

    /// Ein Werkzeug aufrufen. Auch der Weg, den die Tests nehmen.
    pub async fn aufrufen(&self, name: &str, argumente: Option<JsonObject>) -> Ergebnis {
        match name {
            "excalidraw_status" => self.status(argumente),
            "excalidraw_liste" => self.liste(argumente),
            "excalidraw_lesen" => self.lesen(argumente),
            "excalidraw_bild" => self.bild(argumente),
            "excalidraw_oeffnen" => self.oeffnen(argumente),
            "excalidraw_erstellen" => self.erstellen(argumente),
            "excalidraw_aus_quelltext" => self.aus_quelltext(argumente),
            "excalidraw_elemente_aendern" => self.elemente_aendern(argumente),
            "excalidraw_elemente_hinzufuegen" => self.elemente_hinzufuegen(argumente),
            "excalidraw_elemente_entfernen" => self.elemente_entfernen(argumente),
            "excalidraw_exportieren" => self.exportieren(argumente),
            "excalidraw_profile" => self.profile(argumente),
            sonst => Err(McpError::invalid_params(
                format!("Unbekanntes Werkzeug „{sonst}“."),
                None,
            )),
        }
    }

    /// `excalidraw_profile`: der Zugang aus der Umgebung, ohne Geheimnisse.
    fn profile(&self, roh: Option<JsonObject>) -> Ergebnis {
        let _: LeerParam = argumente(roh)?;
        quiver_tools_kern::mcp::profile_antwort(&self.zugang)
    }

    /// Der Zugang aus der Umgebung – oder der Hinweis auf das quiver-Fenster.
    fn profil(&self) -> Result<Profil, OhneProfil> {
        quiver_tools_kern::zugang(TITEL, self.zugang.profil())
    }

    /// Arbeitsordner und Schalter, ohne den Schalter zu prüfen.
    fn ablage(&self) -> Result<(Ablage, bool), OhneProfil> {
        let profil = self.profil()?;
        let Some(ordner) = ordner(&profil) else {
            return Err(OhneProfil::Hinweis(antwort(hinweis_zugang(TITEL))));
        };
        let schreiben = schreiben_an(&profil);
        let ablage = Ablage::neu(ordner, ENDUNG).map_err(|fehler| {
            OhneProfil::Fehler(McpError::invalid_params(fehler.to_string(), None))
        })?;
        Ok((ablage, schreiben))
    }

    /// Der Arbeitsordner – aber nur, wenn der Schalter „schreiben“ an ist.
    fn schreiben(&self) -> Result<Ablage, OhneProfil> {
        let (ablage, schreiben) = self.ablage()?;
        if !schreiben {
            return Err(OhneProfil::Hinweis(antwort(format::schreiben_aus(TITEL))));
        }
        Ok(ablage)
    }

    // ------------------------------------------------------------- lesend

    fn status(&self, roh: Option<JsonObject>) -> Ergebnis {
        let _: LeerParam = argumente(roh)?;
        let (ablage, schreiben) = match self.ablage() {
            Ok(teile) => teile,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let anzahl = ablage.liste().map_err(fehler_zu_mcp)?.len();
        Ok(antwort(format::status(ablage.wurzel(), schreiben, anzahl)))
    }

    fn liste(&self, roh: Option<JsonObject>) -> Ergebnis {
        let _: LeerParam = argumente(roh)?;
        let (ablage, _) = match self.ablage() {
            Ok(teile) => teile,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        Ok(antwort(format::liste(&ablage).map_err(fehler_zu_mcp)?))
    }

    fn lesen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: LesenParam = argumente(roh)?;
        let (ablage, _) = match self.ablage() {
            Ok(teile) => teile,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let inhalt = ablage.lesen(&p.datei).map_err(fehler_zu_mcp)?;
        if p.roh {
            let max = p
                .max_zeichen
                .map(|zahl| (zahl as usize).clamp(100, 200_000))
                .unwrap_or(format::MAX_ROH);
            return Ok(antwort(format::roh(&inhalt.text, max)));
        }
        let datei = match Datei::aus_text(&inhalt.text) {
            Ok(datei) => datei,
            Err(fehler) => {
                return Ok(antwort(format!(
                    "{fehler}\nMit `roh: true` zeigt `excalidraw_lesen` den Quelltext."
                )))
            }
        };
        Ok(antwort(format::lesen(
            &p.datei,
            &inhalt,
            &datei,
            ablage.wurzel(),
        )))
    }

    fn bild(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: BildParam = argumente(roh)?;
        let (ablage, _) = match self.ablage() {
            Ok(teile) => teile,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let datei = self.datei(&ablage, &p.datei)?;
        let gezeichnet = bild::bild(&datei);
        let kante = p.max_kante.unwrap_or(bild::STANDARD_KANTE);
        let png = gezeichnet.png(kante).map_err(fehler_zu_mcp)?;
        Ok(CallToolResult::success(vec![
            ContentBlock::text(format::bild(&gezeichnet, kante)),
            ContentBlock::image(BASE64.encode(&png), "image/png"),
        ]))
    }

    fn oeffnen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: OeffnenParam = argumente(roh)?;
        let (ablage, _) = match self.ablage() {
            Ok(teile) => teile,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let voll = ablage.pfad(&p.datei).map_err(fehler_zu_mcp)?;
        // Der Pfad muss auf eine lesbare Datei zeigen.
        ablage.stand(&p.datei).map_err(fehler_zu_mcp)?;

        if p.nur_link {
            return Ok(antwort(format!(
                "Nichts geöffnet (`nur_link`).\nAdresse: {ADRESSE}\nDatei: {}\n\
                 Öffne die Adresse und ziehe die Datei in das Fenster.",
                voll.display()
            )));
        }
        match self.browser.oeffnen(ADRESSE) {
            Ok(()) => Ok(antwort(format!(
                "Excalidraw ist im Browser geöffnet: {ADRESSE}\nDatei: {}\n\
                 Ziehe die Datei in das Fenster (Menü → Öffnen). Excalidraw lädt nur hoch, wenn \
                 du teilst.",
                voll.display()
            ))),
            Err(fehler) => Ok(antwort(format!(
                "Der Browser ließ sich nicht öffnen ({fehler}).\nÖffne {ADRESSE} von Hand und \
                 ziehe diese Datei hinein: {}",
                voll.display()
            ))),
        }
    }

    // -------------------------------------------------------------- schreibend

    fn erstellen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: ErstellenParam = argumente(roh)?;
        let ablage = match self.schreiben() {
            Ok(ablage) => ablage,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let diagramm = Diagramm::aus_json(&p.modell.to_string()).map_err(fehler_zu_mcp)?;
        let datei = bauen::aus_diagramm(&diagramm).map_err(fehler_zu_mcp)?;
        Selbstpruefung::pruefe(&datei)?;
        let text = datei.als_text().map_err(fehler_zu_mcp)?;
        let stand = ablage.anlegen(&p.datei, &text).map_err(fehler_zu_mcp)?;
        Ok(antwort(format!(
            "`{}` angelegt: {}.\nStand: {stand}\n\n{}",
            p.datei,
            format::anzahl_text(&datei),
            format::beschreibung(&datei)
        )))
    }

    fn aus_quelltext(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: QuelltextParam = argumente(roh)?;
        let ablage = match self.schreiben() {
            Ok(ablage) => ablage,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let datei = pruefen::pruefen_und_lesen(&p.quelltext).map_err(fehler_zu_mcp)?;
        let stand = ablage
            .anlegen(&p.datei, &p.quelltext)
            .map_err(fehler_zu_mcp)?;
        Ok(antwort(format!(
            "`{}` geschrieben: {}.\nStand: {stand}",
            p.datei,
            format::anzahl_text(&datei)
        )))
    }

    fn elemente_aendern(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: AendernParam = argumente(roh)?;
        let ablage = match self.schreiben() {
            Ok(ablage) => ablage,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let stand = stand_aus(&p.stand)?;
        if p.elemente.is_empty() && p.verbindungen.is_empty() {
            return Err(McpError::invalid_params(
                "Nichts zu ändern – `elemente` und `verbindungen` sind leer.",
                None,
            ));
        }
        let (inhalt, mut datei) = self.lesen_und_pruefen(&ablage, &p.datei)?;

        let mut meldungen = Vec::new();
        for aenderung in &p.elemente {
            let id = aenderung.id.trim();
            if let Some(name) = &aenderung.farbe {
                let farbe = farbe_aus(name)?;
                aendern::farbe_setzen(&mut datei, id, farbe).map_err(fehler_zu_mcp)?;
                meldungen.push(format!("Farbe von „{id}“ ist jetzt {name}"));
            }
            if let Some(name) = &aenderung.form {
                let form = form_aus(name)?;
                aendern::form_setzen(&mut datei, id, form).map_err(fehler_zu_mcp)?;
                meldungen.push(format!("Form von „{id}“ ist jetzt {name}"));
            }
            if let Some(text) = &aenderung.text {
                meldungen.push(aendern::text_setzen(&mut datei, id, text).map_err(fehler_zu_mcp)?);
            }
            if aenderung.x.is_some()
                || aenderung.y.is_some()
                || aenderung.breite.is_some()
                || aenderung.hoehe.is_some()
            {
                aendern::verschieben(
                    &mut datei,
                    id,
                    aenderung.x,
                    aenderung.y,
                    aenderung.breite,
                    aenderung.hoehe,
                )
                .map_err(fehler_zu_mcp)?;
                meldungen.push(format!("Lage von „{id}“ gesetzt"));
            }
        }
        for verbindung in &p.verbindungen {
            if verbindung.entfernen {
                let anzahl = aendern::loesen(&mut datei, &verbindung.von, &verbindung.nach)
                    .map_err(fehler_zu_mcp)?;
                meldungen.push(if anzahl == 0 {
                    format!(
                        "Zwischen „{}“ und „{}“ gab es keine Verbindung",
                        verbindung.von, verbindung.nach
                    )
                } else {
                    format!(
                        "Verbindung zwischen „{}“ und „{}“ entfernt",
                        verbindung.von, verbindung.nach
                    )
                });
            } else {
                let art = match &verbindung.art {
                    Some(name) => kante_aus(name)?,
                    None => KanteArt::Pfeil,
                };
                meldungen.push(
                    aendern::verbinden(&mut datei, &verbindung.von, &verbindung.nach, art, None)
                        .map_err(fehler_zu_mcp)?,
                );
            }
        }

        Selbstpruefung::pruefe(&datei)?;
        let neu = self.zurueckschreiben(&ablage, &p.datei, &datei, &stand)?;
        Ok(antwort(format!(
            "{}\n\n`{}` gespeichert, {} (vorher {}).\nStand: {neu}",
            meldungen.join("\n"),
            p.datei,
            format::anzahl_text(&datei),
            format::anzahl_text(&Datei::aus_text(&inhalt.text).unwrap_or_default()),
        )))
    }

    fn elemente_hinzufuegen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: HinzufuegenParam = argumente(roh)?;
        let ablage = match self.schreiben() {
            Ok(ablage) => ablage,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let stand = stand_aus(&p.stand)?;
        if p.elemente.is_empty() {
            return Err(McpError::invalid_params(
                "Nichts hinzuzufügen – `elemente` ist leer.",
                None,
            ));
        }
        let (_, mut datei) = self.lesen_und_pruefen(&ablage, &p.datei)?;

        let mut meldungen = Vec::new();
        for neues in &p.elemente {
            let vorlage = vorlage_aus(neues)?;
            meldungen.push(bauen::hinzufuegen(&mut datei, &vorlage).map_err(fehler_zu_mcp)?);
        }

        Selbstpruefung::pruefe(&datei)?;
        let neu = self.zurueckschreiben(&ablage, &p.datei, &datei, &stand)?;
        Ok(antwort(format!(
            "{}\n\n`{}` gespeichert: {}.\nStand: {neu}",
            meldungen.join("\n"),
            p.datei,
            format::anzahl_text(&datei)
        )))
    }

    fn elemente_entfernen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: EntfernenParam = argumente(roh)?;
        let ablage = match self.schreiben() {
            Ok(ablage) => ablage,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let stand = stand_aus(&p.stand)?;
        if p.ids.is_empty() {
            return Err(McpError::invalid_params(
                "Nichts zu entfernen – `ids` ist leer.",
                None,
            ));
        }
        let (_, mut datei) = self.lesen_und_pruefen(&ablage, &p.datei)?;

        let mut meldungen = Vec::new();
        for id in &p.ids {
            let geloest = aendern::entfernen(&mut datei, id.trim()).map_err(fehler_zu_mcp)?;
            meldungen.push(if geloest == 0 {
                format!("„{id}“ entfernt (keine Bindung offen)")
            } else {
                format!("„{id}“ entfernt, {geloest} Bindung(en) aufgelöst")
            });
        }

        Selbstpruefung::pruefe(&datei)?;
        let neu = self.zurueckschreiben(&ablage, &p.datei, &datei, &stand)?;
        Ok(antwort(format!(
            "{}\n\n`{}` gespeichert: {}.\nStand: {neu}",
            meldungen.join("\n"),
            p.datei,
            format::anzahl_text(&datei)
        )))
    }

    fn exportieren(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: ExportParam = argumente(roh)?;
        let ablage = match self.schreiben() {
            Ok(ablage) => ablage,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let datei = self.datei(&ablage, &p.datei)?;
        let gezeichnet = bild::bild(&datei);
        let kante = p.max_kante.unwrap_or(bild::STANDARD_KANTE);
        let ziel = p.ausgabe.trim();
        let klein = ziel.to_ascii_lowercase();
        let groesse = if klein.ends_with(".svg") {
            let svg = gezeichnet.svg();
            ablage
                .ausgabe_schreiben(ziel, svg.as_bytes())
                .map_err(fehler_zu_mcp)?;
            svg.len() as u64
        } else if klein.ends_with(".png") {
            let png = gezeichnet.png(kante).map_err(fehler_zu_mcp)?;
            ablage
                .ausgabe_schreiben(ziel, &png)
                .map_err(fehler_zu_mcp)?;
            png.len() as u64
        } else {
            return Err(fehler_zu_mcp(Fehler::KeinBild(ziel.to_string())));
        };

        let voll = ablage.ausgabe_pfad(ziel).map_err(fehler_zu_mcp)?;
        let mut text = format!(
            "`{ziel}` geschrieben ({}, {}).",
            format::groesse(groesse),
            bild::bild(&datei).gezeichnet
        );
        if !gezeichnet.nicht_gezeichnet.is_empty() {
            text.push_str("\nNicht gezeichnet: ");
            text.push_str(&gezeichnet.nicht_gezeichnet.join(", "));
        }
        text.push_str(&format!("\nVoller Pfad: {}", voll.display()));
        Ok(antwort(text))
    }

    // ------------------------------------------------------------- Helfer

    /// Eine Datei lesen und als gültiges Format auslegen.
    fn datei(&self, ablage: &Ablage, pfad: &str) -> Result<Datei, McpError> {
        let inhalt = ablage.lesen(pfad).map_err(fehler_zu_mcp)?;
        Datei::aus_text(&inhalt.text).map_err(fehler_zu_mcp)
    }

    /// Eine Datei lesen, prüfen und zum Ändern zurückgeben.
    fn lesen_und_pruefen(&self, ablage: &Ablage, pfad: &str) -> Result<(Inhalt, Datei), McpError> {
        let inhalt: Inhalt = ablage.lesen(pfad).map_err(fehler_zu_mcp)?;
        let roh: Value = serde_json::from_str(&inhalt.text)
            .map_err(|fehler| fehler_zu_mcp(Fehler::Json(fehler.to_string())))?;
        if let Err(meldungen) = pruefen::pruefen(&roh) {
            return Err(McpError::invalid_params(
                format!(
                    "`{pfad}` ist nicht gültig – bitte mit `excalidraw_lesen` ansehen:\n{}",
                    pruefen::meldungstext(&meldungen)
                ),
                None,
            ));
        }
        let datei = Datei::aus_text(&inhalt.text).map_err(fehler_zu_mcp)?;
        Ok((inhalt, datei))
    }

    /// Die geänderte Datei mit dem gelesenen `stand` zurückschreiben.
    fn zurueckschreiben(
        &self,
        ablage: &Ablage,
        pfad: &str,
        datei: &Datei,
        stand: &Stand,
    ) -> Result<Stand, McpError> {
        let text = datei.als_text().map_err(fehler_zu_mcp)?;
        ablage.aendern(pfad, &text, stand).map_err(fehler_zu_mcp)
    }
}

/// Prüft die eigene Ausgabe gegen die Regeln aus [`pruefen`].
struct Selbstpruefung;

impl Selbstpruefung {
    fn pruefe(datei: &Datei) -> Result<(), McpError> {
        let roh = serde_json::to_value(datei).map_err(|fehler| {
            McpError::internal_error(format!("Eigene Datei nicht schreibbar: {fehler}"), None)
        })?;
        pruefen::pruefen(&roh).map_err(|meldungen| {
            McpError::internal_error(
                format!(
                    "Die eigene Ausgabe ist nicht gültig (Fehler im Werkzeug): {}",
                    pruefen::meldungstext(&meldungen)
                ),
                None,
            )
        })
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

/// Ein veränderndes Werkzeug (`destructive_hint`).
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

/// Ein Werkzeug, das etwas öffnet oder schreibt, ohne etwas zu zerstören.
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

/// Ein Fehler des Werkzeugs als MCP-Fehler (die Meldung ist für den Agenten).
fn fehler_zu_mcp(fehler: impl std::fmt::Display) -> McpError {
    McpError::invalid_params(fehler.to_string(), None)
}

/// Der `stand` aus einem Aufruf.
fn stand_aus(text: &str) -> Result<Stand, McpError> {
    Stand::aus_hex(text).ok_or_else(|| {
        McpError::invalid_params(
            "`stand` ist kein SHA-256 (64 Hexzeichen) – bitte mit `excalidraw_lesen` neu lesen.",
            None,
        )
    })
}

/// Ein Farbname aus dem Aufruf.
fn farbe_aus(name: &str) -> Result<Farbe, McpError> {
    farben::farbe_aus_name(name).ok_or_else(|| {
        McpError::invalid_params(
            format!(
                "Farbe „{name}“ kennt das Werkzeug nicht – erlaubt sind: {}",
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

/// Ein Formname aus dem Aufruf.
fn form_aus(name: &str) -> Result<Form, McpError> {
    Form::aus_name(name).ok_or_else(|| {
        McpError::invalid_params(
            format!(
                "Form „{name}“ kennt das Werkzeug nicht – erlaubt sind: {}",
                Form::ALLE
                    .iter()
                    .map(|form| form.name())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            None,
        )
    })
}

/// Ein Kantenname aus dem Aufruf.
fn kante_aus(name: &str) -> Result<KanteArt, McpError> {
    KanteArt::aus_name(name).ok_or_else(|| {
        McpError::invalid_params(
            format!(
                "Kante „{name}“ kennt das Werkzeug nicht – erlaubt sind: {}",
                KanteArt::ALLE
                    .iter()
                    .map(|art| art.name())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            None,
        )
    })
}

/// Ein neues Element aus den Parametern.
fn vorlage_aus(neues: &NeuesParam) -> Result<Vorlage, McpError> {
    let farbe = match &neues.farbe {
        Some(name) => Some(farbe_aus(name)?),
        None => None,
    };
    let kante = match &neues.kante {
        Some(name) => Some(kante_aus(name)?),
        None => None,
    };
    Ok(Vorlage {
        art: neues.art.clone(),
        id: neues.id.clone(),
        text: neues.text.clone(),
        x: neues.x,
        y: neues.y,
        breite: neues.breite,
        hoehe: neues.hoehe,
        farbe,
        titel: neues.titel.clone(),
        von: neues.von.clone(),
        nach: neues.nach.clone(),
        kante,
    })
}

// -------------------------------------------------------------- Parameter

/// Keine Argumente.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct LeerParam {}

/// `excalidraw_lesen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct LesenParam {
    /// Pfad im Arbeitsordner, z. B. `plan.excalidraw`.
    pub datei: String,
    /// Den Quelltext statt der Beschreibung zeigen (gekürzt).
    #[serde(default)]
    pub roh: bool,
    /// Höchstzahl Zeichen für `roh` (Standard 20000).
    #[serde(default)]
    pub max_zeichen: Option<u32>,
}

/// `excalidraw_bild`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct BildParam {
    /// Pfad im Arbeitsordner.
    pub datei: String,
    /// Größte Kantenlänge des PNG in Pixeln (Standard 1600, höchstens 2000).
    #[serde(default)]
    pub max_kante: Option<u32>,
}

/// `excalidraw_oeffnen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct OeffnenParam {
    /// Pfad im Arbeitsordner.
    pub datei: String,
    /// Nichts öffnen, nur Adresse und Pfad nennen.
    #[serde(default)]
    pub nur_link: bool,
}

/// `excalidraw_erstellen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ErstellenParam {
    /// Pfad im Arbeitsordner; die Datei darf es noch nicht geben.
    pub datei: String,
    /// Das Modell: `{ richtung, knoten: [...], kanten: [...], gruppen: [...] }`.
    #[schemars(with = "serde_json::Map<String, Value>")]
    pub modell: Value,
}

/// `excalidraw_aus_quelltext`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct QuelltextParam {
    /// Pfad im Arbeitsordner; die Datei darf es noch nicht geben.
    pub datei: String,
    /// Die fertige Datei als JSON.
    pub quelltext: String,
}

/// Eine Änderung an einem Element.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AenderungParam {
    /// Id des Elements (aus `excalidraw_lesen`).
    pub id: String,
    /// Neuer Text (bei einer Form: der Text darin).
    #[serde(default)]
    pub text: Option<String>,
    /// Neue Füllfarbe: blau, gruen, rot, gelb, grau, violett, orange, tuerkis.
    #[serde(default)]
    pub farbe: Option<String>,
    /// Neue Form: rechteck, raute, ellipse (auch zylinder, notiz → Rechteck).
    #[serde(default)]
    pub form: Option<String>,
    /// Neue linke Kante.
    #[serde(default)]
    pub x: Option<f64>,
    /// Neue Oberkante.
    #[serde(default)]
    pub y: Option<f64>,
    /// Neue Breite.
    #[serde(default)]
    pub breite: Option<f64>,
    /// Neue Höhe.
    #[serde(default)]
    pub hoehe: Option<f64>,
}

/// Eine Verbindung zwischen zwei Elementen.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct VerbindungParam {
    /// Id des Ausgangselements.
    pub von: String,
    /// Id des Zielelements.
    pub nach: String,
    /// Art: pfeil, linie, gestrichelt (Standard pfeil).
    #[serde(default)]
    pub art: Option<String>,
    /// Die Verbindung löschen statt anlegen (entfernt den Pfeil).
    #[serde(default)]
    pub entfernen: bool,
}

/// `excalidraw_elemente_aendern`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AendernParam {
    /// Pfad im Arbeitsordner.
    pub datei: String,
    /// Der zuletzt gelesene `stand` (SHA-256).
    pub stand: String,
    /// Änderungen je Element.
    #[serde(default)]
    pub elemente: Vec<AenderungParam>,
    /// Verbindungen anlegen oder löschen.
    #[serde(default)]
    pub verbindungen: Vec<VerbindungParam>,
}

/// Ein neues Element.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct NeuesParam {
    /// Art: rectangle, diamond, ellipse, text, line, arrow, frame, image.
    pub art: String,
    /// Gewünschte Id; ohne Angabe wird eine freie gewählt.
    #[serde(default)]
    pub id: Option<String>,
    /// Text im Element oder als Beschriftung am Pfeil.
    #[serde(default)]
    pub text: Option<String>,
    /// Linke Kante; ohne Angabe unter dem vorhandenen Inhalt.
    #[serde(default)]
    pub x: Option<f64>,
    /// Oberkante.
    #[serde(default)]
    pub y: Option<f64>,
    /// Breite.
    #[serde(default)]
    pub breite: Option<f64>,
    /// Höhe.
    #[serde(default)]
    pub hoehe: Option<f64>,
    /// Füllfarbe aus der festen Liste.
    #[serde(default)]
    pub farbe: Option<String>,
    /// Überschrift eines Rahmens.
    #[serde(default)]
    pub titel: Option<String>,
    /// Ausgangselement eines Pfeils oder einer Linie.
    #[serde(default)]
    pub von: Option<String>,
    /// Zielelement eines Pfeils oder einer Linie.
    #[serde(default)]
    pub nach: Option<String>,
    /// Art der Kante: pfeil, linie, gestrichelt.
    #[serde(default)]
    pub kante: Option<String>,
}

/// `excalidraw_elemente_hinzufuegen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct HinzufuegenParam {
    /// Pfad im Arbeitsordner.
    pub datei: String,
    /// Der zuletzt gelesene `stand` (SHA-256).
    pub stand: String,
    /// Die neuen Elemente.
    pub elemente: Vec<NeuesParam>,
}

/// `excalidraw_elemente_entfernen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct EntfernenParam {
    /// Pfad im Arbeitsordner.
    pub datei: String,
    /// Der zuletzt gelesene `stand` (SHA-256).
    pub stand: String,
    /// Ids der zu entfernenden Elemente.
    pub ids: Vec<String>,
}

/// `excalidraw_exportieren`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExportParam {
    /// Pfad der Datei im Arbeitsordner.
    pub datei: String,
    /// Ziel im Arbeitsordner, mit `.svg` oder `.png`.
    pub ausgabe: String,
    /// Größte Kantenlänge des PNG (Standard 1600, höchstens 2000).
    #[serde(default)]
    pub max_kante: Option<u32>,
}

// -------------------------------------------------------------- MCP-Anker

impl ServerHandler for ExcalidrawServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(quiver_tools_kern::mcp::server_info(&self.zugang))
            .with_instructions(format!(
                "Excalidraw-Dateien (.excalidraw) lokal im Arbeitsordner: aus einer Beschreibung \
                 erzeugen, lesen, Elemente ändern, hinzufügen, entfernen, als Bild ansehen und \
                 exportieren. Alles ohne Anmeldung, ohne Netz und ohne Browser im Hintergrund. \
                 Der Arbeitsordner ({ORDNER}) und der Schalter „{SCHREIBEN}“ stehen im \
                 quiver-Fenster unter Tools → {TITEL}; ohne Ordner antworten die Werkzeuge mit \
                 dem Hinweis. Änderungen brauchen den `stand` aus `excalidraw_lesen` – stimmt er \
                 nicht, wird nichts geschrieben. Gezeichnet wird selbst mit der eingebetteten \
                 Schrift; `excalidraw_bild` nennt, was es nicht zeichnen kann. Öffnen und Teilen \
                 auf excalidraw.com lädt Inhalte hoch – darum öffnet `excalidraw_oeffnen` nur die \
                 leere Seite, und die Datei wird von Hand hineingezogen."
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
    use crate::browser::KeinBrowser;
    use crate::werkzeug::Excalidraw;
    use serde_json::json;
    use std::collections::BTreeMap;

    /// Ein Server mit festem Arbeitsordner; der Browser tut nichts.
    fn server(ordner: Option<&str>, schreiben: bool) -> ExcalidrawServer {
        let schluessel = |feld: &str| quiver_tools_kern::umgebung::schluessel("excalidraw", feld);
        let werte: BTreeMap<String, String> = match ordner {
            Some(ordner) => [
                (schluessel(ORDNER), ordner.to_string()),
                (
                    schluessel(SCHREIBEN),
                    if schreiben { "an" } else { "aus" }.to_string(),
                ),
            ]
            .into_iter()
            .collect(),
            None => BTreeMap::new(),
        };
        ExcalidrawServer::mit(
            Zugang::mit_umgebung(&Excalidraw, |name| werte.get(name).cloned()),
            Arc::new(KeinBrowser::neu()),
        )
    }

    fn ohne_zugang() -> ExcalidrawServer {
        ExcalidrawServer::mit(
            Zugang::mit_umgebung(&Excalidraw, |_| None),
            Arc::new(KeinBrowser::neu()),
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

    fn aufruf(server: &ExcalidrawServer, name: &str, argumente: Value) -> Ergebnis {
        let laufzeit = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let objekt = argumente.as_object().cloned().unwrap_or_default();
        laufzeit.block_on(server.aufrufen(name, Some(objekt)))
    }

    #[test]
    fn alle_werkzeuge_tragen_den_richtigen_hinweis() {
        let server = ohne_zugang();
        let werkzeuge = server.werkzeuge();
        assert_eq!(werkzeuge.len(), 12, "elf Werkzeuge plus excalidraw_profile");

        let schreibend = [
            "excalidraw_erstellen",
            "excalidraw_aus_quelltext",
            "excalidraw_elemente_aendern",
            "excalidraw_elemente_hinzufuegen",
            "excalidraw_elemente_entfernen",
            "excalidraw_exportieren",
        ];
        for werkzeug in &werkzeuge {
            let hinweise = werkzeug.annotations.as_ref().expect("Hinweise fehlen");
            let name = werkzeug.name.as_ref();
            if name == "excalidraw_profile" || name == "excalidraw_oeffnen" {
                continue;
            }
            let destruktiv = schreibend.contains(&name);
            assert_eq!(hinweise.read_only_hint, Some(!destruktiv), "{name}");
            assert_eq!(hinweise.destructive_hint, Some(destruktiv), "{name}");
            assert!(werkzeug.title.is_some(), "{name} hat keinen Titel");
        }
        let namen: Vec<&str> = werkzeuge.iter().map(|w| w.name.as_ref()).collect();
        assert_eq!(namen[0], "excalidraw_status");
        assert_eq!(namen[11], "excalidraw_profile");
    }

    #[test]
    fn ohne_ordner_kommt_der_hinweis_und_kein_fehler() {
        let server = ohne_zugang();
        for (name, argumente) in [
            ("excalidraw_status", json!({})),
            ("excalidraw_liste", json!({})),
            ("excalidraw_lesen", json!({ "datei": "x.excalidraw" })),
        ] {
            let text = text_von(aufruf(&server, name, argumente));
            assert!(text.contains("quiver-Fenster"), "{name}: {text}");
        }
    }

    #[test]
    fn mit_arbeitsordner_zaehlt_status_die_dateien() {
        let ordner = tempfile::tempdir().expect("temporärer Ordner");
        let pfad = ordner.path().to_string_lossy().to_string();
        let server = server(Some(&pfad), true);

        let status = text_von(aufruf(&server, "excalidraw_status", json!({})));
        assert!(status.contains("0 Datei"), "{status}");
        assert!(status.contains("Schreiben ist an"), "{status}");

        let antwort = text_von(aufruf(
            &server,
            "excalidraw_erstellen",
            json!({ "datei": "plan.excalidraw", "modell": { "knoten": [ { "id": "a", "text": "A" } ] } }),
        ));
        assert!(antwort.contains("Stand: "), "{antwort}");

        let liste = text_von(aufruf(&server, "excalidraw_liste", json!({})));
        assert!(liste.contains("plan.excalidraw"), "{liste}");
    }

    #[test]
    fn ein_unbekanntes_werkzeug_ist_ein_fehler() {
        let server = ohne_zugang();
        let fehler = aufruf(&server, "excalidraw_quatsch", json!({})).unwrap_err();
        assert!(
            fehler.message.contains("Unbekanntes Werkzeug"),
            "{fehler:?}"
        );
    }
}
