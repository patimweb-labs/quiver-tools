// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Was beim Sprechen mit Azure DevOps schiefgehen kann (Aufgabe 09).
//!
//! Zwei Regeln gelten für jeden Text aus diesem Modul: er enthält **keinen PAT**
//! und keine Kopfzeilen, und er ist für einen Agenten lesbar. Azure DevOps
//! antwortet auf Fehler mit JSON (`{"message": …}`) oder – bei einem ungültigen
//! PAT – mit einer HTML-Anmeldeseite; beides wird zu einem kurzen Satz.

/// Ein Fehler beim Sprechen mit Azure DevOps.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DevopsFehler {
    /// HTTP 401 oder 203 mit HTML: der PAT gilt nicht. Wird nie wiederholt.
    #[error("{}", crate::format::pat_ungueltig(.verbindung))]
    Anmeldung { verbindung: String },

    /// Andere HTTP-Antwort.
    #[error("Azure DevOps antwortete mit HTTP {status}: {text}")]
    Http { status: u16, text: String },

    /// Das Netz oder die Adresse stimmt nicht.
    #[error("Keine Verbindung: {text}")]
    Netz { text: String },

    /// Eine Antwort hatte nicht die erwarteten Felder.
    #[error("{text}")]
    Antwort { text: String },

    /// Die Antwort war kein gültiges JSON.
    #[error("Die Antwort von Azure DevOps war kein lesbares JSON: {text}")]
    Json { text: String },
}

impl DevopsFehler {
    /// Ein Netzfehler ohne Kopfzeilen und ohne Zugangsdaten.
    pub fn netz(fehler: &reqwest::Error) -> DevopsFehler {
        // `reqwest` nennt in seiner Meldung nur Methode und Adresse; der PAT
        // steht dort nie.
        DevopsFehler::Netz {
            text: fehler.to_string(),
        }
    }

    /// Aus einer Fehlerantwort einen kurzen Satz machen.
    ///
    /// Ist der Rumpf JSON mit `message`, zählt die Meldung von DevOps; ein
    /// langer HTML-Rumpf (Anmeldeseite, Proxy) wird verworfen.
    pub fn aus_antwort(status: u16, rumpf: &str) -> DevopsFehler {
        if let Ok(wert) = serde_json::from_str::<serde_json::Value>(rumpf) {
            if let Some(meldung) = wert.get("message").and_then(serde_json::Value::as_str) {
                return DevopsFehler::Http {
                    status,
                    text: kuerzen(meldung, 300),
                };
            }
        }
        DevopsFehler::Http {
            status,
            text: kuerzen(&ohne_html(rumpf), 300),
        }
    }
}

/// Sieht der Rumpf nach einer HTML-Seite aus? (DevOps leitet bei ungültigem PAT
/// mit `203` auf die Anmeldung um.)
pub fn ist_html(rumpf: &str) -> bool {
    let anfang = rumpf.trim_start().to_ascii_lowercase();
    anfang.starts_with("<!doctype html") || anfang.starts_with("<html") || anfang.contains("<html")
}

/// Grobe Kürzung für Fehlertexte.
fn kuerzen(text: &str, max: usize) -> String {
    let sauber = text.trim();
    if sauber.chars().count() <= max {
        return sauber.to_string();
    }
    let kurz: String = sauber.chars().take(max).collect();
    format!("{kurz} …")
}

/// HTML-Reste aus einem Fehlertext entfernen (Tags weg, Zeilenumbrüche zu Leerzeichen).
fn ohne_html(text: &str) -> String {
    let mut ergebnis = String::with_capacity(text.len());
    let mut im_tag = false;
    for zeichen in text.chars() {
        match zeichen {
            '<' => im_tag = true,
            '>' => im_tag = false,
            _ if im_tag => {}
            '\n' | '\r' | '\t' => ergebnis.push(' '),
            _ => ergebnis.push(zeichen),
        }
    }
    ergebnis.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eine_devops_meldung_wird_zu_einem_kurzen_satz() {
        let fehler = DevopsFehler::aus_antwort(
            404,
            r#"{"$id":"1","message":"TF401232: Der Pull Request wurde nicht gefunden."}"#,
        );
        assert_eq!(
            fehler,
            DevopsFehler::Http {
                status: 404,
                text: "TF401232: Der Pull Request wurde nicht gefunden.".into()
            }
        );
    }

    #[test]
    fn html_und_lange_texte_werden_gekuerzt() {
        assert!(ist_html(
            "<!DOCTYPE html><html><body>Anmelden</body></html>"
        ));
        assert!(ist_html("  <html lang=\"de\">"));
        assert!(!ist_html("{\"message\":\"nein\"}"));

        let seite = format!("<html><body>{}</body></html>", "x".repeat(1000));
        let DevopsFehler::Http { text, .. } = DevopsFehler::aus_antwort(203, &seite) else {
            panic!("kein HTTP-Fehler");
        };
        assert!(text.chars().count() < 400, "{}", text.chars().count());
        assert!(text.ends_with('…'));
        assert!(!text.contains('<'), "{text}");
    }

    #[test]
    fn ein_netzfehler_nennt_keinen_pat() {
        // `reqwest`-Fehler lassen sich nicht bauen; geprüft wird der Text, den
        // die Anmeldung erzeugt.
        let fehler = DevopsFehler::Anmeldung {
            verbindung: "beispiel".into(),
        };
        let text = fehler.to_string();
        assert!(text.contains("beispiel"), "{text}");
        assert!(text.contains("ungültig oder abgelaufen"), "{text}");
        assert!(text.contains("Tools → DevOps"), "{text}");
    }
}
