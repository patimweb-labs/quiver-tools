// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der ADT-Client: HTTP, CSRF, Cookies und XML (Aufgabe 07).
//!
//! | Modul | Aufgabe |
//! |---|---|
//! | [`client`] | ein Gespräch je System: Basic-Auth, CSRF, Cookies, 401 |
//! | [`xml`] | ein kleiner Baum aus XML, Namensräume fallen weg |
//! | [`daten`] | reine Leser: XML-Text → Daten der Werkzeuge |
//! | [`fehler`] | ADT-Ausnahme und HTTP-Fehler, nie mit Geheimnis |
//!
//! Teil 1 ist **nur lesend**; der Client arbeitet zustandslos. Teil 2
//! (lock/write/activate) braucht `X-sap-adt-sessiontype: stateful` und eine
//! Sitzung – dafür ist hier noch nichts vorbereitet (siehe ADR 0005).

pub mod client;
pub mod daten;
pub mod fehler;
pub mod xml;

pub use client::{Anfrage, Antwort, Client, Methode};
pub use fehler::AdtFehler;
