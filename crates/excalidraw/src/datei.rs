// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Das Format einer `.excalidraw`-Datei (Aufgabe 17).
//!
//! Eine Datei ist JSON mit fünf Pflichtteilen:
//!
//! ```json
//! { "type": "excalidraw", "version": 2, "source": "…",
//!   "elements": [ … ], "appState": { … }, "files": { … } }
//! ```
//!
//! Jedes Element trägt die Pflichtfelder aus [`crate::pruefen::PFLICHTFELDER`];
//! dazu kommt, was seine Art braucht (`text`, `points`, `containerId`,
//! `startBinding` …).
//!
//! **Unbekanntes bleibt stehen.** Die Datei ist bewusst großzügig modelliert:
//! was diese Kiste nicht liest (etwa `customData`, `pressures`, `polygon`, ein
//! ganzes Element vom Typ `embeddable`), wandert beim Lesen in
//! [`Element::weitere`] beziehungsweise [`Datei::weitere`] und wird beim
//! Schreiben unverändert zurückgeschrieben. Ein Werkzeugaufruf kann so nie die
//! Arbeit eines Menschen zerstören, die es nicht versteht.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::fehler::Fehler;

/// Der Wert von `type` in einer gültigen Datei.
pub const ART: &str = "excalidraw";
/// Der Wert von `version` in einer gültigen Datei.
pub const VERSION: u32 = 2;
/// Woher die Datei kommt. Steht als `source` darin.
pub const SOURCE: &str = "https://github.com/patimweb-labs/quiver-tools";
/// Die Endung der Diagrammdateien, ohne Punkt.
pub const ENDUNG: &str = "excalidraw";
/// Hintergrund, den neue Dateien bekommen (`appState.viewBackgroundColor`).
pub const HINTERGRUND: &str = "#ffffff";

/// Welches Ende einer Linie gemeint ist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ende {
    /// Der Anfang (erster Punkt).
    Start,
    /// Das Ende (letzter Punkt).
    Schluss,
}

/// Ein Eintrag in `boundElements`: „an diesem Element hängt etwas“.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    /// `text` oder `arrow`.
    #[serde(rename = "type")]
    pub art: String,
    /// Id des gebundenen Elements.
    pub id: String,
}

impl Binding {
    /// Ein Text hängt an einem Behälter.
    pub fn text(id: impl Into<String>) -> Binding {
        Binding {
            art: "text".to_string(),
            id: id.into(),
        }
    }

    /// Ein Pfeil hängt an einem Element.
    pub fn pfeil(id: impl Into<String>) -> Binding {
        Binding {
            art: "arrow".to_string(),
            id: id.into(),
        }
    }
}

/// Ein Element einer Zeichnung.
///
/// Die Pflichtfelder sind typisiert, alles Übrige steht in [`Element::weitere`]
/// und wird unverändert weitergereicht.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Element {
    /// Eindeutige Id in der Datei.
    #[serde(default)]
    pub id: String,
    /// Art: `rectangle`, `diamond`, `ellipse`, `text`, `arrow`, `line`,
    /// `freedraw`, `frame`, `image` …
    #[serde(rename = "type")]
    pub art: String,
    /// Linke Kante.
    #[serde(default)]
    pub x: f64,
    /// Oberkante.
    #[serde(default)]
    pub y: f64,
    /// Breite.
    #[serde(default)]
    pub width: f64,
    /// Höhe.
    #[serde(default)]
    pub height: f64,
    /// Drehung im Bogenmaß.
    #[serde(default)]
    pub angle: f64,
    /// Strichfarbe als `#rrggbb` oder `transparent`.
    #[serde(rename = "strokeColor", default = "strich_standard")]
    pub stroke_color: String,
    /// Füllfarbe als `#rrggbb` oder `transparent`.
    #[serde(rename = "backgroundColor", default = "hintergrund_standard")]
    pub background_color: String,
    /// `solid`, `hachure` oder `cross-hatch`.
    #[serde(rename = "fillStyle", default = "fullung_standard")]
    pub fill_style: String,
    /// Strichstärke.
    #[serde(rename = "strokeWidth", default = "strichbreite_standard")]
    pub stroke_width: f64,
    /// `solid`, `dashed` oder `dotted`.
    #[serde(rename = "strokeStyle", default = "strichart_standard")]
    pub stroke_style: String,
    /// Rauheit des Strichs (0, 1, 2).
    #[serde(default = "rauheit_standard")]
    pub roughness: f64,
    /// Deckkraft von 0 bis 100.
    #[serde(default = "deckkraft_standard")]
    pub opacity: f64,
    /// Startwert des Zufallsstifts.
    #[serde(default = "eins_i64")]
    pub seed: i64,
    /// Zähler der Änderungen; steigt mit jeder Änderung.
    #[serde(default = "eins_u64")]
    pub version: u64,
    /// Zufallsanteil der Änderung; steigt ebenfalls.
    #[serde(rename = "versionNonce", default)]
    pub version_nonce: i64,
    /// Reihenfolge als Bruchzahl-Index; bei uns `null`.
    #[serde(default)]
    pub index: Value,
    /// Weich gelöscht (Excalidraw behält Gelöschtes im Papierkorb).
    #[serde(rename = "isDeleted", default)]
    pub is_deleted: bool,
    /// Gruppen, zu denen das Element gehört.
    #[serde(rename = "groupIds", default)]
    pub group_ids: Vec<String>,
    /// Rahmen (Frame), in dem das Element liegt.
    #[serde(rename = "frameId", default)]
    pub frame_id: Option<String>,
    /// Was an diesem Element hängt (Texte und Pfeile).
    #[serde(rename = "boundElements", default)]
    pub bound_elements: Option<Vec<Binding>>,
    /// Zeitpunkt der letzten Änderung in Millisekunden.
    #[serde(default)]
    pub updated: i64,
    /// Verweis, nur `https`/`http`/`mailto`.
    #[serde(default)]
    pub link: Option<String>,
    /// Gesperrt (nicht verschiebbar).
    #[serde(default)]
    pub locked: bool,

    // ---- was einzelne Arten zusätzlich brauchen -------------------------
    /// Text eines `text`-Elements.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Der Text, bevor er umbrochen wurde.
    #[serde(
        rename = "originalText",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub original_text: Option<String>,
    /// Behälter eines gebundenen Textes.
    #[serde(
        rename = "containerId",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub container_id: Option<String>,
    /// Schriftgröße eines Textes.
    #[serde(rename = "fontSize", default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
    /// Schriftfamilie (Zahl nach Excalidraw-Konvention).
    #[serde(
        rename = "fontFamily",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub font_family: Option<Value>,
    /// Ausrichtung des Textes (`left`, `center`, `right`).
    #[serde(rename = "textAlign", default, skip_serializing_if = "Option::is_none")]
    pub text_align: Option<String>,
    /// Senkrechte Ausrichtung (`top`, `middle`, `bottom`).
    #[serde(
        rename = "verticalAlign",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub vertical_align: Option<String>,
    /// Zeilenhöhe als Vielfaches der Schriftgröße.
    #[serde(
        rename = "lineHeight",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub line_height: Option<Value>,
    /// Text wächst mit dem Inhalt? Für gebundene Texte `false`.
    #[serde(
        rename = "autoResize",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub auto_resize: Option<bool>,
    /// Stützpunkte einer Linie, relativ zur linken oberen Ecke.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub points: Option<Vec<[f64; 2]>>,
    /// Bindung des Anfangs (Pfeil an Element).
    #[serde(
        rename = "startBinding",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub start_binding: Option<Value>,
    /// Bindung des Endes (Pfeil an Element).
    #[serde(
        rename = "endBinding",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub end_binding: Option<Value>,
    /// Pfeilspitze am Anfang (`arrow` oder `null`).
    #[serde(
        rename = "startArrowhead",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub start_arrowhead: Option<Value>,
    /// Pfeilspitze am Ende (`arrow` oder `null`).
    #[serde(
        rename = "endArrowhead",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub end_arrowhead: Option<Value>,
    /// Überschrift eines Rahmens (Frame).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Eckenradius (`{"type": n}` oder `null`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roundness: Option<Value>,

    /// Alles, was diese Kiste nicht kennt – bleibt unverändert erhalten.
    #[serde(flatten)]
    pub weitere: Map<String, Value>,
}

/// Zähler für Startwerte (`seed`) und Änderungsmarken (`versionNonce`).
static ZAEHLER: AtomicI64 = AtomicI64::new(0);

fn naechste_marks() -> i64 {
    let zahl = ZAEHLER.fetch_add(1, Ordering::Relaxed);
    // Zeit in Sekunden, damit zwei Prozesse nicht dieselben Marken vergeben.
    let stempel = jetzt_ms() / 1000;
    stempel
        .wrapping_mul(1000)
        .wrapping_add(zahl.rem_euclid(1000))
}

/// Der aktuelle Zeitpunkt in Millisekunden seit 1970 (0, wenn die Uhr fehlt).
pub fn jetzt_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|dauer| dauer.as_millis() as i64)
        .unwrap_or(0)
}

fn strich_standard() -> String {
    "#1e1e1e".to_string()
}

fn hintergrund_standard() -> String {
    "transparent".to_string()
}

fn fullung_standard() -> String {
    "solid".to_string()
}

fn strichbreite_standard() -> f64 {
    2.0
}

fn strichart_standard() -> String {
    "solid".to_string()
}

fn rauheit_standard() -> f64 {
    1.0
}

fn deckkraft_standard() -> f64 {
    100.0
}

fn eins_i64() -> i64 {
    1
}

fn eins_u64() -> u64 {
    1
}

impl Element {
    /// Ein neues Element mit allen Pflichtfeldern und den Standardwerten, die
    /// Excalidraw selbst vergibt.
    pub fn neu(art: impl Into<String>, x: f64, y: f64, breite: f64, hoehe: f64) -> Element {
        let marke = naechste_marks();
        Element {
            id: String::new(),
            art: art.into(),
            x,
            y,
            width: breite,
            height: hoehe,
            angle: 0.0,
            stroke_color: strich_standard(),
            background_color: hintergrund_standard(),
            fill_style: fullung_standard(),
            stroke_width: strichbreite_standard(),
            stroke_style: strichart_standard(),
            roughness: rauheit_standard(),
            opacity: deckkraft_standard(),
            seed: marke.abs(),
            version: 1,
            version_nonce: marke,
            index: Value::Null,
            is_deleted: false,
            group_ids: Vec::new(),
            frame_id: None,
            bound_elements: None,
            updated: jetzt_ms(),
            link: None,
            locked: false,
            text: None,
            original_text: None,
            container_id: None,
            font_size: None,
            font_family: None,
            text_align: None,
            vertical_align: None,
            line_height: None,
            auto_resize: None,
            points: None,
            start_binding: None,
            end_binding: None,
            start_arrowhead: None,
            end_arrowhead: None,
            name: None,
            roundness: None,
            weitere: Map::new(),
        }
    }

    /// Ist das Element von dieser Art?
    pub fn ist(&self, art: &str) -> bool {
        self.art == art
    }

    /// Ist es eine Form, in die ein Text passt?
    pub fn ist_form(&self) -> bool {
        matches!(self.art.as_str(), "rectangle" | "diamond" | "ellipse")
    }

    /// Ist es eine Linie oder ein Pfeil?
    pub fn ist_linie(&self) -> bool {
        matches!(self.art.as_str(), "line" | "arrow" | "freedraw")
    }

    /// Die Id des Behälters, in dem der Text liegt.
    pub fn container(&self) -> Option<&str> {
        self.container_id.as_deref()
    }

    /// Die Id des Elements am Anfang der Linie.
    pub fn start(&self) -> Option<&str> {
        bindung_ziel(self.start_binding.as_ref())
    }

    /// Die Id des Elements am Ende der Linie.
    pub fn schluss(&self) -> Option<&str> {
        bindung_ziel(self.end_binding.as_ref())
    }

    /// Setzt oder löst die Bindung eines Linienendes.
    pub fn bindung_setzen(&mut self, ende: Ende, ziel: Option<&str>) {
        let wert = ziel.map(|id| json!({ "elementId": id, "focus": 0, "gap": 4 }));
        match ende {
            Ende::Start => self.start_binding = wert,
            Ende::Schluss => self.end_binding = wert,
        }
    }

    /// Die Ids der Elemente, die an diesem hängen, mit ihrer Art.
    pub fn gebundene(&self, art: &str) -> Vec<String> {
        self.bound_elements
            .iter()
            .flatten()
            .filter(|bindung| bindung.art == art)
            .map(|bindung| bindung.id.clone())
            .collect()
    }

    /// Hängt dieses Element an `id` (als Text oder als Pfeil)?
    pub fn haengt_an(&self, id: &str) -> bool {
        self.bound_elements
            .iter()
            .flatten()
            .any(|bindung| bindung.id == id)
    }

    /// Nimmt eine Bindung aus `boundElements` heraus; `true`, wenn sie da war.
    pub fn bindung_entfernen(&mut self, art: &str, id: &str) -> bool {
        let Some(bindungen) = &mut self.bound_elements else {
            return false;
        };
        let vorher = bindungen.len();
        bindungen.retain(|bindung| !(bindung.art == art && bindung.id == id));
        bindungen.len() != vorher
    }

    /// Trägt eine Bindung in `boundElements` ein, ohne sie zu verdoppeln.
    pub fn bindung_eintragen(&mut self, bindung: Binding) {
        let eintraege = self.bound_elements.get_or_insert_with(Vec::new);
        if eintraege
            .iter()
            .any(|eintrag| eintrag.art == bindung.art && eintrag.id == bindung.id)
        {
            return;
        }
        eintraege.push(bindung);
    }

    /// Eine Änderung vermerken: `version`, `versionNonce` und `updated` steigen.
    pub fn beruehren(&mut self) {
        self.version = self.version.saturating_add(1);
        self.version_nonce = self.version_nonce.saturating_add(1);
        self.updated = jetzt_ms();
    }

    /// Die Stützpunkte in Dateikoordinaten (nicht relativ).
    pub fn punkte(&self) -> Vec<(f64, f64)> {
        match &self.points {
            Some(punkte) if punkte.len() >= 2 => punkte
                .iter()
                .map(|punkt| (self.x + punkt[0], self.y + punkt[1]))
                .collect(),
            _ => vec![
                (self.x, self.y),
                (self.x + self.width, self.y + self.height),
            ],
        }
    }

    /// Setzt die Stützpunkte aus Dateikoordinaten und passt `x`, `y`, `width`
    /// und `height` daran an (Excalidraw hält den ersten Punkt bei `[0, 0]`).
    pub fn punkte_setzen(&mut self, absolut: &[(f64, f64)]) {
        if absolut.is_empty() {
            return;
        }
        let min_x = absolut.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
        let min_y = absolut.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
        let max_x = absolut
            .iter()
            .map(|p| p.0)
            .fold(f64::NEG_INFINITY, f64::max);
        let max_y = absolut
            .iter()
            .map(|p| p.1)
            .fold(f64::NEG_INFINITY, f64::max);
        // `x`/`y` ist der **erste Punkt**, nicht die linke obere Ecke: Excalidraw
        // verlangt `points[0] == [0, 0]`; die übrigen dürfen negativ sein.
        let (start_x, start_y) = absolut[0];
        self.x = start_x;
        self.y = start_y;
        self.width = max_x - min_x;
        self.height = max_y - min_y;
        self.points = Some(
            absolut
                .iter()
                .map(|(x, y)| [x - start_x, y - start_y])
                .collect(),
        );
    }

    /// Der Mittelpunkt (bei Linien der ihrer Ausdehnung, `x`/`y` ist dort der
    /// Anfang und nicht die Ecke).
    pub fn mitte(&self) -> (f64, f64) {
        if self.points.as_ref().is_some_and(|punkte| punkte.len() >= 2) {
            let punkte = self.punkte();
            let spanne = |werte: Vec<f64>| {
                let min = werte.iter().copied().fold(f64::INFINITY, f64::min);
                let max = werte.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                (min + max) / 2.0
            };
            return (
                spanne(punkte.iter().map(|p| p.0).collect()),
                spanne(punkte.iter().map(|p| p.1).collect()),
            );
        }
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }
}

fn bindung_ziel(wert: Option<&Value>) -> Option<&str> {
    wert?.get("elementId")?.as_str()
}

/// Eine ganze `.excalidraw`-Datei.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Datei {
    /// Immer `excalidraw`.
    #[serde(rename = "type", default = "art_standard")]
    pub art: String,
    /// Immer `2`.
    #[serde(default = "version_standard")]
    pub version: u32,
    /// Herkunft der Datei.
    #[serde(default = "source_standard")]
    pub source: String,
    /// Die Elemente in Zeichenreihenfolge.
    #[serde(default)]
    pub elements: Vec<Element>,
    /// Ansichtszustand; wird unverändert weitergereicht.
    #[serde(rename = "appState", default = "app_state_standard")]
    pub app_state: Map<String, Value>,
    /// Eingebettete Bilder und Schriften, nach Id.
    #[serde(default)]
    pub files: Map<String, Value>,
    /// Was diese Kiste nicht kennt – bleibt unverändert erhalten.
    #[serde(flatten)]
    pub weitere: Map<String, Value>,
}

fn art_standard() -> String {
    ART.to_string()
}

fn version_standard() -> u32 {
    VERSION
}

fn source_standard() -> String {
    SOURCE.to_string()
}

fn app_state_standard() -> Map<String, Value> {
    let mut app_state = Map::new();
    app_state.insert("viewBackgroundColor".to_string(), json!(HINTERGRUND));
    app_state
}

impl Default for Datei {
    fn default() -> Datei {
        Datei::neu()
    }
}

impl Datei {
    /// Eine leere, gültige Datei mit Hintergrund und leerer Elementliste.
    pub fn neu() -> Datei {
        Datei {
            art: art_standard(),
            version: VERSION,
            source: source_standard(),
            elements: Vec::new(),
            app_state: app_state_standard(),
            files: Map::new(),
            weitere: Map::new(),
        }
    }

    /// Eine Datei aus Text lesen.
    ///
    /// Fehlende Felder bekommen die Standardwerte, die Excalidraw selbst
    /// vergibt (`restore.ts`); unbekannte bleiben in [`Element::weitere`] und
    /// [`Datei::weitere`] stehen.
    pub fn aus_text(text: &str) -> Result<Datei, Fehler> {
        serde_json::from_str(text).map_err(|fehler| Fehler::Json(fehler.to_string()))
    }

    /// Die Datei als Text (hübsch eingerückt, damit ein Mensch sie lesen kann).
    pub fn als_text(&self) -> Result<String, Fehler> {
        serde_json::to_string_pretty(self).map_err(|fehler| Fehler::Json(fehler.to_string()))
    }

    /// Die sichtbaren Elemente (ohne weich gelöschte).
    pub fn sichtbare(&self) -> impl Iterator<Item = &Element> {
        self.elements.iter().filter(|element| !element.is_deleted)
    }

    /// Anzahl der sichtbaren Elemente.
    pub fn anzahl(&self) -> usize {
        self.sichtbare().count()
    }

    /// Position eines Elements in `elements`.
    pub fn position(&self, id: &str) -> Option<usize> {
        self.elements.iter().position(|element| element.id == id)
    }

    /// Das Element mit dieser Id.
    pub fn element(&self, id: &str) -> Option<&Element> {
        self.elements.iter().find(|element| element.id == id)
    }

    /// Das Element mit dieser Id, zum Ändern.
    pub fn element_mut(&mut self, id: &str) -> Option<&mut Element> {
        self.elements.iter_mut().find(|element| element.id == id)
    }

    /// Eine Id, die es in dieser Datei noch nicht gibt.
    pub fn freie_id(&self, vorsatz: &str) -> String {
        let mut nummer = 1;
        loop {
            let kandidat = format!("{vorsatz}-{nummer}");
            if self.position(&kandidat).is_none() {
                return kandidat;
            }
            nummer += 1;
        }
    }

    /// Ein Element anhängen; die Id muss frei und nicht leer sein.
    pub fn hinzufuegen(&mut self, mut element: Element) -> Result<String, Fehler> {
        if element.id.trim().is_empty() {
            element.id = self.freie_id(&element.art);
        }
        if element.id.trim() != element.id {
            return Err(Fehler::Ungueltig(format!(
                "Id „{}“ hat Leerzeichen am Rand – bitte ohne.",
                element.id
            )));
        }
        if self.position(&element.id).is_some() {
            return Err(Fehler::IdVergeben(element.id));
        }
        let id = element.id.clone();
        self.elements.push(element);
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eine_neue_datei_hat_alle_pflichtteile() {
        let datei = Datei::neu();
        let roh: Value = serde_json::from_str(&datei.als_text().unwrap()).unwrap();
        assert_eq!(roh["type"], "excalidraw");
        assert_eq!(roh["version"], 2);
        assert_eq!(roh["source"], SOURCE);
        assert!(roh["elements"].as_array().unwrap().is_empty());
        assert_eq!(roh["appState"]["viewBackgroundColor"], HINTERGRUND);
        assert!(roh["files"].is_object());
    }

    #[test]
    fn ein_neues_element_hat_die_pflichtfelder() {
        let element = Element::neu("rectangle", 10.0, 20.0, 30.0, 40.0);
        let roh = serde_json::to_value(&element).unwrap();
        for feld in crate::pruefen::PFLICHTFELDER {
            assert!(roh.get(feld).is_some(), "Pflichtfeld „{feld}“ fehlt: {roh}");
        }
        assert_eq!(roh["index"], Value::Null);
        assert_eq!(roh["isDeleted"], false);
        assert_eq!(roh["locked"], false);
        // Artfremdes wird nicht mitgeschrieben.
        assert!(roh.get("text").is_none(), "{roh}");
        assert!(roh.get("points").is_none(), "{roh}");
    }

    #[test]
    fn unbekannte_felder_bleiben_erhalten() {
        let text = r##"{
            "type": "excalidraw", "version": 2, "source": "x",
            "elements": [
                { "id": "a", "type": "image", "x": 0, "y": 0, "width": 1, "height": 1,
                  "angle": 0, "strokeColor": "#000", "backgroundColor": "transparent",
                  "fillStyle": "solid", "strokeWidth": 2, "strokeStyle": "solid",
                  "roughness": 1, "opacity": 100, "seed": 1, "version": 1,
                  "versionNonce": 2, "index": null, "isDeleted": false, "groupIds": [],
                  "frameId": null, "boundElements": null, "updated": 3, "link": null,
                  "locked": false, "fileId": "f1", "status": "saved",
                  "customData": { "fremd": true } }
            ],
            "appState": { "viewBackgroundColor": "#fff", "scrollX": 7 },
            "files": { "f1": { "id": "f1", "mimeType": "image/png", "dataURL": "data:…" } },
            "libraryItems": [ { "status": "published" } ]
        }"##;
        let datei = Datei::aus_text(text).unwrap();
        let zurueck: Value = serde_json::from_str(&datei.als_text().unwrap()).unwrap();

        assert_eq!(zurueck["elements"][0]["fileId"], "f1");
        assert_eq!(zurueck["elements"][0]["status"], "saved");
        assert_eq!(zurueck["elements"][0]["customData"]["fremd"], true);
        assert_eq!(zurueck["appState"]["scrollX"], 7);
        assert_eq!(zurueck["files"]["f1"]["dataURL"], "data:…");
        assert_eq!(zurueck["libraryItems"][0]["status"], "published");
    }

    #[test]
    fn das_beruehren_erhoeht_version_und_marke() {
        let mut element = Element::neu("text", 0.0, 0.0, 10.0, 10.0);
        let version = element.version;
        let marke = element.version_nonce;
        element.beruehren();
        assert_eq!(element.version, version + 1);
        assert!(element.version_nonce > marke);
        assert!(element.updated > 0);
    }

    #[test]
    fn punkte_setzen_haelt_den_ersten_punkt_bei_null() {
        let mut pfeil = Element::neu("arrow", 0.0, 0.0, 0.0, 0.0);
        pfeil.punkte_setzen(&[(100.0, 80.0), (140.0, 120.0)]);
        assert_eq!(pfeil.x, 100.0);
        assert_eq!(pfeil.y, 80.0);
        assert_eq!(pfeil.width, 40.0);
        assert_eq!(pfeil.height, 40.0);
        assert_eq!(pfeil.points.as_ref().unwrap()[0], [0.0, 0.0]);
        assert_eq!(pfeil.punkte(), vec![(100.0, 80.0), (140.0, 120.0)]);
        // Auch nach links und oben – mit Knick.
        let weg = [(100.0, 80.0), (100.0, 20.0), (-40.0, 20.0)];
        pfeil.punkte_setzen(&weg);
        assert_eq!((pfeil.x, pfeil.y), (100.0, 80.0));
        assert_eq!((pfeil.width, pfeil.height), (140.0, 60.0));
        assert_eq!(pfeil.points.as_ref().unwrap()[0], [0.0, 0.0]);
        assert_eq!(pfeil.punkte(), weg.to_vec());
        assert_eq!(pfeil.mitte(), (30.0, 50.0));
    }

    #[test]
    fn freie_ids_werden_gezaehlt() {
        let mut datei = Datei::neu();
        datei
            .hinzufuegen(Element::neu("rectangle", 0.0, 0.0, 1.0, 1.0))
            .unwrap();
        assert_eq!(datei.freie_id("rectangle"), "rectangle-2");
        assert_eq!(datei.elements[0].id, "rectangle-1");
        assert!(datei
            .hinzufuegen(Element::neu("rectangle", 0.0, 0.0, 1.0, 1.0))
            .is_ok());
        assert_eq!(datei.elements[1].id, "rectangle-2");
    }
}
