// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Gemeinsamer Unterbau der Diagramm-Werkzeuge `quiver-excalidraw` und
//! `quiver-drawio` (Aufgabe 16, [ADR 0013](../../docs/adr/0013-diagramme-lokal.md)).
//!
//! Die Kiste kennt drei Schritte, die aufeinander aufbauen:
//!
//! 1. [`modell`] – was ein Diagramm **ist**: Knoten, Kanten, Gruppen, Richtung.
//!    Keine Koordinaten, nur Beschreibung. [`modell::Diagramm::pruefen`] prüft sie.
//! 2. [`layout`] – wo die Teile **liegen**: geschichtetes Layout, Textmaß aus der
//!    eingebetteten Schrift, keine Überlappung, gleiche Eingabe → gleiche Ausgabe.
//! 3. [`zeichnen`] – wie es **aussieht**: eigenes Formenmodell → SVG → PNG (resvg).
//!
//! Dazu kommt [`ablage`], der einzige Weg zu Dateien: ein Arbeitsordner, relative
//! Pfade, kein Ausbruch, Änderungen nur mit [`ablage::Stand`], Sicherung und
//! Dateisperre über Prozesse.
//!
//! Die Werkzeuge selbst liegen in eigenen Crates. Sie benutzen diese Kiste über
//! die hier öffentlichen Typen und Funktionen; die Formate (Excalidraw, draw.io)
//! kennt die Kiste **nicht**.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod ablage;
pub mod layout;
pub mod modell;
pub mod zeichnen;

pub use ablage::{Ablage, AblageFehler, Eintrag, Inhalt, Stand};
pub use layout::{layout, GruppenRahmen, KantenLauf, KnotenPlatz, Zeichnung};
pub use modell::{Diagramm, Farbe, Form, Gruppe, Kante, KanteArt, Knoten, ModellFehler, Richtung};
pub use zeichnen::{Punkt, Szene, ZeichenFehler};
