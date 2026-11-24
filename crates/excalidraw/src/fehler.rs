// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Fehler des Werkzeugs.
//!
//! Jede Meldung ist für einen Agenten gedacht: sie sagt, was falsch ist und was
//! er stattdessen tun soll. Zugangsdaten kommen in diesem Werkzeug nicht vor.

use quiver_diagramm::{AblageFehler, ModellFehler, ZeichenFehler};

/// Was bei einem Aufruf schiefgehen kann.
#[derive(Debug, thiserror::Error)]
pub enum Fehler {
    /// Der Weg zu Dateien (Arbeitsordner, Pfad, Stand, Sperre).
    #[error(transparent)]
    Ablage(#[from] AblageFehler),

    /// Der Text ist kein JSON.
    #[error("Der Text ist kein gültiges JSON: {0}")]
    Json(String),

    /// Die Datei verletzt die Regeln des Formats.
    #[error("{0}")]
    Ungueltig(String),

    /// Es gibt kein Element mit dieser Id.
    #[error("Element „{0}“ gibt es nicht – bitte mit `excalidraw_lesen` die Ids ansehen")]
    ElementFehlt(String),

    /// Die Id ist schon vergeben.
    #[error("Id „{0}“ gibt es in dieser Datei schon – bitte eine andere wählen")]
    IdVergeben(String),

    /// Das Modell (Knoten, Kanten, Gruppen) ist nicht gültig.
    #[error(transparent)]
    Modell(#[from] ModellFehler),

    /// Aus dem eigenen Formenmodell wurde kein Bild.
    #[error("Das Bild ließ sich nicht zeichnen: {0}")]
    Zeichnen(#[from] ZeichenFehler),

    /// Eine Ausgabe soll entstehen, die kein Bild ist.
    #[error("„{0}“ ist kein Bildziel – erwartet wird .svg oder .png")]
    KeinBild(String),
}
