// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Was beim Sprechen mit einem SAP-System schiefgehen kann (Aufgabe 07).
//!
//! Zwei Regeln gelten für jeden Text aus diesem Modul: er enthält **kein
//! Passwort und keine Cookies**, und er ist für einen Agenten lesbar. ADT
//! antwortet auf Fehler mit einer Ausnahme (`exc:exception`) aus Typ und
//! Nachricht; die wird als [`AdtFehler::Ausnahme`] durchgereicht, damit der
//! Agent die SAP-Meldung sieht und nicht einen HTTP-Code.

use crate::adt::xml::Knoten;

/// Ein Fehler beim Sprechen mit dem SAP-System.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AdtFehler {
    /// HTTP 401: Anmeldung abgelehnt. Wird nie wiederholt – SAP sperrt
    /// Benutzer nach zu vielen Fehlversuchen.
    #[error(
        "Anmeldung an {system} abgelehnt – bitte Benutzer und Passwort im \
         quiver-Fenster prüfen (kein Wiederholen, sonst sperrt SAP den Benutzer)."
    )]
    Anmeldung { system: String },

    /// Der Pfad zeigt auf ein anderes System als das eingetragene.
    #[error(
        "Die Adresse gehört nicht zu {system} – erlaubt sind nur Objektpfade dieses Systems \
         (z. B. /sap/bc/adt/…)."
    )]
    FremdesSystem { system: String },

    /// Das CSRF-Token ließ sich nicht holen.
    #[error("Das CSRF-Token von {system} ließ sich nicht holen – bitte Zugang im quiver-Fenster prüfen.")]
    Csrf { system: String },

    /// Die zustandsbehaftete Sitzung ist beim SAP abgelaufen (Aufgabe 08).
    ///
    /// Damit sind auch alle Locks dieser Sitzung weg. Ein Schreibvorgang wird
    /// **nicht** stillschweigend wiederholt: der Agent sperrt neu.
    #[error(
        "Die Sitzung an {system} ist abgelaufen, damit sind alle Locks verloren – bitte \
         erneut abap_lock aufrufen (und den Quelltext neu schreiben)."
    )]
    SitzungAbgelaufen { system: String },

    /// ADT-Ausnahme: Typ und Nachricht aus `exc:exception`.
    #[error("{typ}: {nachricht}")]
    Ausnahme { typ: String, nachricht: String },

    /// Andere HTTP-Antwort.
    #[error("ADT antwortete mit HTTP {status}: {text}")]
    Http { status: u16, text: String },

    /// Die Antwort war kein lesbares XML.
    #[error("Die Antwort war kein lesbares XML: {text}")]
    Xml { text: String },

    /// Das Netz oder die Adresse stimmt nicht.
    #[error("Keine Verbindung: {text}")]
    Netz { text: String },

    /// Eine Antwort hatte nicht die erwarteten Felder.
    #[error("{text}")]
    Antwort { text: String },
}

impl AdtFehler {
    /// Ein Netzfehler ohne Kopfzeilen und ohne Zugangsdaten.
    pub fn netz(fehler: &reqwest::Error) -> AdtFehler {
        // `reqwest` nennt in seiner Meldung nur Methode und Adresse; Passwort
        // und Kopfzeilen stehen dort nie.
        AdtFehler::Netz {
            text: fehler.to_string(),
        }
    }

    /// Ist das die Meldung eines abgelaufenen Sitzungskontexts?
    ///
    /// SAP antwortet dann mit `400` oder `401` und einem Text wie
    /// „Session timed out“; oft steht die Erklärung im Rumpf oder in der
    /// ADT-Ausnahme. Erkannt wird nur, was wirklich nach Sitzung klingt – ein
    /// gewöhnlicher Fehler bleibt ein gewöhnlicher Fehler.
    pub fn ist_sitzungsablauf(status: u16, text: &str) -> bool {
        if !matches!(status, 400 | 401) {
            return false;
        }
        let text = text.to_ascii_lowercase();
        [
            "session timed out",
            "session timeout",
            "sessiontimedout",
            "session expired",
            "session is no longer",
            "session no longer",
            "icm_http_session_timeout",
            "sitzung abgelaufen",
        ]
        .iter()
        .any(|muster| text.contains(muster))
    }

    /// Aus einer Fehlerantwort eine ADT-Ausnahme lesen, sonst ein HTTP-Fehler.
    ///
    /// Der Text wird gekürzt: eine HTML-Fehlerseite des Portals hat keinen
    /// Wert für einen Agenten.
    pub fn aus_antwort(status: u16, rumpf: &str) -> AdtFehler {
        if rumpf.contains("exc:exception") || rumpf.contains("<exception") {
            if let Ok(baum) = Knoten::parse(rumpf) {
                if let Some(ausnahme) = baum.alle("exception").first() {
                    let typ = ausnahme
                        .kind("type")
                        .and_then(|typ| typ.attr_gefuellt("id"))
                        .unwrap_or("ADT")
                        .to_string();
                    let nachricht = ausnahme
                        .kind_text("message")
                        .map(str::trim)
                        .filter(|text| !text.is_empty())
                        .or_else(|| ausnahme.attr_gefuellt("localizedMessage"))
                        .unwrap_or("Das SAP-System hat einen Fehler gemeldet.")
                        .to_string();
                    return AdtFehler::Ausnahme { typ, nachricht };
                }
            }
        }
        AdtFehler::Http {
            status,
            text: kuerzen(rumpf, 300),
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eine_adt_ausnahme_wird_zu_typ_und_nachricht() {
        let rumpf = r#"<?xml version="1.0" encoding="utf-8"?>
        <exc:exception xmlns:exc="http://www.sap.com/abapxml/types/communicationframework">
          <exc:type id="ExceptionCondition"/>
          <exc:message lang="DE">Objekt ZCL_DEMO existiert nicht.</exc:message>
        </exc:exception>"#;
        let fehler = AdtFehler::aus_antwort(400, rumpf);
        assert_eq!(
            fehler,
            AdtFehler::Ausnahme {
                typ: "ExceptionCondition".into(),
                nachricht: "Objekt ZCL_DEMO existiert nicht.".into(),
            }
        );
        assert_eq!(
            fehler.to_string(),
            "ExceptionCondition: Objekt ZCL_DEMO existiert nicht."
        );
    }

    #[test]
    fn ohne_ausnahme_bleibt_der_http_status() {
        let fehler = AdtFehler::aus_antwort(500, "Internal Server Error");
        assert_eq!(
            fehler,
            AdtFehler::Http {
                status: 500,
                text: "Internal Server Error".into()
            }
        );
        // Lange HTML-Seiten werden gekürzt.
        let lang = "x".repeat(1000);
        let AdtFehler::Http { text, .. } = AdtFehler::aus_antwort(404, &lang) else {
            panic!("kein HTTP-Fehler");
        };
        assert!(text.chars().count() < 400, "{}", text.chars().count());
        assert!(text.ends_with('…'));
    }

    #[test]
    fn die_anmeldung_abgelehnt_nennt_das_system_und_kein_passwort() {
        let fehler = AdtFehler::Anmeldung {
            system: "dev-100".into(),
        };
        let text = fehler.to_string();
        assert!(text.contains("dev-100"), "{text}");
        assert!(text.contains("abgelehnt"), "{text}");
    }

    #[test]
    fn ein_sitzungsablauf_wird_erkannt_und_bleibt_verstaendlich() {
        assert!(AdtFehler::ist_sitzungsablauf(400, "Session timed out"));
        assert!(AdtFehler::ist_sitzungsablauf(
            401,
            "ICM_HTTP_SESSION_TIMEOUT"
        ));
        assert!(AdtFehler::ist_sitzungsablauf(
            400,
            "<exc:message>Session expired, please log on again</exc:message>"
        ));
        // Alles andere ist kein Sitzungsablauf.
        assert!(!AdtFehler::ist_sitzungsablauf(400, "Ungültiger Quelltext"));
        assert!(!AdtFehler::ist_sitzungsablauf(403, "Session timed out"));
        assert!(!AdtFehler::ist_sitzungsablauf(500, "Session timed out"));

        let fehler = AdtFehler::SitzungAbgelaufen {
            system: "dev-100".into(),
        };
        let text = fehler.to_string();
        assert!(text.contains("abgelaufen"), "{text}");
        assert!(text.contains("abap_lock"), "{text}");
        assert!(text.contains("dev-100"), "{text}");
    }
}
