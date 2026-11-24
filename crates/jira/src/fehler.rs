// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Was beim Sprechen mit Jira schiefgehen kann (Aufgabe 13).
//!
//! Drei Regeln gelten für jeden Text aus diesem Modul: er enthält **keinen
//! Token** und keine Kopfzeilen, er ist für einen Agenten lesbar, und er sagt,
//! ob etwas passiert ist. Jira antwortet auf Fehler mit JSON
//! (`errorMessages`, `errors`) – bei einem Proxy oder einer Anmeldeseite auch
//! mit HTML; beides wird zu einem kurzen Satz.
//!
//! Wichtig ist der Unterschied zwischen **nicht geschehen** und **unklar**:
//! Ein abgelehnter Wert wird ohne Anfrage abgelehnt ([`JiraFehler::Wert`]), ein
//! unlesbarer Rumpf nach einer Änderung heißt „ausgeführt, Antwort nicht
//! lesbar“ ([`JiraFehler::Ausgefuehrt`]), und eine Änderung ohne jede Antwort
//! ist „Ausgang unklar“ ([`JiraFehler::Unklar`]) – dann darf niemand blind
//! wiederholen.

/// Ein Fehler beim Sprechen mit Jira.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum JiraFehler {
    /// HTTP 401: Token oder E-Mail gelten nicht. Wird nie wiederholt.
    #[error("{}", crate::format::token_ungueltig(.verbindung))]
    Anmeldung { verbindung: String },

    /// HTTP 403: das Konto darf das nicht.
    #[error("{}", crate::format::keine_rechte(.verbindung, .text))]
    Rechte { verbindung: String, text: String },

    /// Andere HTTP-Antwort.
    #[error("Jira antwortete mit HTTP {status}: {text}")]
    Http { status: u16, text: String },

    /// Das Netz oder die Adresse stimmt nicht.
    #[error("Keine Verbindung: {text}")]
    Netz { text: String },

    /// Eine Antwort hatte nicht die erwarteten Felder.
    #[error("{text}")]
    Antwort { text: String },

    /// Die Antwort war kein gültiges JSON.
    #[error("Die Antwort von Jira war kein lesbares JSON: {text}")]
    Json { text: String },

    /// Eine Folgeseite zeigt auf einen anderen Host. Sie wird nicht angefragt.
    #[error(
        "Die Folgeseite zeigt auf einen fremden Host ({host}) – sie wird nicht angefragt. Der \
         Token geht nur an die eingetragene Adresse."
    )]
    FremderHost { host: String },

    /// Ein Pfadwert taugt nicht (`.`, `..` oder leer). **Keine** Anfrage.
    #[error("Der Wert für `{name}` taugt nicht als Pfad ({grund}) – es wurde nichts angefragt.")]
    Wert { name: String, grund: String },

    /// Der Rumpf einer lesenden Antwort war nicht lesbar. Das ist ein Fehler,
    /// nie „leer“.
    #[error("Die Antwort von Jira war nicht lesbar: {text}")]
    Lesen { text: String },

    /// Eine Änderung kam mit 2xx durch, ihr Rumpf war aber nicht lesbar.
    #[error(
        "Ausgeführt, aber die Antwort von Jira war nicht lesbar: {text} – bitte den Stand in Jira \
         prüfen."
    )]
    Ausgefuehrt { text: String },

    /// Eine Änderung wurde geschickt, aber es kam keine Antwort (Zeitüberschreitung,
    /// Abbruch). Ob sie ankam, weiß nur der Server.
    #[error(
        "Ausgang unklar – Stand prüfen, nicht einfach wiederholen. Die Änderung wurde geschickt, \
         Jira hat aber nicht geantwortet: {text}"
    )]
    Unklar { text: String },
}

impl JiraFehler {
    /// Ein Netzfehler ohne Kopfzeilen und ohne Zugangsdaten.
    pub fn netz(fehler: &reqwest::Error) -> JiraFehler {
        // `reqwest` nennt in seiner Meldung nur Methode und Adresse; der Token
        // steht dort nie.
        JiraFehler::Netz {
            text: fehler.to_string(),
        }
    }

    /// Aus einer Fehlerantwort einen kurzen Satz machen.
    ///
    /// Jira liefert `{ "errorMessages": [ … ], "errors": { feld: meldung } }`;
    /// ein HTML-Rumpf (Proxy, Anmeldeseite) wird zu Text ohne Tags.
    pub fn aus_antwort(status: u16, rumpf: &str) -> JiraFehler {
        JiraFehler::Http {
            status,
            text: meldung_kurz(rumpf),
        }
    }
}

/// Die Meldung aus einem Fehlerrumpf, kurz (höchstens 300 Zeichen).
pub fn meldung_kurz(rumpf: &str) -> String {
    kuerzen(&meldung(rumpf), 300)
}

/// Die Meldung aus einem Jira-Fehlerrumpf.
fn meldung(rumpf: &str) -> String {
    let Ok(wert) = serde_json::from_str::<serde_json::Value>(rumpf) else {
        let roh = ohne_html(rumpf);
        return if roh.is_empty() {
            "(ohne Meldung)".to_string()
        } else {
            roh
        };
    };
    let mut teile: Vec<String> = wert
        .get("errorMessages")
        .and_then(serde_json::Value::as_array)
        .map(|meldungen| {
            meldungen
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if let Some(fehler) = wert.get("errors").and_then(serde_json::Value::as_object) {
        for (feld, meldung) in fehler {
            if let Some(meldung) = meldung.as_str() {
                teile.push(format!("{feld}: {meldung}"));
            }
        }
    }
    if teile.is_empty() {
        // Ein unbekannter Rumpf: lieber der nackte Text als gar nichts.
        let roh = ohne_html(rumpf);
        if roh.is_empty() {
            "(ohne Meldung)".to_string()
        } else {
            roh
        }
    } else {
        teile.join("; ")
    }
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
    fn eine_jira_meldung_wird_zu_einem_kurzen_satz() {
        let fehler = JiraFehler::aus_antwort(
            400,
            r#"{"errorMessages":["Die Abfrage ist ungültig."],"errors":{}}"#,
        );
        assert_eq!(
            fehler,
            JiraFehler::Http {
                status: 400,
                text: "Die Abfrage ist ungültig.".into()
            }
        );

        let fehler = JiraFehler::aus_antwort(
            400,
            r#"{"errorMessages":[],"errors":{"summary":"darf nicht leer sein"}}"#,
        );
        let JiraFehler::Http { text, .. } = fehler else {
            panic!("kein HTTP-Fehler");
        };
        assert_eq!(text, "summary: darf nicht leer sein");
    }

    #[test]
    fn html_und_lange_texte_werden_gekuerzt() {
        let seite = format!("<html><body>{}</body></html>", "x".repeat(1000));
        let JiraFehler::Http { text, .. } = JiraFehler::aus_antwort(502, &seite) else {
            panic!("kein HTTP-Fehler");
        };
        assert!(text.chars().count() < 400, "{}", text.chars().count());
        assert!(text.ends_with('…'));
        assert!(!text.contains('<'), "{text}");

        let JiraFehler::Http { text, .. } = JiraFehler::aus_antwort(500, "   ") else {
            panic!("kein HTTP-Fehler");
        };
        assert_eq!(text, "(ohne Meldung)");
    }

    #[test]
    fn der_anmeldefehler_nennt_keinen_token() {
        let fehler = JiraFehler::Anmeldung {
            verbindung: "firma".into(),
        };
        let text = fehler.to_string();
        assert!(text.contains("firma"), "{text}");
        assert!(text.contains("ungültig oder abgelaufen"), "{text}");
        assert!(text.contains("quiver-Fenster"), "{text}");
    }

    #[test]
    fn der_rechtefehler_nennt_die_ursache() {
        let fehler = JiraFehler::Rechte {
            verbindung: "firma".into(),
            text: "You do not have permission".into(),
        };
        let text = fehler.to_string();
        assert!(text.contains("Keine Rechte"), "{text}");
        assert!(text.contains("firma"), "{text}");
        assert!(text.contains("You do not have permission"), "{text}");
    }

    #[test]
    fn ein_fremder_host_wird_im_text_genannt() {
        let fehler = JiraFehler::FremderHost {
            host: "boese.example.test".into(),
        };
        let text = fehler.to_string();
        assert!(text.contains("boese.example.test"), "{text}");
        assert!(text.contains("nicht angefragt"), "{text}");
    }
}
