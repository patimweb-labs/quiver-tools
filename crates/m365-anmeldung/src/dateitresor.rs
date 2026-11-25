// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Geheimnisse in einer eigenen, verschlüsselten Datei – ohne Schlüsselbund
//! des Betriebssystems.
//!
//! Warum nicht der Schlüsselbund: macOS fragt nach jedem neuen Build erneut
//! nach („Immer erlauben“), und unter Linux gibt es ihn je nach Distribution
//! gar nicht. Die Datei verhält sich überall gleich.
//!
//! Aufbau im Ordner (0700):
//! - `tresor.schluessel` – 32 zufällige Bytes (0600)
//! - `tresor.json` – `{"version":1,"nonce":…,"daten":…}`, die Daten sind die
//!   Schlüssel/Wert-Liste als JSON, verschlüsselt mit XChaCha20-Poly1305 (0600)
//!
//! Was das schützt – und was nicht: Wer den Benutzer-Account hat, kann beide
//! Dateien lesen; dagegen hilft nur ein Passwort beim Start. Die
//! Verschlüsselung verhindert, dass beim versehentlichen Lesen der Datei
//! (`cat`, `grep`, ein Agent, eine Sicherung nur von `tresor.json`) Klartext
//! herauskommt.
//!
//! Mehrere Prozesse dürfen gleichzeitig schreiben: jede Änderung liest die
//! Datei unter einer Dateisperre neu, ändert einen Wert und ersetzt die Datei
//! atomar (schreiben, dann umbenennen).

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use rand::RngCore;

/// Ein Tresor in einem Ordner.
pub struct DateiTresor {
    ordner: PathBuf,
}

impl std::fmt::Debug for DateiTresor {
    /// Nie Werte, nur der Ordner.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DateiTresor")
            .field("ordner", &self.ordner)
            .finish_non_exhaustive()
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Huelle {
    version: u32,
    nonce: String,
    daten: String,
}

impl DateiTresor {
    /// Ein Tresor im Ordner `ordner` (wird bei Bedarf angelegt).
    pub fn im_ordner(ordner: impl Into<PathBuf>) -> Self {
        Self {
            ordner: ordner.into(),
        }
    }

    /// Der Ordner des Tresors.
    pub fn ordner(&self) -> &Path {
        &self.ordner
    }

    pub fn lesen(&self, schluessel: &str) -> anyhow::Result<Option<String>> {
        let _sperre = self.sperren()?;
        Ok(self.laden()?.get(schluessel).cloned())
    }

    pub fn schreiben(&self, schluessel: &str, wert: &str) -> anyhow::Result<()> {
        let _sperre = self.sperren()?;
        let mut werte = self.laden()?;
        werte.insert(schluessel.to_string(), wert.to_string());
        self.speichern(&werte)
    }

    pub fn loeschen(&self, schluessel: &str) -> anyhow::Result<()> {
        let _sperre = self.sperren()?;
        let mut werte = self.laden()?;
        if werte.remove(schluessel).is_some() {
            self.speichern(&werte)?;
        }
        Ok(())
    }

    // ------------------------------------------------------------ innen

    fn ordner_anlegen(&self) -> anyhow::Result<()> {
        fs::create_dir_all(&self.ordner)?;
        rechte(&self.ordner, 0o700);
        Ok(())
    }

    /// Eine Sperre über alle Prozesse (`tresor.sperre`), solange der Wert lebt.
    fn sperren(&self) -> anyhow::Result<File> {
        self.ordner_anlegen()?;
        let datei = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.ordner.join("tresor.sperre"))?;
        datei.lock()?;
        Ok(datei)
    }

    fn schluessel(&self) -> anyhow::Result<[u8; 32]> {
        let pfad = self.ordner.join("tresor.schluessel");
        match fs::read(&pfad) {
            Ok(bytes) => bytes
                .try_into()
                .map_err(|_| anyhow::anyhow!("{} ist beschädigt", pfad.display())),
            Err(fehler) if fehler.kind() == std::io::ErrorKind::NotFound => {
                // Ein Tresor **ohne** Schlüssel wird nie stillschweigend mit
                // einem neuen Schlüssel überschrieben: die Werte wären sonst
                // endgültig verloren, ohne dass es jemand merkt (Härtung
                // Paket 4, wie mcp-quiver). Beide Dateien bleiben unberührt.
                if self.ordner.join("tresor.json").exists() {
                    return Err(anyhow::anyhow!(
                        "Der Schlüssel des Tresors fehlt ({}). Die Tresor-Datei bleibt unberührt \
                         – bitte im quiver-Fenster unter Tools den Eintrag erneut anmelden oder \
                         die Datei tresor.schluessel aus einer Sicherung zurückspielen.",
                        pfad.display()
                    ));
                }
                let mut neu = [0u8; 32];
                rand::rng().fill_bytes(&mut neu);
                schreiben_atomar(&pfad, &neu)?;
                Ok(neu)
            }
            Err(fehler) => Err(fehler.into()),
        }
    }

    fn laden(&self) -> anyhow::Result<BTreeMap<String, String>> {
        let pfad = self.ordner.join("tresor.json");
        let text = match fs::read_to_string(&pfad) {
            Ok(text) => text,
            Err(fehler) if fehler.kind() == std::io::ErrorKind::NotFound => {
                return Ok(BTreeMap::new())
            }
            Err(fehler) => return Err(fehler.into()),
        };
        let huelle: Huelle = serde_json::from_str(&text)
            .map_err(|_| anyhow::anyhow!("{} ist beschädigt", pfad.display()))?;
        let nonce = B64
            .decode(&huelle.nonce)
            .ok()
            .filter(|nonce| nonce.len() == 24)
            .ok_or_else(|| anyhow::anyhow!("{} ist beschädigt", pfad.display()))?;
        let daten = B64
            .decode(&huelle.daten)
            .map_err(|_| anyhow::anyhow!("{} ist beschädigt", pfad.display()))?;
        let schluessel = self.schluessel()?;
        let klar = XChaCha20Poly1305::new(Key::from_slice(&schluessel))
            .decrypt(XNonce::from_slice(&nonce), daten.as_ref())
            .map_err(|_| {
                anyhow::anyhow!(
                    "{} lässt sich nicht entschlüsseln (passt tresor.schluessel?)",
                    pfad.display()
                )
            })?;
        serde_json::from_slice(&klar)
            .map_err(|_| anyhow::anyhow!("{} ist beschädigt", pfad.display()))
    }

    fn speichern(&self, werte: &BTreeMap<String, String>) -> anyhow::Result<()> {
        let schluessel = self.schluessel()?;
        let mut nonce = [0u8; 24];
        rand::rng().fill_bytes(&mut nonce);
        let klar = serde_json::to_vec(werte)?;
        let daten = XChaCha20Poly1305::new(Key::from_slice(&schluessel))
            .encrypt(XNonce::from_slice(&nonce), klar.as_ref())
            .map_err(|_| anyhow::anyhow!("Verschlüsseln fehlgeschlagen"))?;
        let huelle = Huelle {
            version: 1,
            nonce: B64.encode(nonce),
            daten: B64.encode(daten),
        };
        schreiben_atomar(
            &self.ordner.join("tresor.json"),
            serde_json::to_string(&huelle)?.as_bytes(),
        )
    }
}

/// Erst in eine temporäre Datei (0600), dann umbenennen – nie halb geschrieben.
fn schreiben_atomar(pfad: &Path, inhalt: &[u8]) -> anyhow::Result<()> {
    let mut zwischen = pfad.as_os_str().to_owned();
    zwischen.push(format!(".{}.neu", std::process::id()));
    let zwischen = PathBuf::from(zwischen);
    {
        let mut optionen = OpenOptions::new();
        optionen.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            optionen.mode(0o600);
        }
        let mut datei = optionen.open(&zwischen)?;
        datei.write_all(inhalt)?;
        datei.sync_all()?;
    }
    fs::rename(&zwischen, pfad)?;
    rechte(pfad, 0o600);
    Ok(())
}

#[cfg(unix)]
fn rechte(pfad: &Path, modus: u32) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(pfad, fs::Permissions::from_mode(modus));
}

/// 0600/0700 gibt es nur auf Unix. Auf Windows bleibt es bei der ACL, die der
/// Ordner vom Benutzerprofil erbt: kein Welt-Recht, aber auch kein ausdrücklich
/// gesetztes Recht. Das ist die Grenze des Betriebssystems, nicht dieses
/// Werkzeugs – dokumentiert in `docs/architektur.md` (Plattformen).
#[cfg(not(unix))]
fn rechte(_pfad: &Path, _modus: u32) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rundlauf_und_kein_klartext_auf_der_platte() {
        let ordner = tempfile::tempdir().unwrap();
        let tresor = DateiTresor::im_ordner(ordner.path().join("t"));
        assert_eq!(tresor.lesen("a").unwrap(), None);
        tresor.schreiben("a", "streng-geheim-4711").unwrap();
        tresor.schreiben("b", "zwei").unwrap();
        assert_eq!(
            tresor.lesen("a").unwrap().as_deref(),
            Some("streng-geheim-4711")
        );

        // Ein zweiter Tresor auf demselben Ordner (anderer Prozess) sieht es.
        let zweiter = DateiTresor::im_ordner(ordner.path().join("t"));
        assert_eq!(zweiter.lesen("b").unwrap().as_deref(), Some("zwei"));

        let roh = fs::read_to_string(ordner.path().join("t/tresor.json")).unwrap();
        assert!(!roh.contains("streng-geheim"), "Klartext in der Datei");
        assert!(!roh.contains("\"a\""), "Schlüsselnamen im Klartext");

        tresor.loeschen("a").unwrap();
        assert_eq!(zweiter.lesen("a").unwrap(), None);
        tresor.loeschen("gibt-es-nicht").unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn nur_der_benutzer_darf_lesen() {
        use std::os::unix::fs::PermissionsExt;
        let ordner = tempfile::tempdir().unwrap();
        let tresor = DateiTresor::im_ordner(ordner.path().join("t"));
        tresor.schreiben("a", "x").unwrap();
        for (datei, soll) in [
            ("", 0o700),
            ("tresor.json", 0o600),
            ("tresor.schluessel", 0o600),
        ] {
            let modus = fs::metadata(ordner.path().join("t").join(datei))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(modus, soll, "{datei}");
        }
    }

    #[test]
    fn ein_fehlender_schluessel_wird_gemeldet_und_nichts_angelegt() {
        let ordner = tempfile::tempdir().unwrap();
        let tresor = DateiTresor::im_ordner(ordner.path());
        tresor.schreiben("a", "streng-geheim-4711").unwrap();

        // Der Schlüssel verschwindet (Sicherung zurückgespielt, Platte getauscht).
        let schluessel = ordner.path().join("tresor.schluessel");
        fs::remove_file(&schluessel).unwrap();
        let daten = fs::read(ordner.path().join("tresor.json")).unwrap();

        let fehler = tresor.lesen("a").unwrap_err().to_string();
        assert!(fehler.contains("Schlüssel"), "{fehler}");
        assert!(fehler.contains("unberührt"), "{fehler}");
        assert!(fehler.contains("quiver-Fenster"), "{fehler}");
        assert!(!fehler.contains("streng-geheim-4711"));

        // Nie ein neuer Schlüssel, nie eine geänderte Datei.
        assert!(!schluessel.exists(), "es wurde ein neuer Schlüssel erzeugt");
        assert_eq!(fs::read(ordner.path().join("tresor.json")).unwrap(), daten);
    }

    #[test]
    fn ein_falscher_schluessel_wird_gemeldet_ohne_inhalt() {
        let ordner = tempfile::tempdir().unwrap();
        let tresor = DateiTresor::im_ordner(ordner.path());
        tresor.schreiben("a", "streng-geheim-4711").unwrap();
        fs::write(ordner.path().join("tresor.schluessel"), [7u8; 32]).unwrap();
        let fehler = tresor.lesen("a").unwrap_err().to_string();
        assert!(fehler.contains("entschlüsseln"), "{fehler}");
        assert!(!fehler.contains("streng-geheim"));
    }

    #[test]
    fn gleichzeitige_schreiber_verlieren_nichts() {
        let ordner = tempfile::tempdir().unwrap();
        let pfad = ordner.path().to_path_buf();
        let faeden: Vec<_> = (0..8)
            .map(|nummer| {
                let pfad = pfad.clone();
                std::thread::spawn(move || {
                    let tresor = DateiTresor::im_ordner(pfad);
                    for runde in 0..10 {
                        tresor.schreiben(&format!("{nummer}-{runde}"), "x").unwrap();
                    }
                })
            })
            .collect();
        for faden in faeden {
            faden.join().unwrap();
        }
        let tresor = DateiTresor::im_ordner(pfad);
        for nummer in 0..8 {
            for runde in 0..10 {
                assert!(tresor
                    .lesen(&format!("{nummer}-{runde}"))
                    .unwrap()
                    .is_some());
            }
        }
    }
}
