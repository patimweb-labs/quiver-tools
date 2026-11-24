// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Storage-Format ↔ Text (Aufgabe 14).
//!
//! Confluence speichert Seiten und Kommentare im **Storage-Format**: XHTML mit
//! eigenen Makro-Elementen (`<ac:…>`, `<ri:…>`). Ein Agent zahlt Tokens für
//! jedes `<p>`; darum wird daraus Text: Absätze, Zeilenumbrüche und Listen
//! werden Zeilen, Tags fallen weg, Entities werden aufgelöst.
//!
//! Umgekehrt wird aus dem `text` eines Aufrufs Storage-Format: **HTML-maskiert**
//! in Absätze gesetzt. `storage` dagegen geht unverändert an Confluence.
//!
//! Beides ist rein und ohne Abhängigkeit – kein HTML-Parser für fünf
//! Auszeichnungen. Makros verlieren ihre Struktur, aber nicht ihren Text.

/// Storage-Format in lesbaren Text übersetzen.
pub fn text_aus_html(html: &str) -> String {
    let roh = ohne_tags(html);
    let roh = entities(&roh);

    let mut zeilen: Vec<String> = roh
        .split('\n')
        .map(|zeile| {
            zeile
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .trim()
                .to_string()
        })
        .collect();

    // Führende und folgende Leerzeilen weg, doppelte Leerzeilen zusammenfassen.
    while zeilen.first().is_some_and(String::is_empty) {
        zeilen.remove(0);
    }
    while zeilen.last().is_some_and(String::is_empty) {
        zeilen.pop();
    }
    let mut ergebnis: Vec<String> = Vec::with_capacity(zeilen.len());
    for zeile in zeilen {
        if zeile.is_empty() && ergebnis.last().is_some_and(String::is_empty) {
            continue;
        }
        ergebnis.push(zeile);
    }
    ergebnis.join("\n")
}

/// Tags entfernen und dabei Zeilenumbrüche und Aufzählungszeichen setzen.
fn ohne_tags(html: &str) -> String {
    let mut ergebnis = String::with_capacity(html.len());
    let mut zeichen = html.chars().peekable();
    while let Some(aktuell) = zeichen.next() {
        if aktuell != '<' {
            ergebnis.push(aktuell);
            continue;
        }
        // Tag einlesen, bis `>` kommt.
        let mut tag = String::new();
        for naechstes in zeichen.by_ref() {
            if naechstes == '>' {
                break;
            }
            tag.push(naechstes);
        }
        let name = tag_name(&tag);
        match name.as_str() {
            "br"
            | "/br"
            | "/p"
            | "/div"
            | "/tr"
            | "/h1"
            | "/h2"
            | "/h3"
            | "/table"
            | "/blockquote"
            | "/ac:rich-text-body"
            | "/ac:plain-text-body" => ergebnis.push('\n'),
            "li" => {
                if ergebnis.ends_with('\n') {
                    ergebnis.push_str("- ");
                } else {
                    ergebnis.push_str("\n- ");
                }
            }
            "/li" => {}
            "td" | "th" => ergebnis.push_str(" | "),
            _ => {}
        }
    }
    ergebnis
}

/// Der Name eines Tags, klein und ohne Attribute: `a href="x"` → `a`.
fn tag_name(tag: &str) -> String {
    let name = tag
        .trim()
        .split(|zeichen: char| zeichen.is_whitespace())
        .next()
        .unwrap_or_default();
    name.to_ascii_lowercase()
}

/// Die üblichen Entities auflösen.
fn entities(text: &str) -> String {
    let mut ergebnis = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(stelle) = rest.find('&') {
        ergebnis.push_str(&rest[..stelle]);
        rest = &rest[stelle..];
        let ende = rest.find(';').filter(|ende| *ende <= 10);
        let Some(ende) = ende else {
            ergebnis.push('&');
            rest = &rest[1..];
            continue;
        };
        let name = &rest[1..ende];
        let aufloesung = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            "ndash" => Some('–'),
            "mdash" => Some('—'),
            "hellip" => Some('…'),
            sonst => sonst
                .strip_prefix('#')
                .and_then(|zahl| match zahl.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok(),
                    None => zahl.parse::<u32>().ok(),
                })
                .and_then(char::from_u32),
        };
        match aufloesung {
            Some(zeichen) => {
                ergebnis.push(zeichen);
                rest = &rest[ende + 1..];
            }
            None => {
                ergebnis.push('&');
                rest = &rest[1..];
            }
        }
    }
    ergebnis.push_str(rest);
    ergebnis
}

/// Text für das Storage-Format maskieren: `&` zuerst, dann `<`, `>`, `"`, `'`.
///
/// Die Reihenfolge ist wichtig: würde `&` zuletzt ersetzt, würden die eigenen
/// Entities wieder maskiert (`&amp;lt;`).
pub fn maskieren(text: &str) -> String {
    let mut ergebnis = String::with_capacity(text.len());
    for zeichen in text.chars() {
        match zeichen {
            '&' => ergebnis.push_str("&amp;"),
            '<' => ergebnis.push_str("&lt;"),
            '>' => ergebnis.push_str("&gt;"),
            '"' => ergebnis.push_str("&quot;"),
            '\'' => ergebnis.push_str("&#39;"),
            sonst => ergebnis.push(sonst),
        }
    }
    ergebnis
}

/// Aus dem `text` eines Aufrufs Storage-Format machen (Aufgabe 14).
///
/// Absätze entstehen an Leerzeilen, Zeilenumbrüche innerhalb eines Absatzes
/// werden `<br />`. Der Text wird **maskiert**, nie als HTML gedeutet.
pub fn aus_text(text: &str) -> String {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut ergebnis = String::with_capacity(text.len() + 16);
    for absatz in text.split("\n\n") {
        let absatz = absatz.trim_matches('\n');
        if absatz.trim().is_empty() {
            continue;
        }
        let inhalt = absatz
            .split('\n')
            .map(maskieren)
            .collect::<Vec<_>>()
            .join("<br />");
        ergebnis.push_str("<p>");
        ergebnis.push_str(&inhalt);
        ergebnis.push_str("</p>");
    }
    if ergebnis.is_empty() {
        ergebnis.push_str("<p></p>");
    }
    ergebnis
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absaetze_und_listen_werden_zeilen() {
        let html = "<p>Erster Absatz</p><p>Zweiter Absatz</p><ul><li>eins</li>\
                    <li>zwei</li></ul>";
        assert_eq!(
            text_aus_html(html),
            "Erster Absatz\nZweiter Absatz\n- eins\n- zwei"
        );
    }

    #[test]
    fn entities_und_leerraum_werden_aufgeloest() {
        assert_eq!(
            text_aus_html("<p>a &amp; b&nbsp;&nbsp;c &#8364; &#x41;</p>"),
            "a & b c € A"
        );
        assert_eq!(text_aus_html("Ich &du; dich"), "Ich &du; dich");
        assert_eq!(text_aus_html("<p>a<br>b</p>"), "a\nb");
        assert_eq!(text_aus_html("  <p> x </p>  "), "x");
        assert_eq!(text_aus_html(""), "");
    }

    #[test]
    fn tabellen_und_ueberschriften_bleiben_lesbar() {
        assert_eq!(
            text_aus_html("<h2>Titel</h2><table><tr><td>a</td><td>b</td></tr></table>"),
            "Titel\n| a | b"
        );
    }

    #[test]
    fn makros_verlieren_die_struktur_aber_nicht_den_text() {
        assert_eq!(
            text_aus_html(
                "<ac:structured-macro ac:name=\"info\"><ac:rich-text-body><p>Hinweis</p>\
                 </ac:rich-text-body></ac:structured-macro>"
            ),
            "Hinweis"
        );
        assert_eq!(
            text_aus_html(
                "<p>Siehe <ac:link><ri:page ri:content-title=\"Anleitung\" />\
                           </ac:link></p>"
            ),
            "Siehe"
        );
    }

    #[test]
    fn maskieren_deckt_die_fuenf_zeichen_ab() {
        assert_eq!(
            maskieren("a & b < c > d \"e\" 'f'"),
            "a &amp; b &lt; c &gt; d &quot;e&quot; &#39;f&#39;"
        );
        // Kein doppeltes Maskieren.
        assert_eq!(maskieren("&amp;"), "&amp;amp;");
    }

    #[test]
    fn aus_text_baut_absaetze_und_zeilenumbrueche() {
        assert_eq!(aus_text("Hallo"), "<p>Hallo</p>");
        assert_eq!(
            aus_text("Hallo & Welt\n\nZweiter"),
            "<p>Hallo &amp; Welt</p><p>Zweiter</p>"
        );
        assert_eq!(aus_text("a\nb"), "<p>a<br />b</p>");
        // HTML im Text bleibt Text.
        assert_eq!(
            aus_text("<script>alert(1)</script>"),
            "<p>&lt;script&gt;alert(1)&lt;/script&gt;</p>"
        );
        assert_eq!(aus_text(""), "<p></p>");
    }

    #[test]
    fn der_weg_hin_und_zurueck_verliert_keinen_text() {
        // Absätze werden zu Zeilen; die Leerzeile dazwischen fällt weg. Kein
        // Zeichen geht verloren, nur die Absatz-Trennung wird zur Zeile.
        let text = "Hallo & Welt\n\nZweiter Absatz";
        assert_eq!(
            text_aus_html(&aus_text(text)),
            "Hallo & Welt\nZweiter Absatz"
        );
        assert_eq!(
            text_aus_html(&aus_text("Zeile eins\nZeile zwei")),
            "Zeile eins\nZeile zwei"
        );
    }
}
