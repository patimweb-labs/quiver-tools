// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Reine Funktionen: Daten → Text für den Agenten.
//!
//! Kurze Antworten (SPEC §2): was da ist, welche Ids, welcher `stand`, und was
//! als Nächstes zu tun ist. Kein JSON-Dump, außer der Agent fragt mit
//! `roh: true` ausdrücklich danach.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use quiver_diagramm::ablage::{Ablage, Inhalt};

use crate::datei::{Datei, Element, ENDUNG};
use crate::fehler::Fehler;

/// So viele Zeichen zeigt `_lesen` mit `roh: true` höchstens.
pub const MAX_ROH: usize = 20_000;

/// Kurze Antwort, wenn der Arbeitsordner leer ist.
pub fn keine_dateien() -> String {
    format!(
        "Im Arbeitsordner liegt noch keine *.{ENDUNG}-Datei. Mit `excalidraw_erstellen` entsteht die erste."
    )
}

/// Antwort von `excalidraw_status`.
pub fn status(ordner: &Path, schreiben: bool, anzahl: usize) -> String {
    let schalter = if schreiben { "an" } else { "aus" };
    format!(
        "Excalidraw-Arbeitsordner: {}\n{anzahl} Datei(en) mit .{ENDUNG}, Schreiben ist {schalter}.",
        ordner.display()
    )
}

/// Antwort von `excalidraw_liste`.
pub fn liste(ablage: &Ablage) -> Result<String, Fehler> {
    let suche = ablage.suche()?;
    let eintraege = &suche.eintraege;
    if eintraege.is_empty() {
        return Ok(quiver_diagramm::ablage::mit_hinweis(
            keine_dateien(),
            &suche,
        ));
    }
    let mut text = format!(
        "{} Datei(en) in {}:",
        eintraege.len(),
        ablage.wurzel().display()
    );
    for eintrag in eintraege {
        text.push_str(&format!(
            "\n- {} ({}, {})",
            eintrag.pfad,
            groesse(eintrag.groesse),
            zeitpunkt(eintrag.geaendert)
        ));
    }
    Ok(quiver_diagramm::ablage::mit_hinweis(text, &suche))
}

/// Antwort von `excalidraw_lesen`.
pub fn lesen(pfad: &str, inhalt: &Inhalt, datei: &Datei, ordner: &Path) -> String {
    let mut text = format!(
        "`{pfad}` – {}, {}\nVoller Pfad: {}\nStand: {}",
        anzahl_text(datei),
        groesse(inhalt.groesse),
        ordner.join(pfad).display(),
        inhalt.stand
    );
    let beschreibung = beschreibung(datei);
    if !beschreibung.is_empty() {
        text.push_str("\n\n");
        text.push_str(&beschreibung);
    }
    text.push_str("\n\nÄndern: `excalidraw_elemente_aendern` mit `datei` und diesem `stand`.");
    text
}

/// Der Quelltext einer Datei, gekürzt mit Hinweis.
pub fn roh(text: &str, max_zeichen: usize) -> String {
    let (gekuerzt, weggelassen) = quiver_tools_kern::kuerzen(text, max_zeichen);
    match weggelassen {
        Some(anzahl) => format!(
            "{gekuerzt}\n\n[… {anzahl} Zeichen weggelassen; mit höherem `max_zeichen` mehr]"
        ),
        None => gekuerzt,
    }
}

/// Eine knappe Beschreibung aller sichtbaren Elemente.
pub fn beschreibung(datei: &Datei) -> String {
    let sichtbar: Vec<&Element> = datei.sichtbare().collect();
    if sichtbar.is_empty() {
        return "Keine Elemente.".to_string();
    }
    let mut text = String::new();
    for (nummer, element) in sichtbar.iter().enumerate() {
        if nummer > 0 {
            text.push('\n');
        }
        text.push_str(&zeile(element));
    }
    text
}

fn zeile(element: &Element) -> String {
    let mut text = format!("- „{}“ {}", element.id, element.art);
    if element.ist_linie() {
        match (element.start(), element.schluss()) {
            (Some(von), Some(nach)) => text.push_str(&format!(" von „{von}“ nach „{nach}“")),
            (Some(von), None) => text.push_str(&format!(" ab „{von}“ (offen)")),
            (None, Some(nach)) => text.push_str(&format!(" nach „{nach}“ (offen)")),
            (None, None) => text.push_str(" (ohne Bindung)"),
        }
    } else {
        text.push_str(&format!(
            " {}, {} · {}×{}",
            zahl(element.x),
            zahl(element.y),
            zahl(element.width),
            zahl(element.height)
        ));
    }
    if let Some(name) = element
        .name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
    {
        text.push_str(&format!(" – „{name}“"));
    }
    if let Some(inhalt) = element
        .text
        .as_deref()
        .filter(|inhalt| !inhalt.trim().is_empty())
    {
        text.push_str(&format!(" – „{}“", kurz(inhalt, 80)));
    }
    if let Some(behaelter) = element.container() {
        text.push_str(&format!(" (Text in „{behaelter}“)"));
    }
    if let Some(rahmen) = element.frame_id.as_deref() {
        text.push_str(&format!(" (Rahmen „{rahmen}“)"));
    }
    text
}

/// „3 Elemente“ oder „1 Element“.
pub fn anzahl_text(datei: &Datei) -> String {
    let anzahl = datei.anzahl();
    if anzahl == 1 {
        "1 Element".to_string()
    } else {
        format!("{anzahl} Elemente")
    }
}

/// Was `excalidraw_bild` über das Bild sagt.
pub fn bild(bild: &crate::bild::Bild, max_kante: u32) -> String {
    let (breite, hoehe) = bild.pixel(max_kante);
    let mut text = format!(
        "Bild aus {} gezeichnet ({}×{} px).",
        elemente_text(bild.gezeichnet),
        breite,
        hoehe
    );
    if !bild.nicht_gezeichnet.is_empty() {
        text.push_str("\nNicht gezeichnet (kennt diese Kiste nicht): ");
        text.push_str(&bild.nicht_gezeichnet.join(", "));
    }
    text
}

fn elemente_text(anzahl: usize) -> String {
    if anzahl == 1 {
        "1 Element".to_string()
    } else {
        format!("{anzahl} Elementen")
    }
}

/// Die Antwort, wenn der Schalter „schreiben“ aus ist.
pub fn schreiben_aus(titel: &str) -> String {
    format!(
        "Der Schalter „schreiben“ ist aus – im quiver-Fenster unter Tools → {titel} einschalten.\n\
         Es wurde nichts geändert und nichts geschrieben."
    )
}

/// Ein Text, gekürzt auf `max` Zeichen (eine Zeile).
fn kurz(text: &str, max: usize) -> String {
    let einzeilig = text.replace('\n', " ");
    if einzeilig.chars().count() <= max {
        return einzeilig;
    }
    let gekuerzt: String = einzeilig.chars().take(max).collect();
    format!("{gekuerzt}…")
}

/// Eine Zahl ohne unnötige Nachkommastellen.
fn zahl(wert: f64) -> String {
    if !wert.is_finite() {
        return "0".to_string();
    }
    let gerundet = (wert * 10.0).round() / 10.0;
    if (gerundet - gerundet.round()).abs() < f64::EPSILON {
        format!("{}", gerundet.round() as i64)
    } else {
        format!("{gerundet:.1}")
    }
}

/// Eine Dateigröße lesbar: Bytes unter 1024, sonst Kilobyte.
pub fn groesse(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} Bytes")
    } else {
        format!("{:.1} kB", bytes as f64 / 1024.0)
    }
}

/// Einen Zeitpunkt als `JJJJ-MM-TT hh:mm UTC`.
pub fn zeitpunkt(zeit: Option<SystemTime>) -> String {
    let Some(zeit) = zeit else {
        return "Zeit unbekannt".to_string();
    };
    let sekunden = zeit
        .duration_since(UNIX_EPOCH)
        .map(|dauer| dauer.as_secs() as i64)
        .unwrap_or(0);
    let tage = sekunden.div_euclid(86_400);
    let rest = sekunden.rem_euclid(86_400);
    let (jahr, monat, tag) = kalendertag(tage);
    format!(
        "{jahr:04}-{monat:02}-{tag:02} {:02}:{:02} UTC",
        rest / 3600,
        (rest % 3600) / 60
    )
}

/// Kalendertag aus Tagen seit 1970 (Howard Hinnant, umgekehrt gerechnet).
fn kalendertag(tage: i64) -> (i64, u32, u32) {
    let z = tage + 719_468;
    let aera = z.div_euclid(146_097);
    let jahrestag = z.rem_euclid(146_097);
    let jahr_der_aera =
        (jahrestag - jahrestag / 1_460 + jahrestag / 36_524 - jahrestag / 146_096) / 365;
    let jahr = jahr_der_aera + aera * 400;
    let tag_im_jahr = jahrestag - (365 * jahr_der_aera + jahr_der_aera / 4 - jahr_der_aera / 100);
    let monatsteil = (5 * tag_im_jahr + 2) / 153;
    let tag = (tag_im_jahr - (153 * monatsteil + 2) / 5 + 1) as u32;
    let monat = if monatsteil < 10 {
        monatsteil + 3
    } else {
        monatsteil - 9
    } as u32;
    (if monat <= 2 { jahr + 1 } else { jahr }, monat, tag)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bauen::Vorlage;
    use std::time::Duration;

    #[test]
    fn groessen_werden_lesbar() {
        assert_eq!(groesse(12), "12 Bytes");
        assert_eq!(groesse(2048), "2.0 kB");
    }

    #[test]
    fn der_kalendertag_stimmt() {
        assert_eq!(kalendertag(0), (1970, 1, 1));
        // 1_700_000_000 Sekunden sind der 14. November 2023, 22:13:20 UTC.
        assert_eq!(kalendertag(1_700_000_000 / 86_400), (2023, 11, 14));
        assert_eq!(
            zeitpunkt(Some(UNIX_EPOCH + Duration::from_secs(1_700_000_000))),
            "2023-11-14 22:13 UTC"
        );
        assert_eq!(zeitpunkt(None), "Zeit unbekannt");
    }

    #[test]
    fn eine_beschreibung_nennt_ids_arten_und_bindungen() {
        let mut datei = Datei::neu();
        crate::bauen::hinzufuegen(
            &mut datei,
            &Vorlage {
                art: "rectangle".to_string(),
                id: Some("form".to_string()),
                text: Some("Hallo".to_string()),
                ..Vorlage::default()
            },
        )
        .unwrap();
        crate::bauen::hinzufuegen(
            &mut datei,
            &Vorlage {
                art: "arrow".to_string(),
                von: Some("form".to_string()),
                nach: Some("form".to_string()),
                ..Vorlage::default()
            },
        )
        .unwrap_err();

        let text = beschreibung(&datei);
        assert!(text.contains("„form“ rectangle"), "{text}");
        assert!(text.contains("Hallo"), "{text}");
        assert_eq!(anzahl_text(&datei), "2 Elemente");
    }

    #[test]
    fn ein_pfeil_wird_mit_seinen_enden_beschrieben() {
        let mut datei = Datei::neu();
        for id in ["a", "b"] {
            crate::bauen::hinzufuegen(
                &mut datei,
                &Vorlage {
                    art: "rectangle".to_string(),
                    id: Some(id.to_string()),
                    ..Vorlage::default()
                },
            )
            .unwrap();
        }
        crate::aendern::verbinden(
            &mut datei,
            "a",
            "b",
            quiver_diagramm::modell::KanteArt::Pfeil,
            None,
        )
        .unwrap();
        let text = beschreibung(&datei);
        assert!(
            text.contains("von „a“ nach „b“"),
            "Pfeil nicht beschrieben: {text}"
        );
    }

    #[test]
    fn roh_kuerzt_mit_hinweis() {
        let text = "a".repeat(100);
        let gekuerzt = roh(&text, 10);
        assert!(gekuerzt.starts_with("aaaaaaaaaa"));
        assert!(gekuerzt.contains("90 Zeichen weggelassen"), "{gekuerzt}");
        assert_eq!(roh("kurz", 10), "kurz");
    }

    #[test]
    fn ein_leerer_ordner_sagt_was_zu_tun_ist() {
        assert!(keine_dateien().contains("excalidraw_erstellen"));
        assert!(schreiben_aus("Excalidraw").contains("nichts geschrieben"));
    }
}
