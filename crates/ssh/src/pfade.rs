// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Wo quiver-ssh seine Dateien ablegt.
//!
//! Schlüssel und die eigene `known_hosts` liegen unter
//! `<daten>/quiver-tools/ssh/`; `<daten>` ist [`DATEN`] (`QUIVER_SSH_DATEN`,
//! Tests setzen das auf einen temp-Ordner) oder `dirs::data_dir()`.
//!
//! ```text
//! <daten>/quiver-tools/ssh/schluessel/web-01_ed25519      (+ .pub)   0600
//! <daten>/quiver-tools/ssh/known_hosts                               0600
//! ```

use std::path::{Path, PathBuf};

/// Umgebungsvariable für `<daten>` (Tests, Sonderfälle).
pub const DATEN: &str = "QUIVER_SSH_DATEN";

/// Ordner für Schlüssel innerhalb von `<daten>/quiver-tools/ssh`.
const SCHLUESSEL: &str = "schluessel";

/// Was an den Pfaden fehlt.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PfadFehler {
    #[error(
        "Der Datenordner ist nicht bekannt – bitte QUIVER_SSH_DATEN setzen oder ein Home-Verzeichnis anlegen."
    )]
    KeinDatenordner,
    #[error("{0} lässt sich nicht anlegen: {1}")]
    Anlegen(PathBuf, String),
    #[error("{0} lässt sich nicht schreiben: {1}")]
    Schreiben(PathBuf, String),
    #[error("{0} lässt sich nicht umbenennen: {1}")]
    Umbenennen(PathBuf, String),
}

/// Die Dateien eines Werkzeugs: der eigene Datenordner und das Home.
#[derive(Debug, Clone)]
pub struct Pfade {
    /// `<daten>/quiver-tools/ssh`; `None` heißt: kein Datenordner bekannt.
    basis: Option<PathBuf>,
    /// Das Home-Verzeichnis für `~/.ssh/known_hosts`; `None` heißt: unbekannt.
    heim: Option<PathBuf>,
}

impl Pfade {
    /// Die Pfade aus der echten Umgebung (so startet quiver das Tool).
    ///
    /// Unter `cfg(test)` liegen sie in einem temporären Ordner je Prozess und
    /// ohne Home: kein Unit-Test kommt so an die echten Schlüssel oder an
    /// `~/.ssh/known_hosts` des Users (Aufgabe 11).
    #[cfg(not(test))]
    pub fn aus_umgebung() -> Pfade {
        let basis = match std::env::var_os(DATEN) {
            Some(wert) if !wert.is_empty() => {
                Some(PathBuf::from(wert).join("quiver-tools").join("ssh"))
            }
            _ => dirs::data_dir().map(|ordner| ordner.join("quiver-tools").join("ssh")),
        };
        Pfade {
            basis,
            heim: heim(),
        }
    }

    /// Im Test: ein temporärer Ordner je Prozess, kein Home.
    #[cfg(test)]
    pub fn aus_umgebung() -> Pfade {
        Pfade {
            basis: Some(
                std::env::temp_dir()
                    .join(format!("quiver-ssh-test-{}", std::process::id()))
                    .join("quiver-tools")
                    .join("ssh"),
            ),
            heim: None,
        }
    }

    /// Feste Pfade (Tests). `basis` ist bereits `<daten>/quiver-tools/ssh`.
    pub fn neu(basis: impl Into<PathBuf>, heim: Option<PathBuf>) -> Pfade {
        Pfade {
            basis: Some(basis.into()),
            heim,
        }
    }

    /// `<daten>/quiver-tools/ssh`.
    pub fn basis(&self) -> Result<&Path, PfadFehler> {
        self.basis.as_deref().ok_or(PfadFehler::KeinDatenordner)
    }

    /// Der private Schlüssel einer Verbindung: `<name>_ed25519`.
    pub fn schluessel(&self, name: &str) -> Result<PathBuf, PfadFehler> {
        Ok(self
            .basis()?
            .join(SCHLUESSEL)
            .join(format!("{name}_ed25519")))
    }

    /// Der vorläufige Schlüssel, solange er noch nicht geprüft ist.
    ///
    /// Er liegt neben dem endgültigen, damit das Übernehmen ein Umbenennen ist
    /// (und damit unteilbar) statt eines Kopierens.
    pub fn vorlaeufig(&self, name: &str) -> Result<PathBuf, PfadFehler> {
        Ok(self
            .basis()?
            .join(SCHLUESSEL)
            .join(format!("{name}_ed25519.neu")))
    }

    /// Die eigene `known_hosts`.
    pub fn bekannte_hosts(&self) -> Result<PathBuf, PfadFehler> {
        Ok(self.basis()?.join("known_hosts"))
    }

    /// Alle Dateien, aus denen Host-Schlüssel gelesen werden: die eigene und
    /// `~/.ssh/known_hosts` (falls ein Home bekannt ist).
    pub fn bekannte_dateien(&self) -> Result<Vec<PathBuf>, PfadFehler> {
        let mut dateien = vec![self.bekannte_hosts()?];
        if let Some(heim) = self.heim.as_ref() {
            dateien.push(heim.join(".ssh").join("known_hosts"));
        }
        Ok(dateien)
    }

    /// Legt `<basis>/schluessel` mit Rechten 0700 an.
    pub fn anlegen(&self) -> Result<(), PfadFehler> {
        let ordner = self.basis()?.join(SCHLUESSEL);
        ordner_anlegen(&ordner)
    }
}

/// `$HOME` (bzw. `%USERPROFILE%`), ohne Plattformwechsel im Code.
pub fn heim() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|wert| !wert.is_empty())
        .map(PathBuf::from)
}

/// `<pfad>.pub`.
pub fn oeffentlich(privat: &Path) -> PathBuf {
    let mut name = privat.as_os_str().to_os_string();
    name.push(".pub");
    PathBuf::from(name)
}

/// Einen Ordner anlegen (samt Eltern) und auf 0700 setzen.
pub fn ordner_anlegen(ordner: &Path) -> Result<(), PfadFehler> {
    std::fs::create_dir_all(ordner)
        .map_err(|fehler| PfadFehler::Anlegen(ordner.to_path_buf(), fehler.to_string()))?;
    nur_ich_ordner(ordner)
        .map_err(|fehler| PfadFehler::Anlegen(ordner.to_path_buf(), fehler.to_string()))
}

/// Eine Datei schreiben und auf 0600 setzen.
pub fn datei_schreiben(datei: &Path, inhalt: &str) -> Result<(), PfadFehler> {
    if let Some(eltern) = datei.parent() {
        ordner_anlegen(eltern)?;
    }
    std::fs::write(datei, inhalt)
        .map_err(|fehler| PfadFehler::Schreiben(datei.to_path_buf(), fehler.to_string()))?;
    nur_ich(datei).map_err(|fehler| PfadFehler::Schreiben(datei.to_path_buf(), fehler.to_string()))
}

/// Eine vorhandene Datei auf 0600 setzen (z. B. eine `known_hosts`, die eine
/// Bibliothek angelegt hat).
pub fn enge_rechte(datei: &Path) -> std::io::Result<()> {
    nur_ich(datei)
}

/// Eine Datei entsteht mit 0600 – auf Systemen ohne diese Rechte ist es ein
/// No-op, weil dort das Home die Grenze zieht.
#[cfg(unix)]
fn nur_ich(datei: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(datei, std::fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn nur_ich(_datei: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn nur_ich_ordner(ordner: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(ordner, std::fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn nur_ich_ordner(_ordner: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schluessel_und_vorlaeufig_liegen_nebeneinander() {
        let pfade = Pfade::neu("/daten/quiver-tools/ssh", None);
        assert_eq!(
            pfade.schluessel("web-01").unwrap(),
            PathBuf::from("/daten/quiver-tools/ssh/schluessel/web-01_ed25519")
        );
        assert_eq!(
            pfade.vorlaeufig("web-01").unwrap(),
            PathBuf::from("/daten/quiver-tools/ssh/schluessel/web-01_ed25519.neu")
        );
        assert_eq!(
            oeffentlich(&pfade.vorlaeufig("web-01").unwrap()),
            PathBuf::from("/daten/quiver-tools/ssh/schluessel/web-01_ed25519.neu.pub")
        );
        assert_eq!(
            pfade.bekannte_hosts().unwrap(),
            PathBuf::from("/daten/quiver-tools/ssh/known_hosts")
        );
    }

    #[test]
    fn bekannte_dateien_enthalten_das_home() {
        let pfade = Pfade::neu("/daten/quiver-tools/ssh", Some(PathBuf::from("/home/test")));
        let dateien = pfade.bekannte_dateien().unwrap();
        assert_eq!(dateien.len(), 2);
        assert_eq!(
            dateien[0],
            PathBuf::from("/daten/quiver-tools/ssh/known_hosts")
        );
        assert_eq!(dateien[1], PathBuf::from("/home/test/.ssh/known_hosts"));

        let ohne_heim = Pfade::neu("/daten/quiver-tools/ssh", None);
        assert_eq!(ohne_heim.bekannte_dateien().unwrap().len(), 1);
    }

    #[test]
    fn ohne_datenordner_ist_alles_ein_fehler_und_kein_panik() {
        let pfade = Pfade {
            basis: None,
            heim: None,
        };
        assert_eq!(
            pfade.schluessel("a").unwrap_err(),
            PfadFehler::KeinDatenordner
        );
        assert_eq!(
            pfade.bekannte_hosts().unwrap_err(),
            PfadFehler::KeinDatenordner
        );
    }

    #[test]
    fn dateien_und_ordner_bekommen_enge_rechte() {
        let temp = tempfile::tempdir().unwrap();
        let pfade = Pfade::neu(temp.path().join("quiver-tools/ssh"), None);
        pfade.anlegen().unwrap();
        let datei = pfade.schluessel("web-01").unwrap();
        datei_schreiben(&datei, "geheim").unwrap();
        assert_eq!(std::fs::read_to_string(&datei).unwrap(), "geheim");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let rechte = std::fs::metadata(&datei).unwrap().permissions().mode() & 0o777;
            assert_eq!(rechte, 0o600);
            let ordner = datei.parent().unwrap();
            let rechte = std::fs::metadata(ordner).unwrap().permissions().mode() & 0o777;
            assert_eq!(rechte, 0o700);
        }
    }

    /// Wächter: die Pfade aus der Umgebung liegen im Test nie unter `HOME` –
    /// sonst schriebe ein Unit-Test in die echten Schlüssel des Users oder
    /// läse dessen `~/.ssh/known_hosts` (Aufgabe 11).
    #[test]
    fn die_pfade_aus_der_umgebung_liegen_im_test_nicht_unter_home() {
        let pfade = Pfade::aus_umgebung();
        let basis = pfade.basis().expect("kein Datenordner im Test");
        for name in ["HOME", "USERPROFILE"] {
            let Some(heim) = std::env::var_os(name).filter(|wert| !wert.is_empty()) else {
                continue;
            };
            let heim = PathBuf::from(heim);
            assert!(
                !basis.starts_with(&heim),
                "{basis:?} liegt unter {name}={heim:?}"
            );
        }
        assert!(basis.starts_with(std::env::temp_dir()), "{basis:?}");
        assert!(pfade.heim.is_none(), "im Test kein Home: {pfade:?}");
    }
}
