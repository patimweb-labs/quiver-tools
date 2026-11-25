// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Strenger XML-Leser und -Schreiber für draw.io-Dateien.
//!
//! Aufgabe 18 verlangt: DTD und Entitäten ablehnen, Tiefe und Größe begrenzen,
//! Attribute sauber maskieren, unbekannte Elemente und Attribute erhalten. Ein
//! Baum aus [`Element`] und [`Knoten`] kann genau das; alles, was darüber
//! hinausgeht (CDATA, Verarbeitungsanweisungen mit Inhalt, Namensräume), wird
//! abgelehnt, weil draw.io es nicht schreibt.
//!
//! Der Baum hält die Reihenfolge von Attributen und Kindern. Wer nichts ändert,
//! schreibt dieselben Elemente mit denselben Attributen wieder heraus – nur die
//! Maskierung wird dabei vereinheitlicht.

use std::fmt;

/// So tief darf ein Dokument verschachtelt sein.
pub const MAX_TIEFE: usize = 64;

/// Ein Knoten im Baum: ein Element oder Text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Knoten {
    /// Ein Element mit Attributen und Kindern.
    Element(Element),
    /// Textinhalt (bereits entschlüsselt).
    Text(String),
}

/// Ein XML-Element.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Element {
    /// Name des Elements, z. B. `mxCell`.
    pub name: String,
    /// Attribute in der Reihenfolge der Datei.
    pub attribute: Vec<(String, String)>,
    /// Kinder in der Reihenfolge der Datei.
    pub kinder: Vec<Knoten>,
}

impl Element {
    /// Ein leeres Element mit diesem Namen.
    pub fn neu(name: impl Into<String>) -> Element {
        Element {
            name: name.into(),
            attribute: Vec::new(),
            kinder: Vec::new(),
        }
    }

    /// Setzt ein Attribut (Builder für feste Bäume).
    pub fn mit(mut self, name: &str, wert: impl Into<String>) -> Element {
        self.setze_attribut(name, wert);
        self
    }

    /// Der Wert eines Attributs.
    pub fn attribut(&self, name: &str) -> Option<&str> {
        self.attribute
            .iter()
            .find(|(schluessel, _)| schluessel == name)
            .map(|(_, wert)| wert.as_str())
    }

    /// Setzt ein Attribut; ein vorhandenes wird an seiner Stelle ersetzt.
    pub fn setze_attribut(&mut self, name: &str, wert: impl Into<String>) {
        let wert = wert.into();
        match self
            .attribute
            .iter_mut()
            .find(|(schluessel, _)| schluessel == name)
        {
            Some((_, vorhanden)) => *vorhanden = wert,
            None => self.attribute.push((name.to_string(), wert)),
        }
    }

    /// Entfernt ein Attribut und gibt seinen Wert zurück.
    pub fn entferne_attribut(&mut self, name: &str) -> Option<String> {
        let stelle = self
            .attribute
            .iter()
            .position(|(schluessel, _)| schluessel == name)?;
        Some(self.attribute.remove(stelle).1)
    }

    /// Das erste Kindelement mit diesem Namen.
    pub fn kind(&self, name: &str) -> Option<&Element> {
        self.kinder_elemente().find(|kind| kind.name == name)
    }

    /// Alle Kindelemente in Reihenfolge.
    pub fn kinder_elemente(&self) -> impl Iterator<Item = &Element> {
        self.kinder.iter().filter_map(|knoten| match knoten {
            Knoten::Element(element) => Some(element),
            Knoten::Text(_) => None,
        })
    }

    /// Der Textinhalt dieses Elements (alle Textknoten zusammengesetzt).
    pub fn text_inhalt(&self) -> String {
        let mut text = String::new();
        for knoten in &self.kinder {
            if let Knoten::Text(teil) = knoten {
                text.push_str(teil);
            }
        }
        text
    }

    /// Ersetzt den Inhalt durch genau einen Textknoten.
    pub fn setze_text(&mut self, text: &str) {
        self.kinder.clear();
        if !text.is_empty() {
            self.kinder.push(Knoten::Text(text.to_string()));
        }
    }

    /// Hängt ein Kindelement an.
    pub fn anhaengen(&mut self, kind: Element) {
        self.kinder.push(Knoten::Element(kind));
    }

    /// Das Element als XML (ohne Deklaration).
    pub fn schreiben(&self, aus: &mut String) {
        aus.push('<');
        aus.push_str(&self.name);
        for (name, wert) in &self.attribute {
            aus.push(' ');
            aus.push_str(name);
            aus.push_str("=\"");
            aus.push_str(&escape_attribut(wert));
            aus.push('"');
        }
        if self.kinder.is_empty() {
            aus.push_str("/>");
            return;
        }
        aus.push('>');
        for kind in &self.kinder {
            match kind {
                Knoten::Element(element) => element.schreiben(aus),
                Knoten::Text(text) => aus.push_str(&escape_text(text)),
            }
        }
        aus.push_str("</");
        aus.push_str(&self.name);
        aus.push('>');
    }

    /// Das Element als vollständiges Dokument (mit XML-Deklaration).
    pub fn als_text(&self) -> String {
        let mut text = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        self.schreiben(&mut text);
        text
    }
}

/// Maskiert Textinhalt: `&`, `<` und `>`.
pub fn escape_text(wert: &str) -> String {
    let mut text = String::with_capacity(wert.len());
    for zeichen in wert.chars() {
        match zeichen {
            '&' => text.push_str("&amp;"),
            '<' => text.push_str("&lt;"),
            '>' => text.push_str("&gt;"),
            sonst => text.push(sonst),
        }
    }
    text
}

/// Maskiert einen Attributwert: zusätzlich `"`, Zeilenumbruch, Tab.
///
/// Zeilenumbrüche werden als Zeichenverweis geschrieben, weil XML sie in
/// Attributen sonst zu Leerzeichen normalisiert.
pub fn escape_attribut(wert: &str) -> String {
    let mut text = String::with_capacity(wert.len());
    for zeichen in wert.chars() {
        match zeichen {
            '&' => text.push_str("&amp;"),
            '<' => text.push_str("&lt;"),
            '>' => text.push_str("&gt;"),
            '"' => text.push_str("&quot;"),
            '\n' => text.push_str("&#10;"),
            '\r' => text.push_str("&#13;"),
            '\t' => text.push_str("&#9;"),
            sonst => text.push(sonst),
        }
    }
    text
}

/// Was beim Lesen schiefging – mit Stelle, damit ein Agent die Datei findet.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("an Zeichen {stelle}: {was}")]
pub struct XmlFehler {
    /// Byte-Stelle im Eingabetext.
    pub stelle: usize,
    /// Was dort nicht stimmt.
    pub was: String,
}

impl XmlFehler {
    fn neu(stelle: usize, was: impl Into<String>) -> XmlFehler {
        XmlFehler {
            stelle,
            was: was.into(),
        }
    }
}

/// Liest ein Dokument mit genau einem Wurzelelement.
///
/// Abgelehnt werden DTDs (`<!DOCTYPE`, `<!ENTITY`), CDATA-Abschnitte,
/// unbekannte Entitäten und mehr als [`MAX_TIEFE`] Ebenen.
pub fn lesen(text: &str) -> Result<Element, XmlFehler> {
    if text.trim().is_empty() {
        return Err(XmlFehler::neu(0, "das Dokument ist leer"));
    }
    let mut leser = Leser {
        text,
        pos: 0,
        tiefe: 0,
    };
    if leser.beginnt("\u{feff}") {
        leser.pos += "\u{feff}".len();
    }
    leser.ueberspringe_aussen()?;
    let element = leser.element()?;
    leser.ueberspringe_aussen()?;
    if leser.pos < leser.text.len() {
        return Err(leser.fehler("nach dem Wurzelelement steht noch Inhalt"));
    }
    Ok(element)
}

struct Leser<'a> {
    text: &'a str,
    pos: usize,
    tiefe: usize,
}

impl<'a> Leser<'a> {
    fn fehler(&self, was: impl Into<String>) -> XmlFehler {
        XmlFehler::neu(self.pos, was)
    }

    fn rest(&self) -> &'a str {
        &self.text[self.pos..]
    }

    fn beginnt(&self, muster: &str) -> bool {
        self.text.as_bytes()[self.pos..].starts_with(muster.as_bytes())
    }

    fn leerraum(&mut self) {
        while let Some(zeichen) = self.rest().chars().next() {
            if zeichen.is_whitespace() {
                self.pos += zeichen.len_utf8();
            } else {
                break;
            }
        }
    }

    fn erwarte(&mut self, muster: &str, was: &str) -> Result<(), XmlFehler> {
        if !self.beginnt(muster) {
            return Err(self.fehler(format!("{was} erwartet, gefunden wurde „{}“", self.blick())));
        }
        self.pos += muster.len();
        Ok(())
    }

    /// Ein kurzer Blick auf das nächste Zeichen für Fehlermeldungen.
    fn blick(&self) -> String {
        let rest = self.rest();
        let kurz: String = rest.chars().take(24).collect();
        if kurz.is_empty() {
            "Ende".to_string()
        } else {
            kurz.replace('\n', " ")
        }
    }

    /// Leerraum, Kommentare und Verarbeitungsanweisungen zwischen Elementen.
    fn ueberspringe_aussen(&mut self) -> Result<(), XmlFehler> {
        loop {
            self.leerraum();
            if self.beginnt("<!--") {
                self.kommentar()?;
            } else if self.beginnt("<?") {
                self.anweisung()?;
            } else if self.beginnt("<!") {
                return Err(self.deklaration());
            } else if self.beginnt("<![CDATA[") {
                return Err(self.fehler("CDATA ist nicht erlaubt"));
            } else {
                return Ok(());
            }
        }
    }

    fn kommentar(&mut self) -> Result<(), XmlFehler> {
        let start = self.pos;
        match self.text[self.pos + 4..].find("-->") {
            Some(ende) => {
                self.pos = start + 4 + ende + 3;
                Ok(())
            }
            None => Err(self.fehler("Kommentar ohne „-->“")),
        }
    }

    fn anweisung(&mut self) -> Result<(), XmlFehler> {
        match self.text[self.pos + 2..].find("?>") {
            Some(ende) => {
                self.pos += 2 + ende + 2;
                Ok(())
            }
            None => Err(self.fehler("Anweisung ohne „?>“")),
        }
    }

    fn deklaration(&self) -> XmlFehler {
        let oben = self.text[self.pos..].to_ascii_uppercase();
        if oben.starts_with("<!DOCTYPE") {
            self.fehler("DTDs sind nicht erlaubt (kein <!DOCTYPE>)")
        } else if oben.starts_with("<!ENTITY") {
            self.fehler("Entitäten sind nicht erlaubt (kein <!ENTITY>)")
        } else {
            self.fehler("unbekannte Deklaration „<!…>“ – erlaubt ist nur reines XML")
        }
    }

    fn element(&mut self) -> Result<Element, XmlFehler> {
        if self.tiefe >= MAX_TIEFE {
            return Err(self.fehler(format!(
                "das Dokument ist tiefer als {MAX_TIEFE} Ebenen verschachtelt"
            )));
        }
        self.tiefe += 1;
        self.erwarte("<", "ein Element")?;
        if self.beginnt("!") || self.beginnt("?") {
            return Err(self.fehler("hier wird ein Elementname erwartet"));
        }
        let name = self.name()?;
        let mut element = Element::neu(name.clone());

        loop {
            self.leerraum();
            if self.beginnt("/>") {
                self.pos += 2;
                self.tiefe -= 1;
                return Ok(element);
            }
            if self.beginnt(">") {
                self.pos += 1;
                break;
            }
            let attribut = self.name()?;
            self.leerraum();
            self.erwarte("=", &format!("„=“ nach dem Attribut „{attribut}“"))?;
            self.leerraum();
            let wert = self.attribut_wert()?;
            element.attribute.push((attribut, wert));
        }

        loop {
            if self.pos >= self.text.len() {
                return Err(self.fehler(format!("das Element „{name}“ wurde nicht geschlossen")));
            }
            if self.beginnt("</") {
                self.pos += 2;
                let ende = self.name()?;
                if ende != name {
                    return Err(self.fehler(format!(
                        "„</{ende}>“ schließt „<{name}>“ – Namen müssen übereinstimmen"
                    )));
                }
                self.leerraum();
                self.erwarte(">", "„>“ im schließenden Tag")?;
                self.tiefe -= 1;
                return Ok(element);
            }
            if self.beginnt("<!--") {
                self.kommentar()?;
                continue;
            }
            if self.beginnt("<![CDATA[") {
                return Err(self.fehler("CDATA ist nicht erlaubt"));
            }
            if self.beginnt("<!") {
                return Err(self.deklaration());
            }
            if self.beginnt("<?") {
                self.anweisung()?;
                continue;
            }
            if self.beginnt("<") {
                let kind = self.element()?;
                element.kinder.push(Knoten::Element(kind));
                continue;
            }
            let start = self.pos;
            while self.pos < self.text.len() && !self.beginnt("<") {
                // Immer ein ganzes Zeichen weiter, damit die Stelle gültig bleibt.
                let zeichen = self.rest().chars().next().expect("nicht leer");
                self.pos += zeichen.len_utf8();
            }
            let roh = &self.text[start..self.pos];
            let text = entschluesseln(roh).map_err(|was| XmlFehler::neu(start, was))?;
            element.kinder.push(Knoten::Text(text));
        }
    }

    fn name(&mut self) -> Result<String, XmlFehler> {
        let start = self.pos;
        while let Some(zeichen) = self.rest().chars().next() {
            let erlaubt =
                zeichen.is_ascii_alphanumeric() || matches!(zeichen, '_' | ':' | '.' | '-');
            if !erlaubt {
                break;
            }
            self.pos += zeichen.len_utf8();
        }
        let name = &self.text[start..self.pos];
        if name.is_empty() {
            return Err(self.fehler("ein Name wird erwartet"));
        }
        if !name
            .chars()
            .next()
            .is_some_and(|z| z.is_alphabetic() || z == '_' || z == ':')
        {
            return Err(XmlFehler::neu(
                start,
                format!("„{name}“ ist kein gültiger Name"),
            ));
        }
        Ok(name.to_string())
    }

    fn attribut_wert(&mut self) -> Result<String, XmlFehler> {
        let quote = match self.rest().chars().next() {
            Some(zeichen @ ('"' | '\'')) => zeichen,
            _ => return Err(self.fehler("ein Attributwert muss in „…“ oder ‚…‘ stehen")),
        };
        self.pos += quote.len_utf8();
        let start = self.pos;
        loop {
            if self.pos >= self.text.len() {
                return Err(self.fehler("Attributwert ohne schließendes Anführungszeichen"));
            }
            if self.rest().starts_with(quote) {
                let roh = &self.text[start..self.pos];
                self.pos += quote.len_utf8();
                if roh.contains('<') {
                    return Err(XmlFehler::neu(
                        start,
                        "„<“ ist in einem Attributwert nicht erlaubt – bitte &lt; schreiben",
                    ));
                }
                return entschluesseln(roh).map_err(|was| XmlFehler::neu(start, was));
            }
            let zeichen = self.rest().chars().next().expect("nicht leer");
            self.pos += zeichen.len_utf8();
        }
    }
}

/// Löst `&amp;`, `&lt;`, `&gt;`, `&quot;`, `&apos;` und Zeichenverweise auf.
///
/// Jede andere Entität ist ein Fehler: eine Datei darf keine Entitäten
/// mitbringen, die dieser Leser nicht kennt.
fn entschluesseln(roh: &str) -> Result<String, String> {
    if !roh.contains('&') {
        return Ok(roh.to_string());
    }
    let mut text = String::with_capacity(roh.len());
    let mut rest = roh;
    while let Some(stelle) = rest.find('&') {
        text.push_str(&rest[..stelle]);
        let nach = &rest[stelle + 1..];
        let ende = nach.find(';').filter(|ende| *ende <= 16).ok_or_else(|| {
            "„&“ ohne „;“ – erlaubt sind nur &amp; &lt; &gt; &quot; &apos; und \
             Zeichenverweise"
                .to_string()
        })?;
        text.push(zeichen_fuer(&nach[..ende])?);
        rest = &nach[ende + 1..];
    }
    text.push_str(rest);
    Ok(text)
}

fn zeichen_fuer(name: &str) -> Result<char, String> {
    let zeichen = match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        sonst => {
            let zahl = sonst
                .strip_prefix("#x")
                .or_else(|| sonst.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| sonst.strip_prefix('#').and_then(|zahl| zahl.parse().ok()));
            zahl.and_then(char::from_u32)
        }
    };
    zeichen.ok_or_else(|| {
        format!(
            "unbekannte Entität „&{name};“ – erlaubt sind nur &amp; &lt; &gt; &quot; \
             &apos; und Zeichenverweise"
        )
    })
}

impl fmt::Display for Element {
    /// Kurzform für Fehlermeldungen: `<mxCell id="2">`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<{}", self.name)?;
        if let Some(id) = self.attribut("id") {
            write!(f, " id=\"{id}\"")?;
        }
        f.write_str(">")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elemente_und_attribute_bleiben_in_reihenfolge() {
        let element = lesen(r#"<a z="1" y="2"><b/>text<c k="v">innen</c></a>"#).unwrap();
        assert_eq!(element.name, "a");
        assert_eq!(
            element.attribute,
            vec![
                ("z".to_string(), "1".to_string()),
                ("y".to_string(), "2".to_string())
            ]
        );
        assert_eq!(element.als_text(), "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<a z=\"1\" y=\"2\"><b/>text<c k=\"v\">innen</c></a>");
    }

    #[test]
    fn texte_und_attribute_werden_entschluesselt_und_wieder_maskiert() {
        let element = lesen(
            r#"<a t="&quot;&amp;&lt;&gt;&#65;&#x42;">&lt;script&gt; &amp; &quot;x&quot;</a>"#,
        )
        .unwrap();
        assert_eq!(element.attribut("t"), Some("\"&<>AB"));
        assert_eq!(element.text_inhalt(), "<script> & \"x\"");
        let text = element.als_text();
        assert!(text.contains("t=\"&quot;&amp;&lt;&gt;AB\""));
        assert!(text.contains("&lt;script&gt; &amp; \"x\""), "{text}");
        // Und der Rundlauf ist stabil.
        assert_eq!(lesen(&text).unwrap(), element);
    }

    #[test]
    fn zeilenumbruch_im_attribut_ueberlebt() {
        let element = Element::neu("a").mit("label", "eins\nzwei\tdrei");
        let text = element.als_text();
        assert!(text.contains("&#10;"), "{text}");
        assert!(text.contains("&#9;"), "{text}");
        assert_eq!(
            lesen(&text).unwrap().attribut("label"),
            Some("eins\nzwei\tdrei")
        );
    }

    #[test]
    fn dtd_und_entitaeten_werden_abgelehnt() {
        let dtd = r#"<!DOCTYPE foo [<!ENTITY xxe SYSTEM "file:///etc/passwd">]><mxfile/>"#;
        let fehler = lesen(dtd).unwrap_err();
        assert!(fehler.was.contains("DTD"), "{fehler}");

        let entity = "<mxfile><diagram>&xxe;</diagram></mxfile>";
        let fehler = lesen(entity).unwrap_err();
        assert!(fehler.was.contains("Entität"), "{fehler}");

        let unbekannt = "<a b=\"&gefaehrlich;\"/>";
        assert!(lesen(unbekannt).is_err());
    }

    #[test]
    fn cdata_und_mehrere_wurzeln_werden_abgelehnt() {
        assert!(lesen("<a><![CDATA[x]]></a>").is_err());
        assert!(lesen("<a/><b/>").is_err());
        assert!(lesen("").is_err());
        assert!(lesen("<a>").is_err());
        assert!(lesen("<a></b>").is_err());
    }

    #[test]
    fn tiefe_ist_begrenzt() {
        let tief = "<a>".repeat(MAX_TIEFE + 1);
        let fehler = lesen(&tief).unwrap_err();
        assert!(fehler.was.contains("Ebenen"), "{fehler}");

        let gerade_noch = format!("{}x{}", "<a>".repeat(MAX_TIEFE), "</a>".repeat(MAX_TIEFE));
        assert!(lesen(&gerade_noch).is_ok());
    }

    #[test]
    fn kommentare_und_anweisungen_werden_uebersprungen() {
        let text = "<?xml version=\"1.0\"?><!-- oben --><a><!-- innen -->x</a><!-- unten -->";
        let element = lesen(text).unwrap();
        assert_eq!(element.name, "a");
        assert_eq!(element.text_inhalt(), "x");
    }

    #[test]
    fn ein_kleiner_als_zeichen_im_attribut_ist_ein_fehler() {
        let fehler = lesen("<a t=\"<b>\"/>").unwrap_err();
        assert!(fehler.was.contains("Attributwert"), "{fehler}");
    }
}
