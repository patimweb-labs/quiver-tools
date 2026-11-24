// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Tests des Layouts: Überlappungsfreiheit über viele zufällige Graphen (fester
//! Startwert), Gleichheit bei gleicher Eingabe, Gruppen und Kantenführung.

use quiver_diagramm::layout::{layout, Zeichnung, ABSTAND_GLEICH, SCHRIFTGROESSE, ZEILENHOEHE};
use quiver_diagramm::modell::{Diagramm, Farbe, Form, Gruppe, Kante, KanteArt, Knoten, Richtung};
use quiver_diagramm::zeichnen::{schrift, Rechteck};

/// Einfacher, fest gesäter Zufall – kein Zusatz-Crate, überall gleiche Zahlen.
struct Zufall(u64);

impl Zufall {
    fn neu(start: u64) -> Zufall {
        Zufall(start)
    }

    fn zahl(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    fn bis(&mut self, grenze: usize) -> usize {
        (self.zahl() % grenze as u64) as usize
    }

    fn wahl<T: Copy>(&mut self, werte: &[T]) -> T {
        werte[self.bis(werte.len())]
    }
}

fn knoten_rechteck(platz: &quiver_diagramm::layout::KnotenPlatz) -> Rechteck {
    Rechteck::neu(platz.x, platz.y, platz.breite, platz.hoehe)
}

fn ueberlappt(a: &Rechteck, b: &Rechteck) -> bool {
    let links = a.x.max(b.x);
    let oben = a.y.max(b.y);
    let rechts = (a.x + a.breite).min(b.x + b.breite);
    let unten = (a.y + a.hoehe).min(b.y + b.hoehe);
    rechts - links > 0.01 && unten - oben > 0.01
}

fn enthaelt(aussen: &Rechteck, innen: &Rechteck) -> bool {
    innen.x >= aussen.x - 0.01
        && innen.y >= aussen.y - 0.01
        && innen.x + innen.breite <= aussen.x + aussen.breite + 0.01
        && innen.y + innen.hoehe <= aussen.y + aussen.hoehe + 0.01
}

fn zufallsdiagramm(zufall: &mut Zufall) -> Diagramm {
    let anzahl = 1 + zufall.bis(24);
    let texte = [
        "Start",
        "Prüfung & Freigabe",
        "sehr langer Text mit vielen Wörtern, der umgebrochen werden muss",
        "Donaudampfschifffahrtsgesellschaftskapitän",
        "zweizeilig\nmit Umbruch",
        "",
    ];
    let mut diagramm = Diagramm::leer();
    diagramm.richtung = zufall.wahl(&[Richtung::Unten, Richtung::Rechts]);
    diagramm.knoten = (0..anzahl)
        .map(|i| Knoten {
            form: zufall.wahl(&Form::ALLE),
            farbe: if zufall.bis(3) == 0 {
                Some(zufall.wahl(&Farbe::ALLE))
            } else {
                None
            },
            gruppe: None,
            ..Knoten::neu(format!("k{i}"), zufall.wahl(&texte))
        })
        .collect();

    let gruppen = zufall.bis(3);
    for g in 0..gruppen {
        let id = format!("g{g}");
        diagramm
            .gruppen
            .push(Gruppe::neu(&id, format!("Gruppe {g}")));
        for (i, knoten) in diagramm.knoten.iter_mut().enumerate() {
            if zufall.bis(4) == 0 || (g == 0 && i == 0) {
                knoten.gruppe = Some(id.clone());
            }
        }
    }

    let kanten = zufall.bis(anzahl * 2);
    for _ in 0..kanten {
        let von = zufall.bis(anzahl);
        let nach = zufall.bis(anzahl);
        if von == nach {
            continue;
        }
        diagramm.kanten.push(Kante {
            text: if zufall.bis(3) == 0 {
                Some(format!("Kante {}", zufall.bis(9)))
            } else {
                None
            },
            art: zufall.wahl(&KanteArt::ALLE),
            ..Kante::neu(format!("k{von}"), format!("k{nach}"))
        });
    }
    diagramm
}

fn pruefe_zeichnung(diagramm: &Diagramm, zeichnung: &Zeichnung) {
    assert_eq!(zeichnung.knoten.len(), diagramm.knoten.len());
    assert_eq!(zeichnung.kanten.len(), diagramm.kanten.len());

    // Gleiche Reihenfolge wie im Modell.
    for (platz, knoten) in zeichnung.knoten.iter().zip(&diagramm.knoten) {
        assert_eq!(platz.id, knoten.id);
        assert_eq!(platz.form, knoten.form);
        assert_eq!(platz.farbe, knoten.farbe);
        assert_eq!(platz.gruppe, knoten.gruppe);
    }

    // Keine zwei Knoten überlappen sich.
    for (i, a) in zeichnung.knoten.iter().enumerate() {
        for b in &zeichnung.knoten[i + 1..] {
            assert!(
                !ueberlappt(&knoten_rechteck(a), &knoten_rechteck(b)),
                "{} und {} überlappen sich: {:?} / {:?}",
                a.id,
                b.id,
                knoten_rechteck(a),
                knoten_rechteck(b)
            );
        }
    }
    // Alles liegt auf der Fläche und die Fläche ist endlich.
    assert!(zeichnung.breite.is_finite() && zeichnung.hoehe.is_finite());
    for platz in &zeichnung.knoten {
        let r = knoten_rechteck(platz);
        assert!(r.x >= -0.01 && r.y >= -0.01, "{r:?} liegt außerhalb");
        assert!(
            r.x + r.breite <= zeichnung.breite + 0.01 && r.y + r.hoehe <= zeichnung.hoehe + 0.01,
            "{r:?} ragt aus {}×{} heraus",
            zeichnung.breite,
            zeichnung.hoehe
        );
        // Der Text passt in den Knoten.
        for zeile in &platz.zeilen {
            assert!(
                schrift::breite(zeile, SCHRIFTGROESSE) <= platz.breite + 0.01,
                "Zeile „{zeile}“ ist breiter als {}",
                platz.breite
            );
        }
        assert!(
            platz.zeilen.len() as f32 * ZEILENHOEHE <= platz.hoehe + 0.01,
            "Text ist höher als {}",
            platz.hoehe
        );
    }

    // Gruppen umschließen ihre Knoten.
    for rahmen in &zeichnung.gruppen {
        let kasten = Rechteck::neu(rahmen.x, rahmen.y, rahmen.breite, rahmen.hoehe);
        assert!(kasten.breite > 0.0 && kasten.hoehe > 0.0);
        for platz in &zeichnung.knoten {
            if platz.gruppe.as_deref() == Some(rahmen.id.as_str()) {
                assert!(
                    enthaelt(&kasten, &knoten_rechteck(platz)),
                    "{} liegt nicht in Rahmen {}",
                    platz.id,
                    rahmen.id
                );
            }
        }
    }

    // Kanten haben einen Weg, und kein Weg läuft durch einen Knoten.
    for lauf in &zeichnung.kanten {
        assert!(lauf.punkte.len() >= 2, "Kante ohne Weg: {lauf:?}");
        for punkt in &lauf.punkte {
            assert!(punkt.x.is_finite() && punkt.y.is_finite());
            for platz in &zeichnung.knoten {
                let r = knoten_rechteck(platz);
                let innen = punkt.x > r.x + 0.5
                    && punkt.x < r.x + r.breite - 0.5
                    && punkt.y > r.y + 0.5
                    && punkt.y < r.y + r.hoehe - 0.5;
                assert!(
                    !innen,
                    "Kante {}→{} läuft durch {}: {punkt:?}",
                    lauf.von, lauf.nach, platz.id
                );
            }
        }
    }
}

#[test]
fn zufallsgraphen_ohne_ueberlappung_und_ohne_zufall() {
    let mut zufall = Zufall::neu(20_261_116);
    for runde in 0..80 {
        let diagramm = zufallsdiagramm(&mut zufall);
        diagramm.pruefen().expect("Zufallsmodell ist gültig");
        let einmal = layout(&diagramm).expect("Layout");
        let nochmal = layout(&diagramm).expect("Layout");
        assert_eq!(einmal, nochmal, "Runde {runde}: Layout ist nicht stabil");
        pruefe_zeichnung(&diagramm, &einmal);
    }
}

#[test]
fn richtung_bestimmt_die_achse() {
    let mut unten = Diagramm::leer();
    unten.knoten = vec![Knoten::neu("a", "A"), Knoten::neu("b", "B")];
    unten.kanten = vec![Kante::neu("a", "b")];
    let zeichnung = layout(&unten).unwrap();
    let a = &zeichnung.knoten[0];
    let b = &zeichnung.knoten[1];
    assert!(a.y + a.hoehe < b.y, "unten: B liegt unter A");

    let mut rechts = unten.clone();
    rechts.richtung = Richtung::Rechts;
    let zeichnung = layout(&rechts).unwrap();
    let a = &zeichnung.knoten[0];
    let b = &zeichnung.knoten[1];
    assert!(a.x + a.breite < b.x, "rechts: B liegt rechts von A");
}

#[test]
fn kreise_werden_gelegt_statt_abgelehnt() {
    let mut diagramm = Diagramm::leer();
    diagramm.knoten = vec![
        Knoten::neu("a", "A"),
        Knoten::neu("b", "B"),
        Knoten::neu("c", "C"),
    ];
    diagramm.kanten = vec![
        Kante::neu("a", "b"),
        Kante::neu("b", "c"),
        Kante::neu("c", "a"),
    ];
    let zeichnung = layout(&diagramm).expect("Kreis ist erlaubt");
    pruefe_zeichnung(&diagramm, &zeichnung);
    // Der Rückweg c→a läuft außen herum und braucht Platz rechts.
    let breiteste = zeichnung
        .knoten
        .iter()
        .map(|k| k.x + k.breite)
        .fold(0.0_f32, f32::max);
    let weg = &zeichnung.kanten[2];
    assert!(
        weg.punkte.iter().any(|p| p.x > breiteste + 1.0),
        "Rückkante läuft nicht außen herum: {weg:?}"
    );
}

#[test]
fn sprung_ueber_eine_schicht_geht_aussen_herum() {
    let mut diagramm = Diagramm::leer();
    diagramm.knoten = vec![
        Knoten::neu("a", "A"),
        Knoten::neu("b", "B"),
        Knoten::neu("c", "C"),
    ];
    diagramm.kanten = vec![
        Kante::neu("a", "b"),
        Kante::neu("b", "c"),
        Kante {
            text: Some("Abkürzung".to_string()),
            ..Kante::neu("a", "c")
        },
    ];
    let zeichnung = layout(&diagramm).expect("Layout");
    pruefe_zeichnung(&diagramm, &zeichnung);
    let breiteste = zeichnung
        .knoten
        .iter()
        .map(|k| k.x + k.breite)
        .fold(0.0_f32, f32::max);
    let sprung = &zeichnung.kanten[2];
    assert!(
        sprung.punkte.iter().any(|p| p.x > breiteste + 1.0),
        "Sprung läuft nicht außen herum: {sprung:?}"
    );
    assert!(sprung.beschriftung.is_some());
}

#[test]
fn leeres_diagramm_und_einzelknoten() {
    let leer = Diagramm::leer();
    let zeichnung = layout(&leer).expect("leer ist erlaubt");
    assert!(zeichnung.knoten.is_empty() && zeichnung.kanten.is_empty());
    assert!(zeichnung.breite > 0.0 && zeichnung.hoehe > 0.0);

    let mut einzeln = Diagramm::leer();
    einzeln.knoten = vec![Knoten::neu("a", "Nur einer")];
    let zeichnung = layout(&einzeln).expect("ein Knoten");
    pruefe_zeichnung(&einzeln, &zeichnung);
}

#[test]
fn leere_gruppe_bleibt_sichtbar() {
    let mut diagramm = Diagramm::leer();
    diagramm.knoten = vec![Knoten::neu("a", "A")];
    diagramm.gruppen = vec![Gruppe::neu("leer", "Ohne Knoten")];
    let zeichnung = layout(&diagramm).expect("Layout");
    assert_eq!(zeichnung.gruppen.len(), 1);
    let rahmen = &zeichnung.gruppen[0];
    assert!(rahmen.breite > 0.0 && rahmen.hoehe > 0.0);
    let kasten = Rechteck::neu(rahmen.x, rahmen.y, rahmen.breite, rahmen.hoehe);
    assert!(kasten.y + kasten.hoehe <= zeichnung.hoehe + 0.01);
}

#[test]
fn alle_formen_und_farben_werden_gezeichnet() {
    let mut diagramm = Diagramm::leer();
    diagramm.knoten = Form::ALLE
        .iter()
        .zip(Farbe::ALLE.iter())
        .enumerate()
        .map(|(i, (form, farbe))| Knoten {
            form: *form,
            farbe: Some(*farbe),
            ..Knoten::neu(format!("k{i}"), format!("Form {}", form.name()))
        })
        .collect();
    let zeichnung = layout(&diagramm).expect("Layout");
    pruefe_zeichnung(&diagramm, &zeichnung);
    for (i, form) in Form::ALLE.iter().enumerate() {
        assert_eq!(zeichnung.knoten[i].form, *form);
    }
}

#[test]
fn ungereimtes_modell_wird_nicht_gezeichnet() {
    let mut diagramm = Diagramm::leer();
    diagramm.knoten = vec![Knoten::neu("a", "A")];
    diagramm.kanten = vec![Kante::neu("a", "fehlt")];
    let fehler = layout(&diagramm).expect_err("unbekanntes Ende");
    assert!(fehler.to_string().contains("kein Knoten"));
}

#[test]
fn grosse_texte_bleiben_im_knoten() {
    let mut diagramm = Diagramm::leer();
    diagramm.knoten = vec![Knoten::neu("a", "Wort ".repeat(400))];
    let zeichnung = layout(&diagramm).expect("Layout");
    let platz = &zeichnung.knoten[0];
    assert!(!platz.zeilen.is_empty());
    for zeile in &platz.zeilen {
        assert!(schrift::breite(zeile, SCHRIFTGROESSE) <= platz.breite);
    }
    assert!(platz.y + platz.hoehe <= zeichnung.hoehe);
}

#[test]
fn abstand_zwischen_knoten_einer_reihe_stimmt() {
    let mut diagramm = Diagramm::leer();
    diagramm.knoten = (0..4)
        .map(|i| Knoten::neu(format!("k{i}"), format!("Knoten {i}")))
        .collect();
    let zeichnung = layout(&diagramm).expect("Layout");
    let mut reihe: Vec<_> = zeichnung.knoten.iter().collect();
    reihe.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
    for paar in reihe.windows(2) {
        let luecke = paar[1].x - (paar[0].x + paar[0].breite);
        assert!(
            (luecke - ABSTAND_GLEICH).abs() < 0.01,
            "Lücke ist {luecke} statt {ABSTAND_GLEICH}"
        );
    }
}
