// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Attrappe der Protokollschicht (SPEC §7, Aufgabe 04).
//!
//! Kein Test spricht je mit einem echten Mailserver. [`Attrappe`] ist
//! gleichzeitig `Postfach` und `Versand`: sie gibt feste Daten zurück und
//! schreibt jeden Aufruf mit, damit die Tests Pfad, UID, Grenzen und
//! Empfänger prüfen können. Das Passwort ist erfunden und steht bewusst in
//! keiner Antwort.

#![allow(dead_code)]

use std::sync::{Arc, Mutex, MutexGuard};

use quiver_email::domain::{
    Adresse, Anhang, Kopfzeilen, KopfzeilenSeite, LoeschErgebnis, Nachricht, PostfachInfo,
    Suchkriterien, VersandErgebnis, VersandNachricht, Zukunft,
};
use quiver_email::fehler::MailFehler;
use quiver_email::{Email, Fabrik, Postfach, Profil, Versand, Zugang};
use quiver_tools_kern::Werkzeug;

/// Das Passwort der Attrappe. Kein echtes Geheimnis, aber es darf in keiner
/// Antwort auftauchen.
pub const PASSWORT: &str = "attrappe-passwort-4711";

/// Ein aufgezeichneter Flaggen-Aufruf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlaggenAufruf {
    pub postfach: String,
    pub uid: u32,
    pub hinzu: Vec<String>,
    pub weg: Vec<String>,
}

/// Ein aufgezeichneter APPEND (Gesendet, Entwürfe).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnhangAufruf {
    pub postfach: String,
    pub roh: Vec<u8>,
    pub flaggen: Vec<String>,
}

/// Ein aufgezeichneter Aufruf einer Protokolloperation.
#[derive(Debug)]
pub struct Aufzeichnung {
    pub postfaecher: Mutex<usize>,
    pub kopfzeilen: Mutex<Vec<(String, usize, bool)>>,
    pub suchen: Mutex<Vec<(String, Suchkriterien, usize)>>,
    pub gelesen: Mutex<Vec<(String, u32)>>,
    pub flaggen: Mutex<Vec<FlaggenAufruf>>,
    pub verschoben: Mutex<Vec<(String, u32, String)>>,
    pub geloescht: Mutex<Vec<(String, u32)>>,
    pub anhaenge: Mutex<Vec<AnhangAufruf>>,
    pub versandt: Mutex<Vec<VersandNachricht>>,
    pub geprueft: Mutex<usize>,
}

fn griff<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|gift| gift.into_inner())
}

impl Default for Aufzeichnung {
    fn default() -> Self {
        Self {
            postfaecher: Mutex::new(0),
            kopfzeilen: Mutex::new(Vec::new()),
            suchen: Mutex::new(Vec::new()),
            gelesen: Mutex::new(Vec::new()),
            flaggen: Mutex::new(Vec::new()),
            verschoben: Mutex::new(Vec::new()),
            geloescht: Mutex::new(Vec::new()),
            anhaenge: Mutex::new(Vec::new()),
            versandt: Mutex::new(Vec::new()),
            geprueft: Mutex::new(0),
        }
    }
}

/// Die Attrappe: feste Daten, mitgeschriebene Aufrufe.
pub struct Attrappe {
    pub aufzeichnung: Aufzeichnung,
    /// Postfächer, die `email_list_mailboxes` liefert.
    pub postfaecher: Vec<PostfachInfo>,
    /// Was `email_fetch` und `email_search` liefern.
    pub kopfzeilen: KopfzeilenSeite,
    /// Was `email_read` liefert.
    pub nachricht: Nachricht,
    /// Der Ordner „Gesendet“; `None` heißt: der Server nennt keinen.
    pub gesendet: Option<String>,
    /// `true`: löschen entfernt endgültig (UIDPLUS).
    pub endgueltig: bool,
    /// Ein Fehler, den `senden` meldet (für den Fall „gesendet, Kopie fehlt“
    /// bzw. „Versand scheitert“).
    pub versand_fehler: Option<String>,
}

impl Default for Attrappe {
    fn default() -> Self {
        Self {
            aufzeichnung: Aufzeichnung::default(),
            postfaecher: vec![
                PostfachInfo::einfach("INBOX"),
                PostfachInfo {
                    attribute: vec!["\\Sent".to_string()],
                    ..PostfachInfo::einfach("[Gmail]/Gesendet")
                },
            ],
            kopfzeilen: KopfzeilenSeite {
                postfach: "INBOX".to_string(),
                kopfzeilen: vec![
                    Kopfzeilen {
                        uid: 42,
                        von: "Ada <ada@example.test>".to_string(),
                        an: "Ich <ich@example.test>".to_string(),
                        cc: String::new(),
                        betreff: "Bericht".to_string(),
                        datum: Some("2026-09-02T10:11:12+02:00".to_string()),
                        flaggen: vec!["\\Seen".to_string()],
                    },
                    Kopfzeilen {
                        uid: 41,
                        von: "Bob <bob@example.test>".to_string(),
                        an: "Ich <ich@example.test>".to_string(),
                        cc: String::new(),
                        betreff: "Frage".to_string(),
                        datum: Some("2026-09-01T08:07:06+02:00".to_string()),
                        flaggen: Vec::new(),
                    },
                ],
                gesamt: 12,
            },
            nachricht: Nachricht {
                uid: 42,
                von: vec![Adresse {
                    name: Some("Ada".to_string()),
                    adresse: Some("ada@example.test".to_string()),
                }],
                antwort_an: Vec::new(),
                an: vec![Adresse {
                    name: Some("Ich".to_string()),
                    adresse: Some("ich@example.test".to_string()),
                }],
                cc: vec![Adresse {
                    name: None,
                    adresse: Some("bob@example.test".to_string()),
                }],
                bcc: Vec::new(),
                betreff: "Bericht".to_string(),
                datum: Some("2026-09-02T10:11:12+02:00".to_string()),
                message_id: Some("<abc@example.test>".to_string()),
                antworten_auf: vec!["<vorher@example.test>".to_string()],
                referenzen: vec!["<vorher@example.test>".to_string()],
                text: "Der Agent bezahlt Tokens für jede Antwort.".to_string(),
                anhaenge: vec![Anhang {
                    name: "bericht.pdf".to_string(),
                    typ: "application/pdf".to_string(),
                    groesse: 2048,
                }],
            },
            gesendet: Some("Gesendet".to_string()),
            endgueltig: true,
            versand_fehler: None,
        }
    }
}

impl Attrappe {
    pub fn neu() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn zukunft<'a, T: Send + 'a>(&'a self, wert: Result<T, MailFehler>) -> Zukunft<'a, T> {
        Box::pin(async move { wert })
    }
}

impl Postfach for Attrappe {
    fn postfaecher(&self) -> Zukunft<'_, Vec<PostfachInfo>> {
        *griff(&self.aufzeichnung.postfaecher) += 1;
        self.zukunft(Ok(self.postfaecher.clone()))
    }

    fn kopfzeilen(
        &self,
        postfach: &str,
        anzahl: usize,
        nur_ungelesen: bool,
    ) -> Zukunft<'_, KopfzeilenSeite> {
        griff(&self.aufzeichnung.kopfzeilen).push((postfach.to_string(), anzahl, nur_ungelesen));
        let mut seite = self.kopfzeilen.clone();
        seite.postfach = postfach.to_string();
        self.zukunft(Ok(seite))
    }

    fn suchen(
        &self,
        postfach: &str,
        kriterien: &Suchkriterien,
        anzahl: usize,
    ) -> Zukunft<'_, KopfzeilenSeite> {
        griff(&self.aufzeichnung.suchen).push((postfach.to_string(), kriterien.clone(), anzahl));
        let mut seite = self.kopfzeilen.clone();
        seite.postfach = postfach.to_string();
        self.zukunft(Ok(seite))
    }

    fn nachricht(&self, postfach: &str, uid: u32) -> Zukunft<'_, Nachricht> {
        griff(&self.aufzeichnung.gelesen).push((postfach.to_string(), uid));
        if uid == self.nachricht.uid {
            self.zukunft(Ok(self.nachricht.clone()))
        } else {
            self.zukunft(Err(MailFehler::NichtGefunden {
                uid,
                postfach: postfach.to_string(),
            }))
        }
    }

    fn flaggen(
        &self,
        postfach: &str,
        uid: u32,
        hinzu: &[String],
        weg: &[String],
    ) -> Zukunft<'_, ()> {
        griff(&self.aufzeichnung.flaggen).push(FlaggenAufruf {
            postfach: postfach.to_string(),
            uid,
            hinzu: hinzu.to_vec(),
            weg: weg.to_vec(),
        });
        self.zukunft(Ok(()))
    }

    fn verschieben(&self, quelle: &str, uid: u32, ziel: &str) -> Zukunft<'_, ()> {
        griff(&self.aufzeichnung.verschoben).push((quelle.to_string(), uid, ziel.to_string()));
        self.zukunft(Ok(()))
    }

    fn loeschen(&self, postfach: &str, uid: u32) -> Zukunft<'_, LoeschErgebnis> {
        griff(&self.aufzeichnung.geloescht).push((postfach.to_string(), uid));
        self.zukunft(Ok(LoeschErgebnis {
            endgueltig: self.endgueltig,
        }))
    }

    fn anhaengen(&self, postfach: &str, roh: &[u8], flaggen: &[&str]) -> Zukunft<'_, ()> {
        griff(&self.aufzeichnung.anhaenge).push(AnhangAufruf {
            postfach: postfach.to_string(),
            roh: roh.to_vec(),
            flaggen: flaggen.iter().map(|flagge| (*flagge).to_string()).collect(),
        });
        self.zukunft(Ok(()))
    }

    fn gesendet(&self) -> Zukunft<'_, Option<String>> {
        self.zukunft(Ok(self.gesendet.clone()))
    }

    fn pruefen(&self) -> Zukunft<'_, ()> {
        *griff(&self.aufzeichnung.geprueft) += 1;
        self.zukunft(Ok(()))
    }
}

impl Versand for Attrappe {
    fn senden(&self, nachricht: &VersandNachricht) -> Zukunft<'_, VersandErgebnis> {
        griff(&self.aufzeichnung.versandt).push(nachricht.clone());
        if let Some(fehler) = &self.versand_fehler {
            return self.zukunft(Err(MailFehler::Smtp(fehler.clone())));
        }
        self.zukunft(Ok(VersandErgebnis {
            message_id: "<gesendet@example.test>".to_string(),
            roh: b"From: ich@example.test\r\nSubject: Test\r\n\r\nHallo\r\n".to_vec(),
        }))
    }

    fn pruefen(&self) -> Zukunft<'_, ()> {
        *griff(&self.aufzeichnung.geprueft) += 1;
        self.zukunft(Ok(()))
    }
}

/// Die Fabrik, die Werkzeuge und Attrappe verbindet.
pub struct AttrappenFabrik {
    pub postfach: Arc<Attrappe>,
    pub versand: Arc<Attrappe>,
}

impl Fabrik for AttrappenFabrik {
    fn postfach(&self, _profil: &Profil) -> Result<Arc<dyn Postfach>, MailFehler> {
        Ok(self.postfach.clone())
    }

    fn versand(&self, _profil: &Profil) -> Result<Arc<dyn Versand>, MailFehler> {
        Ok(self.versand.clone())
    }
}

/// Ein vollständiger Zugang als Paare für `Zugang::mit_umgebung`.
pub fn umgebung(adresse: &str) -> Vec<(&'static str, String)> {
    vec![
        ("EMAIL_ADRESSE", adresse.to_string()),
        ("EMAIL_NAME", "Ich".to_string()),
        ("EMAIL_IMAP_HOST", "imap.example.test".to_string()),
        ("EMAIL_IMAP_PORT", "993".to_string()),
        ("EMAIL_IMAP_SICHERHEIT", "tls".to_string()),
        ("EMAIL_SMTP_HOST", "smtp.example.test".to_string()),
        ("EMAIL_SMTP_PORT", "587".to_string()),
        ("EMAIL_SMTP_SICHERHEIT", "starttls".to_string()),
        ("EMAIL_BENUTZER", adresse.to_string()),
        ("EMAIL_PASSWORT", PASSWORT.to_string()),
        ("EMAIL_ZERTIFIKAT_PRUEFEN", "an".to_string()),
        // Der Schreibschutz ist in den Tests an; die Sperre prüft ein eigener Test.
        ("EMAIL_SCHREIBEN", "an".to_string()),
    ]
}

/// Der Zugang für die Tests, ohne den Prozess zu verändern.
pub fn zugang(adresse: &str) -> Zugang {
    let werte = umgebung(adresse);
    Zugang::mit_umgebung(&Email, |name| {
        werte
            .iter()
            .find(|(schluessel, _)| *schluessel == name)
            .map(|(_, wert)| wert.clone())
    })
}

/// Ein Zugang ohne jede Umgebung.
pub fn ohne_zugang() -> Zugang {
    Zugang::mit_umgebung(&Email, |_| None)
}

/// Ein Zugang ohne Schreibschalter – so wie nach dem Update.
pub fn zugang_ohne_schreiben(adresse: &str) -> Zugang {
    let werte = umgebung(adresse);
    Zugang::mit_umgebung(&Email, |name| {
        if name == "EMAIL_SCHREIBEN" {
            return None;
        }
        werte
            .iter()
            .find(|(schluessel, _)| *schluessel == name)
            .map(|(_, wert)| wert.clone())
    })
}

/// Beide Attrappen zusammen; sie teilen sich die Aufzeichnung nicht.
pub fn fabrik() -> (Arc<Attrappe>, Arc<Attrappe>, AttrappenFabrik) {
    let postfach = Attrappe::neu();
    let versand = Attrappe::neu();
    let fabrik = AttrappenFabrik {
        postfach: postfach.clone(),
        versand: versand.clone(),
    };
    (postfach, versand, fabrik)
}

/// Damit der Compiler die Werkzeug-Beschreibung nicht als unbenutzt ansieht.
pub fn werkzeug_name() -> &'static str {
    Email.name()
}

/// Ein Blick in eine Aufzeichnung, ohne den Gift-Reflex von `unwrap`.
pub fn drin<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    griff(m)
}
