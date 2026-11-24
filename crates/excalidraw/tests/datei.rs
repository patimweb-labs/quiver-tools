// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Das Format gegen die Quellen geprüft (Aufgabe 17).
//!
//! Die Prüfung hier ist **aus den Formatquellen abgeleitet** und läuft gegen das
//! rohe JSON einer erzeugten Datei – nicht gegen die großzügig gelesene
//! Struktur. Sie verlangt alle Pflichtfelder und Bindungen auf beiden Seiten.

use quiver_diagramm::modell::{Diagramm, Farbe, Form, Gruppe, Kante, KanteArt, Knoten, Richtung};
use quiver_excalidraw::datei::{Datei, VERSION};
use quiver_excalidraw::{aendern, bauen, pruefen};
use serde_json::Value;

/// Ein Modell mit allen Formen, allen Farben, Gruppen und beiden Kantenarten.
fn volles_modell() -> Diagramm {
    let formen = [
        Form::Rechteck,
        Form::Raute,
        Form::Ellipse,
        Form::Zylinder,
        Form::Notiz,
    ];
    let farben = Farbe::ALLE;
    let mut diagramm = Diagramm::leer();
    diagramm.richtung = Richtung::Rechts;
    diagramm.knoten = (0..40)
        .map(|i| Knoten {
            form: formen[i % formen.len()],
            farbe: Some(farben[i % farben.len()]),
            gruppe: if i % 4 == 0 {
                Some("vorgang".to_string())
            } else {
                None
            },
            ..Knoten::neu(format!("k{i}"), format!("Knoten {i}\nzweite Zeile"))
        })
        .collect();
    diagramm.kanten = (0..39)
        .map(|i| Kante {
            text: Some(format!("Kante {i}")),
            art: match i % 3 {
                0 => KanteArt::Pfeil,
                1 => KanteArt::Linie,
                _ => KanteArt::Gestrichelt,
            },
            ..Kante::neu(format!("k{}", i % 8), format!("k{}", (i % 20) + 8))
        })
        .collect();
    diagramm.gruppen = vec![Gruppe::neu("vorgang", "Vorgang")];
    diagramm
}

fn roh(datei: &Datei) -> Value {
    serde_json::to_value(datei).expect("Datei ist JSON")
}

#[test]
fn die_erzeugte_datei_haelt_die_regeln_der_quellen_ein() {
    let datei = bauen::aus_diagramm(&volles_modell()).expect("Modell wird gezeichnet");
    let json = roh(&datei);
    pruefen::pruefen(&json).expect("Pflichtfelder und Bindungen stimmen");

    // Kopf der Datei.
    assert_eq!(json["type"], "excalidraw");
    assert_eq!(json["version"], VERSION);
    assert!(json["source"].as_str().unwrap().starts_with("https://"));
    assert!(json["appState"].is_object());
    assert!(json["files"].is_object());

    // Jedes Element trägt wirklich jedes Pflichtfeld.
    let elemente = json["elements"].as_array().expect("Elemente");
    assert!(!elemente.is_empty());
    for element in elemente {
        for feld in pruefen::PFLICHTFELDER {
            assert!(
                element.get(*feld).is_some(),
                "Pflichtfeld „{feld}“ fehlt: {element}"
            );
        }
        // `link` ist bei uns null oder eine erlaubte Adresse.
        match element.get("link").and_then(Value::as_str) {
            None => {}
            Some(link) => assert!(
                pruefen::LINK_SCHEMATA
                    .iter()
                    .any(|schema| link.starts_with(schema)),
                "{link}"
            ),
        }
    }
}

#[test]
fn jeder_knotentext_steht_auf_beiden_seiten_der_bindung() {
    let datei = bauen::aus_diagramm(&volles_modell()).expect("Modell");
    let json = roh(&datei);
    let elemente = json["elements"].as_array().unwrap();

    let mut texte = 0;
    for element in elemente {
        let id = element["id"].as_str().unwrap();
        if let Some(container) = element.get("containerId").and_then(Value::as_str) {
            let behaelter = elemente
                .iter()
                .find(|kandidat| kandidat["id"] == container)
                .expect("Behälter existiert");
            let genannt = behaelter["boundElements"]
                .as_array()
                .expect("boundElements")
                .iter()
                .any(|bindung| bindung["type"] == "text" && bindung["id"] == id);
            assert!(genannt, "„{container}“ nennt den Text „{id}“ nicht");
            texte += 1;
        }
    }
    assert!(texte >= 40, "zu wenige gebundene Texte: {texte}");
}

#[test]
fn der_text_steht_hinter_seiner_form_in_der_datei() {
    // Excalidraw zeichnet in der Reihenfolge der Liste: der Text muss nach der
    // Form stehen, sonst liegt er darunter und ist unsichtbar.
    let datei = bauen::aus_diagramm(&volles_modell()).expect("Modell");
    let elemente = roh(&datei);
    let elemente = elemente["elements"].as_array().unwrap();
    let mut gesehen = std::collections::HashSet::new();
    for element in elemente {
        let id = element["id"].as_str().unwrap();
        if element["type"] == "text" {
            if let Some(container) = element.get("containerId").and_then(Value::as_str) {
                assert!(
                    gesehen.contains(container),
                    "Text „{id}“ steht vor seiner Form „{container}“"
                );
            }
        }
        gesehen.insert(id);
    }
}

#[test]
fn jeder_pfeil_steht_auf_beiden_seiten_der_bindung() {
    let datei = bauen::aus_diagramm(&volles_modell()).expect("Modell");
    let json = roh(&datei);
    let elemente = json["elements"].as_array().unwrap();

    let mut pfeile = 0;
    for element in elemente {
        let id = element["id"].as_str().unwrap();
        for feld in ["startBinding", "endBinding"] {
            let Some(ziel) = element.get(feld).and_then(|b| b.get("elementId")) else {
                continue;
            };
            let ziel = ziel.as_str().expect("elementId ist Text");
            let andere = elemente
                .iter()
                .find(|kandidat| kandidat["id"] == ziel)
                .unwrap_or_else(|| panic!("„{ziel}“ gibt es nicht"));
            let genannt = andere["boundElements"]
                .as_array()
                .expect("boundElements")
                .iter()
                .any(|bindung| bindung["type"] == "arrow" && bindung["id"] == id);
            assert!(genannt, "„{ziel}“ nennt den Pfeil „{id}“ nicht");
            pfeile += 1;
        }
    }
    assert!(pfeile >= 39, "zu wenige Bindungen: {pfeile}");
}

#[test]
fn unbekannte_felder_und_elemente_ueberleben_eine_aenderung() {
    let mut datei = bauen::aus_diagramm(&volles_modell()).expect("Modell");
    // So, wie ein Mensch es in Excalidraw angelegt hätte: ein Element, das
    // dieses Werkzeug nicht kennt, ein Bild und ein unbekanntes Feld.
    datei.files.insert(
        "bild-1".to_string(),
        serde_json::json!({ "id": "bild-1", "mimeType": "image/png", "dataURL": "data:image/png;base64,AA" }),
    );
    datei
        .app_state
        .insert("scrollX".to_string(), serde_json::json!(123));
    let mut fremd = quiver_excalidraw::Element::neu("embeddable", 10.0, 10.0, 100.0, 100.0);
    fremd.id = "fremd".to_string();
    fremd.weitere.insert(
        "customData".to_string(),
        serde_json::json!({ "quelle": "mensch" }),
    );
    datei.hinzufuegen(fremd).expect("fremdes Element");

    aendern::text_setzen(&mut datei, "knoten-k0", "geändert").expect("Text");
    aendern::verschieben(
        &mut datei,
        "knoten-k1",
        Some(500.0),
        Some(600.0),
        None,
        None,
    )
    .expect("verschieben");

    let json = roh(&datei);
    pruefen::pruefen(&json).expect("nach der Änderung weiter gültig");
    assert_eq!(
        json["files"]["bild-1"]["dataURL"],
        "data:image/png;base64,AA"
    );
    assert_eq!(json["appState"]["scrollX"], 123);
    let fremd = json["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|element| element["id"] == "fremd")
        .expect("das fremde Element fehlt");
    assert_eq!(fremd["customData"]["quelle"], "mensch");
    assert_eq!(fremd["type"], "embeddable");
}

#[test]
fn unberuehrte_elemente_bleiben_inhaltsgleich() {
    let datei = bauen::aus_diagramm(&volles_modell()).expect("Modell");
    let vorher = roh(&datei);
    let mut nachher_datei = datei.clone();

    aendern::text_setzen(&mut nachher_datei, "knoten-k0", "nur dieser Text").expect("Text");
    let nachher = roh(&nachher_datei);

    let elemente_vorher = vorher["elements"].as_array().unwrap();
    let elemente_nachher = nachher["elements"].as_array().unwrap();
    assert_eq!(elemente_vorher.len(), elemente_nachher.len());
    for (alt, neu) in elemente_vorher.iter().zip(elemente_nachher.iter()) {
        let betroffen = alt["id"] == "knoten-k0"
            || alt["id"] == "knoten-k0-text"
            || alt["containerId"] == "knoten-k0";
        if betroffen {
            continue;
        }
        assert_eq!(alt, neu, "„{}“ wurde mitverändert", alt["id"]);
    }
    // Nur die betroffenen Elemente tragen eine neue Version.
    let version = |json: &Value, id: &str| {
        json["elements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|element| element["id"] == id)
            .unwrap()["version"]
            .clone()
    };
    assert_ne!(
        version(&vorher, "knoten-k0-text"),
        version(&nachher, "knoten-k0-text")
    );
    assert_eq!(
        version(&vorher, "knoten-k1"),
        version(&nachher, "knoten-k1")
    );
}

#[test]
fn ein_kaputter_link_und_fehlende_pflichtfelder_werden_gemeldet() {
    let datei = bauen::aus_diagramm(&volles_modell()).expect("Modell");
    let mut json = roh(&datei);

    json["elements"][0]["link"] = serde_json::json!("javascript:alert(1)");
    json["elements"][1]
        .as_object_mut()
        .unwrap()
        .remove("versionNonce");
    let meldungen = pruefen::pruefen(&json).expect_err("zwei Verstöße");
    let text = pruefen::meldungstext(&meldungen);
    assert!(text.contains("mailto"), "{text}");
    assert!(text.contains("versionNonce"), "{text}");
}

#[test]
fn eine_datei_ohne_pflichtteile_wird_abgelehnt() {
    for kaputt in [
        serde_json::json!({}),
        serde_json::json!({ "type": "excalidraw", "version": 2 }),
        serde_json::json!({ "type": "drawio", "version": 2, "source": "x", "elements": [], "appState": {}, "files": {} }),
        serde_json::json!({ "type": "excalidraw", "version": 1, "source": "x", "elements": [], "appState": {}, "files": {} }),
    ] {
        assert!(
            pruefen::pruefen(&kaputt).is_err(),
            "unerwartet gültig: {kaputt}"
        );
    }
}

#[test]
fn das_lesen_ist_grosszuegiger_als_die_pruefung() {
    // Excalidraw selbst füllt fehlende Felder beim Laden; `Datei::aus_text`
    // macht dasselbe, damit ein altes Bild nicht unlesbar ist.
    let text = r#"{"type":"excalidraw","version":2,"source":"x",
        "elements":[{"id":"a","type":"rectangle","x":0,"y":0,"width":10,"height":10}],
        "appState":{},"files":{}}"#;
    let datei = Datei::aus_text(text).expect("wird gelesen");
    assert_eq!(datei.elements[0].width, 10.0);
    assert_eq!(datei.elements[0].version, 1);
    assert!(datei.elements[0].bound_elements.is_none());
    // Die strenge Prüfung würde dieselbe Datei ablehnen.
    let json: Value = serde_json::from_str(text).unwrap();
    assert!(pruefen::pruefen(&json).is_err());
}

#[test]
fn formen_und_farben_stehen_als_excalidraw_werte_in_der_datei() {
    let mut diagramm = Diagramm::leer();
    diagramm.knoten = vec![
        Knoten {
            form: Form::Raute,
            farbe: Some(Farbe::Rot),
            ..Knoten::neu("a", "A")
        },
        Knoten {
            form: Form::Ellipse,
            ..Knoten::neu("b", "B")
        },
    ];
    diagramm.kanten = vec![Kante {
        art: KanteArt::Gestrichelt,
        ..Kante::neu("a", "b")
    }];
    let datei = bauen::aus_diagramm(&diagramm).unwrap();
    let a = datei.element("knoten-a").unwrap();
    assert_eq!(a.art, "diamond");
    assert_eq!(
        a.background_color,
        quiver_diagramm::zeichnen::palette::toene(Some(Farbe::Rot)).fuellung
    );
    assert_eq!(datei.element("knoten-b").unwrap().art, "ellipse");
    let pfeil = datei.element("kante-0").unwrap();
    assert_eq!(pfeil.stroke_style, "dashed");
    assert_eq!(pfeil.end_arrowhead, Some(serde_json::json!("arrow")));
}
