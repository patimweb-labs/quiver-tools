// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Microsoft Graph für quiver-m365 (SPEC.md §4).
//!
//! Der Client holt sein Token über [`m365_anmeldung::Anmeldung`]; die Modelle
//! enthalten nur die Felder, die die Tools zurückgeben (Teams: `teams`).

mod client;
mod dateien;
mod fehler;
mod kontakte;
mod modelle;
mod onenote;
mod planner;
mod teams;
mod token;

pub use client::{GraphClient, HOECHSTENS_WARTEN};
pub use dateien::{
    DateiArt, Laufwerk, LaufwerkEintrag, Liste, ListenArt, Listeneintrag, Ordner, Website,
};
pub use fehler::GraphFehler;
pub use kontakte::{Kontakt, MailAdresse, Person, Telefon};
pub use modelle::{
    Adresse, Datei, Eltern, Empfaenger, Koerper, Nachricht, Ort, Profil, Sammlung, Teilnehmer,
    TeilnehmerStatus, Termin, Zeitpunkt,
};
pub use onenote::{Abschnitt, Notizbuch, OneNoteLink, OneNoteLinks, Seite};
pub use planner::{Bucket, Plan, PlannerAufgabe, ToDoAufgabe, ToDoListe};
pub use teams::{
    Absender, Anhang, Chat, Erwaehnter, Erwaehnung, Identitaet, Kanal, KanalIdentitaet, Mitglied,
    Reaktion, Sicht, SucheAntwort, SucheAntwortBlock, Team, TeamsNachricht, Treffer,
    TrefferBehaelter, Vorschau,
};
pub use token::mandant_aus_token;
