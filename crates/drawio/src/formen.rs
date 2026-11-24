// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Formen und Farben, die dieses Werkzeug selbst setzt.
//!
//! Die Auswahl ist fest und deckt ab, was der eigene Zeichner kann (Aufgabe 18):
//! Rechteck, Raute, Ellipse, Zylinder, Notiz, Text und Schwimmbahn/Container.
//! Was eine Datei sonst enthält, bleibt beim Ändern unangetastet; beim Zeichnen
//! wird es als Rechteck gezeichnet und die Antwort nennt den Namen.
//!
//! Farben sind **semantische Namen** aus `quiver_diagramm`; ihre konkreten Werte
//! stehen an genau einer Stelle (`zeichnen::palette`). Hier wird nichts
//! erfunden.

use quiver_diagramm::modell::Farbe;
use quiver_diagramm::zeichnen::palette;

use crate::stil::Stil;

/// Die Formen, die dieses Werkzeug kennt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// Rechteck mit runden Ecken (Vorgang, Schritt).
    Rechteck,
    /// Raute (Entscheidung).
    Raute,
    /// Ellipse (Start, Ende).
    Ellipse,
    /// Zylinder (Daten, Speicher).
    Zylinder,
    /// Notiz mit umgeknickter Ecke.
    Notiz,
    /// Text ohne Rahmen.
    Text,
    /// Schwimmbahn/Container mit Kopfzeile.
    Schwimmbahn,
}

impl Form {
    /// Alle Formen in der Reihenfolge der Dokumentation.
    pub const ALLE: [Form; 7] = [
        Form::Rechteck,
        Form::Raute,
        Form::Ellipse,
        Form::Zylinder,
        Form::Notiz,
        Form::Text,
        Form::Schwimmbahn,
    ];

    /// Name im Werkzeugaufruf (`rechteck`, `raute`, …).
    pub fn name(self) -> &'static str {
        match self {
            Form::Rechteck => "rechteck",
            Form::Raute => "raute",
            Form::Ellipse => "ellipse",
            Form::Zylinder => "zylinder",
            Form::Notiz => "notiz",
            Form::Text => "text",
            Form::Schwimmbahn => "schwimmbahn",
        }
    }

    /// Umgekehrter Weg: Name aus dem Aufruf.
    pub fn aus_name(name: &str) -> Option<Form> {
        Form::ALLE
            .into_iter()
            .find(|form| form.name() == name.trim().to_ascii_lowercase())
    }

    /// Die Namen als Aufzählung für Fehlermeldungen.
    pub fn namen() -> String {
        Form::ALLE
            .iter()
            .map(|form| form.name())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Der Stil, den [`Form::anwenden`] auf eine leere Kette schreibt.
    pub fn stil(self) -> Stil {
        let mut stil = Stil::aus_text("");
        self.anwenden(&mut stil);
        stil
    }

    /// Setzt diese Form in einem vorhandenen Stil.
    ///
    /// Geändert werden nur das Formwort, `shape` und die Schlüssel, die die Form
    /// braucht; alles andere (Schrift, Farbe, eigene Schlüssel) bleibt stehen.
    pub fn anwenden(self, stil: &mut Stil) {
        stil.entferne_form();
        stil.entferne("shape");
        match self {
            Form::Rechteck => stil.setze("rounded", "1"),
            Form::Raute => stil.setze_form("rhombus"),
            Form::Ellipse => stil.setze_form("ellipse"),
            Form::Zylinder => {
                stil.setze("shape", "cylinder3");
                stil.setze("boundedLbl", "1");
            }
            Form::Notiz => {
                stil.setze("shape", "note");
                stil.setze("size", "15");
            }
            Form::Text => stil.setze_form("text"),
            Form::Schwimmbahn => {
                stil.setze_form("swimlane");
                stil.setze("startSize", "30");
            }
        }
        stil.setze("whiteSpace", "wrap");
        stil.setze("html", "1");
    }

    /// Die Form, die ein Stil beschreibt – `None`, wenn es eine fremde ist.
    pub fn aus_stil(stil: &Stil) -> Option<Form> {
        if let Some(shape) = stil.wert("shape") {
            return match shape.trim() {
                "cylinder3" | "cylinder" | "datastore" => Some(Form::Zylinder),
                "note" => Some(Form::Notiz),
                "text" => Some(Form::Text),
                "swimlane" => Some(Form::Schwimmbahn),
                "rect" | "rectangle" => Some(Form::Rechteck),
                _ => None,
            };
        }
        match stil.form() {
            None => Some(Form::Rechteck),
            Some("rhombus") => Some(Form::Raute),
            Some("ellipse" | "doubleEllipse") => Some(Form::Ellipse),
            Some("text") => Some(Form::Text),
            Some("swimlane" | "container") => Some(Form::Schwimmbahn),
            Some("rounded" | "rectangle" | "rect") => Some(Form::Rechteck),
            Some(_) => None,
        }
    }

    /// Setzt eine Füllfarbe samt Rand und Schrift.
    pub fn farbe_setzen(stil: &mut Stil, farbe: Farbe) {
        let toene = palette::toene(Some(farbe));
        stil.setze("fillColor", toene.fuellung);
        stil.setze("strokeColor", toene.rand);
        stil.setze("fontColor", toene.text);
    }

    /// Die semantische Farbe, die ein Stil als Füllung trägt.
    pub fn farbe_aus_stil(stil: &Stil) -> Option<Farbe> {
        let fuellung = stil.wert("fillColor")?;
        Farbe::ALLE.into_iter().find(|farbe| {
            palette::toene(Some(*farbe))
                .fuellung
                .eq_ignore_ascii_case(fuellung)
        })
    }
}

/// Eine Füllfarbe aus einer Datei auf die nächste semantische Farbe abbilden.
///
/// Der Zeichner kennt nur die feste Palette. Was nah dran ist, wird in dieser
/// Farbe gezeichnet; `none`, leer und Weiß heißen „keine Füllung“.
pub fn farbe_aus_hex(wert: &str) -> Option<Farbe> {
    let (rot, gruen, blau) = rgb(wert)?;
    if (rot, gruen, blau) == (255, 255, 255) {
        return None;
    }
    let mut beste: Option<(i32, Farbe)> = None;
    for farbe in Farbe::ALLE {
        let (r, g, b) = rgb(palette::toene(Some(farbe)).fuellung).unwrap_or((255, 255, 255));
        let abstand = (r - rot).pow(2) + (g - gruen).pow(2) + (b - blau).pow(2);
        if beste.is_none_or(|(bisher, _)| abstand < bisher) {
            beste = Some((abstand, farbe));
        }
    }
    beste.map(|(_, farbe)| farbe)
}

/// `#rrggbb` oder `#rgb` als Zahlen; alles andere ergibt `None`.
fn rgb(wert: &str) -> Option<(i32, i32, i32)> {
    let wert = wert.trim().trim_start_matches('#');
    let (r, g, b) = match wert.len() {
        3 => (
            u8::from_str_radix(&wert[0..1].repeat(2), 16).ok()?,
            u8::from_str_radix(&wert[1..2].repeat(2), 16).ok()?,
            u8::from_str_radix(&wert[2..3].repeat(2), 16).ok()?,
        ),
        6 => (
            u8::from_str_radix(&wert[0..2], 16).ok()?,
            u8::from_str_radix(&wert[2..4], 16).ok()?,
            u8::from_str_radix(&wert[4..6], 16).ok()?,
        ),
        _ => return None,
    };
    Some((i32::from(r), i32::from(g), i32::from(b)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jede_form_hat_einen_stil_und_laesst_sich_zuruecklesen() {
        for form in Form::ALLE {
            let stil = form.stil();
            assert!(stil.als_text().contains("html=1"), "{form:?}");
            assert_eq!(Form::aus_stil(&stil), Some(form), "{form:?}");
        }
    }

    #[test]
    fn anwenden_laesst_fremde_schluessel_stehen() {
        let mut stil = Stil::aus_text("rounded=1;sketch=1;fillColor=#e2efff;");
        Form::Raute.anwenden(&mut stil);
        assert_eq!(
            stil.als_text(),
            "rhombus;rounded=1;sketch=1;fillColor=#e2efff;whiteSpace=wrap;html=1;"
        );
        assert_eq!(stil.wert("sketch"), Some("1"));
    }

    #[test]
    fn farben_kommen_aus_der_palette() {
        let mut stil = Stil::aus_text("rounded=1;");
        Form::farbe_setzen(&mut stil, Farbe::Gruen);
        assert_eq!(
            stil.wert("fillColor"),
            Some(palette::toene(Some(Farbe::Gruen)).fuellung)
        );
        assert_eq!(Form::farbe_aus_stil(&stil), Some(Farbe::Gruen));
    }

    #[test]
    fn eine_fremde_fuellung_wird_auf_die_naechste_farbe_abgebildet() {
        assert_eq!(farbe_aus_hex("#e2efff"), Some(Farbe::Blau));
        assert_eq!(farbe_aus_hex("#e0edfd"), Some(Farbe::Blau));
        assert_eq!(farbe_aus_hex("#ffffff"), None);
        assert_eq!(farbe_aus_hex("none"), None);
        assert_eq!(farbe_aus_hex("quatsch"), None);
    }
}
