// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Beispiel: Modell → `.drawio`, fertiges XML → Seite, Datei → PNG und Link.
//!
//! ```bash
//! CARGO_TARGET_DIR=target-pi cargo run -p quiver-drawio --example beispiel
//! ```
//!
//! Schreibt nur in den temporären Ordner des Systems.

use quiver_diagramm::ablage::Ablage;
use quiver_diagramm::modell::{Diagramm, Farbe, Form, Gruppe, Kante, KanteArt, Knoten};
use quiver_drawio::aus_modell;
use quiver_drawio::bild;
use quiver_drawio::link;
use quiver_drawio::mx::{self, Datei};
use quiver_drawio::xml;

/// Formen, die das Modell nicht beschreiben kann – von Hand als XML.
const FORMEN: &str = r#"<mxGraphModel dx="0" dy="0" grid="0" page="1" pageScale="1" pageWidth="850" pageHeight="700" math="0" shadow="0">
  <root>
    <mxCell id="0"/>
    <mxCell id="1" parent="0"/>
    <mxCell id="lane" value="Antragsbearbeitung" style="swimlane;startSize=30;html=1;fillColor=#f7f8fa;strokeColor=#8a94a0;" vertex="1" parent="1">
      <mxGeometry x="40" y="40" width="360" height="240" as="geometry"/>
    </mxCell>
    <mxCell id="start" value="Antrag" style="ellipse;html=1;fillColor=#e2efff;strokeColor=#2f6db5;" vertex="1" parent="lane">
      <mxGeometry x="30" y="60" width="140" height="60" as="geometry"/>
    </mxCell>
    <mxCell id="pruefen" value="Prüfung &amp; Freigabe" style="rhombus;html=1;fillColor=#fdf3d0;strokeColor=#a8871f;" vertex="1" parent="lane">
      <mxGeometry x="30" y="150" width="160" height="70" as="geometry"/>
    </mxCell>
    <mxCell id="archiv" value="Archiv" style="shape=cylinder3;html=1;fillColor=#eceef1;strokeColor=#6b7480;" vertex="1" parent="1">
      <mxGeometry x="470" y="60" width="120" height="90" as="geometry"/>
    </mxCell>
    <mxCell id="notiz" value="Frist: 14 Tage" style="shape=note;html=1;fillColor=#fdeedd;strokeColor=#b06a22;size=15;" vertex="1" parent="1">
      <mxGeometry x="470" y="190" width="140" height="80" as="geometry"/>
    </mxCell>
    <mxCell id="text" value="Quelle: Fachbereich" style="text;html=1;align=left;verticalAlign=top;" vertex="1" parent="1">
      <mxGeometry x="40" y="320" width="220" height="30" as="geometry"/>
    </mxCell>
    <mxCell id="fremd" value="Lambda" style="shape=mxgraph.aws4.lambda;html=1;fillColor=#f0e8fb;strokeColor=#6b46a8;" vertex="1" parent="1">
      <mxGeometry x="470" y="320" width="100" height="90" as="geometry"/>
    </mxCell>
    <UserObject id="antrag" label="Antrag als PDF" link="https://example.test/antrag" eigene="1">
      <mxCell style="rounded=1;html=1;fillColor=#e2f6e7;strokeColor=#2f7d43;" vertex="1" parent="1">
        <mxGeometry x="40" y="400" width="150" height="60" as="geometry"/>
      </mxCell>
    </UserObject>
    <mxCell id="k1" value="eingereicht" style="edgeStyle=orthogonalEdgeStyle;rounded=1;html=1;" edge="1" source="start" target="pruefen" parent="1">
      <mxGeometry relative="1" as="geometry"/>
    </mxCell>
    <mxCell id="k2" value="ja" style="edgeStyle=orthogonalEdgeStyle;rounded=1;html=1;" edge="1" source="pruefen" target="archiv" parent="1">
      <mxGeometry relative="1" as="geometry"/>
    </mxCell>
    <mxCell id="k3" value="danach" style="edgeStyle=orthogonalEdgeStyle;rounded=1;html=1;dashed=1;" edge="1" source="antrag" target="notiz" parent="1">
      <mxGeometry relative="1" as="geometry"/>
    </mxCell>
    <mxCell id="k4" style="endArrow=none;html=1;" edge="1" source="archiv" target="fremd" parent="1">
      <mxGeometry relative="1" as="geometry"/>
    </mxCell>
  </root>
</mxGraphModel>"#;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Eine Beschreibung (wie `drawio_erstellen`) wird zu draw.io-Zellen.
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
    let mut datei = mx::neue_datei("Antrag", aus_modell::seite_aus_diagramm(&diagramm)?)?;

    // 2. Eine zweite Seite aus fertigem XML – mit Formen, die das Modell nicht kennt.
    datei.seite_hinzufuegen("Formen", None, xml::lesen(FORMEN)?)?;

    // 3. Über die Ablage schreiben (wie das Werkzeug) und wieder lesen.
    let ordner =
        std::env::temp_dir().join(format!("quiver-drawio-beispiel-{}", std::process::id()));
    let ablage = Ablage::neu(&ordner.to_string_lossy(), "drawio")?;
    let stand = ablage.anlegen("beispiel.drawio", &datei.als_text())?;
    let gelesen = Datei::aus_text(&ablage.lesen("beispiel.drawio")?.text)?;

    // 4. Jede Seite zeichnen.
    for seite in gelesen.seiten()? {
        let gezeichnet = bild::zeichnung_aus_modell(&gelesen.modell(seite.index)?)?;
        let name = format!("seite-{}.png", seite.index + 1);
        let png = gezeichnet.png(1600)?;
        ablage.ausgabe_schreiben(&name, &png)?;
        println!(
            "{}: „{}“ – {} ({} Bytes)",
            name,
            seite.name,
            gezeichnet.beschreibung().replace('\n', " | "),
            png.len()
        );
    }

    // 5. Der Link trägt das ganze Dokument im Fragment.
    let adresse = link::link(&gelesen.als_text())?;
    println!();
    println!("Ordner:    {}", ordner.display());
    println!("Datei:     beispiel.drawio (stand {stand})");
    println!(
        "Link:      {} Zeichen, {} Seiten",
        adresse.len(),
        gelesen.seiten()?.len()
    );
    Ok(())
}
