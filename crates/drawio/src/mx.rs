// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Das draw.io-Format: `mxfile → diagram → mxGraphModel → root → mxCell`.
//!
//! Eine Datei ist ein [`Element`]-Baum (siehe [`crate::xml`]); dieses Modul legt
//! die fachliche Sicht darüber:
//!
//! - **Datei und Seiten.** `mxfile` trägt eine oder mehrere `diagram`-Seiten mit
//!   `id` und `name`. Eine Seite enthält entweder ein `mxGraphModel` oder –
//!   komprimiert – Base64-Text (siehe [`crate::link`]). Lesen kann beides,
//!   geschrieben wird immer unkomprimiert.
//! - **Zellen.** Die ersten beiden Zellen unter `root` sind fest: `0` (Wurzel)
//!   und `1` (Standardebene). Alle weiteren sind Knoten (`vertex="1"`), Kanten
//!   (`edge="1"`) oder Ebenen. Zellen dürfen in `UserObject`/`object`-Hüllen
//!   stecken; die Hülle trägt dann die Id und `label`, die innere `mxCell` das
//!   Aussehen.
//! - **Prüfung.** [`pruefe_modell`] lehnt ab, was draw.io nicht laden würde:
//!   fehlende Zellen 0/1, doppelte Ids, Kanten auf unbekannte Zellen, Links mit
//!   fremdem Schema.
//!
//! Was dieses Modul **nicht** kennt, bleibt im Baum stehen und wird
//! unverändert wieder geschrieben.

use std::collections::{BTreeMap, BTreeSet};

use quiver_diagramm::modell::Farbe;

use crate::fehler::FormatFehler;
use crate::formen::Form;
use crate::link;
use crate::stil::Stil;
use crate::xml::{self, Element, Knoten};

/// Id der draw.io-Wurzelzelle (immer die erste unter `<root>`).
pub const ROOT_ID: &str = "0";
/// Id der Standardebene (immer die zweite unter `<root>`).
pub const EBENE_ID: &str = "1";

/// Höchstzahl Zellen je Seite.
pub const MAX_ZELLEN: usize = 5000;

/// So tief darf eine Zelle in einer anderen stecken.
const MAX_ELTERN_TIEFE: usize = 32;

/// Die festen Attribute eines `mxGraphModel` (Seitenmaße kommen dazu).
const MODELL_ATTRIBUTE: [(&str, &str); 11] = [
    ("dx", "0"),
    ("dy", "0"),
    ("grid", "0"),
    ("gridSize", "10"),
    ("guides", "1"),
    ("tooltips", "1"),
    ("connect", "1"),
    ("arrows", "1"),
    ("fold", "1"),
    ("page", "1"),
    ("pageScale", "1"),
];

/// Eine draw.io-Datei: das `mxfile`-Element mit seinen Seiten.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Datei {
    mxfile: Element,
}

/// Eine Seite, wie sie in Listen und Antworten erscheint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seite {
    /// Reihenfolge in der Datei, beginnend bei 0.
    pub index: usize,
    /// Id aus dem `diagram`-Element (kann in alten Dateien fehlen).
    pub id: String,
    /// Anzeigename (Reiter in draw.io).
    pub name: String,
    /// Lag die Seite komprimiert in der Datei?
    pub komprimiert: bool,
    /// So viele Zeichen belegt der gespeicherte Inhalt.
    pub gespeichert: usize,
    /// Anzahl der Zellen unter `root` (einschließlich 0 und 1).
    pub zellen: usize,
}

/// Lage und Größe einer Zelle in Nutzerkoordinaten.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lage {
    /// Linke obere Ecke, x.
    pub x: f32,
    /// Linke obere Ecke, y.
    pub y: f32,
    /// Breite.
    pub breite: f32,
    /// Höhe.
    pub hoehe: f32,
}

impl Lage {
    /// Eine Lage aus vier Zahlen.
    pub fn neu(x: f32, y: f32, breite: f32, hoehe: f32) -> Lage {
        Lage {
            x,
            y,
            breite,
            hoehe,
        }
    }

    /// Der Mittelpunkt.
    pub fn mitte(&self) -> (f32, f32) {
        (self.x + self.breite / 2.0, self.y + self.hoehe / 2.0)
    }
}

/// Eine Zelle, wie [`zellen`] sie für Beschreibungen liefert.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Zelle {
    /// Id der Zelle (aus der Hülle oder der `mxCell`).
    pub id: String,
    /// Beschriftung als Text (HTML bleibt stehen, wie es in der Datei steht).
    pub text: String,
    /// Ist es eine Kante?
    pub kante: bool,
    /// Startknoten einer Kante.
    pub quelle: Option<String>,
    /// Zielknoten einer Kante.
    pub ziel: Option<String>,
    /// Elternteil (Ebene, Gruppe, Container).
    pub eltern: Option<String>,
    /// Bekannte Form des Knotens.
    pub form: Option<Form>,
    /// Formname aus der Datei, den dieses Werkzeug nicht zeichnet.
    pub fremde_form: Option<String>,
}

// ------------------------------------------------------------------ Datei

impl Datei {
    /// Liest eine Datei; prüft dabei jede Seite (auch komprimierte).
    pub fn aus_text(text: &str) -> Result<Datei, FormatFehler> {
        Datei::aus_element(xml::lesen(text)?)
    }

    /// Baut eine Datei aus einem fertigen `mxfile`-Element.
    pub fn aus_element(mxfile: Element) -> Result<Datei, FormatFehler> {
        if mxfile.name != "mxfile" {
            return Err(FormatFehler::Struktur(format!(
                "Das ist kein draw.io-Dokument: die Wurzel ist <{}>, erwartet wird <mxfile>.",
                mxfile.name
            )));
        }
        let datei = Datei { mxfile };
        if datei.diagramme().is_empty() {
            return Err(FormatFehler::Struktur(
                "Die Datei hat keine Seite – <mxfile> braucht mindestens ein <diagram>."
                    .to_string(),
            ));
        }
        // Jede Seite einmal lesen: ein Fehler nennt dann die Seite, nicht die Datei.
        datei.seiten()?;
        Ok(datei)
    }

    /// Das `mxfile`-Element (unverändert, inklusive unbekannter Attribute).
    pub fn wurzel(&self) -> &Element {
        &self.mxfile
    }

    /// Die Datei als XML-Text; Seiten sind immer unkomprimiert.
    pub fn als_text(&self) -> String {
        self.mxfile.als_text()
    }

    fn diagramme(&self) -> Vec<&Element> {
        self.mxfile
            .kinder_elemente()
            .filter(|kind| kind.name == "diagram")
            .collect()
    }

    fn diagramm(&self, index: usize) -> Result<&Element, FormatFehler> {
        self.diagramme().get(index).copied().ok_or_else(|| {
            FormatFehler::Seite(format!(
                "Seite {} gibt es nicht – die Datei hat {} Seite(n).",
                index + 1,
                self.diagramme().len()
            ))
        })
    }

    fn diagramm_mut(&mut self, index: usize) -> Result<&mut Element, FormatFehler> {
        let mut zaehler = 0;
        for kind in self.mxfile.kinder.iter_mut() {
            if let Knoten::Element(element) = kind {
                if element.name == "diagram" {
                    if zaehler == index {
                        return Ok(element);
                    }
                    zaehler += 1;
                }
            }
        }
        Err(FormatFehler::Seite(format!(
            "Seite {} gibt es nicht – die Datei hat {zaehler} Seite(n).",
            index + 1
        )))
    }

    /// Alle Seiten mit Randdaten; prüft dabei jede Seite.
    pub fn seiten(&self) -> Result<Vec<Seite>, FormatFehler> {
        let mut seiten = Vec::new();
        for (index, diagramm) in self.diagramme().iter().enumerate() {
            let komprimiert = diagramm.kind("mxGraphModel").is_none();
            let modell = self.modell(index)?;
            let root = modell.kind("root").ok_or_else(|| {
                FormatFehler::Struktur(format!(
                    "Seite {} hat kein <root>.",
                    seiten_name(diagramm, index)
                ))
            })?;
            seiten.push(Seite {
                index,
                id: diagramm.attribut("id").unwrap_or_default().to_string(),
                name: diagramm.attribut("name").unwrap_or_default().to_string(),
                komprimiert,
                gespeichert: diagramm.text_inhalt().trim().len(),
                zellen: eintraege(root).len(),
            });
        }
        Ok(seiten)
    }

    /// Eine Seite als Liste „`1 = „Start“ (id s1)`“ für Meldungen.
    fn seiten_kurz(&self) -> String {
        let diagramme = self.diagramme();
        if diagramme.is_empty() {
            return "keine".to_string();
        }
        diagramme
            .iter()
            .enumerate()
            .map(|(index, diagramm)| {
                let name = diagramm.attribut("name").unwrap_or("");
                let id = diagramm.attribut("id").unwrap_or("");
                format!("{} = „{name}“ (id {id})", index + 1)
            })
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// Die Seite zu `seite` (Id oder Name); ohne Angabe die erste.
    ///
    /// Ein Name, der auf mehrere Seiten passt, ergibt eine Meldung mit den
    /// Kandidaten – geraten wird nicht.
    pub fn seite_index(&self, seite: Option<&str>) -> Result<usize, FormatFehler> {
        let diagramme = self.diagramme();
        if diagramme.is_empty() {
            return Err(FormatFehler::Seite(
                "Die Datei hat keine Seite.".to_string(),
            ));
        }
        let gesucht = match seite.map(str::trim).filter(|seite| !seite.is_empty()) {
            Some(gesucht) => gesucht,
            None => return Ok(0),
        };
        let per_id: Vec<usize> = diagramme
            .iter()
            .enumerate()
            .filter(|(_, diagramm)| diagramm.attribut("id") == Some(gesucht))
            .map(|(index, _)| index)
            .collect();
        let treffer = if per_id.is_empty() {
            diagramme
                .iter()
                .enumerate()
                .filter(|(_, diagramm)| diagramm.attribut("name") == Some(gesucht))
                .map(|(index, _)| index)
                .collect::<Vec<_>>()
        } else {
            per_id
        };
        match treffer.as_slice() {
            [einer] => Ok(*einer),
            [] => Err(FormatFehler::Seite(format!(
                "Seite „{gesucht}“ gibt es nicht – vorhanden: {}",
                self.seiten_kurz()
            ))),
            mehrere => {
                let kandidaten = mehrere
                    .iter()
                    .map(|index| {
                        let diagramm = &diagramme[*index];
                        format!(
                            "{} = „{}“ (id {})",
                            index + 1,
                            diagramm.attribut("name").unwrap_or(""),
                            diagramm.attribut("id").unwrap_or("")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("; ");
                Err(FormatFehler::Seite(format!(
                    "Seite „{gesucht}“ ist mehrdeutig – Kandidaten: {kandidaten}. Bitte die Id nennen."
                )))
            }
        }
    }

    /// Das `mxGraphModel` einer Seite – entpackt und geprüft.
    pub fn modell(&self, index: usize) -> Result<Element, FormatFehler> {
        let diagramm = self.diagramm(index)?;
        let modell = match diagramm.kind("mxGraphModel") {
            Some(modell) => modell.clone(),
            None => {
                let text = diagramm.text_inhalt();
                let text = text.trim();
                if text.is_empty() {
                    return Err(FormatFehler::Struktur(format!(
                        "Seite {} hat keinen Inhalt – weder <mxGraphModel> noch gepackten Text.",
                        seiten_name(diagramm, index)
                    )));
                }
                let xml_text = link::entpacken(text)?;
                let element = xml::lesen(&xml_text)?;
                if element.name != "mxGraphModel" {
                    return Err(FormatFehler::Struktur(format!(
                        "Die entpackte Seite {} beginnt mit <{}> – dort gehört ein \
                         <mxGraphModel> hin.",
                        seiten_name(diagramm, index),
                        element.name
                    )));
                }
                element
            }
        };
        pruefe_modell(&modell)?;
        Ok(modell)
    }

    /// Ersetzt den Inhalt einer Seite; die Seite wird dabei unkomprimiert.
    pub fn modell_setzen(&mut self, index: usize, modell: Element) -> Result<(), FormatFehler> {
        pruefe_modell(&modell)?;
        let diagramm = self.diagramm_mut(index)?;
        diagramm.kinder.clear();
        diagramm.anhaengen(modell);
        self.mxfile.entferne_attribut("compressed");
        Ok(())
    }

    /// Hängt eine Seite an und gibt ihren Index zurück.
    pub fn seite_hinzufuegen(
        &mut self,
        name: &str,
        id: Option<&str>,
        modell: Element,
    ) -> Result<usize, FormatFehler> {
        let name = name.trim();
        if name.is_empty() {
            return Err(FormatFehler::Struktur(
                "Der Name der neuen Seite fehlt – bitte `name` angeben.".to_string(),
            ));
        }
        pruefe_modell(&modell)?;
        let vorhandene: Vec<String> = self
            .diagramme()
            .iter()
            .map(|diagramm| diagramm.attribut("id").unwrap_or_default().to_string())
            .collect();
        let id = match id.map(str::trim).filter(|id| !id.is_empty()) {
            Some(id) => {
                if vorhandene.iter().any(|vorhanden| vorhanden == id) {
                    return Err(FormatFehler::Struktur(format!(
                        "Die Seiten-Id „{id}“ gibt es schon – bitte eine andere wählen oder \
                         weglassen."
                    )));
                }
                id.to_string()
            }
            None => (1..)
                .map(|nummer| format!("seite-{nummer}"))
                .find(|kandidat| !vorhandene.iter().any(|id| id == kandidat))
                .expect("irgendwann ist eine Id frei"),
        };
        let mut diagramm = Element::neu("diagram").mit("id", id).mit("name", name);
        diagramm.anhaengen(modell);
        self.mxfile.anhaengen(diagramm);
        let anzahl = self.diagramme().len();
        if self.mxfile.attribut("pages").is_some() {
            self.mxfile.setze_attribut("pages", anzahl.to_string());
        }
        Ok(anzahl - 1)
    }
}

fn seiten_name(diagramm: &Element, index: usize) -> String {
    format!(
        "{} („{}“)",
        index + 1,
        diagramm.attribut("name").unwrap_or("ohne Namen")
    )
}

// ------------------------------------------------------------------ Zellen

/// Ist das Element eine Zelle oder eine Zellenhülle?
pub fn ist_eintrag(element: &Element) -> bool {
    matches!(element.name.as_str(), "mxCell" | "UserObject" | "object")
}

/// Alle Zellen unter `<root>` in Reihenfolge (inklusive 0 und 1).
pub fn eintraege(root: &Element) -> Vec<&Element> {
    root.kinder_elemente()
        .filter(|kind| ist_eintrag(kind))
        .collect()
}

/// Wie [`eintraege`], aber veränderbar.
pub fn eintraege_mut(root: &mut Element) -> Vec<&mut Element> {
    root.kinder
        .iter_mut()
        .filter_map(|knoten| match knoten {
            Knoten::Element(element) if ist_eintrag(element) => Some(element),
            _ => None,
        })
        .collect()
}

/// Die Id einer Zelle (bei einer Hülle die Id der Hülle).
pub fn id(eintrag: &Element) -> Option<&str> {
    eintrag.attribut("id")
}

/// Die innere `mxCell` (bei einer Hülle) oder das Element selbst.
pub fn zelle(eintrag: &Element) -> Option<&Element> {
    if eintrag.name == "mxCell" {
        Some(eintrag)
    } else {
        eintrag.kind("mxCell")
    }
}

/// Wie [`zelle`], aber veränderbar.
pub fn zelle_mut(eintrag: &mut Element) -> Option<&mut Element> {
    if eintrag.name == "mxCell" {
        return Some(eintrag);
    }
    eintrag.kinder.iter_mut().find_map(|knoten| match knoten {
        Knoten::Element(element) if element.name == "mxCell" => Some(element),
        _ => None,
    })
}

/// Der Elternteil einer Zelle (`parent`).
pub fn eltern(eintrag: &Element) -> Option<&str> {
    zelle(eintrag).and_then(|zelle| zelle.attribut("parent"))
}

/// Setzt den Elternteil einer Zelle.
pub fn setze_eltern(eintrag: &mut Element, eltern: &str) {
    if let Some(zelle) = zelle_mut(eintrag) {
        zelle.setze_attribut("parent", eltern);
    }
}

/// Ist die Zelle eine Kante?
pub fn ist_kante(eintrag: &Element) -> bool {
    zelle(eintrag).and_then(|zelle| zelle.attribut("edge")) == Some("1")
}

/// Ist die Zelle ein Knoten (Form)?
pub fn ist_knoten(eintrag: &Element) -> bool {
    zelle(eintrag).and_then(|zelle| zelle.attribut("vertex")) == Some("1")
}

/// Die Beschriftung: `label` der Hülle, sonst `value` der Zelle.
pub fn wert(eintrag: &Element) -> String {
    if eintrag.name != "mxCell" {
        if let Some(label) = eintrag.attribut("label") {
            return label.to_string();
        }
    }
    zelle(eintrag)
        .and_then(|zelle| zelle.attribut("value"))
        .unwrap_or_default()
        .to_string()
}

/// Liest draw.io die Beschriftung dieser Zelle als HTML?
fn ist_html(stil: &Stil) -> bool {
    stil.wert("html") == Some("1")
}

/// Text so, wie er in `value` gehört: bei `html=1` als HTML maskiert. Sonst
/// verschwände „<neu>“ als unbekanntes Tag, und ein Agent könnte Markup setzen.
pub fn als_label(stil: &Stil, text: &str) -> String {
    if !ist_html(stil) {
        return text.to_string();
    }
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace("\r\n", "\n")
        .replace('\n', "<br>")
}

/// Die Beschriftung als lesbarer Text (Gegenstück zu [`als_label`]).
pub fn text(eintrag: &Element) -> String {
    let roh = wert(eintrag);
    if ist_html(&stil(eintrag)) {
        crate::bild::html_zu_text(&roh)
    } else {
        roh
    }
}

/// Setzt die Beschriftung; die XML-Schicht maskiert sie beim Schreiben.
pub fn setze_wert(eintrag: &mut Element, text: &str) {
    if eintrag.name == "mxCell" {
        eintrag.setze_attribut("value", text);
    } else {
        eintrag.setze_attribut("label", text);
    }
}

/// Der Stil einer Zelle (leer, wenn sie keinen hat).
pub fn stil(eintrag: &Element) -> Stil {
    Stil::aus_text(
        zelle(eintrag)
            .and_then(|zelle| zelle.attribut("style"))
            .unwrap_or_default(),
    )
}

/// Setzt den Stil einer Zelle.
pub fn setze_stil(eintrag: &mut Element, stil: &Stil) {
    if let Some(zelle) = zelle_mut(eintrag) {
        zelle.setze_attribut("style", stil.als_text());
    }
}

/// Der Startknoten einer Kante.
pub fn quelle(eintrag: &Element) -> Option<String> {
    zelle(eintrag)
        .and_then(|zelle| zelle.attribut("source"))
        .map(str::to_string)
}

/// Der Zielknoten einer Kante.
pub fn ziel(eintrag: &Element) -> Option<String> {
    zelle(eintrag)
        .and_then(|zelle| zelle.attribut("target"))
        .map(str::to_string)
}

/// Setzt oder entfernt den Startknoten.
pub fn setze_quelle(eintrag: &mut Element, wert: Option<&str>) {
    if let Some(zelle) = zelle_mut(eintrag) {
        match wert {
            Some(wert) => zelle.setze_attribut("source", wert),
            None => {
                zelle.entferne_attribut("source");
            }
        }
    }
}

/// Setzt oder entfernt den Zielknoten.
pub fn setze_ziel(eintrag: &mut Element, wert: Option<&str>) {
    if let Some(zelle) = zelle_mut(eintrag) {
        match wert {
            Some(wert) => zelle.setze_attribut("target", wert),
            None => {
                zelle.entferne_attribut("target");
            }
        }
    }
}

/// Die Geometrie einer Zelle.
pub fn geometrie(eintrag: &Element) -> Option<&Element> {
    zelle(eintrag)?.kind("mxGeometry")
}

/// Die Geometrie einer Zelle, notfalls neu angelegt.
pub fn geometrie_mut(eintrag: &mut Element) -> Option<&mut Element> {
    let zelle = zelle_mut(eintrag)?;
    let stelle = zelle.kinder.iter().position(
        |knoten| matches!(knoten, Knoten::Element(element) if element.name == "mxGeometry"),
    );
    let stelle = match stelle {
        Some(stelle) => stelle,
        None => {
            zelle.kinder.push(Knoten::Element(
                Element::neu("mxGeometry").mit("as", "geometry"),
            ));
            zelle.kinder.len() - 1
        }
    };
    match &mut zelle.kinder[stelle] {
        Knoten::Element(element) => Some(element),
        Knoten::Text(_) => None,
    }
}

/// Lage und Größe einer Zelle (fehlende Angaben sind 0).
pub fn lage(eintrag: &Element) -> Lage {
    let geometrie = geometrie(eintrag);
    let zahl = |name: &str| {
        geometrie
            .and_then(|geometrie| geometrie.attribut(name))
            .and_then(|wert| wert.trim().parse::<f32>().ok())
            .filter(|wert| wert.is_finite())
            .unwrap_or(0.0)
    };
    Lage {
        x: zahl("x"),
        y: zahl("y"),
        breite: zahl("width"),
        hoehe: zahl("height"),
    }
}

/// Setzt Lage und Größe einer Zelle.
pub fn setze_lage(eintrag: &mut Element, lage: &Lage) {
    if let Some(geometrie) = geometrie_mut(eintrag) {
        geometrie.setze_attribut("as", "geometry");
        geometrie.setze_attribut("x", zahl(lage.x));
        geometrie.setze_attribut("y", zahl(lage.y));
        geometrie.setze_attribut("width", zahl(lage.breite));
        geometrie.setze_attribut("height", zahl(lage.hoehe));
    }
}

/// Eine Zahl so, wie draw.io sie schreibt: ohne unnötige Nullen.
pub fn zahl(wert: f32) -> String {
    if !wert.is_finite() {
        return "0".to_string();
    }
    let gerundet = (wert * 100.0).round() / 100.0;
    if (gerundet - gerundet.round()).abs() < 0.0001 {
        format!("{}", gerundet.round() as i64)
    } else {
        format!("{gerundet}")
    }
}

/// Die Zellen einer Seite als Beschreibung (ohne die festen Zellen 0 und 1).
pub fn zellen(root: &Element) -> Vec<Zelle> {
    eintraege(root)
        .into_iter()
        // Die festen Zellen 0/1 und zusätzliche Ebenen sind keine Elemente.
        .filter(|eintrag| {
            !matches!(id(eintrag), Some(ROOT_ID) | Some(EBENE_ID))
                && (ist_knoten(eintrag) || ist_kante(eintrag))
        })
        .map(|eintrag| {
            let stil = stil(eintrag);
            let form = Form::aus_stil(&stil);
            let fremde_form = match (form, stil.wert("shape"), stil.form()) {
                (None, Some(shape), _) => Some(shape.to_string()),
                (None, None, Some(form)) => Some(form.to_string()),
                _ => None,
            };
            Zelle {
                id: id(eintrag).unwrap_or_default().to_string(),
                text: text(eintrag),
                kante: ist_kante(eintrag),
                quelle: quelle(eintrag),
                ziel: ziel(eintrag),
                eltern: eltern(eintrag).map(str::to_string),
                form,
                fremde_form,
            }
        })
        .collect()
}

// ---------------------------------------------------------------- Prüfung

/// Prüft ein `mxGraphModel` gegen die Regeln aus Aufgabe 18.
pub fn pruefe_modell(modell: &Element) -> Result<(), FormatFehler> {
    if modell.name != "mxGraphModel" {
        return Err(FormatFehler::Struktur(format!(
            "Erwartet wird ein <mxGraphModel>, gefunden wurde <{}>.",
            modell.name
        )));
    }
    let root = modell.kind("root").ok_or_else(|| {
        FormatFehler::Struktur(
            "Dem Modell fehlt <root> – ein draw.io-Diagramm braucht \
             mxGraphModel > root > mxCell."
                .to_string(),
        )
    })?;
    let alle = eintraege(root);
    if alle.len() < 2 {
        return Err(FormatFehler::Struktur(format!(
            "Unter <root> fehlen die zwei festen Zellen 0 und 1 – gefunden wurden {}.",
            alle.len()
        )));
    }
    if id(alle[0]) != Some(ROOT_ID) {
        return Err(FormatFehler::Struktur(format!(
            "Die erste Zelle unter <root> muss id=\"{ROOT_ID}\" haben (draw.io-Wurzel); \
             gefunden wurde id=\"{}\".",
            id(alle[0]).unwrap_or("")
        )));
    }
    if id(alle[1]) != Some(EBENE_ID) {
        return Err(FormatFehler::Struktur(format!(
            "Die zweite Zelle unter <root> muss id=\"{EBENE_ID}\" sein (Standardebene); \
             gefunden wurde id=\"{}\".",
            id(alle[1]).unwrap_or("")
        )));
    }
    if eltern(alle[1]) != Some(ROOT_ID) {
        return Err(FormatFehler::Struktur(format!(
            "Die Standardebene (id=\"{EBENE_ID}\") braucht parent=\"{ROOT_ID}\"."
        )));
    }
    if alle.len() > MAX_ZELLEN {
        return Err(FormatFehler::Struktur(format!(
            "Die Seite hat {} Zellen – erlaubt sind höchstens {MAX_ZELLEN}.",
            alle.len()
        )));
    }

    let mut bekannt: BTreeMap<&str, bool> = BTreeMap::new();
    for eintrag in &alle {
        let Some(id) = id(eintrag) else {
            return Err(FormatFehler::Struktur(format!(
                "Eine Zelle unter <root> hat keine id (<{}>).",
                eintrag.name
            )));
        };
        if id.trim().is_empty() {
            return Err(FormatFehler::Struktur(
                "Eine Zelle unter <root> hat eine leere id.".to_string(),
            ));
        }
        if bekannt.insert(id, ist_knoten(eintrag)).is_some() {
            return Err(FormatFehler::Struktur(format!(
                "Die Zellen-Id „{id}“ kommt doppelt vor – draw.io braucht eindeutige Ids."
            )));
        }
    }

    for eintrag in &alle {
        let kennung = id(eintrag).unwrap_or("");
        if ist_kante(eintrag) {
            for (feld, ende) in [("source", quelle(eintrag)), ("target", ziel(eintrag))] {
                let Some(ende) = ende else {
                    continue;
                };
                if ende.trim().is_empty() {
                    continue;
                }
                match bekannt.get(ende.as_str()) {
                    None => {
                        return Err(FormatFehler::Struktur(format!(
                            "Die Kante „{kennung}“ hat {feld}=\"{ende}\" – diese Zelle gibt es \
                             auf der Seite nicht."
                        )))
                    }
                    Some(false) => {
                        return Err(FormatFehler::Struktur(format!(
                            "Die Kante „{kennung}“ zeigt mit {feld} auf „{ende}“ – das ist kein \
                             Knoten (vertex=\"1\")."
                        )))
                    }
                    Some(true) => {}
                }
            }
        }
        // Links nur mit http(s) oder mailto (Aufgabe 18).
        if let Some(link) = eintrag.attribut("link") {
            pruefe_link(kennung, link)?;
        }
        if let Some(zelle) = zelle(eintrag) {
            if let Some(link) = zelle.attribut("link") {
                pruefe_link(kennung, link)?;
            }
        }
        if let Some(link) = stil(eintrag).wert("link") {
            pruefe_link(kennung, link)?;
        }
    }
    Ok(())
}

fn pruefe_link(kennung: &str, link: &str) -> Result<(), FormatFehler> {
    let link = link.trim();
    if link.is_empty() {
        return Ok(());
    }
    let erlaubt =
        link.starts_with("https://") || link.starts_with("http://") || link.starts_with("mailto:");
    if erlaubt {
        Ok(())
    } else {
        Err(FormatFehler::Struktur(format!(
            "Zelle „{kennung}“ trägt den Link „{link}“ – erlaubt sind nur http(s) und mailto."
        )))
    }
}

/// Ein leeres `mxGraphModel` mit den Zellen 0 und 1.
pub fn leeres_modell() -> Element {
    let mut root = Element::neu("root");
    root.anhaengen(Element::neu("mxCell").mit("id", ROOT_ID));
    root.anhaengen(
        Element::neu("mxCell")
            .mit("id", EBENE_ID)
            .mit("parent", ROOT_ID),
    );
    modell_bauen(root, 850.0, 1100.0)
}

/// Baut ein `mxGraphModel` um ein `root` mit Seitenmaßen.
pub fn modell_bauen(root: Element, breite: f32, hoehe: f32) -> Element {
    let mut modell = Element::neu("mxGraphModel");
    for (name, wert) in MODELL_ATTRIBUTE {
        modell.setze_attribut(name, wert);
    }
    modell.setze_attribut("math", "0");
    modell.setze_attribut("shadow", "0");
    modell.setze_attribut("pageWidth", zahl(breite.max(1.0)));
    modell.setze_attribut("pageHeight", zahl(hoehe.max(1.0)));
    modell.anhaengen(root);
    modell
}

/// Eine neue Datei mit genau einer Seite.
pub fn neue_datei(name: &str, modell: Element) -> Result<Datei, FormatFehler> {
    let mut diagramm = Element::neu("diagram")
        .mit("id", "seite-1")
        .mit("name", name);
    diagramm.anhaengen(modell);
    let mut mxfile = Element::neu("mxfile")
        .mit("host", "quiver-drawio")
        .mit("type", "device");
    mxfile.anhaengen(diagramm);
    Datei::aus_element(mxfile)
}

/// Ersetzt das `<root>` eines Modells und lässt alles andere stehen.
pub fn root_setzen(modell: &mut Element, root: Element) {
    for kind in modell.kinder.iter_mut() {
        if let Knoten::Element(element) = kind {
            if element.name == "root" {
                *element = root;
                return;
            }
        }
    }
    modell.anhaengen(root);
}

// ---------------------------------------------------------------- Ändern

/// Neue Lagewerte für ein Element; was fehlt, bleibt.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct LageAenderung {
    /// Neues x.
    pub x: Option<f32>,
    /// Neues y.
    pub y: Option<f32>,
    /// Neue Breite.
    pub breite: Option<f32>,
    /// Neue Höhe.
    pub hoehe: Option<f32>,
}

impl LageAenderung {
    /// Sind keine Werte gesetzt?
    pub fn ist_leer(&self) -> bool {
        self.x.is_none() && self.y.is_none() && self.breite.is_none() && self.hoehe.is_none()
    }
}

/// Eine Änderung an genau einer Zelle.
#[derive(Debug, Clone)]
pub struct Aenderung<'a> {
    /// Id der Zelle.
    pub id: &'a str,
    /// Neuer Text (als Text, nicht als HTML).
    pub text: Option<&'a str>,
    /// Neue Füllfarbe aus der festen Liste.
    pub farbe: Option<Farbe>,
    /// Neue Form (nur Knoten).
    pub form: Option<Form>,
    /// Neue Lage (nur Knoten).
    pub lage: LageAenderung,
    /// Neuer Startknoten (`Some(None)` löst die Verbindung).
    pub quelle: Option<Option<&'a str>>,
    /// Neuer Zielknoten (`Some(None)` löst die Verbindung).
    pub ziel: Option<Option<&'a str>>,
}

/// Wendet Änderungen an; gibt die geänderten Ids zurück.
///
/// Geprüft wird **vor** der ersten Änderung: eine unbekannte Id, eine Form auf
/// einer Kante oder eine Verbindung auf einen Knoten lässt alles unverändert.
pub fn aendern(root: &mut Element, aenderungen: &[Aenderung]) -> Result<Vec<String>, FormatFehler> {
    let vorhandene: Vec<(String, bool)> = eintraege(root)
        .iter()
        .map(|eintrag| {
            (
                id(eintrag).unwrap_or_default().to_string(),
                ist_knoten(eintrag),
            )
        })
        .collect();
    let knoten_ids: BTreeSet<&str> = vorhandene
        .iter()
        .filter(|(_, knoten)| *knoten)
        .map(|(id, _)| id.as_str())
        .collect();

    for aenderung in aenderungen {
        let Some((_, knoten)) = vorhandene.iter().find(|(id, _)| id == aenderung.id.trim()) else {
            return Err(FormatFehler::Struktur(format!(
                "Zelle „{}“ gibt es auf dieser Seite nicht – bitte mit drawio_lesen die Ids holen.",
                aenderung.id
            )));
        };
        if matches!(aenderung.id.trim(), ROOT_ID | EBENE_ID) {
            return Err(FormatFehler::Struktur(format!(
                "Die Zelle „{}“ ist fest (draw.io-Wurzel bzw. Standardebene) und lässt sich \
                 nicht ändern.",
                aenderung.id
            )));
        }
        if aenderung.form.is_some() && !knoten {
            return Err(FormatFehler::Struktur(format!(
                "Zelle „{}“ ist eine Kante – eine Form hat nur ein Knoten.",
                aenderung.id
            )));
        }
        if !aenderung.lage.ist_leer() && !knoten {
            return Err(FormatFehler::Struktur(format!(
                "Zelle „{}“ ist eine Kante – eine Lage hat nur ein Knoten.",
                aenderung.id
            )));
        }
        if (aenderung.quelle.is_some() || aenderung.ziel.is_some()) && *knoten {
            return Err(FormatFehler::Struktur(format!(
                "Zelle „{}“ ist ein Knoten – `quelle`/`ziel` gilt nur für Kanten.",
                aenderung.id
            )));
        }
        for (feld, wert) in [("quelle", &aenderung.quelle), ("ziel", &aenderung.ziel)] {
            if let Some(Some(ende)) = wert {
                if !knoten_ids.contains(ende.trim()) {
                    return Err(FormatFehler::Struktur(format!(
                        "{feld} „{ende}“ ist kein Knoten dieser Seite – eine Kante braucht \
                         vorhandene Knoten."
                    )));
                }
            }
        }
    }

    let mut geaendert = Vec::new();
    for aenderung in aenderungen {
        let kennung = aenderung.id.trim().to_string();
        let eintrag = eintraege_mut(root)
            .into_iter()
            .find(|eintrag| id(eintrag) == Some(kennung.as_str()));
        let Some(eintrag) = eintrag else {
            continue;
        };
        let mut stil = stil(eintrag);
        let war_html = ist_html(&stil);
        let mut stil_aendern = false;
        if let Some(form) = aenderung.form {
            form.anwenden(&mut stil);
            stil_aendern = true;
        }
        if let Some(farbe) = aenderung.farbe {
            Form::farbe_setzen(&mut stil, farbe);
            stil_aendern = true;
        }
        // Die Beschriftung zuletzt: eine neue Form kann `html=1` einschalten,
        // dann muss auch der vorhandene Text maskiert werden.
        if let Some(text) = aenderung.text {
            setze_wert(eintrag, &als_label(&stil, text));
        } else if ist_html(&stil) && !war_html {
            let alt = wert(eintrag);
            if !alt.is_empty() {
                setze_wert(eintrag, &als_label(&stil, &alt));
            }
        }
        if stil_aendern {
            setze_stil(eintrag, &stil);
        }
        if !aenderung.lage.ist_leer() {
            let alt = lage(eintrag);
            setze_lage(
                eintrag,
                &Lage {
                    x: aenderung.lage.x.unwrap_or(alt.x),
                    y: aenderung.lage.y.unwrap_or(alt.y),
                    breite: aenderung.lage.breite.unwrap_or(alt.breite),
                    hoehe: aenderung.lage.hoehe.unwrap_or(alt.hoehe),
                },
            );
        }
        if let Some(quelle) = aenderung.quelle {
            setze_quelle(
                eintrag,
                quelle.map(str::trim).filter(|wert| !wert.is_empty()),
            );
        }
        if let Some(ziel) = aenderung.ziel {
            setze_ziel(eintrag, ziel.map(str::trim).filter(|wert| !wert.is_empty()));
        }
        geaendert.push(kennung);
    }
    Ok(geaendert)
}

/// Eine neue Zelle, wie ein Agent sie beschreibt.
#[derive(Debug, Clone)]
pub struct NeueZelle<'a> {
    /// Gewünschte Id; fehlt sie, vergibt das Werkzeug eine.
    pub id: Option<&'a str>,
    /// Beschriftung als Text.
    pub text: &'a str,
    /// Form (Knoten).
    pub form: Form,
    /// Füllfarbe (Knoten) bzw. Linienfarbe (Kante).
    pub farbe: Option<Farbe>,
    /// Lage und Größe (Knoten).
    pub lage: Lage,
    /// Elternteil (Ebene, Gruppe, Container); Standard ist die Ebene `1`.
    pub eltern: Option<&'a str>,
    /// Startknoten – gesetzt heißt: es wird eine Kante.
    pub quelle: Option<&'a str>,
    /// Zielknoten.
    pub ziel: Option<&'a str>,
    /// Gestrichelte Linie (Kante).
    pub gestrichelt: bool,
    /// Pfeilspitze am Ziel (Kante).
    pub pfeil: bool,
}

/// Baut eine neue Zelle für eine Seite; prüft Ids und Verbindungen.
pub fn zelle_bauen(root: &Element, neu: &NeueZelle) -> Result<Element, FormatFehler> {
    let vorhandene: Vec<(String, bool)> = eintraege(root)
        .iter()
        .map(|eintrag| {
            (
                id(eintrag).unwrap_or_default().to_string(),
                ist_knoten(eintrag),
            )
        })
        .collect();
    let knoten_ids: BTreeSet<&str> = vorhandene
        .iter()
        .filter(|(_, knoten)| *knoten)
        .map(|(id, _)| id.as_str())
        .collect();

    let id = match neu.id.map(str::trim).filter(|id| !id.is_empty()) {
        Some(id) => {
            if matches!(id, ROOT_ID | EBENE_ID) {
                return Err(FormatFehler::Struktur(format!(
                    "Die Id „{id}“ ist für die festen Zellen 0 und 1 reserviert."
                )));
            }
            if vorhandene.iter().any(|(vorhanden, _)| vorhanden == id) {
                return Err(FormatFehler::Struktur(format!(
                    "Die Id „{id}“ gibt es schon – bitte eine andere wählen oder `id` weglassen."
                )));
            }
            id.to_string()
        }
        None => (1..)
            .map(|nummer| format!("n{nummer}"))
            .find(|kandidat| !vorhandene.iter().any(|(id, _)| id == kandidat))
            .expect("irgendwann ist eine Id frei"),
    };

    let mut eltern = neu.eltern.map(str::trim).filter(|wert| !wert.is_empty());
    if let Some(wert) = eltern {
        if !vorhandene.iter().any(|(id, _)| id == wert) {
            return Err(FormatFehler::Struktur(format!(
                "`eltern` „{wert}“ gibt es auf dieser Seite nicht – bitte eine vorhandene \
                 Gruppe oder Ebene nennen."
            )));
        }
        eltern = Some(wert);
    }

    let kante = neu.quelle.is_some() || neu.ziel.is_some();
    if kante {
        for (feld, ende) in [("quelle", neu.quelle), ("ziel", neu.ziel)] {
            let Some(ende) = ende.map(str::trim).filter(|wert| !wert.is_empty()) else {
                continue;
            };
            if !knoten_ids.contains(ende) {
                return Err(FormatFehler::Struktur(format!(
                    "{feld} „{ende}“ ist kein Knoten dieser Seite – eine Kante braucht \
                     vorhandene Knoten."
                )));
            }
        }
        let mut stil = Stil::aus_text("edgeStyle=orthogonalEdgeStyle;rounded=1;html=1;");
        if neu.gestrichelt {
            stil.setze("dashed", "1");
        }
        stil.setze("endArrow", if neu.pfeil { "classic" } else { "none" });
        if let Some(farbe) = neu.farbe {
            Form::farbe_setzen(&mut stil, farbe);
        }
        let mut zelle = Element::neu("mxCell")
            .mit("id", id)
            .mit("value", als_label(&stil, neu.text))
            .mit("style", stil.als_text())
            .mit("edge", "1")
            .mit("parent", eltern.unwrap_or(EBENE_ID));
        if let Some(quelle) = neu.quelle {
            zelle.setze_attribut("source", quelle);
        }
        if let Some(ziel) = neu.ziel {
            zelle.setze_attribut("target", ziel);
        }
        zelle.anhaengen(
            Element::neu("mxGeometry")
                .mit("relative", "1")
                .mit("as", "geometry"),
        );
        return Ok(zelle);
    }

    let mut stil = neu.form.stil();
    if let Some(farbe) = neu.farbe {
        Form::farbe_setzen(&mut stil, farbe);
    }
    let mut zelle = Element::neu("mxCell")
        .mit("id", id)
        .mit("value", als_label(&stil, neu.text))
        .mit("style", stil.als_text())
        .mit("vertex", "1")
        .mit("parent", eltern.unwrap_or(EBENE_ID));
    let mut geometrie = Element::neu("mxGeometry").mit("as", "geometry");
    geometrie.setze_attribut("x", zahl(neu.lage.x));
    geometrie.setze_attribut("y", zahl(neu.lage.y));
    geometrie.setze_attribut("width", zahl(neu.lage.breite));
    geometrie.setze_attribut("height", zahl(neu.lage.hoehe));
    zelle.anhaengen(geometrie);
    Ok(zelle)
}

/// Hängt neue Zellen an und gibt ihre Ids zurück.
pub fn hinzufuegen(root: &mut Element, neue: &[NeueZelle]) -> Result<Vec<String>, FormatFehler> {
    let mut ids = Vec::new();
    for neu in neue {
        let zelle = zelle_bauen(root, neu)?;
        ids.push(id(&zelle).unwrap_or_default().to_string());
        root.anhaengen(zelle);
    }
    Ok(ids)
}

/// Was beim Entfernen mit Kanten und Kindern geschieht.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Randbehandlung {
    /// Mit entfernen.
    Entfernen,
    /// Behalten (Kanten: Verbindung lösen; Kinder: hochziehen).
    Behalten,
}

impl Randbehandlung {
    /// Liest `entfernen`/`loesen`/`behalten`.
    pub fn aus_text(wert: &str, feld: &str) -> Result<Randbehandlung, FormatFehler> {
        match wert.trim() {
            "entfernen" => Ok(Randbehandlung::Entfernen),
            "loesen" | "behalten" => Ok(Randbehandlung::Behalten),
            "" => Err(FormatFehler::Struktur(format!(
                "`{feld}` ist leer – erlaubt sind `entfernen` und `loesen`/`behalten`."
            ))),
            sonst => Err(FormatFehler::Struktur(format!(
                "`{feld}: {sonst}` kenne ich nicht – erlaubt sind `entfernen` und \
                 `loesen`/`behalten`."
            ))),
        }
    }
}

/// Ein Auftrag zum Entfernen von Zellen.
#[derive(Debug, Clone)]
pub struct Entfernen<'a> {
    /// Die Ids, die weg sollen.
    pub ids: &'a [String],
    /// Behandlung der Kanten, die an ihnen hängen.
    pub kanten: Option<Randbehandlung>,
    /// Behandlung ihrer Kinder.
    pub kinder: Option<Randbehandlung>,
}

struct Bestand {
    id: String,
    eltern: Option<String>,
    kante: bool,
    quelle: Option<String>,
    ziel: Option<String>,
}

impl Bestand {
    /// Beide Enden einer Kante, soweit gesetzt.
    fn enden(&self) -> impl Iterator<Item = &String> {
        [self.quelle.as_ref(), self.ziel.as_ref()]
            .into_iter()
            .flatten()
            .filter(|ende| !ende.trim().is_empty())
    }
}

/// Entfernt Zellen; Kanten und Kinder nur mit ausdrücklicher Angabe.
///
/// Gibt alle entfernten Ids zurück (auch die mit entfernten Kanten und Kinder).
pub fn entfernen(root: &mut Element, auftrag: &Entfernen) -> Result<Vec<String>, FormatFehler> {
    let bestand: Vec<Bestand> = eintraege(root)
        .iter()
        .map(|eintrag| Bestand {
            id: id(eintrag).unwrap_or_default().to_string(),
            eltern: eltern(eintrag).map(str::to_string),
            kante: ist_kante(eintrag),
            quelle: quelle(eintrag).filter(|ende| !ende.trim().is_empty()),
            ziel: ziel(eintrag).filter(|ende| !ende.trim().is_empty()),
        })
        .collect();

    if auftrag.ids.is_empty() {
        return Err(FormatFehler::Struktur(
            "`ids` ist leer – bitte die Zellen nennen, die weg sollen.".to_string(),
        ));
    }

    let mut weg: BTreeSet<String> = BTreeSet::new();
    for kennung in auftrag.ids {
        let kennung = kennung.trim();
        if kennung.is_empty() {
            return Err(FormatFehler::Struktur(
                "Eine leere Id in `ids` – bitte die Zellen nennen.".to_string(),
            ));
        }
        if matches!(kennung, ROOT_ID | EBENE_ID) {
            return Err(FormatFehler::Struktur(format!(
                "Die Zelle „{kennung}“ ist fest (draw.io-Wurzel bzw. Standardebene) und lässt \
                 sich nicht entfernen."
            )));
        }
        if !bestand.iter().any(|eintrag| eintrag.id == kennung) {
            return Err(FormatFehler::Struktur(format!(
                "Zelle „{kennung}“ gibt es auf dieser Seite nicht – bitte mit drawio_lesen die \
                 Ids holen."
            )));
        }
        weg.insert(kennung.to_string());
    }

    // Kinder: ohne ausdrückliche Angabe wird nichts entfernt.
    let mut grenze = vec![weg.iter().cloned().collect::<Vec<_>>()];
    loop {
        let lauf = grenze.pop().unwrap_or_default();
        if lauf.is_empty() {
            break;
        }
        let mut naechste = Vec::new();
        for eltern_id in lauf {
            let kinder: Vec<&Bestand> = bestand
                .iter()
                .filter(|eintrag| eintrag.eltern.as_deref() == Some(eltern_id.as_str()))
                .collect();
            if kinder.is_empty() {
                continue;
            }
            match auftrag.kinder {
                None => {
                    return Err(FormatFehler::Struktur(format!(
                        "Zelle „{eltern_id}“ hat Kinder ({}). Bitte ausdrücklich sagen, was mit \
                         ihnen geschieht: `kinder: \"entfernen\"` oder `kinder: \"behalten\"` \
                         (hochziehen).",
                        kinder
                            .iter()
                            .map(|kind| kind.id.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )))
                }
                Some(Randbehandlung::Entfernen) => {
                    for kind in kinder {
                        if weg.insert(kind.id.clone()) {
                            naechste.push(kind.id.clone());
                        }
                    }
                }
                Some(Randbehandlung::Behalten) => {}
            }
        }
        grenze.push(naechste);
    }

    // Kanten, die an entfernten Knoten hängen.
    loop {
        let mut neu = Vec::new();
        for eintrag in &bestand {
            if !eintrag.kante || weg.contains(&eintrag.id) {
                continue;
            }
            if eintrag.enden().any(|ende| weg.contains(ende)) {
                match auftrag.kanten {
                    None => {
                        return Err(FormatFehler::Struktur(format!(
                            "Die Zelle(n) {} haben Kanten ({}). Bitte ausdrücklich sagen, was \
                             damit geschieht: `kanten: \"entfernen\"` (mit entfernen) oder \
                             `kanten: \"loesen\"` (Kante behalten, Verbindung lösen).",
                            eintrag
                                .enden()
                                .filter(|ende| weg.contains(*ende))
                                .cloned()
                                .collect::<Vec<_>>()
                                .join(", "),
                            eintrag.id
                        )))
                    }
                    Some(Randbehandlung::Entfernen) => neu.push(eintrag.id.clone()),
                    Some(Randbehandlung::Behalten) => {}
                }
            }
        }
        if neu.is_empty() {
            break;
        }
        for kennung in neu {
            weg.insert(kennung);
        }
    }

    // Behalten: Kinder hochziehen, Kanten lösen.
    let mut umhaengen: Vec<(String, String)> = Vec::new();
    let mut loesen: Vec<(String, bool, bool)> = Vec::new();
    if auftrag.kinder == Some(Randbehandlung::Behalten) {
        for eintrag in &bestand {
            if weg.contains(&eintrag.id) {
                continue;
            }
            if let Some(eltern) = &eintrag.eltern {
                if weg.contains(eltern) {
                    umhaengen.push((eintrag.id.clone(), eltern_ersatz(eltern, &weg, &bestand)));
                }
            }
        }
    }
    if auftrag.kanten == Some(Randbehandlung::Behalten) {
        for eintrag in &bestand {
            if !eintrag.kante || weg.contains(&eintrag.id) {
                continue;
            }
            let quelle_weg = eintrag
                .quelle
                .as_ref()
                .is_some_and(|ende| weg.contains(ende));
            let ziel_weg = eintrag.ziel.as_ref().is_some_and(|ende| weg.contains(ende));
            if quelle_weg || ziel_weg {
                loesen.push((eintrag.id.clone(), quelle_weg, ziel_weg));
            }
        }
    }

    for eintrag in eintraege_mut(root) {
        let Some(kennung) = id(eintrag).map(str::to_string) else {
            continue;
        };
        if let Some((_, eltern)) = umhaengen.iter().find(|(id, _)| *id == kennung) {
            setze_eltern(eintrag, eltern);
        }
        if let Some((_, quelle_weg, ziel_weg)) = loesen.iter().find(|(id, _, _)| *id == kennung) {
            if *quelle_weg {
                setze_quelle(eintrag, None);
            }
            if *ziel_weg {
                setze_ziel(eintrag, None);
            }
        }
    }

    root.kinder.retain(|knoten| match knoten {
        Knoten::Element(element) => {
            if !ist_eintrag(element) {
                return true;
            }
            !id(element).is_some_and(|id| weg.contains(id))
        }
        Knoten::Text(_) => true,
    });

    Ok(weg.into_iter().collect())
}

/// Der nächste Elternteil, der bleibt (oder die Standardebene).
fn eltern_ersatz(eltern: &str, weg: &BTreeSet<String>, bestand: &[Bestand]) -> String {
    let mut lauf = Some(eltern.to_string());
    for _ in 0..MAX_ELTERN_TIEFE {
        let Some(kennung) = lauf else {
            break;
        };
        if !weg.contains(&kennung) {
            return kennung;
        }
        lauf = bestand
            .iter()
            .find(|eintrag| eintrag.id == kennung)
            .and_then(|eintrag| eintrag.eltern.clone());
    }
    EBENE_ID.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DATEI: &str = r#"<mxfile host="test" pages="2">
  <diagram id="s1" name="Start">
    <mxGraphModel dx="0" dy="0">
      <root>
        <mxCell id="0"/>
        <mxCell id="1" parent="0"/>
        <UserObject id="a" label="Antrag &amp; Prüfung" link="https://example.test">
          <mxCell style="rounded=1;html=1;" vertex="1" parent="1">
            <mxGeometry x="10" y="20" width="120" height="60" as="geometry"/>
          </mxCell>
        </UserObject>
        <mxCell id="b" value="Entscheidung" style="rhombus;html=1;" vertex="1" parent="1">
          <mxGeometry x="10" y="120" width="120" height="60" as="geometry"/>
        </mxCell>
        <mxCell id="k" value="ja" style="endArrow=classic;" edge="1" source="a" target="b" parent="1">
          <mxGeometry relative="1" as="geometry"/>
        </mxCell>
      </root>
    </mxGraphModel>
  </diagram>
  <diagram id="s2" name="Start">
    <mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/></root></mxGraphModel>
  </diagram>
</mxfile>"#;

    fn seite_mit(text: &str) -> Datei {
        Datei::aus_text(text).expect("Datei ist gültig")
    }

    #[test]
    fn seiten_werden_gelesen_und_geprueft() {
        let datei = seite_mit(DATEI);
        let seiten = datei.seiten().unwrap();
        assert_eq!(seiten.len(), 2);
        assert_eq!(seiten[0].id, "s1");
        assert_eq!(seiten[0].name, "Start");
        assert!(!seiten[0].komprimiert);
        assert_eq!(seiten[0].zellen, 5);
        assert_eq!(seiten[1].id, "s2");
    }

    #[test]
    fn die_seite_wird_ueber_id_oder_name_gefunden() {
        let datei = seite_mit(DATEI);
        assert_eq!(datei.seite_index(None).unwrap(), 0);
        assert_eq!(datei.seite_index(Some("s2")).unwrap(), 1);
        // Im Original heißen beide Seiten gleich → mehrdeutig, Kandidaten im Text.
        let fehler = datei.seite_index(Some("Start")).unwrap_err().to_string();
        assert!(fehler.contains("mehrdeutig"), "{fehler}");
        assert!(fehler.contains("s1"), "{fehler}");
        assert!(fehler.contains("s2"), "{fehler}");
        let fehler = datei
            .seite_index(Some("gibt-es-nicht"))
            .unwrap_err()
            .to_string();
        assert!(fehler.contains("gibt es nicht"), "{fehler}");
    }

    #[test]
    fn unbekannte_attribute_und_hullen_bleiben_erhalten() {
        let mut datei = seite_mit(DATEI);
        let index = datei.seite_index(Some("s1")).unwrap();
        let mut modell = datei.modell(index).unwrap();
        let root = modell.kind("root").unwrap().clone();
        let zellen = zellen(&root);
        assert_eq!(zellen[0].id, "a");
        assert_eq!(zellen[0].text, "Antrag & Prüfung");
        assert_eq!(zellen[0].form, Some(Form::Rechteck));
        assert_eq!(zellen[1].form, Some(Form::Raute));

        // Den Text der Hülle ändern; die Hülle selbst bleibt.
        let mut root = modell.kind("root").unwrap().clone();
        aendern(
            &mut root,
            &[Aenderung {
                id: "a",
                text: Some("<b>neu</b>"),
                farbe: None,
                form: None,
                lage: LageAenderung::default(),
                quelle: None,
                ziel: None,
            }],
        )
        .unwrap();
        root_setzen(&mut modell, root);

        datei.modell_setzen(index, modell).unwrap();
        let text = datei.als_text();
        assert!(text.contains("host=\"test\""), "{text}");
        assert!(text.contains("pages=\"2\""), "{text}");
        assert!(text.contains("dx=\"0\""), "{text}");
        assert!(text.contains("link=\"https://example.test\""), "{text}");
        assert!(text.contains("style=\"rounded=1;html=1;\""), "{text}");
        assert!(text.contains("width=\"120\""), "{text}");
        // Der neue Text steht als Text in der Hülle: für das HTML maskiert
        // (`html=1`) und darüber für das XML.
        assert!(
            text.contains("label=\"&amp;lt;b&amp;gt;neu&amp;lt;/b&amp;gt;\""),
            "{text}"
        );
    }

    #[test]
    fn eine_komprimierte_seite_wird_entpackt() {
        let modell = leeres_modell();
        let gepackt = link::komprimieren(&modell.als_text()).unwrap();
        let datei = format!(
            "<mxfile host=\"test\" compressed=\"true\"><diagram id=\"s1\" name=\"A\">{gepackt}</diagram></mxfile>"
        );
        let datei = seite_mit(&datei);
        let seiten = datei.seiten().unwrap();
        assert!(seiten[0].komprimiert);
        assert_eq!(seiten[0].zellen, 2);
        assert!(datei.modell(0).unwrap().kind("root").is_some());
    }

    #[test]
    fn eine_fehlende_zelle_null_wird_abgelehnt() {
        let text = "<mxfile><diagram id=\"s\" name=\"x\"><mxGraphModel><root>\
                    <mxCell id=\"1\" parent=\"0\"/></root></mxGraphModel></diagram></mxfile>";
        let fehler = Datei::aus_text(text).unwrap_err().to_string();
        assert!(fehler.contains("zwei festen Zellen"), "{fehler}");
    }

    #[test]
    fn eine_kante_auf_eine_unbekannte_zelle_wird_abgelehnt() {
        let text = "<mxfile><diagram id=\"s\" name=\"x\"><mxGraphModel><root>\
                    <mxCell id=\"0\"/><mxCell id=\"1\" parent=\"0\"/>\
                    <mxCell id=\"k\" edge=\"1\" source=\"a\" target=\"b\" parent=\"1\"/>\
                    </root></mxGraphModel></diagram></mxfile>";
        let fehler = Datei::aus_text(text).unwrap_err().to_string();
        assert!(fehler.contains("gibt es auf der Seite nicht"), "{fehler}");
    }

    #[test]
    fn ein_fremdes_link_schema_wird_abgelehnt() {
        let text = "<mxfile><diagram id=\"s\" name=\"x\"><mxGraphModel><root>\
                    <mxCell id=\"0\"/><mxCell id=\"1\" parent=\"0\"/>\
                    <UserObject id=\"a\" label=\"x\" link=\"javascript:alert(1)\">\
                    <mxCell vertex=\"1\" parent=\"1\"/></UserObject>\
                    </root></mxGraphModel></diagram></mxfile>";
        let fehler = Datei::aus_text(text).unwrap_err().to_string();
        assert!(
            fehler.contains("erlaubt sind nur http(s) und mailto"),
            "{fehler}"
        );
    }

    #[test]
    fn doppelte_ids_werden_abgelehnt() {
        let text = "<mxfile><diagram id=\"s\" name=\"x\"><mxGraphModel><root>\
                    <mxCell id=\"0\"/><mxCell id=\"1\" parent=\"0\"/>\
                    <mxCell id=\"a\" vertex=\"1\" parent=\"1\"/>\
                    <mxCell id=\"a\" vertex=\"1\" parent=\"1\"/>\
                    </root></mxGraphModel></diagram></mxfile>";
        let fehler = Datei::aus_text(text).unwrap_err().to_string();
        assert!(fehler.contains("doppelt"), "{fehler}");
    }

    #[test]
    fn aendern_prueft_vor_der_ersten_aenderung() {
        let datei = seite_mit(DATEI);
        let modell = datei.modell(0).unwrap();
        let mut root = modell.kind("root").unwrap().clone();
        let aenderungen = [
            Aenderung {
                id: "a",
                text: Some("geändert"),
                farbe: None,
                form: None,
                lage: LageAenderung::default(),
                quelle: None,
                ziel: None,
            },
            Aenderung {
                id: "gibt-es-nicht",
                text: Some("x"),
                farbe: None,
                form: None,
                lage: LageAenderung::default(),
                quelle: None,
                ziel: None,
            },
        ];
        assert!(aendern(&mut root, &aenderungen).is_err());
        // Nichts wurde geschrieben: der Text von „a“ ist noch der alte.
        let zellen = zellen(&root);
        assert_eq!(zellen[0].text, "Antrag & Prüfung");
    }

    #[test]
    fn entfernen_braucht_ausdrueckliche_angaben() {
        let datei = seite_mit(DATEI);
        let modell = datei.modell(0).unwrap();
        let mut root = modell.kind("root").unwrap().clone();

        // Ohne `kanten` wird nichts entfernt.
        let fehler = entfernen(
            &mut root,
            &Entfernen {
                ids: &["a".to_string()],
                kanten: None,
                kinder: None,
            },
        )
        .unwrap_err()
        .to_string();
        assert!(fehler.contains("Kanten"), "{fehler}");
        assert_eq!(eintraege(&root).len(), 5);

        // `loesen` behält die Kante und entfernt nur die Verbindung.
        let weg = entfernen(
            &mut root,
            &Entfernen {
                ids: &["a".to_string()],
                kanten: Some(Randbehandlung::Behalten),
                kinder: None,
            },
        )
        .unwrap();
        assert_eq!(weg, vec!["a".to_string()]);
        let kante = eintraege(&root)
            .into_iter()
            .find(|eintrag| id(eintrag) == Some("k"))
            .cloned()
            .unwrap();
        assert_eq!(quelle(&kante), None);
        assert_eq!(ziel(&kante), Some("b".to_string()));
    }

    #[test]
    fn entfernen_nimmt_kanten_und_kinder_mit() {
        let text = "<mxfile><diagram id=\"s\" name=\"x\"><mxGraphModel><root>\
                    <mxCell id=\"0\"/><mxCell id=\"1\" parent=\"0\"/>\
                    <mxCell id=\"g\" value=\"Gruppe\" style=\"swimlane;\" vertex=\"1\" parent=\"1\"/>\
                    <mxCell id=\"a\" value=\"Kind\" vertex=\"1\" parent=\"g\"/>\
                    <mxCell id=\"b\" value=\"Ziel\" vertex=\"1\" parent=\"1\"/>\
                    <mxCell id=\"k\" edge=\"1\" source=\"a\" target=\"b\" parent=\"1\"/>\
                    </root></mxGraphModel></diagram></mxfile>";
        let datei = seite_mit(text);
        let modell = datei.modell(0).unwrap();
        let mut root = modell.kind("root").unwrap().clone();
        let weg = entfernen(
            &mut root,
            &Entfernen {
                ids: &["g".to_string()],
                kanten: Some(Randbehandlung::Entfernen),
                kinder: Some(Randbehandlung::Entfernen),
            },
        )
        .unwrap();
        assert_eq!(weg, vec!["a", "g", "k"]);
        let uebrig: Vec<String> = eintraege(&root)
            .iter()
            .map(|eintrag| id(eintrag).unwrap_or_default().to_string())
            .collect();
        assert_eq!(uebrig, vec!["0", "1", "b"]);
    }

    #[test]
    fn neue_zellen_bekommen_ids_und_kanten_brauchen_knoten() {
        let modell = leeres_modell();
        let mut root = modell.kind("root").unwrap().clone();
        let ids = hinzufuegen(
            &mut root,
            &[NeueZelle {
                id: None,
                text: "Start",
                form: Form::Ellipse,
                farbe: Some(Farbe::Gruen),
                lage: Lage::neu(10.0, 20.0, 120.0, 60.0),
                eltern: None,
                quelle: None,
                ziel: None,
                gestrichelt: false,
                pfeil: false,
            }],
        )
        .unwrap();
        assert_eq!(ids, vec!["n1".to_string()]);

        let fehler = hinzufuegen(
            &mut root,
            &[NeueZelle {
                id: None,
                text: "Kante",
                form: Form::Rechteck,
                farbe: None,
                lage: Lage::neu(0.0, 0.0, 0.0, 0.0),
                eltern: None,
                quelle: Some("n1"),
                ziel: Some("fehlt"),
                gestrichelt: false,
                pfeil: true,
            }],
        )
        .unwrap_err()
        .to_string();
        assert!(fehler.contains("kein Knoten"), "{fehler}");

        let ids = hinzufuegen(
            &mut root,
            &[NeueZelle {
                id: None,
                text: "weiter",
                form: Form::Rechteck,
                farbe: None,
                lage: Lage::neu(10.0, 200.0, 120.0, 60.0),
                eltern: None,
                quelle: None,
                ziel: None,
                gestrichelt: false,
                pfeil: false,
            }],
        )
        .unwrap();
        let ids_kante = hinzufuegen(
            &mut root,
            &[NeueZelle {
                id: None,
                text: "dann",
                form: Form::Rechteck,
                farbe: None,
                lage: Lage::neu(0.0, 0.0, 0.0, 0.0),
                eltern: None,
                quelle: Some("n1"),
                ziel: Some(ids[0].as_str()),
                gestrichelt: true,
                pfeil: true,
            }],
        )
        .unwrap();
        assert_eq!(ids_kante, vec!["n3".to_string()]);
        pruefe_modell(&modell_bauen(root, 400.0, 400.0)).unwrap();
    }

    #[test]
    fn eine_neue_seite_bekommt_eine_freie_id() {
        let mut datei = seite_mit(DATEI);
        let index = datei
            .seite_hinzufuegen("Weiter", None, leeres_modell())
            .unwrap();
        assert_eq!(index, 2);
        let seiten = datei.seiten().unwrap();
        assert_eq!(seiten.len(), 3);
        assert_eq!(seiten[2].name, "Weiter");
        assert_eq!(seiten[2].id, "seite-1");
        assert!(datei.als_text().contains("pages=\"3\""));
        // Eine doppelte Seiten-Id wird abgelehnt.
        assert!(datei
            .seite_hinzufuegen("Nochmal", Some("s1"), leeres_modell())
            .is_err());
    }

    #[test]
    fn beschriftungen_sind_bei_html_maskiert_und_lesen_sich_gleich() {
        let html = Stil::aus_text("rounded=1;html=1;");
        let schlicht = Stil::aus_text("rounded=1;");
        for text in [
            "Größe <neu> & \"alt\"",
            "<img src=x onerror=alert(1)>",
            "&lt;schon maskiert&gt;",
            "eins\nzwei",
        ] {
            assert_eq!(als_label(&schlicht, text), text);
            let label = als_label(&html, text);
            assert!(!label.replace("<br>", "").contains('<'), "{label}");
            for (stil, wert) in [("html=1;", label.as_str()), ("rounded=1;", text)] {
                let zelle = Element::neu("mxCell")
                    .mit("id", "a")
                    .mit("value", wert)
                    .mit("style", stil)
                    .mit("vertex", "1");
                assert_eq!(super::text(&zelle), text);
            }
        }
    }
}
