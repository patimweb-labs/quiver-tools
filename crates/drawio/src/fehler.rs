// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Was in diesem Werkzeug schiefgehen kann.
//!
//! Jede Meldung ist für einen Agenten geschrieben: sie sagt, was nicht stimmt
//! und was er stattdessen tun soll. Keine Meldung enthält Zugangsdaten – das
//! Werkzeug hat keine.

use crate::xml::XmlFehler;

/// Ein Fehler im Format, in der Struktur oder im Modell einer Datei.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FormatFehler {
    /// Das XML ließ sich nicht lesen (DTD, Entität, Tiefe, kaputtes Tag).
    #[error("{0}")]
    Xml(String),
    /// Der Aufbau passt nicht zu draw.io.
    #[error("{0}")]
    Struktur(String),
    /// Eine komprimierte Seite ließ sich nicht entpacken.
    #[error("{0}")]
    Komprimierung(String),
    /// Die gewünschte Seite gibt es nicht oder der Name ist mehrdeutig.
    #[error("{0}")]
    Seite(String),
    /// Das Modell (Knoten, Kanten, Gruppen) passt nicht.
    #[error("{0}")]
    Modell(String),
    /// Alles andere (zu groß, unbekannte Form, …).
    #[error("{0}")]
    Sonstiges(String),
}

impl FormatFehler {
    /// Kurzform für Stellen, die eine Zeichenkette erwarten.
    pub fn neu(text: impl Into<String>) -> FormatFehler {
        FormatFehler::Sonstiges(text.into())
    }
}

impl From<XmlFehler> for FormatFehler {
    fn from(fehler: XmlFehler) -> FormatFehler {
        FormatFehler::Xml(format!("XML: {fehler}"))
    }
}
