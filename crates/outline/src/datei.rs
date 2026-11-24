// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Markdown auf die Platte (für `outline_export` und `outline_read` mit
//! `save_dir`). Reine Namensbildung, ein Schreibvorgang – und die Zusicherung,
//! dass ein Dateiname nicht aus dem Zielordner ausbricht.

use std::path::{Path, PathBuf};

/// Was beim Schreiben schiefgehen kann.
#[derive(Debug, thiserror::Error)]
pub enum DateiFehler {
    #[error("Der Dateiname „{0}“ ist nicht zulässig.")]
    UnsichererName(String),
    #[error("{0} lässt sich nicht schreiben: {1}")]
    Schreiben(PathBuf, String),
}

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

/// `inhalt` als `dateiname` in `ordner` schreiben; gibt den Pfad zurück.
///
/// Der Ordnername darf alles sein (er kommt vom User), der **Dateiname** nicht:
/// er darf keine Trenner, keine Punkte am Anfang und kein `..` enthalten.
pub fn schreiben(ordner: &Path, dateiname: &str, inhalt: &str) -> Result<PathBuf, DateiFehler> {
    let unsicher = dateiname.is_empty()
        || dateiname.starts_with('.')
        || dateiname.contains(['/', '\\', ':'])
        || dateiname.contains("..");
    if unsicher {
        return Err(DateiFehler::UnsichererName(dateiname.to_string()));
    }

    std::fs::create_dir_all(ordner)
        .map_err(|fehler| DateiFehler::Schreiben(ordner.to_path_buf(), fehler.to_string()))?;
    let ziel = ordner.join(dateiname);
    std::fs::write(&ziel, inhalt)
        .map_err(|fehler| DateiFehler::Schreiben(ziel.clone(), fehler.to_string()))?;
    Ok(ziel)
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
    fn schreiben_legt_den_ordner_an() {
        let ordner = tempfile::tempdir().unwrap();
        let ziel = ordner.path().join("docs/wiki");
        let pfad = schreiben(&ziel, "seite.md", "# Titel\n").unwrap();
        assert_eq!(pfad, ziel.join("seite.md"));
        assert_eq!(std::fs::read_to_string(&pfad).unwrap(), "# Titel\n");
    }

    #[test]
    fn unsichere_namen_werden_abgewiesen() {
        let ordner = tempfile::tempdir().unwrap();
        for name in [
            "",
            "../boese.md",
            "unter/strich.md",
            ".versteckt",
            "c:boese.md",
        ] {
            assert!(
                matches!(
                    schreiben(ordner.path(), name, "x"),
                    Err(DateiFehler::UnsichererName(_))
                ),
                "{name}"
            );
        }
    }
}
