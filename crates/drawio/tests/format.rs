// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Das Format selbst: XML sicher lesen, Seiten entpacken, Unbekanntes erhalten,
//! Links bauen. Kein Werkzeug, keine Datei auf der Platte.

use quiver_drawio::formen::Form;
use quiver_drawio::link;
use quiver_drawio::mx::{self, Datei};
use quiver_drawio::xml::Element;

/// Eine Datei mit Hülle, HTML-Label, Kante und unbekannten Feldern.
const DATEI: &str = r#"<mxfile host="app.diagrams.net" modified="2026-01-01" eigene="1">
  <diagram id="s1" name="Ablauf">
    <mxGraphModel dx="0" grid="1">
      <root>
        <mxCell id="0"/>
        <mxCell id="1" parent="0"/>
        <UserObject id="a" label="&lt;b&gt;Antrag&lt;/b&gt;" link="mailto:a@example.test" daten="x">
          <mxCell style="rounded=1;html=1;sketch=1;fillColor=#e2efff;" vertex="1" parent="1">
            <mxGeometry x="40" y="40" width="140" height="60" as="geometry"/>
          </mxCell>
        </UserObject>
        <mxCell id="b" value="Fertig" style="ellipse;html=1;" vertex="1" parent="1">
          <mxGeometry x="40" y="160" width="140" height="60" as="geometry"/>
        </mxCell>
        <mxCell id="k" value="ja" style="endArrow=classic;" edge="1" source="a" target="b" parent="1">
          <mxGeometry relative="1" as="geometry"/>
        </mxCell>
      </root>
    </mxGraphModel>
  </diagram>
</mxfile>"#;

/// Ein leeres Modell als Text.
fn leeres_modell() -> String {
    mx::leeres_modell().als_text()
}

#[test]
fn dtd_und_entitaeten_werden_abgelehnt() {
    let dtd = r#"<!DOCTYPE mxfile [<!ENTITY xxe SYSTEM "file:///etc/passwd">]><mxfile/>"#;
    let fehler = Datei::aus_text(dtd).unwrap_err().to_string();
    assert!(fehler.contains("DTD"), "{fehler}");

    let entitaet = "<mxfile><diagram id=\"s\" name=\"x\">&xxe;</diagram></mxfile>";
    let fehler = Datei::aus_text(entitaet).unwrap_err().to_string();
    assert!(fehler.contains("Entität"), "{fehler}");
}

#[test]
fn eine_andere_wurzel_ist_kein_drawio_dokument() {
    let fehler = Datei::aus_text("<svg/>").unwrap_err().to_string();
    assert!(fehler.contains("<mxfile>"), "{fehler}");
}

#[test]
fn unbekannte_felder_und_hullen_ueberleben_eine_aenderung() {
    let mut datei = Datei::aus_text(DATEI).unwrap();
    let mut modell = datei.modell(0).unwrap();
    let mut root: Element = modell.kind("root").unwrap().clone();

    let aenderungen = [mx::Aenderung {
        id: "b",
        text: Some("Erledigt"),
        farbe: Some(quiver_diagramm::modell::Farbe::Gruen),
        form: Some(Form::Raute),
        lage: mx::LageAenderung {
            x: Some(300.0),
            y: Some(160.5),
            breite: None,
            hoehe: None,
        },
        quelle: None,
        ziel: None,
    }];
    let geaendert = mx::aendern(&mut root, &aenderungen).unwrap();
    assert_eq!(geaendert, vec!["b".to_string()]);
    mx::root_setzen(&mut modell, root);
    datei.modell_setzen(0, modell).unwrap();
    let text = datei.als_text();

    // Unbekannte Attribute der Datei und der Hülle bleiben.
    assert!(text.contains("eigene=\"1\""), "{text}");
    assert!(text.contains("modified=\"2026-01-01\""), "{text}");
    assert!(text.contains("daten=\"x\""), "{text}");
    assert!(text.contains("link=\"mailto:a@example.test\""), "{text}");
    // Das HTML-Label wird nicht neu geschrieben.
    assert!(
        text.contains("label=\"&lt;b&gt;Antrag&lt;/b&gt;\""),
        "{text}"
    );
    // Die unberührte Zelle ist inhaltsgleich: Stil und Geometrie stehen noch so da.
    assert!(
        text.contains("style=\"rounded=1;html=1;sketch=1;fillColor=#e2efff;\""),
        "{text}"
    );
    assert!(text.contains("width=\"140\""), "{text}");
    // Die geänderte Zelle trägt die neue Form, Farbe und Lage; `sketch` gab es dort nicht.
    assert!(text.contains("rhombus"), "{text}");
    assert!(text.contains("x=\"300\""), "{text}");
    assert!(text.contains("y=\"160.5\""), "{text}");
    assert!(
        text.contains(
            quiver_diagramm::zeichnen::palette::toene(Some(quiver_diagramm::modell::Farbe::Gruen))
                .fuellung
        ),
        "{text}"
    );
}

#[test]
fn eine_form_aenderung_laesst_fremde_stilschluessel_stehen() {
    let datei = Datei::aus_text(DATEI).unwrap();
    let modell = datei.modell(0).unwrap();
    let mut root = modell.kind("root").unwrap().clone();
    mx::aendern(
        &mut root,
        &[mx::Aenderung {
            id: "a",
            text: None,
            farbe: None,
            form: Some(Form::Zylinder),
            lage: mx::LageAenderung::default(),
            quelle: None,
            ziel: None,
        }],
    )
    .unwrap();
    let eintrag = mx::eintraege(&root)
        .into_iter()
        .find(|eintrag| mx::id(eintrag) == Some("a"))
        .cloned()
        .unwrap();
    let stil = mx::stil(&eintrag);
    assert_eq!(stil.wert("shape"), Some("cylinder3"));
    assert_eq!(stil.wert("sketch"), Some("1"));
    assert_eq!(stil.wert("fillColor"), Some("#e2efff"));
}

#[test]
fn eine_komprimierte_seite_wird_gelesen_und_offen_geschrieben() {
    let modell = mx::leeres_modell();
    let gepackt = link::komprimieren(&modell.als_text()).unwrap();
    let text = format!(
        "<mxfile host=\"test\" compressed=\"true\"><diagram id=\"s1\" name=\"Gepackt\">{gepackt}</diagram></mxfile>"
    );
    let mut datei = Datei::aus_text(&text).unwrap();
    let seite = &datei.seiten().unwrap()[0];
    assert!(seite.komprimiert);
    assert_eq!(seite.zellen, 2);
    assert!(datei.modell(0).unwrap().kind("root").is_some());

    // Zurückschreiben: unkomprimiert, ohne `compressed`.
    datei.modell_setzen(0, mx::leeres_modell()).unwrap();
    let text = datei.als_text();
    assert!(text.contains("<mxGraphModel"), "{text}");
    assert!(!text.contains("compressed"), "{text}");
    assert!(!datei.seiten().unwrap()[0].komprimiert);
}

#[test]
fn eine_kaputte_komprimierte_seite_wird_benannt() {
    let text = "<mxfile><diagram id=\"s1\" name=\"Kaputt\">nicht-base64!</diagram></mxfile>";
    let fehler = Datei::aus_text(text).unwrap_err().to_string();
    assert!(fehler.contains("Base64"), "{fehler}");
}

#[test]
fn kanten_auf_unbekannte_zellen_und_fremde_links_werden_abgelehnt() {
    let kante = "<mxfile><diagram id=\"s\" name=\"x\"><mxGraphModel><root>\
                 <mxCell id=\"0\"/><mxCell id=\"1\" parent=\"0\"/>\
                 <mxCell id=\"k\" edge=\"1\" source=\"fehlt\" parent=\"1\"/>\
                 </root></mxGraphModel></diagram></mxfile>";
    let fehler = Datei::aus_text(kante).unwrap_err().to_string();
    assert!(fehler.contains("gibt es auf der Seite nicht"), "{fehler}");

    let link = "<mxfile><diagram id=\"s\" name=\"x\"><mxGraphModel><root>\
                <mxCell id=\"0\"/><mxCell id=\"1\" parent=\"0\"/>\
                <mxCell id=\"a\" value=\"x\" style=\"rounded=1;link=javascript:alert(1);\" \
                vertex=\"1\" parent=\"1\"/>\
                </root></mxGraphModel></diagram></mxfile>";
    let fehler = Datei::aus_text(link).unwrap_err().to_string();
    assert!(
        fehler.contains("erlaubt sind nur http(s) und mailto"),
        "{fehler}"
    );
}

#[test]
fn der_link_traegt_den_inhalt_nur_im_fragment() {
    let xml = leeres_modell();
    let xml =
        format!("<mxfile host=\"test\"><diagram id=\"s1\" name=\"A\">{xml}</diagram></mxfile>");
    let adresse = link::link(&xml).unwrap();
    assert!(
        adresse.starts_with("https://app.diagrams.net/?grid=0&pv=0&border=10&edit=_blank#R"),
        "{adresse}"
    );
    let (vorn, fragment) = adresse.split_once('#').unwrap();
    assert!(!vorn.contains("mxfile"), "{vorn}");
    assert!(fragment.starts_with('R'));
    let gepackt = link::dekodieren(&fragment[1..]).unwrap();
    assert_eq!(link::entpacken(&gepackt).unwrap(), xml);
}

#[test]
fn ein_link_ueber_einem_megabyte_wird_abgelehnt() {
    let gross = format!("<mxfile>{}</mxfile>", "x".repeat(link::MAX_INHALT));
    let fehler = link::link(&gross).unwrap_err().to_string();
    assert!(fehler.contains("1 MB"), "{fehler}");
}

#[test]
fn gleiche_beschreibung_ergibt_gleiches_dokument() {
    let modell = r#"{"richtung":"unten",
      "knoten":[
        {"id":"start","text":"Antrag","form":"ellipse","farbe":"blau"},
        {"id":"pruefen","text":"Prüfung & Freigabe","form":"raute"},
        {"id":"daten","text":"Kundendaten","form":"zylinder","farbe":"tuerkis","gruppe":"g"},
        {"id":"ende","text":"Fertig","form":"notiz"}],
      "kanten":[
        {"von":"start","nach":"pruefen"},
        {"von":"pruefen","nach":"ende","text":"ja"},
        {"von":"pruefen","nach":"daten","art":"gestrichelt"}],
      "gruppen":[{"id":"g","titel":"Stammdaten"}]}"#;
    let diagramm = quiver_diagramm::Diagramm::aus_json(modell).unwrap();
    let erste = quiver_drawio::aus_modell::seite_aus_diagramm(&diagramm).unwrap();
    let zweite = quiver_drawio::aus_modell::seite_aus_diagramm(&diagramm).unwrap();
    assert_eq!(erste.als_text(), zweite.als_text());
    mx::pruefe_modell(&erste).unwrap();

    // Die Ids des Modells stehen als Zellen-Ids in der Datei.
    let text = erste.als_text();
    for id in ["start", "pruefen", "daten", "ende", "g"] {
        assert!(text.contains(&format!("id=\"{id}\"")), "{id} fehlt: {text}");
    }
    // Der Text wird für HTML (`html=1`) und XML maskiert, wie draw.io es schreibt.
    assert!(text.contains("Prüfung &amp;amp; Freigabe"), "{text}");
}

#[test]
fn ein_modell_mit_gleicher_id_fuer_gruppe_und_knoten_wird_abgelehnt() {
    let modell = r#"{"knoten":[{"id":"a","text":"A","gruppe":"a"}],
                     "gruppen":[{"id":"a","titel":"A"}]}"#;
    let diagramm = quiver_diagramm::Diagramm::aus_json(modell).unwrap();
    let fehler = quiver_drawio::aus_modell::seite_aus_diagramm(&diagramm)
        .unwrap_err()
        .to_string();
    assert!(fehler.contains("dieselbe Id"), "{fehler}");
}
