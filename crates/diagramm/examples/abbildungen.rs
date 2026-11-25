// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Beispiel: Modell → Layout → SVG und PNG, dazu eine Datei über die Ablage.
//!
//! ```bash
//! CARGO_TARGET_DIR=target-pi cargo run -p quiver-diagramm --example abbildungen
//! ```
//!
//! Schreibt nur in den temporären Ordner des Systems.
//!
//! Der Name ist nicht `beispiel`, weil `crates/drawio` schon eins so nennt: zwei
//! Beispiele mit demselben Namen im selben Zielordner überschreiben sich
//! gegenseitig (Cargo warnt: „output filename collision“).

use quiver_diagramm::ablage::Ablage;
use quiver_diagramm::layout::layout;
use quiver_diagramm::modell::{Diagramm, Farbe, Form, Gruppe, Kante, KanteArt, Knoten};
use quiver_diagramm::zeichnen::Szene;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut diagramm = Diagramm::leer();
    diagramm.gruppen = vec![Gruppe::neu("vorgang", "Antragsvorgang")];
    diagramm.knoten = vec![
        Knoten {
            form: Form::Ellipse,
            farbe: Some(Farbe::Blau),
            gruppe: Some("vorgang".to_string()),
            ..Knoten::neu("start", "Antrag")
        },
        Knoten {
            form: Form::Raute,
            farbe: Some(Farbe::Gelb),
            gruppe: Some("vorgang".to_string()),
            ..Knoten::neu("pruefen", "Prüfung")
        },
        Knoten {
            form: Form::Rechteck,
            farbe: Some(Farbe::Gruen),
            ..Knoten::neu("freigeben", "Freigeben")
        },
        Knoten {
            form: Form::Zylinder,
            farbe: Some(Farbe::Grau),
            ..Knoten::neu("archiv", "Archiv")
        },
    ];
    diagramm.kanten = vec![
        Kante {
            text: Some("eingereicht".to_string()),
            ..Kante::neu("start", "pruefen")
        },
        Kante::neu("pruefen", "freigeben"),
        Kante {
            art: KanteArt::Gestrichelt,
            text: Some("danach".to_string()),
            ..Kante::neu("freigeben", "archiv")
        },
        Kante {
            art: KanteArt::Linie,
            ..Kante::neu("archiv", "pruefen")
        },
    ];
    diagramm.pruefen()?;

    let zeichnung = layout(&diagramm)?;
    let szene = Szene::aus_zeichnung(&zeichnung);
    let svg = szene.svg();
    let png = szene.png(2000)?;

    let ordner =
        std::env::temp_dir().join(format!("quiver-diagramm-beispiel-{}", std::process::id()));
    let ablage = Ablage::neu(&ordner.to_string_lossy(), "excalidraw")?;
    let stand = ablage.anlegen("beispiel.excalidraw", &diagramm.als_json()?)?;
    ablage.ausgabe_schreiben("beispiel.svg", svg.as_bytes())?;
    ablage.ausgabe_schreiben("beispiel.png", &png)?;

    println!("Ordner:    {}", ordner.display());
    println!("SVG:       {} Bytes", svg.len());
    println!(
        "PNG:       {} Bytes (Fläche {}×{})",
        png.len(),
        zeichnung.breite,
        zeichnung.hoehe
    );
    println!("Diagramm:  {} (stand {stand})", ablage.liste()?[0].pfad);
    Ok(())
}
