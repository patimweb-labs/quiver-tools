// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Was bei IMAP und SMTP schiefgehen kann.
//!
//! Kein Text dieser Art enthält je ein Passwort: die beiden Protokollschichten
//! ersetzen das Geheimnis in jedem Fehlertext durch `[geheim]`, bevor sie einen
//! [`MailFehler`] bauen (SPEC §3, AGENTS.md §2.9).

/// Ein Fehler aus dem Postfach oder beim Versand.
#[derive(Debug, thiserror::Error)]
pub enum MailFehler {
    /// Im Zugang fehlt etwas oder ein Wert ist unbrauchbar.
    #[error("Der Zugang ist unvollständig: {0}")]
    Konfiguration(String),
    /// Ein Feld hat einen Wert, den das Tool nicht kennt.
    #[error("{0}")]
    Parameter(String),
    /// Der Server war nicht erreichbar.
    #[error("{ziel} war nicht erreichbar: {ursache}")]
    NichtErreichbar { ziel: String, ursache: String },
    /// Der Server hat nicht rechtzeitig geantwortet.
    #[error("{ziel} hat nicht innerhalb von {sekunden} Sekunden geantwortet.")]
    Zeitlimit { ziel: String, sekunden: u64 },
    /// Anmeldung abgelehnt oder nicht möglich.
    #[error("Die Anmeldung an {ziel} ist fehlgeschlagen: {ursache}")]
    Anmeldung { ziel: String, ursache: String },
    /// Ein IMAP-Aufruf ist gescheitert.
    #[error("IMAP-Fehler: {0}")]
    Imap(String),
    /// Ein SMTP-Aufruf ist gescheitert.
    #[error("SMTP-Fehler: {0}")]
    Smtp(String),
    /// Die UID gibt es in diesem Postfach nicht.
    #[error("Keine Nachricht mit UID {uid} in „{postfach}“.")]
    NichtGefunden { uid: u32, postfach: String },
    /// Die Nachricht ließ sich nicht lesen.
    #[error("Die Nachricht ließ sich nicht lesen: {0}")]
    Unlesbar(String),
    /// Eine Nachricht ohne Empfänger lässt sich nicht senden.
    #[error("Die Nachricht hat keine Empfängeradresse.")]
    KeinEmpfaenger,
    /// Eine Adresse ist unbrauchbar.
    #[error("„{0}“ ist keine gültige E-Mail-Adresse.")]
    UngueltigeAdresse(String),
    /// Eine Datei ließ sich nicht schreiben.
    #[error("{0}")]
    Datei(String),
}

impl MailFehler {
    /// Ein Fehlertext ohne das Passwort (Sicherheitsnetz, SPEC §3).
    pub fn ohne_geheimnis(mut self, geheim: &str) -> Self {
        if geheim.is_empty() {
            return self;
        }
        match &mut self {
            MailFehler::NichtErreichbar { ziel, ursache } => {
                *ziel = ziel.replace(geheim, "[geheim]");
                *ursache = ursache.replace(geheim, "[geheim]");
            }
            MailFehler::Zeitlimit { ziel, .. } => {
                *ziel = ziel.replace(geheim, "[geheim]");
            }
            MailFehler::Anmeldung { ziel, ursache } => {
                *ziel = ziel.replace(geheim, "[geheim]");
                *ursache = ursache.replace(geheim, "[geheim]");
            }
            MailFehler::Imap(text)
            | MailFehler::Smtp(text)
            | MailFehler::Unlesbar(text)
            | MailFehler::Parameter(text)
            | MailFehler::Konfiguration(text)
            | MailFehler::UngueltigeAdresse(text)
            | MailFehler::Datei(text) => *text = text.replace(geheim, "[geheim]"),
            MailFehler::NichtGefunden { postfach, .. } => {
                *postfach = postfach.replace(geheim, "[geheim]");
            }
            MailFehler::KeinEmpfaenger => {}
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GEHEIM: &str = "nur-im-test-4711";

    #[test]
    fn jeder_text_verliert_das_geheimnis() {
        let fehler = [
            MailFehler::Konfiguration(format!("Wert {GEHEIM}")),
            MailFehler::Parameter(format!("Wert {GEHEIM}")),
            MailFehler::NichtErreichbar {
                ziel: format!("host {GEHEIM}"),
                ursache: format!("io {GEHEIM}"),
            },
            MailFehler::Zeitlimit {
                ziel: format!("host {GEHEIM}"),
                sekunden: 30,
            },
            MailFehler::Anmeldung {
                ziel: format!("host {GEHEIM}"),
                ursache: format!("nein {GEHEIM}"),
            },
            MailFehler::Imap(format!("bad {GEHEIM}")),
            MailFehler::Smtp(format!("bad {GEHEIM}")),
            MailFehler::NichtGefunden {
                uid: 7,
                postfach: GEHEIM.to_string(),
            },
            MailFehler::Unlesbar(format!("bad {GEHEIM}")),
            MailFehler::UngueltigeAdresse(GEHEIM.to_string()),
            MailFehler::Datei(GEHEIM.to_string()),
        ];
        for fehler in fehler {
            let text = fehler.ohne_geheimnis(GEHEIM).to_string();
            assert!(!text.contains(GEHEIM), "{text}");
            assert!(text.contains("[geheim]"), "{text}");
        }
    }

    #[test]
    fn ohne_geheimnis_bleibt_der_text() {
        let fehler = MailFehler::Imap("kaputt".to_string());
        assert_eq!(fehler.ohne_geheimnis("").to_string(), "IMAP-Fehler: kaputt");
    }
}
