// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Was beim Sprechen mit Confluence schiefgehen kann (Aufgabe 14).
//!
//! Zwei Regeln gelten für jeden Text aus diesem Modul: er enthält **weder
//! Token noch E-Mail** und keine Kopfzeilen (Aufgabe 12, Sicherheit 3), und er
//! ist für einen Agenten lesbar. Confluence antwortet auf Fehler mit JSON
//! (`{"message": …}` oder `{"errors": [ … ]}`); daraus wird ein kurzer Satz.

use crate::client::kurz;

/// Ein Fehler beim Sprechen mit Confluence.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Fehler {
    /// HTTP 401: Token oder E-Mail gelten nicht. Wird nie wiederholt.
    #[error("{}", crate::format::token_ungueltig(.verbindung))]
    Anmeldung { verbindung: String },

    /// HTTP 403: das Konto darf das nicht.
    #[error("{}", crate::format::keine_rechte(.verbindung))]
    Rechte { verbindung: String },

    /// Eine Folgeseite gehört zu einem anderen Host: keine Anfrage.
    #[error(
        "Die Folgeseite „{url}“ gehört zu einem anderen Host – die Anfrage wurde nicht gesendet."
    )]
    FremderHost { url: String },

    /// Eine Adresse taugt nicht (Benutzerteil, kein http/https …): keine Anfrage.
    #[error("Die Adresse „{text}“ ist nicht verwendbar – die Anfrage wurde nicht gesendet.")]
    Adresse { text: String },

    /// Ein Wert taugt nicht als Pfadteil (leer, `.` oder `..`): keine Anfrage.
    #[error(
        "Der Wert „{wert}“ ist als Pfadteil nicht erlaubt (leer, „.“ oder „..“) – die Anfrage \
         wurde nicht gesendet."
    )]
    Pfad { wert: String },

    /// Eine Änderung kam ohne Antwort zurück (Zeitüberschreitung, Abbruch).
    #[error(
        "Ausgang unklar – die Änderung kam ohne Antwort zurück ({text}). Stand prüfen, nicht \
         einfach wiederholen."
    )]
    Unklar { text: String },

    /// Eine Änderung war erfolgreich, aber ihre Antwort war nicht lesbar.
    #[error(
        "Die Änderung wurde ausgeführt, die Antwort war nicht lesbar ({text}). Stand prüfen, \
         nicht einfach wiederholen."
    )]
    Ausgefuehrt { text: String },

    /// Der Rumpf einer lesenden Antwort war nicht lesbar – nie „leer“.
    #[error("Die Antwort von Confluence war nicht lesbar: {text}")]
    Unlesbar { text: String },

    /// Andere HTTP-Antwort.
    #[error("Confluence antwortete mit HTTP {status}: {text}")]
    Http { status: u16, text: String },

    /// Das Netz oder die Adresse stimmt nicht.
    #[error("Keine Verbindung: {text}")]
    Netz { text: String },

    /// Eine Antwort hatte nicht die erwarteten Felder.
    #[error("{text}")]
    Antwort { text: String },

    /// Die Antwort war kein gültiges JSON.
    #[error("Die Antwort von Confluence war kein lesbares JSON: {text}")]
    Json { text: String },
}

impl Fehler {
    /// Ein Netzfehler ohne Kopfzeilen und ohne Zugangsdaten.
    pub fn netz(fehler: &reqwest::Error) -> Fehler {
        // `reqwest` nennt in seiner Meldung nur Methode und Adresse; Token und
        // E-Mail stehen dort nie.
        Fehler::Netz {
            text: fehler.to_string(),
        }
    }

    /// Eine Änderung ohne Antwort: ob sie ankam, weiß nur der Server.
    pub fn unklar(fehler: &reqwest::Error) -> Fehler {
        Fehler::Unklar {
            text: fehler.to_string(),
        }
    }

    /// Eine Änderung mit 2xx, aber der Rumpf ließ sich nicht lesen.
    pub fn ausgefuehrt(fehler: &reqwest::Error) -> Fehler {
        Fehler::Ausgefuehrt {
            text: fehler.to_string(),
        }
    }

    /// Ein lesender Aufruf, dessen Rumpf nicht lesbar war.
    pub fn unlesbar(fehler: &reqwest::Error) -> Fehler {
        Fehler::Unlesbar {
            text: fehler.to_string(),
        }
    }

    /// Aus einer Fehlerantwort einen kurzen Satz machen.
    ///
    /// Confluence v1 meldet `message`, v2 eine Liste `errors`. Ein langer
    /// HTML-Rumpf (Proxy, Anmeldeseite) wird verworfen.
    pub fn aus_antwort(status: u16, rumpf: &str) -> Fehler {
        if let Ok(wert) = serde_json::from_str::<serde_json::Value>(rumpf) {
            if let Some(meldung) = wert.get("message").and_then(serde_json::Value::as_str) {
                return Fehler::Http {
                    status,
                    text: kurz_saetze(meldung, 300),
                };
            }
            if let Some(fehler) = wert.get("errors") {
                // `[{ "title": …, "detail": … }]` zu einem Satz.
                let texte: Vec<String> = fehler
                    .as_array()
                    .map(|liste| {
                        liste
                            .iter()
                            .filter_map(|eintrag| {
                                eintrag
                                    .get("title")
                                    .and_then(serde_json::Value::as_str)
                                    .map(str::to_string)
                                    .or_else(|| {
                                        eintrag
                                            .get("detail")
                                            .and_then(serde_json::Value::as_str)
                                            .map(str::to_string)
                                    })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                if !texte.is_empty() {
                    return Fehler::Http {
                        status,
                        text: kurz_saetze(&texte.join(" · "), 300),
                    };
                }
            }
        }
        Fehler::Http {
            status,
            text: kurz_saetze(&ohne_html(rumpf), 300),
        }
    }
}

/// Grobe Kürzung für Fehlertexte.
fn kurz_saetze(text: &str, max: usize) -> String {
    let sauber = text.trim();
    if sauber.chars().count() <= max {
        return sauber.to_string();
    }
    let gekuerzt: String = sauber.chars().take(max).collect();
    format!("{} …", gekuerzt.trim_end())
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

/// Ein Text für eine Fehlermeldung, nie länger als `kurz`.
pub fn ohne_zugang(url: &str) -> String {
    kurz(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eine_confluence_meldung_wird_zu_einem_kurzen_satz() {
        let fehler = Fehler::aus_antwort(
            404,
            r#"{"statusCode":404,"message":"Cannot find page with id 999"}"#,
        );
        assert_eq!(
            fehler,
            Fehler::Http {
                status: 404,
                text: "Cannot find page with id 999".into()
            }
        );
    }

    #[test]
    fn die_v2_fehlerliste_wird_gelesen() {
        let fehler = Fehler::aus_antwort(
            400,
            r#"{"errors":[{"status":400,"code":"INVALID_REQUEST_PARAMETER","title":"Title is missing","detail":"a title is required"}]}"#,
        );
        assert_eq!(
            fehler,
            Fehler::Http {
                status: 400,
                text: "Title is missing".into()
            }
        );
    }

    #[test]
    fn html_und_lange_texte_werden_gekuerzt() {
        let seite = format!("<html><body>{}</body></html>", "x".repeat(1000));
        let Fehler::Http { text, .. } = Fehler::aus_antwort(502, &seite) else {
            panic!("kein HTTP-Fehler");
        };
        assert!(text.chars().count() < 400, "{}", text.chars().count());
        assert!(text.ends_with('…'));
        assert!(!text.contains('<'), "{text}");
    }

    #[test]
    fn ein_anmeldefehler_nennt_das_quiver_fenster_und_keinen_token() {
        let fehler = Fehler::Anmeldung {
            verbindung: "firma".into(),
        };
        let text = fehler.to_string();
        assert!(text.contains("firma"), "{text}");
        assert!(text.contains("ungültig oder abgelaufen"), "{text}");
        assert!(text.contains("Tools → Confluence"), "{text}");

        let fehler = Fehler::Rechte {
            verbindung: "firma".into(),
        };
        let text = fehler.to_string();
        assert!(text.contains("Fehlende Rechte"), "{text}");
        assert!(text.contains("Tools → Confluence"), "{text}");
    }

    #[test]
    fn ein_unlesbarer_rumpf_ist_ein_fehler_und_nie_leer() {
        assert!(Fehler::Unlesbar {
            text: "incomplete message".into()
        }
        .to_string()
        .contains("nicht lesbar"));

        // Eine Änderung mit 2xx sagt, dass sie ausgeführt wurde.
        let text = Fehler::Ausgefuehrt {
            text: "incomplete message".into(),
        }
        .to_string();
        assert!(text.contains("ausgeführt"), "{text}");
        assert!(text.contains("nicht lesbar"), "{text}");

        // Ohne Antwort ist der Ausgang unklar – kein Wiederholen.
        let text = Fehler::Unklar {
            text: "timeout".into(),
        }
        .to_string();
        assert!(text.contains("Ausgang unklar"), "{text}");
        assert!(text.contains("nicht einfach wiederholen"), "{text}");
    }

    #[test]
    fn eine_fremde_folgeseite_wird_ohne_zugangsdaten_gemeldet() {
        let fehler = Fehler::FremderHost {
            url: "https://evil.test/wiki/api/v2/spaces?cursor=2".into(),
        };
        let text = fehler.to_string();
        assert!(text.contains("evil.test"), "{text}");
        assert!(text.contains("nicht gesendet"), "{text}");
    }
}
