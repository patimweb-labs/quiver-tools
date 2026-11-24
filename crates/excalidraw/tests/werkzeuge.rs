// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die elf Werkzeuge von Anfang bis Ende (Aufgabe 17).
//!
//! Jeder Test arbeitet in einem **temporären** Ordner; kein Browser wird
//! geöffnet (der Browser liegt hinter einem Trait, der hier zählt statt zu
//! starten). Geprüft wird, was beim Agenten ankommt **und** was auf der Platte
//! steht – die geschriebene Datei muss die Prüfung aus den Formatquellen
//! bestehen.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use quiver_excalidraw::browser::{Browser, KeinBrowser};
use quiver_excalidraw::werkzeug::{Excalidraw, ORDNER, SCHREIBEN};
use quiver_excalidraw::{pruefen, ExcalidrawServer, Zugang};
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::ErrorData as McpError;
use serde_json::{json, Value};
use tempfile::TempDir;

/// Ein Browser, der jeden Aufruf nur mitschreibt.
#[derive(Clone, Default)]
struct ZaehlenderBrowser {
    aufrufe: Arc<Mutex<Vec<String>>>,
}

impl ZaehlenderBrowser {
    fn aufrufe(&self) -> Vec<String> {
        self.aufrufe.lock().expect("Sperre").clone()
    }
}

impl Browser for ZaehlenderBrowser {
    fn oeffnen(&self, url: &str) -> anyhow::Result<()> {
        self.aufrufe.lock().expect("Sperre").push(url.to_string());
        Ok(())
    }
}

fn server_mit(ordner: &Path, schreiben: bool, browser: Arc<dyn Browser>) -> ExcalidrawServer {
    let schluessel = |feld: &str| quiver_tools_kern::umgebung::schluessel("excalidraw", feld);
    let werte: BTreeMap<String, String> = [
        (schluessel(ORDNER), ordner.to_string_lossy().to_string()),
        (
            schluessel(SCHREIBEN),
            if schreiben { "an" } else { "aus" }.to_string(),
        ),
    ]
    .into_iter()
    .collect();
    ExcalidrawServer::mit(
        Zugang::mit_umgebung(&Excalidraw, |name| werte.get(name).cloned()),
        browser,
    )
}

fn server(ordner: &Path, schreiben: bool) -> ExcalidrawServer {
    server_mit(ordner, schreiben, Arc::new(KeinBrowser::neu()))
}

fn aufruf(
    server: &ExcalidrawServer,
    name: &str,
    argumente: Value,
) -> Result<CallToolResult, McpError> {
    let laufzeit = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Laufzeit");
    let objekt = argumente.as_object().cloned().unwrap_or_default();
    laufzeit.block_on(server.aufrufen(name, Some(objekt)))
}

fn text(server: &ExcalidrawServer, name: &str, argumente: Value) -> String {
    let ergebnis = aufruf(server, name, argumente).expect("unerwarteter MCP-Fehler");
    ergebnis
        .content
        .iter()
        .find_map(|inhalt| inhalt.as_text())
        .map(|inhalt| inhalt.text.clone())
        .expect("keine Textantwort")
}

fn fehler(server: &ExcalidrawServer, name: &str, argumente: Value) -> String {
    let fehler = aufruf(server, name, argumente).expect_err("unerwartet kein Fehler");
    fehler.message.to_string()
}

/// Der `stand` aus einer Leseantwort.
fn stand(antwort: &str) -> String {
    antwort
        .lines()
        .find_map(|zeile| zeile.strip_prefix("Stand: "))
        .map(|wert| wert.trim().to_string())
        .unwrap_or_else(|| panic!("kein Stand in: {antwort}"))
}

/// Prüft die Datei auf der Platte gegen die Regeln der Formatquellen.
fn datei_auf_der_platte(ordner: &Path, pfad: &str) -> Value {
    let text = std::fs::read_to_string(ordner.join(pfad)).expect("Datei fehlt");
    let roh: Value = serde_json::from_str(&text).expect("kein JSON");
    pruefen::pruefen(&roh).expect("Datei ist nicht gültig");
    roh
}

fn modell() -> Value {
    json!({
        "richtung": "unten",
        "knoten": [
            { "id": "start", "text": "Antrag", "form": "ellipse", "farbe": "blau" },
            { "id": "pruefen", "text": "Prüfung", "form": "raute", "gruppe": "vorgang" },
            { "id": "ende", "text": "abgelehnt", "farbe": "rot" }
        ],
        "kanten": [
            { "von": "start", "nach": "pruefen", "text": "eingereicht" },
            { "von": "pruefen", "nach": "ende", "art": "gestrichelt" }
        ],
        "gruppen": [ { "id": "vorgang", "titel": "Vorgang" } ]
    })
}

#[test]
fn status_und_liste_auf_leerem_ordner() {
    let ordner = TempDir::new().expect("temporärer Ordner");
    let server = server(ordner.path(), true);

    let status = text(&server, "excalidraw_status", json!({}));
    assert!(status.contains("0 Datei"), "{status}");
    assert!(status.contains("Schreiben ist an"), "{status}");
    assert!(status.contains(&ordner.path().to_string_lossy().to_string()));

    let liste = text(&server, "excalidraw_liste", json!({}));
    assert!(liste.contains("noch keine"), "{liste}");
}

#[test]
fn erstellen_lesen_aendern_rundlauf() {
    let ordner = TempDir::new().expect("temporärer Ordner");
    let server = server(ordner.path(), true);

    let angelegt = text(
        &server,
        "excalidraw_erstellen",
        json!({ "pfad": "plan.excalidraw", "modell": modell() }),
    );
    assert!(angelegt.contains("angelegt"), "{angelegt}");
    assert!(angelegt.contains("knoten-start"), "{angelegt}");
    assert!(angelegt.contains("kante-0"), "{angelegt}");
    let stand_neu = stand(&angelegt);

    // Die Datei auf der Platte ist gültig und vollständig.
    let roh = datei_auf_der_platte(ordner.path(), "plan.excalidraw");
    assert_eq!(roh["type"], "excalidraw");
    assert_eq!(roh["version"], 2);
    assert!(roh["elements"].as_array().unwrap().len() >= 9);

    // Zweimal anlegen geht nicht.
    let fehler_text = fehler(
        &server,
        "excalidraw_erstellen",
        json!({ "pfad": "plan.excalidraw", "modell": modell() }),
    );
    assert!(fehler_text.contains("existiert bereits"), "{fehler_text}");

    let liste = text(&server, "excalidraw_liste", json!({}));
    assert!(liste.contains("plan.excalidraw"), "{liste}");

    let gelesen = text(
        &server,
        "excalidraw_lesen",
        json!({ "pfad": "plan.excalidraw" }),
    );
    assert!(gelesen.contains("„knoten-pruefen“ diamond"), "{gelesen}");
    assert!(gelesen.contains("Text in „knoten-pruefen“"), "{gelesen}");
    assert!(gelesen.contains("von „knoten-start“ nach „knoten-pruefen“"));
    assert_eq!(stand(&gelesen), stand_neu);

    // Rohtext ist da, wo die Beschreibung nicht reicht.
    let roh_text = text(
        &server,
        "excalidraw_lesen",
        json!({ "pfad": "plan.excalidraw", "roh": true, "max_zeichen": 200 }),
    );
    assert!(roh_text.contains("\"type\": \"excalidraw\""), "{roh_text}");
    assert!(roh_text.contains("Zeichen weggelassen"), "{roh_text}");

    let geaendert = text(
        &server,
        "excalidraw_elemente_aendern",
        json!({
            "pfad": "plan.excalidraw",
            "stand": stand_neu,
            "elemente": [
                { "id": "knoten-pruefen", "text": "Freigabe", "farbe": "gruen", "form": "rechteck" },
                { "id": "knoten-ende", "x": 900.0, "y": 40.0 }
            ]
        }),
    );
    assert!(
        geaendert.contains("Text „knoten-pruefen-text“ in „knoten-pruefen“"),
        "{geaendert}"
    );
    assert!(geaendert.contains("Lage von „knoten-ende“"), "{geaendert}");
    let stand_danach = stand(&geaendert);
    assert_ne!(stand_danach, stand_neu);

    let roh = datei_auf_der_platte(ordner.path(), "plan.excalidraw");
    let elemente = roh["elements"].as_array().unwrap();
    let form = elemente
        .iter()
        .find(|element| element["id"] == "knoten-pruefen")
        .unwrap();
    assert_eq!(form["type"], "rectangle");
    let text_id = form["boundElements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|bindung| bindung["type"] == "text")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let inhalt = elemente
        .iter()
        .find(|element| element["id"] == text_id.as_str())
        .unwrap();
    assert_eq!(inhalt["text"], "Freigabe");
    assert_eq!(inhalt["containerId"], "knoten-pruefen");
}

#[test]
fn ein_falscher_stand_schreibt_nichts() {
    let ordner = TempDir::new().expect("temporärer Ordner");
    let server = server(ordner.path(), true);
    text(
        &server,
        "excalidraw_erstellen",
        json!({ "pfad": "plan.excalidraw", "modell": modell() }),
    );
    let vorher = std::fs::read_to_string(ordner.path().join("plan.excalidraw")).unwrap();

    let fehler_text = fehler(
        &server,
        "excalidraw_elemente_aendern",
        json!({
            "pfad": "plan.excalidraw",
            "stand": "0".repeat(64),
            "elemente": [ { "id": "knoten-start", "text": "anders" } ]
        }),
    );
    assert!(fehler_text.contains("inzwischen geändert"), "{fehler_text}");
    let nachher = std::fs::read_to_string(ordner.path().join("plan.excalidraw")).unwrap();
    assert_eq!(vorher, nachher, "die Datei wurde verändert");

    // Ein `stand`, der kein SHA-256 ist, wird eigens gemeldet.
    let fehler_text = fehler(
        &server,
        "excalidraw_elemente_aendern",
        json!({ "pfad": "plan.excalidraw", "stand": "abc", "elemente": [] }),
    );
    assert!(fehler_text.contains("kein SHA-256"), "{fehler_text}");
}

#[test]
fn ohne_schreiben_wird_nichts_angelegt() {
    let ordner = TempDir::new().expect("temporärer Ordner");
    let server = server(ordner.path(), false);

    let hinweis = text(
        &server,
        "excalidraw_erstellen",
        json!({ "pfad": "plan.excalidraw", "modell": modell() }),
    );
    assert!(
        hinweis.contains("Schalter „schreiben“ ist aus"),
        "{hinweis}"
    );
    assert!(!ordner.path().join("plan.excalidraw").exists());

    // Lesen geht weiter.
    let status = text(&server, "excalidraw_status", json!({}));
    assert!(status.contains("Schreiben ist aus"), "{status}");
}

#[test]
fn hinzufuegen_und_entfernen_loesen_die_bindungen() {
    let ordner = TempDir::new().expect("temporärer Ordner");
    let server = server(ordner.path(), true);
    let angelegt = text(
        &server,
        "excalidraw_erstellen",
        json!({ "pfad": "plan.excalidraw", "modell": modell() }),
    );
    let stand_neu = stand(&angelegt);

    let hinzugefuegt = text(
        &server,
        "excalidraw_elemente_hinzufuegen",
        json!({
            "pfad": "plan.excalidraw",
            "stand": stand_neu,
            "elemente": [
                { "art": "rectangle", "id": "notiz", "text": "Neu", "farbe": "gelb", "x": 400.0, "y": 0.0 },
                { "art": "arrow", "von": "notiz", "nach": "knoten-start", "kante": "gestrichelt" }
            ]
        }),
    );
    assert!(
        hinzugefuegt.contains("rectangle „notiz“ angelegt"),
        "{hinzugefuegt}"
    );
    assert!(
        hinzugefuegt.contains("von „notiz“ nach „knoten-start“"),
        "{hinzugefuegt}"
    );
    let stand_danach = stand(&hinzugefuegt);

    let roh = datei_auf_der_platte(ordner.path(), "plan.excalidraw");
    let elemente = roh["elements"].as_array().unwrap();
    let pfeil = elemente
        .iter()
        .find(|element| element["startBinding"]["elementId"] == "notiz")
        .expect("kein Pfeil von „notiz“");
    assert_eq!(pfeil["endBinding"]["elementId"], "knoten-start");

    // Entfernen: der Pfeil verliert das Ende, die Gegenseite auch.
    let entfernt = text(
        &server,
        "excalidraw_elemente_entfernen",
        json!({ "pfad": "plan.excalidraw", "stand": stand_danach, "ids": ["knoten-start"] }),
    );
    assert!(entfernt.contains("entfernt"), "{entfernt}");

    let roh = datei_auf_der_platte(ordner.path(), "plan.excalidraw");
    let elemente = roh["elements"].as_array().unwrap();
    assert!(elemente
        .iter()
        .all(|element| element["id"] != "knoten-start"));
    // Der Text, der in „knoten-start“ lag, lebt frei weiter.
    let freier_text = elemente
        .iter()
        .find(|element| element["id"] == "knoten-start-text")
        .expect("der Text ist verschwunden");
    assert!(freier_text["containerId"].is_null());
    // Der Pfeil hat kein Ende mehr und niemand nennt ihn mehr.
    let pfeil = elemente
        .iter()
        .find(|element| element["id"] == pfeil["id"].as_str().unwrap())
        .unwrap();
    assert!(pfeil["endBinding"].is_null());
    // Nur „notiz“ nennt den Pfeil noch – das andere Ende ist gelöst.
    let genannte: Vec<&str> = elemente
        .iter()
        .filter(|element| {
            element["boundElements"]
                .as_array()
                .is_some_and(|liste| liste.iter().any(|bindung| bindung["id"] == pfeil["id"]))
        })
        .map(|element| element["id"].as_str().unwrap())
        .collect();
    assert_eq!(genannte, vec!["notiz"], "Gegenseite nicht aufgelöst");
}

#[test]
fn ein_unbekanntes_element_wird_beim_bild_genannt() {
    let ordner = TempDir::new().expect("temporärer Ordner");
    let server = server(ordner.path(), true);
    text(
        &server,
        "excalidraw_erstellen",
        json!({ "pfad": "plan.excalidraw", "modell": modell() }),
    );
    let gelesen = text(
        &server,
        "excalidraw_lesen",
        json!({ "pfad": "plan.excalidraw" }),
    );
    let stand_neu = stand(&gelesen);

    // Ein Bild ohne Datei: die Kiste zeichnet einen Platzhalter.
    text(
        &server,
        "excalidraw_elemente_hinzufuegen",
        json!({
            "pfad": "plan.excalidraw",
            "stand": stand_neu,
            "elemente": [ { "art": "image", "id": "bild", "x": 0.0, "y": 400.0 } ]
        }),
    );

    let ergebnis = aufruf(
        &server,
        "excalidraw_bild",
        json!({ "pfad": "plan.excalidraw", "max_kante": 600 }),
    )
    .expect("Bild");
    let arten: Vec<&str> = ergebnis
        .content
        .iter()
        .map(|inhalt| match inhalt {
            ContentBlock::Text(_) => "text",
            ContentBlock::Image(_) => "image",
            _ => "anderes",
        })
        .collect();
    assert_eq!(arten, vec!["text", "image"]);

    let bild_text = ergebnis.content[0].as_text().unwrap().text.clone();
    assert!(bild_text.contains("px"), "{bild_text}");
    match &ergebnis.content[1] {
        ContentBlock::Image(bild) => {
            assert_eq!(bild.mime_type, "image/png");
            assert!(bild.data.len() > 100, "das Bild ist leer");
            assert!(
                bild.data.starts_with("iVBOR"),
                "kein PNG: {}",
                &bild.data[..12]
            );
        }
        sonst => panic!("kein Bild: {sonst:?}"),
    }
}

#[test]
fn exportieren_schreibt_svg_und_png() {
    let ordner = TempDir::new().expect("temporärer Ordner");
    let server = server(ordner.path(), true);
    text(
        &server,
        "excalidraw_erstellen",
        json!({ "pfad": "plan.excalidraw", "modell": modell() }),
    );

    let svg_antwort = text(
        &server,
        "excalidraw_exportieren",
        json!({ "pfad": "plan.excalidraw", "ziel": "ausgabe/plan.svg" }),
    );
    assert!(svg_antwort.contains("plan.svg"), "{svg_antwort}");
    let svg = std::fs::read_to_string(ordner.path().join("ausgabe/plan.svg")).unwrap();
    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("Antrag"));

    text(
        &server,
        "excalidraw_exportieren",
        json!({ "pfad": "plan.excalidraw", "ziel": "plan.png", "max_kante": 400 }),
    );
    let png = std::fs::read(ordner.path().join("plan.png")).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");

    // Nie über Vorhandenes.
    let fehler_text = fehler(
        &server,
        "excalidraw_exportieren",
        json!({ "pfad": "plan.excalidraw", "ziel": "plan.png" }),
    );
    assert!(fehler_text.contains("existiert bereits"), "{fehler_text}");

    // Nur .svg und .png.
    let fehler_text = fehler(
        &server,
        "excalidraw_exportieren",
        json!({ "pfad": "plan.excalidraw", "ziel": "plan.txt" }),
    );
    assert!(fehler_text.contains(".svg oder .png"), "{fehler_text}");
}

#[test]
fn oeffnen_startet_den_browser_nur_wenn_gewuenscht() {
    let ordner = TempDir::new().expect("temporärer Ordner");
    let browser = ZaehlenderBrowser::default();
    let server = server_mit(ordner.path(), true, Arc::new(browser.clone()));
    text(
        &server,
        "excalidraw_erstellen",
        json!({ "pfad": "plan.excalidraw", "modell": modell() }),
    );

    let nur_link = text(
        &server,
        "excalidraw_oeffnen",
        json!({ "pfad": "plan.excalidraw", "nur_link": true }),
    );
    assert!(nur_link.contains("Nichts geöffnet"), "{nur_link}");
    assert!(nur_link.contains("https://excalidraw.com"), "{nur_link}");
    assert!(nur_link.contains("plan.excalidraw"), "{nur_link}");
    assert!(
        browser.aufrufe().is_empty(),
        "es wurde ein Browser geöffnet"
    );

    let geoeffnet = text(
        &server,
        "excalidraw_oeffnen",
        json!({ "pfad": "plan.excalidraw" }),
    );
    assert!(geoeffnet.contains("im Browser geöffnet"), "{geoeffnet}");
    assert!(geoeffnet.contains("Ziehe die Datei"), "{geoeffnet}");
    assert_eq!(browser.aufrufe(), vec!["https://excalidraw.com"]);

    // Eine Datei, die es nicht gibt, wird nicht geöffnet.
    let fehler_text = fehler(
        &server,
        "excalidraw_oeffnen",
        json!({ "pfad": "gibtsnicht.excalidraw" }),
    );
    assert!(fehler_text.contains("gibt es nicht"), "{fehler_text}");
    assert_eq!(browser.aufrufe().len(), 1);
}

#[test]
fn der_stille_browser_ist_eine_meldung_und_kein_absturz() {
    let ordner = TempDir::new().expect("temporärer Ordner");
    let server = server(ordner.path(), true);
    text(
        &server,
        "excalidraw_erstellen",
        json!({ "pfad": "plan.excalidraw", "modell": modell() }),
    );
    let antwort = text(
        &server,
        "excalidraw_oeffnen",
        json!({ "pfad": "plan.excalidraw" }),
    );
    assert!(antwort.contains("ließ sich nicht öffnen"), "{antwort}");
    assert!(antwort.contains("plan.excalidraw"), "{antwort}");
}

#[test]
fn aus_quelltext_prueft_streng_und_erhaelt_unbekanntes() {
    let ordner = TempDir::new().expect("temporärer Ordner");
    let server = server(ordner.path(), true);
    text(
        &server,
        "excalidraw_erstellen",
        json!({ "pfad": "vorlage.excalidraw", "modell": modell() }),
    );
    let mut roh = datei_auf_der_platte(ordner.path(), "vorlage.excalidraw");
    // Ein Mensch hat daran gearbeitet: ein eigenes Feld, ein Bild.
    roh["elements"][0]
        .as_object_mut()
        .unwrap()
        .insert("customData".to_string(), json!({ "quelle": "mensch" }));
    roh["files"] =
        json!({ "bild": { "id": "bild", "mimeType": "image/png", "dataURL": "data:…" } });
    let quelltext = serde_json::to_string_pretty(&roh).unwrap();

    let geschrieben = text(
        &server,
        "excalidraw_aus_quelltext",
        json!({ "pfad": "mensch.excalidraw", "quelltext": quelltext }),
    );
    assert!(geschrieben.contains("geschrieben"), "{geschrieben}");
    let stand_neu = stand(&geschrieben);

    // Ändern erhält das Unbekannte.
    text(
        &server,
        "excalidraw_elemente_hinzufuegen",
        json!({
            "pfad": "mensch.excalidraw",
            "stand": stand_neu,
            "elemente": [ { "art": "text", "text": "dazu" } ]
        }),
    );
    let nachher = datei_auf_der_platte(ordner.path(), "mensch.excalidraw");
    assert_eq!(nachher["files"]["bild"]["dataURL"], "data:…");
    assert_eq!(nachher["elements"][0]["customData"]["quelle"], "mensch");

    // Ein fehlendes Pflichtfeld wird mit Ort gemeldet.
    let mut kaputt = roh.clone();
    kaputt["elements"][1]
        .as_object_mut()
        .unwrap()
        .remove("versionNonce");
    let fehler_text = fehler(
        &server,
        "excalidraw_aus_quelltext",
        json!({ "pfad": "kaputt.excalidraw", "quelltext": serde_json::to_string_pretty(&kaputt).unwrap() }),
    );
    assert!(fehler_text.contains("versionNonce"), "{fehler_text}");
    assert!(!ordner.path().join("kaputt.excalidraw").exists());
}

#[test]
fn pfade_verlassen_den_arbeitsordner_nicht() {
    let ordner = TempDir::new().expect("temporärer Ordner");
    let server = server(ordner.path(), true);
    for pfad in [
        "../ausbruch.excalidraw",
        "/etc/passwd.excalidraw",
        ".versteckt.excalidraw",
    ] {
        let fehler_text = fehler(
            &server,
            "excalidraw_erstellen",
            json!({ "pfad": pfad, "modell": modell() }),
        );
        assert!(
            fehler_text.contains("Arbeitsordner")
                || fehler_text.contains("versteckt")
                || fehler_text.contains("Endung"),
            "{pfad}: {fehler_text}"
        );
    }
    let fehler_text = fehler(
        &server,
        "excalidraw_erstellen",
        json!({ "pfad": "ohne-endung", "modell": modell() }),
    );
    assert!(fehler_text.contains("Endung"), "{fehler_text}");
}

#[test]
fn eine_kaputte_datei_wird_mit_hinweis_auf_roh_gemeldet() {
    let ordner = TempDir::new().expect("temporärer Ordner");
    std::fs::write(ordner.path().join("kaputt.excalidraw"), "{ nicht json").unwrap();
    let server = server(ordner.path(), true);

    let antwort = text(
        &server,
        "excalidraw_lesen",
        json!({ "pfad": "kaputt.excalidraw" }),
    );
    assert!(antwort.contains("kein gültiges JSON"), "{antwort}");
    assert!(antwort.contains("roh: true"), "{antwort}");

    let roh = text(
        &server,
        "excalidraw_lesen",
        json!({ "pfad": "kaputt.excalidraw", "roh": true }),
    );
    assert_eq!(roh, "{ nicht json");
}

#[test]
fn ein_ungueltiges_modell_wird_mit_allen_meldungen_abgelehnt() {
    let ordner = TempDir::new().expect("temporärer Ordner");
    let server = server(ordner.path(), true);
    let fehler_text = fehler(
        &server,
        "excalidraw_erstellen",
        json!({
            "pfad": "plan.excalidraw",
            "modell": { "knoten": [ { "id": "a", "text": "A" }, { "id": "a", "text": "B" } ],
                        "kanten": [ { "von": "a", "nach": "gibtsnicht" } ] }
        }),
    );
    assert!(fehler_text.contains("knoten[1].id"), "{fehler_text}");
    assert!(fehler_text.contains("gibtsnicht"), "{fehler_text}");
    assert!(!ordner.path().join("plan.excalidraw").exists());
}

#[test]
fn farbe_form_und_kante_werden_geprueft() {
    let ordner = TempDir::new().expect("temporärer Ordner");
    let server = server(ordner.path(), true);
    let angelegt = text(
        &server,
        "excalidraw_erstellen",
        json!({ "pfad": "plan.excalidraw", "modell": modell() }),
    );
    let stand_neu = stand(&angelegt);

    let fehler_text = fehler(
        &server,
        "excalidraw_elemente_aendern",
        json!({
            "pfad": "plan.excalidraw", "stand": stand_neu,
            "elemente": [ { "id": "knoten-start", "farbe": "pink" } ]
        }),
    );
    assert!(fehler_text.contains("tuerkis"), "{fehler_text}");

    let fehler_text = fehler(
        &server,
        "excalidraw_elemente_aendern",
        json!({
            "pfad": "plan.excalidraw", "stand": stand_neu,
            "verbindungen": [ { "von": "knoten-start", "nach": "knoten-ende", "art": "wackelig" } ]
        }),
    );
    assert!(fehler_text.contains("gestrichelt"), "{fehler_text}");

    let fehler_text = fehler(
        &server,
        "excalidraw_elemente_aendern",
        json!({
            "pfad": "plan.excalidraw", "stand": stand_neu,
            "elemente": [ { "id": "gibtsnicht", "text": "x" } ]
        }),
    );
    assert!(fehler_text.contains("gibt es nicht"), "{fehler_text}");
}

#[test]
fn verbindungen_werden_angelegt_geaendert_und_geloest() {
    let ordner = TempDir::new().expect("temporärer Ordner");
    let server = server(ordner.path(), true);
    let angelegt = text(
        &server,
        "excalidraw_erstellen",
        json!({ "pfad": "plan.excalidraw", "modell": modell() }),
    );
    let stand_neu = stand(&angelegt);

    let verbunden = text(
        &server,
        "excalidraw_elemente_aendern",
        json!({
            "pfad": "plan.excalidraw", "stand": stand_neu,
            "verbindungen": [ { "von": "knoten-start", "nach": "knoten-ende", "art": "linie" } ]
        }),
    );
    assert!(verbunden.contains("angelegt"), "{verbunden}");
    let stand_zwei = stand(&verbunden);
    datei_auf_der_platte(ordner.path(), "plan.excalidraw");

    let geloest = text(
        &server,
        "excalidraw_elemente_aendern",
        json!({
            "pfad": "plan.excalidraw", "stand": stand_zwei,
            "verbindungen": [ { "von": "knoten-start", "nach": "knoten-ende", "entfernen": true } ]
        }),
    );
    assert!(geloest.contains("entfernt"), "{geloest}");
    let roh = datei_auf_der_platte(ordner.path(), "plan.excalidraw");
    let elemente = roh["elements"].as_array().unwrap();
    let zwischen = elemente.iter().filter(|element| {
        (element["startBinding"]["elementId"] == "knoten-start"
            && element["endBinding"]["elementId"] == "knoten-ende")
            || (element["startBinding"]["elementId"] == "knoten-ende"
                && element["endBinding"]["elementId"] == "knoten-start")
    });
    assert_eq!(zwischen.count(), 0);
}

#[test]
fn das_profil_werkzeug_zeigt_den_ordner_ohne_geheimnis() {
    let ordner = TempDir::new().expect("temporärer Ordner");
    let server = server(ordner.path(), true);
    let antwort = text(&server, "excalidraw_profile", json!({}));
    assert!(antwort.contains("Excalidraw"), "{antwort}");
    assert!(
        antwort.contains(&ordner.path().to_string_lossy().to_string()),
        "{antwort}"
    );
    assert!(antwort.contains("schreiben: an"), "{antwort}");
}
