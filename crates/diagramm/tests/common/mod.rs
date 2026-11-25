// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Größere Beispielmodelle für die Abnahme – von den Tests geteilt.

/// 25 Knoten in drei Gruppen – ein größeres Bild für die Abnahme.
pub fn modell_weit() -> quiver_diagramm::Diagramm {
    use quiver_diagramm::modell::{Farbe, Form, Gruppe, Kante, KanteArt, Knoten, Richtung};

    let mut d = quiver_diagramm::Diagramm::leer();
    d.richtung = Richtung::Rechts;
    d.gruppen = vec![
        Gruppe::neu("erfassung", "Erfassung"),
        Gruppe::neu("pruefung", "Prüfung"),
        Gruppe::neu("ausgabe", "Ausgabe"),
    ];
    let mut knoten = vec![Knoten {
        form: Form::Ellipse,
        farbe: Some(Farbe::Blau),
        ..Knoten::neu("start", "Antrag")
    }];
    for i in 0..6 {
        knoten.push(Knoten {
            gruppe: Some("erfassung".to_string()),
            farbe: Some(Farbe::Tuerkis),
            ..Knoten::neu(format!("e{i}"), format!("Erfassen {i}"))
        });
    }
    for i in 0..6 {
        knoten.push(Knoten {
            gruppe: Some("pruefung".to_string()),
            form: if i == 2 { Form::Raute } else { Form::Rechteck },
            farbe: Some(Farbe::Gelb),
            ..Knoten::neu(format!("p{i}"), format!("Prüfen {i}"))
        });
    }
    for i in 0..5 {
        knoten.push(Knoten {
            gruppe: Some("ausgabe".to_string()),
            farbe: Some(Farbe::Gruen),
            ..Knoten::neu(format!("a{i}"), format!("Ausgeben {i}"))
        });
    }
    for i in 0..7 {
        knoten.push(Knoten {
            form: if i == 6 {
                Form::Zylinder
            } else {
                Form::Rechteck
            },
            farbe: if i == 6 { Some(Farbe::Grau) } else { None },
            ..Knoten::neu(format!("f{i}"), format!("Nebenweg {i}"))
        });
    }
    d.knoten = knoten;

    let paare = [
        ("start", "e0"),
        ("start", "e1"),
        ("e0", "e2"),
        ("e0", "e3"),
        ("e1", "e3"),
        ("e1", "e4"),
        ("e2", "e5"),
        ("e3", "e5"),
        ("e4", "f0"),
        ("e5", "p0"),
        ("e5", "p1"),
        ("f0", "p1"),
        ("p0", "p2"),
        ("p1", "p2"),
        ("p1", "p3"),
        ("p2", "p4"),
        ("p3", "p4"),
        ("p3", "f1"),
        ("p2", "p5"),
        ("p4", "a0"),
        ("p5", "a0"),
        ("p4", "a1"),
        ("f1", "a1"),
        ("a0", "a2"),
        ("a1", "a2"),
        ("a1", "a3"),
        ("a2", "a4"),
        ("a3", "a4"),
        ("a3", "f2"),
        ("a4", "f3"),
        ("f2", "f3"),
        ("f3", "f4"),
        ("f4", "f5"),
        ("f5", "f6"),
        ("f6", "start"),
    ];
    let mut kanten: Vec<Kante> = paare
        .iter()
        .map(|(von, nach)| Kante::neu(*von, *nach))
        .collect();
    kanten[9].text = Some("vollständig".to_string());
    kanten[18].text = Some("ja".to_string());
    kanten[19].text = Some("nein".to_string());
    kanten[16].art = KanteArt::Gestrichelt;
    d.kanten = kanten;
    d
}

/// Kreis mit Mehrfachkanten.
pub fn modell_kreis() -> quiver_diagramm::Diagramm {
    use quiver_diagramm::modell::{Farbe, Form, Kante, KanteArt, Knoten};

    let mut d = quiver_diagramm::Diagramm::leer();
    d.knoten = vec![
        Knoten {
            form: Form::Ellipse,
            farbe: Some(Farbe::Blau),
            ..Knoten::neu("a", "Anfang")
        },
        Knoten {
            form: Form::Raute,
            farbe: Some(Farbe::Gelb),
            ..Knoten::neu("b", "Geprüft?")
        },
        Knoten {
            farbe: Some(Farbe::Gruen),
            ..Knoten::neu("c", "Freigeben")
        },
        Knoten {
            form: Form::Zylinder,
            farbe: Some(Farbe::Grau),
            ..Knoten::neu("d", "Archiv")
        },
    ];
    d.kanten = vec![
        Kante {
            text: Some("eingereicht".to_string()),
            ..Kante::neu("a", "b")
        },
        Kante {
            text: Some("nachtragen".to_string()),
            ..Kante::neu("a", "b")
        },
        Kante {
            text: Some("ja".to_string()),
            ..Kante::neu("b", "c")
        },
        Kante {
            text: Some("nein".to_string()),
            art: KanteArt::Gestrichelt,
            ..Kante::neu("b", "a")
        },
        Kante::neu("c", "d"),
        Kante {
            text: Some("Wiederaufnahme".to_string()),
            ..Kante::neu("d", "b")
        },
    ];
    d
}
