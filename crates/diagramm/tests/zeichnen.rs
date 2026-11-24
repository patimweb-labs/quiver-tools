// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Tests des Zeichnens: Maskierung (kein Skript, kein externer Verweis), PNG mit
//! eingebetteter Schrift und die Grenze von 2000 px.

use quiver_diagramm::layout::Punkt;
use quiver_diagramm::modell::Farbe;
use quiver_diagramm::zeichnen::{
    schrift, Ausrichtung, Figur, Rechteck, Szene, Text, MAX_PNG_KANTE, SCHRIFT_DATEN,
    SCHRIFT_FAMILIE,
};

fn text(zeilen: Vec<&str>, groesse: f32) -> Text {
    Text {
        zeilen: zeilen.into_iter().map(str::to_string).collect(),
        groesse,
        ausrichtung: Ausrichtung::Mitte,
        farbe: None,
    }
}

fn szene_mit_text(inhalt: &str) -> Szene {
    let mut szene = Szene::leer(600.0, 120.0);
    szene.formen.push(Figur::Text {
        mass: Rechteck::neu(20.0, 20.0, 560.0, 80.0),
        text: text(vec![inhalt], 24.0),
    });
    szene
}

fn png_masse(png: &[u8]) -> (u32, u32) {
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    (
        u32::from_be_bytes([png[16], png[17], png[18], png[19]]),
        u32::from_be_bytes([png[20], png[21], png[22], png[23]]),
    )
}

/// Zählt dunkle Bildpunkte – so sieht der Test, ob wirklich etwas gezeichnet wurde.
fn dunkle_punkte(png: &[u8]) -> usize {
    let decoder = png::Decoder::new(std::io::Cursor::new(png));
    let mut leser = decoder.read_info().expect("PNG lesbar");
    let mut puffer = vec![0; leser.output_buffer_size()];
    let info = leser.next_frame(&mut puffer).expect("PNG-Rahmen");
    assert_eq!(info.color_type, png::ColorType::Rgba);
    puffer
        .chunks(4)
        .filter(|pixel| pixel[0] < 128 && pixel[3] > 200)
        .count()
}

#[test]
fn einschleusung_ueber_texte_bleibt_text() {
    let boese = [
        "<script>alert(1)</script>",
        "\" onload=\"alert(1)\" x=\"",
        "<!-- Kommentar -->",
        "<![CDATA[ <script> ]]>",
        "</text><script>x</script>",
        "&amp; &lt; & < > \" '",
    ];
    let mut szene = Szene::leer(600.0, 500.0);
    for (i, inhalt) in boese.iter().enumerate() {
        szene.formen.push(Figur::Text {
            mass: Rechteck::neu(20.0, 20.0 + i as f32 * 60.0, 560.0, 40.0),
            text: text(vec![inhalt], 16.0),
        });
    }
    szene.formen.push(Figur::Container {
        mass: Rechteck::neu(10.0, 10.0, 580.0, 480.0),
        titel: Some("<script>Gruppe</script>".to_string()),
    });

    let svg = szene.svg();
    assert!(!svg.contains("<script"), "{svg}");
    assert!(!svg.contains("<img"), "{svg}");
    assert!(
        !svg.contains("onload=\""),
        "kein Attribut geschmuggelt: {svg}"
    );
    assert!(!svg.contains("<!--"));
    assert!(!svg.contains("<![CDATA"));
    assert!(!svg.contains("href="));
    assert!(!svg.contains("<image"));
    assert!(!svg.contains("url("));
    assert!(svg.contains("&lt;script&gt;"));

    // Der Beweis: das SVG ist wohlgeformtes XML, und kein Element trägt ein
    // Ereignis- oder Verweis-Attribut.
    let dokument = usvg::roxmltree::Document::parse(&svg).expect("SVG ist XML");
    for element in dokument.descendants().filter(|k| k.is_element()) {
        for attribut in element.attributes() {
            let name = attribut.name().to_ascii_lowercase();
            assert!(
                !name.starts_with("on") && name != "href" && name != "src",
                "unerwartetes Attribut {name} an {}",
                element.tag_name().name()
            );
        }
    }
    let texte: Vec<String> = dokument
        .descendants()
        .filter(|knoten| knoten.has_tag_name("text"))
        .filter_map(|knoten| knoten.text())
        .map(str::to_string)
        .collect();
    assert_eq!(texte.len(), boese.len() + 1, "jeder Text kommt einmal vor");
    // roxmltree gibt den Text unescaped zurück: genau der ursprüngliche Inhalt.
    assert!(texte.iter().any(|t| t == "<script>alert(1)</script>"));
    assert!(texte.iter().any(|t| t.contains("&amp; &lt; & < > \" '")));
}

#[test]
fn alle_figuren_ergeben_wohlgeformtes_svg() {
    let mut szene = Szene::leer(400.0, 500.0);
    let mass = Rechteck::neu(20.0, 20.0, 120.0, 60.0);
    for (i, farbe) in Farbe::ALLE.iter().enumerate() {
        let y = 20.0 + i as f32 * 60.0;
        let mass = Rechteck::neu(mass.x, y, mass.breite, mass.hoehe);
        szene.formen.push(Figur::Rechteck {
            mass,
            radius: 6.0,
            farbe: Some(*farbe),
        });
        szene.formen.push(Figur::Ellipse {
            mass,
            farbe: Some(*farbe),
        });
        szene.formen.push(Figur::Raute {
            mass,
            farbe: Some(*farbe),
        });
        szene.formen.push(Figur::Zylinder {
            mass,
            farbe: Some(*farbe),
        });
        szene.formen.push(Figur::Notiz {
            mass,
            farbe: Some(*farbe),
        });
    }
    szene.formen.push(Figur::Container {
        mass: Rechteck::neu(0.0, 0.0, 400.0, 500.0),
        titel: Some("Alles".to_string()),
    });
    szene.formen.push(Figur::Platzhalter {
        mass: Rechteck::neu(200.0, 20.0, 100.0, 60.0),
        beschriftung: Some("Bild".to_string()),
    });
    szene.formen.push(Figur::Linie {
        punkte: vec![Punkt::neu(20.0, 480.0), Punkt::neu(380.0, 480.0)],
        gestrichelt: true,
        pfeil: true,
    });

    let svg = szene.svg();
    usvg::roxmltree::Document::parse(&svg).expect("SVG ist XML");
    let png = szene.png(MAX_PNG_KANTE).expect("PNG");
    let (breite, hoehe) = png_masse(&png);
    assert!(breite <= MAX_PNG_KANTE && hoehe <= MAX_PNG_KANTE);
    assert!(dunkle_punkte(&png) > 100, "es wurde kaum etwas gezeichnet");
}

#[test]
fn schrift_ist_eingebettet_und_wird_benutzt() {
    // Die Bytes sind eine echte TrueType-Schrift …
    assert!(
        SCHRIFT_DATEN.starts_with(&[0x00, 0x01, 0x00, 0x00]) || SCHRIFT_DATEN.starts_with(b"OTTO"),
        "keine TrueType/OpenType-Schrift"
    );
    assert!(SCHRIFT_DATEN.len() > 100_000);
    assert_eq!(SCHRIFT_FAMILIE, "IBM Plex Sans");

    // … sie misst Text …
    assert!(schrift::breite("Hallo", 14.0) > 0.0);
    assert!(schrift::breite("Hallo", 28.0) > schrift::breite("Hallo", 14.0));
    assert_eq!(schrift::zeilenhoehe(10.0), 13.5);

    // … und sie wird gezeichnet: eine Szene mit nur Text ergibt dunkle Punkte.
    let png = szene_mit_text("Ärger mit Ümläüten und ß")
        .png(600)
        .expect("PNG");
    assert!(
        dunkle_punkte(&png) > 50,
        "Text wurde nicht gezeichnet – Schrift fehlt?"
    );

    let leer = Szene::leer(600.0, 120.0).png(600).expect("PNG");
    assert_eq!(dunkle_punkte(&leer), 0, "leere Szene bleibt leer");
}

#[test]
fn umbruch_haelt_die_breite_ein() {
    let text = "Ein langer Satz mit vielen Wörtern und einem \
                Donaudampfschifffahrtsgesellschaftskapitän am Ende";
    let zeilen = schrift::umbrechen(text, 14.0, 120.0);
    assert!(zeilen.len() > 1);
    for zeile in &zeilen {
        assert!(
            schrift::breite(zeile, 14.0) <= 120.0 + 0.01,
            "Zeile zu breit: {zeile}"
        );
    }
    // Kein Zeichen geht verloren (getrennte Wörter ergeben zusammen wieder den Text).
    assert_eq!(
        zeilen.concat().split_whitespace().collect::<String>(),
        text.split_whitespace().collect::<String>()
    );

    // Zeilenumbrüche im Text bleiben erhalten.
    let zeilen = schrift::umbrechen("eins\nzwei", 14.0, 200.0);
    assert_eq!(zeilen, vec!["eins".to_string(), "zwei".to_string()]);
}

#[test]
fn png_grenze_gilt_auch_bei_winzigem_wunsch() {
    let szene = szene_mit_text("Grenze");
    let png = szene.png(10_000).expect("PNG");
    let (breite, hoehe) = png_masse(&png);
    assert!(breite <= MAX_PNG_KANTE && hoehe <= MAX_PNG_KANTE);

    let png = szene.png(0).expect("PNG");
    let (breite, hoehe) = png_masse(&png);
    assert!(breite >= 1 && hoehe >= 1);
    assert!(breite <= MAX_PNG_KANTE && hoehe <= MAX_PNG_KANTE);
}

#[test]
fn svg_hat_keine_externen_verweise() {
    let svg = szene_mit_text("nur Text").svg();
    assert!(svg.contains("xmlns=\"http://www.w3.org/2000/svg\""));
    assert!(!svg.contains("http://") || svg.matches("http://").count() == 1);
    assert!(!svg.contains("https://"));
    assert!(!svg.contains("file:"));
    assert!(!svg.contains("data:"));
    assert!(!svg.contains("<foreignObject"));
    assert!(!svg.contains("<script"));
}
