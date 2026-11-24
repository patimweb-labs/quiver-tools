// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Aus einer `.excalidraw`-Datei ein Bild – SVG oder PNG.
//!
//! Gezeichnet wird **selbst** über [`quiver_diagramm::zeichnen`], mit derselben
//! eingebetteten Schrift wie das Layout: kein Browser, kein Excalidraw im
//! Hintergrund, kein Netz.
//!
//! Gezeichnet werden `rectangle`, `diamond`, `ellipse`, `line`, `arrow`, `text`,
//! `freedraw` (als Linienzug) und `frame`; `image` wird zum Platzhalter. Alles
//! andere (etwa `embeddable`, `iframe`, `stickynote`) nennt die Antwort beim
//! Namen, statt es stillschweigend wegzulassen.

use quiver_diagramm::zeichnen::{Ausrichtung, Figur, Punkt, Rechteck, Szene, Text};

use crate::datei::{Datei, Element};
use crate::farben;
use crate::fehler::Fehler;

/// Rand um das ganze Bild.
pub const RAND: f32 = 32.0;
/// Kantenlänge eines PNG ohne Angabe (höchstens 2000, siehe Bibliothek).
pub const STANDARD_KANTE: u32 = 1600;

/// Das fertige Bild samt dem, was nicht gezeichnet wurde.
#[derive(Debug, Clone)]
pub struct Bild {
    /// Die Zeichenfläche mit allen Figuren.
    pub szene: Szene,
    /// Elemente, die diese Kiste nicht zeichnen kann (mit Art und Id).
    pub nicht_gezeichnet: Vec<String>,
    /// Wie viele Elemente gezeichnet wurden.
    pub gezeichnet: usize,
}

impl Bild {
    /// Die Pixelmaße, die [`Bild::png`] erzeugen wird.
    ///
    /// Die Rechnung ist dieselbe wie in [`Szene::png`], weil die Bibliothek die
    /// Maße nicht zurückgibt (steht in der README unter „Offen“).
    pub fn pixel(&self, max_kante: u32) -> (u32, u32) {
        let max_kante = max_kante.clamp(16, quiver_diagramm::zeichnen::MAX_PNG_KANTE);
        let breite = self.szene.breite.max(1.0);
        let hoehe = self.szene.hoehe.max(1.0);
        let faktor = (max_kante as f32 / breite.max(hoehe)).min(1.0);
        (
            (breite * faktor).round().max(1.0) as u32,
            (hoehe * faktor).round().max(1.0) as u32,
        )
    }

    /// Das Bild als eigenständiges SVG.
    pub fn svg(&self) -> String {
        self.szene.svg()
    }

    /// Das Bild als PNG, höchstens `max_kante` Pixel je Kante.
    pub fn png(&self, max_kante: u32) -> Result<Vec<u8>, Fehler> {
        Ok(self.szene.png(max_kante)?)
    }
}

/// Baut die Zeichenfläche aus einer Datei.
pub fn bild(datei: &Datei) -> Bild {
    let sichtbar: Vec<&Element> = datei.sichtbare().collect();
    let mut nicht_gezeichnet = Vec::new();
    let mut flaechen: Vec<(Rechteck, Figur)> = Vec::new();
    let mut gezeichnet = 0;

    // Rahmen liegen hinten, alles andere darauf – in der Reihenfolge der Datei.
    let rahmen_zuerst = sichtbar
        .iter()
        .filter(|element| element.ist("frame"))
        .chain(sichtbar.iter().filter(|element| !element.ist("frame")));

    for element in rahmen_zuerst {
        match figur(element) {
            Some(figur) => {
                flaechen.push((flaeche(element), figur));
                gezeichnet += 1;
            }
            None => nicht_gezeichnet.push(format!("{} „{}“", element.art, element.id)),
        }
    }

    if flaechen.is_empty() {
        return Bild {
            szene: Szene::leer(2.0 * RAND, 2.0 * RAND),
            nicht_gezeichnet,
            gezeichnet,
        };
    }

    // Alles in den sichtbaren Bereich schieben.
    let mut gesamt = flaechen[0].0;
    for (flaeche, _) in &flaechen[1..] {
        gesamt = gesamt.vereinigt(flaeche);
    }
    let versatz = (RAND - gesamt.x, RAND - gesamt.y);

    let mut szene = Szene::leer(
        (gesamt.breite + 2.0 * RAND).max(1.0),
        (gesamt.hoehe + 2.0 * RAND).max(1.0),
    );
    for (_, figur) in flaechen {
        szene.formen.push(schieben(&figur, versatz));
    }
    Bild {
        szene,
        nicht_gezeichnet,
        gezeichnet,
    }
}

/// Die Fläche, die ein Element im Bild einnimmt.
fn flaeche(element: &Element) -> Rechteck {
    if element.ist_linie() {
        let punkte = element.punkte();
        if punkte.is_empty() {
            return Rechteck::neu(element.x as f32, element.y as f32, 1.0, 1.0);
        }
        let min_x = punkte.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
        let min_y = punkte.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
        let max_x = punkte.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
        let max_y = punkte.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
        return Rechteck::neu(
            min_x as f32,
            min_y as f32,
            (max_x - min_x).max(1.0) as f32,
            (max_y - min_y).max(1.0) as f32,
        );
    }
    Rechteck::neu(
        element.x as f32,
        element.y as f32,
        (element.width.max(1.0)) as f32,
        (element.height.max(1.0)) as f32,
    )
}

/// Die Figur zu einem Element – oder `None`, wenn die Kiste sie nicht zeichnet.
fn figur(element: &Element) -> Option<Figur> {
    let mass = flaeche(element);
    let fuellung = farben::farbe_aus_hex(&element.background_color);
    match element.art.as_str() {
        "rectangle" => Some(Figur::Rechteck {
            mass,
            radius: 8.0,
            farbe: fuellung,
        }),
        "diamond" => Some(Figur::Raute {
            mass,
            farbe: fuellung,
        }),
        "ellipse" => Some(Figur::Ellipse {
            mass,
            farbe: fuellung,
        }),
        "line" | "arrow" | "freedraw" => {
            let punkte: Vec<Punkt> = element
                .punkte()
                .into_iter()
                .map(|(x, y)| Punkt::neu(x as f32, y as f32))
                .collect();
            Some(Figur::Linie {
                punkte,
                gestrichelt: element.stroke_style != "solid",
                pfeil: element
                    .end_arrowhead
                    .as_ref()
                    .is_some_and(|spitze| !spitze.is_null()),
            })
        }
        "text" => {
            let inhalt = element.text.as_deref().unwrap_or("");
            if inhalt.trim().is_empty() {
                return None;
            }
            let zeilen: Vec<String> = inhalt.split('\n').map(str::to_string).collect();
            let ausrichtung = match element.text_align.as_deref() {
                Some("center") => Ausrichtung::Mitte,
                Some("right") => Ausrichtung::Rechts,
                _ => Ausrichtung::Links,
            };
            Some(Figur::Text {
                mass,
                text: Text {
                    zeilen,
                    groesse: element.font_size.unwrap_or(crate::bauen::TEXTGROESSE) as f32,
                    ausrichtung,
                    farbe: farben::farbe_aus_hex(&element.stroke_color),
                },
            })
        }
        "frame" => Some(Figur::Container {
            mass,
            titel: element.name.clone().filter(|name| !name.trim().is_empty()),
        }),
        "image" => Some(Figur::Platzhalter {
            mass,
            beschriftung: Some("Bild".to_string()),
        }),
        _ => None,
    }
}

/// Verschiebt eine Figur um `versatz`.
fn schieben(figur: &Figur, versatz: (f32, f32)) -> Figur {
    let (dx, dy) = versatz;
    let mass = |mass: &Rechteck| Rechteck::neu(mass.x + dx, mass.y + dy, mass.breite, mass.hoehe);
    match figur {
        Figur::Rechteck {
            mass: m,
            radius,
            farbe,
        } => Figur::Rechteck {
            mass: mass(m),
            radius: *radius,
            farbe: *farbe,
        },
        Figur::Ellipse { mass: m, farbe } => Figur::Ellipse {
            mass: mass(m),
            farbe: *farbe,
        },
        Figur::Raute { mass: m, farbe } => Figur::Raute {
            mass: mass(m),
            farbe: *farbe,
        },
        Figur::Zylinder { mass: m, farbe } => Figur::Zylinder {
            mass: mass(m),
            farbe: *farbe,
        },
        Figur::Notiz { mass: m, farbe } => Figur::Notiz {
            mass: mass(m),
            farbe: *farbe,
        },
        Figur::Container { mass: m, titel } => Figur::Container {
            mass: mass(m),
            titel: titel.clone(),
        },
        Figur::Text { mass: m, text } => Figur::Text {
            mass: mass(m),
            text: text.clone(),
        },
        Figur::Linie {
            punkte,
            gestrichelt,
            pfeil,
        } => Figur::Linie {
            punkte: punkte
                .iter()
                .map(|punkt| Punkt::neu(punkt.x + dx, punkt.y + dy))
                .collect(),
            gestrichelt: *gestrichelt,
            pfeil: *pfeil,
        },
        Figur::Platzhalter {
            mass: m,
            beschriftung,
        } => Figur::Platzhalter {
            mass: mass(m),
            beschriftung: beschriftung.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bauen::{hinzufuegen, Vorlage};
    use quiver_diagramm::modell::{Diagramm, Farbe, Form, Kante, Knoten};

    fn datei_mit_formen() -> Datei {
        let mut diagramm = Diagramm::leer();
        diagramm.knoten = vec![
            Knoten {
                form: Form::Ellipse,
                farbe: Some(Farbe::Blau),
                ..Knoten::neu("a", "Antrag <b>")
            },
            Knoten::neu("b", "Ende"),
        ];
        diagramm.kanten = vec![Kante::neu("a", "b")];
        crate::bauen::aus_diagramm(&diagramm).unwrap()
    }

    #[test]
    fn aus_einer_datei_wird_ein_png() {
        let bild = bild(&datei_mit_formen());
        assert_eq!(bild.gezeichnet, 5, "zwei Formen, zwei Texte, ein Pfeil");
        assert!(
            bild.nicht_gezeichnet.is_empty(),
            "{:?}",
            bild.nicht_gezeichnet
        );
        let png = bild.png(800).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert!(bild.szene.breite > 2.0 * RAND);
    }

    #[test]
    fn das_svg_hat_keine_verweise_und_maskiert_texte() {
        let svg = bild(&datei_mit_formen()).svg();
        assert!(svg.starts_with("<svg"));
        assert!(svg.trim_end().ends_with("</svg>"));
        assert!(!svg.contains("href"), "{svg}");
        assert!(!svg.contains("<script"), "{svg}");
        assert!(svg.contains("Antrag &lt;b&gt;"), "Text nicht maskiert");
    }

    #[test]
    fn ein_bild_wird_als_platzhalter_gezeichnet() {
        let mut datei = Datei::neu();
        hinzufuegen(
            &mut datei,
            &Vorlage {
                art: "image".to_string(),
                ..Vorlage::default()
            },
        )
        .unwrap();
        let bild = bild(&datei);
        assert_eq!(bild.gezeichnet, 1);
        assert!(bild.svg().contains("Bild"));
    }

    #[test]
    fn unbekanntes_wird_genannt() {
        let mut datei = Datei::neu();
        let mut fremd = Element::neu("embeddable", 0.0, 0.0, 100.0, 100.0);
        fremd.id = "fremd".to_string();
        datei.hinzufuegen(fremd).unwrap();
        let bild = bild(&datei);
        assert_eq!(bild.gezeichnet, 0);
        assert_eq!(bild.nicht_gezeichnet, vec!["embeddable „fremd“"]);
        assert!(bild.szene.formen.is_empty());
    }

    #[test]
    fn eine_leere_datei_ergibt_ein_kleines_bild() {
        let bild = bild(&Datei::neu());
        assert_eq!(bild.szene.breite, 2.0 * RAND);
        assert!(bild.png(100).is_ok());
    }

    #[test]
    fn geloeschte_elemente_werden_nicht_gezeichnet() {
        let mut datei = datei_mit_formen();
        let id = datei.elements[0].id.clone();
        datei.element_mut(&id).unwrap().is_deleted = true;
        let anzahl = bild(&datei).gezeichnet;
        assert!(anzahl < 5);
    }

    #[test]
    fn texte_liegen_ueber_ihren_formen() {
        let bild = bild(&datei_mit_formen());
        let formen = &bild.szene.formen;
        let ellipse = formen
            .iter()
            .position(|figur| matches!(figur, Figur::Ellipse { .. }))
            .expect("keine Ellipse");
        let text = formen
            .iter()
            .position(|figur| match figur {
                Figur::Text { text, .. } => {
                    text.zeilen.iter().any(|zeile| zeile.contains("Antrag"))
                }
                _ => false,
            })
            .expect("kein Text „Antrag“");
        assert!(
            text > ellipse,
            "der Text liegt unter der Form und ist unsichtbar"
        );
    }

    #[test]
    fn ein_freier_text_wird_gezeichnet() {
        let mut datei = Datei::neu();
        hinzufuegen(
            &mut datei,
            &Vorlage {
                art: "text".to_string(),
                text: Some("Hallo".to_string()),
                ..Vorlage::default()
            },
        )
        .unwrap();
        assert!(bild(&datei).svg().contains("Hallo"));
    }
}
