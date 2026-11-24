// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Abnahme: was ein Agent nacheinander tut.

mod common;

use quiver_diagramm::ablage::Ablage;

#[test]
fn in_einen_vorhandenen_unterordner_laesst_sich_schreiben() {
    let ordner = tempfile::tempdir().unwrap();
    let ablage = Ablage::neu(ordner.path().to_str().unwrap(), "drawio").unwrap();
    ablage.anlegen("projekt/a.drawio", "eins").unwrap();
    // Der Unterordner gibt es jetzt – die zweite Datei muss trotzdem gehen.
    ablage.anlegen("projekt/b.drawio", "zwei").unwrap();
    let stand = ablage.stand("projekt/a.drawio").unwrap();
    ablage.aendern("projekt/a.drawio", "drei", &stand).unwrap();
    assert_eq!(ablage.lesen("projekt/a.drawio").unwrap().text, "drei");
    ablage
        .ausgabe_schreiben("projekt/a.svg", b"<svg/>")
        .unwrap();
    assert_eq!(ablage.liste().unwrap().len(), 2);
}

#[test]
fn ein_symlink_als_unterordner_wird_weiter_abgelehnt() {
    #[cfg(unix)]
    {
        let ordner = tempfile::tempdir().unwrap();
        let fremd = tempfile::tempdir().unwrap();
        let ablage = Ablage::neu(ordner.path().to_str().unwrap(), "drawio").unwrap();
        std::os::unix::fs::symlink(fremd.path(), ablage.wurzel().join("link")).unwrap();
        assert!(ablage.anlegen("link/a.drawio", "x").is_err());
        assert!(ablage.anlegen("link/tiefer/a.drawio", "x").is_err());
        assert_eq!(std::fs::read_dir(fremd.path()).unwrap().count(), 0);
    }
}

/// Beispielbild für die Abnahme: `TMP_BILD=<ordner>`.
#[test]
fn beispielbild() {
    let Ok(ziel) = std::env::var("TMP_BILD") else {
        return;
    };
    for (name, text) in [
        (
            "ablauf",
            r#"{"richtung":"unten","knoten":[{"id":"a","text":"Bestellung eingegangen"},{"id":"b","text":"Bezahlt?","form":"raute"},{"id":"c","text":"Ware versenden","farbe":"gruen"},{"id":"d","text":"Erinnerung senden","farbe":"gelb"},{"id":"e","text":"Kundendaten","form":"zylinder"},{"id":"f","text":"Fertig","form":"ellipse"}],"kanten":[{"von":"a","nach":"b"},{"von":"b","nach":"c","text":"ja"},{"von":"b","nach":"d","text":"nein"},{"von":"d","nach":"b","art":"gestrichelt"},{"von":"c","nach":"f"},{"von":"a","nach":"e"}],"gruppen":[]}"#,
        ),
        (
            "gruppen",
            r#"{"richtung":"rechts","knoten":[{"id":"pi","text":"pi","gruppe":"ag"},{"id":"he","text":"Hermes","gruppe":"ag"},{"id":"cl","text":"quiver-Client","gruppe":"ge"},{"id":"m","text":"quiver-m365","gruppe":"ge"},{"id":"z","text":"quiver-zep","gruppe":"ge"},{"id":"s","text":"quiver-Server (Bibliothek)"},{"id":"g","text":"Microsoft Graph"},{"id":"ze","text":"ZEP"}],"kanten":[{"von":"pi","nach":"cl"},{"von":"he","nach":"cl"},{"von":"cl","nach":"m"},{"von":"cl","nach":"z"},{"von":"cl","nach":"s","text":"Abgleich"},{"von":"m","nach":"g"},{"von":"z","nach":"ze"}],"gruppen":[{"id":"ag","titel":"Agenten"},{"id":"ge","titel":"Gerät"}]}"#,
        ),
    ] {
        let d = quiver_diagramm::Diagramm::aus_json(text).unwrap();
        let png = quiver_diagramm::Szene::aus_zeichnung(&quiver_diagramm::layout(&d).unwrap())
            .png(1200)
            .unwrap();
        std::fs::write(format!("{ziel}/{name}.png"), png).unwrap();
    }
}

/// Beispielbilder für die Abnahme: `TMP_BILD=<ordner>`.
#[test]
fn weitere_beispielbilder() {
    let Ok(ziel) = std::env::var("TMP_BILD") else {
        return;
    };
    for (name, diagramm) in [
        ("weit", common::modell_weit()),
        ("kreis", common::modell_kreis()),
    ] {
        diagramm.pruefen().expect("Modell ist gültig");
        let zeichnung = quiver_diagramm::layout(&diagramm).expect("Layout");
        let png = quiver_diagramm::Szene::aus_zeichnung(&zeichnung)
            .png(1200)
            .unwrap();
        std::fs::write(format!("{ziel}/{name}.png"), png).unwrap();
    }
}
