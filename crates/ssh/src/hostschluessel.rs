// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Host-Schlüssel: Trust on first use (TOFU), nie ein stilles Überschreiben.
//!
//! Gelesen werden zwei Dateien: die eigene `<daten>/quiver-tools/ssh/known_hosts`
//! und `~/.ssh/known_hosts` (auch gehashte Einträge, `|1|salt|hash`). Gemerkt
//! wird nur in der eigenen Datei; `~/.ssh/known_hosts` gehört dem User und wird
//! nie verändert.
//!
//! Ein **geänderter** Host-Schlüssel ist ein harter Fehler: er sieht genauso aus
//! wie ein Angriff und genauso wie ein neu aufgesetzter Server. Die Entscheidung
//! trifft der User, nicht das Werkzeug.

use std::path::PathBuf;

use russh::keys::known_hosts::known_host_keys_path;
use ssh_key::{HashAlg, PublicKey};

use crate::pfade::{self, PfadFehler, Pfade};

/// Was die Prüfung eines angebotenen Host-Schlüssels ergeben hat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stand {
    /// Genau dieser Schlüssel steht schon in einer der Dateien.
    Bekannt,
    /// Für diesen Host ist nichts gespeichert (erste Verbindung).
    Unbekannt,
    /// Für diesen Host ist ein anderer Schlüssel derselben Art gespeichert.
    Geaendert {
        /// Fingerabdrücke der gespeicherten Schlüssel.
        alte: Vec<String>,
        /// Die Datei mit dem ersten Eintrag.
        datei: PathBuf,
        /// Die Zeile darin (1-basiert) – die der User löschen muss.
        zeile: usize,
    },
}

/// Ein gespeicherter Host-Schlüssel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gemerkt {
    pub datei: PathBuf,
    pub zeile: usize,
    pub schluessel: PublicKey,
}

impl Gemerkt {
    /// `SHA256:…`
    pub fn fingerprint(&self) -> String {
        crate::schluessel::fingerprint(&self.schluessel)
    }
}

/// Prüft einen angebotenen Host-Schlüssel gegen beide Dateien.
pub fn pruefen(
    pfade: &Pfade,
    host: &str,
    port: u16,
    angeboten: &PublicKey,
) -> Result<Stand, PfadFehler> {
    let gespeichert = gespeichert(pfade, host, port)?;

    // Nur das Schlüsselmaterial vergleichen; ein Kommentar in der Zeile ändert
    // den Schlüssel nicht.
    if gespeichert
        .iter()
        .any(|eintrag| eintrag.schluessel.key_data() == angeboten.key_data())
    {
        return Ok(Stand::Bekannt);
    }

    // Nur Schlüssel derselben Art sind ein Widerspruch: ein Host darf neben
    // seinem gespeicherten RSA-Schlüssel einen ed25519-Schlüssel anbieten.
    let gleiche_art: Vec<&Gemerkt> = gespeichert
        .iter()
        .filter(|eintrag| eintrag.schluessel.algorithm() == angeboten.algorithm())
        .collect();
    match gleiche_art.first() {
        None => Ok(Stand::Unbekannt),
        Some(erster) => Ok(Stand::Geaendert {
            alte: gleiche_art
                .iter()
                .map(|eintrag| eintrag.fingerprint())
                .collect(),
            datei: erster.datei.clone(),
            zeile: erster.zeile,
        }),
    }
}

/// Alle gespeicherten Schlüssel für `host:port`, aus beiden Dateien.
pub fn gespeichert(pfade: &Pfade, host: &str, port: u16) -> Result<Vec<Gemerkt>, PfadFehler> {
    let mut alle = Vec::new();
    for datei in pfade.bekannte_dateien()? {
        // Eine fehlende oder unlesbare Datei heißt nur: nichts bekannt.
        let Ok(kandidaten) = known_host_keys_path(host, port, &datei) else {
            continue;
        };
        for (zeile, schluessel) in kandidaten {
            alle.push(Gemerkt {
                datei: datei.clone(),
                zeile,
                schluessel,
            });
        }
    }
    Ok(alle)
}

/// Merkt sich einen Host-Schlüssel in der eigenen `known_hosts` (TOFU).
///
/// Die Zeile wird selbst geschrieben statt über `learn_known_hosts_path`: die
/// Bibliothek setzt in eine leere Datei zuerst einen Zeilenumbruch und verschiebt
/// damit alle Zeilennummern, auf die sich die Warnung bei einem geänderten
/// Schlüssel bezieht.
pub fn merken(
    pfade: &Pfade,
    host: &str,
    port: u16,
    schluessel: &PublicKey,
) -> Result<(), PfadFehler> {
    // Schon bekannt: nichts anhängen (ein zweiter Aufruf darf die Datei nicht
    // wachsen lassen).
    if matches!(pruefen(pfade, host, port, schluessel)?, Stand::Bekannt) {
        return Ok(());
    }
    let datei = pfade.bekannte_hosts()?;
    let mut inhalt = std::fs::read_to_string(&datei).unwrap_or_default();
    if !inhalt.is_empty() && !inhalt.ends_with('\n') {
        inhalt.push('\n');
    }
    inhalt.push_str(&format!(
        "{} {}\n",
        hostmuster(host, port),
        schluessel
            .to_openssh()
            .map_err(|fehler| PfadFehler::Schreiben(datei.clone(), fehler.to_string()))?
    ));
    pfade::datei_schreiben(&datei, &inhalt)
}

/// Wie OpenSSH einen Host schreibt: `host` oder `[host]:port`.
pub fn hostmuster(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_string()
    } else {
        format!("[{host}]:{port}")
    }
}

/// Ob für `host:port` überhaupt ein Schlüssel gespeichert ist (für `ssh_status`).
pub fn bekannt(pfade: &Pfade, host: &str, port: u16) -> Option<String> {
    gespeichert(pfade, host, port)
        .ok()
        .and_then(|alle| alle.first().map(Gemerkt::fingerprint))
}

/// Der Fingerabdruck eines angebotenen Schlüssels.
pub fn fingerprint(schluessel: &PublicKey) -> String {
    schluessel.fingerprint(HashAlg::Sha256).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ssh_key::{Algorithm, EcdsaCurve, PrivateKey};

    /// Ein frischer Schlüssel je Test.
    fn neuer_schluessel() -> PublicKey {
        PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519)
            .unwrap()
            .public_key()
            .clone()
    }

    fn pfade(temp: &tempfile::TempDir) -> Pfade {
        Pfade::neu(temp.path().join("quiver-tools/ssh"), None)
    }

    #[test]
    fn unbekannt_dann_gemerkt_dann_bekannt() {
        let temp = tempfile::tempdir().unwrap();
        let pfade = pfade(&temp);
        let key = neuer_schluessel();

        assert_eq!(
            pruefen(&pfade, "web.example.test", 22, &key).unwrap(),
            Stand::Unbekannt
        );
        assert!(bekannt(&pfade, "web.example.test", 22).is_none());

        merken(&pfade, "web.example.test", 22, &key).unwrap();
        assert_eq!(
            pruefen(&pfade, "web.example.test", 22, &key).unwrap(),
            Stand::Bekannt
        );
        assert_eq!(
            bekannt(&pfade, "web.example.test", 22),
            Some(fingerprint(&key))
        );

        let inhalt = std::fs::read_to_string(pfade.bekannte_hosts().unwrap()).unwrap();
        assert!(
            inhalt.starts_with("web.example.test ssh-ed25519 "),
            "{inhalt}"
        );
    }

    #[test]
    fn ein_anderer_schluessel_derselben_art_ist_geaendert() {
        let temp = tempfile::tempdir().unwrap();
        let pfade = pfade(&temp);
        let alt = neuer_schluessel();
        merken(&pfade, "web.example.test", 2222, &alt).unwrap();

        let neu = neuer_schluessel();
        match pruefen(&pfade, "web.example.test", 2222, &neu).unwrap() {
            Stand::Geaendert { alte, datei, zeile } => {
                assert_eq!(alte, vec![fingerprint(&alt)]);
                assert_eq!(datei, pfade.bekannte_hosts().unwrap());
                assert_eq!(zeile, 1);
            }
            sonst => panic!("kein geänderter Schlüssel: {sonst:?}"),
        }

        // Ein anderer Port ist ein anderer Eintrag.
        assert_eq!(
            pruefen(&pfade, "web.example.test", 22, &neu).unwrap(),
            Stand::Unbekannt
        );
    }

    #[test]
    fn eine_andere_schluesselart_ist_kein_widerspruch() {
        let temp = tempfile::tempdir().unwrap();
        let pfade = pfade(&temp);
        merken(&pfade, "web.example.test", 22, &neuer_schluessel()).unwrap();

        // Ein Schlüssel einer anderen Art ist „unbekannt“, nicht „geändert“.
        // (Ein zweiter ed25519 wäre eine Änderung – darum ECDSA/P-256, das geht
        // ohne RSA-Erzeugung schnell.)
        let ecdsa = PrivateKey::random(
            &mut rand::rng(),
            Algorithm::Ecdsa {
                curve: EcdsaCurve::NistP256,
            },
        )
        .unwrap();
        assert_eq!(
            pruefen(&pfade, "web.example.test", 22, ecdsa.public_key()).unwrap(),
            Stand::Unbekannt
        );
    }

    #[test]
    fn die_zweite_datei_wird_gelesen_aber_nie_geschrieben() {
        let temp = tempfile::tempdir().unwrap();
        let heim = temp.path().join("home");
        let pfade = Pfade::neu(temp.path().join("quiver-tools/ssh"), Some(heim.clone()));
        std::fs::create_dir_all(heim.join(".ssh")).unwrap();
        let key = neuer_schluessel();
        let zeile = format!("web.example.test {}\n", key.to_openssh().unwrap());
        std::fs::write(heim.join(".ssh/known_hosts"), &zeile).unwrap();

        assert_eq!(
            pruefen(&pfade, "web.example.test", 22, &key).unwrap(),
            Stand::Bekannt
        );
        // Ein schon bekannter Schlüssel wird nicht doppelt gemerkt.
        merken(&pfade, "web.example.test", 22, &key).unwrap();
        assert_eq!(
            std::fs::read_to_string(heim.join(".ssh/known_hosts")).unwrap(),
            zeile
        );
        assert!(
            !pfade.bekannte_hosts().unwrap().exists(),
            "nichts Neues zu merken, also keine eigene Datei"
        );

        // Ein anderer Host landet in der eigenen Datei, die zweite bleibt unberührt.
        let anderer = neuer_schluessel();
        merken(&pfade, "db.example.test", 22, &anderer).unwrap();
        let eigen = std::fs::read_to_string(pfade.bekannte_hosts().unwrap()).unwrap();
        assert!(eigen.starts_with("db.example.test ssh-ed25519 "), "{eigen}");
        assert_eq!(
            std::fs::read_to_string(heim.join(".ssh/known_hosts")).unwrap(),
            zeile
        );
    }

    #[test]
    fn gehashte_eintraege_werden_erkannt() {
        // Ein echter, von OpenSSH gehashter Eintrag (|1|salt|hmac-sha1).
        const GEHASHT: &str = "|1|O33ESRMWPVkMYIwJ1Uw+n877jTo=|nuuC5vEqXlEZ/8BXQR7m619W6Ak=";
        const ZEILE: &str =
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILIG2T/B0l0gaqj3puu510tu9N1OkQ4znY3LYuEm5zCF";

        let temp = tempfile::tempdir().unwrap();
        let heim = temp.path().join("home");
        let pfade = Pfade::neu(temp.path().join("quiver-tools/ssh"), Some(heim.clone()));
        std::fs::create_dir_all(heim.join(".ssh")).unwrap();
        std::fs::write(
            heim.join(".ssh/known_hosts"),
            format!("{GEHASHT} {ZEILE}\n"),
        )
        .unwrap();

        let schluessel = PublicKey::from_openssh(ZEILE).unwrap();
        assert_eq!(
            pruefen(&pfade, "example.com", 22, &schluessel).unwrap(),
            Stand::Bekannt
        );
        assert_eq!(
            pruefen(&pfade, "kein-treffer.example", 22, &schluessel).unwrap(),
            Stand::Unbekannt
        );
    }
}
