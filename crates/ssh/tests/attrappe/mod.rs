// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der Test-SSH-Server (SPEC §7): ein echter SSH-Server auf `127.0.0.1`, mit
//! Passwort- und Schlüssel-Anmeldung, `exec`, einem simulierten `sudo -S` und
//! SFTP über einen temp-Ordner. Kein Test spricht je mit einem echten Host.
//!
//! Die Attrappe spielt die Gegenseite so, wie `sshd` sie spielt:
//! - `authorized_keys` ist eine Liste im Speicher; nur dort eingetragene
//!   Schlüssel werden angenommen;
//! - `sudo -S -p '' -- sh -c '<befehl>'` liest das Passwort aus stdin;
//! - ein `printf '%s\n' '<zeile>' >> <datei>` trägt einen Schlüssel ein.

#![allow(dead_code)]

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use russh::server::{Auth, ChannelOpenHandle, Msg, Server as _, Session, VerifiedPublicKey};
use russh::{Channel, ChannelId};
use russh_sftp::protocol::{
    Attrs, Data, File, FileAttributes, Handle, Name, OpenFlags, SftpHandle, Status, StatusCode,
    Version,
};
use russh_sftp::server::Handler as SftpHandler;
use serde_json::{json, Value};
use ssh_key::{Algorithm, PrivateKey, PublicKey};

/// Benutzer, den die Attrappe kennt.
pub const BENUTZER: &str = "test";
/// Das eine Passwort (kein echtes Geheimnis): Anmeldung und sudo.
pub const PASSWORT: &str = "anmelde-4711";
/// Ein eigenes sudo-Passwort aus früheren Versionen (kein echtes Geheimnis).
pub const SUDO_PASSWORT: &str = "sudo-4712";

fn griff<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|gift| gift.into_inner())
}

/// Was die Attrappe über die Verbindungen weiß und mitschreibt.
pub struct Zustand {
    /// Simuliertes Home des Benutzers (`$HOME`).
    home: PathBuf,
    /// Wurzel für SFTP.
    dateien: PathBuf,
    /// Zeilen, die in `authorized_keys` stehen.
    autorisiert: Mutex<Vec<String>>,
    /// Wenn `false`, lehnt der Server jede Schlüssel-Anmeldung ab
    /// (`PubkeyAuthentication no`).
    pubkey_erlaubt: AtomicBool,
    passwort_versuche: AtomicUsize,
    pubkey_versuche: AtomicUsize,
    /// Was `exec` gesehen hat (für Tests).
    befehle: Mutex<Vec<String>>,
}

impl Zustand {
    fn neu(home: PathBuf, dateien: PathBuf) -> Zustand {
        Zustand {
            home,
            dateien,
            autorisiert: Mutex::new(Vec::new()),
            pubkey_erlaubt: AtomicBool::new(true),
            passwort_versuche: AtomicUsize::new(0),
            pubkey_versuche: AtomicUsize::new(0),
            befehle: Mutex::new(Vec::new()),
        }
    }

    /// Das simulierte Home auf der „entfernten“ Maschine.
    pub fn home(&self) -> &Path {
        &self.home
    }

    /// Die Wurzel des SFTP-Bereichs.
    pub fn dateien(&self) -> &Path {
        &self.dateien
    }

    /// Server lehnt Schlüssel ab (wie `PubkeyAuthentication no`).
    pub fn sperre_schluessel(&self) {
        self.pubkey_erlaubt.store(false, Ordering::SeqCst);
    }

    /// Server „vergisst“ alle eingetragenen Schlüssel (Neuaufbau).
    pub fn vergiss_schluessel(&self) {
        griff(&self.autorisiert).clear();
    }

    pub fn passwort_versuche(&self) -> usize {
        self.passwort_versuche.load(Ordering::SeqCst)
    }

    pub fn pubkey_versuche(&self) -> usize {
        self.pubkey_versuche.load(Ordering::SeqCst)
    }

    /// Die eingetragenen `authorized_keys`-Zeilen (ohne Kommentar verglichen).
    pub fn autorisierte_zeilen(&self) -> Vec<String> {
        griff(&self.autorisiert).clone()
    }

    /// Die Befehle, die `exec` gesehen hat.
    pub fn befehle(&self) -> Vec<String> {
        griff(&self.befehle).clone()
    }

    fn kennt_schluessel(&self, schluessel: &PublicKey) -> bool {
        griff(&self.autorisiert)
            .iter()
            .filter_map(|zeile| PublicKey::from_openssh(zeile).ok())
            // Nur das Schlüsselmaterial zählt, nicht der Kommentar.
            .any(|bekannt| bekannt.key_data() == schluessel.key_data())
    }
}

/// Eine laufende Attrappe.
pub struct Attrappe {
    /// Port auf `127.0.0.1`.
    pub port: u16,
    pub zustand: Arc<Zustand>,
    temp: Arc<tempfile::TempDir>,
    ende: Option<tokio::task::JoinHandle<()>>,
    abschalten: russh::server::RunningServerHandle,
}

impl Attrappe {
    /// Startet einen Server auf einem freien Port.
    pub async fn starte() -> Attrappe {
        Attrappe::bauen(0, None).await
    }

    /// Beendet den Server ordentlich.
    pub async fn stoppen(self) {
        let Attrappe {
            abschalten, ende, ..
        } = self;
        abschalten.shutdown("ende".to_string());
        if let Some(ende) = ende {
            let _ = ende.await;
        }
    }

    async fn bauen(port: u16, temp: Option<Arc<tempfile::TempDir>>) -> Attrappe {
        let temp = temp.unwrap_or_else(|| Arc::new(tempfile::tempdir().unwrap()));
        let home = temp.path().join("home");
        let dateien = temp.path().join("dateien");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&dateien).unwrap();
        let zustand = Arc::new(Zustand::neu(home, dateien));
        Attrappe::bauen_mit(port, zustand, temp).await
    }

    async fn bauen_mit(port: u16, zustand: Arc<Zustand>, temp: Arc<tempfile::TempDir>) -> Attrappe {
        let config = Arc::new(russh::server::Config {
            auth_rejection_time: Duration::from_millis(10),
            auth_rejection_time_initial: Some(Duration::from_millis(0)),
            inactivity_timeout: Some(Duration::from_secs(30)),
            keys: vec![PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap()],
            ..Default::default()
        });

        let listener =
            tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port))
                .await
                .expect("Attrappe bindet nicht");
        let port = listener.local_addr().unwrap().port();

        // Der Server lebt im Task; der Abschalter kommt per Kanal heraus.
        let (sender, empfaenger) = tokio::sync::oneshot::channel();
        let zustand_fuer_task = zustand.clone();
        let ende = tokio::spawn(async move {
            let mut server = Ssh {
                zustand: zustand_fuer_task,
            };
            let laufend = server.run_on_socket(config, &listener);
            let _ = sender.send(laufend.handle());
            let _ = laufend.await;
        });
        let abschalten = empfaenger.await.expect("Attrappe startet nicht");

        Attrappe {
            port,
            zustand,
            temp,
            ende: Some(ende),
            abschalten,
        }
    }
}

/// Ein Eintrag für `QUIVER_ZUGAENGE`, wie quiver ihn übergeben würde.
///
/// `passwort` landet im Feld „Passwort“ (Anmeldung und sudo), `sudo_alt` im
/// Feld `sudo_passwort` aus früheren Versionen.
pub fn zugang(name: &str, port: u16, passwort: Option<&str>, sudo_alt: Option<&str>) -> Value {
    let mut werte = serde_json::Map::new();
    werte.insert("host".to_string(), json!("127.0.0.1"));
    werte.insert("port".to_string(), json!(port.to_string()));
    werte.insert("benutzer".to_string(), json!(BENUTZER));
    if let Some(passwort) = passwort {
        werte.insert("anmelde_passwort".to_string(), json!(passwort));
    }
    if let Some(passwort) = sudo_alt {
        werte.insert("sudo_passwort".to_string(), json!(passwort));
    }
    // Die Tests prüfen die Werkzeuge, nicht den Schalter: Schreiben ist an.
    werte.insert("schreiben".to_string(), json!("an"));
    json!({ "name": name, "werte": werte })
}

/// Ein Eintrag ohne Schreibschalter – so wie nach dem Update.
pub fn zugang_ohne_schreiben(name: &str, port: u16) -> Value {
    let mut eintrag = zugang(name, port, None, None);
    if let Some(werte) = eintrag
        .get_mut("werte")
        .and_then(|werte| werte.as_object_mut())
    {
        werte.remove("schreiben");
    }
    eintrag
}

/// Ein Eintrag mit Austauschordner (für Hoch- und Herunterladen).
pub fn zugang_mit_ordner(name: &str, port: u16, ordner: &str) -> Value {
    let mut eintrag = zugang(name, port, None, None);
    if let Some(werte) = eintrag
        .get_mut("werte")
        .and_then(|werte| werte.as_object_mut())
    {
        werte.insert("austauschordner".to_string(), json!(ordner));
    }
    eintrag
}

// ------------------------------------------------------------ SSH-Server

#[derive(Clone)]
struct Ssh {
    zustand: Arc<Zustand>,
}

impl russh::server::Server for Ssh {
    type Handler = Sitzung;

    fn new_client(&mut self, _: Option<SocketAddr>) -> Sitzung {
        Sitzung {
            zustand: self.zustand.clone(),
            kanal: Arc::new(Mutex::new(HashMap::new())),
            wartend: HashMap::new(),
            eingabe: HashMap::new(),
        }
    }
}

struct Sitzung {
    zustand: Arc<Zustand>,
    /// Offene Kanäle, damit `subsystem_request` den SFTP-Kanal übernehmen kann.
    kanal: Arc<Mutex<HashMap<ChannelId, Channel<Msg>>>>,
    /// `sudo`-Befehle, die noch auf stdin warten.
    wartend: HashMap<ChannelId, String>,
    eingabe: HashMap<ChannelId, Vec<u8>>,
}

impl russh::server::Handler for Sitzung {
    type Error = anyhow::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        self.zustand
            .passwort_versuche
            .fetch_add(1, Ordering::SeqCst);
        if user == BENUTZER && password == PASSWORT {
            Ok(Auth::Accept)
        } else {
            Ok(Auth::reject())
        }
    }

    async fn auth_publickey(
        &mut self,
        user: &str,
        key: VerifiedPublicKey<'_>,
    ) -> Result<Auth, Self::Error> {
        self.zustand.pubkey_versuche.fetch_add(1, Ordering::SeqCst);
        let erlaubt = self.zustand.pubkey_erlaubt.load(Ordering::SeqCst)
            && user == BENUTZER
            && self.zustand.kennt_schluessel(key.public_key);
        Ok(if erlaubt {
            Auth::Accept
        } else {
            Auth::reject()
        })
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        griff(&self.kanal).insert(channel.id(), channel);
        reply.accept().await;
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let befehl = String::from_utf8_lossy(data).to_string();
        griff(&self.zustand.befehle).push(befehl.clone());
        session.channel_success(channel)?;

        if befehl.starts_with("sudo ") {
            // Das Passwort kommt erst nach dem Start über stdin.
            self.wartend.insert(channel, befehl);
        } else {
            let ausgang = ausfuehren(&self.zustand, &befehl, "");
            antworten(session, channel, &ausgang)?;
        }
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.eingabe
            .entry(channel)
            .or_default()
            .extend_from_slice(data);
        Ok(())
    }

    async fn channel_eof(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some(befehl) = self.wartend.remove(&channel) {
            let eingabe = self.eingabe.remove(&channel).unwrap_or_default();
            let ausgang = ausfuehren(&self.zustand, &befehl, &String::from_utf8_lossy(&eingabe));
            antworten(session, channel, &ausgang)?;
        }
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        channel: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if name != "sftp" {
            session.channel_failure(channel)?;
            return Ok(());
        }
        let kanal = griff(&self.kanal).remove(&channel);
        match kanal {
            Some(kanal) => {
                session.channel_success(channel)?;
                let sftp = Sftp::neu(self.zustand.dateien.clone());
                russh_sftp::server::run(kanal.into_stream(), sftp).await;
            }
            None => session.channel_failure(channel)?,
        }
        Ok(())
    }
}

fn antworten(
    session: &mut Session,
    channel: ChannelId,
    ausgang: &Ausgang,
) -> Result<(), anyhow::Error> {
    if !ausgang.stdout.is_empty() {
        session.data(channel, ausgang.stdout.clone().into_bytes())?;
    }
    if !ausgang.stderr.is_empty() {
        session.extended_data(channel, 1, ausgang.stderr.clone().into_bytes())?;
    }
    session.exit_status_request(channel, ausgang.status)?;
    session.eof(channel)?;
    session.close(channel)?;
    Ok(())
}

/// Was ein simulierter Befehl ausgegeben hat.
struct Ausgang {
    stdout: String,
    stderr: String,
    status: u32,
}

impl Ausgang {
    fn ok(stdout: impl Into<String>) -> Ausgang {
        Ausgang {
            stdout: stdout.into(),
            stderr: String::new(),
            status: 0,
        }
    }

    fn fehler(stderr: impl Into<String>, status: u32) -> Ausgang {
        Ausgang {
            stdout: String::new(),
            stderr: stderr.into(),
            status,
        }
    }

    fn nur_status(status: u32) -> Ausgang {
        Ausgang {
            stdout: String::new(),
            stderr: String::new(),
            status,
        }
    }
}

/// Ein sehr kleiner Ersatz für `sh -c`: nur, was die Tests brauchen.
fn ausfuehren(zustand: &Zustand, befehl: &str, stdin: &str) -> Ausgang {
    let befehl = befehl.trim();

    // sudo: `sudo -S -p '' -- sh -c '<befehl>'`
    if let Some(rest) = befehl.strip_prefix("sudo -S -p '' -- sh -c ") {
        let Some((inner, _)) = argument(rest) else {
            return Ausgang::fehler("attrappe: sudo-Befehl nicht lesbar\n", 2);
        };
        let passwort = stdin.lines().next().unwrap_or("");
        // Die Attrappe nimmt beide Testpasswörter an: das eine Feld „Passwort“
        // und ein eigenes sudo-Passwort aus früheren Versionen.
        if passwort != SUDO_PASSWORT && passwort != PASSWORT {
            return Ausgang::fehler("Sorry, try again.\n", 1);
        }
        return ausfuehren(zustand, &inner, "");
    }

    // Schritt 1 der Schlüssel-Einrichtung: ~/.ssh vorbereiten, Pfad ausgeben.
    if befehl.starts_with("umask 077 && mkdir -p ") {
        let ordner = zustand.home().join(".ssh");
        std::fs::create_dir_all(&ordner).unwrap();
        let datei = ordner.join("authorized_keys");
        if !datei.exists() {
            std::fs::write(&datei, "").unwrap();
        }
        return Ausgang::ok(datei.display().to_string());
    }

    // Schritt 2: vorhandene authorized_keys lesen.
    if let Some(rest) = befehl.strip_prefix("cat ") {
        let pfad = entquoten(rest);
        return match std::fs::read_to_string(&pfad) {
            Ok(inhalt) => Ausgang::ok(inhalt),
            Err(fehler) => Ausgang::fehler(format!("cat: {fehler}\n"), 1),
        };
    }

    // Schritt 3: eine Zeile anhängen (`printf '%s\n' '<zeile>' >> '<pfad>'`).
    if let Some(rest) = befehl.strip_prefix("printf '%s\\n' ") {
        let Some((zeile, rest)) = argument(rest) else {
            return Ausgang::fehler("attrappe: printf-Argument nicht lesbar\n", 2);
        };
        let Some(rest) = rest.trim_start().strip_prefix(">> ") else {
            return Ausgang::fehler("attrappe: printf ohne >>\n", 2);
        };
        let pfad = entquoten(rest);
        let mut inhalt = std::fs::read_to_string(&pfad).unwrap_or_default();
        inhalt.push_str(&zeile);
        inhalt.push('\n');
        std::fs::write(&pfad, inhalt).unwrap();
        griff(&zustand.autorisiert).push(zeile);
        return Ausgang::ok("");
    }

    match befehl {
        "id -un" => Ausgang::ok("root\n"),
        "whoami" => Ausgang::ok("test\n"),
        "false" => Ausgang::fehler("", 1),
        "" => Ausgang::ok(""),
        sonst => {
            if let Some(text) = sonst.strip_prefix("echo ") {
                return Ausgang::ok(format!("{text}\n"));
            }
            if let Some(zahl) = sonst.strip_prefix("exit ") {
                let status = zahl.trim().parse().unwrap_or(2);
                return Ausgang::nur_status(status);
            }
            Ausgang::fehler(format!("attrappe: unbekannter Befehl: {sonst}\n"), 127)
        }
    }
}

/// Ein Argument in einfachen Anführungszeichen (mit `'\''` als Escape).
fn argument(text: &str) -> Option<(String, String)> {
    let mut rest = text.strip_prefix('\'')?;
    let mut wert = String::new();
    loop {
        let stelle = rest.find('\'')?;
        wert.push_str(&rest[..stelle]);
        let nach = &rest[stelle + 1..];
        if let Some(weiter) = nach.strip_prefix("\\''") {
            wert.push('\'');
            rest = weiter;
        } else {
            return Some((wert, nach.to_string()));
        }
    }
}

/// Ein einfaches Argument entquoten (`'a b'` → `a b`).
fn entquoten(text: &str) -> String {
    let text = text.trim();
    match argument(text) {
        Some((wert, _)) => wert,
        None => text.to_string(),
    }
}

// ----------------------------------------------------------- SFTP-Server

struct Sftp {
    wurzel: PathBuf,
    dateien: HashMap<String, std::fs::File>,
    verzeichnisse: HashMap<String, Vec<File>>,
    nummer: u32,
}

impl Sftp {
    fn neu(wurzel: PathBuf) -> Sftp {
        Sftp {
            wurzel,
            dateien: HashMap::new(),
            verzeichnisse: HashMap::new(),
            nummer: 0,
        }
    }

    fn ziel(&self, pfad: &str) -> Result<PathBuf, StatusCode> {
        let relativ = pfad.trim_start_matches('/');
        if relativ.split('/').any(|teil| teil == "..") {
            return Err(StatusCode::PermissionDenied);
        }
        Ok(self.wurzel.join(relativ))
    }

    fn handle(&mut self, art: char) -> String {
        self.nummer += 1;
        format!("{art}{}", self.nummer)
    }
}

/// Der Name, unter dem ein Handle in den Tabellen steht.
fn handle_name(handle: &SftpHandle) -> String {
    String::from_utf8_lossy(handle.as_bytes()).to_string()
}

fn ok(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: "Ok".to_string(),
        language_tag: "en-US".to_string(),
    }
}

fn attrs(pfad: &Path) -> FileAttributes {
    let Ok(meta) = std::fs::metadata(pfad) else {
        return FileAttributes::empty();
    };
    let mut attrs = FileAttributes::empty();
    attrs.size = Some(meta.len());
    attrs.permissions = Some(if meta.is_dir() { 0o040755 } else { 0o100644 });
    attrs
}

impl SftpHandler for Sftp {
    type Error = StatusCode;

    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }

    async fn init(
        &mut self,
        _version: u32,
        _extensions: HashMap<String, String>,
    ) -> Result<Version, Self::Error> {
        Ok(Version::new())
    }

    async fn realpath(&mut self, id: u32, path: String) -> Result<Name, Self::Error> {
        Ok(Name {
            id,
            files: vec![File::dummy(if path.is_empty() {
                ".".to_string()
            } else {
                path
            })],
        })
    }

    async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, Self::Error> {
        let pfad = self.ziel(&path)?;
        let eintraege = std::fs::read_dir(&pfad).map_err(|_| StatusCode::NoSuchFile)?;
        let mut dateien = Vec::new();
        for eintrag in eintraege.flatten() {
            let name = eintrag.file_name().to_string_lossy().to_string();
            dateien.push(File::new(name, attrs(&eintrag.path())));
        }
        let handle = self.handle('d');
        self.verzeichnisse.insert(handle.clone(), dateien);
        Ok(Handle {
            id,
            handle: handle.into(),
        })
    }

    async fn readdir(&mut self, id: u32, handle: SftpHandle) -> Result<Name, Self::Error> {
        let dateien = self
            .verzeichnisse
            .get_mut(&handle_name(&handle))
            .ok_or(StatusCode::Failure)?;
        if dateien.is_empty() {
            return Err(StatusCode::Eof);
        }
        let alle = std::mem::take(dateien);
        Ok(Name { id, files: alle })
    }

    async fn open(
        &mut self,
        id: u32,
        filename: String,
        pflags: OpenFlags,
        _attrs: FileAttributes,
    ) -> Result<Handle, Self::Error> {
        let pfad = self.ziel(&filename)?;
        let optionen: std::fs::OpenOptions = pflags.into();
        let mut optionen = optionen;
        // SFTP-CREATE heißt: Datei anlegen, wenn sie fehlt.
        if pflags.contains(OpenFlags::CREATE) {
            optionen.create(true);
        }
        let datei = optionen.open(&pfad).map_err(|_| StatusCode::NoSuchFile)?;
        let handle = self.handle('f');
        self.dateien.insert(handle.clone(), datei);
        Ok(Handle {
            id,
            handle: handle.into(),
        })
    }

    async fn close(&mut self, id: u32, handle: SftpHandle) -> Result<Status, Self::Error> {
        let name = handle_name(&handle);
        self.dateien.remove(&name);
        self.verzeichnisse.remove(&name);
        Ok(ok(id))
    }

    async fn read(
        &mut self,
        id: u32,
        handle: SftpHandle,
        offset: u64,
        len: u32,
    ) -> Result<Data, Self::Error> {
        use std::io::{Read, Seek, SeekFrom};
        let datei = self
            .dateien
            .get_mut(&handle_name(&handle))
            .ok_or(StatusCode::Failure)?;
        datei
            .seek(SeekFrom::Start(offset))
            .map_err(|_| StatusCode::Failure)?;
        let mut puffer = vec![0u8; len as usize];
        let gelesen = datei.read(&mut puffer).map_err(|_| StatusCode::Failure)?;
        puffer.truncate(gelesen);
        if puffer.is_empty() {
            return Err(StatusCode::Eof);
        }
        Ok(Data { id, data: puffer })
    }

    async fn write(
        &mut self,
        id: u32,
        handle: SftpHandle,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<Status, Self::Error> {
        use std::io::{Seek, SeekFrom, Write};
        let datei = self
            .dateien
            .get_mut(&handle_name(&handle))
            .ok_or(StatusCode::Failure)?;
        datei
            .seek(SeekFrom::Start(offset))
            .map_err(|_| StatusCode::Failure)?;
        datei.write_all(&data).map_err(|_| StatusCode::Failure)?;
        Ok(ok(id))
    }

    async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        let pfad = self.ziel(&path)?;
        Ok(Attrs {
            id,
            attrs: attrs(&pfad),
        })
    }

    async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        self.stat(id, path).await
    }

    async fn mkdir(
        &mut self,
        id: u32,
        path: String,
        _attrs: FileAttributes,
    ) -> Result<Status, Self::Error> {
        let pfad = self.ziel(&path)?;
        std::fs::create_dir_all(&pfad).map_err(|_| StatusCode::Failure)?;
        Ok(ok(id))
    }
}
