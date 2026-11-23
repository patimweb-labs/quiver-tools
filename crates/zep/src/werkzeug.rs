// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Das Werkzeug „ZEP“ (Aufgabe 06).
//!
//! `quiver-zep` baut den offiziellen ZEP-MCP **nicht** nach: es reicht dessen
//! Werkzeuge durch und hält die Anmeldung am Leben. Der User trägt Adresse,
//! Benutzer und Passwort im quiver-Fenster ein; quiver setzt sie als Umgebung
//! (`ZEP_URL`, `ZEP_BENUTZER`, `ZEP_PASSWORT`).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use quiver_tools_kern::{Feld, FeldArt, Profil, Werkzeug};

use crate::anmeldung::Anmeldung;
use crate::browser::{Browser, KeinBrowser};

/// Kurzname: Teil der Umgebungsvariablen (`ZEP_…`) und der Werkzeugnamen.
pub const NAME: &str = "zep";
/// Anzeigename im quiver-Fenster und in MCP.
pub const TITEL: &str = "ZEP";
/// Feldname der MCP-Adresse (`ZEP_URL`).
pub const ADRESSE: &str = "url";
/// Feldname des Benutzers (`ZEP_BENUTZER`).
pub const BENUTZER: &str = "benutzer";
/// Feldname des Passworts (`ZEP_PASSWORT`, geheim).
pub const PASSWORT: &str = "passwort";

/// Der Standard für den Takt der Hintergrund-Erneuerung.
pub const TAKT: Duration = Duration::from_secs(60);
/// So lange wartet der Notfall über den Browser auf die Rückleitung.
pub const WARTEZEIT: Duration = Duration::from_secs(180);

/// Das ZEP-Werkzeug.
#[derive(Debug, Clone, Copy, Default)]
pub struct Zep;

impl Werkzeug for Zep {
    fn name(&self) -> &'static str {
        NAME
    }

    fn titel(&self) -> &'static str {
        TITEL
    }

    fn beschreibung(&self) -> &'static str {
        "ZEP über den offiziellen ZEP-MCP: Projekte, Zeiten und Berichte, dauerhaft angemeldet."
    }

    fn felder(&self) -> Vec<Feld> {
        vec![
            Feld {
                name: ADRESSE,
                titel: "MCP-Adresse",
                art: FeldArt::Url,
                pflicht: true,
                standard: None,
                hilfe: "z. B. https://www.zep-online.de/zepneoimpulse/next/mcp",
                link: None,
            },
            Feld {
                name: BENUTZER,
                titel: "Benutzername",
                art: FeldArt::Text,
                pflicht: true,
                standard: None,
                hilfe: "Ihr ZEP-Benutzername (kein Geheimnis, wird nicht gepusht)",
                link: None,
            },
            Feld {
                name: PASSWORT,
                titel: "Passwort",
                art: FeldArt::Passwort,
                pflicht: true,
                standard: None,
                hilfe: "Ihr ZEP-Passwort; liegt in der Tresor-Datei und geht nie an einen Agenten",
                link: None,
            },
        ]
    }

    async fn testen(&self, profil: &Profil) -> anyhow::Result<String> {
        let konfig = ZepKonfig::aus_profil(profil)?;
        let benutzer = konfig.benutzer.clone();
        let anmeldung = Arc::new(Anmeldung::neu(
            konfig,
            crate::tresor::aus_umgebung(),
            Arc::new(KeinBrowser::neu()),
        ));
        let vermittler = crate::vermittler::Vermittler::neu(Arc::clone(&anmeldung));
        let werkzeuge = vermittler
            .werkzeuge()
            .await
            .map_err(|fehler| anyhow::anyhow!(fehler.to_string()))?;
        Ok(format!(
            "Angemeldet als {benutzer} – der ZEP-MCP meldet {} Werkzeug(e).",
            werkzeuge.len()
        ))
    }
}

/// Alles, was das Werkzeug über seinen Zugang wissen muss.
#[derive(Clone)]
pub struct ZepKonfig {
    /// Die MCP-Adresse, z. B. `https://www.zep-online.de/<mandant>/next/mcp`.
    pub url: String,
    /// ZEP-Benutzername (kein Geheimnis, aber persönlich).
    pub benutzer: String,
    /// ZEP-Passwort. `Debug` zeigt es nie.
    pub passwort: String,
    /// Ordner für den Sperrzustand der Routine.
    pub zustand: PathBuf,
    /// Takt der Hintergrund-Erneuerung.
    pub takt: Duration,
    /// Wie lange der Notfall über den Browser wartet.
    pub wartezeit: Duration,
}

impl std::fmt::Debug for ZepKonfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ZepKonfig")
            .field("url", &self.url)
            .field("benutzer", &self.benutzer)
            .field("passwort", &"<geheim>")
            .field("zustand", &self.zustand)
            .field("takt", &self.takt)
            .field("wartezeit", &self.wartezeit)
            .finish()
    }
}

impl ZepKonfig {
    /// Die Angaben aus dem einen Zugang (Umgebung), mit den Standardwerten.
    pub fn aus_profil(profil: &Profil) -> anyhow::Result<ZepKonfig> {
        ZepKonfig::aus_profil_mit(profil, zustandsordner(), TAKT, WARTEZEIT)
    }

    /// Wie [`ZepKonfig::aus_profil`], aber mit eigenem Zustandsordner und
    /// eigenem Takt (Tests, betriebliche Sonderfälle).
    pub fn aus_profil_mit(
        profil: &Profil,
        zustand: PathBuf,
        takt: Duration,
        wartezeit: Duration,
    ) -> anyhow::Result<ZepKonfig> {
        let url = pflichtwert(profil, ADRESSE, "MCP-Adresse")?;
        let benutzer = pflichtwert(profil, BENUTZER, "Benutzername")?;
        let passwort = profil
            .geheim
            .get(PASSWORT)
            .map(String::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        if passwort.is_empty() {
            anyhow::bail!("Im Zugang fehlt das ZEP-Passwort.");
        }
        Ok(ZepKonfig {
            url,
            benutzer,
            passwort,
            zustand,
            takt,
            wartezeit,
        })
    }
}

/// Ein Pflichtwert aus den offenen Feldern – der Fehler nennt nur den Feldnamen.
fn pflichtwert(profil: &Profil, feld: &str, titel: &str) -> anyhow::Result<String> {
    let wert = profil
        .werte
        .get(feld)
        .map(String::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if wert.is_empty() {
        anyhow::bail!("Im Zugang fehlt „{titel}“.");
    }
    Ok(wert)
}

/// Wo der Sperrzustand liegt: `ZEP_ZUSTAND`, sonst im Zustandsordner des Users.
///
/// Plattformneutral gelöst: kein `cfg(target_os)`, sondern die üblichen
/// Umgebungsvariablen, zuletzt das temporäre Verzeichnis.
pub fn zustandsordner() -> PathBuf {
    if let Some(pfad) = std::env::var_os("ZEP_ZUSTAND") {
        return PathBuf::from(pfad);
    }
    let basis = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|heim| PathBuf::from(heim).join(".local/state")))
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
        .unwrap_or_else(std::env::temp_dir);
    basis.join("quiver-zep")
}

/// Der Anmeldezustand für dieses Werkzeug: `None`, wenn der Zugang fehlt.
///
/// Ein fehlender Zugang ist kein Fehler – der Kern antwortet dann mit dem
/// Hinweis auf das quiver-Fenster (SPEC §2).
pub fn anmeldung_aus_zugang(
    zugang: &quiver_tools_kern::Zugang,
    tresor: Arc<dyn crate::tresor::Tresor>,
    browser: Arc<dyn Browser>,
) -> Option<Anmeldung> {
    let profil = zugang.profil().ok()?;
    let konfig = ZepKonfig::aus_profil(&profil).ok()?;
    Some(Anmeldung::neu(konfig, tresor, browser))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profil() -> Profil {
        let mut profil = Profil::leer();
        profil.werte.insert(
            ADRESSE.to_string(),
            "https://www.zep-online.de/zepneoimpulse/next/mcp".to_string(),
        );
        profil.werte.insert(BENUTZER.to_string(), "ada".to_string());
        profil
            .geheim
            .insert(PASSWORT.to_string(), "streng-geheim-4711".to_string());
        profil
    }

    #[test]
    fn das_werkzeug_nennt_drei_felder_und_ein_geheimes() {
        let werkzeug = Zep;
        assert_eq!(werkzeug.name(), "zep");
        assert_eq!(werkzeug.titel(), "ZEP");
        assert!(werkzeug.beschreibung().contains("ZEP"));
        let felder = werkzeug.felder();
        assert_eq!(felder.len(), 3);
        assert_eq!(felder[0].name, "url");
        assert_eq!(felder[0].art, FeldArt::Url);
        assert!(felder[0].pflicht);
        assert_eq!(felder[1].name, "benutzer");
        assert_eq!(felder[1].art, FeldArt::Text);
        assert!(felder[1].pflicht);
        assert_eq!(felder[2].name, "passwort");
        assert_eq!(felder[2].art, FeldArt::Passwort);
        assert!(felder[2].pflicht);
    }

    #[test]
    fn die_konfiguration_kommt_aus_dem_profil() {
        let konfig = ZepKonfig::aus_profil(&profil()).unwrap();
        assert_eq!(
            konfig.url,
            "https://www.zep-online.de/zepneoimpulse/next/mcp"
        );
        assert_eq!(konfig.benutzer, "ada");
        assert_eq!(konfig.passwort, "streng-geheim-4711");
    }

    #[test]
    fn ein_fehlendes_feld_wird_ohne_wert_gemeldet() {
        let mut ohne_url = profil();
        ohne_url.werte.remove(ADRESSE);
        let fehler = ZepKonfig::aus_profil(&ohne_url).unwrap_err().to_string();
        assert!(fehler.contains("MCP-Adresse"), "{fehler}");

        let mut ohne_passwort = profil();
        ohne_passwort.geheim.remove(PASSWORT);
        let fehler = ZepKonfig::aus_profil(&ohne_passwort)
            .unwrap_err()
            .to_string();
        assert!(fehler.contains("Passwort"), "{fehler}");
        assert!(!fehler.contains("streng-geheim-4711"));
    }

    #[test]
    fn debug_zeigt_das_passwort_nicht() {
        let text = format!("{:?}", ZepKonfig::aus_profil(&profil()).unwrap());
        assert!(text.contains("ada"));
        assert!(!text.contains("streng-geheim-4711"));
    }

    #[test]
    fn der_zustandsordner_ist_ein_pfad_mit_namen() {
        let ordner = zustandsordner();
        assert!(ordner.ends_with("quiver-zep"), "{ordner:?}");
    }
}
