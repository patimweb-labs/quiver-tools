// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! HTML → Text (Aufgabe 09).
//!
//! Azure DevOps speichert Beschreibungen, Akzeptanzkriterien und Kommentare als
//! HTML. Ein Agent zahlt Tokens für jedes `<div>`; darum wird daraus Text:
//! Absätze und Listen werden zu Zeilen, Tags fallen weg, Entities werden
//! aufgelöst, mehrfacher Leerraum wird einer.
//!
//! Die Funktion ist rein und ohne Abhängigkeit – kein HTML-Parser für fünf
//! Auszeichnungen.

/// HTML in lesbaren Text übersetzen.
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
            "br" | "/br" | "/p" | "/div" | "/tr" | "/h1" | "/h2" | "/h3" | "/table"
            | "/blockquote" => ergebnis.push('\n'),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absaetze_und_listen_werden_zeilen() {
        let html = "<div>Erster Absatz</div><div>Zweiter Absatz</div><ul><li>eins</li>\
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
}
