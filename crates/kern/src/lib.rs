// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Gemeinsamer Kern aller quiver-tools (SPEC.md §2–§4).
//!
//! Ein Tool beschreibt sich über [`Werkzeug`]: Name, Titel, Beschreibung, die
//! Felder seines Zugangs und wie man eine Verbindung testet. Der Kern liest
//! daraus den Zugang aus der Umgebung (quiver füllt die Felder des
//! Bibliothekseintrags), baut den Bibliothekseintrag für mcp-quiver und liefert
//! die MCP-Hilfen.

pub mod eintrag;
pub mod mcp;
pub mod profil;
pub mod start;
pub mod umgebung;

pub use eintrag::{eintrag, Eintrag, Herkunft, Laufzeit, Quelle};
pub use mcp::{antwort, kuerzen, zugang, OhneProfil};
pub use profil::{Profil, ProfilFehler};
pub use start::{aufruf_aus_args, starten, Aufruf};
pub use umgebung::Zugang;

/// Art eines Feldes im quiver-Fenster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeldArt {
    Text,
    /// Geheim: kommt als Umgebungsvariable, aber nie zurück an Agent, Log oder Datei.
    Passwort,
    Zahl,
    Schalter,
    Url,
    /// Feste Auswahl (Wert, Anzeige).
    Auswahl(Vec<(String, String)>),
}

/// Ein Feld des Zugangs, wie der User es im quiver-Fenster sieht.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feld {
    /// Schlüssel, z. B. `api_key`; Teil der Umgebungsvariablen `<NAME>_<FELD>`.
    pub name: &'static str,
    /// Beschriftung im quiver-Fenster.
    pub titel: &'static str,
    pub art: FeldArt,
    pub pflicht: bool,
    pub standard: Option<&'static str>,
    /// Kurzer Hilfetext unter dem Feld.
    pub hilfe: &'static str,
    /// Optionaler Hilfe-Link (Text, Adresse). Die Adresse darf `${feld}` anderer Felder enthalten;
    /// quiver zeigt ihn als Knopf neben dem Feld und öffnet ihn im Browser.
    pub link: Option<(&'static str, &'static str)>,
}

/// Was ein Tool dem Kern über sich sagt.
pub trait Werkzeug: Send + Sync + 'static {
    /// Kurzname, Teil der Umgebungsvariablen und der Werkzeugnamen: `outline` → `quiver-outline`.
    fn name(&self) -> &'static str;
    /// Anzeigename, z. B. „Outline-Wiki“.
    fn titel(&self) -> &'static str;
    /// Ein Satz, der im quiver-Fenster unter dem Titel steht.
    fn beschreibung(&self) -> &'static str;
    /// Die Felder des Zugangs.
    fn felder(&self) -> Vec<Feld>;
    /// Verbindung prüfen; `Ok` mit einer kurzen Meldung („Angemeldet als …“).
    /// Fehlertexte dürfen keine Geheimnisse enthalten.
    fn testen(
        &self,
        profil: &Profil,
    ) -> impl std::future::Future<Output = anyhow::Result<String>> + Send;
}

/// Text, den jedes Tool liefert, wenn der Zugang fehlt (SPEC §2).
pub fn hinweis_zugang(titel: &str) -> String {
    format!("Zugang fehlt – bitte im quiver-Fenster unter Tools → {titel} die Angaben eintragen.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn der_hinweis_nennt_das_quiver_fenster_und_den_titel() {
        let text = hinweis_zugang("Outline-Wiki");
        assert_eq!(
            text,
            "Zugang fehlt – bitte im quiver-Fenster unter Tools → Outline-Wiki die Angaben eintragen."
        );
    }
}
