// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Was beim Sprechen mit Zephyr Scale schiefgehen kann (Aufgabe 15).
//!
//! Zwei Regeln gelten für jeden Text aus diesem Modul: er enthält **kein
//! Token** und keine Kopfzeilen, und er ist für einen Agenten lesbar. Zephyr
//! antwortet auf Fehler mit JSON (`{"message": …}` oder `{"errorMessage": …}`);
//! beides wird zu einem kurzen Satz.

/// Ein Fehler beim Sprechen mit Zephyr Scale.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ZephyrFehler {
    /// HTTP 401: das Token gilt nicht (oder ist abgelaufen). Wird nie wiederholt.
    #[error("{}", crate::format::token_ungueltig(.verbindung))]
    Anmeldung { verbindung: String },

    /// HTTP 403: das Token darf nicht, was der Aufruf will.
    #[error("{}", crate::format::keine_rechte(.verbindung, .text))]
    Rechte { verbindung: String, text: String },

    /// Andere HTTP-Antwort.
    #[error("Zephyr antwortete mit HTTP {status}: {text}")]
    Http { status: u16, text: String },

    /// Eine Weiterleitung. Das Token geht nur an den eingetragenen Host, also
    /// wird sie nicht verfolgt (Aufgabe 15, Regel 2).
    #[error(
        "Zephyr antwortete mit einer Weiterleitung (HTTP {status}) – quiver-zephyr folgt ihr \
             nicht, damit das Token den eingetragenen Host nicht verlässt."
    )]
    Weiterleitung { status: u16 },

    /// Eine Folgeseite zeigt auf einen fremden Host.
    #[error(
        "Die Folgeseite zeigt auf den fremden Host „{host}“ – quiver-zephyr folgt ihr nicht, \
         damit das Token den eingetragenen Host nicht verlässt."
    )]
    FremdeFolgeseite { host: String },

    /// Das Netz oder die Adresse stimmt nicht.
    #[error("Keine Verbindung: {text}")]
    Netz { text: String },

    /// Eine Antwort hatte nicht die erwarteten Felder.
    #[error("{text}")]
    Antwort { text: String },

    /// Die Antwort war kein gültiges JSON.
    #[error("Die Antwort von Zephyr war kein lesbares JSON: {text}")]
    Json { text: String },

    /// Der Rumpf einer lesenden Antwort ließ sich nicht lesen.
    ///
    /// Das ist ein Fehler und **keine** leere Antwort: wer hier „nichts
    /// gefunden“ meldet, behauptet etwas, das er nicht weiß.
    #[error(
        "Zephyr antwortete mit HTTP {status}, aber der Antwortrumpf ließ sich nicht lesen \
         ({text}) – das ist ein Fehler, keine leere Antwort."
    )]
    Unlesbar { status: u16, text: String },

    /// Eine Änderung kam an (2xx), ihre Antwort ließ sich aber nicht lesen.
    #[error(
        "Zephyr hat die Änderung ausgeführt (HTTP {status}), aber die Antwort ließ sich nicht \
         lesen – bitte den Stand in Zephyr prüfen, statt den Aufruf zu wiederholen."
    )]
    AusgefuehrtNichtLesbar { status: u16 },

    /// Eine Änderung ging ohne Antwort zu Ende (Zeitüberschreitung, Abbruch).
    ///
    /// Ob sie ankam, weiß nur der Server – darum wird sie nicht wiederholt.
    #[error(
        "Ausgang unklar – Stand prüfen, nicht einfach wiederholen: der Aufruf an {verbindung} \
         endete ohne Antwort ({text})."
    )]
    AusgangUnklar { verbindung: String, text: String },

    /// Ein Wert taugt nicht für einen Pfad oder eine Abfrage.
    #[error("{text}")]
    Parameter { text: String },
}

impl ZephyrFehler {
    /// Ein Netzfehler ohne Kopfzeilen und ohne Token.
    pub fn netz(fehler: &reqwest::Error) -> ZephyrFehler {
        // `reqwest` nennt in seiner Meldung nur Methode und Adresse; das Token
        // steht dort nie.
        ZephyrFehler::Netz {
            text: fehler.to_string(),
        }
    }

    /// Der Fehler beim **Senden** – bei einer Änderung ist der Ausgang offen.
    ///
    /// Scheitert schon der Verbindungsaufbau (`is_connect`), ist nichts
    /// angekommen. Bei einer Zeitüberschreitung oder einem Abbruch danach kann
    /// die Änderung sehr wohl angekommen sein: dann heißt es „Ausgang unklar“,
    /// nicht „nochmal“ (Aufgabe 15 b, Regel 3).
    pub fn beim_senden(fehler: reqwest::Error, lesend: bool, verbindung: &str) -> ZephyrFehler {
        if !lesend && !fehler.is_connect() {
            return ZephyrFehler::AusgangUnklar {
                verbindung: verbindung.to_string(),
                text: fehler.to_string(),
            };
        }
        ZephyrFehler::netz(&fehler)
    }

    /// Aus einer Fehlerantwort einen kurzen Satz machen.
    pub fn aus_antwort(status: u16, rumpf: &str) -> ZephyrFehler {
        ZephyrFehler::Http {
            status,
            text: meldung(rumpf),
        }
    }

    /// Sagt der Server „gibt es nicht“ (HTTP 404)?
    pub fn ist_nicht_gefunden(&self) -> bool {
        matches!(self, ZephyrFehler::Http { status: 404, .. })
    }

    /// Sagt der Server „so nicht“ (HTTP 422)?
    ///
    /// Zephyr nutzt 422 unter anderem für „dieser Testfall hat Schritte, kein
    /// Skript“ – das ist kein Fehler, sondern eine Auskunft.
    pub fn ist_nicht_verarbeitbar(&self) -> bool {
        matches!(self, ZephyrFehler::Http { status: 422, .. })
    }
}

/// Der Grund aus einer Fehlerantwort, kurz und ohne HTML.
///
/// Zephyr meldet ihn in `message` oder `errorMessage`; ein langer HTML-Rumpf
/// (Proxy, Anmeldeseite) wird zu Text ohne Tags.
pub fn meldung(rumpf: &str) -> String {
    if let Ok(wert) = serde_json::from_str::<serde_json::Value>(rumpf) {
        for feld in ["message", "errorMessage"] {
            if let Some(meldung) = wert.get(feld).and_then(serde_json::Value::as_str) {
                return kuerzen(meldung, 300);
            }
        }
    }
    kuerzen(&ohne_html(rumpf), 300)
}

/// Sieht der Rumpf nach einer HTML-Seite aus?
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
    fn eine_zephyr_meldung_wird_zu_einem_kurzen_satz() {
        let fehler = ZephyrFehler::aus_antwort(
            400,
            r#"{"errorCode":400,"message":"projectKey is required"}"#,
        );
        assert_eq!(
            fehler,
            ZephyrFehler::Http {
                status: 400,
                text: "projectKey is required".into()
            }
        );
        let fehler = ZephyrFehler::aus_antwort(404, r#"{"errorMessage":"Test case not found."}"#);
        assert_eq!(
            fehler,
            ZephyrFehler::Http {
                status: 404,
                text: "Test case not found.".into()
            }
        );
    }

    #[test]
    fn html_und_lange_texte_werden_gekuerzt() {
        assert!(ist_html("<!DOCTYPE html><html><body>x</body></html>"));
        assert!(!ist_html(r#"{"message":"nein"}"#));

        let seite = format!("<html><body>{}</body></html>", "x".repeat(1000));
        let ZephyrFehler::Http { text, .. } = ZephyrFehler::aus_antwort(500, &seite) else {
            panic!("kein HTTP-Fehler");
        };
        assert!(text.chars().count() < 400, "{}", text.chars().count());
        assert!(text.ends_with('…'));
        assert!(!text.contains('<'), "{text}");
    }

    #[test]
    fn anmeldung_und_rechte_nennen_die_verbindung_und_kein_token() {
        let fehler = ZephyrFehler::Anmeldung {
            verbindung: "kunde".into(),
        };
        let text = fehler.to_string();
        assert!(text.contains("kunde"), "{text}");
        assert!(text.contains("ungültig oder abgelaufen"), "{text}");
        assert!(text.contains("Tools → Zephyr"), "{text}");

        let text = ZephyrFehler::Rechte {
            verbindung: "kunde".into(),
            text: "Forbidden".into(),
        }
        .to_string();
        assert!(text.contains("Rechte"), "{text}");
        assert!(text.contains("403"), "{text}");
    }

    #[test]
    fn eine_weiterleitung_und_eine_fremde_folgeseite_werden_abgelehnt() {
        let text = ZephyrFehler::Weiterleitung { status: 302 }.to_string();
        assert!(text.contains("Weiterleitung"), "{text}");
        assert!(text.contains("folgt ihr nicht"), "{text}");

        let text = ZephyrFehler::FremdeFolgeseite {
            host: "fremder.example.test".into(),
        }
        .to_string();
        assert!(text.contains("fremder.example.test"), "{text}");
        assert!(text.contains("folgt ihr nicht"), "{text}");
    }

    #[test]
    fn ein_unlesbarer_rumpf_ist_ein_fehler_und_keine_leere_antwort() {
        let text = ZephyrFehler::Unlesbar {
            status: 200,
            text: "incomplete message".into(),
        }
        .to_string();
        assert!(text.contains("nicht lesen"), "{text}");
        assert!(text.contains("keine leere Antwort"), "{text}");

        let text = ZephyrFehler::AusgefuehrtNichtLesbar { status: 201 }.to_string();
        assert!(text.contains("ausgeführt"), "{text}");
        assert!(text.contains("nicht lesen"), "{text}");
        assert!(text.contains("statt den Aufruf zu wiederholen"), "{text}");

        let text = ZephyrFehler::AusgangUnklar {
            verbindung: "kunde".into(),
            text: "timed out".into(),
        }
        .to_string();
        assert!(text.contains("Ausgang unklar"), "{text}");
        assert!(
            text.contains("Stand prüfen, nicht einfach wiederholen"),
            "{text}"
        );
        assert!(text.contains("kunde"), "{text}");
    }
}
