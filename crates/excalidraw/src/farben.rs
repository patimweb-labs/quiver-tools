// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Farben und Formen zwischen Modell und Datei übersetzen.
//!
//! Das Modell denkt in **Namen** (`blau`, `raute`), die Datei in **Werten**
//! (`#e2efff`, `diamond`). Diese Übersetzung ist der einzige Ort, an dem beides
//! zusammenkommt. Die konkreten Farbwerte stehen weiterhin nur in
//! [`quiver_diagramm::zeichnen::palette`] – hier wird nichts erfunden.

use quiver_diagramm::modell::{Farbe, Form, KanteArt};
use quiver_diagramm::zeichnen::palette::{self, Toene};
use quiver_diagramm::zeichnen::schrift;

/// Die Töne einer semantischen Farbe.
pub fn toene(farbe: Farbe) -> Toene {
    palette::toene(Some(farbe))
}

/// Eine semantische Farbe aus ihrem Namen (`blau`, `gruen`, …).
pub fn farbe_aus_name(name: &str) -> Option<Farbe> {
    Farbe::aus_name(name.trim())
}

/// Die Excalidraw-Art zu einer Modellform.
///
/// Zylinder und Notiz gibt es in Excalidraw nicht; sie werden zum Rechteck
/// (was fehlt, steht in der README unter „Offen“).
pub fn art_der_form(form: Form) -> &'static str {
    match form {
        Form::Rechteck | Form::Zylinder | Form::Notiz => "rectangle",
        Form::Raute => "diamond",
        Form::Ellipse => "ellipse",
    }
}

/// Die Modellform zu einer Excalidraw-Art.
pub fn form_der_art(art: &str) -> Option<Form> {
    match art {
        "rectangle" => Some(Form::Rechteck),
        "diamond" => Some(Form::Raute),
        "ellipse" => Some(Form::Ellipse),
        _ => None,
    }
}

/// Strichart und Pfeilspitze zu einer Kantenart.
pub fn kantenstil(art: KanteArt) -> (&'static str, bool) {
    let gestrichelt = if art.ist_gestrichelt() {
        "dashed"
    } else {
        "solid"
    };
    (gestrichelt, art.hat_pfeil())
}

/// Eine Farbe aus einem Farbwert der Datei.
///
/// `transparent` und Schwarz/Weiß sind keine Modellfarben und ergeben `None`.
/// Andere Werte (etwa die Palette von excalidraw.com) werden auf den nächsten
/// Ton abgebildet – die Kiste zeichnet nur mit ihren eigenen Tönen.
pub fn farbe_aus_hex(hex: &str) -> Option<Farbe> {
    let wert = hex.trim().to_ascii_lowercase();
    if !wert.starts_with('#') {
        return None;
    }
    for farbe in Farbe::ALLE {
        let toene = toene(farbe);
        if [toene.fuellung, toene.rand, toene.text]
            .iter()
            .any(|ton| ton.eq_ignore_ascii_case(&wert))
        {
            return Some(farbe);
        }
    }
    let (r, g, b) = rgb(&wert)?;
    // Grau, Schwarz und Weiß sind keine Modellfarben, sondern neutral.
    if r.abs_diff(g) <= 12 && g.abs_diff(b) <= 12 && r.abs_diff(b) <= 12 {
        return None;
    }
    let mut beste: Option<(u32, Farbe)> = None;
    for farbe in Farbe::ALLE {
        let toene = toene(farbe);
        for ton in [toene.fuellung, toene.rand, toene.text] {
            let Some((tr, tg, tb)) = rgb(ton) else {
                continue;
            };
            let abstand = abweichung((r, g, b), (tr, tg, tb));
            if beste.is_none_or(|(bisher, _)| abstand < bisher) {
                beste = Some((abstand, farbe));
            }
        }
    }
    // Ohne echte Nähe bleibt das Element neutral.
    beste
        .filter(|(abstand, _)| *abstand <= 48 * 48 * 3)
        .map(|(_, f)| f)
}

/// `#rrggbb` oder `#rgb` als Zahlen.
///
/// Geprüft wird über Bytes: ein Wert wie `😀ab` hat sechs Bytes, aber keine
/// sechs Zeichen – ein Schnitt an Zeichengrenzen würde das Programm beenden.
fn rgb(hex: &str) -> Option<(u32, u32, u32)> {
    let wert = hex.trim_start_matches('#');
    if !wert.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let teile: Vec<u32> = match wert.len() {
        3 => wert
            .chars()
            .map(|zeichen| {
                let ziffer = zeichen.to_digit(16)?;
                Some(ziffer * 17)
            })
            .collect::<Option<Vec<u32>>>()?,
        6 => (0..3)
            .map(|i| u32::from_str_radix(&wert[i * 2..i * 2 + 2], 16).ok())
            .collect::<Option<Vec<u32>>>()?,
        _ => return None,
    };
    Some((teile[0], teile[1], teile[2]))
}

fn abweichung(a: (u32, u32, u32), b: (u32, u32, u32)) -> u32 {
    let dr = a.0.abs_diff(b.0);
    let dg = a.1.abs_diff(b.1);
    let db = a.2.abs_diff(b.2);
    dr * dr + dg * dg + db * db
}

/// Bricht `text` so um, wie es die eingebettete Schrift misst.
pub fn umbrechen(text: &str, groesse: f64, breite: f64) -> Vec<String> {
    schrift::umbrechen(text, groesse as f32, breite as f32)
}

/// Die Breite einer Zeile in der eingebetteten Schrift.
pub fn textbreite(text: &str, groesse: f64) -> f64 {
    f64::from(schrift::breite(text, groesse as f32))
}

#[cfg(test)]
mod mehrbyte_tests {
    use super::*;

    #[test]
    fn farbwerte_mit_mehrbyte_zeichen_paniken_nicht() {
        assert_eq!(farbe_aus_hex("😀ab"), None);
        assert_eq!(farbe_aus_hex("#€"), None);
        assert_eq!(farbe_aus_hex("#äöü"), None);
        // Ein Ton aus der Palette bleibt erkennbar.
        let ton = toene(Farbe::ALLE[0]).fuellung;
        assert_eq!(farbe_aus_hex(ton), Some(Farbe::ALLE[0]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn farbnamen_und_werte_passen_zusammen() {
        for farbe in Farbe::ALLE {
            let toene = toene(farbe);
            assert_eq!(farbe_aus_hex(toene.fuellung), Some(farbe));
            assert_eq!(farbe_aus_hex(toene.rand), Some(farbe));
            assert_eq!(farbe_aus_hex(toene.text), Some(farbe));
            // Großschreibung ist kein Unterschied.
            assert_eq!(farbe_aus_hex(&toene.rand.to_uppercase()), Some(farbe));
        }
    }

    #[test]
    fn transparent_und_schwarz_sind_keine_modellfarben() {
        assert_eq!(farbe_aus_hex("transparent"), None);
        assert_eq!(farbe_aus_hex("#1e1e1e"), None);
        assert_eq!(farbe_aus_hex(""), None);
        assert_eq!(farbe_aus_hex("#12345"), None);
    }

    #[test]
    fn eine_fremde_palette_wird_auf_den_naechsten_ton_abgebildet() {
        // Leicht abweichende Töne landen bei ihrer Farbe.
        assert_eq!(farbe_aus_hex("#e1eeff"), Some(Farbe::Blau));
        assert_eq!(farbe_aus_hex("#e1f6e8"), Some(Farbe::Gruen));
        // Eine ganz fremde Farbe ergibt irgendeine Modellfarbe, nie einen Fehler.
        assert!(farbe_aus_hex("#c0ffee").is_some());
    }

    #[test]
    fn formen_und_kanten_werden_uebersetzt() {
        assert_eq!(art_der_form(Form::Raute), "diamond");
        assert_eq!(art_der_form(Form::Zylinder), "rectangle");
        assert_eq!(form_der_art("ellipse"), Some(Form::Ellipse));
        assert_eq!(form_der_art("text"), None);
        assert_eq!(kantenstil(KanteArt::Pfeil), ("solid", true));
        assert_eq!(kantenstil(KanteArt::Linie), ("solid", false));
        assert_eq!(kantenstil(KanteArt::Gestrichelt), ("dashed", true));
    }
}
