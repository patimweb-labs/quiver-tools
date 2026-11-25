// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Base64, raw inflate und Prozent-Kodierung – das Verfahren aus `index.js`.
//!
//! draw.io trägt ein Diagramm im Fragment einer Adresse:
//! `https://app.diagrams.net/#R<…>`. Der Inhalt ist
//! `encodeURIComponent(XML)` → raw deflate → Base64, und das Ganze noch einmal
//! prozentkodiert. Weil das Fragment nie an einen Server geht, bleibt das
//! Diagramm auf dem Rechner; genau darum gibt es diesen Link.
//!
//! Dieselbe Kodierung benutzt draw.io für **komprimierte Seiten** in einer
//! Datei (`<diagram>` mit Textinhalt statt `<mxGraphModel>`). Lesen kann dieses
//! Werkzeug das (höchstens 20 MB entpackt), schreiben tut es unkomprimiert.

use std::io::{Read, Write};

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use flate2::Compression;

use crate::fehler::FormatFehler;

/// Adresse von draw.io im Netz; nur zum Öffnen, nie zum Hochladen.
pub const BASIS: &str = "https://app.diagrams.net/";

/// Anzeigeparameter aus `index.js`: kein Gitter, keine Vorschau, Rahmen,
/// Bearbeitung im selben Fenster.
pub const PARAMETER: &str = "?grid=0&pv=0&border=10&edit=_blank";

/// Höchstgröße des Inhalts, der in einen Link passt: 1 MB (Aufgabe 18).
pub const MAX_INHALT: usize = 1024 * 1024;

/// Höchstgröße einer entpackten Seite: 20 MB (Aufgabe 18).
pub const MAX_ENTPACKT: usize = 20 * 1024 * 1024;

/// Grenze, bis zu der ein Betriebssystem eine Adresse aus einem Startbefehl
/// zuverlässig annimmt (Windows: `INTERNET_MAX_URL_LENGTH`). Darüber nennt das
/// Werkzeug den Dateipfad statt zu öffnen.
pub const MAX_ADRESSE: usize = 2000;

/// Der Link auf das **ganze** Diagramm (alle Seiten, Inhalt nur im Fragment).
pub fn link(xml: &str) -> Result<String, FormatFehler> {
    if xml.len() > MAX_INHALT {
        return Err(FormatFehler::Sonstiges(format!(
            "Der Inhalt ist {} Bytes groß – ein Link trägt höchstens 1 MB. Bitte die Datei \
             verkleinern (z. B. eine Seite entfernen) oder sie im Browser über die Datei öffnen.",
            xml.len()
        )));
    }
    Ok(format!(
        "{BASIS}{PARAMETER}#R{}",
        kodieren(&komprimieren(xml)?)
    ))
}

/// `encodeURIComponent`: alles außer den unreservierten Zeichen wird prozentkodiert.
pub fn kodieren(text: &str) -> String {
    let mut aus = String::with_capacity(text.len());
    for byte in text.as_bytes() {
        let zeichen = char::from(*byte);
        let erlaubt = zeichen.is_ascii_alphanumeric()
            || matches!(
                zeichen,
                '-' | '_' | '.' | '!' | '~' | '*' | '\'' | '(' | ')'
            );
        if erlaubt {
            aus.push(zeichen);
        } else {
            aus.push_str(&format!("%{byte:02X}"));
        }
    }
    aus
}

/// Gegenstück zu [`kodieren`]; ein `%` ohne zwei Hexzeichen ist ein Fehler.
pub fn dekodieren(text: &str) -> Result<String, FormatFehler> {
    if !text.contains('%') {
        return Ok(text.to_string());
    }
    let bytes = text.as_bytes();
    let mut aus = Vec::with_capacity(bytes.len());
    let mut stelle = 0;
    while stelle < bytes.len() {
        if bytes[stelle] == b'%' {
            let hex = bytes
                .get(stelle + 1..stelle + 3)
                .and_then(|hex| std::str::from_utf8(hex).ok())
                .ok_or_else(|| {
                    FormatFehler::Komprimierung(
                        "Die Seite ist nicht lesbar: „%“ ohne zwei Hexzeichen.".to_string(),
                    )
                })?;
            let wert = u8::from_str_radix(hex, 16).map_err(|_| {
                FormatFehler::Komprimierung(format!(
                    "Die Seite ist nicht lesbar: „%{hex}“ ist keine Prozentkodierung."
                ))
            })?;
            aus.push(wert);
            stelle += 3;
        } else {
            aus.push(bytes[stelle]);
            stelle += 1;
        }
    }
    String::from_utf8(aus).map_err(|_| {
        FormatFehler::Komprimierung(
            "Die entpackte Seite ist kein UTF-8-Text – sie lässt sich nicht lesen.".to_string(),
        )
    })
}

/// `encodeURIComponent(XML)` → raw deflate → Base64.
pub fn komprimieren(xml: &str) -> Result<String, FormatFehler> {
    let kodiert = kodieren(xml);
    let mut schreiber = DeflateEncoder::new(Vec::new(), Compression::default());
    schreiber.write_all(kodiert.as_bytes()).map_err(|fehler| {
        FormatFehler::Komprimierung(format!("Der Inhalt ließ sich nicht packen: {fehler}"))
    })?;
    let gepackt = schreiber.finish().map_err(|fehler| {
        FormatFehler::Komprimierung(format!("Der Inhalt ließ sich nicht packen: {fehler}"))
    })?;
    Ok(STANDARD.encode(gepackt))
}

/// Base64 → raw inflate → Prozentdekodierung; höchstens [`MAX_ENTPACKT`].
pub fn entpacken(basis64: &str) -> Result<String, FormatFehler> {
    let sauber: String = basis64
        .chars()
        .filter(|zeichen| !zeichen.is_whitespace())
        .collect();
    let roh = STANDARD.decode(sauber.as_bytes()).map_err(|fehler| {
        FormatFehler::Komprimierung(format!("Die Seite ist kein gültiges Base64: {fehler}"))
    })?;

    let mut leser = DeflateDecoder::new(roh.as_slice());
    let mut aus = Vec::new();
    leser
        .by_ref()
        .take(MAX_ENTPACKT as u64 + 1)
        .read_to_end(&mut aus)
        .map_err(|fehler| {
            FormatFehler::Komprimierung(format!(
                "Die Seite ließ sich nicht entpacken (raw inflate): {fehler}"
            ))
        })?;
    if aus.len() > MAX_ENTPACKT {
        return Err(FormatFehler::Sonstiges(format!(
            "Die Seite ist entpackt größer als 20 MB ({} Bytes) – bitte die Seite verkleinern.",
            MAX_ENTPACKT
        )));
    }
    let text = String::from_utf8(aus).map_err(|_| {
        FormatFehler::Komprimierung(
            "Die entpackte Seite ist kein UTF-8-Text – sie lässt sich nicht lesen.".to_string(),
        )
    })?;
    dekodieren(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prozentkodierung_ist_encode_uri_component() {
        assert_eq!(kodieren("a b"), "a%20b");
        assert_eq!(kodieren("-_.!~*'()"), "-_.!~*'()");
        assert_eq!(kodieren("ä"), "%C3%A4");
        assert_eq!(kodieren("<a&b>"), "%3Ca%26b%3E");
        assert_eq!(kodieren("%"), "%25");
        assert_eq!(dekodieren("a%20b").unwrap(), "a b");
        assert_eq!(dekodieren("%C3%A4").unwrap(), "ä");
        assert_eq!(dekodieren("+").unwrap(), "+");
        assert!(dekodieren("%zz").is_err());
        assert!(dekodieren("%2").is_err());
        assert_eq!("äöü & <>\n", dekodieren(&kodieren("äöü & <>\n")).unwrap());
    }

    #[test]
    fn packen_und_entpacken_ist_derselbe_inhalt() {
        let xml = r#"<mxfile host="test"><diagram id="s1" name="Start"><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="2" value="Prüfung &amp; Freigabe" vertex="1" parent="1"/></root></mxGraphModel></diagram></mxfile>"#;
        let gepackt = komprimieren(xml).unwrap();
        assert_eq!(entpacken(&gepackt).unwrap(), xml);
        // Leerraum im Base64 (draw.io schreibt umbrochene Zeilen) stört nicht.
        let umbrochen = gepackt
            .as_bytes()
            .chunks(60)
            .map(|stueck| String::from_utf8_lossy(stueck).to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(entpacken(&umbrochen).unwrap(), xml);
    }

    #[test]
    fn ein_leerer_inhalt_packt_zu_leerem_entpackten() {
        let gepackt = komprimieren("").unwrap();
        assert_eq!(entpacken(&gepackt).unwrap(), "");
    }

    #[test]
    fn der_link_traegt_den_inhalt_nur_im_fragment() {
        let xml = "<mxfile/>";
        let link = link(xml).unwrap();
        assert!(link.starts_with("https://app.diagrams.net/?grid=0&pv=0&border=10&edit=_blank#R"));
        // Vor dem Fragment steht nichts vom Inhalt.
        let (vorn, fragment) = link.split_once('#').unwrap();
        assert!(!vorn.contains("mxfile"));
        assert!(fragment.starts_with('R'));
        // Und aus dem Fragment lässt sich der Inhalt zurückholen.
        let gepackt = dekodieren(&fragment[1..]).unwrap();
        assert_eq!(entpacken(&gepackt).unwrap(), xml);
        // Der Inhalt ist prozentkodiert, damit das Fragment in einer Adresse hält.
        assert!(!link.contains('<'));
        assert!(!link.contains('"'));
        assert!(!link.contains(' '));
    }

    #[test]
    fn ein_zu_grosser_inhalt_wird_abgelehnt() {
        let gross = "x".repeat(MAX_INHALT + 1);
        let fehler = link(&gross).unwrap_err();
        assert!(fehler.to_string().contains("1 MB"), "{fehler}");
    }

    #[test]
    fn eine_zu_grosse_entpackte_seite_wird_abgelehnt() {
        // 20 MB + 1 Byte Nullen packen sehr klein; das Entpacken muss die Grenze halten.
        let gross = vec![b'x'; MAX_ENTPACKT + 1];
        let mut schreiber = DeflateEncoder::new(Vec::new(), Compression::default());
        schreiber.write_all(&gross).unwrap();
        let gepackt = STANDARD.encode(schreiber.finish().unwrap());
        let fehler = entpacken(&gepackt).unwrap_err();
        assert!(fehler.to_string().contains("20 MB"), "{fehler}");
    }
}
