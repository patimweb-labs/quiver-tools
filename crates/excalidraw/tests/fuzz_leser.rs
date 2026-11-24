// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Fuzz-Test über die Leser von Excalidraw-Dateien.
//!
//! Fester Startwert, 20 000 erzeugte Eingaben: JSON-Rumpf, Elemente, Farben.
//! Geprüft wird nur, dass kein Leser das Programm beendet; ein Fehler darf
//! immer als `Err` kommen.

use std::panic::{catch_unwind, AssertUnwindSafe};

use quiver_excalidraw::{datei::Datei, farben, pruefen};

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
    '{', '}', '[', ']', '"', ':', ',', '\\', '\n', '\r', '\0', ' ', 'a', 'F', '0', '9', '-', '_',
    'ä', 'ß', '€', '😀', '中', '#',
];

fn vorlagen() -> Vec<String> {
    vec![
        r##"{"type":"excalidraw","version":2,"source":"test","elements":[{"id":"a","type":"rectangle","x":0,"y":0,"width":100,"height":50,"strokeColor":"#1e1e1e","backgroundColor":"#ffffff","fillStyle":"solid","strokeWidth":2,"roughness":1,"opacity":100,"seed":1,"version":1,"versionNonce":1,"isDeleted":false,"groupIds":[],"boundElements":[],"updated":1,"link":null,"locked":false}],"appState":{},"files":{}}"##.to_string(),
        r#"{"type":"excalidraw","elements":[]}"#.to_string(),
        r##"{"elements":[{"id":"x","type":"text","x":"nan","y":null,"width":-1e400,"strokeColor":"#😀ab"}]}"##.to_string(),
        r#"{"type":"excalidraw","elements":"kein Array"}"#.to_string(),
        "#ff0000".to_string(),
        "😀ab".to_string(),
        String::new(),
    ]
}

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
            _ => zeichen.insert(stelle, BAUSTEINE[zufall.bis(BAUSTEINE.len())]),
        }
    }
    zeichen.into_iter().collect()
}

#[test]
fn zwanzigtausend_eingaben_beenden_keinen_leser() {
    let mut zufall = Zufall::neu(0x5EED_1234_ABCD_0002);
    let vorlagen = vorlagen();

    for runde in 0..20_000 {
        let vorlage = &vorlagen[zufall.bis(vorlagen.len())];
        let eingabe = veraendern(&mut zufall, vorlage);

        let ergebnis = catch_unwind(AssertUnwindSafe(|| {
            let _ = farben::farbe_aus_hex(&eingabe);
            let _ = farben::farbe_aus_name(&eingabe);
            if let Ok(wert) = serde_json::from_str::<serde_json::Value>(&eingabe) {
                let _ = pruefen::pruefen(&wert);
            }
            if let Ok(datei) = Datei::aus_text(&eingabe) {
                let _ = format!("{datei:?}");
                let _ = quiver_excalidraw::bild::bild(&datei);
            }
        }));

        assert!(
            ergebnis.is_ok(),
            "Runde {runde} hat einen Leser beendet; Eingabe: {eingabe:?}"
        );
    }
}
