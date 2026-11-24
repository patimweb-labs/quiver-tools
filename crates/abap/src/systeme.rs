// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Ein SAP-System, wie der User es im quiver-Fenster pflegt (Aufgabe 07).
//!
//! quiver übergibt **alle** Systeme als JSON in `QUIVER_ZUGAENGE`
//! (`quiver_domain::Mehrfach`): Name plus Werte der Felder `benutzer`, `url`,
//! `mandant`, `sprache`, `passwort`, `zertifikat_pruefen`, `schreiben`. Dieses Modul macht
//! daraus geprüfte [`System`]e und wählt das richtige aus. Das Passwort bleibt
//! im Speicher und erscheint in keiner Antwort – `Debug` zeigt nur, *ob* eines
//! gesetzt ist.

use std::collections::BTreeMap;

use quiver_tools_kern::zugaenge::{self, ZugaengeFehler};

/// Feldname des SAP-Benutzers (Pflicht).
pub const BENUTZER: &str = "benutzer";
/// Feldname der ADT-Adresse (Pflicht), z. B. `https://sap-dev.firma.de:44300`.
pub const URL: &str = "url";
/// Feldname des Mandanten (Pflicht), z. B. `100`.
pub const MANDANT: &str = "mandant";
/// Feldname der Anmeldesprache (Standard `DE`).
pub const SPRACHE: &str = "sprache";
/// Feldname des SAP-Passworts (geheim, Pflicht).
pub const PASSWORT: &str = "passwort";
/// Feldname des Schalters „Serverzertifikat prüfen“ (Standard an).
pub const ZERTIFIKAT_PRUEFEN: &str = "zertifikat_pruefen";
/// Feldname des Schalters „Schreiben erlauben“ (Standard **aus**, Aufgabe 08).
pub const SCHREIBEN: &str = "schreiben";

/// Anmeldesprache, wenn keine eingetragen ist.
pub const STANDARD_SPRACHE: &str = "DE";

/// Ein SAP-System mit allen Werten. `Debug` zeigt das Passwort nie.
#[derive(Clone, PartialEq, Eq)]
pub struct System {
    /// Name aus dem quiver-Fenster, z. B. `dev-100`.
    pub name: String,
    pub benutzer: String,
    /// Basisadresse ohne Schrägstrich am Ende.
    pub url: String,
    pub mandant: String,
    /// Zweistelliger Sprachschlüssel, Großbuchstaben (`DE`, `EN`).
    pub sprache: String,
    pub passwort: String,
    /// Serverzertifikat gegen den System-Zertifikatsspeicher prüfen.
    pub zertifikat_pruefen: bool,
    /// Erlaubt die schreibenden Werkzeuge. Standard **aus**.
    pub schreiben: bool,
}

impl std::fmt::Debug for System {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("System")
            .field("name", &self.name)
            .field("benutzer", &self.benutzer)
            .field("url", &self.url)
            .field("mandant", &self.mandant)
            .field("sprache", &self.sprache)
            .field("passwort", &(!self.passwort.is_empty()))
            .field("zertifikat_pruefen", &self.zertifikat_pruefen)
            .field("schreiben", &self.schreiben)
            .finish()
    }
}

impl System {
    /// Ein System aus einem Eintrag der Liste.
    pub fn aus_zugang(zugang: &zugaenge::Zugang) -> Result<System, SystemFehler> {
        let name = zugang.name.trim().to_string();
        if name.is_empty() {
            return Err(SystemFehler::Name);
        }

        let feld = |feld: &'static str| -> Result<String, SystemFehler> {
            zugang
                .wert(feld)
                .map(str::to_string)
                .ok_or_else(|| SystemFehler::Feld {
                    name: name.clone(),
                    feld,
                })
        };

        let benutzer = feld(BENUTZER)?;
        let url = feld(URL)?;
        let passwort = feld(PASSWORT)?;
        let mandant = feld(MANDANT)?;

        let url = url.trim_end_matches('/').to_string();
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(SystemFehler::Url {
                name: name.clone(),
                url,
            });
        }

        let sprache = zugang
            .wert(SPRACHE)
            .map(|sprache| sprache.trim().to_uppercase())
            .filter(|sprache| !sprache.is_empty())
            .unwrap_or_else(|| STANDARD_SPRACHE.to_string());

        Ok(System {
            name,
            benutzer,
            url,
            mandant,
            sprache,
            passwort,
            zertifikat_pruefen: schalter(zugang.wert(ZERTIFIKAT_PRUEFEN)),
            schreiben: schalter_aus(zugang.wert(SCHREIBEN)),
        })
    }

    /// `Schreiben` – ob die schreibenden Werkzeuge auf diesem System dürfen.
    pub fn schreiben_erlaubt(&self) -> bool {
        self.schreiben
    }

    /// `name · benutzer@host:port · Mandant 100 · DE · Schreiben aus` – ohne Passwort.
    pub fn beschreibung(&self) -> String {
        format!(
            "{} · {}@{} · Mandant {} · {} · Schreiben {}",
            self.name,
            self.benutzer,
            self.host(),
            self.mandant,
            self.sprache,
            if self.schreiben { "an" } else { "aus" }
        )
    }

    /// `benutzer@host` – der Name, mit dem sich das Werkzeug anmeldet.
    pub fn anmeldung(&self) -> String {
        format!("{}@{}", self.benutzer, self.host())
    }

    /// Der Host (mit Port), wie er in der Adresse steht.
    pub fn host(&self) -> &str {
        let ohne_schema = self
            .url
            .strip_prefix("https://")
            .or_else(|| self.url.strip_prefix("http://"))
            .unwrap_or(&self.url);
        ohne_schema.split('/').next().unwrap_or(ohne_schema)
    }

    /// Prüft das Serverzertifikat nicht (nur wenn der User es abschaltet).
    pub fn zertifikat_ungeprueft(&self) -> bool {
        !self.zertifikat_pruefen
    }
}

/// Ein Schalter aus dem quiver-Fenster: fehlt er, gilt „an“.
///
/// Nur ausdrückliche Abschaltungen (`aus`, `nein`, `0`, `false`, `off`) sind
/// falsch; alles andere ist wahr. So bleibt der sichere Zustand der Standard.
pub fn schalter(wert: Option<&str>) -> bool {
    match wert.map(str::trim) {
        None | Some("") => true,
        Some(wert) => !matches!(
            wert.to_ascii_lowercase().as_str(),
            "aus" | "nein" | "0" | "false" | "off"
        ),
    }
}

/// Ein Schalter, der im Zweifel **aus** ist (Feld `schreiben`, Aufgabe 08).
///
/// Nur ein ausdrückliches Einschalten (`an`, `ja`, `1`, `true`, `ein`, `on`)
/// gilt; ein fehlendes Feld und jede andere Schreibweise bleiben aus. Für ein
/// Produktivsystem ist „nichts gesagt“ die sichere Antwort.
pub fn schalter_aus(wert: Option<&str>) -> bool {
    match wert.map(str::trim) {
        None | Some("") => false,
        Some(wert) => matches!(
            wert.to_ascii_lowercase().as_str(),
            "an" | "ja" | "1" | "true" | "ein" | "on"
        ),
    }
}

/// Alle Systeme, in der Reihenfolge der Liste.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sammlung {
    systeme: Vec<System>,
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
        let mut systeme = Vec::with_capacity(liste.len());
        let mut namen = BTreeMap::new();
        for zugang in &liste {
            let system = System::aus_zugang(zugang)?;
            if namen.insert(system.name.clone(), ()).is_some() {
                return Err(SammelFehler::Doppelt(system.name));
            }
            systeme.push(system);
        }
        Ok(Sammlung { systeme })
    }

    pub fn alle(&self) -> &[System] {
        &self.systeme
    }

    pub fn ist_leer(&self) -> bool {
        self.systeme.is_empty()
    }

    pub fn namen(&self) -> Vec<&str> {
        self.systeme
            .iter()
            .map(|system| system.name.as_str())
            .collect()
    }

    /// Das gewünschte System.
    ///
    /// Ohne Namen gilt das einzige; bei mehreren muss der Agent wählen.
    pub fn waehlen(&self, name: Option<&str>) -> Result<&System, AuswahlFehler> {
        match name.map(str::trim).filter(|name| !name.is_empty()) {
            Some(name) => self
                .systeme
                .iter()
                .find(|system| system.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| AuswahlFehler::Unbekannt {
                    name: name.to_string(),
                    bekannt: self.namen().iter().map(|n| n.to_string()).collect(),
                }),
            None => match self.systeme.as_slice() {
                [] => Err(AuswahlFehler::Keine),
                [einzige] => Ok(einzige),
                mehrere => Err(AuswahlFehler::Mehrdeutig(
                    mehrere.iter().map(|s| s.name.clone()).collect(),
                )),
            },
        }
    }
}

/// Was an den Werten eines Systems nicht stimmt.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SystemFehler {
    #[error("Ein System im quiver-Fenster hat keinen Namen – bitte einen Namen vergeben.")]
    Name,
    #[error(
        "Bei dem System „{name}“ fehlt das Feld „{feld}“ – bitte im quiver-Fenster eintragen."
    )]
    Feld { name: String, feld: &'static str },
    #[error(
        "Bei dem System „{name}“ ist die Adresse „{url}“ keine ADT-Adresse – sie muss mit \
         http:// oder https:// beginnen."
    )]
    Url { name: String, url: String },
}

/// Warum die Liste nicht gelesen werden konnte.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SammelFehler {
    #[error(transparent)]
    Zugaenge(#[from] ZugaengeFehler),
    #[error(transparent)]
    System(#[from] SystemFehler),
    #[error("Das System „{0}“ gibt es mehrfach – bitte im quiver-Fenster einen Namen ändern.")]
    Doppelt(String),
}

/// Warum kein System gewählt werden konnte.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuswahlFehler {
    #[error("Es ist kein SAP-System eingerichtet.")]
    Keine,
    #[error(
        "Es gibt mehrere Systeme ({}) – bitte `system` angeben.",
        .0.join(", ")
    )]
    Mehrdeutig(Vec<String>),
    #[error(
        "Das System „{}“ gibt es nicht. Bekannt sind: {}.",
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

    fn vollstaendig(name: &str) -> zugaenge::Zugang {
        zugang(
            name,
            &[
                ("benutzer", "DEVELOPER"),
                ("url", "https://sap-dev.example.test:44300"),
                ("mandant", "100"),
                ("passwort", "geheim-4711"),
            ],
        )
    }

    #[test]
    fn ein_system_wird_geprueft_und_hat_vorgaben() {
        let system = System::aus_zugang(&vollstaendig("dev-100")).unwrap();
        assert_eq!(system.name, "dev-100");
        assert_eq!(system.benutzer, "DEVELOPER");
        assert_eq!(system.url, "https://sap-dev.example.test:44300");
        assert_eq!(system.mandant, "100");
        assert_eq!(system.sprache, "DE");
        assert!(system.zertifikat_pruefen);
        // Ohne Feld ist das Schreiben aus – der sichere Zustand.
        assert!(!system.schreiben);
        assert!(!system.schreiben_erlaubt());
        assert_eq!(system.host(), "sap-dev.example.test:44300");
        assert_eq!(
            system.beschreibung(),
            "dev-100 · DEVELOPER@sap-dev.example.test:44300 · Mandant 100 · DE · Schreiben aus"
        );
        assert_eq!(system.anmeldung(), "DEVELOPER@sap-dev.example.test:44300");
    }

    #[test]
    fn schreiben_ist_nur_ausdruecklich_an() {
        let mit = |wert: &str| {
            let alle = vec![
                ("benutzer", "DEVELOPER"),
                ("url", "https://sap.example.test"),
                ("mandant", "100"),
                ("passwort", "x"),
                ("schreiben", wert),
            ];
            System::aus_zugang(&zugang("dev", &alle)).unwrap().schreiben
        };

        for an in ["an", "AN", "ja", "1", "true", "ein", "on"] {
            assert!(mit(an), "{an} sollte an sein");
        }
        for aus in ["aus", "nein", "0", "false", "off", "quatsch", "  "] {
            assert!(!mit(aus), "{aus} sollte aus sein");
        }
        // Ohne Feld und mit leerem Wert bleibt es aus.
        assert!(!schalter_aus(None));
        assert!(!schalter_aus(Some("")));
    }

    #[test]
    fn sprache_und_abschluesse_werden_gelesen() {
        let mit = |werte: &[(&str, &str)]| {
            let mut alle = vec![
                ("benutzer", "DEVELOPER"),
                ("url", "https://sap.example.test/"),
                ("mandant", "100"),
                ("passwort", "x"),
            ];
            alle.extend_from_slice(werte);
            System::aus_zugang(&zugang("dev", &alle)).unwrap()
        };

        let system = mit(&[("sprache", "en"), ("zertifikat_pruefen", "aus")]);
        assert_eq!(system.sprache, "EN");
        assert!(!system.zertifikat_pruefen);
        assert!(system.zertifikat_ungeprueft());
        // Der Schrägstrich am Ende fliegt raus.
        assert_eq!(system.url, "https://sap.example.test");

        // „an“ und fehlende Angabe sind beide wahr.
        assert!(mit(&[("zertifikat_pruefen", "an")]).zertifikat_pruefen);
        assert!(mit(&[]).zertifikat_pruefen);
        assert!(!schalter(Some("nein")));
        assert!(!schalter(Some("false")));
        assert!(!schalter(Some("0")));
        assert!(!schalter(Some("off")));
        assert!(schalter(Some("  ")));
        assert!(schalter(Some("1")));
    }

    #[test]
    fn der_systemname_ist_ohne_gross_und_kleinschreibung() {
        // Der User nennt das System „BL4_900“, der Agent schreibt „bl4_900“.
        let sammlung =
            Sammlung::aus_zugaengen(vec![vollstaendig("BL4_900"), vollstaendig("dev-100")])
                .unwrap();
        assert_eq!(sammlung.waehlen(Some("bl4_900")).unwrap().name, "BL4_900");
        assert_eq!(sammlung.waehlen(Some("DEV-100")).unwrap().name, "dev-100");
        assert!(sammlung.waehlen(Some("bl4_901")).is_err());
    }

    #[test]
    fn debug_zeigt_kein_passwort() {
        let system = System::aus_zugang(&vollstaendig("dev-100")).unwrap();
        let text = format!("{system:?}");
        assert!(!text.contains("geheim-4711"), "{text}");
        assert!(text.contains("passwort: true"), "{text}");
    }

    #[test]
    fn pflichtfelder_und_adresse_werden_geprueft() {
        assert_eq!(
            System::aus_zugang(&zugang("dev", &[("url", "https://x"), ("mandant", "100")])),
            Err(SystemFehler::Feld {
                name: "dev".into(),
                feld: BENUTZER
            })
        );
        assert_eq!(
            System::aus_zugang(&zugang(
                "dev",
                &[("benutzer", "u"), ("mandant", "100"), ("passwort", "p")]
            )),
            Err(SystemFehler::Feld {
                name: "dev".into(),
                feld: URL
            })
        );
        assert_eq!(
            System::aus_zugang(&zugang(
                "dev",
                &[
                    ("benutzer", "u"),
                    ("url", "sap.example.test"),
                    ("mandant", "100"),
                    ("passwort", "p"),
                ]
            )),
            Err(SystemFehler::Url {
                name: "dev".into(),
                url: "sap.example.test".into()
            })
        );
        assert_eq!(
            System::aus_zugang(&zugang("  ", &[("benutzer", "u"), ("url", "https://x")])),
            Err(SystemFehler::Name)
        );
    }

    #[test]
    fn die_auswahl_ist_ohne_namen_eindeutig() {
        let eins = Sammlung::aus_zugaengen(vec![vollstaendig("dev-100")]).unwrap();
        assert_eq!(eins.waehlen(None).unwrap().name, "dev-100");
        assert_eq!(eins.waehlen(Some("dev-100")).unwrap().name, "dev-100");
        assert!(matches!(
            eins.waehlen(Some("gibtsnicht")),
            Err(AuswahlFehler::Unbekannt { .. })
        ));

        let zwei = zwei_systeme();
        assert_eq!(zwei.alle().len(), 2);
        assert_eq!(zwei.namen(), vec!["dev-100", "qas-200"]);
        assert!(matches!(
            zwei.waehlen(None),
            Err(AuswahlFehler::Mehrdeutig(_))
        ));
        assert_eq!(zwei.waehlen(Some("qas-200")).unwrap().mandant, "200");

        let keine = Sammlung::default();
        assert!(keine.ist_leer());
        assert_eq!(keine.waehlen(None), Err(AuswahlFehler::Keine));
    }

    /// Zwei Systeme für die Auswahl-Tests.
    fn zwei_systeme() -> Sammlung {
        Sammlung::aus_zugaengen(vec![
            vollstaendig("dev-100"),
            zugang(
                "qas-200",
                &[
                    ("benutzer", "DEVELOPER"),
                    ("url", "https://sap-qas.example.test:44300"),
                    ("mandant", "200"),
                    ("passwort", "geheim-4712"),
                ],
            ),
        ])
        .unwrap()
    }

    #[test]
    fn doppelte_namen_und_kaputtes_json_werden_gemeldet() {
        let doppelt = Sammlung::aus_zugaengen(vec![vollstaendig("dev"), vollstaendig("dev")]);
        assert_eq!(doppelt, Err(SammelFehler::Doppelt("dev".into())));

        let kaputt = Sammlung::mit_umgebung(|_| Some("[{\"name\":".to_string()));
        assert!(matches!(kaputt, Err(SammelFehler::Zugaenge(_))));

        let ohne_feld = Sammlung::aus_zugaengen(vec![zugang("dev", &[])]);
        assert!(matches!(ohne_feld, Err(SammelFehler::System(_))));
    }

    #[test]
    fn ohne_umgebung_ist_die_sammlung_leer() {
        let leer = Sammlung::mit_umgebung(|_| None).unwrap();
        assert!(leer.ist_leer());
    }
}
