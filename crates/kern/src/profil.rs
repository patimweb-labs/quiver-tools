// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der Profilspeicher eines Werkzeugs (SPEC §3).
//!
//! [`Profile`] ist die Menge der Profile **eines** Werkzeugs: die nicht-geheimen
//! Werte liegen in `<konfig>/quiver-tools/<werkzeug>/profile.toml`, die
//! Geheimnisse im [`Tresor`]. Die Datei nennt nie einen geheimen Wert.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::tresor::{Tresor, TresorFehler};
use crate::{Feld, FeldArt, Profil, Werkzeug};

/// Dateiname im Ordner des Werkzeugs.
pub const DATEI: &str = "profile.toml";

/// Ordner, unter dem alle Werkzeuge ihre Profile ablegen (SPEC §3).
///
/// `QUIVER_TOOLS_DIR` verlegt ihn (Tests, mitgenommene Installationen), sonst
/// `<dirs::config_dir()>/quiver-tools`.
pub fn basis_ordner() -> PathBuf {
    match std::env::var_os("QUIVER_TOOLS_DIR") {
        Some(pfad) if !pfad.is_empty() => PathBuf::from(pfad),
        _ => dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("quiver-tools"),
    }
}

/// Was beim Umgang mit Profilen schiefgehen kann. Kein Fehlertext enthält ein
/// Geheimnis.
#[derive(Debug, thiserror::Error)]
pub enum ProfilFehler {
    /// Es gibt kein Profil – der Normalfall vor dem Einrichten (SPEC §2).
    #[error("Noch kein Profil eingerichtet.")]
    Keins,
    #[error("Das Profil „{0}“ gibt es nicht.")]
    Unbekannt(String),
    #[error(
        "Der Profilname „{0}“ ist ungültig: erlaubt sind 1–40 Zeichen aus a–z, 0–9, „-“ und „_“."
    )]
    UngueltigerName(String),
    #[error("Pflichtfeld fehlt: {0}")]
    Pflichtfeld(String),
    #[error("Der Wert im Feld „{0}“ ist keine Zahl.")]
    KeineZahl(String),
    #[error("Der Wert im Feld „{0}“ ist nicht zulässig.")]
    UngueltigerWert(String),
    #[error(transparent)]
    Tresor(#[from] TresorFehler),
    #[error("Die Profildatei {0} lässt sich nicht lesen oder schreiben: {1}")]
    Datei(PathBuf, String),
}

/// Stand der Profildatei: alle Profile (ohne Geheimnisse) und der Standardname.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stand {
    pub standard: Option<String>,
    pub profile: Vec<Profil>,
}

/// Die Profile eines Werkzeugs.
#[derive(Clone)]
pub struct Profile {
    werkzeug: String,
    titel: String,
    felder: Vec<Feld>,
    ordner: PathBuf,
    tresor: Arc<dyn Tresor>,
}

impl std::fmt::Debug for Profile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Profile")
            .field("werkzeug", &self.werkzeug)
            .field("ordner", &self.ordner)
            .finish()
    }
}

/// So sieht die Datei aus. Die Geheimnisse stehen hier nicht.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Datei {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    standard: Option<String>,
    #[serde(default)]
    profil: Vec<Eintrag>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Eintrag {
    name: String,
    #[serde(default)]
    beschreibung: String,
    #[serde(default)]
    werte: BTreeMap<String, String>,
}

impl Profile {
    /// Die Profile des Werkzeugs im Standardordner.
    pub fn laden<W: Werkzeug>(werkzeug: &W, tresor: Arc<dyn Tresor>) -> Self {
        Self {
            werkzeug: werkzeug.name().to_string(),
            titel: werkzeug.titel().to_string(),
            felder: werkzeug.felder(),
            ordner: basis_ordner().join(werkzeug.name()),
            tresor,
        }
    }

    /// Die Profile des Werkzeugs in einem selbst gewählten Ordner (Tests, Sonderfälle).
    pub fn im_ordner<W: Werkzeug>(werkzeug: &W, tresor: Arc<dyn Tresor>, ordner: PathBuf) -> Self {
        Self {
            werkzeug: werkzeug.name().to_string(),
            titel: werkzeug.titel().to_string(),
            felder: werkzeug.felder(),
            ordner,
            tresor,
        }
    }

    /// Kurzname des Werkzeugs, z. B. `outline`.
    pub fn werkzeug(&self) -> &str {
        &self.werkzeug
    }

    /// Anzeigename, z. B. „Outline-Wiki“.
    pub fn titel(&self) -> &str {
        &self.titel
    }

    /// Die Felder eines Profils.
    pub fn felder(&self) -> &[Feld] {
        &self.felder
    }

    /// Ordner dieses Werkzeugs.
    pub fn ordner(&self) -> &Path {
        &self.ordner
    }

    /// Dienst im Tresor: `quiver-<werkzeug>`.
    pub fn dienst(&self) -> String {
        format!("quiver-{}", self.werkzeug)
    }

    /// Schlüssel im Tresor: `<profil>/<feld>`.
    pub fn schluessel(&self, profil: &str, feld: &str) -> String {
        format!("{profil}/{feld}")
    }

    /// Alle Profile ohne Geheimnisse plus der Standardname.
    pub fn stand(&self) -> Result<Stand, ProfilFehler> {
        let datei = self.datei()?;
        Ok(Stand {
            standard: datei.standard,
            profile: datei
                .profil
                .into_iter()
                .map(|eintrag| Profil {
                    name: eintrag.name,
                    beschreibung: eintrag.beschreibung,
                    werte: eintrag.werte,
                    geheim: BTreeMap::new(),
                })
                .collect(),
        })
    }

    /// Alle Profile ohne Geheimnisse.
    pub fn alle(&self) -> Result<Vec<Profil>, ProfilFehler> {
        Ok(self.stand()?.profile)
    }

    /// Der eingestellte Standardname (falls gesetzt und vorhanden).
    pub fn standard(&self) -> Result<Option<String>, ProfilFehler> {
        let stand = self.stand()?;
        Ok(stand
            .standard
            .filter(|name| stand.profile.iter().any(|profil| &profil.name == name)))
    }

    /// Ein Profil **mit** Geheimnissen: `None` → Standard, sonst das erste.
    pub fn holen(&self, name: Option<&str>) -> Result<Profil, ProfilFehler> {
        let stand = self.stand()?;
        let gewaehlt = match name {
            Some(name) => stand
                .profile
                .iter()
                .find(|profil| profil.name == name)
                .ok_or_else(|| ProfilFehler::Unbekannt(name.to_string()))?,
            None => {
                let standard = stand
                    .standard
                    .as_ref()
                    .and_then(|name| stand.profile.iter().find(|profil| &profil.name == name));
                match standard.or_else(|| stand.profile.first()) {
                    Some(profil) => profil,
                    None => return Err(ProfilFehler::Keins),
                }
            }
        };

        let mut profil = gewaehlt.clone();
        for feld in &self.felder {
            if feld.art != FeldArt::Passwort {
                continue;
            }
            let schluessel = self.schluessel(&profil.name, feld.name);
            if let Some(wert) = self.tresor.lesen(&self.dienst(), &schluessel)? {
                profil.geheim.insert(feld.name.to_string(), wert);
            }
        }
        Ok(profil)
    }

    /// Prüft Namen und Pflichtfelder, ohne zu schreiben.
    pub fn pruefen(&self, profil: &Profil) -> Result<(), ProfilFehler> {
        if !name_gueltig(&profil.name) {
            return Err(ProfilFehler::UngueltigerName(profil.name.clone()));
        }

        let bekannt: BTreeSet<&str> = self
            .felder
            .iter()
            .map(|feld| feld.name)
            .collect::<BTreeSet<_>>();
        for schluessel in profil.werte.keys() {
            if !bekannt.contains(schluessel.as_str()) {
                return Err(ProfilFehler::Unbekannt(format!(
                    "Feld „{schluessel}“ gehört nicht zu {}",
                    self.titel
                )));
            }
        }

        for feld in &self.felder {
            if !feld.pflicht {
                continue;
            }
            let wert = match feld.art {
                FeldArt::Passwort => self
                    .tresor
                    .lesen(&self.dienst(), &self.schluessel(&profil.name, feld.name))?
                    .or_else(|| profil.geheim.get(feld.name).cloned()),
                _ => profil.werte.get(feld.name).cloned(),
            };
            let wert = wert.unwrap_or_default();
            if wert.trim().is_empty() {
                return Err(ProfilFehler::Pflichtfeld(feld.titel.to_string()));
            }
            match &feld.art {
                FeldArt::Zahl if wert.trim().parse::<f64>().is_err() => {
                    return Err(ProfilFehler::KeineZahl(feld.titel.to_string()));
                }
                FeldArt::Auswahl(moeglichkeiten)
                    if !moeglichkeiten.iter().any(|(wahl, _)| wahl == &wert)
                        && !(wert.is_empty() && !feld.pflicht) =>
                {
                    return Err(ProfilFehler::UngueltigerWert(feld.titel.to_string()));
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Ein Profil speichern: Werte in die Datei, Geheimnisse in den Tresor.
    ///
    /// Ein leeres Geheimnis lässt das gespeicherte stehen („leer = unverändert“).
    /// Das erste Profil wird Standard, wenn noch keiner gesetzt ist.
    pub fn speichern(&self, profil: &Profil) -> Result<(), ProfilFehler> {
        if !name_gueltig(&profil.name) {
            return Err(ProfilFehler::UngueltigerName(profil.name.clone()));
        }
        self.pruefen(profil)?;

        let mut datei = self.datei()?;
        let werte: BTreeMap<String, String> = profil
            .werte
            .iter()
            .filter(|(name, _)| {
                !self
                    .felder
                    .iter()
                    .any(|feld| feld.name == name.as_str() && feld.art == FeldArt::Passwort)
            })
            .map(|(name, wert)| (name.clone(), wert.clone()))
            .collect();

        let eintrag = Eintrag {
            name: profil.name.clone(),
            beschreibung: profil.beschreibung.clone(),
            werte,
        };
        match datei
            .profil
            .iter_mut()
            .find(|vorhanden| vorhanden.name == profil.name)
        {
            Some(vorhanden) => *vorhanden = eintrag,
            None => datei.profil.push(eintrag),
        }
        if datei.standard.is_none() {
            datei.standard = Some(profil.name.clone());
        }
        self.schreiben(&datei)?;

        for feld in &self.felder {
            if feld.art != FeldArt::Passwort {
                continue;
            }
            let Some(wert) = profil.geheim.get(feld.name) else {
                continue;
            };
            if wert.is_empty() {
                continue;
            }
            self.tresor.schreiben(
                &self.dienst(),
                &self.schluessel(&profil.name, feld.name),
                wert,
            )?;
        }
        Ok(())
    }

    /// Ein Profil samt seinen Geheimnissen entfernen.
    pub fn loeschen(&self, name: &str) -> Result<(), ProfilFehler> {
        let mut datei = self.datei()?;
        let laenge = datei.profil.len();
        datei.profil.retain(|eintrag| eintrag.name != name);
        if datei.profil.len() == laenge {
            return Err(ProfilFehler::Unbekannt(name.to_string()));
        }
        if datei.standard.as_deref() == Some(name) {
            datei.standard = None;
        }
        self.schreiben(&datei)?;

        for feld in &self.felder {
            if feld.art != FeldArt::Passwort {
                continue;
            }
            self.tresor
                .loeschen(&self.dienst(), &self.schluessel(name, feld.name))?;
        }
        Ok(())
    }

    /// Ein Profil zum Standard machen.
    pub fn als_standard(&self, name: &str) -> Result<(), ProfilFehler> {
        let mut datei = self.datei()?;
        if !datei.profil.iter().any(|eintrag| eintrag.name == name) {
            return Err(ProfilFehler::Unbekannt(name.to_string()));
        }
        datei.standard = Some(name.to_string());
        self.schreiben(&datei)
    }

    fn pfad(&self) -> PathBuf {
        self.ordner.join(DATEI)
    }

    fn datei(&self) -> Result<Datei, ProfilFehler> {
        let pfad = self.pfad();
        let text = match std::fs::read_to_string(&pfad) {
            Ok(text) => text,
            Err(fehler) if fehler.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Datei::default());
            }
            Err(fehler) => return Err(ProfilFehler::Datei(pfad, fehler.to_string())),
        };
        toml::from_str(&text).map_err(|fehler| ProfilFehler::Datei(pfad, fehler.to_string()))
    }

    /// Schreibt über eine Zwischendatei und benennt sie um: entweder die alte
    /// oder die neue Fassung, nie eine halbe (Datei 0600, sie nennt Hosts und
    /// Zugangsnamen).
    fn schreiben(&self, datei: &Datei) -> Result<(), ProfilFehler> {
        let ziel = self.pfad();
        let ordner = &self.ordner;
        std::fs::create_dir_all(ordner)
            .map_err(|fehler| ProfilFehler::Datei(ordner.clone(), fehler.to_string()))?;

        let text = toml::to_string_pretty(datei)
            .map_err(|fehler| ProfilFehler::Datei(ziel.clone(), fehler.to_string()))?;

        let zwischen = ordner.join(format!(".{DATEI}.{}.tmp", std::process::id()));
        {
            let mut optionen = std::fs::OpenOptions::new();
            optionen.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                optionen.mode(0o600);
            }
            let mut griff = optionen
                .open(&zwischen)
                .map_err(|fehler| ProfilFehler::Datei(zwischen.clone(), fehler.to_string()))?;
            use std::io::Write;
            griff
                .write_all(text.as_bytes())
                .and_then(|()| griff.sync_all())
                .map_err(|fehler| ProfilFehler::Datei(zwischen.clone(), fehler.to_string()))?;
        }
        std::fs::rename(&zwischen, &ziel).map_err(|fehler| {
            let _ = std::fs::remove_file(&zwischen);
            ProfilFehler::Datei(ziel.clone(), fehler.to_string())
        })
    }
}

/// Profilnamen: 1–40 Zeichen aus `a-z`, `0-9`, `-` und `_` (SPEC §3).
pub fn name_gueltig(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= 40
        && name.chars().all(|zeichen| {
            zeichen.is_ascii_lowercase()
                || zeichen.is_ascii_digit()
                || zeichen == '-'
                || zeichen == '_'
        })
}

impl Profil {
    /// Ein leeres Profil mit diesem Namen.
    pub fn leer(name: impl Into<String>) -> Self {
        Profil {
            name: name.into(),
            beschreibung: String::new(),
            werte: BTreeMap::new(),
            geheim: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tresor::{Tresor, TresorFehler};
    use crate::{Feld, FeldArt};
    use std::future::Future;
    use std::sync::Mutex;

    /// Ein eigener Tresor je Test.
    ///
    /// [`crate::SpeicherTresor`] ist **ein** globaler Speicher für den ganzen
    /// Prozess; parallele Tests würden einander die Geheimnisse leeren und
    /// gelegentlich falsche Ergebnisse sehen. Darum hat jeder Test hier seinen
    /// eigenen.
    #[derive(Default)]
    struct EigenTresor {
        werte: Mutex<BTreeMap<(String, String), String>>,
    }

    impl EigenTresor {
        fn griff(&self) -> std::sync::MutexGuard<'_, BTreeMap<(String, String), String>> {
            self.werte.lock().unwrap_or_else(|gift| gift.into_inner())
        }
    }

    impl Tresor for EigenTresor {
        fn lesen(&self, dienst: &str, schluessel: &str) -> Result<Option<String>, TresorFehler> {
            Ok(self
                .griff()
                .get(&(dienst.to_string(), schluessel.to_string()))
                .cloned())
        }

        fn schreiben(
            &self,
            dienst: &str,
            schluessel: &str,
            wert: &str,
        ) -> Result<(), TresorFehler> {
            self.griff().insert(
                (dienst.to_string(), schluessel.to_string()),
                wert.to_string(),
            );
            Ok(())
        }

        fn loeschen(&self, dienst: &str, schluessel: &str) -> Result<(), TresorFehler> {
            self.griff()
                .remove(&(dienst.to_string(), schluessel.to_string()));
            Ok(())
        }
    }

    struct Attrappe;

    impl Werkzeug for Attrappe {
        fn name(&self) -> &'static str {
            "attrappe"
        }
        fn titel(&self) -> &'static str {
            "Attrappe"
        }
        fn felder(&self) -> Vec<Feld> {
            vec![
                Feld {
                    name: "url",
                    titel: "Adresse",
                    art: FeldArt::Url,
                    pflicht: true,
                    standard: None,
                    hilfe: "Zum Beispiel https://example.test",
                },
                Feld {
                    name: "token",
                    titel: "Token",
                    art: FeldArt::Passwort,
                    pflicht: true,
                    standard: None,
                    hilfe: "Steht in den Einstellungen.",
                },
            ]
        }
        fn testen(&self, _profil: &Profil) -> impl Future<Output = anyhow::Result<String>> + Send {
            std::future::ready(Ok("Angemeldet.".to_string()))
        }
    }

    /// Kein echtes Geheimnis: nur ein Wert, der in keinem Text auftauchen darf.
    const GEHEIM: &str = "nur-ein-testwert";

    fn aufbau() -> (tempfile::TempDir, Profile) {
        let ordner = tempfile::tempdir().unwrap();
        let profile = Profile::im_ordner(
            &Attrappe,
            Arc::new(EigenTresor::default()),
            ordner.path().to_path_buf(),
        );
        (ordner, profile)
    }

    fn vollstaendig(name: &str) -> Profil {
        let mut profil = Profil::leer(name);
        profil.beschreibung = "Mein Zugang".to_string();
        profil
            .werte
            .insert("url".to_string(), "https://example.test".to_string());
        profil
            .geheim
            .insert("token".to_string(), GEHEIM.to_string());
        profil
    }

    #[test]
    fn anlegen_laden_standard_loeschen() {
        let (_ordner, profile) = aufbau();
        assert_eq!(
            profile.holen(None).unwrap_err().to_string(),
            "Noch kein Profil eingerichtet."
        );

        profile.speichern(&vollstaendig("arbeit")).unwrap();
        profile.speichern(&vollstaendig("privat")).unwrap();

        assert_eq!(profile.alle().unwrap().len(), 2);
        // Das erste Profil wird Standard.
        assert_eq!(profile.standard().unwrap().as_deref(), Some("arbeit"));

        let geladen = profile.holen(None).unwrap();
        assert_eq!(geladen.name, "arbeit");
        assert_eq!(geladen.werte["url"], "https://example.test");
        assert_eq!(geladen.geheim["token"], GEHEIM);

        let gewaehlt = profile.holen(Some("privat")).unwrap();
        assert_eq!(gewaehlt.name, "privat");
        assert!(profile.holen(Some("fehlt")).is_err());

        profile.als_standard("privat").unwrap();
        assert_eq!(profile.holen(None).unwrap().name, "privat");

        profile.loeschen("privat").unwrap();
        assert_eq!(profile.alle().unwrap().len(), 1);
        assert_eq!(profile.standard().unwrap(), None);
        // Geheimnis ist weg.
        assert!(profile
            .tresor
            .lesen(&profile.dienst(), "privat/token")
            .unwrap()
            .is_none());
        // Nach dem Löschen des Standards greift wieder das erste Profil.
        assert_eq!(profile.holen(None).unwrap().name, "arbeit");
    }

    #[test]
    fn toml_nennt_kein_geheimnis() {
        let (ordner, profile) = aufbau();
        profile.speichern(&vollstaendig("arbeit")).unwrap();

        let text = std::fs::read_to_string(ordner.path().join(DATEI)).unwrap();
        assert!(
            !text.contains(GEHEIM),
            "Geheimnis steht in der Datei: {text}"
        );
        assert!(text.contains("https://example.test"));
        assert!(text.contains("standard = \"arbeit\""));
        assert!(text.contains("[[profil]]"));

        // Das Geheimnis liegt im Tresor.
        assert_eq!(
            profile
                .tresor
                .lesen(&profile.dienst(), "arbeit/token")
                .unwrap()
                .as_deref(),
            Some(GEHEIM)
        );
    }

    #[test]
    fn leeres_geheimnis_laesst_das_gespeicherte_stehen() {
        let (_ordner, profile) = aufbau();
        profile.speichern(&vollstaendig("arbeit")).unwrap();

        let mut ohne = vollstaendig("arbeit");
        ohne.geheim.clear();
        ohne.beschreibung = "Neu".to_string();
        profile.speichern(&ohne).unwrap();

        let geladen = profile.holen(Some("arbeit")).unwrap();
        assert_eq!(geladen.beschreibung, "Neu");
        assert_eq!(geladen.geheim["token"], GEHEIM);
    }

    #[test]
    fn pflichtfeld_fehlt() {
        let (_ordner, profile) = aufbau();
        let mut unvollstaendig = Profil::leer("arbeit");
        unvollstaendig
            .werte
            .insert("url".to_string(), " ".to_string());
        let fehler = profile.speichern(&unvollstaendig).unwrap_err();
        assert_eq!(fehler.to_string(), "Pflichtfeld fehlt: Adresse");
        assert!(profile.alle().unwrap().is_empty());

        // Das geheime Pflichtfeld fehlt ebenfalls.
        let mut ohne_token = Profil::leer("arbeit");
        ohne_token
            .werte
            .insert("url".to_string(), "https://example.test".to_string());
        assert_eq!(
            profile.speichern(&ohne_token).unwrap_err().to_string(),
            "Pflichtfeld fehlt: Token"
        );
    }

    #[test]
    fn namensregeln() {
        assert!(name_gueltig("arbeit"));
        assert!(name_gueltig("kunde-a_1"));
        assert!(name_gueltig(&"a".repeat(40)));
        assert!(!name_gueltig(""));
        assert!(!name_gueltig(&"a".repeat(41)));
        assert!(!name_gueltig("Arbeit"));
        assert!(!name_gueltig("kunde a"));
        assert!(!name_gueltig("kunde.a"));
        assert!(!name_gueltig("küste"));

        let (_ordner, profile) = aufbau();
        assert!(matches!(
            profile.speichern(&vollstaendig("Arbeit")).unwrap_err(),
            ProfilFehler::UngueltigerName(_)
        ));
    }

    #[test]
    fn auswahl_prueft_ihre_moeglichkeiten() {
        struct MitAuswahl;

        impl Werkzeug for MitAuswahl {
            fn name(&self) -> &'static str {
                "mitauswahl"
            }
            fn titel(&self) -> &'static str {
                "Mit Auswahl"
            }
            fn felder(&self) -> Vec<Feld> {
                vec![Feld {
                    name: "umgebung",
                    titel: "Umgebung",
                    art: FeldArt::Auswahl(vec![
                        ("test".to_string(), "Test".to_string()),
                        ("produktion".to_string(), "Produktion".to_string()),
                    ]),
                    pflicht: true,
                    standard: Some("test"),
                    hilfe: "",
                }]
            }
            fn testen(
                &self,
                _profil: &Profil,
            ) -> impl Future<Output = anyhow::Result<String>> + Send {
                std::future::ready(Ok("ok".to_string()))
            }
        }

        let ordner = tempfile::tempdir().unwrap();
        let profile = Profile::im_ordner(
            &MitAuswahl,
            Arc::new(EigenTresor::default()),
            ordner.path().to_path_buf(),
        );

        let mut gut = Profil::leer("arbeit");
        gut.werte
            .insert("umgebung".to_string(), "produktion".to_string());
        profile.speichern(&gut).unwrap();

        let mut schlecht = Profil::leer("test");
        schlecht
            .werte
            .insert("umgebung".to_string(), "irgendwas".to_string());
        assert_eq!(
            profile.speichern(&schlecht).unwrap_err().to_string(),
            "Der Wert im Feld „Umgebung“ ist nicht zulässig."
        );
    }

    #[test]
    fn fremdes_feld_wird_abgewiesen() {
        let (_ordner, profile) = aufbau();
        let mut profil = vollstaendig("arbeit");
        profil
            .werte
            .insert("unbekannt".to_string(), "x".to_string());
        assert!(profile.speichern(&profil).is_err());
    }

    #[test]
    fn schreiben_ist_atomar_und_privat() {
        let (ordner, profile) = aufbau();
        profile.speichern(&vollstaendig("arbeit")).unwrap();

        // Keine Zwischendatei zurückgelassen.
        let uebrig: Vec<String> = std::fs::read_dir(ordner.path())
            .unwrap()
            .filter_map(|eintrag| eintrag.ok())
            .map(|eintrag| eintrag.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(uebrig, vec![DATEI.to_string()]);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let rechte = std::fs::metadata(ordner.path().join(DATEI))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(rechte, 0o600);
        }
    }

    #[test]
    fn kaputte_datei_meldet_sich() {
        let (ordner, profile) = aufbau();
        std::fs::write(ordner.path().join(DATEI), "das ist kein toml = = =").unwrap();
        assert!(matches!(
            profile.alle().unwrap_err(),
            ProfilFehler::Datei(..)
        ));
    }
}
