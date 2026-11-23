// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Das Werkzeug „E-Mail“ (SPEC §2, §4, Aufgabe 04).
//!
//! [`Email`] sagt dem Kern, welche Felder der Zugang hat und wie man die
//! Verbindung prüft. Die Felder selbst kommen aus der Umgebung
//! (`EMAIL_ADRESSE`, `EMAIL_PASSWORT`, …), die quiver aus dem
//! Bibliothekseintrag setzt. Das Passwort liegt im Schlüsselbund und steht in
//! keiner Antwort.

use quiver_tools_kern::{Feld, FeldArt, Profil, Werkzeug};

use crate::domain::{schalter, zahl, ImapKonfiguration, Konto, Sicherheit, SmtpKonfiguration};
use crate::fehler::MailFehler;
use crate::postfach::{ImapPostfach, Postfach};
use crate::versand::{LettreVersand, Versand};

/// Kurzname: Teil der Umgebungsvariablen (`EMAIL_…`) und der Werkzeugnamen.
pub const NAME: &str = "email";
/// Anzeigename im quiver-Fenster und in MCP.
pub const TITEL: &str = "E-Mail";
/// Absenderadresse, Pflicht (`EMAIL_ADRESSE`).
pub const ADRESSE: &str = "adresse";
/// Anzeigename des Absenders (`EMAIL_NAME`).
pub const ANZEIGENAME: &str = "name";
/// IMAP-Server, Pflicht (`EMAIL_IMAP_HOST`).
pub const IMAP_HOST: &str = "imap_host";
/// IMAP-Port (`EMAIL_IMAP_PORT`).
pub const IMAP_PORT: &str = "imap_port";
/// IMAP-Verschlüsselung: `tls`, `starttls` oder `keine`.
pub const IMAP_SICHERHEIT: &str = "imap_sicherheit";
/// SMTP-Server, Pflicht (`EMAIL_SMTP_HOST`).
pub const SMTP_HOST: &str = "smtp_host";
/// SMTP-Port (`EMAIL_SMTP_PORT`).
pub const SMTP_PORT: &str = "smtp_port";
/// SMTP-Verschlüsselung.
pub const SMTP_SICHERHEIT: &str = "smtp_sicherheit";
/// Anmeldename für IMAP und SMTP; leer heißt: die Adresse (`EMAIL_BENUTZER`).
pub const BENUTZER: &str = "benutzer";
/// Passwort, geheim (`EMAIL_PASSWORT`).
pub const PASSWORT: &str = "passwort";
/// Zertifikat prüfen (`EMAIL_ZERTIFIKAT_PRUEFEN`, Standard `an`).
pub const ZERTIFIKAT: &str = "zertifikat_pruefen";

/// Das E-Mail-Werkzeug.
#[derive(Debug, Clone, Copy, Default)]
pub struct Email;

impl Werkzeug for Email {
    fn name(&self) -> &'static str {
        NAME
    }

    fn titel(&self) -> &'static str {
        TITEL
    }

    fn beschreibung(&self) -> &'static str {
        "E-Mail über IMAP und SMTP: lesen, suchen, senden, antworten und sortieren."
    }

    fn felder(&self) -> Vec<Feld> {
        vec![
            Feld {
                name: ADRESSE,
                titel: "Adresse",
                art: FeldArt::Text,
                pflicht: true,
                standard: None,
                hilfe: "Absenderadresse, z. B. du@gmail.com",
                link: None,
            },
            Feld {
                name: ANZEIGENAME,
                titel: "Name",
                art: FeldArt::Text,
                pflicht: false,
                standard: None,
                hilfe: "Anzeigename des Absenders",
                link: None,
            },
            Feld {
                name: IMAP_HOST,
                titel: "IMAP-Server",
                art: FeldArt::Text,
                pflicht: true,
                standard: None,
                hilfe: "z. B. imap.gmail.com; Proton-Bridge: 127.0.0.1",
                link: Some((
                    "Proton-Bridge einrichten",
                    "https://proton.me/support/bridge",
                )),
            },
            Feld {
                name: IMAP_PORT,
                titel: "IMAP-Port",
                art: FeldArt::Zahl,
                pflicht: false,
                standard: Some("993"),
                hilfe: "Meist 993 (TLS) oder 143 (STARTTLS); Proton-Bridge: 1143",
                link: None,
            },
            Feld {
                name: IMAP_SICHERHEIT,
                titel: "IMAP-Verschlüsselung",
                art: FeldArt::Auswahl(vec![
                    ("tls".to_string(), "TLS".to_string()),
                    ("starttls".to_string(), "STARTTLS".to_string()),
                    ("keine".to_string(), "Keine".to_string()),
                ]),
                pflicht: false,
                standard: Some("tls"),
                hilfe: "TLS ab dem ersten Byte; STARTTLS für Port 143",
                link: None,
            },
            Feld {
                name: SMTP_HOST,
                titel: "SMTP-Server",
                art: FeldArt::Text,
                pflicht: true,
                standard: None,
                hilfe: "z. B. smtp.gmail.com; Proton-Bridge: 127.0.0.1",
                link: None,
            },
            Feld {
                name: SMTP_PORT,
                titel: "SMTP-Port",
                art: FeldArt::Zahl,
                pflicht: false,
                standard: Some("587"),
                hilfe: "Meist 587 (STARTTLS) oder 465 (TLS); Proton-Bridge: 1025",
                link: None,
            },
            Feld {
                name: SMTP_SICHERHEIT,
                titel: "SMTP-Verschlüsselung",
                art: FeldArt::Auswahl(vec![
                    ("tls".to_string(), "TLS".to_string()),
                    ("starttls".to_string(), "STARTTLS".to_string()),
                    ("keine".to_string(), "Keine".to_string()),
                ]),
                pflicht: false,
                standard: Some("starttls"),
                hilfe: "STARTTLS für Port 587, TLS für Port 465",
                link: None,
            },
            Feld {
                name: BENUTZER,
                titel: "Benutzername",
                art: FeldArt::Text,
                pflicht: false,
                standard: None,
                hilfe: "Anmeldename für IMAP und SMTP; leer = Adresse",
                link: None,
            },
            Feld {
                name: PASSWORT,
                titel: "Passwort",
                art: FeldArt::Passwort,
                pflicht: true,
                standard: None,
                hilfe: "Bei Gmail ein App-Passwort, nicht das Kontopasswort",
                link: None,
            },
            Feld {
                name: ZERTIFIKAT,
                titel: "Zertifikat prüfen",
                art: FeldArt::Schalter,
                pflicht: false,
                standard: Some("an"),
                hilfe: "Nur für lokale Brücken (127.0.0.1, localhost) wirksam",
                link: None,
            },
        ]
    }

    async fn testen(&self, profil: &Profil) -> anyhow::Result<String> {
        let konto = konto(profil)?;
        ImapPostfach::neu(konto.imap.clone()).pruefen().await?;
        LettreVersand::neu(&konto)?.pruefen().await?;
        Ok(format!(
            "IMAP und SMTP erreichbar, angemeldet als {}",
            konto.benutzer()
        ))
    }
}

impl Konto {
    /// Der Anmeldename, wie ihn beide Protokolle benutzen.
    pub fn benutzer(&self) -> &str {
        if self.imap.benutzer.is_empty() {
            &self.adresse
        } else {
            &self.imap.benutzer
        }
    }
}

/// Der eine Zugang aus dem Profil, mit allen Vorgaben der Aufgabe.
pub fn konto(profil: &Profil) -> Result<Konto, MailFehler> {
    let werte = &profil.werte;

    let adresse = pflicht(werte.get(ADRESSE), ADRESSE)?.to_string();
    let passwort = match profil.geheim.get(PASSWORT) {
        Some(passwort) if !passwort.trim().is_empty() => passwort.clone(),
        _ => {
            return Err(MailFehler::Konfiguration(format!(
                "{PASSWORT} fehlt im Zugang"
            )));
        }
    };
    let name = werte
        .get(ANZEIGENAME)
        .map(|wert| wert.trim())
        .filter(|wert| !wert.is_empty())
        .map(str::to_string);
    let benutzer = werte
        .get(BENUTZER)
        .map(|wert| wert.trim())
        .filter(|wert| !wert.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| adresse.clone());

    let imap_sicherheit = sicherheit(werte.get(IMAP_SICHERHEIT), Sicherheit::Tls, IMAP_SICHERHEIT)?;
    let smtp_sicherheit = sicherheit(
        werte.get(SMTP_SICHERHEIT),
        Sicherheit::Starttls,
        SMTP_SICHERHEIT,
    )?;
    let zertifikat = schalter(werte.get(ZERTIFIKAT).map(String::as_str), true);

    let imap_host = pflicht(werte.get(IMAP_HOST), IMAP_HOST)?.to_string();
    let smtp_host = pflicht(werte.get(SMTP_HOST), SMTP_HOST)?.to_string();

    Ok(Konto {
        adresse,
        name,
        imap: ImapKonfiguration {
            port: zahl(werte.get(IMAP_PORT).map(String::as_str), 993, IMAP_PORT)?,
            sicherheit: imap_sicherheit,
            zertifikat_pruefen: Konto::prueft_zertifikat(zertifikat, &imap_host),
            host: imap_host,
            benutzer: benutzer.clone(),
            passwort: passwort.clone(),
        },
        smtp: SmtpKonfiguration {
            port: zahl(werte.get(SMTP_PORT).map(String::as_str), 587, SMTP_PORT)?,
            sicherheit: smtp_sicherheit,
            zertifikat_pruefen: Konto::prueft_zertifikat(zertifikat, &smtp_host),
            host: smtp_host,
            benutzer,
            passwort,
        },
    })
}

/// Ein Pflichtwert aus dem Profil; der Fehler nennt nur den Feldnamen.
fn pflicht<'a>(wert: Option<&'a String>, feld: &str) -> Result<&'a str, MailFehler> {
    match wert.map(|wert| wert.trim()).filter(|wert| !wert.is_empty()) {
        Some(wert) => Ok(wert),
        None => Err(MailFehler::Konfiguration(format!("{feld} fehlt im Zugang"))),
    }
}

/// Die Verschlüsselung aus einem Feld; ein unbekannter Wert wird gemeldet.
fn sicherheit(
    wert: Option<&String>,
    standard: Sicherheit,
    feld: &str,
) -> Result<Sicherheit, MailFehler> {
    let Some(wert) = wert.map(|wert| wert.trim()).filter(|wert| !wert.is_empty()) else {
        return Ok(standard);
    };
    Sicherheit::aus_text(wert).ok_or_else(|| {
        MailFehler::Konfiguration(format!(
            "{feld} „{wert}“ ist unbekannt – erlaubt sind tls, starttls und keine"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ist_lokaler_host;

    /// Ein vollständiger Zugang als Paare; das Passwort liegt in `geheim`.
    fn profil(werte: &[(&str, &str)]) -> Profil {
        let mut profil = Profil::leer();
        for (feld, wert) in werte {
            if *feld == PASSWORT {
                profil.geheim.insert(feld.to_string(), (*wert).to_string());
            } else {
                profil.werte.insert(feld.to_string(), (*wert).to_string());
            }
        }
        profil
    }

    fn vollstaendig() -> Profil {
        profil(&[
            (ADRESSE, "ich@example.test"),
            (ANZEIGENAME, "Ich"),
            (IMAP_HOST, "imap.example.test"),
            (SMTP_HOST, "smtp.example.test"),
            (PASSWORT, "nur-im-test-4711"),
        ])
    }

    #[test]
    fn werkzeug_nennt_name_titel_und_elf_felder() {
        let werkzeug = Email;
        assert_eq!(werkzeug.name(), "email");
        assert_eq!(werkzeug.titel(), "E-Mail");
        assert!(werkzeug.beschreibung().contains("IMAP"));

        let felder = werkzeug.felder();
        assert_eq!(felder.len(), 11, "{felder:?}");
        let namen: Vec<&str> = felder.iter().map(|feld| feld.name).collect();
        assert_eq!(
            namen,
            vec![
                "adresse",
                "name",
                "imap_host",
                "imap_port",
                "imap_sicherheit",
                "smtp_host",
                "smtp_port",
                "smtp_sicherheit",
                "benutzer",
                "passwort",
                "zertifikat_pruefen",
            ]
        );

        let passwort = felder.iter().find(|feld| feld.name == PASSWORT).unwrap();
        assert_eq!(passwort.art, FeldArt::Passwort);
        assert!(passwort.pflicht);
        assert_eq!(passwort.standard, None);

        let port = felder.iter().find(|feld| feld.name == IMAP_PORT).unwrap();
        assert_eq!(port.art, FeldArt::Zahl);
        assert_eq!(port.standard, Some("993"));
        assert!(!port.pflicht);

        let sicherheit = felder
            .iter()
            .find(|feld| feld.name == SMTP_SICHERHEIT)
            .unwrap();
        assert_eq!(
            sicherheit.art,
            FeldArt::Auswahl(vec![
                ("tls".to_string(), "TLS".to_string()),
                ("starttls".to_string(), "STARTTLS".to_string()),
                ("keine".to_string(), "Keine".to_string()),
            ])
        );
        assert_eq!(sicherheit.standard, Some("starttls"));

        let schalter = felder.iter().find(|feld| feld.name == ZERTIFIKAT).unwrap();
        assert_eq!(schalter.art, FeldArt::Schalter);
        assert_eq!(schalter.standard, Some("an"));
    }

    #[test]
    fn die_vorgaben_der_aufgabe_werden_gesetzt() {
        let gelesen = konto(&vollstaendig()).unwrap();
        assert_eq!(gelesen.adresse, "ich@example.test");
        assert_eq!(gelesen.name.as_deref(), Some("Ich"));
        assert_eq!(gelesen.benutzer(), "ich@example.test");
        assert_eq!(gelesen.imap.port, 993);
        assert_eq!(gelesen.imap.sicherheit, Sicherheit::Tls);
        assert_eq!(gelesen.smtp.port, 587);
        assert_eq!(gelesen.smtp.sicherheit, Sicherheit::Starttls);
        assert!(gelesen.imap.zertifikat_pruefen);
        assert!(gelesen.smtp.zertifikat_pruefen);
        assert_eq!(gelesen.smtp.passwort, "nur-im-test-4711");
    }

    #[test]
    fn benutzer_ohne_angabe_ist_die_adresse() {
        let gelesen = konto(&vollstaendig()).unwrap();
        assert_eq!(gelesen.imap.benutzer, "ich@example.test");
        assert_eq!(gelesen.smtp.benutzer, "ich@example.test");

        let mit_benutzer = konto(&profil(&[
            (ADRESSE, "ich@example.test"),
            (IMAP_HOST, "imap.example.test"),
            (SMTP_HOST, "smtp.example.test"),
            (BENUTZER, "anmeldung@example.test"),
            (PASSWORT, "nur-im-test-4711"),
        ]))
        .unwrap();
        assert_eq!(mit_benutzer.benutzer(), "anmeldung@example.test");
    }

    #[test]
    fn verschluesselung_und_ports_kommen_aus_der_umgebung() {
        let gelesen = konto(&profil(&[
            (ADRESSE, "ich@example.test"),
            (IMAP_HOST, "127.0.0.1"),
            (IMAP_PORT, "1143"),
            (IMAP_SICHERHEIT, "starttls"),
            (SMTP_HOST, "127.0.0.1"),
            (SMTP_PORT, "1025"),
            (SMTP_SICHERHEIT, "keine"),
            (ZERTIFIKAT, "aus"),
            (PASSWORT, "nur-im-test-4711"),
        ]))
        .unwrap();
        assert_eq!(gelesen.imap.port, 1143);
        assert_eq!(gelesen.imap.sicherheit, Sicherheit::Starttls);
        assert_eq!(gelesen.smtp.port, 1025);
        assert_eq!(gelesen.smtp.sicherheit, Sicherheit::Keine);
        assert!(!gelesen.imap.zertifikat_pruefen);
        assert!(!gelesen.smtp.zertifikat_pruefen);
    }

    #[test]
    fn ein_fremder_host_ignoriert_den_zertifikatsschalter() {
        let gelesen = konto(&profil(&[
            (ADRESSE, "ich@example.test"),
            (IMAP_HOST, "imap.example.test"),
            (SMTP_HOST, "smtp.example.test"),
            (ZERTIFIKAT, "aus"),
            (PASSWORT, "nur-im-test-4711"),
        ]))
        .unwrap();
        assert!(gelesen.imap.zertifikat_pruefen);
        assert!(gelesen.smtp.zertifikat_pruefen);
    }

    #[test]
    fn ohne_adresse_oder_passwort_kein_konto() {
        let ohne_adresse = konto(&profil(&[
            (IMAP_HOST, "imap.example.test"),
            (SMTP_HOST, "smtp.example.test"),
            (PASSWORT, "nur-im-test-4711"),
        ]))
        .unwrap_err()
        .to_string();
        assert!(ohne_adresse.contains("adresse"), "{ohne_adresse}");

        let ohne_passwort = konto(&profil(&[
            (ADRESSE, "ich@example.test"),
            (IMAP_HOST, "imap.example.test"),
            (SMTP_HOST, "smtp.example.test"),
        ]))
        .unwrap_err()
        .to_string();
        assert!(ohne_passwort.contains("passwort"), "{ohne_passwort}");
    }

    #[test]
    fn ein_unbekannter_wert_wird_gemeldet_ohne_geheimnis() {
        let fehler = konto(&profil(&[
            (ADRESSE, "ich@example.test"),
            (IMAP_HOST, "imap.example.test"),
            (IMAP_SICHERHEIT, "quatsch"),
            (SMTP_HOST, "smtp.example.test"),
            (PASSWORT, "nur-im-test-4711"),
        ]))
        .unwrap_err()
        .to_string();
        assert!(fehler.contains("imap_sicherheit"), "{fehler}");
        assert!(!fehler.contains("nur-im-test-4711"), "{fehler}");
    }

    #[test]
    fn ein_lokaler_host_kommt_mit_dem_schalter_aus() {
        assert!(ist_lokaler_host("127.0.0.1"));
        assert!(ist_lokaler_host("localhost"));
    }
}
