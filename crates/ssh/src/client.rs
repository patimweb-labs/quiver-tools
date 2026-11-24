// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der einzige Ort mit Netzverkehr: eine Sitzung über `russh` und `russh-sftp`.
//!
//! Die Sitzung entsteht nur über [`crate::anmeldung`]: dort wird entschieden,
//! ob mit Schlüssel oder (nur zur ersten Einrichtung) mit Passwort angemeldet
//! wird. Hier stehen die Handlungen: `exec` (auch mit `sudo`), Verzeichnis
//! lesen, hoch- und herunterladen.
//!
//! Kein Fehlertext enthält ein Geheimnis. Das sudo-Passwort geht ausschließlich
//! über stdin des entfernten Befehls.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use russh::client;
use russh::keys::PublicKeyOrCertificate;
use russh::ChannelMsg;
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::FileType;
use ssh_key::PublicKey;

use crate::hostschluessel::{self, Stand};
use crate::pfade::Pfade;
use crate::verbindung::Verbindung;

/// Eine geöffnete, angemeldete Sitzung.
pub struct Sitzung {
    handle: client::Handle<Pruefer>,
    /// Fingerabdruck, wenn der Host-Schlüssel bei dieser Verbindung zum ersten
    /// Mal gesehen und gemerkt wurde.
    pub neu_gemerkt: Option<String>,
}

/// Was ein Befehl ergeben hat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ausfuehrung {
    /// Der Befehl, wie er entfernt lief (ohne sudo-Passwort).
    pub befehl: String,
    pub stdout: String,
    pub stderr: String,
    pub status: u32,
    pub dauer_ms: u128,
}

/// Ein Eintrag eines entfernten Verzeichnisses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Eintrag {
    pub name: String,
    pub art: Art,
    pub groesse: u64,
    /// `rwxr-xr-x` (ohne Typzeichen).
    pub rechte: String,
}

/// Was für ein Ding ein Verzeichniseintrag ist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Art {
    Datei,
    Ordner,
    Verknuepfung,
    Anderes,
}

/// Wie angemeldet wird. Passwort nur für die erste Einrichtung.
#[derive(Debug, Clone, Copy)]
pub enum Anmeldung<'a> {
    Schluessel(&'a Path),
    Passwort(&'a str),
}

/// Der Prüfer des Host-Schlüssels: er merkt sich das Ergebnis der Prüfung,
/// weil `client::connect` den Handler bei einem Fehler mitnimmt.
struct Pruefer {
    host: String,
    port: u16,
    pfade: Pfade,
    stand: Arc<Mutex<Option<Stand>>>,
    angebot: Arc<Mutex<Option<PublicKey>>>,
}

impl client::Handler for Pruefer {
    type Error = anyhow::Error;

    async fn check_server_key(
        &mut self,
        schluessel: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let angebot = schluessel.public_key();
        let stand = hostschluessel::pruefen(&self.pfade, &self.host, self.port, &angebot)
            .unwrap_or(Stand::Unbekannt);
        *griff(&self.stand) = Some(stand.clone());
        *griff(&self.angebot) = Some(angebot);
        // Ein geänderter Host-Schlüssel ist ein harter Fehler (TOFU), nie eine
        // Rückfrage und nie ein Überschreiben.
        Ok(!matches!(stand, Stand::Geaendert { .. }))
    }
}

fn griff<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|gift| gift.into_inner())
}

/// Warum eine Sitzung nicht zustande kommt. Enthält nie ein Geheimnis.
#[derive(Debug, thiserror::Error)]
pub enum ClientFehler {
    /// Der Host-Schlüssel hat sich geändert; es wird nichts überschrieben.
    #[error(transparent)]
    HostSchluessel(#[from] HostSchluesselGeaendert),
    /// Aufbau gescheitert (Netz, Port, Zeitlimit …).
    #[error("Verbindung zu {adresse} fehlgeschlagen: {grund}")]
    Verbindung { adresse: String, grund: String },
    /// Der Server hat die Anmeldung abgelehnt.
    #[error("Anmeldung abgelehnt")]
    Abgelehnt,
    /// Der lokale Schlüssel ließ sich nicht laden.
    #[error("Der SSH-Schlüssel lässt sich nicht laden: {0}")]
    Schluessel(String),
}

/// Öffnet eine Sitzung mit genau der angegebenen Anmeldung.
///
/// Kein stiller Rückfall: wer sich mit dem Schlüssel anmelden soll, meldet sich
/// nur mit dem Schlüssel an, und ein Passwort wird nur benutzt, wenn es
/// ausdrücklich verlangt ist.
pub async fn verbinden(
    verbindung: &Verbindung,
    pfade: &Pfade,
    anmeldung: Anmeldung<'_>,
) -> Result<Sitzung, ClientFehler> {
    let stand = Arc::new(Mutex::new(None));
    let angebot = Arc::new(Mutex::new(None));
    let pruefer = Pruefer {
        host: verbindung.host.clone(),
        port: verbindung.port,
        pfade: pfade.clone(),
        stand: stand.clone(),
        angebot: angebot.clone(),
    };

    let config = Arc::new(client::Config::default());
    let mut handle =
        match client::connect(config, (verbindung.host.as_str(), verbindung.port), pruefer).await {
            Ok(handle) => handle,
            Err(fehler) => {
                if let Some(Stand::Geaendert { alte, datei, zeile }) = griff(&stand).clone() {
                    return Err(HostSchluesselGeaendert {
                        host: verbindung.adresse(),
                        alte,
                        neu: griff(&angebot)
                            .as_ref()
                            .map(crate::schluessel::fingerprint)
                            .unwrap_or_default(),
                        datei,
                        zeile,
                    }
                    .into());
                }
                return Err(ClientFehler::Verbindung {
                    adresse: verbindung.adresse(),
                    grund: fehler.to_string(),
                });
            }
        };

    let erfolgreich = match anmeldung {
        Anmeldung::Schluessel(datei) => {
            let schluessel = russh::keys::load_secret_key(datei, None)
                .map_err(|fehler| ClientFehler::Schluessel(fehler.to_string()))?;
            handle
                .authenticate_publickey(
                    verbindung.benutzer.clone(),
                    russh::keys::PrivateKeyWithHashAlg::new(Arc::new(schluessel), None),
                )
                .await
                .map_err(|fehler| ClientFehler::Verbindung {
                    adresse: verbindung.adresse(),
                    grund: fehler.to_string(),
                })?
                .success()
        }
        Anmeldung::Passwort(passwort) => handle
            .authenticate_password(verbindung.benutzer.clone(), passwort.to_string())
            .await
            .map_err(|fehler| ClientFehler::Verbindung {
                adresse: verbindung.adresse(),
                grund: fehler.to_string(),
            })?
            .success(),
    };

    if !erfolgreich {
        return Err(ClientFehler::Abgelehnt);
    }

    // Erste Verbindung zu diesem Host: merken (TOFU). Erst nach erfolgreicher
    // Anmeldung, damit kein fremder Host in der Liste landet.
    let mut neu_gemerkt = None;
    if matches!(griff(&stand).as_ref(), Some(Stand::Unbekannt)) {
        if let Some(schluessel) = griff(&angebot).clone() {
            if hostschluessel::merken(pfade, &verbindung.host, verbindung.port, &schluessel).is_ok()
            {
                neu_gemerkt = Some(crate::schluessel::fingerprint(&schluessel));
            }
        }
    }

    Ok(Sitzung {
        handle,
        neu_gemerkt,
    })
}

/// Der Host-Schlüssel hat sich geändert – hart abbrechen, mit beiden
/// Fingerabdrücken und der Stelle, die der User selbst löschen muss.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "Der Host-Schlüssel von {host} hat sich geändert.\n\
     Alt (gespeichert): {}\n\
     Neu (angeboten):   {}\n\
     In {datei}, Zeile {zeile}.\n\
     Das kann ein neu aufgesetzter Server sein – oder ein Angriff. quiver-ssh \
     überschreibt nichts. Wenn der neue Schlüssel stimmt, die alte Zeile in \
     {datei} löschen und den Aufruf wiederholen.",
    .alte.join(", "),
    .neu
)]
pub struct HostSchluesselGeaendert {
    pub host: String,
    pub alte: Vec<String>,
    pub neu: String,
    pub datei: PathBuf,
    pub zeile: usize,
}

impl Sitzung {
    /// Einen Befehl ausführen; mit `sudo` wird er über `sudo -S -p ''` gestartet
    /// und das Passwort über stdin geschrieben.
    pub async fn exec(
        &self,
        befehl: &str,
        sudo: Option<&str>,
        zeitlimit: Duration,
    ) -> Result<Ausfuehrung, anyhow::Error> {
        let (kommando, eingabe) = match sudo {
            Some(passwort) => (
                sudo_befehl(befehl),
                Some(format!("{passwort}\n").into_bytes()),
            ),
            None => (befehl.to_string(), None),
        };

        let start = Instant::now();
        let lauf = async {
            let mut channel = self.handle.channel_open_session().await?;
            channel.exec(true, kommando.clone()).await?;
            if let Some(eingabe) = eingabe {
                channel.data_bytes(eingabe).await?;
                channel.eof().await?;
            }

            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let mut status = None;
            while let Some(nachricht) = channel.wait().await {
                match nachricht {
                    ChannelMsg::Data { data } => stdout.extend_from_slice(&data),
                    ChannelMsg::ExtendedData { data, .. } => stderr.extend_from_slice(&data),
                    ChannelMsg::ExitStatus { exit_status } => status = Some(exit_status),
                    ChannelMsg::ExitSignal { .. } => status = Some(255),
                    _ => {}
                }
            }
            Ok::<_, anyhow::Error>((stdout, stderr, status.unwrap_or(0)))
        };

        let (stdout, stderr, status) =
            tokio::time::timeout(zeitlimit, lauf).await.map_err(|_| {
                anyhow::anyhow!(
                    "Der Befehl wurde nach {} s abgebrochen (Zeitlimit).",
                    zeitlimit.as_secs()
                )
            })??;

        Ok(Ausfuehrung {
            befehl: befehl.to_string(),
            stdout: String::from_utf8_lossy(&stdout).to_string(),
            stderr: String::from_utf8_lossy(&stderr).to_string(),
            status,
            dauer_ms: start.elapsed().as_millis(),
        })
    }

    /// Ein entferntes Verzeichnis lesen.
    pub async fn liste(&self, pfad: &str) -> Result<Vec<Eintrag>, anyhow::Error> {
        let sftp = self.sftp().await?;
        let mut eintraege = Vec::new();
        for eintrag in sftp.read_dir(pfad).await? {
            let meta = eintrag.metadata();
            let art = match eintrag.file_type() {
                FileType::Dir => Art::Ordner,
                FileType::File => Art::Datei,
                FileType::Symlink => Art::Verknuepfung,
                FileType::Other => Art::Anderes,
            };
            eintraege.push(Eintrag {
                name: eintrag.file_name(),
                art,
                groesse: meta.len(),
                rechte: rechte(&meta.permissions()),
            });
        }
        eintraege.sort_by(|a, b| a.name.cmp(&b.name));
        sftp.close().await.ok();
        Ok(eintraege)
    }

    /// Eine lokale Datei zum entfernten Pfad hochladen; gibt die Bytezahl zurück.
    pub async fn hochladen(&self, lokal: &Path, entfernt: &str) -> Result<u64, anyhow::Error> {
        let inhalt = std::fs::read(lokal).map_err(|fehler| {
            anyhow::anyhow!("{} lässt sich nicht lesen: {fehler}", lokal.display())
        })?;
        let sftp = self.sftp().await?;
        sftp.write(entfernt, &inhalt).await?;
        sftp.close().await.ok();
        Ok(inhalt.len() as u64)
    }

    /// Eine entfernte Datei als Bytes holen (die Ablage schreibt sie).
    pub async fn herunterladen_bytes(&self, entfernt: &str) -> Result<Vec<u8>, anyhow::Error> {
        let sftp = self.sftp().await?;
        let inhalt = sftp.read(entfernt).await?;
        sftp.close().await.ok();
        Ok(inhalt)
    }

    /// Eine entfernte Datei lokal ablegen; gibt die Bytezahl zurück.
    pub async fn herunterladen(&self, entfernt: &str, lokal: &Path) -> Result<u64, anyhow::Error> {
        let sftp = self.sftp().await?;
        let inhalt = sftp.read(entfernt).await?;
        sftp.close().await.ok();
        if let Some(eltern) = lokal.parent() {
            if !eltern.as_os_str().is_empty() {
                std::fs::create_dir_all(eltern).map_err(|fehler| {
                    anyhow::anyhow!("{} lässt sich nicht anlegen: {fehler}", eltern.display())
                })?;
            }
        }
        std::fs::write(lokal, &inhalt).map_err(|fehler| {
            anyhow::anyhow!("{} lässt sich nicht schreiben: {fehler}", lokal.display())
        })?;
        Ok(inhalt.len() as u64)
    }

    /// Eine SFTP-Sitzung über einen eigenen Kanal.
    async fn sftp(&self) -> Result<SftpSession, anyhow::Error> {
        let channel = self.handle.channel_open_session().await?;
        channel.request_subsystem(true, "sftp").await?;
        Ok(SftpSession::new(channel.into_stream()).await?)
    }

    /// Die Sitzung ordentlich beenden; Fehler sind egal.
    pub async fn beenden(self) {
        let _ = self
            .handle
            .disconnect(russh::Disconnect::ByApplication, "", "de")
            .await;
    }
}

/// Ein Befehl für `sudo`: `sudo -S -p '' -- sh -c '<befehl>'`.
///
/// `-S` liest das Passwort von stdin, `-p ''` unterdrückt die Aufforderung
/// (die sonst in der Antwort landen könnte), `--` beendet die Optionen.
pub fn sudo_befehl(befehl: &str) -> String {
    format!("sudo -S -p '' -- sh -c {}", quoten(befehl))
}

/// Ein Argument für die entfernte Shell quotieren (einfache Anführungszeichen).
pub fn quoten(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// `rwxr-xr-x` aus den Rechten.
fn rechte(rechte: &russh_sftp::protocol::FilePermissions) -> String {
    let mut text = String::with_capacity(9);
    for (lesen, schreiben, ausfuehren) in [
        (rechte.owner_read, rechte.owner_write, rechte.owner_exec),
        (rechte.group_read, rechte.group_write, rechte.group_exec),
        (rechte.other_read, rechte.other_write, rechte.other_exec),
    ] {
        text.push(if lesen { 'r' } else { '-' });
        text.push(if schreiben { 'w' } else { '-' });
        text.push(if ausfuehren { 'x' } else { '-' });
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sudo_befehl_quotiert_sauber() {
        assert_eq!(
            sudo_befehl("systemctl status nginx"),
            "sudo -S -p '' -- sh -c 'systemctl status nginx'"
        );
        assert_eq!(
            sudo_befehl("echo 'x' && id"),
            r"sudo -S -p '' -- sh -c 'echo '\''x'\'' && id'"
        );
        // Kein Passwort im Befehl: nur der Platz dafür ist fest.
        assert!(!sudo_befehl("id").contains("passwort"));
    }

    #[test]
    fn quoten_schuetzt_vor_der_entfernten_shell() {
        assert_eq!(quoten("a b"), "'a b'");
        assert_eq!(quoten("a'b"), r"'a'\''b'");
        assert_eq!(quoten("$(rm -rf /)"), "'$(rm -rf /)'");
    }
}
