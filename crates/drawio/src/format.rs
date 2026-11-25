// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Reine Funktionen: Was der Agent als Antwort liest.
//!
//! Kurz, ohne Wiederholung der Beschreibung, mit dem, was er zum nächsten
//! Schritt braucht: Pfad, `stand`, Ids, Seiten. Keine Farben, keine Größen, kein
//! Zierrat – und nie ein Geheimnis (dieses Werkzeug hat keins).

use std::path::Path;

use quiver_diagramm::ablage::Eintrag;

use crate::mx;

/// Eine Dateigröße lesbar machen.
pub fn groesse(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0).replace('.', ",")
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0)).replace('.', ",")
    }
}

/// Antwort von `drawio_status`.
pub fn status(wurzel: &Path, dateien: usize, schreiben: bool) -> String {
    format!(
        "Arbeitsordner: {}\nEndung: .drawio · Dateien: {dateien}\nSchreiben: {} (nur im \
         Arbeitsordner, nie über Vorhandenes, nur mit `stand`)\nKeine Anmeldung, kein Netz: die \
         Dateien liegen lokal.",
        wurzel.display(),
        if schreiben { "an" } else { "aus" }
    )
}

/// Antwort von `drawio_liste`.
pub fn liste(wurzel: &Path, eintraege: &[Eintrag]) -> String {
    if eintraege.is_empty() {
        return format!(
            "Keine .drawio-Datei in {} – mit drawio_erstellen oder drawio_aus_quelltext anlegen.",
            wurzel.display()
        );
    }
    let mut text = format!("{} Datei(en) in {}:\n", eintraege.len(), wurzel.display());
    for eintrag in eintraege {
        text.push_str(&format!(
            "- {} – {}\n",
            eintrag.pfad,
            groesse(eintrag.groesse)
        ));
    }
    text.trim_end().to_string()
}

/// Antwort von `drawio_seiten`.
pub fn seiten(datei: &str, seiten: &[mx::Seite]) -> String {
    let mut text = format!("Datei „{datei}“ – {} Seite(n):\n", seiten.len());
    for seite in seiten {
        text.push_str(&format!("{}. {}", seite.index + 1, seiten_zeile(seite)));
        text.push('\n');
    }
    text.push_str("`seite` nimmt die Id oder den Namen.");
    text
}

fn seiten_zeile(seite: &mx::Seite) -> String {
    let name = if seite.name.trim().is_empty() {
        "(ohne Namen)"
    } else {
        seite.name.as_str()
    };
    let mut text = format!(
        "„{name}“ (id {}) – {} Zellen",
        if seite.id.trim().is_empty() {
            "—"
        } else {
            seite.id.as_str()
        },
        seite.zellen
    );
    if seite.komprimiert {
        text.push_str(&format!(
            ", komprimiert ({} Zeichen gepackt)",
            seite.gespeichert
        ));
    } else {
        text.push_str(", unkomprimiert");
    }
    text
}

/// Antwort von `drawio_lesen` (knappe Beschreibung mit Ids und `stand`).
pub fn beschreibung(
    datei: &str,
    stand: &str,
    seite: &mx::Seite,
    anzahl_seiten: usize,
    zellen: &[mx::Zelle],
    max: usize,
) -> String {
    let mut text = format!("Datei „{datei}“ · stand: {stand}\n");
    text.push_str(&format!(
        "Seite {} von {} {} · {}\n",
        seite.index + 1,
        anzahl_seiten,
        seiten_zeile(seite),
        if zellen.is_empty() {
            "keine Zellen außer 0 und 1".to_string()
        } else {
            format!("{} Zelle(n)", zellen.len())
        }
    ));
    for zelle in zellen.iter().take(max) {
        text.push_str(&zell_zeile(zelle));
        text.push('\n');
    }
    if zellen.len() > max {
        text.push_str(&format!(
            "… und {} weitere Zellen (mit `roh: true` steht der Quelltext; mit `seite` eine \
             andere Seite)\n",
            zellen.len() - max
        ));
    }
    text.push_str("Ids für `_elemente_aendern`/`_elemente_entfernen` stehen vorn.");
    text
}

fn zell_zeile(zelle: &mx::Zelle) -> String {
    let mut text = format!("- {} · ", zelle.id);
    if zelle.kante {
        text.push_str("Kante");
        match (&zelle.quelle, &zelle.ziel) {
            (Some(quelle), Some(ziel)) => text.push_str(&format!(" · {quelle} → {ziel}")),
            (Some(quelle), None) => text.push_str(&format!(" · {quelle} → ?")),
            (None, Some(ziel)) => text.push_str(&format!(" · ? → {ziel}")),
            (None, None) => text.push_str(" · ohne Enden"),
        }
    } else {
        text.push_str("Knoten");
        if let Some(form) = zelle.form {
            text.push_str(&format!(" · {}", form.name()));
        } else if let Some(fremd) = &zelle.fremde_form {
            text.push_str(&format!(" · Form „{fremd}“"));
        }
        if let Some(eltern) = &zelle.eltern {
            if eltern != mx::EBENE_ID {
                text.push_str(&format!(" · in „{eltern}“"));
            }
        }
    }
    text.push_str(&format!(" · {}", kurz(&zelle.text)));
    text
}

fn kurz(text: &str) -> String {
    let text = text.replace(['\n', '\r'], " ");
    let text = text.trim();
    if text.is_empty() {
        return "(ohne Text)".to_string();
    }
    let zeichen: Vec<char> = text.chars().collect();
    if zeichen.len() <= 80 {
        format!("„{text}“")
    } else {
        let gekuerzt: String = zeichen.into_iter().take(80).collect();
        format!("„{gekuerzt}…“")
    }
}

/// Antwort, wenn eine Datei angelegt wurde.
pub fn erstellt(datei: &str, stand: &str, seiten: usize, zellen: usize) -> String {
    format!(
        "„{datei}“ angelegt ({seiten} Seite(n), {zellen} Zelle(n)).\nstand: {stand}\nMit \
         drawio_bild ansehen oder mit drawio_oeffnen im Browser öffnen."
    )
}

/// Antwort, wenn eine Datei geändert wurde.
pub fn geaendert(datei: &str, stand: &str, anzahl: usize, was: &str) -> String {
    let sicherung = sicherung(datei);
    format!("„{datei}“ geändert: {anzahl} {was}. Sicherung: {sicherung}.\nstand: {stand}")
}

fn sicherung(datei: &str) -> String {
    format!("{datei}.bak")
}

/// Antwort, wenn eine Seite angehängt wurde.
pub fn seite_angehaengt(datei: &str, seitenname: &str, seiten: usize, stand: &str) -> String {
    format!(
        "„{datei}“ geändert: Seite „{seitenname}“ angehängt (jetzt {seiten} Seiten). Sicherung: \
         {}.\nstand: {stand}",
        sicherung(datei)
    )
}

/// Antwort, wenn etwas geschrieben wurde (Export).
pub fn exportiert(datei: &str, ausgabe: &str, bytes: usize, hinweis: &str) -> String {
    format!(
        "„{ausgabe}“ geschrieben ({}) aus „{datei}“ – vorhandene Dateien werden nie \
         überschrieben.\n{hinweis}",
        groesse(bytes as u64)
    )
}

/// Antwort, wenn `schreiben` aus ist.
pub fn schreiben_aus() -> String {
    "Schreiben ist aus – bitte im quiver-Fenster unter Tools → draw.io den Schalter \
     „schreiben“ einschalten. Gelesen, gezeichnet und verlinkt werden kann weiterhin."
        .to_string()
}

/// Antwort von `drawio_link`: was der Link ist und was er nicht tut.
pub fn link_hinweis(seiten: usize) -> String {
    format!(
        "Der Link enthält das ganze Diagramm ({seiten} Seite(n)) im Fragment der Adresse. Dieses \
         Fragment geht nie an einen Server – draw.io lädt nichts hoch, es braucht keine \
         Anmeldung. Mit drawio_oeffnen öffnet ihn der Standardbrowser."
    )
}

/// Antwort von `drawio_oeffnen`.
pub fn oeffnen(wurzel: &Path, datei: &str) -> String {
    let pfad = wurzel.join(datei);
    format!(
        "draw.io ist im Standardbrowser offen – der Link trägt das ganze Diagramm im Fragment.\n\
         Wichtig: Änderungen im Browser landen NICHT in der lokalen Datei. Dort „Datei → \
         Speichern unter“ wählen und die Datei zurück in den Arbeitsordner legen ({}).",
        pfad.display()
    )
}

/// Antwort, wenn der Link für den Startbefehl des Systems zu lang ist.
pub fn oeffnen_zu_lang(pfad: &Path, laenge: usize, max: usize) -> String {
    format!(
        "Der Link ist {laenge} Zeichen lang – zu lang für den Startbefehl des Betriebssystems \
         (höchstens {max}). Geöffnet wurde nichts.\nBitte die Datei direkt in draw.io öffnen \
         (Datei → Öffnen): {}",
        pfad.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seite(index: usize, name: &str, komprimiert: bool) -> mx::Seite {
        mx::Seite {
            index,
            id: format!("s{}", index + 1),
            name: name.to_string(),
            komprimiert,
            gespeichert: if komprimiert { 40 } else { 0 },
            zellen: 4,
        }
    }

    #[test]
    fn groessen_werden_lesbar() {
        assert_eq!(groesse(12), "12 B");
        assert_eq!(groesse(2048), "2,0 KB");
        assert_eq!(groesse(3 * 1024 * 1024), "3,0 MB");
    }

    #[test]
    fn die_liste_ist_leer_oder_zaehlt() {
        let leer = liste(Path::new("/tmp/x"), &[]);
        assert!(leer.contains("Keine .drawio-Datei"), "{leer}");
        let eintraege = vec![Eintrag {
            pfad: "a.drawio".to_string(),
            groesse: 21,
            geaendert: None,
        }];
        let text = liste(Path::new("/tmp/x"), &eintraege);
        assert!(text.contains("a.drawio – 21 B"), "{text}");
    }

    #[test]
    fn seiten_nennen_id_name_und_zustand() {
        let text = seiten("a.drawio", &[seite(0, "Start", false), seite(1, "", true)]);
        assert!(text.contains("1. „Start“ (id s1)"), "{text}");
        assert!(text.contains("unkomprimiert"), "{text}");
        assert!(text.contains("2. „(ohne Namen)“"), "{text}");
        assert!(text.contains("komprimiert"), "{text}");
    }

    #[test]
    fn die_beschreibung_ist_knapp_und_nennt_den_stand() {
        let zellen = vec![
            mx::Zelle {
                id: "a".to_string(),
                text: "Antrag".to_string(),
                kante: false,
                quelle: None,
                ziel: None,
                eltern: Some("1".to_string()),
                form: Some(crate::formen::Form::Raute),
                fremde_form: None,
            },
            mx::Zelle {
                id: "k".to_string(),
                text: "ja".to_string(),
                kante: true,
                quelle: Some("a".to_string()),
                ziel: Some("b".to_string()),
                eltern: Some("1".to_string()),
                form: None,
                fremde_form: None,
            },
        ];
        let text = beschreibung("a.drawio", "abc", &seite(0, "Start", false), 2, &zellen, 20);
        assert!(text.contains("stand: abc"), "{text}");
        assert!(text.contains("Seite 1 von 2"), "{text}");
        assert!(text.contains("- a · Knoten · raute · „Antrag“"), "{text}");
        assert!(text.contains("- k · Kante · a → b · „ja“"), "{text}");
    }

    #[test]
    fn eine_lange_beschreibung_wird_gekuerzt() {
        let zellen: Vec<mx::Zelle> = (0..5)
            .map(|i| mx::Zelle {
                id: format!("n{i}"),
                text: "x".to_string(),
                kante: false,
                quelle: None,
                ziel: None,
                eltern: None,
                form: Some(crate::formen::Form::Rechteck),
                fremde_form: None,
            })
            .collect();
        let text = beschreibung("a.drawio", "abc", &seite(0, "Start", false), 1, &zellen, 3);
        assert!(text.contains("2 weitere Zellen"), "{text}");
    }

    #[test]
    fn der_zu_lange_link_ist_eine_meldung_mit_pfad() {
        let text = oeffnen_zu_lang(Path::new("/tmp/a.drawio"), 5000, 2000);
        assert!(text.contains("5000 Zeichen"), "{text}");
        assert!(text.contains("/tmp/a.drawio"), "{text}");
        assert!(text.contains("Geöffnet wurde nichts"), "{text}");
    }

    #[test]
    fn das_oeffnen_sagt_dass_der_browser_nicht_in_die_datei_schreibt() {
        let text = oeffnen(Path::new("/tmp"), "a.drawio");
        assert!(text.contains("NICHT in der lokalen Datei"), "{text}");
        assert!(text.contains("/tmp/a.drawio"), "{text}");
    }
}
