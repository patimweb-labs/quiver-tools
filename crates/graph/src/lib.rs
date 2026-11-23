// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Microsoft Graph für quiver-m365 (SPEC.md §4).
//!
//! Der Client holt sein Token über [`m365_anmeldung::Anmeldung`]; die Modelle
//! enthalten nur die Felder, die die Tools zurückgeben.

mod client;
mod fehler;
mod modelle;

pub use client::{GraphClient, HOECHSTENS_WARTEN};
pub use fehler::GraphFehler;
pub use modelle::{
    Adresse, Datei, Eltern, Empfaenger, Koerper, Nachricht, Ort, Profil, Sammlung, Teilnehmer,
    TeilnehmerStatus, Termin, Zeitpunkt,
};
