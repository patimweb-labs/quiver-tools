// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Tests des Layouts: Überlappungsfreiheit über viele zufällige Graphen (fester
//! Startwert), Gleichheit bei gleicher Eingabe, Gruppen und Kantenführung.

mod common;

use quiver_diagramm::layout::{
    layout, Zeichnung, ABSTAND_GLEICH, ABSTAND_SCHICHT, SCHRIFTGROESSE, ZEILENHOEHE,
};
use quiver_diagramm::modell::{Diagramm, Farbe, Form, Gruppe, Kante, KanteArt, Knoten, Richtung};
use quiver_diagramm::zeichnen::{schrift, Figur, Punkt, Rechteck, Szene};

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

/// Ruhigerer Zufallsgraph (etwa ein Knoten je Kante) – für Prüfungen, die
/// lesbare Beschriftungen verlangen.
fn zufallsdiagramm_ruhig(zufall: &mut Zufall) -> Diagramm {
    let mut diagramm = zufallsdiagramm(zufall);
    let grenze = diagramm.knoten.len();
    if diagramm.kanten.len() > grenze {
        diagramm.kanten.truncate(grenze);
        // Doppelte Kanten entfernen, damit das Modell gültig bleibt ist nicht
        // nötig: mehrfache Kanten sind erlaubt.
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

    // Kanten haben einen Weg, und kein Weg läuft durch einen Knoten. Geprüft
    // wird die Form, nicht der Kasten: Raute und Ellipse sind innen kleiner.
    for lauf in &zeichnung.kanten {
        assert!(lauf.punkte.len() >= 2, "Kante ohne Weg: {lauf:?}");
        for punkt in &lauf.punkte {
            assert!(punkt.x.is_finite() && punkt.y.is_finite());
            for platz in &zeichnung.knoten {
                assert!(
                    !im_knoten(platz, punkt),
                    "Kante {}→{} läuft durch {}: {punkt:?}",
                    lauf.von,
                    lauf.nach,
                    platz.id
                );
            }
        }
    }
}

/// Liegt der Punkt im Inneren der Knotenform (nicht nur im Kasten)?
fn im_knoten(platz: &quiver_diagramm::layout::KnotenPlatz, punkt: &Punkt) -> bool {
    let halb_breite = platz.breite / 2.0;
    let halb_hoehe = platz.hoehe / 2.0;
    let dx = (punkt.x - (platz.x + halb_breite)).abs() / halb_breite.max(0.01);
    let dy = (punkt.y - (platz.y + halb_hoehe)).abs() / halb_hoehe.max(0.01);
    match platz.form {
        Form::Raute => dx + dy < 0.9,
        Form::Ellipse => dx * dx + dy * dy < 0.8,
        _ => dx < 0.9 && dy < 0.9,
    }
}

/// Rechteck eines Gruppenrahmens.
fn rahmen_rechteck(rahmen: &quiver_diagramm::layout::GruppenRahmen) -> Rechteck {
    Rechteck::neu(rahmen.x, rahmen.y, rahmen.breite, rahmen.hoehe)
}

/// Kleinster Abstand zweier achsenparalleler Rechtecke (0 = sie berühren sich).
fn luecke(a: &Rechteck, b: &Rechteck) -> f32 {
    let dx = (a.x - (b.x + b.breite)).max(b.x - (a.x + a.breite));
    let dy = (a.y - (b.y + b.hoehe)).max(b.y - (a.y + a.hoehe));
    dx.max(dy).max(0.0)
}

/// Kleinster Abstand eines inneren Rechtecks zu den Kanten des äußeren.
fn rand_abstand(aussen: &Rechteck, innen: &Rechteck) -> f32 {
    let links = innen.x - aussen.x;
    let oben = innen.y - aussen.y;
    let rechts = (aussen.x + aussen.breite) - (innen.x + innen.breite);
    let unten = (aussen.y + aussen.hoehe) - (innen.y + innen.hoehe);
    links.min(oben).min(rechts).min(unten)
}

/// Trifft die Strecke `a`–`b` das Rechteck (sein Inneres)?
fn strecke_trifft(a: Punkt, b: Punkt, r: &Rechteck) -> bool {
    let (x_lo, x_hi) = (a.x.min(b.x), a.x.max(b.x));
    let (y_lo, y_hi) = (a.y.min(b.y), a.y.max(b.y));
    if (x_hi - x_lo).abs() < 0.01 {
        y_hi.min(r.y + r.hoehe) - y_lo.max(r.y) > 0.5
            && x_lo > r.x + 0.5
            && x_lo < r.x + r.breite - 0.5
    } else if (y_hi - y_lo).abs() < 0.01 {
        x_hi.min(r.x + r.breite) - x_lo.max(r.x) > 0.5
            && y_lo > r.y + 0.5
            && y_lo < r.y + r.hoehe - 0.5
    } else {
        false
    }
}

/// Rechteck einer Kantenbeschriftung, so wie sie gezeichnet wird.
fn beschriftungs_rechteck(beschriftung: &quiver_diagramm::layout::Kantenbeschriftung) -> Rechteck {
    Rechteck::neu(
        beschriftung.position.x,
        beschriftung.position.y,
        beschriftung.breite,
        beschriftung.zeilen.len() as f32 * ZEILENHOEHE,
    )
}

/// Abstand eines Punktes zum Linienzug.
fn abstand_zum_weg(punkt: &Punkt, weg: &[Punkt]) -> f32 {
    let mut bestes = f32::INFINITY;
    for paar in weg.windows(2) {
        let (a, b) = (paar[0], paar[1]);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let laenge = (dx * dx + dy * dy).sqrt();
        let anteil = if laenge < 0.01 {
            0.0
        } else {
            (((punkt.x - a.x) * dx + (punkt.y - a.y) * dy) / (laenge * laenge)).clamp(0.0, 1.0)
        };
        let nah = Punkt::neu(a.x + dx * anteil, a.y + dy * anteil);
        let abstand = ((punkt.x - nah.x).powi(2) + (punkt.y - nah.y).powi(2)).sqrt();
        bestes = bestes.min(abstand);
    }
    bestes
}

/// Gruppen der beiden Enden einer Kante.
fn kanten_gruppen(diagramm: &Diagramm, lauf: &quiver_diagramm::layout::KantenLauf) -> Vec<String> {
    let mut liste = Vec::new();
    for id in [&lauf.von, &lauf.nach] {
        if let Some(index) = diagramm.knoten_index(id) {
            if let Some(gruppe) = &diagramm.knoten[index].gruppe {
                if !liste.contains(gruppe) {
                    liste.push(gruppe.clone());
                }
            }
        }
    }
    liste
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

// ------------------------------------------------------ Gruppen und Rahmen (1)

#[test]
fn gruppenrahmen_umschliessen_nur_ihre_mitglieder() {
    let mut zufall = Zufall::neu(4_242);
    for runde in 0..40 {
        let diagramm = zufallsdiagramm(&mut zufall);
        let zeichnung = layout(&diagramm).expect("Layout");
        for (i, rahmen) in zeichnung.gruppen.iter().enumerate() {
            let kasten = rahmen_rechteck(rahmen);
            assert!(kasten.breite > 0.0 && kasten.hoehe > 0.0);
            for platz in &zeichnung.knoten {
                let r = knoten_rechteck(platz);
                if platz.gruppe.as_deref() == Some(rahmen.id.as_str()) {
                    assert!(
                        enthaelt(&kasten, &r),
                        "Runde {runde}: {} liegt nicht in Rahmen {}",
                        platz.id,
                        rahmen.id
                    );
                } else {
                    assert!(
                        luecke(&kasten, &r) >= 1.0,
                        "Runde {runde}: Rahmen {} berührt den fremden Knoten {} ({kasten:?} / {r:?})",
                        rahmen.id,
                        platz.id
                    );
                }
            }
            for andere in &zeichnung.gruppen[i + 1..] {
                assert!(
                    luecke(&kasten, &rahmen_rechteck(andere)) >= 1.0,
                    "Runde {runde}: Rahmen {} und {} überlappen sich",
                    rahmen.id,
                    andere.id
                );
            }
        }
    }
}

#[test]
fn mitglieder_liegen_zusammen_und_getrennt_von_fremden() {
    let mut zufall = Zufall::neu(9_191);
    for runde in 0..24 {
        let diagramm = zufallsdiagramm(&mut zufall);
        let zeichnung = layout(&diagramm).expect("Layout");
        for rahmen in &zeichnung.gruppen {
            let mitglieder: Vec<&quiver_diagramm::layout::KnotenPlatz> = zeichnung
                .knoten
                .iter()
                .filter(|platz| platz.gruppe.as_deref() == Some(rahmen.id.as_str()))
                .collect();
            if mitglieder.is_empty() {
                continue;
            }
            // Alle Mitglieder liegen im Rahmen, mit Rand zu seiner Kante.
            let kasten = rahmen_rechteck(rahmen);
            for mitglied in &mitglieder {
                let r = knoten_rechteck(mitglied);
                assert!(
                    rand_abstand(&kasten, &r) >= 1.0,
                    "Runde {runde}: {} klebt am Rahmen {} ({:?} / {r:?})",
                    mitglied.id,
                    rahmen.id,
                    kasten
                );
            }
        }
    }
}

// --------------------------------------------------------------- Kanten (3)

#[test]
fn kanten_meiden_fremde_knoten_und_fremde_rahmen() {
    let mut zufall = Zufall::neu(31_337);
    for runde in 0..30 {
        let diagramm = zufallsdiagramm(&mut zufall);
        let zeichnung = layout(&diagramm).expect("Layout");
        for lauf in &zeichnung.kanten {
            let eigene = kanten_gruppen(&diagramm, lauf);
            for rahmen in &zeichnung.gruppen {
                if eigene.iter().any(|id| id == &rahmen.id) {
                    continue;
                }
                let kasten = rahmen_rechteck(rahmen);
                for paar in lauf.punkte.windows(2) {
                    assert!(
                        !strecke_trifft(paar[0], paar[1], &kasten),
                        "Runde {runde}: Kante {}→{} läuft durch Rahmen {}",
                        lauf.von,
                        lauf.nach,
                        rahmen.id
                    );
                }
            }
            for platz in &zeichnung.knoten {
                if platz.id == lauf.von || platz.id == lauf.nach {
                    continue;
                }
                let r = knoten_rechteck(platz);
                for paar in lauf.punkte.windows(2) {
                    assert!(
                        !strecke_trifft(paar[0], paar[1], &r),
                        "Runde {runde}: Kante {}→{} läuft durch Knoten {}",
                        lauf.von,
                        lauf.nach,
                        platz.id
                    );
                }
            }
        }
    }
}

#[test]
fn parallele_kanten_treten_getrennt_aus() {
    let mut diagramm = Diagramm::leer();
    diagramm.knoten = vec![
        Knoten {
            form: Form::Raute,
            ..Knoten::neu("frage", "Bezahlt?")
        },
        Knoten::neu("ja", "Ware versenden"),
        Knoten::neu("nein", "Erinnerung senden"),
        Knoten::neu("spaeter", "Später prüfen"),
    ];
    diagramm.kanten = vec![
        Kante::neu("frage", "ja"),
        Kante::neu("frage", "nein"),
        Kante::neu("frage", "spaeter"),
    ];
    let zeichnung = layout(&diagramm).expect("Layout");
    let startpunkte: Vec<Punkt> = zeichnung.kanten.iter().map(|lauf| lauf.punkte[0]).collect();
    for (i, a) in startpunkte.iter().enumerate() {
        for b in &startpunkte[i + 1..] {
            let abstand = ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt();
            assert!(
                abstand >= 4.0,
                "zwei Kanten starten am selben Punkt: {a:?} und {b:?}"
            );
        }
    }
}

// -------------------------------------------------------- Beschriftungen (2)

/// Prüft alle Zusagen an Kantenbeschriftungen für ein Diagramm.
fn pruefe_beschriftungen(zeichnung: &Zeichnung, runde: usize) {
    for (i, lauf) in zeichnung.kanten.iter().enumerate() {
        let Some(beschriftung) = &lauf.beschriftung else {
            continue;
        };
        let feld = beschriftungs_rechteck(beschriftung);
        assert!(feld.breite > 0.0 && feld.hoehe > 0.0);

        // Nicht auf einem Knoten.
        for platz in &zeichnung.knoten {
            assert!(
                !ueberlappt(&feld, &knoten_rechteck(platz)),
                "Runde {runde}: „{}“ liegt auf Knoten {}",
                beschriftung.text,
                platz.id
            );
        }
        // Nicht auf einer fremden Linie.
        for (j, anderer) in zeichnung.kanten.iter().enumerate() {
            if i == j {
                continue;
            }
            for paar in anderer.punkte.windows(2) {
                assert!(
                    !strecke_trifft(paar[0], paar[1], &feld),
                    "Runde {runde}: „{}“ liegt auf der Kante {}→{}",
                    beschriftung.text,
                    anderer.von,
                    anderer.nach
                );
            }
        }
        // Zwei Beschriftungen überlappen sich nicht.
        for (j, anderer) in zeichnung.kanten.iter().enumerate() {
            if i == j {
                continue;
            }
            if let Some(andere) = &anderer.beschriftung {
                assert!(
                    !ueberlappt(&feld, &beschriftungs_rechteck(andere)),
                    "Runde {runde}: „{}“ und „{}“ überlappen sich",
                    beschriftung.text,
                    andere.text
                );
            }
        }
        // Sie sitzt an ihrer eigenen Kante.
        let mitte = Punkt::neu(feld.x + feld.breite / 2.0, feld.y + feld.hoehe / 2.0);
        // Mittig auf der Kante oder höchstens daneben – nie irgendwo sonst.
        let grenze = feld.breite.max(feld.hoehe) / 2.0 + 8.0;
        assert!(
            abstand_zum_weg(&mitte, &lauf.punkte) <= grenze,
            "Runde {runde}: „{}“ sitzt nicht an seiner Kante ({mitte:?})",
            beschriftung.text
        );
    }
}

#[test]
fn kantenbeschriftungen_liegen_frei_und_an_ihrer_kante() {
    // Die Abnahmebilder: Ablauf mit „ja“/„nein“, Gerät mit „Abgleich“.
    for text in [
        r#"{"knoten":[{"id":"a","text":"Bestellung eingegangen"},{"id":"b","text":"Bezahlt?","form":"raute"},{"id":"c","text":"Ware versenden"},{"id":"d","text":"Erinnerung senden"},{"id":"e","text":"Kundendaten","form":"zylinder"},{"id":"f","text":"Fertig","form":"ellipse"}],"kanten":[{"von":"a","nach":"b"},{"von":"b","nach":"c","text":"ja"},{"von":"b","nach":"d","text":"nein"},{"von":"d","nach":"b","art":"gestrichelt"},{"von":"c","nach":"f"},{"von":"a","nach":"e"}]}"#,
        r#"{"richtung":"rechts","knoten":[{"id":"pi","text":"pi","gruppe":"ag"},{"id":"he","text":"Hermes","gruppe":"ag"},{"id":"cl","text":"quiver-Client","gruppe":"ge"},{"id":"m","text":"quiver-m365","gruppe":"ge"},{"id":"z","text":"quiver-zep","gruppe":"ge"},{"id":"s","text":"quiver-Server (Bibliothek)"},{"id":"g","text":"Microsoft Graph"},{"id":"ze","text":"ZEP"}],"kanten":[{"von":"pi","nach":"cl"},{"von":"he","nach":"cl"},{"von":"cl","nach":"m"},{"von":"cl","nach":"z"},{"von":"cl","nach":"s","text":"Abgleich"},{"von":"m","nach":"g"},{"von":"z","nach":"ze"}],"gruppen":[{"id":"ag","titel":"Agenten"},{"id":"ge","titel":"Gerät"}]}"#,
    ] {
        let diagramm = Diagramm::aus_json(text).expect("Modell");
        let zeichnung = layout(&diagramm).expect("Layout");
        pruefe_beschriftungen(&zeichnung, 0);
    }

    // Kreis mit Mehrfachkanten und Beschriftungen.
    let mut kreis = Diagramm::leer();
    kreis.knoten = vec![
        Knoten::neu("a", "Anfang"),
        Knoten::neu("b", "Mitte"),
        Knoten::neu("c", "Ende"),
    ];
    kreis.kanten = vec![
        Kante {
            text: Some("hin".to_string()),
            ..Kante::neu("a", "b")
        },
        Kante {
            text: Some("zurück".to_string()),
            ..Kante::neu("b", "c")
        },
        Kante {
            text: Some("nochmal".to_string()),
            art: KanteArt::Gestrichelt,
            ..Kante::neu("c", "a")
        },
        Kante {
            text: Some("quer".to_string()),
            ..Kante::neu("a", "c")
        },
    ];
    pruefe_beschriftungen(&layout(&kreis).expect("Layout"), 1);

    // Kleine Zufallsgraphen mit wenigen Beschriftungen.
    let mut zufall = Zufall::neu(5_150);
    for runde in 0..20 {
        let mut diagramm = zufallsdiagramm_ruhig(&mut zufall);
        for (i, kante) in diagramm.kanten.iter_mut().enumerate() {
            if i % 3 != 0 {
                kante.text = None;
            }
        }
        let zeichnung = layout(&diagramm).expect("Layout");
        pruefe_beschriftungen(&zeichnung, 100 + runde);
    }
}

#[test]
fn jede_beschriftung_hat_einen_deckenden_hintergrund() {
    let mut diagramm = Diagramm::leer();
    diagramm.knoten = vec![
        Knoten::neu("a", "Antrag"),
        Knoten::neu("b", "Prüfung"),
        Knoten::neu("c", "Freigabe"),
    ];
    diagramm.kanten = vec![
        Kante {
            text: Some("eingereicht".to_string()),
            ..Kante::neu("a", "b")
        },
        Kante {
            text: Some("Abgleich".to_string()),
            ..Kante::neu("b", "c")
        },
    ];
    let zeichnung = layout(&diagramm).expect("Layout");
    let szene = Szene::aus_zeichnung(&zeichnung);
    let hintergruende: Vec<Rechteck> = szene
        .formen
        .iter()
        .filter_map(|figur| match figur {
            Figur::Rechteck {
                mass, farbe: None, ..
            } => Some(*mass),
            _ => None,
        })
        .collect();
    for lauf in &zeichnung.kanten {
        let feld = beschriftungs_rechteck(lauf.beschriftung.as_ref().expect("Beschriftung"));
        let mitte = Punkt::neu(feld.x + feld.breite / 2.0, feld.y + feld.hoehe / 2.0);
        assert!(
            hintergruende.iter().any(|mass| {
                enthaelt(mass, &feld)
                    && mitte.x >= mass.x
                    && mitte.x <= mass.x + mass.breite
                    && mitte.y >= mass.y
                    && mitte.y <= mass.y + mass.hoehe
            }),
            "kein deckender Hintergrund für „{}“",
            lauf.beschriftung.as_ref().expect("Beschriftung").text
        );
    }
}

// ----------------------------------------------------------- Schichtabstand (4)

#[test]
fn schichtdicke_folgt_der_breitesten_spalte() {
    // Drei Schichten; in der Mitte stehen drei Knoten übereinander. Die Spalte
    // wird so dick wie ihr breitester Knoten – nicht so dick wie alle zusammen.
    let mut breit = Diagramm::leer();
    breit.richtung = Richtung::Rechts;
    breit.knoten = vec![
        Knoten::neu("a", "Anfang"),
        Knoten::neu("b1", "Mitte mit etwas mehr Text"),
        Knoten::neu("b2", "Mitte"),
        Knoten::neu("b3", "Später"),
        Knoten::neu("c", "Ende"),
    ];
    breit.kanten = vec![
        Kante::neu("a", "b1"),
        Kante::neu("a", "b2"),
        Kante::neu("a", "b3"),
        Kante::neu("b1", "c"),
    ];
    let breite_breit = layout(&breit).expect("Layout").breite;

    let mut schmal = Diagramm::leer();
    schmal.richtung = Richtung::Rechts;
    schmal.knoten = vec![
        Knoten::neu("a", "Anfang"),
        Knoten::neu("b1", "Mitte mit etwas mehr Text"),
        Knoten::neu("c", "Ende"),
    ];
    schmal.kanten = vec![Kante::neu("a", "b1"), Kante::neu("b1", "c")];
    let breite_schmal = layout(&schmal).expect("Layout").breite;

    assert!(
        breite_breit <= breite_schmal + 1.0,
        "die mittlere Spalte ist {breite_breit} statt {breite_schmal} breit – die Dicke \
         folgt nicht dem breitesten Knoten"
    );
}

#[test]
fn zwischenraeume_sind_nicht_groesser_als_noetig() {
    let mut diagramm = Diagramm::leer();
    diagramm.knoten = vec![
        Knoten::neu("a", "A"),
        Knoten::neu("b", "B"),
        Knoten::neu("c", "C"),
    ];
    diagramm.kanten = vec![Kante::neu("a", "b"), Kante::neu("b", "c")];
    let zeichnung = layout(&diagramm).expect("Layout");
    let mut reihe: Vec<_> = zeichnung.knoten.iter().collect();
    reihe.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap());
    for paar in reihe.windows(2) {
        let luecke = paar[1].y - (paar[0].y + paar[0].hoehe);
        assert!(
            (ABSTAND_SCHICHT - 0.01..=ABSTAND_SCHICHT + 60.0).contains(&luecke),
            "Zwischenraum ist {luecke} statt etwa {ABSTAND_SCHICHT}"
        );
    }
}

// -------------------------------------------------- Große Abnahmemodelle (5)

/// Alle Zusagen auf einmal – auch für die beiden großen Abnahmebilder.
#[test]
fn grosse_diagramme_halten_alle_zusagen() {
    for (name, diagramm) in [
        ("weit", common::modell_weit()),
        ("kreis", common::modell_kreis()),
    ] {
        diagramm.pruefen().expect("Modell ist gültig");
        let zeichnung = layout(&diagramm).expect("Layout");
        pruefe_zeichnung(&diagramm, &zeichnung);
        pruefe_beschriftungen(&zeichnung, 0);

        for (i, rahmen) in zeichnung.gruppen.iter().enumerate() {
            let kasten = rahmen_rechteck(rahmen);
            for platz in &zeichnung.knoten {
                let r = knoten_rechteck(platz);
                if platz.gruppe.as_deref() == Some(rahmen.id.as_str()) {
                    assert!(
                        enthaelt(&kasten, &r),
                        "{name}: {} nicht in {}",
                        platz.id,
                        rahmen.id
                    );
                } else {
                    assert!(
                        luecke(&kasten, &r) >= 1.0,
                        "{name}: Rahmen {} berührt {}",
                        rahmen.id,
                        platz.id
                    );
                }
            }
            for andere in &zeichnung.gruppen[i + 1..] {
                assert!(
                    luecke(&kasten, &rahmen_rechteck(andere)) >= 1.0,
                    "{name}: Rahmen {} und {} überlappen sich",
                    rahmen.id,
                    andere.id
                );
            }
        }

        for lauf in &zeichnung.kanten {
            let eigene = kanten_gruppen(&diagramm, lauf);
            for rahmen in &zeichnung.gruppen {
                if eigene.iter().any(|id| id == &rahmen.id) {
                    continue;
                }
                let kasten = rahmen_rechteck(rahmen);
                for paar in lauf.punkte.windows(2) {
                    assert!(
                        !strecke_trifft(paar[0], paar[1], &kasten),
                        "{name}: Kante {}→{} läuft durch Rahmen {}",
                        lauf.von,
                        lauf.nach,
                        rahmen.id
                    );
                }
            }
            for platz in &zeichnung.knoten {
                if platz.id == lauf.von || platz.id == lauf.nach {
                    continue;
                }
                let r = knoten_rechteck(platz);
                for paar in lauf.punkte.windows(2) {
                    assert!(
                        !strecke_trifft(paar[0], paar[1], &r),
                        "{name}: Kante {}→{} läuft durch {}",
                        lauf.von,
                        lauf.nach,
                        platz.id
                    );
                }
            }
        }
    }
}
