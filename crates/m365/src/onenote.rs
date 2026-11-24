// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! OneNote für die Antworten (docs/aufgaben/06-graph-erweiterung.md).
//!
//! Seiteninhalt kommt als HTML. Gelesen wird er über dieselbe Umwandlung wie
//! Teams-Nachrichten (`teams::html_zu_text`), geschrieben als HTML mit
//! **escaptem** Titel und Text – ein `<b>` im Text ist Text, kein Markup.

use m365_graph::{Abschnitt, Notizbuch, Seite};
use serde_json::{json, Value};

use crate::teams::{html_zu_text, text_kuerzen};

/// Standard-Länge einer Seite in der Antwort.
pub const STANDARD_MAX_ZEICHEN: usize = 20_000;
/// Mehr Text gibt `onenote_seite_lesen` nie heraus.
pub const HOECHSTENS_MAX_ZEICHEN: usize = 50_000;

/// `max_zeichen` begrenzen: Standard 20 000, immer zwischen 1 und 50 000.
pub fn max_zeichen_begrenzen(max_zeichen: Option<u32>) -> usize {
    max_zeichen
        .unwrap_or(STANDARD_MAX_ZEICHEN as u32)
        .clamp(1, HOECHSTENS_MAX_ZEICHEN as u32) as usize
}

/// Reiner Text → HTML-Fragment: `&`, `<`, `>`, `"`, `'` escapen,
/// Zeilenumbrüche zu `<br>` (wie bei Teams, Aufgabe 05).
pub fn text_zu_html(text: &str) -> String {
    let mut raus = String::with_capacity(text.len() + 16);
    for zeichen in text.replace("\r\n", "\n").chars() {
        match zeichen {
            '&' => raus.push_str("&amp;"),
            '<' => raus.push_str("&lt;"),
            '>' => raus.push_str("&gt;"),
            '"' => raus.push_str("&quot;"),
            '\'' => raus.push_str("&#39;"),
            '\n' => raus.push_str("<br>"),
            anderes => raus.push(anderes),
        }
    }
    raus
}

/// Eine neue Seite als HTML: Titel im `<title>`, der Text im `<body>`.
///
/// Beides wird escaped – sonst würde ein `<script>` im Text ausgeführt.
pub fn seite_als_html(titel: &str, text: &str) -> String {
    format!(
        "<!DOCTYPE html><html><head><title>{}</title></head><body><p>{}</p></body></html>",
        text_zu_html(titel.trim()),
        text_zu_html(text)
    )
}

/// HTML einer Seite → Text, gekürzt.
///
/// Zweiter und dritter Wert: `gekuerzt` und die Länge des vollen Textes – nur
/// gesetzt, wenn wirklich gekürzt wurde.
pub fn seite_als_text(html: &str, max: usize) -> (String, Option<bool>, Option<usize>) {
    let text = html_zu_text(&ohne_head(html));
    match text_kuerzen(&text, max) {
        (text, None) => (text, None, None),
        (text, Some(gesamt)) => (text, Some(true), Some(gesamt)),
    }
}

/// `<head>…</head>` entfernen: der Titel steht schon im Feld `titel` der
/// Antwort, im Text wäre er nur eine doppelte erste Zeile.
fn ohne_head(html: &str) -> String {
    let mut raus = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(stelle) = rest.to_lowercase().find("<head") {
        raus.push_str(&rest[..stelle]);
        let nach = &rest[stelle..];
        match nach.to_lowercase().find("</head>") {
            Some(ende) => rest = &nach[ende + "</head>".len()..],
            None => {
                // Kein Ende: der Rest ist kein Kopf, sondern Text.
                raus.push_str(nach);
                return raus;
            }
        }
    }
    raus.push_str(rest);
    raus
}

/// Das Notizbuch als knappes JSON.
pub fn notizbuch_json(notizbuch: &Notizbuch) -> Value {
    json!({
        "id": notizbuch.id,
        "name": notizbuch.display_name,
        "link": notizbuch.web_url,
    })
}

/// Der Abschnitt als knappes JSON.
pub fn abschnitt_json(abschnitt: &Abschnitt) -> Value {
    json!({
        "id": abschnitt.id,
        "name": abschnitt.display_name,
        "link": abschnitt.web_url,
    })
}

/// Die Seite als knappes JSON.
pub fn seite_json(seite: &Seite) -> Value {
    json!({
        "id": seite.id,
        "titel": seite.title,
        "link": seite.web_url,
        "geaendert": seite.last_modified_date_time,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_wird_escaped_und_bekommt_zeilenumbrueche() {
        assert_eq!(
            text_zu_html("Milch & Brot <b>fett</b>\r\nZeile 2"),
            "Milch &amp; Brot &lt;b&gt;fett&lt;/b&gt;<br>Zeile 2"
        );
        assert_eq!(
            text_zu_html("Er sagte \"nein\" – 'ok'"),
            "Er sagte &quot;nein&quot; – &#39;ok&#39;"
        );
    }

    #[test]
    fn eine_seite_escapet_titel_und_text() {
        let html = seite_als_html("Bericht <1>", "a & b\nc");
        assert!(html.starts_with("<!DOCTYPE html>"), "{html}");
        assert!(html.contains("<title>Bericht &lt;1&gt;</title>"), "{html}");
        assert!(html.contains("<p>a &amp; b<br>c</p>"), "{html}");
        assert!(!html.contains("<1>"), "der Titel ist Text: {html}");
    }

    #[test]
    fn html_wird_text_und_gekuerzt() {
        let html = "<html><body><p>Erste Zeile</p><p>Zweite Zeile</p></body></html>";
        let (text, gekuerzt, gesamt) = seite_als_text(html, 20_000);
        assert_eq!(text, "Erste Zeile\nZweite Zeile");
        assert_eq!(gekuerzt, None);
        assert_eq!(gesamt, None);

        let (text, gekuerzt, gesamt) = seite_als_text(html, 12);
        assert_eq!(text, "Erste Zeile");
        assert_eq!(gekuerzt, Some(true));
        assert!(gesamt.unwrap_or(0) > 12);
    }

    #[test]
    fn max_zeichen_hat_einen_standard_und_grenzen() {
        assert_eq!(max_zeichen_begrenzen(None), 20_000);
        assert_eq!(max_zeichen_begrenzen(Some(0)), 1);
        assert_eq!(max_zeichen_begrenzen(Some(999_999)), 50_000);
    }

    #[test]
    fn json_bleibt_knapp() {
        let notizbuch = Notizbuch {
            id: "nb-1".to_string(),
            display_name: Some("Arbeit".to_string()),
            web_url: Some("https://example/nb-1".to_string()),
            ..Notizbuch::default()
        };
        let wert = notizbuch_json(&notizbuch);
        assert_eq!(wert["name"], "Arbeit");
        assert!(wert.get("created_date_time").is_none());

        let seite = Seite {
            id: "p1".to_string(),
            title: Some("Notizen".to_string()),
            ..Seite::default()
        };
        assert_eq!(seite_json(&seite)["titel"], "Notizen");
    }
}
