// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die SMTP-Seite (SPEC §6, Aufgabe 04).
//!
//! [`Versand`] ist die Naht: die Werkzeuge kennen nur sie, die Tests setzen
//! eine Attrappe davor. Die echte Umsetzung [`LettreVersand`] baut die
//! Nachricht mit `lettre` und schickt sie über `lettre`s rustls-Transport –
//! **kein** OpenSSL. Dieselben Bytes gehen danach als Kopie in den Ordner
//! „Gesendet“, damit der Empfänger und das Postfach dasselbe sehen.

use std::str::FromStr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use lettre::message::{Mailbox, Message};
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::client::{Tls, TlsParameters};
use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};

use crate::domain::{Adresse, Konto, Sicherheit, VersandErgebnis, VersandNachricht, Zukunft};
use crate::fehler::MailFehler;

/// Wie lange ein SMTP-Aufruf höchstens dauert.
pub const ZEITLIMIT: Duration = Duration::from_secs(30);

/// Die eine Naht zum Versand.
pub trait Versand: Send + Sync {
    /// Eine Nachricht senden; die Antwort enthält die Bytes für die Ablage.
    fn senden(&self, nachricht: &VersandNachricht) -> Zukunft<'_, VersandErgebnis>;
    /// Verbinden, `EHLO` und anmelden – ohne zu senden.
    fn pruefen(&self) -> Zukunft<'_, ()>;
}

/// Der echte SMTP-Versand aus dem Zugang.
pub struct LettreVersand {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    absender: Adresse,
    ziel: String,
    passwort: String,
}

impl std::fmt::Debug for LettreVersand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LettreVersand")
            .field("absender", &self.absender)
            .field("ziel", &self.ziel)
            .finish_non_exhaustive()
    }
}

impl LettreVersand {
    /// Der Versand für genau diesen Zugang.
    pub fn neu(konto: &Konto) -> Result<Self, MailFehler> {
        // lettre baut seine rustls-Konfiguration über den Standard-Provider.
        // Das ist idempotent und darf mehrfach passieren.
        let _ = rustls::crypto::ring::default_provider().install_default();

        let smtp = &konto.smtp;
        let parameter = TlsParameters::builder(smtp.host.clone())
            .dangerous_accept_invalid_certs(!smtp.zertifikat_pruefen)
            .dangerous_accept_invalid_hostnames(!smtp.zertifikat_pruefen)
            .build()
            .map_err(|fehler| MailFehler::Konfiguration(format!("SMTP-TLS: {fehler}")))?;
        let tls = match smtp.sicherheit {
            Sicherheit::Tls => Tls::Wrapper(parameter),
            Sicherheit::Starttls => Tls::Required(parameter),
            Sicherheit::Keine => Tls::None,
        };

        let transport = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&smtp.host)
            .port(smtp.port)
            .credentials(Credentials::new(
                smtp.benutzer.clone(),
                smtp.passwort.clone(),
            ))
            .timeout(Some(ZEITLIMIT))
            .tls(tls)
            .build();

        Ok(LettreVersand {
            transport,
            absender: Adresse {
                name: konto.name.clone(),
                adresse: Some(konto.adresse.clone()),
            },
            ziel: format!("{}:{}", smtp.host, smtp.port),
            passwort: smtp.passwort.clone(),
        })
    }

    /// Ein Fehlertext ohne das Passwort (Sicherheitsnetz, SPEC §3).
    fn reinigen(&self, text: &str) -> String {
        if self.passwort.is_empty() {
            text.to_string()
        } else {
            text.replace(&self.passwort, "[geheim]")
        }
    }

    fn smtp_fehler(&self, fehler: lettre::transport::smtp::Error) -> MailFehler {
        MailFehler::Smtp(self.reinigen(&fehler.to_string()))
    }

    /// Die fertige Nachricht – auch für die Ablage im Ordner „Gesendet“.
    pub fn nachricht(&self, nachricht: &VersandNachricht) -> Result<Message, MailFehler> {
        nachricht_bauen(&self.absender, nachricht)
    }
}

impl Versand for LettreVersand {
    fn senden(&self, nachricht: &VersandNachricht) -> Zukunft<'_, VersandErgebnis> {
        let nachricht = nachricht.clone();
        Box::pin(async move {
            if nachricht.an.is_empty() {
                return Err(MailFehler::KeinEmpfaenger);
            }
            let nachricht = self.nachricht(&nachricht)?;
            let roh = nachricht.formatted();
            let kennung = nachricht
                .headers()
                .get_raw("Message-ID")
                .unwrap_or("")
                .trim()
                .to_string();

            match tokio::time::timeout(ZEITLIMIT, self.transport.send(nachricht)).await {
                Ok(Ok(_)) => Ok(VersandErgebnis {
                    message_id: kennung,
                    roh,
                }),
                Ok(Err(fehler)) => Err(self.smtp_fehler(fehler)),
                Err(_) => Err(MailFehler::Zeitlimit {
                    ziel: self.ziel.clone(),
                    sekunden: ZEITLIMIT.as_secs(),
                }),
            }
        })
    }

    fn pruefen(&self) -> Zukunft<'_, ()> {
        Box::pin(async move {
            match tokio::time::timeout(ZEITLIMIT, self.transport.test_connection()).await {
                Ok(Ok(true)) => Ok(()),
                Ok(Ok(false)) => Err(MailFehler::Anmeldung {
                    ziel: self.ziel.clone(),
                    ursache: "der Server hat die Verbindung nicht bestätigt".to_string(),
                }),
                Ok(Err(fehler)) => Err(self.smtp_fehler(fehler)),
                Err(_) => Err(MailFehler::Zeitlimit {
                    ziel: self.ziel.clone(),
                    sekunden: ZEITLIMIT.as_secs(),
                }),
            }
        })
    }
}

/// Baut die RFC-822-Nachricht, ohne sie zu senden.
///
/// `lettre` erledigt Adresskodierung, `Message-ID`, `Date` und die
/// Zeilenenden; die Tests prüfen das Ergebnis über [`lettre`]s Stub-Transport.
pub fn nachricht_bauen(
    absender: &Adresse,
    nachricht: &VersandNachricht,
) -> Result<Message, MailFehler> {
    if nachricht.an.is_empty() {
        return Err(MailFehler::KeinEmpfaenger);
    }
    let mut bauer = Message::builder()
        .from(mailbox(absender)?)
        .subject(nachricht.betreff.clone());

    for adresse in &nachricht.an {
        bauer = bauer.to(mailbox_aus_text(adresse)?);
    }
    for adresse in &nachricht.cc {
        bauer = bauer.cc(mailbox_aus_text(adresse)?);
    }
    for adresse in &nachricht.bcc {
        bauer = bauer.bcc(mailbox_aus_text(adresse)?);
    }
    if let Some(kennung) = nachricht
        .in_reply_to
        .as_deref()
        .map(str::trim)
        .filter(|kennung| !kennung.is_empty())
    {
        bauer = bauer.in_reply_to(kennung.to_string());
    }
    let referenzen: Vec<&str> = nachricht
        .references
        .iter()
        .map(|teil| teil.trim())
        .filter(|teil| !teil.is_empty())
        .collect();
    if !referenzen.is_empty() {
        bauer = bauer.references(referenzen.join(" "));
    }

    // Eine eigene Message-ID, damit die Kopie im Ordner „Gesendet“ und die
    // Antwort eines Empfängers auf dieselbe Kennung zeigen.
    bauer = bauer.message_id(Some(neue_kennung(absender)));

    bauer
        .body(nachricht.text.clone())
        .map_err(|fehler| MailFehler::Smtp(fehler.to_string()))
}

/// Eine [`Adresse`] als lettre-Mailbox.
fn mailbox(adresse: &Adresse) -> Result<Mailbox, MailFehler> {
    let mail = adresse
        .adresse
        .as_deref()
        .map(str::trim)
        .filter(|mail| !mail.is_empty())
        .ok_or(MailFehler::KeinEmpfaenger)?;
    let geparst = lettre::Address::from_str(mail)
        .map_err(|_| MailFehler::UngueltigeAdresse(mail.to_string()))?;
    Ok(Mailbox::new(
        adresse
            .name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string),
        geparst,
    ))
}

/// „Name \<mail\>“ oder nur die Adresse.
fn mailbox_aus_text(text: &str) -> Result<Mailbox, MailFehler> {
    let text = text.trim();
    let (name, mail) = match (text.find('<'), text.rfind('>')) {
        (Some(auf), Some(zu)) if zu > auf => (
            Some(text[..auf].trim().trim_matches('"').trim().to_string())
                .filter(|name| !name.is_empty()),
            text[auf + 1..zu].trim(),
        ),
        _ => (None, text),
    };
    let adresse = lettre::Address::from_str(mail)
        .map_err(|_| MailFehler::UngueltigeAdresse(text.to_string()))?;
    Ok(Mailbox::new(name, adresse))
}

/// Eine Message-ID aus Uhrzeit, Prozess und Domain – ohne Zufallsabhängigkeit.
fn neue_kennung(absender: &Adresse) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|dauer| dauer.as_nanos())
        .unwrap_or_default();
    let domain = absender
        .adresse
        .as_deref()
        .and_then(|adresse| adresse.rsplit_once('@'))
        .map(|(_, domain)| domain)
        .filter(|domain| !domain.is_empty())
        .unwrap_or("localhost");
    format!("<{nanos}.{}@{domain}>", std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{antwort_betreff, antwort_kette};

    fn absender() -> Adresse {
        Adresse {
            name: Some("Ich".to_string()),
            adresse: Some("ich@example.test".to_string()),
        }
    }

    fn nachricht() -> VersandNachricht {
        VersandNachricht {
            von: absender(),
            an: vec!["Ada <ada@example.test>".to_string()],
            cc: vec!["bob@example.test".to_string()],
            bcc: vec!["geheim@example.test".to_string()],
            betreff: "Bericht".to_string(),
            text: "Hallo Welt".to_string(),
            in_reply_to: None,
            references: Vec::new(),
        }
    }

    #[test]
    fn die_nachricht_traegt_empfaenger_und_kopfzeilen() {
        let nachricht = nachricht_bauen(&absender(), &nachricht()).unwrap();
        let roh = String::from_utf8(nachricht.formatted()).unwrap();
        assert!(roh.contains("From: Ich <ich@example.test>"), "{roh}");
        assert!(roh.contains("To: Ada <ada@example.test>"), "{roh}");
        assert!(roh.contains("Cc: bob@example.test"), "{roh}");
        // BCC gehört in den Umschlag, nie in die Kopfzeilen.
        assert!(!roh.contains("Bcc:"), "{roh}");
        assert!(!roh.contains("geheim@example.test"), "{roh}");
        assert!(roh.contains("Subject: Bericht"), "{roh}");
        assert!(roh.contains("Message-ID: <"), "{roh}");
        assert!(roh.contains("Hallo Welt"), "{roh}");

        // Der Umschlag trägt alle Empfänger, auch CC und BCC.
        let empfaenger: Vec<String> = nachricht
            .envelope()
            .to()
            .iter()
            .map(|adresse| adresse.to_string())
            .collect();
        assert_eq!(
            empfaenger,
            vec![
                "ada@example.test",
                "bob@example.test",
                "geheim@example.test"
            ]
        );
    }

    #[test]
    fn threading_landet_in_den_kopfzeilen() {
        let mut nachricht = nachricht();
        nachricht.in_reply_to = Some("<abc@example.test>".to_string());
        nachricht.references = vec![
            "<vorher@example.test>".to_string(),
            "<abc@example.test>".to_string(),
        ];
        let gebaut = nachricht_bauen(&absender(), &nachricht).unwrap();
        let roh = String::from_utf8(gebaut.formatted()).unwrap();
        assert!(roh.contains("In-Reply-To: <abc@example.test>"), "{roh}");
        assert!(
            roh.contains("References: <vorher@example.test> <abc@example.test>"),
            "{roh}"
        );
    }

    #[test]
    fn antwort_und_weiterleitung_bauen_ihre_kopfzeilen() {
        let original = crate::domain::Nachricht {
            von: vec![Adresse {
                name: Some("Ada".to_string()),
                adresse: Some("ada@example.test".to_string()),
            }],
            betreff: "Bericht".to_string(),
            message_id: Some("<abc@example.test>".to_string()),
            ..crate::domain::Nachricht::default()
        };
        let kette = antwort_kette(&original);
        let mut antwort = nachricht();
        antwort.betreff = antwort_betreff(&original.betreff);
        antwort.in_reply_to = original.message_id.clone();
        antwort.references = kette;
        let roh =
            String::from_utf8(nachricht_bauen(&absender(), &antwort).unwrap().formatted()).unwrap();
        assert!(roh.contains("Subject: Re: Bericht"), "{roh}");
        assert!(roh.contains("In-Reply-To: <abc@example.test>"), "{roh}");
    }

    #[test]
    fn eine_unbrauchbare_adresse_ist_ein_fehler() {
        let mut nachricht = nachricht();
        nachricht.an = vec!["keine adresse".to_string()];
        let fehler = nachricht_bauen(&absender(), &nachricht).unwrap_err();
        assert!(
            matches!(fehler, MailFehler::UngueltigeAdresse(_)),
            "{fehler}"
        );
    }

    #[test]
    fn ohne_empfaenger_kein_versand() {
        let mut nachricht = nachricht();
        nachricht.an.clear();
        assert!(matches!(
            nachricht_bauen(&absender(), &nachricht),
            Err(MailFehler::KeinEmpfaenger)
        ));
    }

    #[test]
    fn eine_kennung_enthaelt_die_domain_des_absenders() {
        let kennung = neue_kennung(&absender());
        assert!(kennung.starts_with('<'), "{kennung}");
        assert!(kennung.ends_with("@example.test>"), "{kennung}");
        assert_ne!(kennung, neue_kennung(&absender()));
    }

    #[test]
    fn eine_adresse_mit_namen_geht_auch_mit_anfuehrungszeichen() {
        let mailbox = mailbox_aus_text("\"Ada Lovelace\" <ada@example.test>").unwrap();
        assert_eq!(mailbox.name.as_deref(), Some("Ada Lovelace"));
        assert_eq!(mailbox.email.to_string(), "ada@example.test");
    }

    #[tokio::test]
    async fn der_stub_transport_sieht_kopfzeilen_threading_und_empfaenger() {
        use lettre::transport::stub::AsyncStubTransport;

        let stub = AsyncStubTransport::new_ok();
        let mut nachricht = nachricht();
        nachricht.in_reply_to = Some("<abc@example.test>".to_string());
        nachricht.references = vec!["<abc@example.test>".to_string()];

        let gebaut = nachricht_bauen(&absender(), &nachricht).unwrap();
        stub.send(gebaut).await.unwrap();

        let protokoll = stub.messages().await;
        assert_eq!(protokoll.len(), 1);
        let (envelope, roh) = &protokoll[0];
        assert!(roh.contains("Subject: Bericht"), "{roh}");
        assert!(roh.contains("From: Ich <ich@example.test>"), "{roh}");
        assert!(roh.contains("In-Reply-To: <abc@example.test>"), "{roh}");
        assert!(roh.contains("References: <abc@example.test>"), "{roh}");
        assert!(roh.contains("Hallo Welt"), "{roh}");

        // Der Umschlag trägt auch BCC; die Kopfzeilen nicht.
        let empfaenger: Vec<String> = envelope
            .to()
            .iter()
            .map(|adresse| adresse.to_string())
            .collect();
        assert_eq!(
            empfaenger,
            vec![
                "ada@example.test",
                "bob@example.test",
                "geheim@example.test"
            ]
        );
        assert!(!roh.contains("geheim@example.test"), "{roh}");
    }

    #[test]
    fn der_versand_bleibt_im_debug_ohne_passwort() {
        let konto = Konto {
            adresse: "ich@example.test".to_string(),
            name: None,
            imap: crate::domain::ImapKonfiguration {
                host: "imap.example.test".to_string(),
                port: 993,
                sicherheit: Sicherheit::Tls,
                benutzer: "ich@example.test".to_string(),
                passwort: "nur-im-test-4711".to_string(),
                zertifikat_pruefen: true,
            },
            smtp: crate::domain::SmtpKonfiguration {
                host: "smtp.example.test".to_string(),
                port: 587,
                sicherheit: Sicherheit::Starttls,
                benutzer: "ich@example.test".to_string(),
                passwort: "nur-im-test-4711".to_string(),
                zertifikat_pruefen: true,
            },
        };
        let versand = LettreVersand::neu(&konto).unwrap();
        let text = format!("{versand:?}");
        assert!(text.contains("smtp.example.test"), "{text}");
        assert!(!text.contains("nur-im-test-4711"), "{text}");
    }
}
