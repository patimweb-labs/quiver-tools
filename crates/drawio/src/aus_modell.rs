// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Aus der Beschreibung ein draw.io-Dokument bauen.
//!
//! Ein Agent beschreibt ein Diagramm mit Knoten, Kanten, Gruppen und Richtung
//! ([`quiver_diagramm::modell`]); `quiver-diagramm` rechnet daraus ein Layout
//! mit Koordinaten. Dieses Modul übersetzt das Ergebnis in draw.io-Zellen:
//! Gruppen werden Schwimmbahnen, Knoten werden Formen mit Stil und Geometrie,
//! Kanten werden Verbindungen mit ihren Stützpunkten.
//!
//! Die Ids aus dem Modell bleiben die Zellen-Ids – so findet der Agent seine
//! Knoten in der Datei wieder. Kanten bekommen eigene Ids (`kante-1`, …), weil
//! das Modell ihnen keine gibt.

use std::collections::BTreeSet;

use quiver_diagramm::layout::{layout, Zeichnung};
use quiver_diagramm::modell::{Diagramm, Form as ModellForm};
use quiver_diagramm::zeichnen::palette;

use crate::fehler::FormatFehler;
use crate::formen::Form;
use crate::mx::{self, Lage};
use crate::stil::Stil;
use crate::xml::Element;

/// Baut die Seite (ein `mxGraphModel`) aus einem Modell.
pub fn seite_aus_diagramm(diagramm: &Diagramm) -> Result<Element, FormatFehler> {
    let zeichnung = layout(diagramm).map_err(|fehler| FormatFehler::Modell(fehler.to_string()))?;
    for gruppe in &diagramm.gruppen {
        if diagramm.knoten_index(&gruppe.id).is_some() {
            return Err(FormatFehler::Modell(format!(
                "Die Gruppe „{}“ hat dieselbe Id wie ein Knoten – draw.io braucht für jede Zelle \
                 eine eigene Id. Bitte eine der beiden umbenennen.",
                gruppe.id
            )));
        }
    }
    Ok(modell_aus_zeichnung(&zeichnung))
}

/// Die Übersetzung selbst (auch für Tests ohne Modell).
pub fn modell_aus_zeichnung(zeichnung: &Zeichnung) -> Element {
    let mut root = Element::neu("root");
    root.anhaengen(Element::neu("mxCell").mit("id", mx::ROOT_ID));
    root.anhaengen(
        Element::neu("mxCell")
            .mit("id", mx::EBENE_ID)
            .mit("parent", mx::ROOT_ID),
    );

    let mut vergeben: BTreeSet<String> = BTreeSet::new();

    // Gruppen werden Schwimmbahnen; ihre Kinder liegen relativ zu ihnen.
    for gruppe in &zeichnung.gruppen {
        vergeben.insert(gruppe.id.clone());
        let mut stil = Form::Schwimmbahn.stil();
        let toene = palette::toene(None);
        stil.setze("fillColor", toene.fuellung);
        stil.setze("strokeColor", toene.rand);
        let zelle = Element::neu("mxCell")
            .mit("id", gruppe.id.as_str())
            .mit("value", gruppe.titel.as_str())
            .mit("style", stil.als_text())
            .mit("vertex", "1")
            .mit("parent", mx::EBENE_ID);
        root.anhaengen(mit_geometrie(
            zelle,
            &Lage::neu(gruppe.x, gruppe.y, gruppe.breite, gruppe.hoehe),
        ));
    }

    for knoten in &zeichnung.knoten {
        vergeben.insert(knoten.id.clone());
        let (eltern, versatz_x, versatz_y) = match knoten.gruppe.as_ref() {
            Some(gruppe) => (
                gruppe.clone(),
                zeichnung
                    .gruppen
                    .iter()
                    .find(|rahmen| &rahmen.id == gruppe)
                    .map(|rahmen| rahmen.x)
                    .unwrap_or(0.0),
                zeichnung
                    .gruppen
                    .iter()
                    .find(|rahmen| &rahmen.id == gruppe)
                    .map(|rahmen| rahmen.y)
                    .unwrap_or(0.0),
            ),
            None => (mx::EBENE_ID.to_string(), 0.0, 0.0),
        };
        let mut stil = form(knoten.form).stil();
        if let Some(farbe) = knoten.farbe {
            Form::farbe_setzen(&mut stil, farbe);
        }
        let zelle = Element::neu("mxCell")
            .mit("id", knoten.id.as_str())
            .mit("value", knoten.text.as_str())
            .mit("style", stil.als_text())
            .mit("vertex", "1")
            .mit("parent", eltern);
        root.anhaengen(mit_geometrie(
            zelle,
            &Lage::neu(
                knoten.x - versatz_x,
                knoten.y - versatz_y,
                knoten.breite,
                knoten.hoehe,
            ),
        ));
    }

    let mut nummer = 0;
    for kante in &zeichnung.kanten {
        nummer += 1;
        let mut kennung = format!("kante-{nummer}");
        while vergeben.contains(&kennung) {
            nummer += 1;
            kennung = format!("kante-{nummer}");
        }
        vergeben.insert(kennung.clone());

        let mut stil = Stil::aus_text("edgeStyle=orthogonalEdgeStyle;rounded=1;html=1;");
        if kante.art.ist_gestrichelt() {
            stil.setze("dashed", "1");
        }
        if !kante.art.hat_pfeil() {
            stil.setze("endArrow", "none");
        }
        let beschriftung = kante
            .beschriftung
            .as_ref()
            .map(|beschriftung| beschriftung.text.as_str())
            .unwrap_or("");
        let mut zelle = Element::neu("mxCell")
            .mit("id", kennung)
            .mit("value", beschriftung)
            .mit("style", stil.als_text())
            .mit("edge", "1")
            .mit("source", kante.von.as_str())
            .mit("target", kante.nach.as_str())
            .mit("parent", mx::EBENE_ID);
        let mut geometrie = Element::neu("mxGeometry")
            .mit("relative", "1")
            .mit("as", "geometry");
        // Nur die Zwischenpunkte: Anfang und Ende ergeben sich aus den Knoten.
        if kante.punkte.len() > 2 {
            let mut punkte = Element::neu("Array").mit("as", "points");
            for punkt in &kante.punkte[1..kante.punkte.len() - 1] {
                punkte.anhaengen(
                    Element::neu("mxPoint")
                        .mit("x", mx::zahl(punkt.x))
                        .mit("y", mx::zahl(punkt.y)),
                );
            }
            geometrie.anhaengen(punkte);
        }
        zelle.anhaengen(geometrie);
        root.anhaengen(zelle);
    }

    mx::modell_bauen(root, zeichnung.breite, zeichnung.hoehe)
}

fn mit_geometrie(zelle: Element, lage: &Lage) -> Element {
    let mut zelle = zelle;
    let mut geometrie = Element::neu("mxGeometry").mit("as", "geometry");
    geometrie.setze_attribut("x", mx::zahl(lage.x));
    geometrie.setze_attribut("y", mx::zahl(lage.y));
    geometrie.setze_attribut("width", mx::zahl(lage.breite));
    geometrie.setze_attribut("height", mx::zahl(lage.hoehe));
    zelle.anhaengen(geometrie);
    zelle
}

fn form(form: ModellForm) -> Form {
    match form {
        ModellForm::Rechteck => Form::Rechteck,
        ModellForm::Raute => Form::Raute,
        ModellForm::Ellipse => Form::Ellipse,
        ModellForm::Zylinder => Form::Zylinder,
        ModellForm::Notiz => Form::Notiz,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quiver_diagramm::modell::{Farbe, Gruppe, Kante, Knoten, Richtung};

    fn beispiel() -> Diagramm {
        let mut diagramm = Diagramm::leer();
        diagramm.richtung = Richtung::Unten;
        diagramm.gruppen = vec![Gruppe::neu("gruppe", "Vorgang")];
        diagramm.knoten = vec![
            Knoten {
                gruppe: Some("gruppe".to_string()),
                farbe: Some(Farbe::Blau),
                ..Knoten::neu("start", "Antrag & Prüfung")
            },
            Knoten::neu("ende", "Fertig"),
        ];
        diagramm.kanten = vec![Kante {
            text: Some("ja".to_string()),
            ..Kante::neu("start", "ende")
        }];
        diagramm
    }

    #[test]
    fn ein_modell_wird_zu_einer_gueltigen_seite() {
        let modell = seite_aus_diagramm(&beispiel()).unwrap();
        mx::pruefe_modell(&modell).unwrap();
        let root = modell.kind("root").unwrap();
        let zellen = mx::zellen(root);
        assert_eq!(
            zellen
                .iter()
                .map(|zelle| zelle.id.as_str())
                .collect::<Vec<_>>(),
            vec!["gruppe", "start", "ende", "kante-1"]
        );
        assert_eq!(zellen[0].form, Some(Form::Schwimmbahn));
        assert_eq!(zellen[1].form, Some(Form::Rechteck));
        assert_eq!(zellen[1].eltern.as_deref(), Some("gruppe"));
        assert!(zellen[1].text.contains("Antrag & Prüfung"));
        assert_eq!(zellen[3].quelle.as_deref(), Some("start"));
        assert_eq!(zellen[3].ziel.as_deref(), Some("ende"));
        assert_eq!(zellen[3].text, "ja");
        // Kinder liegen relativ zur Gruppe, nicht absolut.
        let gruppe = mx::eintraege(root)
            .into_iter()
            .find(|eintrag| mx::id(eintrag) == Some("gruppe"))
            .cloned()
            .unwrap();
        let kind = mx::eintraege(root)
            .into_iter()
            .find(|eintrag| mx::id(eintrag) == Some("start"))
            .cloned()
            .unwrap();
        assert!(mx::lage(&kind).x < mx::lage(&gruppe).x + mx::lage(&gruppe).breite);
        assert!(mx::lage(&kind).y >= 0.0);
    }

    #[test]
    fn eine_gruppe_mit_knoten_id_wird_abgelehnt() {
        let mut diagramm = beispiel();
        // Ein Knoten, der so heißt wie eine Gruppe: draw.io braucht eindeutige Ids.
        diagramm.gruppen = vec![
            Gruppe::neu("gruppe", "Vorgang"),
            Gruppe::neu("start", "Doppelt"),
        ];
        let fehler = seite_aus_diagramm(&diagramm).unwrap_err().to_string();
        assert!(fehler.contains("dieselbe Id"), "{fehler}");
    }

    #[test]
    fn die_kanten_ids_kollidieren_nicht_mit_knoten() {
        let mut diagramm = beispiel();
        diagramm.knoten.push(Knoten::neu("kante-1", "Zufall"));
        let modell = seite_aus_diagramm(&diagramm).unwrap();
        let ids: Vec<String> = mx::zellen(modell.kind("root").unwrap())
            .iter()
            .map(|zelle| zelle.id.clone())
            .collect();
        // Die echte Kante bekommt die nächste freie Nummer.
        assert!(ids.contains(&"kante-2".to_string()), "{ids:?}");
    }
}
