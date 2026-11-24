// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der eine Weg, Dateien eines Werkzeugs abzulegen (Härtung Paket 4).
//!
//! Ein [`Ablage`] hat **einen** Arbeitsordner aus dem quiver-Fenster (Feld
//! `ordner`, Feldart „Ordner“). Alles darin ist relativ, und ein relativer Pfad
//! verlässt den Ordner nie:
//!
//! - **Abgelehnt** werden absolute Pfade, `..`, `.`, NUL, Backslash, `:`,
//!   versteckte Namen (`.…`), Steuerzeichen und Windows-Gerätenamen
//!   (`CON`, `NUL`, `COM1`, …) – auf jeder Ebene.
//! - **Keine symbolischen Links**: jede existierende Ebene wird geprüft.
//! - **Kein Überschreiben**: neue Dateien entstehen mit `create_new`.
//! - **Grenze**: höchstens [`MAX_BYTES`] je Datei.
//!
//! Damit kann ein Dateiname aus dem Inhalt (etwa ein Wiki-Titel) nichts
//! außerhalb des Arbeitsordners anlegen.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Höchstgröße einer Datei: 10 MB.
pub const MAX_BYTES: u64 = 10 * 1024 * 1024;

/// So tief dürfen Unterordner sein.
const MAX_TIEFE: usize = 16;

/// Ein Arbeitsordner.
#[derive(Debug, Clone)]
pub struct Ablage {
    wurzel: PathBuf,
}

impl Ablage {
    /// Eine Ablage über dem eingetragenen Ordner; er wird bei Bedarf angelegt.
    ///
    /// `~` am Anfang wird auf das Benutzerverzeichnis aufgelöst (kein
    /// Betriebssystem-Sonderfall: `HOME` gilt überall, wo die Tools laufen).
    pub fn neu(ordner: &str) -> Result<Ablage, AblageFehler> {
        let ordner = ordner.trim();
        if ordner.is_empty() {
            return Err(AblageFehler::Leer);
        }
        let pfad = aufloesen(ordner);
        std::fs::create_dir_all(&pfad).map_err(|fehler| AblageFehler::Ordner {
            pfad: pfad.display().to_string(),
            grund: fehler.to_string(),
        })?;
        let wurzel = std::fs::canonicalize(&pfad).map_err(|fehler| AblageFehler::Ordner {
            pfad: pfad.display().to_string(),
            grund: fehler.to_string(),
        })?;
        Ok(Ablage { wurzel })
    }

    /// Der Arbeitsordner, wie er auf der Platte heißt (aufgelöst).
    pub fn wurzel(&self) -> &Path {
        &self.wurzel
    }

    /// Der volle Pfad zu einem relativen Namen; Symlinks werden geprüft.
    pub fn pfad(&self, relativ: &str) -> Result<PathBuf, AblageFehler> {
        let teile = pruefe_relativ(relativ)?;
        let voll = teile
            .iter()
            .fold(self.wurzel.clone(), |pfad, teil| pfad.join(teil));
        pruefe_symlinks(&self.wurzel, &voll)?;
        Ok(voll)
    }

    /// Eine Datei neu anlegen; Vorhandenes bleibt unberührt.
    pub fn schreiben_neu(&self, relativ: &str, inhalt: &str) -> Result<PathBuf, AblageFehler> {
        self.schreiben_bytes_neu(relativ, inhalt.as_bytes())
    }

    /// Wie [`Ablage::schreiben_neu`], für beliebige Bytes.
    pub fn schreiben_bytes_neu(
        &self,
        relativ: &str,
        inhalt: &[u8],
    ) -> Result<PathBuf, AblageFehler> {
        if inhalt.len() as u64 > MAX_BYTES {
            return Err(AblageFehler::ZuGross {
                pfad: relativ.to_string(),
                max_mb: MAX_BYTES / (1024 * 1024),
            });
        }
        let voll = self.pfad(relativ)?;
        if let Some(eltern) = voll.parent() {
            std::fs::create_dir_all(eltern).map_err(|fehler| AblageFehler::Ordner {
                pfad: eltern.display().to_string(),
                grund: fehler.to_string(),
            })?;
        }
        pruefe_symlinks(&self.wurzel, &voll)?;
        let mut datei = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&voll)
            .map_err(|fehler| match fehler.kind() {
                std::io::ErrorKind::AlreadyExists => AblageFehler::Existiert(relativ.to_string()),
                _ => AblageFehler::Datei {
                    pfad: voll.display().to_string(),
                    grund: fehler.to_string(),
                },
            })?;
        datei
            .write_all(inhalt)
            .map_err(|fehler| AblageFehler::Datei {
                pfad: voll.display().to_string(),
                grund: fehler.to_string(),
            })?;
        Ok(voll)
    }
}

/// Was beim Ablegen schiefgeht.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AblageFehler {
    #[error("Der Arbeitsordner ist nicht eingetragen – bitte im quiver-Fenster das Feld „Ordner“ füllen.")]
    Leer,
    #[error("Der Arbeitsordner „{pfad}“ lässt sich nicht anlegen: {grund}")]
    Ordner { pfad: String, grund: String },
    #[error("Der Name „{0}“ ist nicht zulässig.")]
    Verboten(String),
    #[error("„{0}“ liegt außerhalb des Arbeitsordners.")]
    Ausbruch(String),
    #[error("„{0}“ ist ein symbolischer Link – dort wird nicht geschrieben.")]
    Symlink(String),
    #[error("„{0}“ gibt es schon – vorhandene Dateien werden nicht überschrieben.")]
    Existiert(String),
    #[error("„{pfad}“ ist größer als {max_mb} MB.")]
    ZuGross { pfad: String, max_mb: u64 },
    #[error("„{pfad}“ lässt sich nicht schreiben: {grund}")]
    Datei { pfad: String, grund: String },
}

/// `~` am Anfang auf `HOME` auflösen (kein `cfg`-Sonderfall).
fn aufloesen(ordner: &str) -> PathBuf {
    match (ordner.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(zuhause)) if !rest.is_empty() => PathBuf::from(zuhause).join(rest),
        _ => PathBuf::from(ordner),
    }
}

/// Die Regeln für einen relativen Namen; liefert die Segmente.
fn pruefe_relativ(relativ: &str) -> Result<Vec<String>, AblageFehler> {
    let relativ = relativ.trim();
    if relativ.is_empty() {
        return Err(AblageFehler::Leer);
    }
    if relativ.contains('\0') || relativ.chars().any(char::is_control) {
        return Err(AblageFehler::Verboten(relativ.to_string()));
    }
    if relativ.contains('\\') || relativ.contains(':') {
        return Err(AblageFehler::Verboten(relativ.to_string()));
    }
    if Path::new(relativ).is_absolute() || relativ.starts_with('/') {
        return Err(AblageFehler::Ausbruch(relativ.to_string()));
    }

    let mut teile = Vec::new();
    for teil in relativ.split('/') {
        if teil.is_empty() || teil == "." {
            return Err(AblageFehler::Verboten(relativ.to_string()));
        }
        if teil == ".." {
            return Err(AblageFehler::Ausbruch(relativ.to_string()));
        }
        if teil.starts_with('.') {
            return Err(AblageFehler::Verboten(teil.to_string()));
        }
        if ist_geraetename(teil) {
            return Err(AblageFehler::Verboten(teil.to_string()));
        }
        if teil.chars().count() > 255 {
            return Err(AblageFehler::Verboten(teil.to_string()));
        }
        teile.push(teil.to_string());
    }
    if teile.len() > MAX_TIEFE + 1 {
        return Err(AblageFehler::Verboten(relativ.to_string()));
    }
    Ok(teile)
}

/// Jede existierende Ebene unterhalb der Wurzel auf einen Symlink prüfen.
fn pruefe_symlinks(wurzel: &Path, voll: &Path) -> Result<(), AblageFehler> {
    let relativ = voll
        .strip_prefix(wurzel)
        .map_err(|_| AblageFehler::Ausbruch(voll.display().to_string()))?;
    let mut laufend = wurzel.to_path_buf();
    for teil in relativ.iter() {
        laufend.push(teil);
        match std::fs::symlink_metadata(&laufend) {
            Ok(daten) if daten.file_type().is_symlink() => {
                return Err(AblageFehler::Symlink(laufend.display().to_string()))
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    Ok(())
}

/// Windows-Gerätenamen, die auch auf anderen Systemen Ärger machen.
fn ist_geraetename(teil: &str) -> bool {
    let ohne_endung = teil.split('.').next().unwrap_or(teil);
    let gross = ohne_endung.to_ascii_uppercase();
    matches!(gross.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ("COM1"..="COM9").contains(&gross.as_str())
        || ("LPT1"..="LPT9").contains(&gross.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eine_datei_entsteht_nur_einmal() {
        let ordner = tempfile::tempdir().unwrap();
        let ablage = Ablage::neu(ordner.path().to_str().unwrap()).unwrap();
        let pfad = ablage.schreiben_neu("wiki/seite.md", "# Titel\n").unwrap();
        assert_eq!(std::fs::read_to_string(&pfad).unwrap(), "# Titel\n");
        assert_eq!(
            ablage.schreiben_neu("wiki/seite.md", "anders"),
            Err(AblageFehler::Existiert("wiki/seite.md".into()))
        );
        assert_eq!(std::fs::read_to_string(&pfad).unwrap(), "# Titel\n");
    }

    #[test]
    fn pfade_bleiben_im_arbeitsordner() {
        let ordner = tempfile::tempdir().unwrap();
        let ablage = Ablage::neu(ordner.path().to_str().unwrap()).unwrap();
        for falsch in [
            "",
            "/etc/passwd",
            "../raus.md",
            "wiki/../../raus.md",
            ".versteckt.md",
            "wiki\\seite.md",
            "c:seite.md",
            "CON.md",
            "a\0b.md",
        ] {
            assert!(ablage.pfad(falsch).is_err(), "{falsch:?}");
        }
        assert!(ablage.pfad("wiki/seite.md").is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn ein_symlink_wird_nicht_durchschrieben() {
        use std::os::unix::fs::symlink;
        let ordner = tempfile::tempdir().unwrap();
        let ablage = Ablage::neu(ordner.path().to_str().unwrap()).unwrap();
        let ziel = ordner.path().join("ziel.txt");
        std::fs::write(&ziel, "unberuehrt").unwrap();
        symlink(&ziel, ordner.path().join("seite.md")).unwrap();

        let fehler = ablage.schreiben_neu("seite.md", "boese").unwrap_err();
        assert!(matches!(fehler, AblageFehler::Symlink(_)), "{fehler:?}");
        assert_eq!(std::fs::read_to_string(&ziel).unwrap(), "unberuehrt");
    }

    #[test]
    fn eine_zu_grosse_datei_wird_abgelehnt() {
        let ordner = tempfile::tempdir().unwrap();
        let ablage = Ablage::neu(ordner.path().to_str().unwrap()).unwrap();
        let riesig = vec![b'x'; (MAX_BYTES + 1) as usize];
        assert!(matches!(
            ablage.schreiben_bytes_neu("gross.md", &riesig),
            Err(AblageFehler::ZuGross { .. })
        ));
        assert!(!ordner.path().join("gross.md").exists());
    }

    #[test]
    fn ein_leerer_ordner_wird_gemeldet() {
        assert_eq!(Ablage::neu("   ").unwrap_err(), AblageFehler::Leer);
    }
}
