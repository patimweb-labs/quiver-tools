// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Fuzz-Test über alle Leser von draw.io-Dateien.
//!
//! Fester Startwert, 20 000 erzeugte Eingaben: XML, unkomprimierte und
//! komprimierte Seiten, Stil- und Farbwerte. Geprüft wird nur, dass kein Leser
//! das Programm beendet – jeder Fehler darf als `Err` zurückkommen, ein Panik
//! nicht. Der Startwert ist fest, damit ein Fehlschlag wiederholbar ist.

use std::panic::{catch_unwind, AssertUnwindSafe};

use quiver_drawio::{formen, link, mx, stil, xml};

/// Ein kleiner, fester Zufallsgenerator (xorshift64\*).
struct Zufall(u64);

impl Zufall {
    fn neu(start: u64) -> Zufall {
        Zufall(start | 1)
    }

    fn naechste(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn bis(&mut self, grenze: usize) -> usize {
        if grenze == 0 {
            return 0;
        }
        (self.naechste() % grenze as u64) as usize
    }
}

/// Bausteine, aus denen die Eingaben entstehen – auch Mehrbyte-Zeichen.
const BAUSTEINE: &[char] = &[
    '<', '>', '/', '=', '"', '\'', '&', ';', '#', ':', '.', '\\', '\n', '\r', '\0', ' ', 'a', 'F',
    '0', '9', '-', '_', 'ä', 'ö', 'ü', 'ß', '€', '😀', '中',
];

/// Die Vorlagen, die verändert werden.
fn vorlagen() -> Vec<String> {
    let seite = r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="2" value="A" style="rounded=1;fillColor=#ff0000;" vertex="1" parent="1"><mxGeometry x="1" y="2" width="3" height="4" as="geometry"/></mxCell></root></mxGraphModel>"#;
    vec![
        format!(
            r#"<mxfile host="test"><diagram id="a" name="Seite">{seite}</diagram></mxfile>"#
        ),
        "<mxfile><diagram> &lt;mxGraphModel&gt;&lt;root/&gt;&lt;/mxGraphModel&gt; </diagram></mxfile>".to_string(),
        link::komprimieren(seite).unwrap_or_default(),
        r#"fillColor=#ff0000;strokeColor=#00ff00;rounded=1;html=1;"#.to_string(),
        "#ff0000".to_string(),
        "😀ab".to_string(),
        String::new(),
    ]
}

/// Eine Vorlage zufällig verändern: einfügen, löschen, ersetzen, verdoppeln.
fn veraendern(zufall: &mut Zufall, vorlage: &str) -> String {
    let mut zeichen: Vec<char> = vorlage.chars().collect();
    for _ in 0..1 + zufall.bis(4) {
        let stelle = zufall.bis(zeichen.len() + 1);
        match zufall.bis(4) {
            0 => zeichen.insert(stelle, BAUSTEINE[zufall.bis(BAUSTEINE.len())]),
            1 if stelle < zeichen.len() => {
                zeichen.remove(stelle);
            }
            2 if stelle < zeichen.len() => {
                zeichen[stelle] = BAUSTEINE[zufall.bis(BAUSTEINE.len())];
            }
            _ => {
                let anfang = zufall.bis(zeichen.len() + 1);
                let rest: Vec<char> = zeichen[anfang..].to_vec();
                zeichen.insert(stelle, BAUSTEINE[zufall.bis(BAUSTEINE.len())]);
                let _ = rest;
            }
        }
    }
    zeichen.into_iter().collect()
}

#[test]
fn zwanzigtausend_eingaben_beenden_keinen_leser() {
    let mut zufall = Zufall::neu(0x5EED_1234_ABCD_0001);
    let vorlagen = vorlagen();

    for runde in 0..20_000 {
        let vorlage = &vorlagen[zufall.bis(vorlagen.len())];
        let eingabe = veraendern(&mut zufall, vorlage);

        let ergebnis = catch_unwind(AssertUnwindSafe(|| {
            // Alle Leser, die mit Dateiinhalt zu tun haben.
            let _ = xml::lesen(&eingabe);
            let _ = mx::Datei::aus_text(&eingabe);
            let _ = link::entpacken(&eingabe);
            let _ = formen::farbe_aus_hex(&eingabe);
            let _ = stil::Stil::aus_text(&eingabe);
            if let Ok(datei) = mx::Datei::aus_text(&eingabe) {
                let _ = datei.als_text();
                if let Ok(seiten) = datei.seiten() {
                    for seite in seiten {
                        let _ = seite.komprimiert;
                        let _ = seite.name.clone();
                    }
                }
            }
        }));

        assert!(
            ergebnis.is_ok(),
            "Runde {runde} hat einen Leser beendet; Eingabe: {eingabe:?}"
        );
    }
}
