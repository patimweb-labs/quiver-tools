// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der SSH-Schlüssel je Verbindung (ed25519).
//!
//! Ein Schlüssel entsteht nur, wenn der User noch keinen hat, und wird erst
//! **nach** einer erfolgreichen Schlüssel-Anmeldung endgültig übernommen
//! (Aufgabe 05). Bis dahin liegt er als `.neu` neben dem Ziel; scheitert etwas,
//! werden die Dateien wieder entfernt. Der alte Schlüssel bleibt so lange
//! unangetastet.

use std::path::{Path, PathBuf};

use ssh_key::{Algorithm, HashAlg, LineEnding, PrivateKey, PublicKey};

use crate::pfade::{self, PfadFehler};

/// Ein erzeugtes Schlüsselpaar auf der Platte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paar {
    /// Privater Schlüssel (0600).
    pub privat: PathBuf,
    /// Öffentlicher Schlüssel (`<privat>.pub`).
    pub oeffentlich: PathBuf,
    /// Eine Zeile für `authorized_keys`, mit Kommentar.
    pub zeile: String,
    /// `SHA256:…` des öffentlichen Teils.
    pub fingerprint: String,
}

impl Paar {
    /// Wie das Schlüsselpaar heißt, wenn es übernommen wurde.
    pub fn uebernommen(&self, ziel: &Path) -> Paar {
        Paar {
            privat: ziel.to_path_buf(),
            oeffentlich: pfade::oeffentlich(ziel),
            zeile: self.zeile.clone(),
            fingerprint: self.fingerprint.clone(),
        }
    }
}

/// Was am Schlüssel schiefgehen kann. Enthält nie Schlüsselmaterial.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SchluesselFehler {
    #[error("Der SSH-Schlüssel {0} lässt sich nicht erzeugen: {1}")]
    Erzeugen(PathBuf, String),
    #[error("Der SSH-Schlüssel {0} lässt sich nicht lesen: {1}")]
    Lesen(PathBuf, String),
    #[error("Der SSH-Schlüssel {0} lässt sich nicht übernehmen: {1}")]
    Uebernehmen(PathBuf, String),
    #[error(transparent)]
    Pfad(#[from] PfadFehler),
}

/// Erzeugt einen ed25519-Schlüssel und schreibt beide Dateien (0600).
///
/// Der Kommentar landet als `quiver-ssh <name>` in der `.pub`-Zeile und damit
/// in `authorized_keys`; so findet der User den Eintrag wieder.
pub fn erzeugen(privat: &Path, kommentar: &str) -> Result<Paar, SchluesselFehler> {
    let mut schluessel = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519)
        .map_err(|fehler| SchluesselFehler::Erzeugen(privat.to_path_buf(), fehler.to_string()))?;
    schluessel.set_comment(kommentar);
    let oeffentlich = pfade::oeffentlich(privat);

    let text = schluessel
        .to_openssh(LineEnding::LF)
        .map_err(|fehler| SchluesselFehler::Erzeugen(privat.to_path_buf(), fehler.to_string()))?;
    pfade::datei_schreiben(privat, &text)?;

    let zeile = zeile(&schluessel, kommentar)
        .map_err(|fehler| SchluesselFehler::Erzeugen(privat.to_path_buf(), fehler.to_string()))?;
    pfade::datei_schreiben(&oeffentlich, &format!("{zeile}\n"))?;

    Ok(Paar {
        privat: privat.to_path_buf(),
        oeffentlich,
        fingerprint: fingerprint(schluessel.public_key()),
        zeile,
    })
}

/// Lädt einen privaten Schlüssel von der Platte.
pub fn laden(privat: &Path) -> Result<PrivateKey, SchluesselFehler> {
    russh::keys::load_secret_key(privat, None)
        .map_err(|fehler| SchluesselFehler::Lesen(privat.to_path_buf(), fehler.to_string()))
}

/// `SHA256:…` eines öffentlichen Schlüssels, wie `ssh-keygen -l` sie zeigt.
pub fn fingerprint(schluessel: &PublicKey) -> String {
    schluessel.fingerprint(HashAlg::Sha256).to_string()
}

/// Eine `authorized_keys`-Zeile: `<typ> <base64> <kommentar>`.
pub fn zeile(schluessel: &PrivateKey, kommentar: &str) -> Result<String, ssh_key::Error> {
    let oeffentlich = schluessel.public_key().to_openssh()?;
    let kommentar = kommentar.trim();
    if kommentar.is_empty() {
        Ok(oeffentlich)
    } else {
        Ok(format!("{oeffentlich} {kommentar}"))
    }
}

/// Übernimmt das geprüfte Schlüsselpaar nach `ziel` und entfernt den Vorgänger.
///
/// Erst hier – nachdem eine Anmeldung mit genau diesem Schlüssel geklappt hat –
/// wird der alte Schlüssel ersetzt.
pub fn uebernehmen(paar: &Paar, ziel: &Path) -> Result<Paar, SchluesselFehler> {
    let neues_pub = pfade::oeffentlich(ziel);
    ersetzen(&paar.privat, ziel)?;
    ersetzen(&paar.oeffentlich, &neues_pub)?;
    Ok(paar.uebernommen(ziel))
}

/// Entfernt beide Dateien eines (auch vorläufigen) Paares; Fehler sind egal.
pub fn entfernen(paar: &Paar) {
    let _ = std::fs::remove_file(&paar.privat);
    let _ = std::fs::remove_file(&paar.oeffentlich);
}

fn ersetzen(von: &Path, nach: &Path) -> Result<(), SchluesselFehler> {
    if von == nach {
        return Ok(());
    }
    if nach.exists() {
        std::fs::remove_file(nach).map_err(|fehler| {
            SchluesselFehler::Uebernehmen(nach.to_path_buf(), fehler.to_string())
        })?;
    }
    std::fs::rename(von, nach)
        .map_err(|fehler| SchluesselFehler::Uebernehmen(nach.to_path_buf(), fehler.to_string()))?;
    // Umbenennen ändert die Rechte nicht; den alten Zustand aber nicht neu setzen.
    Ok(())
}

/// Vergleicht zwei `authorized_keys`-Zeilen am Schlüsselmaterial, ohne
/// Kommentar und ohne Optionen davor – so entsteht kein doppelter Eintrag.
pub fn gleiches_material(a: &str, b: &str) -> bool {
    fn material(zeile: &str) -> String {
        let teile: Vec<&str> = zeile.split_whitespace().collect();
        let Some(stelle) = teile.iter().position(|teil| {
            teil.starts_with("ssh-") || teil.starts_with("ecdsa-") || teil.starts_with("sk-")
        }) else {
            return String::new();
        };
        teile[stelle..(stelle + 2).min(teile.len())].join(" ")
    }
    let links = material(a);
    !links.is_empty() && links == material(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn erzeugen_schreibt_beide_dateien_und_eine_zeile_mit_kommentar() {
        let temp = tempfile::tempdir().unwrap();
        let privat = temp.path().join("schluessel/web-01_ed25519");
        let paar = erzeugen(&privat, "quiver-ssh web-01").unwrap();

        assert_eq!(paar.privat, privat);
        assert_eq!(paar.oeffentlich, pfade::oeffentlich(&privat));
        assert!(privat.exists());
        assert!(paar.oeffentlich.exists());

        let oeffentlich = std::fs::read_to_string(&paar.oeffentlich).unwrap();
        assert!(oeffentlich.starts_with("ssh-ed25519 "), "{oeffentlich}");
        assert!(
            oeffentlich.trim_end().ends_with(" quiver-ssh web-01"),
            "{oeffentlich}"
        );
        assert_eq!(
            paar.fingerprint,
            fingerprint(laden(&privat).unwrap().public_key())
        );
        assert!(paar.fingerprint.starts_with("SHA256:"));

        // Der geladene Schlüssel passt zur Zeile.
        let geladen = laden(&privat).unwrap();
        assert!(gleiches_material(
            &paar.zeile,
            &format!(
                "{} beliebiger kommentar",
                geladen.public_key().to_openssh().unwrap()
            )
        ));
    }

    #[test]
    fn uebernehmen_ersetzt_den_alten_schluessel_erst_jetzt() {
        let temp = tempfile::tempdir().unwrap();
        let alt = temp.path().join("a_ed25519");
        let altes = erzeugen(&alt, "alt").unwrap();
        let vorlaeufig = temp.path().join("a_ed25519.neu");
        let neues = erzeugen(&vorlaeufig, "neu").unwrap();

        let uebernommen = uebernehmen(&neues, &alt).unwrap();
        assert_eq!(uebernommen.privat, alt);
        assert!(!vorlaeufig.exists(), "die vorläufige Datei bleibt liegen");
        assert_eq!(
            std::fs::read_to_string(pfade::oeffentlich(&alt))
                .unwrap()
                .trim(),
            neues.zeile
        );
        assert_ne!(altes.fingerprint, uebernommen.fingerprint);
    }

    #[test]
    fn entfernen_raeumt_beide_dateien_weg() {
        let temp = tempfile::tempdir().unwrap();
        let privat = temp.path().join("x_ed25519");
        let paar = erzeugen(&privat, "quiver-ssh x").unwrap();
        entfernen(&paar);
        assert!(!paar.privat.exists());
        assert!(!paar.oeffentlich.exists());
        // Zweimal entfernen ist kein Fehler.
        entfernen(&paar);
    }

    #[test]
    fn gleiches_material_ignoriert_kommentar_und_optionen() {
        let zeile = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIabc quiver-ssh a";
        assert!(gleiches_material(zeile, zeile));
        assert!(gleiches_material(
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIabc anderes",
            zeile
        ));
        assert!(gleiches_material(
            "command=\"/bin/true\" ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIabc",
            zeile
        ));
        assert!(!gleiches_material(
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIxyz quiver-ssh a",
            zeile
        ));
        assert!(!gleiches_material("", zeile));
    }
}
