// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Werkzeuge, wie ein Agent sie benutzt: anlegen, lesen, ändern, zeichnen,
//! verlinken – alles in einem temporären Arbeitsordner, nie im echten.
//!
//! Ein Browser wird nur aufgezeichnet, nie geöffnet ([`Aufzeichner`]).

use std::sync::{Arc, Mutex};

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use quiver_drawio::browser::Browser;
use quiver_drawio::{Drawio, DrawioServer};
use quiver_tools_kern::Zugang;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::ErrorData as McpError;
use serde_json::{json, Value};

/// Ein Browser, der nichts öffnet und die Adresse nur merkt.
#[derive(Default)]
struct Aufzeichner {
    adressen: Mutex<Vec<String>>,
}

impl Browser for Aufzeichner {
    fn oeffnen(&self, adresse: &str) -> Result<(), String> {
        self.adressen
            .lock()
            .expect("Sperre")
            .push(adresse.to_string());
        Ok(())
    }
}

/// Ein Server mit eigenem Arbeitsordner.
struct Umgebung {
    ordner: tempfile::TempDir,
    server: DrawioServer,
    browser: Arc<Aufzeichner>,
}

/// Einen Ordner so anzeigen, wie die Werkzeuge ihn anzeigen: aufgelöst.
///
/// `Path::display()` eines aufgelösten Pfades ist auf jedem System etwas anders
/// (Windows: `\\?\C:\…`, macOS: `/private/var/…`); darum nie mit dem rohen
/// Pfad aus `tempdir()` vergleichen.
fn aufgeloest(pfad: &std::path::Path) -> String {
    std::fs::canonicalize(pfad)
        .unwrap_or_else(|_| pfad.to_path_buf())
        .display()
        .to_string()
}

fn umgebung(schreiben: bool) -> Umgebung {
    let ordner = tempfile::tempdir().expect("temporärer Ordner");
    let pfad = ordner.path().to_string_lossy().to_string();
    let zugang = Zugang::mit_umgebung(&Drawio, move |name| match name {
        "DRAWIO_ORDNER" => Some(pfad.clone()),
        "DRAWIO_SCHREIBEN" => Some(if schreiben { "an" } else { "aus" }.to_string()),
        _ => None,
    });
    let browser = Arc::new(Aufzeichner::default());
    Umgebung {
        server: DrawioServer::mit(zugang, browser.clone()),
        ordner,
        browser,
    }
}

fn aufruf(server: &DrawioServer, name: &str, argumente: Value) -> Result<CallToolResult, McpError> {
    server.aufrufen(name, argumente.as_object().cloned())
}

fn text(ergebnis: Result<CallToolResult, McpError>) -> String {
    let ergebnis = ergebnis.expect("unerwarteter MCP-Fehler");
    ergebnis
        .content
        .iter()
        .find_map(|block| block.as_text().map(|text| text.text.clone()))
        .expect("keine Textantwort")
}

fn antwort(umgebung: &Umgebung, name: &str, argumente: Value) -> String {
    text(aufruf(&umgebung.server, name, argumente))
}

fn fehler(ergebnis: Result<CallToolResult, McpError>) -> String {
    ergebnis
        .expect_err("es wurde ein Fehler erwartet")
        .to_string()
}

/// Der `stand` aus einer Antwort.
fn stand(antwort: &str) -> String {
    for zeile in antwort.lines() {
        if let Some(rest) = zeile.trim().strip_prefix("stand:") {
            let wert = rest.trim();
            if wert.len() == 64 && wert.chars().all(|zeichen| zeichen.is_ascii_hexdigit()) {
                return wert.to_string();
            }
        }
    }
    panic!("kein stand in: {antwort}");
}

/// Ein Modell mit zwei Knoten, einer Kante und einer Gruppe.
const MODELL: &str = r#"{"richtung":"unten",
  "knoten":[
    {"id":"start","text":"Antrag","form":"ellipse","farbe":"blau","gruppe":"vorgang"},
    {"id":"ende","text":"Fertig","farbe":"gruen"}],
  "kanten":[{"von":"start","nach":"ende","text":"ja"}],
  "gruppen":[{"id":"vorgang","titel":"Vorgang"}]}"#;

fn modell() -> Value {
    serde_json::from_str(MODELL).expect("Modell-JSON")
}

fn eine_datei(umgebung: &Umgebung) -> String {
    antwort(
        umgebung,
        "drawio_erstellen",
        json!({ "datei": "ablauf.drawio", "modell": modell() }),
    )
}

// ------------------------------------------------------------------ lesend

#[test]
fn status_und_liste_zeigen_den_arbeitsordner() {
    let umgebung = umgebung(true);
    let status = antwort(&umgebung, "drawio_status", json!({}));
    assert!(status.contains("Schreiben: an"), "{status}");
    // Der Status zeigt den aufgelösten Ordner; darum auch hier auflösen
    // (Windows: `\\?\C:\…`, macOS: `/private/var/…`).
    assert!(
        status.contains(&aufgeloest(umgebung.ordner.path())),
        "{status}"
    );
    assert!(status.contains("Keine Anmeldung"), "{status}");

    let liste = antwort(&umgebung, "drawio_liste", json!({}));
    assert!(liste.contains("Keine .drawio-Datei"), "{liste}");
}

#[test]
fn erstellen_lesen_und_seiten() {
    let umgebung = umgebung(true);
    let erstellt = eine_datei(&umgebung);
    assert!(erstellt.contains("„ablauf.drawio“ angelegt"), "{erstellt}");
    let stand_neu = stand(&erstellt);

    let lesen = antwort(
        &umgebung,
        "drawio_lesen",
        json!({ "datei": "ablauf.drawio" }),
    );
    assert!(lesen.contains(&stand_neu), "{lesen}");
    assert!(lesen.contains("Seite 1 von 1"), "{lesen}");
    assert!(lesen.contains("vorgang"), "Gruppe fehlt: {lesen}");
    assert!(lesen.contains("start"), "{lesen}");
    assert!(lesen.contains("ja"), "{lesen}");

    let roh = antwort(
        &umgebung,
        "drawio_lesen",
        json!({ "datei": "ablauf.drawio", "roh": true, "max_zeichen": 300 }),
    );
    assert!(roh.contains("<mxfile"), "{roh}");
    assert!(roh.contains("Zeichen weggelassen"), "{roh}");

    let seiten = antwort(
        &umgebung,
        "drawio_seiten",
        json!({ "datei": "ablauf.drawio" }),
    );
    assert!(seiten.contains("„ablauf“"), "{seiten}");
    assert!(seiten.contains("unkomprimiert"), "{seiten}");

    let liste = antwort(&umgebung, "drawio_liste", json!({}));
    assert!(liste.contains("ablauf.drawio"), "{liste}");
}

#[test]
fn eine_seite_wird_ueber_id_oder_name_gewaehlt() {
    let umgebung = umgebung(true);
    eine_datei(&umgebung);
    let adresse = antwort(
        &umgebung,
        "drawio_seiten",
        json!({ "datei": "ablauf.drawio" }),
    );
    assert!(adresse.contains("id seite-1"), "{adresse}");
    let per_id = antwort(
        &umgebung,
        "drawio_lesen",
        json!({ "datei": "ablauf.drawio", "seite": "seite-1" }),
    );
    assert!(per_id.contains("Seite 1 von 1"), "{per_id}");
    let per_name = antwort(
        &umgebung,
        "drawio_lesen",
        json!({ "datei": "ablauf.drawio", "seite": "ablauf" }),
    );
    assert!(per_name.contains("Seite 1 von 1"), "{per_name}");
    let unbekannt = fehler(aufruf(
        &umgebung.server,
        "drawio_lesen",
        json!({ "datei": "ablauf.drawio", "seite": "gibt-es-nicht" }),
    ));
    assert!(unbekannt.contains("gibt es nicht"), "{unbekannt}");
}

#[test]
fn ohne_arbeitsordner_kommt_ein_hinweis_und_kein_fehler() {
    let zugang = Zugang::mit_umgebung(&Drawio, |_| None);
    let server = DrawioServer::mit(zugang, Arc::new(Aufzeichner::default()));
    // Kein `expect_err`: der Hinweis ist ein Text, kein MCP-Fehler (SPEC §2).
    let ergebnis = aufruf(&server, "drawio_status", json!({})).expect("kein MCP-Fehler");
    let text = ergebnis.content[0]
        .as_text()
        .expect("Textantwort")
        .text
        .clone();
    assert!(text.contains("Zugang fehlt"), "{text}");
    assert!(text.contains("draw.io"), "{text}");
    assert!(text.contains("quiver-Fenster"), "{text}");
}

#[test]
fn ein_fremdes_quelltext_dokument_wird_geprueft_und_gezeichnet() {
    let umgebung = umgebung(true);
    let quelltext = r#"<mxGraphModel><root>
        <mxCell id="0"/><mxCell id="1" parent="0"/>
        <UserObject id="a" label="Lambda" eigene="1">
          <mxCell style="shape=mxgraph.aws4.lambda;html=1;fillColor=#e2efff;" vertex="1" parent="1">
            <mxGeometry x="40" y="40" width="120" height="80" as="geometry"/>
          </mxCell>
        </UserObject>
        <mxCell id="b" value="Ziel" style="rounded=1;html=1;" vertex="1" parent="1">
          <mxGeometry x="40" y="200" width="120" height="60" as="geometry"/>
        </mxCell>
        <mxCell id="k" value="weiter" style="endArrow=classic;dashed=1;" edge="1" source="a" target="b" parent="1">
          <mxGeometry relative="1" as="geometry"/>
        </mxCell>
      </root></mxGraphModel>"#;
    let erstellt = antwort(
        &umgebung,
        "drawio_aus_quelltext",
        json!({ "datei": "fremd.drawio", "quelltext": quelltext }),
    );
    assert!(erstellt.contains("angelegt"), "{erstellt}");
    assert!(erstellt.contains("1 Seite(n)"), "{erstellt}");

    let bild = antwort(
        &umgebung,
        "drawio_bild",
        json!({ "datei": "fremd.drawio", "max_kante": 400 }),
    );
    assert!(bild.contains("Gezeichnet: 2 Knoten, 1 Kanten"), "{bild}");
    assert!(
        bild.contains("Unbekannte Formen als Rechteck gezeichnet"),
        "{bild}"
    );
    assert!(bild.contains("mxgraph.aws4.lambda"), "{bild}");

    // Ein DTD wird abgelehnt, und es entsteht keine Datei.
    let dtd = fehler(aufruf(
        &umgebung.server,
        "drawio_aus_quelltext",
        json!({
            "datei": "boese.drawio",
            "quelltext": "<!DOCTYPE x [<!ENTITY e SYSTEM \"file:///etc/passwd\">]><mxfile/>"
        }),
    ));
    assert!(dtd.contains("DTD"), "{dtd}");
    assert!(!umgebung.ordner.path().join("boese.drawio").exists());
}

// --------------------------------------------------------------- schreibend

#[test]
fn eine_vorhandene_datei_wird_nie_ueberschrieben() {
    let umgebung = umgebung(true);
    eine_datei(&umgebung);
    let fehler = fehler(aufruf(
        &umgebung.server,
        "drawio_erstellen",
        json!({ "datei": "ablauf.drawio", "modell": modell() }),
    ));
    assert!(fehler.contains("existiert bereits"), "{fehler}");
}

#[test]
fn geaendert_wird_nur_mit_dem_gelesenen_stand() {
    let umgebung = umgebung(true);
    let stand_alt = stand(&eine_datei(&umgebung));

    let geaendert = antwort(
        &umgebung,
        "drawio_elemente_aendern",
        json!({
            "datei": "ablauf.drawio",
            "stand": stand_alt,
            "elemente": [{
                "id": "ende",
                "text": "Abgeschlossen",
                "farbe": "rot",
                "form": "raute",
                "x": 320,
                "y": 240,
                "breite": 160,
                "hoehe": 90
            }]
        }),
    );
    assert!(geaendert.contains("1 Element(e)"), "{geaendert}");
    assert!(geaendert.contains(".bak"), "{geaendert}");
    let stand_neu = stand(&geaendert);
    assert_ne!(stand_neu, stand_alt);

    let lesen = antwort(
        &umgebung,
        "drawio_lesen",
        json!({ "datei": "ablauf.drawio" }),
    );
    assert!(lesen.contains(&stand_neu), "{lesen}");
    assert!(lesen.contains("Abgeschlossen"), "{lesen}");
    assert!(lesen.contains("raute"), "{lesen}");

    // Mit dem alten Stand wird nichts geschrieben.
    let alt = fehler(aufruf(
        &umgebung.server,
        "drawio_elemente_aendern",
        json!({
            "datei": "ablauf.drawio",
            "stand": stand_alt,
            "elemente": [{ "id": "ende", "text": "Nochmal" }]
        }),
    ));
    assert!(alt.contains("inzwischen geändert"), "{alt}");
    let lesen = antwort(
        &umgebung,
        "drawio_lesen",
        json!({ "datei": "ablauf.drawio" }),
    );
    assert!(lesen.contains("Abgeschlossen"), "{lesen}");
}

#[test]
fn hinzufuegen_und_entfernen_mit_ausdruecklicher_angabe() {
    let umgebung = umgebung(true);
    let stand_alt = stand(&eine_datei(&umgebung));

    let hinzugefuegt = antwort(
        &umgebung,
        "drawio_elemente_hinzufuegen",
        json!({
            "datei": "ablauf.drawio",
            "stand": stand_alt,
            "elemente": [
                { "text": "Prüfung", "form": "raute", "x": 240, "y": 40 },
                { "text": "danach", "quelle": "start", "ziel": "n1", "gestrichelt": true }
            ]
        }),
    );
    assert!(hinzugefuegt.contains("Ids: n1, n2"), "{hinzugefuegt}");
    let stand_neu = stand(&hinzugefuegt);

    // Ohne `kanten` wird nichts entfernt.
    let ohne = fehler(aufruf(
        &umgebung.server,
        "drawio_elemente_entfernen",
        json!({ "datei": "ablauf.drawio", "stand": stand_neu, "ids": ["start"] }),
    ));
    assert!(ohne.contains("Kanten"), "{ohne}");
    assert!(ohne.contains("kanten"), "{ohne}");

    // `loesen` behält die Kanten und entfernt nur die Verbindung.
    let entfernt = antwort(
        &umgebung,
        "drawio_elemente_entfernen",
        json!({
            "datei": "ablauf.drawio",
            "stand": stand_neu,
            "ids": ["start"],
            "kanten": "loesen"
        }),
    );
    assert!(entfernt.contains("Entfernt: start"), "{entfernt}");
    let lesen = antwort(
        &umgebung,
        "drawio_lesen",
        json!({ "datei": "ablauf.drawio" }),
    );
    assert!(lesen.contains("? → ende"), "{lesen}");
    assert!(!lesen.contains("start ·"), "{lesen}");

    // Die festen Zellen 0 und 1 bleiben.
    let stand_drei = stand(&entfernt);
    let fest = fehler(aufruf(
        &umgebung.server,
        "drawio_elemente_entfernen",
        json!({ "datei": "ablauf.drawio", "stand": stand_drei, "ids": ["1"] }),
    ));
    assert!(fest.contains("fest"), "{fest}");
}

#[test]
fn eine_seite_wird_angehaengt_und_ausgewaehlt() {
    let umgebung = umgebung(true);
    let stand_alt = stand(&eine_datei(&umgebung));
    let angehaengt = antwort(
        &umgebung,
        "drawio_seite_hinzufuegen",
        json!({
            "datei": "ablauf.drawio",
            "stand": stand_alt,
            "name": "Zweit",
            "modell": { "knoten": [{ "id": "z", "text": "Zweite Seite" }] }
        }),
    );
    assert!(angehaengt.contains("Seite „Zweit“"), "{angehaengt}");
    assert!(angehaengt.contains("jetzt 2 Seiten"), "{angehaengt}");

    let seiten = antwort(
        &umgebung,
        "drawio_seiten",
        json!({ "datei": "ablauf.drawio" }),
    );
    assert!(seiten.contains("2 Seite(n)"), "{seiten}");
    assert!(seiten.contains("„Zweit“"), "{seiten}");

    let zweite = antwort(
        &umgebung,
        "drawio_lesen",
        json!({ "datei": "ablauf.drawio", "seite": "Zweit" }),
    );
    assert!(zweite.contains("Seite 2 von 2"), "{zweite}");
    assert!(zweite.contains("Zweite Seite"), "{zweite}");

    // Eine leere Seite entsteht ohne Modell.
    let stand_zwei = stand(&angehaengt);
    let leer = antwort(
        &umgebung,
        "drawio_seite_hinzufuegen",
        json!({ "datei": "ablauf.drawio", "stand": stand_zwei, "name": "Leer" }),
    );
    assert!(leer.contains("jetzt 3 Seiten"), "{leer}");
}

#[test]
fn schreiben_aus_verhindert_jede_datei_und_laesst_das_lesen_zu() {
    let umgebung = umgebung(false);
    let erstellt = antwort(
        &umgebung,
        "drawio_erstellen",
        json!({ "datei": "ablauf.drawio", "modell": modell() }),
    );
    assert!(erstellt.contains("Schreiben ist aus"), "{erstellt}");
    assert!(!umgebung.ordner.path().join("ablauf.drawio").exists());
    assert!(!umgebung.ordner.path().join("ablauf.drawio.bak").exists());

    // Lesen geht weiterhin – der Ordner wird dabei angelegt.
    let liste = antwort(&umgebung, "drawio_liste", json!({}));
    assert!(liste.contains("Keine .drawio-Datei"), "{liste}");
}

#[test]
fn eine_komprimierte_seite_wird_gelesen_und_offen_geschrieben() {
    let umgebung = umgebung(true);
    let modell = quiver_drawio::mx::leeres_modell().als_text();
    let gepackt = quiver_drawio::link::komprimieren(&modell).expect("packen");
    let quelltext = format!(
        "<mxfile host=\"test\" compressed=\"true\"><diagram id=\"s1\" name=\"Gepackt\">{gepackt}</diagram></mxfile>"
    );
    let erstellt = antwort(
        &umgebung,
        "drawio_aus_quelltext",
        json!({ "datei": "gepackt.drawio", "quelltext": quelltext }),
    );
    assert!(erstellt.contains("angelegt"), "{erstellt}");
    let seiten = antwort(
        &umgebung,
        "drawio_seiten",
        json!({ "datei": "gepackt.drawio" }),
    );
    assert!(seiten.contains("komprimiert"), "{seiten}");

    // Eine Zelle hinzufügen: die Seite wird dabei offen geschrieben.
    let stand_alt = stand(&erstellt);
    let hinzugefuegt = antwort(
        &umgebung,
        "drawio_elemente_hinzufuegen",
        json!({
            "datei": "gepackt.drawio",
            "stand": stand_alt,
            "elemente": [{ "text": "Neu" }]
        }),
    );
    assert!(hinzugefuegt.contains("Ids: n1"), "{hinzugefuegt}");
    let seiten = antwort(
        &umgebung,
        "drawio_seiten",
        json!({ "datei": "gepackt.drawio" }),
    );
    assert!(seiten.contains("unkomprimiert"), "{seiten}");
}

// -------------------------------------------------------------- Bild, Link

#[test]
fn das_bild_kommt_als_png_zurueck() {
    let umgebung = umgebung(true);
    eine_datei(&umgebung);
    let ergebnis = aufruf(
        &umgebung.server,
        "drawio_bild",
        json!({ "datei": "ablauf.drawio", "max_kante": 300 }),
    )
    .expect("kein Fehler");
    let bild = ergebnis
        .content
        .iter()
        .find_map(|block| match block {
            ContentBlock::Image(bild) => Some(bild),
            _ => None,
        })
        .expect("kein Bildinhalt");
    assert_eq!(bild.mime_type, "image/png");
    let daten = STANDARD.decode(&bild.data).expect("Base64");
    assert_eq!(&daten[..8], b"\x89PNG\r\n\x1a\n");
    // Und die Antwort nennt, was gezeichnet wurde.
    let text = ergebnis
        .content
        .iter()
        .find_map(|block| block.as_text().map(|text| text.text.clone()))
        .expect("kein Text");
    assert!(
        text.contains("Gezeichnet: 3 Knoten (davon 1 Container), 1 Kanten"),
        "{text}"
    );
}

#[test]
fn export_schreibt_svg_und_png_und_nie_ueber_vorhandenes() {
    let umgebung = umgebung(true);
    eine_datei(&umgebung);
    let svg = antwort(
        &umgebung,
        "drawio_exportieren",
        json!({ "datei": "ablauf.drawio", "ausgabe": "ablauf.svg" }),
    );
    assert!(svg.contains("„ablauf.svg“ geschrieben"), "{svg}");
    let inhalt = std::fs::read_to_string(umgebung.ordner.path().join("ablauf.svg")).unwrap();
    assert!(inhalt.starts_with("<svg"), "{inhalt}");
    assert!(inhalt.contains("<ellipse"), "{inhalt}");

    let png = antwort(
        &umgebung,
        "drawio_exportieren",
        json!({ "datei": "ablauf.drawio", "ausgabe": "ablauf.png", "max_kante": 200 }),
    );
    assert!(png.contains("„ablauf.png“ geschrieben"), "{png}");
    let daten = std::fs::read(umgebung.ordner.path().join("ablauf.png")).unwrap();
    assert_eq!(&daten[..8], b"\x89PNG\r\n\x1a\n");

    let nochmal = fehler(aufruf(
        &umgebung.server,
        "drawio_exportieren",
        json!({ "datei": "ablauf.drawio", "ausgabe": "ablauf.svg" }),
    ));
    assert!(nochmal.contains("existiert bereits"), "{nochmal}");

    let falsch = fehler(aufruf(
        &umgebung.server,
        "drawio_exportieren",
        json!({ "datei": "ablauf.drawio", "ausgabe": "ablauf.txt" }),
    ));
    assert!(falsch.contains(".svg"), "{falsch}");
}

#[test]
fn der_link_ist_ein_text_und_das_oeffnen_geht_durch_die_attrappe() {
    let umgebung = umgebung(true);
    eine_datei(&umgebung);
    let link = antwort(
        &umgebung,
        "drawio_link",
        json!({ "datei": "ablauf.drawio" }),
    );
    assert!(link.contains("ganze Diagramm"), "{link}");
    let adresse = link.lines().last().expect("Adresse").trim();
    assert!(
        adresse.starts_with("https://app.diagrams.net/?grid=0&pv=0&border=10&edit=_blank#R"),
        "{adresse}"
    );

    let geoeffnet = antwort(
        &umgebung,
        "drawio_oeffnen",
        json!({ "datei": "ablauf.drawio" }),
    );
    assert!(
        geoeffnet.contains("landen NICHT in der lokalen Datei"),
        "{geoeffnet}"
    );
    assert!(geoeffnet.contains("ablauf.drawio"), "{geoeffnet}");
    // Genau der selbst gebaute Link, kein anderer.
    assert_eq!(
        umgebung.browser.adressen.lock().unwrap().clone(),
        vec![adresse.to_string()]
    );
}

/// Ein fester Zufall für unkomprimierbaren Text (kein Zusatz-Crate).
fn zufallstext(start: u64, laenge: usize) -> String {
    let mut zustand = start;
    let mut text = String::with_capacity(laenge);
    for _ in 0..laenge {
        zustand = zustand
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let buchstabe = ((zustand >> 33) % 26) as u8;
        text.push(char::from(b'a' + buchstabe));
    }
    text
}

#[test]
fn ein_zu_langer_link_nennt_den_pfad_statt_eines_fehlers() {
    let umgebung = umgebung(true);
    let mut zellen = String::new();
    for nummer in 0..120u64 {
        zellen.push_str(&format!(
            r#"<mxCell id="n{nummer}" value="{}" style="rounded=1;html=1;" vertex="1" parent="1"><mxGeometry x="{}" y="{}" width="120" height="60" as="geometry"/></mxCell>"#,
            zufallstext(nummer + 1, 60),
            nummer * 5,
            nummer * 5
        ));
    }
    let quelltext = format!(
        "<mxGraphModel><root><mxCell id=\"0\"/><mxCell id=\"1\" parent=\"0\"/>{zellen}</root></mxGraphModel>"
    );
    antwort(
        &umgebung,
        "drawio_aus_quelltext",
        json!({ "datei": "gross.drawio", "quelltext": quelltext }),
    );

    let meldung = antwort(
        &umgebung,
        "drawio_oeffnen",
        json!({ "datei": "gross.drawio" }),
    );
    assert!(meldung.contains("zu lang für den Startbefehl"), "{meldung}");
    assert!(meldung.contains("gross.drawio"), "{meldung}");
    assert!(meldung.contains("Geöffnet wurde nichts"), "{meldung}");
    assert!(
        umgebung.browser.adressen.lock().unwrap().is_empty(),
        "der Browser wurde trotzdem geöffnet"
    );
}
