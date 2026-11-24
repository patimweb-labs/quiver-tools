// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Ein kleiner XML-Baum für die ADT-Antworten (Aufgabe 07).
//!
//! ADT liefert zu jedem Endpunkt ein eigenes XML mit eigenen Namensräumen. Wir
//! brauchen davon nur einzelne Felder. Statt eines Schema-Modells für jede
//! Antwort liest dieses Modul das Dokument in einen Baum aus [`Knoten`] und
//! gibt Namensräume auf: `adtcore:name` und `name` heißen beide `name`. Das
//! genügt, weil ADT in einem Dokument nie zwei verschiedene Präfixe für
//! dasselbe Feld verwendet.
//!
//! Kein Zustand, keine Ein-/Ausgabe – nur Text zu Baum, unit-getestet.

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::adt::fehler::AdtFehler;

/// Ein Element des Baums.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Knoten {
    /// Name ohne Namensraum-Präfix, z. B. `checkMessage`.
    pub name: String,
    /// Attribute ohne Präfix (`name`, `type`, `uri` …), in Dokumentreihenfolge.
    pub attrs: Vec<(String, String)>,
    /// Kindelemente in Dokumentreihenfolge.
    pub kinder: Vec<Knoten>,
    /// Direkter Textinhalt (ohne den der Kinder), roh – mit Leerraum und
    /// aufgelösten Entities. Getrimmt liest ihn [`Knoten::kind_text`].
    pub text: String,
}

/// Der lokale Teil eines qualifizierten Namens: `adtcore:name` → `name`.
fn lokal(name: &str) -> &str {
    match name.rsplit_once(':') {
        Some((_, lokal)) => lokal,
        None => name,
    }
}

impl Knoten {
    /// Liest ein Dokument; die Wurzel heißt `#dokument` und hat ein Kind.
    pub fn parse(text: &str) -> Result<Knoten, AdtFehler> {
        // Text bleibt roh: Leerraum zwischen Elementen ist Inhalt, kein Füllsel.
        // Getrimmt wird erst beim Lesen (`kind_text`).
        let mut leser = Reader::from_str(text);

        let mut wurzel = Knoten {
            name: "#dokument".to_string(),
            ..Knoten::default()
        };
        let mut stapel: Vec<Knoten> = Vec::new();

        loop {
            match leser.read_event() {
                Ok(Event::Start(start)) => stapel.push(knoten_aus(&start)?),
                Ok(Event::Empty(leer)) => {
                    let knoten = knoten_aus(&leer)?;
                    anhaengen(&mut wurzel, &mut stapel, knoten);
                }
                Ok(Event::End(_)) => {
                    if let Some(fertig) = stapel.pop() {
                        anhaengen(&mut wurzel, &mut stapel, fertig);
                    }
                }
                Ok(Event::Text(text)) => {
                    // `quick-xml` liefert Entities als eigenes Ereignis; hier steht
                    // reiner Text.
                    let inhalt = text.decode().unwrap_or_default();
                    if let Some(oben) = stapel.last_mut() {
                        oben.text.push_str(&inhalt);
                    }
                }
                Ok(Event::GeneralRef(referenz)) => {
                    // `&amp;` oder `&#38;`: wieder zu einem Zeichen machen, statt es
                    // stillschweigend zu verlieren.
                    let roh = referenz.decode().unwrap_or_default();
                    if let Some(oben) = stapel.last_mut() {
                        oben.text.push_str(&referenz_aufloesen(&roh));
                    }
                }
                Ok(Event::CData(inhalt)) => {
                    let inhalt = String::from_utf8_lossy(&inhalt).to_string();
                    if let Some(oben) = stapel.last_mut() {
                        oben.text.push_str(&inhalt);
                    }
                }
                Ok(Event::Eof) => break,
                Ok(_) => {}
                Err(fehler) => {
                    return Err(AdtFehler::Xml {
                        text: fehler.to_string(),
                    })
                }
            }
        }

        if stapel.is_empty() {
            Ok(wurzel)
        } else {
            Err(AdtFehler::Xml {
                text: "Das Dokument bricht mitten in einem Element ab.".to_string(),
            })
        }
    }

    /// Ein Attribut, ohne Präfix im Namen.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(schluessel, _)| schluessel == name)
            .map(|(_, wert)| wert.as_str())
    }

    /// Ein Attribut, das nicht leer ist.
    pub fn attr_gefuellt(&self, name: &str) -> Option<&str> {
        self.attr(name)
            .map(str::trim)
            .filter(|wert| !wert.is_empty())
    }

    /// Das erste Kind mit diesem Namen.
    pub fn kind(&self, name: &str) -> Option<&Knoten> {
        self.kinder.iter().find(|kind| kind.name == name)
    }

    /// Alle Kinder mit diesem Namen.
    pub fn kinder_mit<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Knoten> + 'a {
        self.kinder.iter().filter(move |kind| kind.name == name)
    }

    /// Der Text eines Kindes, ohne Leerraum am Rand.
    pub fn kind_text(&self, name: &str) -> Option<&str> {
        self.kind(name).map(|kind| kind.text.trim())
    }

    /// Ein Pfad aus Kindnamen, z. B. `["tm:root", "tm:request"]` in lokal.
    pub fn pfad(&self, namen: &[&str]) -> Option<&Knoten> {
        let mut aktuell = self;
        for name in namen {
            aktuell = aktuell.kind(name)?;
        }
        Some(aktuell)
    }

    /// Alle Nachfahren mit diesem Namen (Tiefensuche, Dokumentreihenfolge).
    pub fn alle(&self, name: &str) -> Vec<&Knoten> {
        let mut treffer = Vec::new();
        self.sammle(name, &mut treffer);
        treffer
    }

    fn sammle<'a>(&'a self, name: &str, treffer: &mut Vec<&'a Knoten>) {
        for kind in &self.kinder {
            if kind.name == name {
                treffer.push(kind);
            }
            kind.sammle(name, treffer);
        }
    }

    /// Alle Nachfahren mit diesem Namen unterhalb eines Pfads.
    pub fn alle_unter<'a>(&'a self, pfad: &[&str], name: &str) -> Vec<&'a Knoten> {
        match self.pfad(pfad) {
            Some(knoten) => knoten.alle(name),
            None => Vec::new(),
        }
    }
}

impl std::fmt::Display for Knoten {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

/// Einen Text für ein XML-Attribut unschädlich machen.
///
/// Nur die fünf Zeichen, die XML in einem Attributwert nicht erlaubt; alles
/// andere (Umlaute, Zeilenumbrüche) bleibt stehen.
pub fn attribut_wert(wert: &str) -> String {
    wert.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// `amp`, `lt`, `#38`, `#x26` wieder zu einem Zeichen machen.
fn referenz_aufloesen(name: &str) -> String {
    match name {
        "amp" => "&".to_string(),
        "lt" => "<".to_string(),
        "gt" => ">".to_string(),
        "quot" => "\"".to_string(),
        "apos" => "'".to_string(),
        zahl => {
            let zeichen = if let Some(hex) = zahl.strip_prefix("#x") {
                u32::from_str_radix(hex, 16).ok()
            } else if let Some(dezimal) = zahl.strip_prefix('#') {
                dezimal.parse::<u32>().ok()
            } else {
                None
            };
            match zeichen.and_then(char::from_u32) {
                Some(zeichen) => zeichen.to_string(),
                // Unbekannte Entity: unverändert stehen lassen.
                None => format!("&{zahl};"),
            }
        }
    }
}

/// Ein Element aus einem Start- oder Leer-Element lesen.
fn knoten_aus(element: &quick_xml::events::BytesStart<'_>) -> Result<Knoten, AdtFehler> {
    let name = lokal(&String::from_utf8_lossy(element.name().as_ref())).to_string();
    let mut attrs = Vec::new();
    for attr in element.attributes() {
        let attr = attr.map_err(|fehler| AdtFehler::Xml {
            text: fehler.to_string(),
        })?;
        let schluessel = lokal(&String::from_utf8_lossy(attr.key.as_ref())).to_string();
        let wert = attr
            .normalized_value(quick_xml::XmlVersion::Explicit1_0)
            .map(|wert| wert.to_string())
            .unwrap_or_default();
        // Das Namensraum-Präfix selbst interessiert uns nicht.
        if schluessel.starts_with("xmlns") {
            continue;
        }
        attrs.push((schluessel, wert));
    }
    Ok(Knoten {
        name,
        attrs,
        kinder: Vec::new(),
        text: String::new(),
    })
}

/// Einen fertigen Knoten an seinen Elternknoten hängen.
fn anhaengen(wurzel: &mut Knoten, stapel: &mut [Knoten], knoten: Knoten) {
    match stapel.last_mut() {
        Some(oben) => oben.kinder.push(knoten),
        None => wurzel.kinder.push(knoten),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namensraeume_verschwinden() {
        let baum = Knoten::parse(
            r#"<?xml version="1.0"?>
            <adtcore:objectReferences xmlns:adtcore="http://www.sap.com/adt/core">
              <adtcore:objectReference adtcore:uri="/sap/bc/adt/x" adtcore:name="ZX &amp; Y"/>
            </adtcore:objectReferences>"#,
        )
        .unwrap();

        let liste = baum.kind("objectReferences").expect("Liste fehlt");
        let treffer = liste.kind("objectReference").expect("Treffer fehlt");
        assert_eq!(treffer.attr("uri"), Some("/sap/bc/adt/x"));
        // Entitäten werden aufgelöst.
        assert_eq!(treffer.attr("name"), Some("ZX & Y"));
        assert_eq!(treffer.attr("gibtsnicht"), None);
    }

    #[test]
    fn pfade_kinder_und_texte() {
        let baum = Knoten::parse("<root><a><b>eins</b><b>zwei</b></a><c/></root>").unwrap();
        let a = baum.pfad(&["root", "a"]).unwrap();
        assert_eq!(a.kind_text("b"), Some("eins"));
        let bs: Vec<&str> = a.kinder_mit("b").map(|k| k.text.as_str()).collect();
        assert_eq!(bs, vec!["eins", "zwei"]);
        assert!(baum.pfad(&["root", "gibtsnicht"]).is_none());
        assert_eq!(baum.kind_text("root"), Some(""));
        assert_eq!(baum.alle("b").len(), 2);
        assert_eq!(baum.alle_unter(&["root", "a"], "b").len(), 2);
        assert!(baum.alle_unter(&["root", "c"], "b").is_empty());
    }

    #[test]
    fn kaputtes_xml_ist_ein_fehler() {
        let fehler = Knoten::parse("<a><b></a>").unwrap_err();
        let text = fehler.to_string();
        assert!(!text.is_empty());
        // Ein abgebrochenes Dokument ist ein Fehler, kein halber Baum.
        assert!(Knoten::parse("<a><b>").is_err());
    }

    #[test]
    fn entities_im_text_gehen_nicht_verloren() {
        let baum = Knoten::parse("<a>Wert &amp; Zahl &#38; mehr &lt;x&gt;</a>").unwrap();
        assert_eq!(baum.pfad(&["a"]).unwrap().text, "Wert & Zahl & mehr <x>");
        // Unbekannte Entities bleiben stehen.
        assert_eq!(referenz_aufloesen("unbekannt"), "&unbekannt;");
        assert_eq!(referenz_aufloesen("#x26"), "&");
    }

    #[test]
    fn entity_und_text() {
        assert_eq!(
            attribut_wert("a&b<c>d\"e'f"),
            "a&amp;b&lt;c&gt;d&quot;e&apos;f"
        );
        assert_eq!(attribut_wert("Grüße"), "Grüße");
    }

    #[test]
    fn leerzeichen_um_text_verschwinden() {
        let baum = Knoten::parse("<a>\n  <b>  Wert  </b>\n</a>").unwrap();
        assert_eq!(baum.pfad(&["a"]).unwrap().kind_text("b"), Some("Wert"));
    }
}
