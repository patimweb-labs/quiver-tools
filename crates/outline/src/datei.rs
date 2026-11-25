// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Dateinamen für `outline_export`. Reine Namensbildung – das Schreiben selbst
//! geht über `kern::Ablage` (feste Wurzel, keine Symlinks, kein Überschreiben).

/// Ein Titel als Dateiname: klein, ASCII, ohne Umlaute, höchstens 80 Zeichen.
///
/// Wikis sind voll von Seiten, die „Übersicht“ heißen; der Aufrufer hängt bei
/// Gleichstand selbst eine Kennung an.
pub fn slug(titel: &str) -> String {
    let mut raus = String::new();
    let mut trennung = true;
    for zeichen in titel.chars() {
        let ersatz = match zeichen {
            'ä' | 'Ä' => Some("ae"),
            'ö' | 'Ö' => Some("oe"),
            'ü' | 'Ü' => Some("ue"),
            'ß' => Some("ss"),
            _ => None,
        };
        match ersatz {
            Some(text) => {
                raus.push_str(text);
                trennung = false;
            }
            None if zeichen.is_ascii_alphanumeric() => {
                raus.push(zeichen.to_ascii_lowercase());
                trennung = false;
            }
            None => {
                if !trennung {
                    raus.push('-');
                    trennung = true;
                }
            }
        }
        if raus.len() >= 80 {
            break;
        }
    }
    let raus = raus.trim_matches('-');
    if raus.is_empty() {
        "dokument".to_string()
    } else {
        raus.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_macht_kleine_ascii_namen() {
        assert_eq!(slug("Betriebshandbuch"), "betriebshandbuch");
        assert_eq!(slug("Größe & Umlaute"), "groesse-umlaute");
        assert_eq!(slug("  Viele   Leerzeichen  "), "viele-leerzeichen");
        assert_eq!(slug("SAP/ABAP: Start"), "sap-abap-start");
        assert_eq!(slug(""), "dokument");
        assert_eq!(slug("???"), "dokument");
        assert_eq!(slug("Straße"), "strasse");
    }

    #[test]
    fn slug_bleibt_kurz() {
        let lang = slug(&"a".repeat(200));
        assert!(lang.len() <= 80, "{}", lang.len());
    }

    #[test]
    fn der_slug_ist_immer_ein_sicherer_name() {
        // Der einzige Weg zu einem Dateinamen ist `slug`: keine Trenner, keine
        // Punkte am Anfang, kein `..`, keine Steuerzeichen.
        for titel in [
            "../boese",
            "unter/strich",
            ".versteckt",
            "c:boese",
            "a\0b",
            "Größe & Umlaute",
        ] {
            let name = format!("{}.md", slug(titel));
            assert!(!name.contains('/'), "{name}");
            assert!(!name.contains('\\'), "{name}");
            assert!(!name.contains(':'), "{name}");
            assert!(!name.starts_with('.'), "{name}");
            assert!(!name.contains(".."), "{name}");
            assert!(!name.chars().any(char::is_control), "{name}");
        }
    }
}
