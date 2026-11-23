// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Eine SSH-Verbindung, wie der User sie im quiver-Fenster pflegt.
//!
//! quiver übergibt **alle** Verbindungen als JSON in `QUIVER_ZUGAENGE`
//! (`quiver_domain::Mehrfach`): Name plus Werte der Felder `host`, `port`,
//! `benutzer`, `anmelde_passwort`, `sudo_passwort`. Dieses Modul macht daraus
//! geprüfte [`Verbindung`]en und wählt die richtige aus.

use std::collections::BTreeMap;

use quiver_tools_kern::zugaenge::{self, ZugaengeFehler};

/// Feldname des Hosts (Pflicht).
pub const HOST: &str = "host";
/// Feldname des Ports (Standard 22).
pub const PORT: &str = "port";
/// Feldname des Anmeldenamens (Pflicht).
pub const BENUTZER: &str = "benutzer";
/// Feldname des Anmelde-Passworts (geheim, nur für die erste Anmeldung).
pub const ANMELDE_PASSWORT: &str = "anmelde_passwort";
/// Feldname des sudo-Passworts (geheim, getrennt vom Anmelde-Passwort).
pub const SUDO_PASSWORT: &str = "sudo_passwort";

/// Port, wenn keiner eingetragen ist.
pub const STANDARD_PORT: u16 = 22;

/// Eine Verbindung mit allen Werten. `Debug` zeigt Geheimnisse nie.
#[derive(Clone, PartialEq, Eq)]
pub struct Verbindung {
    /// Name aus dem quiver-Fenster, z. B. `web-01`; auch der Schlüsseldateiname.
    pub name: String,
    pub host: String,
    pub port: u16,
    pub benutzer: String,
    /// Nur für die erste Anmeldung; danach meldet sich quiver-ssh per Schlüssel an.
    pub anmelde_passwort: Option<String>,
    /// Nur für Befehle mit `sudo`.
    pub sudo_passwort: Option<String>,
}

impl std::fmt::Debug for Verbindung {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Verbindung")
            .field("name", &self.name)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("benutzer", &self.benutzer)
            .field("anmelde_passwort", &self.anmelde_passwort.is_some())
            .field("sudo_passwort", &self.sudo_passwort.is_some())
            .finish()
    }
}

impl Verbindung {
    /// Eine Verbindung aus einem Eintrag der Liste.
    pub fn aus_zugang(zugang: &zugaenge::Zugang) -> Result<Verbindung, VerbindungsFehler> {
        let name = zugang.name.trim().to_string();
        if !name_gueltig(&name) {
            return Err(VerbindungsFehler::Name(name));
        }

        let host = zugang
            .wert(HOST)
            .ok_or_else(|| VerbindungsFehler::Feld {
                name: name.clone(),
                feld: HOST,
            })?
            .to_string();
        let benutzer = zugang
            .wert(BENUTZER)
            .ok_or_else(|| VerbindungsFehler::Feld {
                name: name.clone(),
                feld: BENUTZER,
            })?
            .to_string();

        let port = match zugang.wert(PORT) {
            None => STANDARD_PORT,
            Some(roh) => roh
                .parse::<u16>()
                .ok()
                .filter(|port| *port > 0)
                .ok_or_else(|| VerbindungsFehler::Port {
                    name: name.clone(),
                    wert: roh.to_string(),
                })?,
        };

        let wert = |feld: &str| zugang.wert(feld).map(str::to_string);
        Ok(Verbindung {
            name,
            host,
            port,
            benutzer,
            anmelde_passwort: wert(ANMELDE_PASSWORT),
            sudo_passwort: wert(SUDO_PASSWORT),
        })
    }

    /// `benutzer@host` bzw. `benutzer@host:port` (der Standardport fehlt).
    pub fn ziel(&self) -> String {
        if self.port == STANDARD_PORT {
            format!("{}@{}", self.benutzer, self.host)
        } else {
            format!("{}@{}:{}", self.benutzer, self.host, self.port)
        }
    }

    /// `host:port`, wie known_hosts es schreibt.
    pub fn adresse(&self) -> String {
        if self.port == STANDARD_PORT {
            self.host.clone()
        } else {
            format!("[{}]:{}", self.host, self.port)
        }
    }
}

/// Namen aus dem quiver-Fenster: `a-z0-9-_`, 1–40 Zeichen, eindeutig.
///
/// Der Name wird Teil eines Dateinamens (`<name>_ed25519`) und darf darum
/// nichts anderes enthalten.
pub fn name_gueltig(name: &str) -> bool {
    (1..=40).contains(&name.len())
        && name.chars().all(|zeichen| {
            zeichen.is_ascii_lowercase()
                || zeichen.is_ascii_digit()
                || zeichen == '-'
                || zeichen == '_'
        })
}

/// Alle Verbindungen, in der Reihenfolge der Liste.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sammlung {
    verbindungen: Vec<Verbindung>,
}

impl Sammlung {
    /// Aus der echten Umgebung (`QUIVER_ZUGAENGE`).
    pub fn aus_umgebung() -> Result<Sammlung, SammelFehler> {
        Sammlung::aus_zugaengen(zugaenge::zugaenge()?)
    }

    /// Aus einer beliebigen Quelle (Tests).
    pub fn mit_umgebung<F>(lesen: F) -> Result<Sammlung, SammelFehler>
    where
        F: Fn(&str) -> Option<String>,
    {
        Sammlung::aus_zugaengen(zugaenge::mit_umgebung(lesen)?)
    }

    /// Aus der Liste, wie quiver sie übergibt.
    pub fn aus_zugaengen(liste: Vec<zugaenge::Zugang>) -> Result<Sammlung, SammelFehler> {
        let mut verbindungen = Vec::with_capacity(liste.len());
        let mut namen = BTreeMap::new();
        for zugang in &liste {
            let verbindung = Verbindung::aus_zugang(zugang)?;
            if namen.insert(verbindung.name.clone(), ()).is_some() {
                return Err(SammelFehler::Doppelt(verbindung.name));
            }
            verbindungen.push(verbindung);
        }
        Ok(Sammlung { verbindungen })
    }

    pub fn alle(&self) -> &[Verbindung] {
        &self.verbindungen
    }

    pub fn ist_leer(&self) -> bool {
        self.verbindungen.is_empty()
    }

    pub fn namen(&self) -> Vec<&str> {
        self.verbindungen
            .iter()
            .map(|verbindung| verbindung.name.as_str())
            .collect()
    }

    /// Die gewünschte Verbindung.
    ///
    /// Ohne Namen gilt die einzige; bei mehreren muss der Agent wählen.
    pub fn waehlen(&self, name: Option<&str>) -> Result<&Verbindung, AuswahlFehler> {
        match name.map(str::trim).filter(|name| !name.is_empty()) {
            Some(name) => self
                .verbindungen
                .iter()
                .find(|verbindung| verbindung.name == name)
                .ok_or_else(|| AuswahlFehler::Unbekannt {
                    name: name.to_string(),
                    bekannt: self.namen().iter().map(|n| n.to_string()).collect(),
                }),
            None => match self.verbindungen.as_slice() {
                [] => Err(AuswahlFehler::Keine),
                [einzige] => Ok(einzige),
                mehrere => Err(AuswahlFehler::Mehrdeutig(
                    mehrere.iter().map(|v| v.name.clone()).collect(),
                )),
            },
        }
    }
}

/// Was an den Werten einer Verbindung nicht stimmt.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VerbindungsFehler {
    #[error("Der Name „{0}“ ist nicht zulässig: erlaubt sind a–z, 0–9, - und _ (1–40 Zeichen).")]
    Name(String),
    #[error(
        "Bei der Verbindung „{name}“ fehlt das Feld „{feld}“ – bitte im quiver-Fenster eintragen."
    )]
    Feld { name: String, feld: &'static str },
    #[error("Bei der Verbindung „{name}“ ist der Port „{wert}“ keine Zahl zwischen 1 und 65535.")]
    Port { name: String, wert: String },
}

/// Warum die Liste nicht gelesen werden konnte.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SammelFehler {
    #[error(transparent)]
    Zugaenge(#[from] ZugaengeFehler),
    #[error(transparent)]
    Verbindung(#[from] VerbindungsFehler),
    #[error("Die Verbindung „{0}“ gibt es mehrfach – bitte im quiver-Fenster einen Namen ändern.")]
    Doppelt(String),
}

/// Warum keine Verbindung gewählt werden konnte.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuswahlFehler {
    #[error("Es ist keine SSH-Verbindung eingerichtet.")]
    Keine,
    #[error(
        "Es gibt mehrere Verbindungen ({}) – bitte `verbindung` angeben.",
        .0.join(", ")
    )]
    Mehrdeutig(Vec<String>),
    #[error(
        "Die Verbindung „{}“ gibt es nicht. Bekannt sind: {}.",
        .name,
        .bekannt.join(", ")
    )]
    Unbekannt { name: String, bekannt: Vec<String> },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zugang(name: &str, werte: &[(&str, &str)]) -> zugaenge::Zugang {
        zugaenge::Zugang {
            name: name.to_string(),
            werte: werte
                .iter()
                .map(|(feld, wert)| (feld.to_string(), wert.to_string()))
                .collect(),
        }
    }

    #[test]
    fn eine_verbindung_wird_geprueft_und_standardport_gesetzt() {
        let verbindung = Verbindung::aus_zugang(&zugang(
            "web-01",
            &[("host", "web.example.test"), ("benutzer", "admin")],
        ))
        .unwrap();
        assert_eq!(verbindung.name, "web-01");
        assert_eq!(verbindung.host, "web.example.test");
        assert_eq!(verbindung.port, 22);
        assert_eq!(verbindung.benutzer, "admin");
        assert_eq!(verbindung.ziel(), "admin@web.example.test");
        assert_eq!(verbindung.adresse(), "web.example.test");
        assert!(verbindung.anmelde_passwort.is_none());
        assert!(verbindung.sudo_passwort.is_none());

        let mit_port = Verbindung::aus_zugang(&zugang(
            "db-02",
            &[
                ("host", "db.example.test"),
                ("port", "2222"),
                ("benutzer", "root"),
                ("anmelde_passwort", "geheim-4711"),
                ("sudo_passwort", "geheim-4712"),
            ],
        ))
        .unwrap();
        assert_eq!(mit_port.ziel(), "root@db.example.test:2222");
        assert_eq!(mit_port.adresse(), "[db.example.test]:2222");
    }

    #[test]
    fn debug_zeigt_keine_passwoerter() {
        let verbindung = Verbindung::aus_zugang(&zugang(
            "web-01",
            &[
                ("host", "web.example.test"),
                ("benutzer", "admin"),
                ("anmelde_passwort", "anmelde-4711"),
                ("sudo_passwort", "sudo-4712"),
            ],
        ))
        .unwrap();
        let text = format!("{verbindung:?}");
        assert!(!text.contains("anmelde-4711"), "{text}");
        assert!(!text.contains("sudo-4712"), "{text}");
        assert!(text.contains("anmelde_passwort: true"), "{text}");
    }

    #[test]
    fn pflichtfelder_und_port_werden_geprueft() {
        assert_eq!(
            Verbindung::aus_zugang(&zugang("web-01", &[("benutzer", "admin")])),
            Err(VerbindungsFehler::Feld {
                name: "web-01".into(),
                feld: HOST
            })
        );
        assert_eq!(
            Verbindung::aus_zugang(&zugang("web-01", &[("host", "h")])),
            Err(VerbindungsFehler::Feld {
                name: "web-01".into(),
                feld: BENUTZER
            })
        );
        assert!(matches!(
            Verbindung::aus_zugang(&zugang(
                "web-01",
                &[("host", "h"), ("benutzer", "u"), ("port", "ssh")]
            )),
            Err(VerbindungsFehler::Port { .. })
        ));
        assert!(matches!(
            Verbindung::aus_zugang(&zugang("Böser Name", &[("host", "h"), ("benutzer", "u")])),
            Err(VerbindungsFehler::Name(_))
        ));
    }

    #[test]
    fn die_auswahl_ist_ohne_namen_eindeutig() {
        let eine =
            Sammlung::aus_zugaengen(vec![zugang("web-01", &[("host", "h"), ("benutzer", "u")])])
                .unwrap();
        assert_eq!(eine.waehlen(None).unwrap().name, "web-01");
        assert_eq!(eine.waehlen(Some("web-01")).unwrap().name, "web-01");
        assert!(matches!(
            eine.waehlen(Some("gibtsnicht")),
            Err(AuswahlFehler::Unbekannt { .. })
        ));

        let zwei = Sammlung::aus_zugaengen(vec![
            zugang("a", &[("host", "h1"), ("benutzer", "u")]),
            zugang("b", &[("host", "h2"), ("benutzer", "u")]),
        ])
        .unwrap();
        assert_eq!(zwei.alle().len(), 2);
        assert_eq!(zwei.namen(), vec!["a", "b"]);
        assert!(matches!(
            zwei.waehlen(None),
            Err(AuswahlFehler::Mehrdeutig(_))
        ));
        assert_eq!(zwei.waehlen(Some("b")).unwrap().host, "h2");

        let keine = Sammlung::default();
        assert!(keine.ist_leer());
        assert_eq!(keine.waehlen(None), Err(AuswahlFehler::Keine));
    }

    #[test]
    fn doppelte_namen_und_kaputtes_json_werden_gemeldet() {
        let doppelt = Sammlung::aus_zugaengen(vec![
            zugang("a", &[("host", "h1"), ("benutzer", "u")]),
            zugang("a", &[("host", "h2"), ("benutzer", "u")]),
        ]);
        assert_eq!(doppelt, Err(SammelFehler::Doppelt("a".into())));

        let kaputt = Sammlung::mit_umgebung(|_| Some("[{\"name\":".to_string()));
        assert!(matches!(kaputt, Err(SammelFehler::Zugaenge(_))));
    }

    #[test]
    fn ohne_umgebung_ist_die_sammlung_leer() {
        let leer = Sammlung::mit_umgebung(|_| None).unwrap();
        assert!(leer.ist_leer());
    }
}
