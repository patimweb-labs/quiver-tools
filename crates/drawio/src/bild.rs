// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Ein draw.io-Diagramm selbst zeichnen – ohne Browser und ohne Netz.
//!
//! [`zeichnung_aus_modell`] liest die Zellen einer Seite und baut daraus eine
//! [`Szene`] aus `quiver-diagramm`; die Szene wird zu SVG und über `resvg` zu
//! PNG. Gezeichnet werden die Formen aus Aufgabe 18:
//!
//! | Stil | Figur |
//! |---|---|
//! | `rounded=1`, `rectangle` | Rechteck |
//! | `ellipse` | Ellipse |
//! | `rhombus` | Raute |
//! | `shape=cylinder3` | Zylinder |
//! | `shape=note` | Notiz |
//! | `text` | Text ohne Rahmen |
//! | `swimlane`, `container` | Container mit Kopfzeile |
//!
//! **Unbekanntes wird als Rechteck gezeichnet, und die Antwort nennt den Namen**
//! (Aufgabe 18). Was gar nicht gezeichnet werden kann (eine Kante ohne Enden),
//! steht als Hinweis in der Antwort.
//!
//! Die Farbe einer Zelle wird auf die feste Palette abgebildet: der Zeichner
//! kennt keine freien Werte (ADR 0013).

use std::collections::BTreeMap;

use quiver_diagramm::layout::{SCHRIFTGROESSE, ZEILENHOEHE};
use quiver_diagramm::modell::Farbe;
use quiver_diagramm::zeichnen::{schrift, Ausrichtung, Figur, Punkt, Rechteck, Szene, Text};

use crate::fehler::FormatFehler;
use crate::formen::farbe_aus_hex;
use crate::mx::{self, Lage};
use crate::stil::Stil;
use crate::xml::Element;

/// Rand um das Diagramm im Bild.
pub const RAND: f32 = 24.0;

/// So breit darf eine Kantenbeschriftung höchstens umbrechen.
const KANTEN_TEXT_BREITE: f32 = 200.0;

/// So tief darf eine Zelle in Containern stecken.
const MAX_TIEFE: usize = 32;

/// Ein gezeichnetes Diagramm samt Randnotizen für die Antwort.
#[derive(Debug)]
pub struct Bild {
    /// Die fertige Szene; `szene.svg()` und `szene.png()` zeichnen sie.
    pub szene: Szene,
    /// Anzahl gezeichneter Knoten (Container mitgezählt).
    pub knoten: usize,
    /// Davon Container (Schwimmbahnen).
    pub container: usize,
    /// Anzahl gezeichneter Kanten.
    pub kanten: usize,
    /// Formen aus der Datei, die als Rechteck gezeichnet wurden.
    pub fremde_formen: Vec<String>,
    /// Was nicht gezeichnet wurde, mit Grund.
    pub hinweise: Vec<String>,
}

impl Bild {
    /// Das Bild als PNG.
    pub fn png(&self, max_kante: u32) -> Result<Vec<u8>, FormatFehler> {
        self.szene.png(max_kante).map_err(|fehler| {
            FormatFehler::Sonstiges(format!("Das Bild ließ sich nicht erzeugen: {fehler}"))
        })
    }

    /// Kurze Beschreibung für die Antwort.
    pub fn beschreibung(&self) -> String {
        let mut text = format!(
            "Gezeichnet: {} Knoten{}, {} Kanten ({}×{}).",
            self.knoten,
            if self.container > 0 {
                format!(" (davon {} Container)", self.container)
            } else {
                String::new()
            },
            self.kanten,
            self.szene.breite.ceil() as i64,
            self.szene.hoehe.ceil() as i64
        );
        if !self.fremde_formen.is_empty() {
            text.push_str(&format!(
                "\nUnbekannte Formen als Rechteck gezeichnet: {}.",
                self.fremde_formen.join(", ")
            ));
        }
        if !self.hinweise.is_empty() {
            text.push_str(&format!(
                "\nNicht gezeichnet: {}.",
                self.hinweise.join("; ")
            ));
        }
        text
    }
}

/// Welche Figur eine Zelle ergibt.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Art {
    Rechteck,
    Ellipse,
    Raute,
    Zylinder,
    Notiz,
    Text,
    Container,
    Gruppe,
    Fremd(String),
}

/// Eine Zelle als das, was der Zeichner braucht.
#[derive(Debug, Clone)]
struct Kopf {
    id: String,
    eltern: Option<String>,
    knoten: bool,
    kante: bool,
    lage: Lage,
    text: String,
    art: Art,
    farbe: Option<Farbe>,
    schriftgroesse: f32,
    quelle: Option<String>,
    ziel: Option<String>,
    gestrichelt: bool,
    pfeil: bool,
}

/// Liest eine Seite und baut die Szene.
pub fn zeichnung_aus_modell(modell: &Element) -> Result<Bild, FormatFehler> {
    let root = modell.kind("root").ok_or_else(|| {
        FormatFehler::Struktur(
            "Dem Modell fehlt <root> – ohne Zellen gibt es nichts zu zeichnen.".to_string(),
        )
    })?;

    let mut zellen: Vec<&Element> = Vec::new();
    let mut koepfe: Vec<Kopf> = Vec::new();
    for eintrag in mx::eintraege(root) {
        let Some(kopf) = kopf(eintrag) else {
            continue;
        };
        zellen.push(eintrag);
        koepfe.push(kopf);
    }
    let index: BTreeMap<String, usize> = koepfe
        .iter()
        .enumerate()
        .map(|(stelle, kopf)| (kopf.id.clone(), stelle))
        .collect();

    // Grenzen des Inhalts in absoluten Koordinaten.
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut merke = |x: f32, y: f32| {
        if x.is_finite() && y.is_finite() {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    };
    for (stelle, kopf) in koepfe.iter().enumerate() {
        if kopf.knoten {
            let lage = absolut(stelle, &koepfe, &index);
            merke(lage.x, lage.y);
            merke(lage.x + lage.breite, lage.y + lage.hoehe);
        } else if kopf.kante {
            for punkt in wegpunkte(zellen[stelle]) {
                merke(punkt.0, punkt.1);
            }
            for art in ["sourcePoint", "targetPoint"] {
                if let Some((x, y)) = punkt_aus(zellen[stelle], art) {
                    merke(x, y);
                }
            }
        }
    }
    let (breite, hoehe, versatz_x, versatz_y) = if min_x.is_finite() {
        (
            (max_x - min_x) + 2.0 * RAND,
            (max_y - min_y) + 2.0 * RAND,
            RAND - min_x,
            RAND - min_y,
        )
    } else {
        (400.0, 300.0, 0.0, 0.0)
    };
    let mut szene = Szene::leer(breite.max(2.0), hoehe.max(2.0));
    let mut fremde_formen = Vec::new();
    let mut hinweise = Vec::new();
    let mut knoten = 0;
    let mut container = 0;
    let mut kanten = 0;

    // 1. Container (Schwimmbahnen) zuerst: sie liegen hinter ihren Kindern.
    for (stelle, kopf) in koepfe.iter().enumerate() {
        if !kopf.knoten || kopf.art != Art::Container {
            continue;
        }
        let lage = verschoben(absolut(stelle, &koepfe, &index), versatz_x, versatz_y);
        szene.formen.push(Figur::Container {
            mass: rechteck(&lage),
            titel: Some(kopf.text.clone()).filter(|text| !text.trim().is_empty()),
        });
        knoten += 1;
        container += 1;
    }

    // 2. Formen und ihre Beschriftungen.
    for (stelle, kopf) in koepfe.iter().enumerate() {
        if !kopf.knoten || kopf.art == Art::Container {
            continue;
        }
        let lage = verschoben(absolut(stelle, &koepfe, &index), versatz_x, versatz_y);
        let mass = rechteck(&lage);
        match &kopf.art {
            Art::Gruppe => {}
            Art::Text => {}
            Art::Fremd(shape) => {
                szene.formen.push(Figur::Rechteck {
                    mass,
                    radius: 0.0,
                    farbe: kopf.farbe,
                });
                fremde_formen.push(format!("{shape} (Zelle „{}“)", kopf.id));
            }
            Art::Rechteck => szene.formen.push(Figur::Rechteck {
                mass,
                radius: 8.0,
                farbe: kopf.farbe,
            }),
            Art::Ellipse => szene.formen.push(Figur::Ellipse {
                mass,
                farbe: kopf.farbe,
            }),
            Art::Raute => szene.formen.push(Figur::Raute {
                mass,
                farbe: kopf.farbe,
            }),
            Art::Zylinder => szene.formen.push(Figur::Zylinder {
                mass,
                farbe: kopf.farbe,
            }),
            Art::Notiz => szene.formen.push(Figur::Notiz {
                mass,
                farbe: kopf.farbe,
            }),
            Art::Container => {}
        }
        let zeilen = text_zeilen(&kopf.text, kopf.schriftgroesse, lage.breite - 16.0);
        if !zeilen.is_empty() {
            szene.formen.push(Figur::Text {
                mass,
                text: Text {
                    zeilen,
                    groesse: kopf.schriftgroesse,
                    ausrichtung: Ausrichtung::Mitte,
                    farbe: None,
                },
            });
        }
        if kopf.art != Art::Gruppe {
            knoten += 1;
        }
    }

    // 3. Kanten und ihre Beschriftungen.
    for (stelle, kopf) in koepfe.iter().enumerate() {
        if !kopf.kante {
            continue;
        }
        let eintrag = zellen[stelle];
        let weg: Vec<Punkt> = wegpunkte(eintrag)
            .into_iter()
            .map(|(x, y)| Punkt::neu(x + versatz_x, y + versatz_y))
            .collect();
        let quell_lage = kopf
            .quelle
            .as_ref()
            .and_then(|id| index.get(id))
            .map(|&stelle| verschoben(absolut(stelle, &koepfe, &index), versatz_x, versatz_y));
        let ziel_lage = kopf
            .ziel
            .as_ref()
            .and_then(|id| index.get(id))
            .map(|&stelle| verschoben(absolut(stelle, &koepfe, &index), versatz_x, versatz_y));

        let ziel_punkt = punkt_aus(eintrag, "targetPoint")
            .map(|(x, y)| Punkt::neu(x + versatz_x, y + versatz_y));
        let start = punkt_aus(eintrag, "sourcePoint")
            .map(|(x, y)| Punkt::neu(x + versatz_x, y + versatz_y))
            .or_else(|| {
                let lage = quell_lage?;
                let hin = weg
                    .first()
                    .copied()
                    .or_else(|| {
                        ziel_lage.map(|lage| {
                            let (x, y) = lage.mitte();
                            Punkt::neu(x, y)
                        })
                    })
                    .or(ziel_punkt)
                    .unwrap_or_else(|| {
                        let (x, y) = lage.mitte();
                        Punkt::neu(x, y)
                    });
                Some(randpunkt(&lage, hin))
            });
        let ende = ziel_punkt.or_else(|| {
            let lage = ziel_lage?;
            let zurueck = weg.last().copied().or(start).unwrap_or_else(|| {
                let (x, y) = lage.mitte();
                Punkt::neu(x, y)
            });
            Some(randpunkt(&lage, zurueck))
        });
        let (Some(start), Some(ende)) = (start, ende) else {
            hinweise.push(format!(
                "Kante „{}“ hat keinen Anfang oder kein Ende",
                kopf.id
            ));
            continue;
        };
        let mut punkte = Vec::with_capacity(weg.len() + 2);
        punkte.push(start);
        punkte.extend(weg);
        punkte.push(ende);
        szene.formen.push(Figur::Linie {
            punkte: punkte.clone(),
            gestrichelt: kopf.gestrichelt,
            pfeil: kopf.pfeil,
        });
        kanten += 1;

        let beschriftung = kopf.text.trim();
        if !beschriftung.is_empty() {
            let groesse = kopf.schriftgroesse.min(SCHRIFTGROESSE);
            let zeilen = text_zeilen(beschriftung, groesse, KANTEN_TEXT_BREITE);
            let text_breite = zeilen
                .iter()
                .map(|zeile| schrift::breite(zeile, groesse))
                .fold(0.0_f32, f32::max)
                .max(20.0);
            let text_hoehe = (zeilen.len() as f32 * ZEILENHOEHE).max(ZEILENHOEHE);
            let (x, y) = mitte(&punkte);
            szene.formen.push(Figur::Text {
                mass: Rechteck::neu(
                    x - text_breite / 2.0,
                    y - text_hoehe / 2.0,
                    text_breite,
                    text_hoehe,
                ),
                text: Text {
                    zeilen,
                    groesse,
                    ausrichtung: Ausrichtung::Mitte,
                    farbe: None,
                },
            });
        }
    }

    Ok(Bild {
        szene,
        knoten,
        container,
        kanten,
        fremde_formen,
        hinweise,
    })
}

fn kopf(eintrag: &Element) -> Option<Kopf> {
    let id = mx::id(eintrag)?.to_string();
    if matches!(id.as_str(), mx::ROOT_ID | mx::EBENE_ID) {
        return None;
    }
    let kante = mx::ist_kante(eintrag);
    let knoten = mx::ist_knoten(eintrag);
    if !kante && !knoten {
        return None;
    }
    let stil = mx::stil(eintrag);
    let schriftgroesse = stil
        .wert("fontSize")
        .and_then(|wert| wert.trim().parse::<f32>().ok())
        .filter(|groesse| groesse.is_finite() && *groesse >= 4.0 && *groesse <= 96.0)
        .unwrap_or(SCHRIFTGROESSE);
    Some(Kopf {
        id,
        eltern: mx::eltern(eintrag).map(str::to_string),
        knoten,
        kante,
        lage: mx::lage(eintrag),
        text: html_zu_text(&mx::wert(eintrag)),
        art: art(&stil),
        farbe: stil.wert("fillColor").and_then(farbe_aus_hex),
        schriftgroesse,
        quelle: mx::quelle(eintrag),
        ziel: mx::ziel(eintrag),
        gestrichelt: stil.wert("dashed") == Some("1"),
        pfeil: stil.wert("endArrow") != Some("none"),
    })
}

fn art(stil: &Stil) -> Art {
    if let Some(shape) = stil.wert("shape") {
        return match shape.trim() {
            "cylinder3" | "cylinder" | "datastore" => Art::Zylinder,
            "note" => Art::Notiz,
            "text" => Art::Text,
            "swimlane" => Art::Container,
            "rect" | "rectangle" => Art::Rechteck,
            sonst => Art::Fremd(sonst.to_string()),
        };
    }
    match stil.form() {
        Some("rhombus") => Art::Raute,
        Some("ellipse") | Some("doubleEllipse") => Art::Ellipse,
        Some("text") => Art::Text,
        Some("swimlane") | Some("container") => Art::Container,
        Some("group") => Art::Gruppe,
        _ => Art::Rechteck,
    }
}

fn rechteck(lage: &Lage) -> Rechteck {
    Rechteck::neu(lage.x, lage.y, lage.breite, lage.hoehe)
}

fn verschoben(lage: Lage, dx: f32, dy: f32) -> Lage {
    Lage {
        x: lage.x + dx,
        y: lage.y + dy,
        ..lage
    }
}

/// Die absolute Lage einer Zelle (Elternketten aufaddiert).
fn absolut(stelle: usize, koepfe: &[Kopf], index: &BTreeMap<String, usize>) -> Lage {
    let mut lage = koepfe[stelle].lage;
    let mut eltern = koepfe[stelle].eltern.clone();
    for _ in 0..MAX_TIEFE {
        let Some(kennung) = eltern else {
            break;
        };
        let Some(&eltern_stelle) = index.get(&kennung) else {
            break;
        };
        lage.x += koepfe[eltern_stelle].lage.x;
        lage.y += koepfe[eltern_stelle].lage.y;
        eltern = koepfe[eltern_stelle].eltern.clone();
    }
    lage
}

/// Stützpunkte einer Kante (`Array as="points"`), in Dateikoordinaten.
fn wegpunkte(eintrag: &Element) -> Vec<(f32, f32)> {
    let Some(geometrie) = mx::geometrie(eintrag) else {
        return Vec::new();
    };
    let Some(array) = geometrie
        .kinder_elemente()
        .find(|kind| kind.name == "Array" && kind.attribut("as") == Some("points"))
    else {
        return Vec::new();
    };
    array
        .kinder_elemente()
        .filter(|punkt| punkt.name == "mxPoint")
        .filter_map(|punkt| Some((zahl(punkt, "x")?, zahl(punkt, "y")?)))
        .collect()
}

/// Ein fester Punkt der Kante (`sourcePoint`/`targetPoint`).
fn punkt_aus(eintrag: &Element, art: &str) -> Option<(f32, f32)> {
    let geometrie = mx::geometrie(eintrag)?;
    let punkt = geometrie
        .kinder_elemente()
        .find(|kind| kind.name == "mxPoint" && kind.attribut("as") == Some(art))?;
    Some((zahl(punkt, "x")?, zahl(punkt, "y")?))
}

fn zahl(element: &Element, name: &str) -> Option<f32> {
    element
        .attribut(name)?
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|wert| wert.is_finite())
}

/// Wo eine Linie den Rand einer Form trifft.
fn randpunkt(lage: &Lage, nach: Punkt) -> Punkt {
    let (cx, cy) = lage.mitte();
    let dx = nach.x - cx;
    let dy = nach.y - cy;
    if dx.abs() < 0.001 && dy.abs() < 0.001 {
        return Punkt::neu(cx, cy);
    }
    let tx = if dx.abs() < 0.001 {
        f32::INFINITY
    } else {
        (lage.breite / 2.0) / dx.abs()
    };
    let ty = if dy.abs() < 0.001 {
        f32::INFINITY
    } else {
        (lage.hoehe / 2.0) / dy.abs()
    };
    let t = tx.min(ty);
    Punkt::neu(cx + dx * t, cy + dy * t)
}

fn mitte(punkte: &[Punkt]) -> (f32, f32) {
    if punkte.is_empty() {
        return (0.0, 0.0);
    }
    let stelle = punkte.len() / 2;
    let a = punkte[stelle.saturating_sub(1)];
    let b = punkte[stelle];
    ((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)
}

fn text_zeilen(text: &str, groesse: f32, breite: f32) -> Vec<String> {
    let breite = breite.max(groesse * 2.0);
    let mut zeilen = Vec::new();
    for absatz in text.split('\n') {
        if absatz.trim().is_empty() {
            zeilen.push(String::new());
            continue;
        }
        zeilen.extend(schrift::umbrechen(absatz, groesse, breite));
    }
    while zeilen.last().is_some_and(|zeile| zeile.trim().is_empty()) {
        zeilen.pop();
    }
    zeilen
}

/// Macht aus einem draw.io-Label den Text für das Bild.
///
/// `html=1` heißt: das Label kann `<b>`, `<br>`, `<div>` enthalten. Im Bild
/// steht nur der Text; Tags fallen weg, Zeilenumbrüche bleiben.
pub fn html_zu_text(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        text.push_str(&rest[..start]);
        let Some(ende) = rest[start..].find('>') else {
            // Kein schließendes „>“: der Rest ist Text.
            rest = &rest[start..];
            break;
        };
        let tag = &rest[start + 1..start + ende];
        let schliessend = tag.trim_start().starts_with('/');
        let name = tag
            .trim_start_matches('/')
            .split(|zeichen: char| zeichen.is_whitespace() || zeichen == '/' || zeichen == '>')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        // `<br>` bricht immer um; `<div>`, `<p>`, `<li>`, `<tr>` nur am Ende,
        // damit „<div>a</div><div>b</div>“ zwei Zeilen ergibt und nicht vier.
        if name == "br" || (schliessend && matches!(name.as_str(), "div" | "p" | "li" | "tr")) {
            text.push('\n');
        }
        rest = &rest[start + ende + 1..];
    }
    text.push_str(rest);
    entschaerfen(&text)
}

fn entschaerfen(text: &str) -> String {
    let ersetzt = text
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&amp;", "&");
    ersetzt.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mx::{leeres_modell, ROOT_ID};

    fn seite(inhalt: &str) -> Element {
        let xml = format!(
            "<mxGraphModel><root><mxCell id=\"{ROOT_ID}\"/>\
             <mxCell id=\"1\" parent=\"{ROOT_ID}\"/>{inhalt}</root></mxGraphModel>"
        );
        crate::xml::lesen(&xml).unwrap()
    }

    #[test]
    fn die_bekannten_formen_werden_gezeichnet() {
        let inhalt = [
            ("r", "rounded=1;html=1;", 0.0),
            ("e", "ellipse;html=1;", 20.0),
            ("d", "rhombus;html=1;", 40.0),
            ("z", "shape=cylinder3;html=1;", 60.0),
            ("n", "shape=note;html=1;", 80.0),
            ("t", "text;html=1;", 100.0),
        ]
        .iter()
        .map(|(id, stil, y)| {
            format!(
                r#"<mxCell id="{id}" value="{id}" style="{stil}" vertex="1" parent="1"><mxGeometry x="10" y="{y}" width="120" height="60" as="geometry"/></mxCell>"#
            )
        })
        .collect::<String>();
        let bild = zeichnung_aus_modell(&seite(&inhalt)).unwrap();
        assert_eq!(bild.knoten, 6);
        assert!(bild.fremde_formen.is_empty());
        assert!(bild.szene.svg().contains("<ellipse"));
        assert!(bild.szene.svg().contains("<polygon"));
    }

    #[test]
    fn eine_fremde_form_wird_zum_rechteck_und_genannt() {
        let inhalt = r#"<mxCell id="a" value="Lambda" style="shape=mxgraph.aws4.lambda;html=1;" vertex="1" parent="1"><mxGeometry x="5" y="5" width="80" height="80" as="geometry"/></mxCell>"#;
        let bild = zeichnung_aus_modell(&seite(inhalt)).unwrap();
        assert_eq!(bild.knoten, 1);
        assert_eq!(bild.fremde_formen.len(), 1);
        assert!(bild.fremde_formen[0].contains("mxgraph.aws4.lambda"));
        let text = bild.beschreibung();
        assert!(text.contains("Unbekannte Formen als Rechteck"), "{text}");
    }

    #[test]
    fn eine_kante_zwischen_zwei_knoten_hat_eine_linie() {
        let inhalt = [
            r#"<mxCell id="a" value="A" style="rounded=1;" vertex="1" parent="1"><mxGeometry x="10" y="10" width="100" height="40" as="geometry"/></mxCell>"#,
            r#"<mxCell id="b" value="B" style="rounded=1;" vertex="1" parent="1"><mxGeometry x="10" y="150" width="100" height="40" as="geometry"/></mxCell>"#,
            r#"<mxCell id="k" value="ja" style="endArrow=classic;" edge="1" source="a" target="b" parent="1"><mxGeometry relative="1" as="geometry"/></mxCell>"#,
        ]
        .concat();
        let bild = zeichnung_aus_modell(&seite(&inhalt)).unwrap();
        assert_eq!(bild.kanten, 1);
        assert!(bild.hinweise.is_empty());
        let svg = bild.szene.svg();
        assert!(svg.contains("<polyline"), "{svg}");
        assert!(svg.contains("<polygon"), "Pfeilspitze fehlt: {svg}");
    }

    #[test]
    fn eine_kante_ohne_enden_wird_benannt() {
        let inhalt = r#"<mxCell id="k" value="frei" edge="1" parent="1"><mxGeometry relative="1" as="geometry"/></mxCell>"#;
        let bild = zeichnung_aus_modell(&seite(inhalt)).unwrap();
        assert_eq!(bild.kanten, 0);
        assert!(
            bild.hinweise[0].contains("Kante „k“"),
            "{:?}",
            bild.hinweise
        );
    }

    #[test]
    fn ein_container_zeichnet_seine_kinder_absolut() {
        let inhalt = [
            r#"<mxCell id="c" value="Lane" style="swimlane;html=1;" vertex="1" parent="1"><mxGeometry x="10" y="10" width="300" height="200" as="geometry"/></mxCell>"#,
            r#"<mxCell id="a" value="Kind" style="rounded=1;" vertex="1" parent="c"><mxGeometry x="20" y="40" width="100" height="40" as="geometry"/></mxCell>"#,
        ]
        .concat();
        let bild = zeichnung_aus_modell(&seite(&inhalt)).unwrap();
        assert_eq!(bild.knoten, 2);
        // Der Container beginnt bei RAND, das Kind bei RAND + 20.
        let svg = bild.szene.svg();
        assert!(svg.contains("stroke-dasharray"), "kein Container: {svg}");
    }

    #[test]
    fn html_labels_werden_zu_text() {
        assert_eq!(html_zu_text("eins<br>zwei"), "eins\nzwei");
        assert_eq!(
            html_zu_text("<b>fett</b> &amp; <i>kursiv</i>"),
            "fett & kursiv"
        );
        assert_eq!(html_zu_text("a&nbsp;b"), "a b");
        assert_eq!(html_zu_text("&lt;script&gt;"), "<script>");
        assert_eq!(html_zu_text("<div>eins</div><div>zwei</div>"), "eins\nzwei");
    }

    #[test]
    fn farben_werden_auf_die_palette_abgebildet() {
        let inhalt = r#"<mxCell id="a" value="A" style="rounded=1;fillColor=#e2efff;" vertex="1" parent="1"><mxGeometry x="0" y="0" width="80" height="40" as="geometry"/></mxCell>"#;
        let bild = zeichnung_aus_modell(&seite(inhalt)).unwrap();
        let svg = bild.szene.svg();
        assert!(
            svg.contains(quiver_diagramm::zeichnen::palette::toene(Some(Farbe::Blau)).fuellung),
            "{svg}"
        );
    }

    #[test]
    fn eine_leere_seite_ergibt_ein_kleines_bild() {
        let bild = zeichnung_aus_modell(&leeres_modell()).unwrap();
        assert_eq!(bild.knoten, 0);
        assert_eq!(bild.kanten, 0);
        assert!(bild.szene.breite >= 2.0);
        assert!(!bild.png(200).unwrap().is_empty());
    }
}
