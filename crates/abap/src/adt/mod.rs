// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der ADT-Client: HTTP, CSRF, Cookies und XML (Aufgaben 07 und 08).
//!
//! | Modul | Aufgabe |
//! |---|---|
//! | [`client`] | ein Gespräch je System: Basic-Auth, CSRF, Cookies, 401, `stateful` |
//! | [`sitzung`] | die zustandsbehaftete Sitzung je System samt Lock-Tabelle |
//! | [`xml`] | ein kleiner Baum aus XML, Namensräume fallen weg |
//! | [`daten`] | reine Leser: XML-Text → Daten der Werkzeuge |
//! | [`fehler`] | ADT-Ausnahme, HTTP, 401, abgelaufene Sitzung – nie mit Geheimnis |
//!
//! Teil 1 (alle lesenden Werkzeuge) arbeitet zustandslos. Teil 2 (Sperren,
//! Schreiben, Aktivieren, Anlegen, Textelemente) läuft über [`sitzung`], weil
//! ein Lock-Handle nur in der Sitzung gilt, die es geholt hat.

pub mod client;
pub mod daten;
pub mod fehler;
pub mod sitzung;
pub mod xml;

pub use client::{Anfrage, Antwort, Client, Methode};
pub use fehler::AdtFehler;
pub use sitzung::{Sitzung, Sitzungen, FREIGABE_FRIST};
