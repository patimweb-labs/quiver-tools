// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Gemeinsamer Kern aller quiver-tools (SPEC.md §2–§4).
//!
//! Ein Tool beschreibt sich über [`Werkzeug`]: Name, Titel, die Felder eines
//! Profils und wie man eine Verbindung testet. Der Kern liefert daraus
//! Profilspeicher, Tresor, Einrichtungsseite und MCP-Hilfen.
//!
//! Die Schnittstelle legt der Architekt fest; die Umsetzung steht in
//! `docs/aufgaben/01-kern.md`.

use std::collections::BTreeMap;

pub mod browser;
pub mod mcp;
pub mod profil;
pub mod seite;
pub mod start;
pub mod tresor;

pub use browser::{Browser, EchterBrowser, Merker};
pub use mcp::{antwort, kuerzen, zugang, OhneProfil, ProfilWahl};
pub use profil::{ProfilFehler, Profile, Stand};
pub use seite::{einrichten, einrichten_mit, einrichten_mit_profilen, Ende};
pub use start::{aufruf_aus_args, starten, Aufruf};
pub use tresor::{Schluesselbund, SpeicherTresor, Tresor, TresorFehler};

/// Art eines Feldes auf der Einrichtungsseite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeldArt {
    Text,
    /// Geheim: liegt im Schlüsselbund, kommt nie zurück an Browser, Log oder Agent.
    Passwort,
    Zahl,
    Schalter,
    Url,
    /// Feste Auswahl (Wert, Anzeige).
    Auswahl(Vec<(String, String)>),
}

/// Ein Feld eines Profils.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feld {
    /// Schlüssel, z. B. `api_key`.
    pub name: &'static str,
    /// Beschriftung auf der Seite.
    pub titel: &'static str,
    pub art: FeldArt,
    pub pflicht: bool,
    pub standard: Option<&'static str>,
    /// Kurzer Hilfetext unter dem Feld.
    pub hilfe: &'static str,
}

/// Ein gespeichertes Profil: nicht-geheime Werte plus geladene Geheimnisse.
/// `Debug` zeigt Geheimnisse nie.
#[derive(Clone, PartialEq, Eq)]
pub struct Profil {
    pub name: String,
    pub beschreibung: String,
    pub werte: BTreeMap<String, String>,
    pub geheim: BTreeMap<String, String>,
}

impl std::fmt::Debug for Profil {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Profil")
            .field("name", &self.name)
            .field("beschreibung", &self.beschreibung)
            .field("werte", &self.werte)
            .field("geheim", &self.geheim.keys().collect::<Vec<_>>())
            .finish()
    }
}

/// Was ein Tool dem Kern über sich sagt.
pub trait Werkzeug: Send + Sync + 'static {
    /// Kurzname, Teil der Pfade und des Schlüsselbund-Dienstes: `outline` → `quiver-outline`.
    fn name(&self) -> &'static str;
    /// Anzeigename, z. B. „Outline-Wiki“.
    fn titel(&self) -> &'static str;
    /// Die Felder eines Profils.
    fn felder(&self) -> Vec<Feld>;
    /// Verbindung prüfen; `Ok` mit einer kurzen Meldung („Angemeldet als …“).
    /// Fehlertexte dürfen keine Geheimnisse enthalten.
    fn testen(
        &self,
        profil: &Profil,
    ) -> impl std::future::Future<Output = anyhow::Result<String>> + Send;
}

/// Text, den jedes Tool liefert, wenn noch kein Profil existiert (SPEC §2).
pub fn hinweis_einrichten(titel: &str) -> String {
    format!(
        "Noch kein Zugang eingerichtet – bitte im quiver-Fenster unter Tools → {titel} auf „Einrichten“ klicken."
    )
}
