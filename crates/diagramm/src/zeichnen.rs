// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Eigenes Formenmodell → SVG → PNG.
//!
//! Die Kiste zeichnet **selbst**; sie ruft weder Excalidraw noch draw.io noch
//! einen Browser. [`Szene`] ist eine flache Liste von Figuren in
//! Bildkoordinaten (Ursprung links oben, y nach unten). Aus ihr entsteht
//!
//! - **SVG** ([`Szene::svg`]): alles maskiert, kein Skript, keine externen
//!   Verweise, keine `<image>`-Elemente – ein Text kann also kein Markup
//!   einschleusen.
//! - **PNG** ([`Szene::png`]): über `resvg`, mit der eingebetteten Schrift
//!   „IBM Plex Sans“ (SIL Open Font License, Lizenz in `schriften/OFL.txt`).
//!   Höchstens 2000 px Kantenlänge.
//!
//! Welche Figur welche Knotenform bekommt, entscheidet
//! [`Szene::aus_zeichnung`]; die Werkzeuge (Excalidraw, draw.io) bauen ihre
//! eigene Szene aus ihrem Format und benutzen dieselben Figuren.

use crate::layout::Zeichnung;
use crate::modell::{Farbe, Form};
use std::fmt::Write as _;

/// Größte Kantenlänge eines PNG in Pixeln (Aufgabe 16).
pub const MAX_PNG_KANTE: u32 = 2000;

/// Die eingebettete Schrift: Kennung für SVG und `fontdb`.
pub const SCHRIFT_FAMILIE: &str = "IBM Plex Sans";

/// Die eingebettete Schrift als Bytes (SIL Open Font License 1.1).
pub const SCHRIFT_DATEN: &[u8] = include_bytes!("../schriften/PlexSans-Regular.ttf");

/// Zeichenfläche in Bildkoordinaten.
#[derive(Debug, Clone, PartialEq)]
pub struct Szene {
    /// Breite in Nutzerkoordinaten (vor der PNG-Skalierung).
    pub breite: f32,
    /// Höhe in Nutzerkoordinaten (vor der PNG-Skalierung).
    pub hoehe: f32,
    /// Die Figuren in Zeichenreihenfolge (hinten zuerst).
    pub formen: Vec<Figur>,
}

/// Ein Punkt in Bildkoordinaten (Ursprung links oben, y nach unten).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Punkt {
    /// Abstand von links.
    pub x: f32,
    /// Abstand von oben.
    pub y: f32,
}

impl Punkt {
    /// Punkt aus x und y.
    pub fn neu(x: f32, y: f32) -> Punkt {
        Punkt { x, y }
    }
}

/// Ein Rechteck in Bildkoordinaten.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rechteck {
    /// Linke Kante.
    pub x: f32,
    /// Oberkante.
    pub y: f32,
    /// Breite (positiv).
    pub breite: f32,
    /// Höhe (positiv).
    pub hoehe: f32,
}

impl Rechteck {
    /// Rechteck aus linker oberer Ecke und Größe.
    pub fn neu(x: f32, y: f32, breite: f32, hoehe: f32) -> Rechteck {
        Rechteck {
            x,
            y,
            breite,
            hoehe,
        }
    }

    /// Rechteck um `rand` nach allen Seiten erweitern.
    pub fn erweitert(&self, rand: f32) -> Rechteck {
        Rechteck {
            x: self.x - rand,
            y: self.y - rand,
            breite: self.breite + 2.0 * rand,
            hoehe: self.hoehe + 2.0 * rand,
        }
    }

    /// Vereinigung zweier Rechtecke.
    pub fn vereinigt(&self, andere: &Rechteck) -> Rechteck {
        let x = self.x.min(andere.x);
        let y = self.y.min(andere.y);
        let rechts = (self.x + self.breite).max(andere.x + andere.breite);
        let unten = (self.y + self.hoehe).max(andere.y + andere.hoehe);
        Rechteck {
            x,
            y,
            breite: rechts - x,
            hoehe: unten - y,
        }
    }
}

/// Ausrichtung eines Textes innerhalb seiner Box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ausrichtung {
    /// Am linken Rand der Box.
    Links,
    /// Mittig in der Box.
    Mitte,
    /// Am rechten Rand der Box.
    Rechts,
}

/// Eine Textzeile oder ein Textblock.
///
/// `position` ist die linke obere Ecke der Textbox, `breite` ihre Breite (für
/// die Ausrichtung); die erste Zeile steht auf `position.y + groesse`.
#[derive(Debug, Clone, PartialEq)]
pub struct Text {
    /// Die Zeilen; jede wird als eigenes `<text>`-Element gezeichnet.
    pub zeilen: Vec<String>,
    /// Schriftgröße in Nutzerkoordinaten.
    pub groesse: f32,
    /// Ausrichtung in der Box.
    pub ausrichtung: Ausrichtung,
    /// Textfarbe; fehlt sie, gilt die neutrale Textfarbe.
    pub farbe: Option<Farbe>,
}

/// Eine Figur der Szene.
#[derive(Debug, Clone, PartialEq)]
pub enum Figur {
    /// Rechteck mit optionalem Eckenradius.
    Rechteck {
        /// Lage und Größe.
        mass: Rechteck,
        /// Eckenradius; `0` heißt scharfe Ecken.
        radius: f32,
        /// Füllfarbe; fehlt sie, bleibt die Figur weiß.
        farbe: Option<Farbe>,
    },
    /// Ellipse.
    Ellipse {
        /// Lage und Größe (umschließendes Rechteck).
        mass: Rechteck,
        /// Füllfarbe.
        farbe: Option<Farbe>,
    },
    /// Raute.
    Raute {
        /// Lage und Größe (umschließendes Rechteck).
        mass: Rechteck,
        /// Füllfarbe.
        farbe: Option<Farbe>,
    },
    /// Zylinder (stehend, Deckel oben).
    Zylinder {
        /// Lage und Größe.
        mass: Rechteck,
        /// Füllfarbe.
        farbe: Option<Farbe>,
    },
    /// Notiz mit umgeknickter Ecke.
    Notiz {
        /// Lage und Größe.
        mass: Rechteck,
        /// Füllfarbe.
        farbe: Option<Farbe>,
    },
    /// Rahmen mit Kopfzeile (draw.io: swimlane/Container, Excalidraw: frame).
    Container {
        /// Lage und Größe.
        mass: Rechteck,
        /// Überschrift in der Kopfzeile; fehlt sie, bleibt die Kopfzeile leer.
        titel: Option<String>,
    },
    /// Textblock ohne Rahmen.
    Text {
        /// Lage und Größe der Box.
        mass: Rechteck,
        /// Der Text selbst.
        text: Text,
    },
    /// Linienzug, wahlweise mit Pfeilspitze und gestrichelt.
    Linie {
        /// Die Stützpunkte; mindestens zwei.
        punkte: Vec<Punkt>,
        /// Gestrichelt zeichnen?
        gestrichelt: bool,
        /// Pfeilspitze am letzten Punkt?
        pfeil: bool,
    },
    /// Platzhalter für ein Bild, das die Kiste nicht zeichnet.
    Platzhalter {
        /// Lage und Größe.
        mass: Rechteck,
        /// Beschriftung im Platzhalter.
        beschriftung: Option<String>,
    },
}

impl Szene {
    /// Leere Szene mit fester Größe.
    pub fn leer(breite: f32, hoehe: f32) -> Szene {
        Szene {
            breite,
            hoehe,
            formen: Vec::new(),
        }
    }

    /// Baut die Szene aus einem Layout.
    ///
    /// Reihenfolge: Gruppenrahmen (hinten), Kanten, Knotenformen, Knotentexte,
    /// Kantenbeschriftungen (vorn).
    pub fn aus_zeichnung(zeichnung: &Zeichnung) -> Szene {
        let mut szene = Szene::leer(zeichnung.breite, zeichnung.hoehe);

        for gruppe in &zeichnung.gruppen {
            szene.formen.push(Figur::Container {
                mass: Rechteck::neu(gruppe.x, gruppe.y, gruppe.breite, gruppe.hoehe),
                titel: Some(gruppe.titel.clone()),
            });
        }

        for kante in &zeichnung.kanten {
            szene.formen.push(Figur::Linie {
                punkte: kante.punkte.clone(),
                gestrichelt: kante.art.ist_gestrichelt(),
                pfeil: kante.art.hat_pfeil(),
            });
        }

        for knoten in &zeichnung.knoten {
            szene.formen.push(figur_fuer(knoten));
        }

        for knoten in &zeichnung.knoten {
            szene.formen.push(Figur::Text {
                mass: Rechteck::neu(knoten.x, knoten.y, knoten.breite, knoten.hoehe),
                text: Text {
                    zeilen: knoten.zeilen.clone(),
                    groesse: knoten.schriftgroesse,
                    ausrichtung: Ausrichtung::Mitte,
                    farbe: None,
                },
            });
        }

        for kante in &zeichnung.kanten {
            if let Some(beschriftung) = &kante.beschriftung {
                szene.formen.push(Figur::Text {
                    mass: Rechteck::neu(
                        beschriftung.position.x,
                        beschriftung.position.y,
                        beschriftung.breite,
                        hoechste_zeile(&beschriftung.zeilen),
                    ),
                    text: Text {
                        zeilen: beschriftung.zeilen.clone(),
                        groesse: crate::layout::SCHRIFTGROESSE,
                        ausrichtung: Ausrichtung::Mitte,
                        farbe: None,
                    },
                });
            }
        }

        szene
    }

    /// Die Szene als eigenständiges SVG-Dokument.
    ///
    /// Kein Skript, keine externen Verweise; alle Texte sind maskiert.
    pub fn svg(&self) -> String {
        let breite = self.breite.max(1.0);
        let hoehe = self.hoehe.max(1.0);
        let mut svg = String::with_capacity(4096);
        let _ = write!(
            svg,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">",
            zahl(breite.ceil()),
            zahl(hoehe.ceil()),
            zahl(breite),
            zahl(hoehe)
        );
        let _ = write!(
            svg,
            "<rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"{}\"/>",
            zahl(breite),
            zahl(hoehe),
            palette::PAPIER
        );

        for figur in &self.formen {
            zeichne_figur(&mut svg, figur);
        }
        svg.push_str("</svg>");
        svg
    }

    /// Die Szene als PNG, höchstens [`MAX_PNG_KANTE`] Pixel je Kante.
    ///
    /// Kleine Bilder bleiben klein; ein Wunsch unter 16 wird auf 16 px angehoben,
    /// einer über 2000 auf 2000 gekappt. Die Schrift ist eingebettet; es werden
    /// **keine** Systemschriften benutzt, damit das Bild überall gleich aussieht.
    pub fn png(&self, max_kante: u32) -> Result<Vec<u8>, ZeichenFehler> {
        let max_kante = max_kante.clamp(16, MAX_PNG_KANTE);
        let breite = self.breite.max(1.0);
        let hoehe = self.hoehe.max(1.0);
        let laengste = breite.max(hoehe);
        let faktor = (max_kante as f32 / laengste).min(1.0);
        let px_breite = (breite * faktor).round().max(1.0) as u32;
        let px_hoehe = (hoehe * faktor).round().max(1.0) as u32;

        let svg = self.svg();
        let mut optionen = usvg::Options::default();
        optionen.fontdb_mut().load_font_data(SCHRIFT_DATEN.to_vec());
        optionen.font_family = SCHRIFT_FAMILIE.to_string();
        let baum = usvg::Tree::from_str(&svg, &optionen)
            .map_err(|fehler| ZeichenFehler::Png(format!("SVG nicht lesbar: {fehler}")))?;
        let mut pixmap = resvg::tiny_skia::Pixmap::new(px_breite, px_hoehe)
            .ok_or_else(|| ZeichenFehler::Png("Bildfläche ist leer".to_string()))?;
        let transform = resvg::tiny_skia::Transform::from_scale(faktor, faktor);
        resvg::render(&baum, transform, &mut pixmap.as_mut());
        pixmap
            .encode_png()
            .map_err(|fehler| ZeichenFehler::Png(fehler.to_string()))
    }
}

fn figur_fuer(knoten: &crate::layout::KnotenPlatz) -> Figur {
    let mass = Rechteck::neu(knoten.x, knoten.y, knoten.breite, knoten.hoehe);
    match knoten.form {
        Form::Rechteck => Figur::Rechteck {
            mass,
            radius: 8.0,
            farbe: knoten.farbe,
        },
        Form::Raute => Figur::Raute {
            mass,
            farbe: knoten.farbe,
        },
        Form::Ellipse => Figur::Ellipse {
            mass,
            farbe: knoten.farbe,
        },
        Form::Zylinder => Figur::Zylinder {
            mass,
            farbe: knoten.farbe,
        },
        Form::Notiz => Figur::Notiz {
            mass,
            farbe: knoten.farbe,
        },
    }
}

fn hoechste_zeile(zeilen: &[String]) -> f32 {
    zeilen.len() as f32 * crate::layout::ZEILENHOEHE
}

fn zeichne_figur(svg: &mut String, figur: &Figur) {
    match figur {
        Figur::Rechteck {
            mass,
            radius,
            farbe,
        } => {
            let toene = palette::toene(*farbe);
            let radius = radius.min(mass.breite / 2.0).min(mass.hoehe / 2.0);
            let _ = write!(
                svg,
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" ry=\"{}\"",
                zahl(mass.x),
                zahl(mass.y),
                zahl(mass.breite),
                zahl(mass.hoehe),
                zahl(radius),
                zahl(radius)
            );
            schliesse_form(svg, &toene);
        }
        Figur::Ellipse { mass, farbe } => {
            let toene = palette::toene(*farbe);
            let _ = write!(
                svg,
                "<ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\"",
                zahl(mass.x + mass.breite / 2.0),
                zahl(mass.y + mass.hoehe / 2.0),
                zahl(mass.breite / 2.0),
                zahl(mass.hoehe / 2.0)
            );
            schliesse_form(svg, &toene);
        }
        Figur::Raute { mass, farbe } => {
            let toene = palette::toene(*farbe);
            let _ = write!(
                svg,
                "<polygon points=\"{},{} {},{} {},{} {},{}\"",
                zahl(mass.x + mass.breite / 2.0),
                zahl(mass.y),
                zahl(mass.x + mass.breite),
                zahl(mass.y + mass.hoehe / 2.0),
                zahl(mass.x + mass.breite / 2.0),
                zahl(mass.y + mass.hoehe),
                zahl(mass.x),
                zahl(mass.y + mass.hoehe / 2.0)
            );
            schliesse_form(svg, &toene);
        }
        Figur::Zylinder { mass, farbe } => {
            let toene = palette::toene(*farbe);
            let deckel = (mass.hoehe * 0.18).clamp(6.0, 24.0);
            let _ = write!(
                svg,
                "<path d=\"M {} {} A {} {} 0 0 1 {} {} L {} {} A {} {} 0 0 1 {} {} Z\"",
                zahl(mass.x),
                zahl(mass.y + deckel),
                zahl(mass.breite / 2.0),
                zahl(deckel),
                zahl(mass.x + mass.breite),
                zahl(mass.y + deckel),
                zahl(mass.x + mass.breite),
                zahl(mass.y + mass.hoehe - deckel),
                zahl(mass.breite / 2.0),
                zahl(deckel),
                zahl(mass.x),
                zahl(mass.y + mass.hoehe - deckel)
            );
            schliesse_form(svg, &toene);
            let _ = write!(
                svg,
                "<ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\"/>",
                zahl(mass.x + mass.breite / 2.0),
                zahl(mass.y + deckel),
                zahl(mass.breite / 2.0),
                zahl(deckel),
                toene.rand,
                palette::STRICH
            );
        }
        Figur::Notiz { mass, farbe } => {
            let toene = palette::toene(*farbe);
            let knick = 16.0_f32.min(mass.breite / 3.0).min(mass.hoehe / 3.0);
            let _ = write!(
                svg,
                "<path d=\"M {} {} L {} {} L {} {} L {} {} L {} {} Z\"",
                zahl(mass.x),
                zahl(mass.y),
                zahl(mass.x + mass.breite),
                zahl(mass.y),
                zahl(mass.x + mass.breite),
                zahl(mass.y + mass.hoehe - knick),
                zahl(mass.x + mass.breite - knick),
                zahl(mass.y + mass.hoehe),
                zahl(mass.x),
                zahl(mass.y + mass.hoehe)
            );
            schliesse_form(svg, &toene);
            let _ = write!(
                svg,
                "<path d=\"M {} {} L {} {} L {} {}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\"/>",
                zahl(mass.x + mass.breite),
                zahl(mass.y + mass.hoehe - knick),
                zahl(mass.x + mass.breite - knick),
                zahl(mass.y + mass.hoehe - knick),
                zahl(mass.x + mass.breite - knick),
                zahl(mass.y + mass.hoehe),
                toene.rand,
                palette::STRICH
            );
        }
        Figur::Container { mass, titel } => {
            let _ = write!(
                svg,
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"6\" ry=\"6\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" stroke-dasharray=\"6 4\"/>",
                zahl(mass.x),
                zahl(mass.y),
                zahl(mass.breite),
                zahl(mass.hoehe),
                palette::GRUPPE,
                palette::STRICH
            );
            if let Some(titel) = titel {
                if !titel.trim().is_empty() {
                    let _ = write!(
                        svg,
                        "<text x=\"{}\" y=\"{}\" font-family=\"{}\" font-size=\"{}\" font-weight=\"600\" fill=\"{}\" text-anchor=\"start\">{}</text>",
                        zahl(mass.x + 12.0),
                        zahl(mass.y + 20.0),
                        escape(SCHRIFT_FAMILIE),
                        zahl(crate::layout::SCHRIFTGROESSE),
                        palette::TEXT,
                        escape(titel)
                    );
                }
            }
        }
        Figur::Text { mass, text } => {
            zeichne_text(svg, mass, text);
        }
        Figur::Linie {
            punkte,
            gestrichelt,
            pfeil,
        } => {
            if punkte.len() < 2 {
                return;
            }
            let mut punkte_text = String::new();
            for (i, punkt) in punkte.iter().enumerate() {
                if i > 0 {
                    punkte_text.push(' ');
                }
                let _ = write!(punkte_text, "{},{}", zahl(punkt.x), zahl(punkt.y));
            }
            let _ = write!(
                svg,
                "<polyline points=\"{punkte_text}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" stroke-linejoin=\"round\"",
                palette::LINIE,
                palette::STRICH
            );
            if *gestrichelt {
                svg.push_str(" stroke-dasharray=\"7 5\"");
            }
            svg.push_str("/>");
            if *pfeil {
                if let Some(spitze) = pfeilspitze(punkte) {
                    let _ = write!(
                        svg,
                        "<polygon points=\"{}\" fill=\"{}\"/>",
                        spitze,
                        palette::LINIE
                    );
                }
            }
        }
        Figur::Platzhalter { mass, beschriftung } => {
            let _ = write!(
                svg,
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\" stroke-dasharray=\"4 4\"/>",
                zahl(mass.x),
                zahl(mass.y),
                zahl(mass.breite),
                zahl(mass.hoehe),
                palette::PAPIER,
                palette::LINIE,
                palette::STRICH
            );
            if let Some(text) = beschriftung {
                let innen = Rechteck::neu(mass.x, mass.y, mass.breite, mass.hoehe);
                let block = Text {
                    zeilen: vec![text.clone()],
                    groesse: crate::layout::SCHRIFTGROESSE,
                    ausrichtung: Ausrichtung::Mitte,
                    farbe: Some(Farbe::Grau),
                };
                zeichne_text(svg, &innen, &block);
            }
        }
    }
}

fn schliesse_form(svg: &mut String, toene: &palette::Toene) {
    let _ = write!(
        svg,
        " fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\"/>",
        toene.fuellung,
        toene.rand,
        palette::STRICH
    );
}

fn zeichne_text(svg: &mut String, mass: &Rechteck, text: &Text) {
    if text.zeilen.is_empty() {
        return;
    }
    let textfarbe = match text.farbe {
        Some(farbe) => palette::toene(Some(farbe)).text,
        None => palette::TEXT,
    };
    let (anker, x) = match text.ausrichtung {
        Ausrichtung::Links => ("start", mass.x),
        Ausrichtung::Mitte => ("middle", mass.x + mass.breite / 2.0),
        Ausrichtung::Rechts => ("end", mass.x + mass.breite),
    };
    let gesamt = text.zeilen.len() as f32 * crate::layout::ZEILENHOEHE;
    let oben = mass.y + (mass.hoehe - gesamt) / 2.0;
    for (i, zeile) in text.zeilen.iter().enumerate() {
        if zeile.is_empty() {
            continue;
        }
        let y =
            oben + (i as f32 + 1.0) * crate::layout::ZEILENHOEHE - crate::layout::ZEILENHOEHE * 0.3;
        let _ = write!(
            svg,
            "<text x=\"{}\" y=\"{}\" font-family=\"{}\" font-size=\"{}\" fill=\"{}\" text-anchor=\"{anker}\">{}</text>",
            zahl(x),
            zahl(y),
            escape(SCHRIFT_FAMILIE),
            zahl(text.groesse),
            textfarbe,
            escape(zeile)
        );
    }
}

fn pfeilspitze(punkte: &[Punkt]) -> Option<String> {
    let ende = punkte.last()?;
    let vorher = punkte.get(punkte.len().checked_sub(2)?)?;
    let dx = ende.x - vorher.x;
    let dy = ende.y - vorher.y;
    let laenge = (dx * dx + dy * dy).sqrt();
    if laenge < 0.5 {
        return None;
    }
    let (ux, uy) = (dx / laenge, dy / laenge);
    let (nx, ny) = (-uy, ux);
    let groesse = 9.0;
    let halb = 4.2;
    let basis = Punkt {
        x: ende.x - ux * groesse,
        y: ende.y - uy * groesse,
    };
    Some(format!(
        "{},{} {},{} {},{}",
        zahl(ende.x),
        zahl(ende.y),
        zahl(basis.x + nx * halb),
        zahl(basis.y + ny * halb),
        zahl(basis.x - nx * halb),
        zahl(basis.y - ny * halb)
    ))
}

/// Zahl mit höchstens einer Nachkommastelle (stabil und kurz im SVG).
fn zahl(wert: f32) -> String {
    if !wert.is_finite() {
        return "0".to_string();
    }
    let gerundet = (wert * 10.0).round() / 10.0;
    if (gerundet - gerundet.round()).abs() < f32::EPSILON {
        format!("{}", gerundet.round() as i64)
    } else {
        format!("{gerundet:.1}")
    }
}

/// Maskiert Text für SVG-Inhalt und Attributwerte.
///
/// `&`, `<`, `>`, `"` und `'` werden ersetzt. Zeilenumbruch und Tabulator werden
/// zu einem Leerzeichen (ein SVG-Text ist eine Zeile), andere Steuerzeichen
/// fallen weg. Damit kann ein Knotentext kein Element, Attribut oder Skript
/// einschleusen.
pub fn escape(text: &str) -> String {
    let mut ausgabe = String::with_capacity(text.len());
    for zeichen in text.chars() {
        match zeichen {
            '&' => ausgabe.push_str("&amp;"),
            '<' => ausgabe.push_str("&lt;"),
            '>' => ausgabe.push_str("&gt;"),
            '"' => ausgabe.push_str("&quot;"),
            '\'' => ausgabe.push_str("&apos;"),
            '\n' => ausgabe.push(' '),
            '\t' => ausgabe.push(' '),
            z if z.is_control() => {}
            z => ausgabe.push(z),
        }
    }
    ausgabe
}

/// Die konkreten Farbwerte der semantischen Farben.
///
/// **Der einzige Ort mit Farbwerten** in dieser Kiste. Die Namen kommen aus
/// [`Farbe`]; hier stehen Papierton, Linien, Rahmen und je Farbe drei Töne
/// (Füllung, Rand, Text). Kein Werkzeug darf eigene Werte erfinden.
pub mod palette {
    use super::Farbe;

    /// Hintergrund des Diagramms.
    pub const PAPIER: &str = "#ffffff";
    /// Farbe der Kanten.
    pub const LINIE: &str = "#4a5560";
    /// Standard-Textfarbe.
    pub const TEXT: &str = "#1f2933";
    /// Rahmenfarbe von Gruppen und Platzhaltern.
    pub const GRUPPE: &str = "#8a94a0";
    /// Strichstärke aller Linien und Rahmen.
    pub const STRICH: &str = "1.5";

    /// Die drei Töne einer Farbe.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Toene {
        /// Füllfläche.
        pub fuellung: &'static str,
        /// Rand der Fläche.
        pub rand: &'static str,
        /// Text auf der Fläche.
        pub text: &'static str,
    }

    /// Töne für eine Farbe; ohne Farbe ein neutrales Weiß/Grau.
    pub fn toene(farbe: Option<Farbe>) -> Toene {
        match farbe {
            None => Toene {
                fuellung: "#f7f8fa",
                rand: "#8a94a0",
                text: TEXT,
            },
            Some(Farbe::Blau) => Toene {
                fuellung: "#e2efff",
                rand: "#2f6db5",
                text: "#123a63",
            },
            Some(Farbe::Gruen) => Toene {
                fuellung: "#e2f6e7",
                rand: "#2f7d43",
                text: "#14401f",
            },
            Some(Farbe::Rot) => Toene {
                fuellung: "#fde7e7",
                rand: "#b53a3a",
                text: "#5c1a1a",
            },
            Some(Farbe::Gelb) => Toene {
                fuellung: "#fdf3d0",
                rand: "#a8871f",
                text: "#4d3d08",
            },
            Some(Farbe::Grau) => Toene {
                fuellung: "#eceef1",
                rand: "#6b7480",
                text: TEXT,
            },
            Some(Farbe::Violett) => Toene {
                fuellung: "#f0e8fb",
                rand: "#6b46a8",
                text: "#33205c",
            },
            Some(Farbe::Orange) => Toene {
                fuellung: "#fdeedd",
                rand: "#b06a22",
                text: "#5a3208",
            },
            Some(Farbe::Tuerkis) => Toene {
                fuellung: "#ddf3f3",
                rand: "#2b8181",
                text: "#123d3d",
            },
        }
    }
}

/// Maße und Umbruch der eingebetteten Schrift.
///
/// Layout und Zeichnen benutzen dieselben Zahlen; so passt der Text immer in
/// den Knoten, den das Layout berechnet hat.
pub mod schrift {
    use super::SCHRIFT_DATEN;
    use std::sync::OnceLock;
    use ttf_parser::Face;

    /// Zeilenhöhe im Verhältnis zur Schriftgröße.
    pub const ZEILENFAKTOR: f32 = 1.35;

    fn flaeche() -> &'static Face<'static> {
        static FLAECHE: OnceLock<Face<'static>> = OnceLock::new();
        FLAECHE.get_or_init(|| {
            Face::parse(SCHRIFT_DATEN, 0).expect("die eingebettete Schrift ist gültig")
        })
    }

    /// Breite eines Textes ohne Umbruch.
    pub fn breite(text: &str, groesse: f32) -> f32 {
        let face = flaeche();
        let einheit = face.units_per_em() as f32;
        if einheit <= 0.0 {
            return text.chars().count() as f32 * groesse * 0.5;
        }
        let mut summe = 0.0;
        for zeichen in text.chars() {
            let vorschub = face
                .glyph_index(zeichen)
                .and_then(|glyph| face.glyph_hor_advance(glyph))
                .map(f32::from)
                .unwrap_or(einheit * 0.5);
            summe += vorschub;
        }
        summe * groesse / einheit
    }

    /// Höhe einer Zeile bei dieser Schriftgröße.
    pub fn zeilenhoehe(groesse: f32) -> f32 {
        groesse * ZEILENFAKTOR
    }

    /// Bricht `text` an Wortgrenzen auf `max_breite` um.
    ///
    /// Zeilenumbrüche im Text bleiben erhalten. Ein einzelnes Wort, das breiter
    /// ist als die Box, wird zeichenweise getrennt – so bleibt der Text im
    /// Knoten und die Überlappungsfreiheit erhalten.
    pub fn umbrechen(text: &str, groesse: f32, max_breite: f32) -> Vec<String> {
        let max_breite = max_breite.max(groesse);
        let mut zeilen = Vec::new();
        for absatz in text.split('\n') {
            let mut aktuell = String::new();
            for wort in absatz.split_whitespace() {
                anhaengen(&mut zeilen, &mut aktuell, wort, groesse, max_breite);
            }
            zeilen.push(aktuell);
        }
        if zeilen.is_empty() {
            zeilen.push(String::new());
        }
        zeilen
    }

    /// Hängt ein Wort an die laufende Zeile an; bricht um, wenn es nicht passt,
    /// und trennt ein zu breites Wort zeichenweise.
    fn anhaengen(
        zeilen: &mut Vec<String>,
        aktuell: &mut String,
        wort: &str,
        groesse: f32,
        max_breite: f32,
    ) {
        if !aktuell.is_empty() {
            let mut probe = aktuell.clone();
            probe.push(' ');
            probe.push_str(wort);
            if breite(&probe, groesse) <= max_breite {
                *aktuell = probe;
                return;
            }
            zeilen.push(std::mem::take(aktuell));
        }
        if breite(wort, groesse) <= max_breite {
            aktuell.push_str(wort);
            return;
        }
        let mut stueck = String::new();
        for zeichen in wort.chars() {
            let mut probe = stueck.clone();
            probe.push(zeichen);
            if breite(&probe, groesse) > max_breite && !stueck.is_empty() {
                zeilen.push(std::mem::take(&mut stueck));
            }
            stueck.push(zeichen);
        }
        *aktuell = stueck;
    }
}

/// Fehler beim Zeichnen.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ZeichenFehler {
    /// Das Bild ließ sich nicht erzeugen.
    #[error("Bild ließ sich nicht erzeugen: {0}")]
    Png(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modell::Diagramm;

    fn beispiel() -> Szene {
        let mut diagramm = Diagramm::leer();
        diagramm.knoten = vec![
            crate::modell::Knoten {
                farbe: Some(Farbe::Blau),
                ..crate::modell::Knoten::neu("a", "Antrag <script>alert(1)</script>")
            },
            crate::modell::Knoten::neu("b", "Prüfung & Freigabe"),
        ];
        diagramm.kanten = vec![crate::modell::Kante::neu("a", "b")];
        let zeichnung = crate::layout::layout(&diagramm).expect("Layout");
        Szene::aus_zeichnung(&zeichnung)
    }

    #[test]
    fn texte_werden_maskiert() {
        let svg = beispiel().svg();
        assert!(!svg.contains("<script"), "kein Skript im SVG: {svg}");
        assert!(svg.contains("&lt;script&gt;"));
        assert!(svg.contains("&amp;"));
        assert!(!svg.contains("onload"));
        assert!(!svg.contains("<image"));
        assert!(!svg.contains("href"));
    }

    #[test]
    fn svg_ist_wohlgeformtes_xml() {
        let svg = beispiel().svg();
        let dokument = usvg::roxmltree::Document::parse(&svg).expect("SVG ist XML");
        let wurzel = dokument.root_element();
        assert_eq!(wurzel.tag_name().name(), "svg");
        assert!(wurzel.attribute("width").is_some());
    }

    #[test]
    fn png_entsteht_und_bleibt_unter_der_grenze() {
        let png = beispiel().png(2000).expect("PNG");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let breite = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
        let hoehe = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
        assert!(breite <= MAX_PNG_KANTE && hoehe <= MAX_PNG_KANTE);
        assert!(breite > 0 && hoehe > 0);
    }

    #[test]
    fn riesige_szene_wird_heruntergerechnet() {
        let szene = Szene::leer(9000.0, 4000.0);
        let png = szene.png(2000).expect("PNG");
        let breite = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
        let hoehe = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
        assert!(breite <= MAX_PNG_KANTE && hoehe <= MAX_PNG_KANTE);
    }

    #[test]
    fn zeichnung_wird_zur_szene() {
        let szene = beispiel();
        assert!(szene
            .formen
            .iter()
            .any(|f| matches!(f, Figur::Raute { .. }) || matches!(f, Figur::Rechteck { .. })));
        assert!(szene
            .formen
            .iter()
            .any(|f| matches!(f, Figur::Linie { .. })));
    }

    #[test]
    fn escape_kennt_alle_fuenf_zeichen() {
        assert_eq!(escape("<&>\"'"), "&lt;&amp;&gt;&quot;&apos;");
        assert_eq!(escape("a\nb\tc"), "a b c");
    }
}
