// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die zustandsbehaftete Sitzung je System (Aufgabe 08).
//!
//! Ein Lock-Handle gilt in SAP **nur** in der Sitzung, die es geholt hat
//! (Cookie `SAP_SESSIONID_*` plus Kopf `X-sap-adt-sessiontype: stateful`).
//! Deshalb hält `quiver-abap` je System **eine** [`Sitzung`] im Speicher:
//!
//! - einen [`Client`] mit `stateful`, also Cookies und CSRF-Token dieser Sitzung,
//! - eine Tabelle `objectUrl → lockHandle` ([`Sitzung::merken`]),
//! - und die Nachricht, dass die Sitzung abgelaufen ist.
//!
//! Lesende Werkzeuge arbeiten weiter zustandslos; sie bekommen nie eine
//! Sitzung zu sehen.
//!
//! Zwei Regeln sind hier zu Hause:
//!
//! 1. **Kein stilles Wiederholen.** Läuft die Sitzung ab (400/401 mit „Session
//!    timed out“), wird sie samt Lock-Tabelle verworfen und der Aufruf endet mit
//!    [`AdtFehler::SitzungAbgelaufen`]. Ein Schreibvorgang wird nie
//!    stillschweigend nachgeschoben.
//! 2. **Locks werden freigegeben.** Beim Ende des Prozesses gibt
//!    [`Sitzung::alle_entsperren`] alle bekannten Locks frei, höchstens 5 s;
//!    Fehler gehen nur ins Log.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use crate::adt::client::{Anfrage, Antwort, Client, Methode};
use crate::adt::daten::{self, Sperre};
use crate::adt::fehler::AdtFehler;
use crate::systeme::System;

/// So lange dürfen Locks beim Beenden des Prozesses höchstens freigegeben werden.
pub const FREIGABE_FRIST: Duration = Duration::from_secs(5);

/// Der `Accept`-Kopf, mit dem ADT auf `?_action=LOCK` antwortet.
const SPERR_ANNAHME: &str =
    "application/*,application/vnd.sap.as+xml;charset=UTF-8;dataname=com.sap.adt.lock.result";

fn griff<T>(sperre: &Mutex<T>) -> MutexGuard<'_, T> {
    sperre.lock().unwrap_or_else(|gift| gift.into_inner())
}

/// Ein Sperrvermerk dieser Sitzung.
#[derive(Debug, Clone)]
struct Vermerk {
    handle: String,
    /// Der Objektname, wie SAP ihn nennt (`ZCL_DEMO`) – für die Prüfung eines
    /// Handles, bevor eine Anfrage hinausgeht.
    name: String,
    url: String,
}

/// Eine stateful Sitzung mit ihren Locks.
pub struct Sitzung {
    system: System,
    client: Client,
    /// `objectUrl → Sperrvermerk` – nur Locks **dieser** Sitzung.
    locks: Mutex<BTreeMap<String, Vermerk>>,
    /// Hat SAP die Sitzung beendet? Dann ist auch die Lock-Tabelle wertlos.
    abgelaufen: Mutex<bool>,
}

impl std::fmt::Debug for Sitzung {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sitzung")
            .field("system", &self.system.name)
            .field("locks", &griff(&self.locks).len())
            .field("abgelaufen", &self.ist_abgelaufen())
            .finish()
    }
}

impl Sitzung {
    /// Eine neue Sitzung für dieses System. Sie meldet sich beim ersten Aufruf an.
    pub fn neu(system: &System) -> Result<Sitzung, AdtFehler> {
        Ok(Sitzung {
            system: system.clone(),
            client: Client::neu_stateful(system)?,
            locks: Mutex::new(BTreeMap::new()),
            abgelaufen: Mutex::new(false),
        })
    }

    pub fn system(&self) -> &System {
        &self.system
    }

    /// Ist die Sitzung abgelaufen (und damit die Lock-Tabelle verloren)?
    pub fn ist_abgelaufen(&self) -> bool {
        *griff(&self.abgelaufen)
    }

    /// Der bekannte Handle zu diesem Objekt, falls diese Sitzung es gesperrt hat.
    ///
    /// Geprüft wird **ohne** Anfrage an SAP – über den Objektnamen (wie der
    /// Agent ihn übergibt) oder über die Objekt-URL. Darum lässt sich ein
    /// fremder Handle ablehnen, bevor irgendetwas hinausgeht.
    pub fn handle_fuer(&self, name: Option<&str>, objekt_url: Option<&str>) -> Option<String> {
        let url = objekt_url.map(str::trim).filter(|url| !url.is_empty());
        let name = name
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_uppercase);
        for vermerk in griff(&self.locks).values() {
            let passt_url = url.is_some_and(|url| vermerk.url == url);
            let passt_name = name
                .as_deref()
                .is_some_and(|name| vermerk.name.eq_ignore_ascii_case(name));
            if passt_url || passt_name {
                return Some(vermerk.handle.clone());
            }
        }
        None
    }

    /// Einen Handle merken, nachdem das Sperren geklappt hat.
    pub fn merken(&self, objekt_url: &str, objekt_name: &str, handle: &str) {
        griff(&self.locks).insert(
            objekt_url.to_string(),
            Vermerk {
                handle: handle.to_string(),
                name: objekt_name.to_string(),
                url: objekt_url.to_string(),
            },
        );
    }

    /// Einen Handle vergessen (nach dem Entsperren).
    pub fn vergessen(&self, objekt_url: &str) {
        griff(&self.locks).remove(objekt_url);
    }

    /// Die Objektadressen aller offenen Locks dieser Sitzung.
    pub fn offene_locks(&self) -> Vec<String> {
        griff(&self.locks).keys().cloned().collect()
    }

    /// Ein Aufruf **in dieser Sitzung**.
    ///
    /// Übersetzt die Antworten, die nur in einer Sitzung vorkommen: ein
    /// abgelaufener Kontext wird zu [`AdtFehler::SitzungAbgelaufen`], ein
    /// gewöhnliches `401` zu [`AdtFehler::Anmeldung`]. Beides wird nie
    /// wiederholt.
    pub async fn anfrage(&self, anfrage: &Anfrage) -> Result<Antwort, AdtFehler> {
        if self.ist_abgelaufen() {
            return Err(self.abgelaufen_fehler());
        }
        let ergebnis = self.client.anfrage(anfrage).await;
        match ergebnis {
            Ok(antwort) => Ok(antwort),
            Err(fehler) => Err(self.deuten(fehler)),
        }
    }

    /// Ein Objekt sperren (`POST <objekt-url>?_action=LOCK`).
    pub async fn sperren(&self, objekt_url: &str, modus: &str) -> Result<Sperre, AdtFehler> {
        let antwort = self
            .anfrage(
                &Anfrage::ohne_inhalt(Methode::Post, objekt_url)
                    .mit_annahme(SPERR_ANNAHME)
                    .mit_abfrage("_action", "LOCK")
                    .mit_abfrage("accessMode", modus),
            )
            .await?;
        daten::sperre(&antwort.text)
    }

    /// Ein Objekt entsperren (`POST <objekt-url>?_action=UNLOCK`).
    pub async fn entsperren(&self, objekt_url: &str, handle: &str) -> Result<(), AdtFehler> {
        self.anfrage(
            &Anfrage::ohne_inhalt(Methode::Post, objekt_url)
                .mit_abfrage("_action", "UNLOCK")
                .mit_abfrage("lockHandle", handle),
        )
        .await?;
        self.vergessen(objekt_url);
        Ok(())
    }

    /// Alle bekannten Locks freigeben (Prozessende). Gibt zurück, wie viele
    /// Freigaben geklappt haben; Fehler gehen **nur** ins Log.
    pub async fn alle_entsperren(&self) -> usize {
        let offene: Vec<(String, String)> = griff(&self.locks)
            .values()
            .map(|vermerk| (vermerk.url.clone(), vermerk.handle.clone()))
            .collect();
        let mut freigegeben = 0;
        for (objekt_url, handle) in offene {
            match self.entsperren(&objekt_url, &handle).await {
                Ok(()) => freigegeben += 1,
                Err(fehler) => tracing::warn!(
                    system = %self.system.name,
                    objekt = %objekt_url,
                    "Lock beim Beenden nicht freigegeben: {fehler}"
                ),
            }
        }
        freigegeben
    }

    // --------------------------------------------------------------- innen

    fn abgelaufen_fehler(&self) -> AdtFehler {
        AdtFehler::SitzungAbgelaufen {
            system: self.system.name.clone(),
        }
    }

    /// Eine Antwort so deuten, wie sie in einer Sitzung gilt.
    fn deuten(&self, fehler: AdtFehler) -> AdtFehler {
        match &fehler {
            AdtFehler::Http { status, text } if AdtFehler::ist_sitzungsablauf(*status, text) => {
                *griff(&self.abgelaufen) = true;
                // Die Sitzung ist weg – ihre Lock-Tabelle ist nichts mehr wert.
                griff(&self.locks).clear();
                self.abgelaufen_fehler()
            }
            // In einer Sitzung kann ein `401` auch „Sitzung abgelaufen“ heißen;
            // was übrig bleibt, ist eine abgelehnte Anmeldung.
            AdtFehler::Http { status: 401, .. } => AdtFehler::Anmeldung {
                system: self.system.name.clone(),
            },
            _ => fehler,
        }
    }
}

/// Die offenen Sitzungen, ein Eintrag je System.
#[derive(Debug, Default)]
pub struct Sitzungen {
    sitzungen: Mutex<BTreeMap<String, Arc<Sitzung>>>,
}

impl Sitzungen {
    /// Die Sitzung dieses Systems – eine vorhandene, sonst eine neue.
    ///
    /// Eine abgelaufene Sitzung wird dabei stillschweigend ersetzt: der Agent
    /// bekommt beim nächsten Schreibversuch „erst abap_lock aufrufen“ und
    /// sperrt neu. Ohne Anfrage an SAP entsteht dabei nichts.
    pub fn hole(&self, system: &System) -> Result<Arc<Sitzung>, AdtFehler> {
        let mut alle = griff(&self.sitzungen);
        if let Some(vorhanden) = alle.get(&system.name) {
            if !vorhanden.ist_abgelaufen() {
                return Ok(vorhanden.clone());
            }
            alle.remove(&system.name);
        }
        let sitzung = Arc::new(Sitzung::neu(system)?);
        alle.insert(system.name.clone(), sitzung.clone());
        Ok(sitzung)
    }

    /// Die Sitzung, falls es schon eine gibt (ohne eine anzulegen).
    pub fn vorhandene(&self, system_name: &str) -> Option<Arc<Sitzung>> {
        griff(&self.sitzungen).get(system_name).cloned()
    }

    /// Alle Sitzungen, die noch Locks halten.
    pub fn mit_locks(&self) -> Vec<Arc<Sitzung>> {
        griff(&self.sitzungen)
            .values()
            .filter(|sitzung| !sitzung.offene_locks().is_empty())
            .cloned()
            .collect()
    }
}

/// Alle Locks dieser Sitzungen freigeben, **höchstens** [`FREIGABE_FRIST`].
///
/// Läuft in einem eigenen Thread mit eigener Laufzeit: der Aufruf kommt aus
/// `Drop` (stdin zu, normales Ende) oder aus dem SIGTERM-Wächter, und in beiden
/// Fällen darf er nicht auf die Laufzeit des Programms angewiesen sein.
pub fn freigeben_mit_frist(sitzungen: Vec<Arc<Sitzung>>) {
    if sitzungen.is_empty() {
        return;
    }
    let (fertig, warten) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        if let Ok(laufzeit) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            laufzeit.block_on(async {
                let _ = tokio::time::timeout(FREIGABE_FRIST, async {
                    for sitzung in &sitzungen {
                        sitzung.alle_entsperren().await;
                    }
                })
                .await;
            });
        }
        let _ = fertig.send(());
    });
    if warten.recv_timeout(FREIGABE_FRIST).is_err() {
        tracing::warn!("Locks konnten nicht rechtzeitig freigegeben werden");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn system() -> System {
        System {
            name: "dev-100".into(),
            benutzer: "DEVELOPER".into(),
            url: "https://sap.example.test:44300".into(),
            mandant: "100".into(),
            sprache: "DE".into(),
            passwort: "geheim-4711".into(),
            zertifikat_pruefen: true,
            schreiben: true,
            ca_zertifikat: None,
        }
    }

    #[test]
    fn die_lock_tabelle_gehoert_zur_sitzung() {
        let sitzung = Sitzung::neu(&system()).unwrap();
        let url = "/sap/bc/adt/oo/classes/zcl_demo";
        assert_eq!(sitzung.handle_fuer(Some("ZCL_DEMO"), None), None);
        sitzung.merken(url, "ZCL_DEMO", "LH4711");
        // Der Handle gilt für den Namen (auch klein geschrieben) und die URL.
        assert_eq!(
            sitzung.handle_fuer(Some("ZCL_DEMO"), None).as_deref(),
            Some("LH4711")
        );
        assert_eq!(
            sitzung.handle_fuer(Some("zcl_demo"), None).as_deref(),
            Some("LH4711")
        );
        assert_eq!(
            sitzung.handle_fuer(None, Some(url)).as_deref(),
            Some("LH4711")
        );
        // Ein anderes Objekt hat keinen Handle.
        assert_eq!(sitzung.handle_fuer(Some("ZCL_ANDERS"), None), None);
        assert_eq!(sitzung.offene_locks(), vec![url.to_string()]);
        sitzung.vergessen(url);
        assert_eq!(sitzung.handle_fuer(Some("ZCL_DEMO"), None), None);
        assert!(sitzung.offene_locks().is_empty());
        assert!(!sitzung.ist_abgelaufen());
    }

    #[test]
    fn der_debug_zeigt_kein_passwort() {
        let sitzung = Sitzung::neu(&system()).unwrap();
        let text = format!("{sitzung:?}");
        assert!(!text.contains("geheim-4711"), "{text}");
        assert!(text.contains("dev-100"), "{text}");
    }

    #[test]
    fn eine_abgelaufene_sitzung_wird_ersetzt() {
        let sitzungen = Sitzungen::default();
        let erste = sitzungen.hole(&system()).unwrap();
        assert!(Arc::ptr_eq(&erste, &sitzungen.hole(&system()).unwrap()));
        assert!(sitzungen.vorhandene("dev-100").is_some());
        assert!(sitzungen.vorhandene("gibtsnicht").is_none());

        // Abgelaufen: die nächste Anfrage liefert eine neue Sitzung.
        erste.merken("/sap/bc/adt/oo/classes/zcl_demo", "ZCL_DEMO", "LH1");
        *griff(&erste.abgelaufen) = true;
        griff(&erste.locks).clear();
        assert!(erste.ist_abgelaufen());
        assert!(sitzungen.mit_locks().is_empty());
        let zweite = sitzungen.hole(&system()).unwrap();
        assert!(!Arc::ptr_eq(&erste, &zweite));
        assert!(!zweite.ist_abgelaufen());
        assert_eq!(zweite.handle_fuer(Some("ZCL_DEMO"), None), None);
    }

    #[test]
    fn nur_sitzungen_mit_locks_werden_beim_ende_freigegeben() {
        let sitzungen = Sitzungen::default();
        let sitzung = sitzungen.hole(&system()).unwrap();
        assert!(sitzungen.mit_locks().is_empty());
        sitzung.merken("/sap/bc/adt/oo/classes/zcl_demo", "ZCL_DEMO", "LH4711");
        assert_eq!(sitzungen.mit_locks().len(), 1);
    }

    #[test]
    fn eine_leere_liste_gibt_sofort_frei() {
        // Ohne Sitzung wird kein Thread gestartet.
        freigeben_mit_frist(Vec::new());
    }
}
