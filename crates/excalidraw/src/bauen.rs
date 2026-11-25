// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Aus einer Beschreibung werden Elemente – und aus zwei Elementen ein Pfeil.
//!
//! Die Wege hier erzeugen **gültige** Elemente: alle Pflichtfelder sind gesetzt,
//! jeder Text in einer Form hängt über `containerId` **und** `boundElements`
//! an seinem Behälter, jeder Pfeil über `startBinding`/`endBinding` **und**
//! `boundElements` an seinen Enden. Was ein Aufruf nicht versteht (etwa
//! `freedraw` von Hand), sagt er mit einer Meldung, statt etwas Halbes zu
//! schreiben.

use std::collections::BTreeMap;

use quiver_diagramm::layout::layout;
use quiver_diagramm::modell::{Diagramm, Farbe, KanteArt};
use quiver_diagramm::zeichnen::palette;
use serde_json::{json, Value};

use crate::datei::{Binding, Datei, Element, Ende};
use crate::farben;
use crate::fehler::Fehler;

/// Schriftgröße der Texte in der Datei.
pub const TEXTGROESSE: f64 = 16.0;
/// Zeilenhöhe im Verhältnis zur Schriftgröße.
pub const ZEILENFAKTOR: f64 = 1.25;
/// Innenabstand eines Textes in seiner Form.
pub const TEXT_INNEN: f64 = 12.0;
/// Breite einer neuen Form ohne Angabe.
pub const STANDARD_BREITE: f64 = 160.0;
/// Höhe einer neuen Form ohne Angabe.
pub const STANDARD_HOEHE: f64 = 60.0;
/// Breite eines freien Textes ohne Angabe.
pub const TEXT_BREITE: f64 = 240.0;
/// Schriftfamilie 5 ist Excalidraws „Excalifont“ (Standard).
pub const SCHRIFT_FAMILIE: i64 = 5;

/// Arten, die `excalidraw_elemente_hinzufuegen` anlegen kann.
pub const ARTEN: &[&str] = &[
    "rectangle",
    "diamond",
    "ellipse",
    "text",
    "line",
    "arrow",
    "frame",
    "image",
];

/// Eine Vorlage für ein neues Element (Parameter von
/// `excalidraw_elemente_hinzufuegen`, schon übersetzt).
#[derive(Debug, Clone, Default)]
pub struct Vorlage {
    /// Art des Elements (`rectangle`, `text`, `arrow` …).
    pub art: String,
    /// Gewünschte Id; ohne Angabe wird eine freie gewählt.
    pub id: Option<String>,
    /// Text im Element oder als Beschriftung am Pfeil.
    pub text: Option<String>,
    /// Linke Kante; ohne Angabe unter dem vorhandenen Inhalt.
    pub x: Option<f64>,
    /// Oberkante.
    pub y: Option<f64>,
    /// Breite.
    pub breite: Option<f64>,
    /// Höhe.
    pub hoehe: Option<f64>,
    /// Füllfarbe aus der festen Liste.
    pub farbe: Option<Farbe>,
    /// Überschrift eines Rahmens.
    pub titel: Option<String>,
    /// Ausgangselement eines Pfeils.
    pub von: Option<String>,
    /// Zielelement eines Pfeils.
    pub nach: Option<String>,
    /// Art der Kante (`pfeil`, `linie`, `gestrichelt`).
    pub kante: Option<KanteArt>,
}

/// Ein Diagramm aus dem Modell: Layout rechnen, Elemente bauen.
pub fn aus_diagramm(diagramm: &Diagramm) -> Result<Datei, Fehler> {
    let zeichnung = layout(diagramm)?;
    let mut datei = Datei::neu();

    // 1. Gruppen werden zu Rahmen (Frames) – sie liegen hinten.
    let mut rahmen: BTreeMap<String, String> = BTreeMap::new();
    for gruppe in &zeichnung.gruppen {
        let id = format!("gruppe-{}", gruppe.id);
        let mut element = Element::neu(
            "frame",
            f64::from(gruppe.x),
            f64::from(gruppe.y),
            f64::from(gruppe.breite),
            f64::from(gruppe.hoehe),
        );
        element.id = id.clone();
        element.name = Some(gruppe.titel.clone());
        element.stroke_color = palette::GRUPPE.to_string();
        element.bound_elements = Some(Vec::new());
        datei.elements.push(element);
        rahmen.insert(gruppe.id.clone(), id);
    }

    // 2. Knoten als Form mit gebundenem Text.
    let mut knoten: BTreeMap<String, String> = BTreeMap::new();
    for platz in &zeichnung.knoten {
        let id = format!("knoten-{}", platz.id);
        let rahmen_id = platz
            .gruppe
            .as_ref()
            .and_then(|gruppe| rahmen.get(gruppe))
            .cloned();
        let mut form = form(
            farben::art_der_form(platz.form),
            f64::from(platz.x),
            f64::from(platz.y),
            f64::from(platz.breite),
            f64::from(platz.hoehe),
            platz.farbe,
            rahmen_id.as_deref(),
        );
        form.id = id.clone();
        // Der Text kommt **nach** seiner Form in die Datei: so liegt er in
        // Excalidraw (und im eigenen Bild) über der Fläche, nicht darunter.
        let mut text = None;
        if !platz.text.trim().is_empty() {
            let mut gebunden = text_im(&platz.text, &form, platz.farbe, "center");
            gebunden.id = format!("{id}-text");
            form.bindung_eintragen(Binding::text(gebunden.id.clone()));
            text = Some(gebunden);
        }
        datei.elements.push(form);
        if let Some(text) = text {
            datei.elements.push(text);
        }
        knoten.insert(platz.id.clone(), id);
    }

    // 3. Kanten als Pfeil oder Linie, mit Beschriftung.
    for (nummer, kante) in zeichnung.kanten.iter().enumerate() {
        let (Some(von), Some(nach)) = (knoten.get(&kante.von), knoten.get(&kante.nach)) else {
            continue;
        };
        let mut pfeil = Element::neu(
            if kante.art == KanteArt::Linie {
                "line"
            } else {
                "arrow"
            },
            0.0,
            0.0,
            0.0,
            0.0,
        );
        let id = format!("kante-{nummer}");
        pfeil.id = id.clone();
        let punkte: Vec<(f64, f64)> = kante
            .punkte
            .iter()
            .map(|punkt| (f64::from(punkt.x), f64::from(punkt.y)))
            .collect();
        pfeil.punkte_setzen(&punkte);
        stil_setzen(&mut pfeil, kante.art);
        pfeil.bindung_setzen(Ende::Start, Some(von));
        pfeil.bindung_setzen(Ende::Schluss, Some(nach));
        pfeil.beruehren();
        datei.elements.push(pfeil);

        if let Some(ziel) = datei.element_mut(von) {
            ziel.bindung_eintragen(Binding::pfeil(id.clone()));
        }
        if let Some(ziel) = datei.element_mut(nach) {
            ziel.bindung_eintragen(Binding::pfeil(id.clone()));
        }

        if let Some(beschriftung) = &kante.beschriftung {
            // Die Breite der Beschriftung wird an der eigenen Schrift gemessen:
            // das Layout hat mit derselben Schrift gemessen, aber etwas enger.
            let breite =
                (farben::textbreite(&beschriftung.text, TEXTGROESSE) + 2.0).clamp(24.0, 240.0);
            let mut text = freier_text(
                &beschriftung.text,
                f64::from(beschriftung.position.x),
                f64::from(beschriftung.position.y),
                breite,
                None,
            );
            text.id = format!("{id}-beschriftung");
            text.text_align = Some("center".to_string());
            text.container_id = Some(id.clone());
            if let Some(ziel) = datei.element_mut(&id) {
                ziel.bindung_eintragen(Binding::text(text.id.clone()));
            }
            datei.elements.push(text);
        }
    }

    Ok(datei)
}

/// Eine Form mit allen Pflichtfeldern.
pub fn form(
    art: &str,
    x: f64,
    y: f64,
    breite: f64,
    hoehe: f64,
    farbe: Option<Farbe>,
    rahmen: Option<&str>,
) -> Element {
    let mut element = Element::neu(art, x, y, breite.max(1.0), hoehe.max(1.0));
    element.frame_id = rahmen.map(str::to_string);
    element.roundness = Some(match art {
        "rectangle" => json!({ "type": 3 }),
        _ => json!({ "type": 2 }),
    });
    if let Some(farbe) = farbe {
        let toene = farben::toene(farbe);
        element.background_color = toene.fuellung.to_string();
        element.stroke_color = toene.rand.to_string();
    }
    element.bound_elements = Some(Vec::new());
    element
}

/// Ein Text, der in einer Form liegt (beidseitig gebunden).
pub fn text_im(
    inhalt: &str,
    behaelter: &Element,
    farbe: Option<Farbe>,
    ausrichtung: &str,
) -> Element {
    let innen = TEXT_INNEN.max(1.0);
    let max_breite = (behaelter.width - 2.0 * innen).max(24.0);
    let mut element = text_element(inhalt, max_breite, farbe);
    element.container_id = Some(behaelter.id.clone());
    element.auto_resize = Some(false);
    element.text_align = Some(ausrichtung.to_string());
    element.text_width_ausrichten(behaelter);
    element
}

/// Ein freier Text an einer Stelle.
pub fn freier_text(inhalt: &str, x: f64, y: f64, breite: f64, farbe: Option<Farbe>) -> Element {
    let mut element = text_element(inhalt, breite.max(24.0), farbe);
    element.x = x;
    element.y = y;
    element
}

/// Der gemeinsame Teil aller Texte: umbrechen, messen, Felder setzen.
fn text_element(inhalt: &str, breite: f64, farbe: Option<Farbe>) -> Element {
    let zeilen = farben::umbrechen(inhalt, TEXTGROESSE, breite);
    let umbrochen = zeilen.join("\n");
    let gemessen = zeilen
        .iter()
        .map(|zeile| farben::textbreite(zeile, TEXTGROESSE))
        .fold(0.0_f64, f64::max);
    let hoehe = zeilen.len() as f64 * TEXTGROESSE * ZEILENFAKTOR;

    let mut element = Element::neu("text", 0.0, 0.0, gemessen.max(1.0), hoehe.max(1.0));
    element.text = Some(umbrochen);
    element.original_text = Some(inhalt.to_string());
    element.font_size = Some(TEXTGROESSE);
    element.font_family = Some(json!(SCHRIFT_FAMILIE));
    element.text_align = Some("left".to_string());
    element.vertical_align = Some("middle".to_string());
    element.line_height = Some(json!(ZEILENFAKTOR));
    element.auto_resize = Some(false);
    element.background_color = "transparent".to_string();
    element.stroke_color = farbe
        .map(|farbe| farben::toene(farbe).text.to_string())
        .unwrap_or_else(|| palette::TEXT.to_string());
    element
}

impl Element {
    /// Setzt Größe und Lage eines Textes mittig in seinen Behälter.
    fn text_width_ausrichten(&mut self, behaelter: &Element) {
        let mitte = behaelter.mitte();
        self.x = mitte.0 - self.width / 2.0;
        self.y = mitte.1 - self.height / 2.0;
    }
}

/// Strichart und Pfeilspitze einer Kante setzen (auch an einem vorhandenen Pfeil).
pub fn stil_setzen(element: &mut Element, art: KanteArt) {
    let (strich, pfeil) = farben::kantenstil(art);
    element.stroke_style = strich.to_string();
    element.start_arrowhead = Some(Value::Null);
    element.end_arrowhead = Some(if pfeil { json!("arrow") } else { Value::Null });
    element.weitere.insert("polygon".to_string(), json!(false));
    element.weitere.insert("elbowed".to_string(), json!(false));
}

/// Ein Pfeil zwischen zwei Elementen: Anfang und Ende am Rand der Kästen.
pub fn pfeil_zwischen(von: &Element, nach: &Element, art: KanteArt) -> Element {
    let start = randpunkt(von, nach.mitte());
    let schluss = randpunkt(nach, von.mitte());
    let mut pfeil = Element::neu(
        if art == KanteArt::Linie {
            "line"
        } else {
            "arrow"
        },
        0.0,
        0.0,
        0.0,
        0.0,
    );
    pfeil.punkte_setzen(&[start, schluss]);
    stil_setzen(&mut pfeil, art);
    pfeil.bindung_setzen(Ende::Start, Some(&von.id));
    pfeil.bindung_setzen(Ende::Schluss, Some(&nach.id));
    pfeil.bound_elements = Some(Vec::new());
    pfeil
}

/// Der Punkt auf dem Rand eines Elements in Richtung `zu`.
///
/// Bei einer Ellipse und einer Raute wird der echte Rand genommen, sonst der
/// Kasten. Ein Element ohne Fläche ergibt seinen Mittelpunkt.
pub fn randpunkt(element: &Element, zu: (f64, f64)) -> (f64, f64) {
    let (mitte_x, mitte_y) = element.mitte();
    let dx = zu.0 - mitte_x;
    let dy = zu.1 - mitte_y;
    if dx.abs() < f64::EPSILON && dy.abs() < f64::EPSILON {
        return (mitte_x, mitte_y);
    }
    let halbe_breite = element.width / 2.0;
    let halbe_hoehe = element.height / 2.0;
    if halbe_breite <= f64::EPSILON || halbe_hoehe <= f64::EPSILON {
        return (mitte_x, mitte_y);
    }

    let anteil = match element.art.as_str() {
        "ellipse" => {
            let a = dx / halbe_breite;
            let b = dy / halbe_hoehe;
            1.0 / (a * a + b * b).sqrt()
        }
        "diamond" => 1.0 / ((dx / halbe_breite).abs() + (dy / halbe_hoehe).abs()),
        _ => {
            let waagerecht = if dx.abs() > f64::EPSILON {
                halbe_breite / dx.abs()
            } else {
                f64::INFINITY
            };
            let senkrecht = if dy.abs() > f64::EPSILON {
                halbe_hoehe / dy.abs()
            } else {
                f64::INFINITY
            };
            waagerecht.min(senkrecht)
        }
    };
    if !anteil.is_finite() {
        return (mitte_x, mitte_y);
    }
    (mitte_x + dx * anteil, mitte_y + dy * anteil)
}

/// Eine Stelle unter dem vorhandenen Inhalt, für Elemente ohne `x`/`y`.
pub fn freie_stelle(datei: &Datei) -> (f64, f64) {
    let unten = datei
        .elements
        .iter()
        .map(|element| element.y + element.height)
        .fold(f64::NEG_INFINITY, f64::max);
    if unten.is_finite() {
        (0.0, unten + 40.0)
    } else {
        (0.0, 0.0)
    }
}

/// Ein Element aus einer Vorlage anlegen (Weg von
/// `excalidraw_elemente_hinzufuegen`).
pub fn hinzufuegen(datei: &mut Datei, vorlage: &Vorlage) -> Result<String, Fehler> {
    let art = vorlage.art.trim().to_ascii_lowercase();
    if !ARTEN.contains(&art.as_str()) {
        return Err(Fehler::Ungueltig(format!(
            "Art „{}“ kennt dieses Werkzeug nicht – erlaubt sind: {}",
            vorlage.art,
            ARTEN.join(", ")
        )));
    }
    let (standard_x, standard_y) = freie_stelle(datei);
    let x = vorlage.x.unwrap_or(standard_x);
    let y = vorlage.y.unwrap_or(standard_y);

    match art.as_str() {
        "rectangle" | "diamond" | "ellipse" => {
            let breite = vorlage.breite.unwrap_or(STANDARD_BREITE).max(1.0);
            let hoehe = vorlage.hoehe.unwrap_or(STANDARD_HOEHE).max(1.0);
            let mut element = form(&art, x, y, breite, hoehe, vorlage.farbe, None);
            element.id = vorlage.id.clone().unwrap_or_default();
            let id = datei.hinzufuegen(element)?;
            let mut meldung = format!("{art} „{id}“ angelegt");
            if let Some(inhalt) = &vorlage.text {
                if !inhalt.trim().is_empty() {
                    let behaelter = datei.element(&id).cloned().expect("gerade angelegt");
                    let text = text_im(inhalt, &behaelter, vorlage.farbe, "center");
                    let text_id = datei.hinzufuegen(text)?;
                    if let Some(ziel) = datei.element_mut(&id) {
                        ziel.bindung_eintragen(Binding::text(text_id.clone()));
                        ziel.beruehren();
                    }
                    meldung.push_str(&format!(", Text „{text_id}“ darin"));
                }
            }
            Ok(meldung)
        }
        "text" => {
            let breite = vorlage.breite.unwrap_or(TEXT_BREITE).max(24.0);
            let mut element = freier_text(
                vorlage.text.as_deref().unwrap_or(""),
                x,
                y,
                breite,
                vorlage.farbe,
            );
            element.id = vorlage.id.clone().unwrap_or_default();
            let id = datei.hinzufuegen(element)?;
            Ok(format!("text „{id}“ angelegt"))
        }
        "frame" => {
            let breite = vorlage.breite.unwrap_or(400.0).max(1.0);
            let hoehe = vorlage.hoehe.unwrap_or(300.0).max(1.0);
            let mut element = Element::neu("frame", x, y, breite, hoehe);
            element.id = vorlage.id.clone().unwrap_or_default();
            element.name = vorlage.titel.clone().or_else(|| vorlage.text.clone());
            element.stroke_color = palette::GRUPPE.to_string();
            element.bound_elements = Some(Vec::new());
            let id = datei.hinzufuegen(element)?;
            Ok(format!("frame „{id}“ angelegt"))
        }
        "image" => {
            let breite = vorlage.breite.unwrap_or(STANDARD_BREITE).max(1.0);
            let hoehe = vorlage.hoehe.unwrap_or(STANDARD_HOEHE).max(1.0);
            let mut element = Element::neu("image", x, y, breite, hoehe);
            element.id = vorlage.id.clone().unwrap_or_default();
            element.stroke_color = "transparent".to_string();
            element.bound_elements = Some(Vec::new());
            element.weitere.insert("fileId".to_string(), Value::Null);
            element
                .weitere
                .insert("status".to_string(), json!("pending"));
            element.weitere.insert("scale".to_string(), json!([1, 1]));
            element.weitere.insert("crop".to_string(), Value::Null);
            let id = datei.hinzufuegen(element)?;
            Ok(format!(
                "image „{id}“ als Platzhalter angelegt – eine Bilddatei kann dieses Werkzeug nicht einbetten"
            ))
        }
        _ => {
            // line und arrow: sie verbinden zwei Elemente.
            let Some(von) = vorlage.von.as_deref() else {
                return Err(Fehler::Ungueltig(
                    "Ein Pfeil braucht „von“ und „nach“ (Ids aus `excalidraw_lesen`).".to_string(),
                ));
            };
            let Some(nach) = vorlage.nach.as_deref() else {
                return Err(Fehler::Ungueltig(
                    "Ein Pfeil braucht „von“ und „nach“ (Ids aus `excalidraw_lesen`).".to_string(),
                ));
            };
            let kante = vorlage.kante.unwrap_or(if art == "line" {
                KanteArt::Linie
            } else {
                KanteArt::Pfeil
            });
            crate::aendern::verbinden(datei, von, nach, kante, vorlage.id.as_deref())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pruefen;
    use quiver_diagramm::modell::{Form, Gruppe, Kante, Knoten};

    fn beispiel() -> Diagramm {
        let mut diagramm = Diagramm::leer();
        diagramm.knoten = vec![
            Knoten {
                form: Form::Ellipse,
                farbe: Some(Farbe::Blau),
                ..Knoten::neu("start", "Antrag")
            },
            Knoten {
                form: Form::Raute,
                gruppe: Some("vorgang".to_string()),
                ..Knoten::neu("pruefen", "Prüfung nötig?")
            },
            Knoten::neu("ende", "abgelehnt"),
        ];
        diagramm.kanten = vec![
            Kante {
                text: Some("eingereicht".to_string()),
                ..Kante::neu("start", "pruefen")
            },
            Kante {
                art: KanteArt::Gestrichelt,
                ..Kante::neu("pruefen", "ende")
            },
        ];
        diagramm.gruppen = vec![Gruppe::neu("vorgang", "Vorgang")];
        diagramm
    }

    #[test]
    fn aus_einem_modell_wird_eine_gueltige_datei() {
        let datei = aus_diagramm(&beispiel()).unwrap();
        let roh = serde_json::to_value(&datei).unwrap();
        pruefen::pruefen(&roh).expect("die erzeugte Datei ist gültig");
        // Drei Knoten mit Text, ein Rahmen, zwei Kanten mit Beschriftung.
        assert_eq!(datei.elements.len(), 3 + 3 + 1 + 2 + 1);
        assert_eq!(datei.sichtbare().count(), datei.elements.len());
        assert!(datei.element("gruppe-vorgang").unwrap().ist("frame"));
        assert!(datei.element("kante-0").unwrap().ist("arrow"));
        assert!(datei.element("kante-1").unwrap().ist("arrow"));
        assert_eq!(datei.element("kante-1").unwrap().stroke_style, "dashed");
        assert_eq!(
            datei
                .element("kante-0-beschriftung")
                .unwrap()
                .text
                .as_deref(),
            Some("eingereicht")
        );
    }

    #[test]
    fn jeder_knotentext_haengt_an_seiner_form() {
        let datei = aus_diagramm(&beispiel()).unwrap();
        for platz in ["start", "pruefen", "ende"] {
            let form = datei.element(&format!("knoten-{platz}")).unwrap();
            let texte = form.gebundene("text");
            assert_eq!(texte.len(), 1, "Form „{platz}“ hat keinen Text");
            let text = datei.element(&texte[0]).unwrap();
            assert_eq!(text.container(), Some(form.id.as_str()));
            assert_eq!(text.text_align.as_deref(), Some("center"));
            // Der Text liegt mittig in der Form.
            assert!((text.x - (form.x + (form.width - text.width) / 2.0)).abs() < 0.001);
        }
    }

    #[test]
    fn jeder_pfeil_beginnt_bei_null_auch_nach_links_und_nach_dem_verschieben() {
        // Excalidraw verlangt `points[0] == [0, 0]`.
        let diagramm = Diagramm::aus_json(
            r#"{"richtung":"unten","knoten":[{"id":"a","text":"A"},{"id":"b","text":"B"},
                {"id":"c","text":"C"},{"id":"d","text":"D"}],
                "kanten":[{"von":"a","nach":"b"},{"von":"a","nach":"c","text":"ja"},
                {"von":"a","nach":"d"},{"von":"d","nach":"a"}],"gruppen":[]}"#,
        )
        .unwrap();
        let mut datei = aus_diagramm(&diagramm).unwrap();
        let pruefen = |datei: &Datei| {
            for pfeil in datei.elements.iter().filter(|e| e.ist("arrow")) {
                assert_eq!(
                    pfeil.points.as_ref().unwrap()[0],
                    [0.0, 0.0],
                    "{}",
                    pfeil.id
                );
            }
        };
        pruefen(&datei);
        crate::aendern::verschieben(&mut datei, "knoten-a", Some(900.0), Some(500.0), None, None)
            .unwrap();
        pruefen(&datei);
        // Danach laufen die Pfeile gerade vom einen Rand zum anderen.
        let a = datei.element("knoten-a").unwrap().clone();
        for id in a.gebundene("arrow") {
            let pfeil = datei.element(&id).unwrap();
            assert_eq!(pfeil.punkte().len(), 2, "{id}");
            if pfeil.start() == Some("knoten-a") {
                let (x, y) = pfeil.punkte()[0];
                assert!(x >= a.x - 0.01 && x <= a.x + a.width + 0.01, "{id}: {x}");
                assert!(y >= a.y - 0.01 && y <= a.y + a.height + 0.01, "{id}: {y}");
            }
        }
    }

    #[test]
    fn ein_pfeil_haengt_an_beiden_enden() {
        let datei = aus_diagramm(&beispiel()).unwrap();
        let pfeil = datei.element("kante-0").unwrap();
        assert_eq!(pfeil.start(), Some("knoten-start"));
        assert_eq!(pfeil.schluss(), Some("knoten-pruefen"));
        // Beide Enden nennen den Pfeil in ihrer Liste.
        for id in ["knoten-start", "knoten-pruefen"] {
            assert!(
                datei
                    .element(id)
                    .unwrap()
                    .gebundene("arrow")
                    .contains(&"kante-0".to_string()),
                "„{id}“ nennt den Pfeil nicht"
            );
        }
        // Die Stützpunkte beginnen am Rand, nicht im Mittelpunkt.
        let start = pfeil.punkte()[0];
        let form = datei.element("knoten-start").unwrap();
        let (mx, my) = form.mitte();
        assert!(start != (mx, my));
    }

    #[test]
    fn ein_leeres_modell_ergibt_eine_leere_datei() {
        let datei = aus_diagramm(&Diagramm::leer()).unwrap();
        assert!(datei.elements.is_empty());
        let roh = serde_json::to_value(&datei).unwrap();
        pruefen::pruefen(&roh).expect("gültig");
    }

    #[test]
    fn randpunkte_liegen_auf_dem_rand() {
        let mut form = Element::neu("rectangle", 0.0, 0.0, 100.0, 50.0);
        form.id = "r".to_string();
        assert_eq!(randpunkt(&form, (500.0, 25.0)), (100.0, 25.0));
        assert_eq!(randpunkt(&form, (50.0, 500.0)), (50.0, 50.0));

        let mut ellipse = Element::neu("ellipse", 0.0, 0.0, 100.0, 50.0);
        ellipse.id = "e".to_string();
        let (x, y) = randpunkt(&ellipse, (500.0, 25.0));
        assert!((x - 100.0).abs() < 0.001 && (y - 25.0).abs() < 0.001);

        let mut raute = Element::neu("diamond", 0.0, 0.0, 100.0, 50.0);
        raute.id = "d".to_string();
        let (x, _) = randpunkt(&raute, (500.0, 25.0));
        assert!((x - 100.0).abs() < 0.001, "{x}");
    }

    #[test]
    fn eine_vorlage_legt_ein_element_mit_text_an() {
        let mut datei = Datei::neu();
        let meldung = hinzufuegen(
            &mut datei,
            &Vorlage {
                art: "rectangle".to_string(),
                text: Some("Hallo".to_string()),
                farbe: Some(Farbe::Gruen),
                ..Vorlage::default()
            },
        )
        .unwrap();
        assert!(meldung.contains("rectangle-1"), "{meldung}");
        let roh = serde_json::to_value(&datei).unwrap();
        pruefen::pruefen(&roh).expect("gültig");
    }

    #[test]
    fn ein_pfeil_ohne_enden_wird_abgelehnt() {
        let mut datei = Datei::neu();
        let fehler = hinzufuegen(
            &mut datei,
            &Vorlage {
                art: "arrow".to_string(),
                ..Vorlage::default()
            },
        )
        .unwrap_err();
        assert!(fehler.to_string().contains("von"), "{fehler}");
    }

    #[test]
    fn eine_unbekannte_art_wird_abgelehnt() {
        let mut datei = Datei::neu();
        let fehler = hinzufuegen(
            &mut datei,
            &Vorlage {
                art: "freedraw".to_string(),
                ..Vorlage::default()
            },
        )
        .unwrap_err();
        assert!(fehler.to_string().contains("erlaubt sind"), "{fehler}");
    }
}
