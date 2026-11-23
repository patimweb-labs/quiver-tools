// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die IMAP-Seite (SPEC §6, Aufgabe 04).
//!
//! [`Postfach`] ist die Naht, hinter der das Protokoll liegt: die Werkzeuge
//! kennen nur diese Trait, die Tests setzen eine Attrappe davor. Die einzige
//! echte Umsetzung ist [`ImapPostfach`] mit `async-imap`, `tokio-rustls` und
//! den Wurzelzertifikaten aus `webpki-roots` – **kein** OpenSSL.
//!
//! Jeder Aufruf öffnet eine eigene Verbindung, meldet sich an und schließt sie
//! wieder. Das kostet eine Anmeldung je Werkzeug, hält aber den Zustand aus dem
//! Programm: es gibt keine Sitzung, die altert oder die ein anderer Aufruf
//! gerade benutzt.

use std::fmt;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use async_imap::types::{Fetch, Name, NameAttribute, Uid};
use async_imap::{Client, Session};
use futures_util::TryStreamExt;
use mail_parser::{AddressList, MessageParser};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName as TlsName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;

use crate::domain::{
    adressen_text, Adresse, Anhang, ImapKonfiguration, Kopfzeilen, KopfzeilenSeite, LoeschErgebnis,
    Nachricht, PostfachInfo, Sicherheit, Suchkriterien, Zukunft, INBOX,
};
use crate::fehler::MailFehler;

/// Wie lange ein IMAP-Aufruf höchstens dauert.
pub const ZEITLIMIT: Duration = Duration::from_secs(30);

/// Die eine Naht zum Postfach. Alle Methoden sind lesend oder verändernd, aber
/// nie zustandsbehaftet: jede öffnet ihre eigene Verbindung.
pub trait Postfach: Send + Sync {
    /// Alle Postfächer (Ordner) des Servers.
    fn postfaecher(&self) -> Zukunft<'_, Vec<PostfachInfo>>;
    /// Die letzten `anzahl` Kopfzeilen, neueste zuerst.
    fn kopfzeilen(
        &self,
        postfach: &str,
        anzahl: usize,
        nur_ungelesen: bool,
    ) -> Zukunft<'_, KopfzeilenSeite>;
    /// Suche nach Absender, Betreff, Text und Zeitraum.
    fn suchen(
        &self,
        postfach: &str,
        kriterien: &Suchkriterien,
        anzahl: usize,
    ) -> Zukunft<'_, KopfzeilenSeite>;
    /// Eine Nachricht samt Text und Anhangsliste.
    fn nachricht(&self, postfach: &str, uid: u32) -> Zukunft<'_, Nachricht>;
    /// Flaggen setzen und entfernen (IMAP-Syntax, z. B. `\Seen`).
    fn flaggen(
        &self,
        postfach: &str,
        uid: u32,
        hinzu: &[String],
        weg: &[String],
    ) -> Zukunft<'_, ()>;
    /// Eine Nachricht in ein anderes Postfach verschieben.
    fn verschieben(&self, quelle: &str, uid: u32, ziel: &str) -> Zukunft<'_, ()>;
    /// Eine Nachricht löschen; `endgueltig`, wenn der Server UID EXPUNGE kann.
    fn loeschen(&self, postfach: &str, uid: u32) -> Zukunft<'_, LoeschErgebnis>;
    /// Eine fertige RFC-822-Nachricht ablegen (Gesendet, Entwürfe).
    fn anhaengen(&self, postfach: &str, roh: &[u8], flaggen: &[&str]) -> Zukunft<'_, ()>;
    /// Der Ordner für gesendete Nachrichten (`\Sent`), falls der Server einen nennt.
    fn gesendet(&self) -> Zukunft<'_, Option<String>>;
    /// Anmelden und `SELECT INBOX` – für `quiver-email testen`.
    fn pruefen(&self) -> Zukunft<'_, ()>;
}

/// Die echte IMAP-Verbindung aus dem Zugang.
#[derive(Clone)]
pub struct ImapPostfach {
    konfiguration: ImapKonfiguration,
}

impl fmt::Debug for ImapPostfach {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImapPostfach")
            .field("konfiguration", &self.konfiguration)
            .finish()
    }
}

impl ImapPostfach {
    /// Ein Postfach für genau diesen Zugang.
    pub fn neu(konfiguration: ImapKonfiguration) -> Self {
        Self { konfiguration }
    }

    /// `host:port` für Meldungen – nie mit Passwort.
    fn ziel(&self) -> String {
        format!("{}:{}", self.konfiguration.host, self.konfiguration.port)
    }

    /// Ein Text ohne das Passwort (Sicherheitsnetz, SPEC §3).
    fn reinigen(&self, text: &str) -> String {
        if self.konfiguration.passwort.is_empty() {
            text.to_string()
        } else {
            text.replace(&self.konfiguration.passwort, "[geheim]")
        }
    }

    fn netzfehler(&self, fehler: std::io::Error) -> MailFehler {
        MailFehler::NichtErreichbar {
            ziel: self.ziel(),
            ursache: self.reinigen(&fehler.to_string()),
        }
    }

    fn imap_fehler(&self, fehler: async_imap::error::Error) -> MailFehler {
        MailFehler::Imap(self.reinigen(&fehler.to_string()))
    }

    /// Eine Operation mit Zeitlimit; danach gibt es keine hängenden Sockets.
    async fn mit_limit<T>(
        &self,
        arbeit: impl std::future::Future<Output = Result<T, MailFehler>>,
    ) -> Result<T, MailFehler> {
        match tokio::time::timeout(ZEITLIMIT, arbeit).await {
            Ok(ergebnis) => ergebnis,
            Err(_) => Err(MailFehler::Zeitlimit {
                ziel: self.ziel(),
                sekunden: ZEITLIMIT.as_secs(),
            }),
        }
    }

    /// Die TLS-Konfiguration: Webpki-Wurzeln, oder gar keine Prüfung für eine
    /// lokale Brücke mit selbstsigniertem Zertifikat.
    fn tls_konfiguration(&self) -> Result<ClientConfig, MailFehler> {
        let anbieter = Arc::new(rustls::crypto::ring::default_provider());
        let bauer = ClientConfig::builder_with_provider(anbieter.clone())
            .with_safe_default_protocol_versions()
            .map_err(|fehler| MailFehler::Konfiguration(format!("TLS: {fehler}")))?;

        if self.konfiguration.zertifikat_pruefen {
            let mut wurzeln = RootCertStore::empty();
            wurzeln.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            Ok(bauer.with_root_certificates(wurzeln).with_no_client_auth())
        } else {
            Ok(bauer
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(AllesAkzeptieren(anbieter)))
                .with_no_client_auth())
        }
    }

    /// TCP plus TLS; der Name im Zertifikat ist der Host aus dem Zugang.
    async fn tls(&self, tcp: TcpStream) -> Result<TlsStream<TcpStream>, MailFehler> {
        let name = TlsName::try_from(self.konfiguration.host.clone()).map_err(|_| {
            MailFehler::Konfiguration(format!(
                "„{}“ ist kein gültiger Servername für TLS",
                self.konfiguration.host
            ))
        })?;
        let verbinder = TlsConnector::from(Arc::new(self.tls_konfiguration()?));
        verbinder
            .connect(name, tcp)
            .await
            .map_err(|fehler| MailFehler::NichtErreichbar {
                ziel: self.ziel(),
                ursache: self.reinigen(&fehler.to_string()),
            })
    }

    /// Die Verbindung, wie sie das Feld `imap_sicherheit` beschreibt.
    async fn verbinden(&self) -> Result<Client<Strom>, MailFehler> {
        let tcp = TcpStream::connect((self.konfiguration.host.as_str(), self.konfiguration.port))
            .await
            .map_err(|fehler| self.netzfehler(fehler))?;

        match self.konfiguration.sicherheit {
            Sicherheit::Tls => {
                let strom = self.tls(tcp).await?;
                Ok(Client::new(Strom::Tls(Box::new(strom))))
            }
            Sicherheit::Keine => Ok(Client::new(Strom::Klar(tcp))),
            Sicherheit::Starttls => {
                let mut client = Client::new(Strom::Klar(tcp));
                // Erst die Begrüßung, dann STARTTLS – so steht es in der
                // Dokumentation von async-imap.
                let begruessung = client
                    .read_response()
                    .await
                    .map_err(|fehler| self.netzfehler(fehler))?;
                if begruessung.is_none() {
                    return Err(MailFehler::Imap(
                        "Der Server hat sich nicht gemeldet.".into(),
                    ));
                }
                client
                    .run_command_and_check_ok("STARTTLS", None)
                    .await
                    .map_err(|fehler| self.imap_fehler(fehler))?;
                let tcp = match client.into_inner() {
                    Strom::Klar(tcp) => tcp,
                    Strom::Tls(_) => {
                        return Err(MailFehler::Imap(
                            "STARTTLS auf einer TLS-Verbindung.".into(),
                        ));
                    }
                };
                let strom = self.tls(tcp).await?;
                Ok(Client::new(Strom::Tls(Box::new(strom))))
            }
        }
    }

    /// Anmelden; der Fehler nennt den Server, nie das Passwort.
    async fn anmelden(&self) -> Result<Session<Strom>, MailFehler> {
        let client = self.verbinden().await?;
        client
            .login(&self.konfiguration.benutzer, &self.konfiguration.passwort)
            .await
            .map_err(|(fehler, _client)| MailFehler::Anmeldung {
                ziel: self.ziel(),
                ursache: self.reinigen(&fehler.to_string()),
            })
    }

    /// Anmelden und ein Postfach wählen.
    async fn waehlen(&self, postfach: &str) -> Result<Session<Strom>, MailFehler> {
        let mut session = self.anmelden().await?;
        session
            .select(postfach)
            .await
            .map_err(|fehler| self.imap_fehler(fehler))?;
        Ok(session)
    }

    /// Ordentlich abmelden; ein Fehler dabei ist keiner mehr wert.
    async fn beenden(&self, mut session: Session<Strom>) {
        if let Err(fehler) = session.logout().await {
            tracing::debug!(
                ziel = %self.ziel(),
                fehler = %self.reinigen(&fehler.to_string()),
                "Abmelden fehlgeschlagen"
            );
        }
    }

    /// Die letzte Suche plus Kopfzeilen – der gemeinsame Weg von `fetch` und
    /// `search`.
    async fn kopfzeilen_mit(
        &self,
        postfach: &str,
        kriterium: &str,
        anzahl: usize,
    ) -> Result<KopfzeilenSeite, MailFehler> {
        let mut session = self.waehlen(postfach).await?;

        let mut uids: Vec<Uid> = session
            .uid_search(kriterium)
            .await
            .map_err(|fehler| self.imap_fehler(fehler))?
            .into_iter()
            .collect();
        uids.sort_unstable();
        let gesamt = uids.len();

        let seite = KopfzeilenSeite {
            postfach: postfach.to_string(),
            kopfzeilen: Vec::new(),
            gesamt,
        };
        if uids.is_empty() {
            self.beenden(session).await;
            return Ok(seite);
        }

        // Die neuesten zuerst: hohe UIDs sind später angekommen.
        let auswahl: Vec<Uid> = uids.iter().rev().take(anzahl).copied().collect();
        let menge = auswahl
            .iter()
            .map(Uid::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let strom = session
            .uid_fetch(&menge, "(UID FLAGS BODY.PEEK[HEADER])")
            .await
            .map_err(|fehler| self.imap_fehler(fehler))?;
        let eintraege: Vec<Fetch> = strom
            .try_collect()
            .await
            .map_err(|fehler| self.imap_fehler(fehler))?;

        let mut kopfzeilen: Vec<Kopfzeilen> = eintraege
            .iter()
            .map(|eintrag| {
                let uid = eintrag.uid.unwrap_or_default();
                let flaggen = eintrag.flags().map(|flagge| flaggen_text(flagge)).collect();
                kopfzeile_aus_roh(uid, flaggen, eintrag.header())
            })
            .collect();
        kopfzeilen.sort_by_key(|kopfzeile| std::cmp::Reverse(kopfzeile.uid));

        self.beenden(session).await;
        Ok(KopfzeilenSeite {
            postfach: postfach.to_string(),
            kopfzeilen,
            gesamt,
        })
    }
}

impl Postfach for ImapPostfach {
    fn postfaecher(&self) -> Zukunft<'_, Vec<PostfachInfo>> {
        Box::pin(self.mit_limit(async {
            let mut session = self.anmelden().await?;
            let strom = session
                .list(Some(""), Some("*"))
                .await
                .map_err(|fehler| self.imap_fehler(fehler))?;
            let namen: Vec<Name> = strom
                .try_collect()
                .await
                .map_err(|fehler| self.imap_fehler(fehler))?;

            let mut postfaecher: Vec<PostfachInfo> = namen.iter().map(postfach_aus_name).collect();
            postfaecher.sort_by(|a, b| a.pfad.cmp(&b.pfad));

            self.beenden(session).await;
            Ok(postfaecher)
        }))
    }

    fn kopfzeilen(
        &self,
        postfach: &str,
        anzahl: usize,
        nur_ungelesen: bool,
    ) -> Zukunft<'_, KopfzeilenSeite> {
        let postfach = postfach.to_string();
        let kriterium = if nur_ungelesen { "UNSEEN" } else { "ALL" };
        Box::pin(
            self.mit_limit(async move { self.kopfzeilen_mit(&postfach, kriterium, anzahl).await }),
        )
    }

    fn suchen(
        &self,
        postfach: &str,
        kriterien: &Suchkriterien,
        anzahl: usize,
    ) -> Zukunft<'_, KopfzeilenSeite> {
        let postfach = postfach.to_string();
        let kriterium = suchbefehl(kriterien);
        Box::pin(
            self.mit_limit(async move { self.kopfzeilen_mit(&postfach, &kriterium, anzahl).await }),
        )
    }

    fn nachricht(&self, postfach: &str, uid: u32) -> Zukunft<'_, Nachricht> {
        let postfach = postfach.to_string();
        Box::pin(self.mit_limit(async move {
            let mut session = self.waehlen(&postfach).await?;
            let strom = session
                .uid_fetch(uid.to_string(), "(UID FLAGS BODY.PEEK[])")
                .await
                .map_err(|fehler| self.imap_fehler(fehler))?;
            let eintraege: Vec<Fetch> = strom
                .try_collect()
                .await
                .map_err(|fehler| self.imap_fehler(fehler))?;

            let ergebnis = match eintraege.first().and_then(|eintrag| eintrag.body()) {
                Some(roh) => nachricht_aus_roh(uid, roh),
                None => Err(MailFehler::NichtGefunden {
                    uid,
                    postfach: postfach.to_string(),
                }),
            };
            self.beenden(session).await;
            ergebnis
        }))
    }

    fn flaggen(
        &self,
        postfach: &str,
        uid: u32,
        hinzu: &[String],
        weg: &[String],
    ) -> Zukunft<'_, ()> {
        let postfach = postfach.to_string();
        let hinzu = hinzu.to_vec();
        let weg = weg.to_vec();
        Box::pin(self.mit_limit(async move {
            let mut session = self.waehlen(&postfach).await?;
            if !hinzu.is_empty() {
                let befehl = format!("+FLAGS ({})", hinzu.join(" "));
                let strom = session
                    .uid_store(uid.to_string(), befehl)
                    .await
                    .map_err(|fehler| self.imap_fehler(fehler))?;
                let _: Vec<Fetch> = strom
                    .try_collect()
                    .await
                    .map_err(|fehler| self.imap_fehler(fehler))?;
            }
            if !weg.is_empty() {
                let befehl = format!("-FLAGS ({})", weg.join(" "));
                let strom = session
                    .uid_store(uid.to_string(), befehl)
                    .await
                    .map_err(|fehler| self.imap_fehler(fehler))?;
                let _: Vec<Fetch> = strom
                    .try_collect()
                    .await
                    .map_err(|fehler| self.imap_fehler(fehler))?;
            }
            self.beenden(session).await;
            Ok(())
        }))
    }

    fn verschieben(&self, quelle: &str, uid: u32, ziel: &str) -> Zukunft<'_, ()> {
        let quelle = quelle.to_string();
        let ziel = ziel.to_string();
        Box::pin(self.mit_limit(async move {
            let mut session = self.waehlen(&quelle).await?;

            // MOVE ist eine Erweiterung. Ohne sie: kopieren, als gelöscht
            // markieren, gezielt entfernen.
            let kann_move = match session.capabilities().await {
                Ok(faehigkeiten) => faehigkeiten.has_str("MOVE"),
                Err(_) => false,
            };
            if kann_move {
                session
                    .uid_mv(uid.to_string(), &ziel)
                    .await
                    .map_err(|fehler| self.imap_fehler(fehler))?;
            } else {
                session
                    .uid_copy(uid.to_string(), &ziel)
                    .await
                    .map_err(|fehler| self.imap_fehler(fehler))?;
                let strom = session
                    .uid_store(uid.to_string(), "+FLAGS (\\Deleted)")
                    .await
                    .map_err(|fehler| self.imap_fehler(fehler))?;
                let _: Vec<Fetch> = strom
                    .try_collect()
                    .await
                    .map_err(|fehler| self.imap_fehler(fehler))?;
                let _ = session.uid_expunge(uid.to_string()).await;
            }

            self.beenden(session).await;
            Ok(())
        }))
    }

    fn loeschen(&self, postfach: &str, uid: u32) -> Zukunft<'_, LoeschErgebnis> {
        let postfach = postfach.to_string();
        Box::pin(self.mit_limit(async move {
            let mut session = self.waehlen(&postfach).await?;
            let strom = session
                .uid_store(uid.to_string(), "+FLAGS (\\Deleted)")
                .await
                .map_err(|fehler| self.imap_fehler(fehler))?;
            let _: Vec<Fetch> = strom
                .try_collect()
                .await
                .map_err(|fehler| self.imap_fehler(fehler))?;

            // Nur UID EXPUNGE ist auf diese Nachricht beschränkt (RFC 4315).
            // Ein blankes EXPUNGE würde alles löschen, was ein anderer Client
            // vorher markiert hat – dann bleibt die Flagge lieber stehen.
            let kann_uidplus = match session.capabilities().await {
                Ok(faehigkeiten) => faehigkeiten.has_str("UIDPLUS"),
                Err(_) => false,
            };
            let endgueltig = if kann_uidplus {
                match session.uid_expunge(uid.to_string()).await {
                    Ok(_) => true,
                    Err(fehler) => {
                        tracing::warn!(
                            fehler = %self.reinigen(&fehler.to_string()),
                            "UID EXPUNGE fehlgeschlagen – Nachricht bleibt markiert"
                        );
                        false
                    }
                }
            } else {
                false
            };

            self.beenden(session).await;
            Ok(LoeschErgebnis { endgueltig })
        }))
    }

    fn anhaengen(&self, postfach: &str, roh: &[u8], flaggen: &[&str]) -> Zukunft<'_, ()> {
        let postfach = postfach.to_string();
        let roh = roh.to_vec();
        let flaggen: Vec<String> = flaggen.iter().map(|flagge| (*flagge).to_string()).collect();
        Box::pin(self.mit_limit(async move {
            let mut session = self.anmelden().await?;
            let flags = if flaggen.is_empty() {
                None
            } else {
                Some(format!("({})", flaggen.join(" ")))
            };
            // APPEND hängt selbst CRLF an; ein zweites erzeugte eine leere Zeile
            // hinter der Nachricht.
            let inhalt = roh.strip_suffix(b"\r\n").unwrap_or(&roh);
            session
                // async-imap setzt den Pfad selbst in Anführungszeichen;
                // ein zweites Paar wäre ein ungültiger Befehl.
                .append(postfach.as_str(), flags.as_deref(), None, inhalt)
                .await
                .map_err(|fehler| self.imap_fehler(fehler))?;
            self.beenden(session).await;
            Ok(())
        }))
    }

    fn gesendet(&self) -> Zukunft<'_, Option<String>> {
        Box::pin(self.mit_limit(async {
            let postfaecher = self.postfaecher().await?;
            Ok(gesendet_aus(&postfaecher))
        }))
    }

    fn pruefen(&self) -> Zukunft<'_, ()> {
        Box::pin(self.mit_limit(async {
            let session = self.waehlen(INBOX).await?;
            self.beenden(session).await;
            Ok(())
        }))
    }
}

/// Der Ordner „Gesendet“: zuerst `\Sent` (RFC 6154), sonst ein bekannter Name.
pub fn gesendet_aus(postfaecher: &[PostfachInfo]) -> Option<String> {
    if let Some(postfach) = postfaecher.iter().find(|postfach| postfach.ist_gesendet()) {
        return Some(postfach.pfad.clone());
    }
    const KANDIDATEN: &[&str] = &[
        "sent",
        "sent items",
        "sent messages",
        "sent mail",
        "inbox.sent",
        "gesendet",
        "gesendete objekte",
        "gesendete elemente",
        "[gmail]/sent mail",
    ];
    for kandidat in KANDIDATEN {
        if let Some(postfach) = postfaecher
            .iter()
            .find(|postfach| postfach.pfad.eq_ignore_ascii_case(kandidat))
        {
            return Some(postfach.pfad.clone());
        }
    }
    None
}

fn postfach_aus_name(name: &Name) -> PostfachInfo {
    let pfad = name.name().to_string();
    let trenner = name.delimiter().unwrap_or("/");
    let tiefe = if trenner.is_empty() {
        0
    } else {
        pfad.matches(trenner).count()
    };
    let name_teil = if trenner.is_empty() {
        pfad.clone()
    } else {
        pfad.rsplit(trenner).next().unwrap_or(&pfad).to_string()
    };
    PostfachInfo {
        name: name_teil,
        waehlbar: !name
            .attributes()
            .iter()
            .any(|attribut| matches!(attribut, NameAttribute::NoSelect)),
        tiefe,
        attribute: name.attributes().iter().map(attribut_text).collect(),
        pfad,
    }
}

/// Ein IMAP-Attribut als Text, wie es in der Antwort steht.
fn attribut_text(attribut: &NameAttribute<'_>) -> String {
    let text = match attribut {
        NameAttribute::NoInferiors => "\\Noinferiors",
        NameAttribute::NoSelect => "\\Noselect",
        NameAttribute::Marked => "\\Marked",
        NameAttribute::Unmarked => "\\Unmarked",
        NameAttribute::All => "\\All",
        NameAttribute::Archive => "\\Archive",
        NameAttribute::Drafts => "\\Drafts",
        NameAttribute::Flagged => "\\Flagged",
        NameAttribute::Junk => "\\Junk",
        NameAttribute::Sent => "\\Sent",
        NameAttribute::Trash => "\\Trash",
        NameAttribute::Extension(name) => name.as_ref(),
        sonst => return format!("{sonst:?}"),
    };
    text.to_string()
}

/// Eine Flagge als Text (`\Seen` usw.).
fn flaggen_text(flagge: async_imap::types::Flag<'_>) -> String {
    use async_imap::types::Flag;
    match flagge {
        Flag::Seen => "\\Seen".to_string(),
        Flag::Answered => "\\Answered".to_string(),
        Flag::Flagged => "\\Flagged".to_string(),
        Flag::Deleted => "\\Deleted".to_string(),
        Flag::Draft => "\\Draft".to_string(),
        Flag::Recent => "\\Recent".to_string(),
        Flag::MayCreate => "\\*".to_string(),
        Flag::Custom(name) => name.to_string(),
    }
}

/// Die IMAP-Suche aus den Kriterien der Werkzeuge. Alle Werte werden in
/// Anführungszeichen gesetzt, damit ein Leerzeichen kein neues Kriterium wird.
pub fn suchbefehl(kriterien: &Suchkriterien) -> String {
    let mut teile: Vec<String> = Vec::new();
    if kriterien.ungelesen {
        teile.push("UNSEEN".to_string());
    }
    if let Some(wert) = suchwert(&kriterien.von) {
        teile.push(format!("FROM {wert}"));
    }
    if let Some(wert) = suchwert(&kriterien.betreff) {
        teile.push(format!("SUBJECT {wert}"));
    }
    if let Some(wert) = suchwert(&kriterien.text) {
        teile.push(format!("BODY {wert}"));
    }
    if let Some(datum) = suchdatum(&kriterien.seit) {
        teile.push(format!("SINCE {datum}"));
    }
    if let Some(datum) = suchdatum(&kriterien.vor) {
        teile.push(format!("BEFORE {datum}"));
    }
    if teile.is_empty() {
        "ALL".to_string()
    } else {
        teile.join(" ")
    }
}

fn suchwert(wert: &Option<String>) -> Option<String> {
    wert.as_deref()
        .map(str::trim)
        .filter(|wert| !wert.is_empty())
        .map(|wert| format!("\"{}\"", wert.replace('\\', "\\\\").replace('"', "\\\"")))
}

/// `YYYY-MM-DD` → `02-Sep-2026` (RFC 3501 verlangt dieses Format).
fn suchdatum(wert: &Option<String>) -> Option<String> {
    let wert = wert.as_deref()?.trim();
    let teile: Vec<&str> = wert.split('-').collect();
    if teile.len() != 3 {
        return None;
    }
    let jahr: u32 = teile[0].parse().ok()?;
    let monat: usize = teile[1].parse().ok()?;
    let tag: u32 = teile[2].parse().ok()?;
    const MONATE: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let monat = MONATE.get(monat.checked_sub(1)?)?;
    Some(format!("{tag:02}-{monat}-{jahr:04}"))
}

/// Kopfzeilen aus dem rohen Kopfteil einer Nachricht.
fn kopfzeile_aus_roh(uid: u32, flaggen: Vec<String>, roh: Option<&[u8]>) -> Kopfzeilen {
    let leer = Kopfzeilen {
        uid,
        flaggen: flaggen.clone(),
        ..Kopfzeilen::default()
    };
    let Some(roh) = roh else {
        return leer;
    };
    let Some(nachricht) = MessageParser::new().parse_headers(roh) else {
        return leer;
    };
    Kopfzeilen {
        uid,
        von: adressen_text(&adressen(nachricht.from())),
        an: adressen_text(&adressen(nachricht.to())),
        cc: adressen_text(&adressen(nachricht.cc())),
        betreff: nachricht.subject().unwrap_or("").to_string(),
        datum: nachricht.date().map(|datum| datum.to_rfc3339()),
        flaggen,
    }
}

/// Eine ganze Nachricht aus ihren rohen Bytes.
fn nachricht_aus_roh(uid: u32, roh: &[u8]) -> Result<Nachricht, MailFehler> {
    let geparst = MessageParser::new()
        .parse(roh)
        .ok_or_else(|| MailFehler::Unlesbar("kein RFC-822-Dokument".to_string()))?;

    // `text_bodies` nimmt den Textteil; fehlt er, wird der HTML-Teil in Text
    // umgewandelt (Aufgabe 04).
    let text = geparst
        .text_bodies()
        .next()
        .map(|text| text.trim_end().to_string())
        .unwrap_or_default();

    let anhaenge = geparst
        .attachments()
        .map(|teil| Anhang {
            name: teil
                .attachment_name()
                .map(str::to_string)
                .unwrap_or_else(|| "ohne Namen".to_string()),
            typ: teil
                .content_type()
                .map(|typ| match typ.subtype() {
                    Some(untertyp) => format!("{}/{}", typ.ctype(), untertyp),
                    None => typ.ctype().to_string(),
                })
                .unwrap_or_else(|| "unbekannt".to_string()),
            groesse: teil.decoded_len(),
        })
        .collect();

    Ok(Nachricht {
        uid,
        von: adressen(geparst.from()),
        antwort_an: adressen(geparst.reply_to()),
        an: adressen(geparst.to()),
        cc: adressen(geparst.cc()),
        bcc: adressen(geparst.bcc()),
        betreff: geparst.subject().unwrap_or("").to_string(),
        datum: geparst.date().map(|datum| datum.to_rfc3339()),
        message_id: geparst.message_id().map(mit_klammern),
        antworten_auf: textliste(geparst.in_reply_to()),
        referenzen: textliste(geparst.references()),
        text,
        anhaenge,
    })
}

/// `mail-parser` liefert die Message-ID ohne spitze Klammern; `References`
/// und `In-Reply-To` verlangen sie.
fn mit_klammern(kennung: &str) -> String {
    let kennung = kennung.trim();
    if kennung.starts_with('<') && kennung.ends_with('>') {
        kennung.to_string()
    } else {
        format!("<{kennung}>")
    }
}

fn adressen(liste: Option<AddressList<'_>>) -> Vec<Adresse> {
    liste
        .map(|liste| {
            liste
                .mailboxes()
                .map(|mailbox| Adresse {
                    name: mailbox.name().map(str::to_string),
                    adresse: mailbox.address().map(str::to_string),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn textliste(liste: Option<mail_parser::TextList<'_>>) -> Vec<String> {
    liste
        .map(|liste| liste.iter().map(mit_klammern).collect())
        .unwrap_or_default()
}

/// Nimmt jedes Zertifikat an – nur für `127.0.0.1`/`localhost` erreichbar
/// (siehe [`crate::domain::Konto::prueft_zertifikat`]).
#[derive(Debug)]
struct AllesAkzeptieren(Arc<CryptoProvider>);

impl ServerCertVerifier for AllesAkzeptieren {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &TlsName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

/// Der Strom unter async-imap: mal Klartext, mal TLS.
#[derive(Debug)]
enum Strom {
    Klar(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
}

impl AsyncRead for Strom {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Strom::Klar(strom) => Pin::new(strom).poll_read(cx, buf),
            Strom::Tls(strom) => Pin::new(strom).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Strom {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            Strom::Klar(strom) => Pin::new(strom).poll_write(cx, buf),
            Strom::Tls(strom) => Pin::new(strom).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Strom::Klar(strom) => Pin::new(strom).poll_flush(cx),
            Strom::Tls(strom) => Pin::new(strom).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Strom::Klar(strom) => Pin::new(strom).poll_shutdown(cx),
            Strom::Tls(strom) => Pin::new(strom).poll_shutdown(cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{mails, Sicherheit};

    fn kopf(roh: &str) -> Vec<u8> {
        roh.as_bytes().to_vec()
    }

    #[test]
    fn kopfzeilen_kommen_aus_dem_rohen_kopf() {
        let roh = kopf(concat!(
            "From: Ada <ada@example.test>\r\n",
            "To: Ich <ich@example.test>\r\n",
            "Cc: Bob <bob@example.test>\r\n",
            "Subject: =?UTF-8?B?R3LDvMOfZQ==?=\r\n",
            "Date: Wed, 2 Sep 2026 10:11:12 +0200\r\n",
            "\r\n"
        ));
        let kopfzeile = kopfzeile_aus_roh(7, vec!["\\Seen".to_string()], Some(&roh));
        assert_eq!(kopfzeile.uid, 7);
        assert_eq!(kopfzeile.von, "Ada <ada@example.test>");
        assert_eq!(kopfzeile.an, "Ich <ich@example.test>");
        assert_eq!(kopfzeile.cc, "Bob <bob@example.test>");
        assert_eq!(kopfzeile.betreff, "Grüße");
        assert_eq!(
            kopfzeile.datum.as_deref(),
            Some("2026-09-02T10:11:12+02:00")
        );
        assert_eq!(kopfzeile.flaggen, vec!["\\Seen"]);
    }

    #[test]
    fn ohne_kopf_bleibt_die_uid() {
        let kopfzeile = kopfzeile_aus_roh(3, Vec::new(), None);
        assert_eq!(kopfzeile.uid, 3);
        assert_eq!(kopfzeile.betreff, "");
        assert!(kopfzeile.datum.is_none());
    }

    #[test]
    fn eine_nachricht_hat_text_adressen_und_anhaenge() {
        let roh = concat!(
            "From: Ada <ada@example.test>\r\n",
            "Reply-To: Liste <liste@example.test>\r\n",
            "To: Ich <ich@example.test>\r\n",
            "Cc: Bob <bob@example.test>\r\n",
            "Subject: Bericht\r\n",
            "Date: Wed, 2 Sep 2026 10:11:12 +0200\r\n",
            "Message-ID: <abc@example.test>\r\n",
            "In-Reply-To: <vorher@example.test>\r\n",
            "References: <noch-vorher@example.test>\r\n",
            "MIME-Version: 1.0\r\n",
            "Content-Type: multipart/mixed; boundary=\"trenner\"\r\n",
            "\r\n",
            "--trenner\r\n",
            "Content-Type: text/plain; charset=utf-8\r\n",
            "\r\n",
            "Hallo Welt\r\n",
            "--trenner\r\n",
            "Content-Type: application/pdf; name=\"bericht.pdf\"\r\n",
            "Content-Disposition: attachment; filename=\"bericht.pdf\"\r\n",
            "Content-Transfer-Encoding: base64\r\n",
            "\r\n",
            "AAECAwQ=\r\n",
            "--trenner--\r\n"
        );
        let nachricht = nachricht_aus_roh(9, roh.as_bytes()).unwrap();
        assert_eq!(nachricht.uid, 9);
        assert_eq!(nachricht.betreff, "Bericht");
        assert_eq!(nachricht.von[0].anzeige(), "Ada <ada@example.test>");
        assert_eq!(mails(&nachricht.antwort_an), vec!["liste@example.test"]);
        assert_eq!(mails(&nachricht.an), vec!["ich@example.test"]);
        assert_eq!(nachricht.message_id.as_deref(), Some("<abc@example.test>"));
        assert_eq!(nachricht.referenzen, vec!["<noch-vorher@example.test>"]);
        assert!(nachricht.text.contains("Hallo Welt"), "{}", nachricht.text);
        assert_eq!(nachricht.anhaenge.len(), 1);
        assert_eq!(nachricht.anhaenge[0].name, "bericht.pdf");
        assert_eq!(nachricht.anhaenge[0].typ, "application/pdf");
        assert_eq!(nachricht.anhaenge[0].groesse, 5);
    }

    #[test]
    fn ohne_textteil_wird_html_zu_text() {
        let roh = concat!(
            "From: Ada <ada@example.test>\r\n",
            "Subject: HTML\r\n",
            "MIME-Version: 1.0\r\n",
            "Content-Type: text/html; charset=utf-8\r\n",
            "\r\n",
            "<p>Hallo <b>Welt</b>.</p><p>Zweite Zeile.</p>\r\n"
        );
        let nachricht = nachricht_aus_roh(1, roh.as_bytes()).unwrap();
        assert!(nachricht.text.contains("Hallo Welt."), "{}", nachricht.text);
        assert!(
            nachricht.text.contains("Zweite Zeile."),
            "{}",
            nachricht.text
        );
        assert!(!nachricht.text.contains('<'), "{}", nachricht.text);
    }

    #[test]
    fn eine_kennung_bekommt_spitze_klammern() {
        assert_eq!(mit_klammern("abc@example.test"), "<abc@example.test>");
        assert_eq!(mit_klammern("<abc@example.test>"), "<abc@example.test>");
    }

    #[test]
    fn lateinische_charsets_werden_gelesen() {
        let mut roh = concat!(
            "From: Ada <ada@example.test>\r\n",
            "Subject: Bericht\r\n",
            "MIME-Version: 1.0\r\n",
            "Content-Type: text/plain; charset=iso-8859-1\r\n",
            "Content-Transfer-Encoding: 8bit\r\n",
            "\r\n",
            "Gr"
        )
        .as_bytes()
        .to_vec();
        // 0xE4 ist „ä“, 0xDF ist „ß“ in ISO-8859-1.
        roh.extend_from_slice(&[0xE4, 0xDF]);
        roh.extend_from_slice(b"e\r\n");

        let nachricht = nachricht_aus_roh(1, &roh).unwrap();
        assert!(nachricht.text.contains("Gräße"), "{}", nachricht.text);
    }

    #[test]
    fn kaputte_nachricht_ist_ein_fehler() {
        let fehler = nachricht_aus_roh(1, b"").unwrap_err();
        assert!(matches!(fehler, MailFehler::Unlesbar(_)), "{fehler}");
    }

    #[test]
    fn der_suchbefehl_setzt_werte_in_anfuehrungszeichen() {
        let kriterien = Suchkriterien {
            von: Some("ada@example.test".to_string()),
            betreff: Some("zwei Worte".to_string()),
            text: Some("inhalt".to_string()),
            seit: Some("2026-01-02".to_string()),
            vor: Some("2026-02-03".to_string()),
            ungelesen: true,
        };
        assert_eq!(
            suchbefehl(&kriterien),
            "UNSEEN FROM \"ada@example.test\" SUBJECT \"zwei Worte\" BODY \"inhalt\" SINCE 02-Jan-2026 BEFORE 03-Feb-2026"
        );
    }

    #[test]
    fn eine_leere_suche_ist_all() {
        assert_eq!(suchbefehl(&Suchkriterien::default()), "ALL");
        assert!(Suchkriterien::default().ist_leer());
        assert!(!Suchkriterien {
            ungelesen: true,
            ..Suchkriterien::default()
        }
        .ist_leer());
    }

    #[test]
    fn anfuehrungszeichen_im_suchwert_werden_geschuetzt() {
        let kriterien = Suchkriterien {
            betreff: Some("sag \"hallo\"".to_string()),
            ..Suchkriterien::default()
        };
        assert_eq!(suchbefehl(&kriterien), "SUBJECT \"sag \\\"hallo\\\"\"");
    }

    #[test]
    fn ein_unbrauchbares_datum_faellt_weg() {
        let kriterien = Suchkriterien {
            seit: Some("02.01.2026".to_string()),
            ..Suchkriterien::default()
        };
        assert_eq!(suchbefehl(&kriterien), "ALL");
    }

    #[test]
    fn gesendet_kommt_aus_dem_attribut_oder_einem_namen() {
        let mit_attribut = vec![
            PostfachInfo::einfach("INBOX"),
            PostfachInfo {
                attribute: vec!["\\Sent".to_string()],
                ..PostfachInfo::einfach("[Gmail]/Gesendet")
            },
        ];
        assert_eq!(
            gesendet_aus(&mit_attribut).as_deref(),
            Some("[Gmail]/Gesendet")
        );

        let ohne_attribut = vec![
            PostfachInfo::einfach("INBOX"),
            PostfachInfo::einfach("Gesendet"),
        ];
        assert_eq!(gesendet_aus(&ohne_attribut).as_deref(), Some("Gesendet"));
        assert_eq!(gesendet_aus(&[PostfachInfo::einfach("INBOX")]), None);
    }

    #[test]
    fn die_konfiguration_bleibt_im_debug_ohne_passwort() {
        let postfach = ImapPostfach::neu(ImapKonfiguration {
            host: "imap.example.test".to_string(),
            port: 993,
            sicherheit: Sicherheit::Tls,
            benutzer: "ich@example.test".to_string(),
            passwort: "nur-im-test-4711".to_string(),
            zertifikat_pruefen: true,
        });
        let text = format!("{postfach:?}");
        assert!(text.contains("imap.example.test"), "{text}");
        assert!(!text.contains("nur-im-test-4711"), "{text}");
    }
}
