// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Einrichtungsseite: axum auf `127.0.0.1:0`, ein Einmal-Token im Pfad,
//! Profile anlegen, bearbeiten, löschen, testen (SPEC §4).
//!
//! Alles, was die Seite zeigt, steht in [`html::Sicht`]; die Handler laden es
//! aus dem [`Profile`]-Speicher und reichen es an [`html::seite`] weiter. Die
//! Formulare sind POST und antworten mit einer Umleitung (PRG), damit ein
//! Neuladen nichts doppelt speichert.

pub mod html;

use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::Router;
use rand::RngCore;
use tokio::net::TcpListener;
use tokio::sync::watch;

use crate::browser::{Browser, EchterBrowser};
use crate::profil::Profile;
use crate::tresor::{self, Tresor};
use crate::{FeldArt, Profil, Werkzeug};
use html::{Formular, Meldung, ProfilZeile, Sicht};

/// Nach dieser Zeit ohne Anfrage endet die Seite (SPEC §4).
pub const ZEITLIMIT: Duration = Duration::from_secs(30 * 60);

/// Wie die Einrichtungsseite geendet hat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ende {
    /// „Fertig“ nach dem Speichern: Exit 0.
    Gespeichert,
    /// „Fertig“, ohne dass etwas gespeichert wurde: Exit 1.
    NichtsGespeichert,
    /// 30 Minuten ohne Anfrage: Exit 1.
    Zeitlimit,
    /// Abgebrochen (Strg-C) oder nicht startbar: Exit 1.
    Abgebrochen,
}

impl Ende {
    /// Der Exit-Code des Programms (SPEC §4).
    pub fn exit_code(self) -> ExitCode {
        match self {
            Ende::Gespeichert => ExitCode::SUCCESS,
            _ => ExitCode::FAILURE,
        }
    }
}

/// Was die Seite beendet hat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ausloeser {
    Fertig,
    Zeitlimit,
    Abgebrochen,
}

/// Die Einrichtungsseite für ein Werkzeug öffnen und begleiten (SPEC §4).
pub async fn einrichten<W: Werkzeug>(werkzeug: W) -> ExitCode {
    einrichten_mit(
        werkzeug,
        Arc::new(EchterBrowser),
        tresor::aus_umgebung(),
        ZEITLIMIT,
    )
    .await
    .exit_code()
}

/// Wie [`einrichten`], aber mit eigenem Browser, eigenem Tresor und eigenem
/// Zeitlimit – so prüfen Tests die Seite ohne Schlüsselbund und ohne Fenster.
pub async fn einrichten_mit<W: Werkzeug>(
    werkzeug: W,
    browser: Arc<dyn Browser>,
    tresor: Arc<dyn Tresor>,
    zeitlimit: Duration,
) -> Ende {
    let profile = Profile::laden(&werkzeug, tresor);
    einrichten_mit_profilen(werkzeug, profile, browser, zeitlimit).await
}

/// Wie [`einrichten`], aber mit fertig geladenem Profilspeicher.
///
/// Damit kann ein Test Ordner und Tresor selbst bestimmen, ohne die Umgebung
/// zu verändern.
pub async fn einrichten_mit_profilen<W: Werkzeug>(
    werkzeug: W,
    profile: Profile,
    browser: Arc<dyn Browser>,
    zeitlimit: Duration,
) -> Ende {
    match betreiben(werkzeug, profile, browser, zeitlimit).await {
        Ok(ende) => ende,
        Err(fehler) => {
            eprintln!("Die Einrichtungsseite ließ sich nicht starten: {fehler:#}");
            Ende::Abgebrochen
        }
    }
}

async fn betreiben<W: Werkzeug>(
    werkzeug: W,
    profile: Profile,
    browser: Arc<dyn Browser>,
    zeitlimit: Duration,
) -> anyhow::Result<Ende> {
    let token = neues_token();
    let (ende, _) = watch::channel(None);

    let zustand = Arc::new(Zustand {
        werkzeug,
        profile,
        token,
        gespeichert: AtomicBool::new(false),
        letzte_anfrage: Mutex::new(Instant::now()),
        meldung: Mutex::new(None),
        ende,
    });

    let app = router(zustand.clone());
    // Nur die Loopback-Adresse, Port vergibt das System: die Seite ist von
    // außen nicht erreichbar (SPEC §4).
    let listener = TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)).await?;
    let adresse = listener.local_addr()?;
    let url = format!("http://{adresse}{}", zustand.basis());

    if let Err(fehler) = browser.oeffnen(&url) {
        eprintln!("Der Browser ließ sich nicht öffnen ({fehler}). Bitte diese Adresse von Hand öffnen: {url}");
    }
    eprintln!(
        "Einrichtung für {} läuft unter {url} – „Fertig“ beendet sie, sonst nach {} Minuten ohne Anfrage.",
        zustand.profile.titel(),
        zeitlimit.as_secs() / 60
    );

    let wache = zustand.clone();
    let wachhund = tokio::spawn(async move {
        let tick = (zeitlimit / 20).clamp(Duration::from_millis(20), Duration::from_secs(5));
        loop {
            tokio::time::sleep(tick).await;
            if sperre(&wache.letzte_anfrage).elapsed() >= zeitlimit {
                wache.ende.send_replace(Some(Ausloeser::Zeitlimit));
                break;
            }
        }
    });

    let mut warten = zustand.ende.subscribe();
    let beender = zustand.ende.clone();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            tokio::select! {
                _ = warten.changed() => {}
                _ = tokio::signal::ctrl_c() => {
                    beender.send_replace(Some(Ausloeser::Abgebrochen));
                }
            }
        })
        .await?;
    wachhund.abort();

    let ausloeser = zustand
        .ende
        .borrow()
        .to_owned()
        .unwrap_or(Ausloeser::Abgebrochen);
    Ok(match ausloeser {
        Ausloeser::Fertig => {
            if zustand.gespeichert.load(Ordering::SeqCst) {
                Ende::Gespeichert
            } else {
                Ende::NichtsGespeichert
            }
        }
        Ausloeser::Zeitlimit => Ende::Zeitlimit,
        Ausloeser::Abgebrochen => Ende::Abgebrochen,
    })
}

/// Der Zustand einer laufenden Einrichtungsseite.
struct Zustand<W: Werkzeug> {
    werkzeug: W,
    profile: Profile,
    token: String,
    gespeichert: AtomicBool,
    letzte_anfrage: Mutex<Instant>,
    meldung: Mutex<Option<Meldung>>,
    ende: watch::Sender<Option<Ausloeser>>,
}

fn sperre<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|gift| gift.into_inner())
}

impl<W: Werkzeug> Zustand<W> {
    /// Pfadpräfix mit Einmal-Token: `/e/<token>/`.
    fn basis(&self) -> String {
        format!("/e/{}/", self.token)
    }

    /// Jede Anfrage hält die Seite am Leben (SPEC §4).
    fn anfrage(&self) {
        *sperre(&self.letzte_anfrage) = Instant::now();
    }

    fn merken(&self, meldung: Meldung) {
        *sperre(&self.meldung) = Some(meldung);
    }

    fn meldung_nehmen(&self) -> Option<Meldung> {
        sperre(&self.meldung).take()
    }

    fn beenden(&self, ausloeser: Ausloeser) {
        self.ende.send_replace(Some(ausloeser));
    }

    /// Die Sicht auf den Speicher: Liste, Standard, letzte Meldung.
    fn sicht(&self, formular: Option<Formular>) -> Sicht {
        let mut meldung = self.meldung_nehmen();
        let (profile, standard) = match self.profile.stand() {
            Ok(stand) => {
                let zeilen = stand
                    .profile
                    .iter()
                    .map(|profil| ProfilZeile {
                        name: profil.name.clone(),
                        beschreibung: profil.beschreibung.clone(),
                        werte: self
                            .profile
                            .felder()
                            .iter()
                            .filter_map(|feld| {
                                let wert = profil
                                    .werte
                                    .get(feld.name)
                                    .map(String::as_str)
                                    .unwrap_or("");
                                anzeigewert(feld, wert).map(|wert| (feld.titel.to_string(), wert))
                            })
                            .collect(),
                        standard: stand.standard.as_deref() == Some(profil.name.as_str()),
                    })
                    .collect();
                (zeilen, stand.standard)
            }
            Err(fehler) => {
                if meldung.is_none() {
                    meldung = Some(Meldung::fehler(fehler.to_string()));
                }
                (Vec::new(), None)
            }
        };
        Sicht {
            titel: self.profile.titel().to_string(),
            basis: self.basis(),
            felder: self.profile.felder().to_vec(),
            profile,
            standard,
            meldung,
            formular,
        }
    }

    /// Antwortseite.
    fn seite(&self, formular: Option<Formular>) -> Response {
        Html(html::seite(&self.sicht(formular))).into_response()
    }

    /// Nach dem Speichern zurück zur Liste (PRG).
    fn zurueck(&self, meldung: Meldung) -> Response {
        self.merken(meldung);
        Redirect::to(&self.basis()).into_response()
    }

    /// Ein Formular zum Bearbeiten; `gesetzt` nennt die hinterlegten Geheimnisse.
    fn formular(&self, name: &str, neu: bool, fehler: Option<String>) -> Formular {
        let gespeichert = self.profile.holen(Some(name)).ok();
        let (beschreibung, werte) = match (&gespeichert, neu) {
            (Some(profil), false) => (profil.beschreibung.clone(), profil.werte.clone()),
            _ => (String::new(), Default::default()),
        };
        let gesetzt = match &gespeichert {
            Some(profil) => profil.geheim.keys().cloned().collect::<BTreeSet<_>>(),
            None => BTreeSet::new(),
        };
        Formular {
            neu,
            name: name.to_string(),
            beschreibung,
            werte,
            gesetzt,
            fehler,
        }
    }

    /// Ein Text ohne die Geheimnisse dieses Profils.
    ///
    /// Sicherheitsnetz: `Werkzeug::testen` darf keine Geheimnisse nennen (SPEC
    /// §3); sollte doch eins im Text stehen, wird es ersetzt, bevor es auf die
    /// Seite, in eine Antwort oder ins Log geht.
    fn ohne_geheimnisse(&self, text: &str, profil: &Profil) -> String {
        let mut sauber = text.to_string();
        for wert in profil.geheim.values() {
            if wert.trim().is_empty() {
                continue;
            }
            sauber = sauber.replace(wert.as_str(), "[geheim]");
        }
        sauber
    }
}

/// Wie ein Wert in der Profilzeile steht: Schalter als „ja“, leere Werte gar
/// nicht, Geheimes nie (es kommt hier nicht an).
fn anzeigewert(feld: &crate::Feld, wert: &str) -> Option<String> {
    match feld.art {
        FeldArt::Passwort => None,
        FeldArt::Schalter => matches!(wert, "true" | "1").then(|| "ja".to_string()),
        _ => {
            let wert = wert.trim();
            (!wert.is_empty()).then(|| wert.to_string())
        }
    }
}

fn neues_token() -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn router<W: Werkzeug>(zustand: Arc<Zustand<W>>) -> Router {
    let basis = zustand.basis();
    let pfad = |rest: &str| format!("{basis}{rest}");
    Router::new()
        .route(&basis, get(liste::<W>))
        .route(&pfad("neu"), get(neu::<W>))
        .route(&pfad("bearbeiten"), get(bearbeiten::<W>))
        .route(&pfad("speichern"), post(speichern::<W>))
        .route(&pfad("testen"), post(testen::<W>))
        .route(&pfad("loeschen"), post(loeschen::<W>))
        .route(&pfad("standard"), post(als_standard::<W>))
        .route(&pfad("fertig"), post(fertig::<W>))
        .route(&pfad("stil.css"), get(stil))
        .route(&pfad("schrift/{datei}"), get(schrift))
        .with_state(zustand)
}

// ------------------------------------------------------------------ Handler

async fn liste<W: Werkzeug>(State(zustand): State<Arc<Zustand<W>>>) -> Response {
    zustand.anfrage();
    zustand.seite(None)
}

async fn neu<W: Werkzeug>(State(zustand): State<Arc<Zustand<W>>>) -> Response {
    zustand.anfrage();
    let formular = zustand.formular("", true, None);
    zustand.seite(Some(formular))
}

async fn bearbeiten<W: Werkzeug>(
    State(zustand): State<Arc<Zustand<W>>>,
    Query(abfrage): Query<std::collections::BTreeMap<String, String>>,
) -> Response {
    zustand.anfrage();
    let name = abfrage.get("name").cloned().unwrap_or_default();
    if zustand.profile.holen(Some(&name)).is_err() {
        zustand.merken(Meldung::fehler(format!(
            "Das Profil „{name}“ gibt es nicht."
        )));
        return Redirect::to(&zustand.basis()).into_response();
    }
    let formular = zustand.formular(&name, false, None);
    zustand.seite(Some(formular))
}

async fn speichern<W: Werkzeug>(
    State(zustand): State<Arc<Zustand<W>>>,
    koerper: Bytes,
) -> Response {
    zustand.anfrage();
    let daten = formdaten::Daten::lesen(&koerper);
    let name = daten.get("name").trim().to_string();
    let neu = !zustand
        .profile
        .stand()
        .map(|stand| stand.profile.iter().any(|profil| profil.name == name))
        .unwrap_or(false);

    let mut profil = Profil::leer(&name);
    profil.beschreibung = daten.get("beschreibung");
    for feld in zustand.profile.felder() {
        let schluessel = format!("f_{}", feld.name);
        match feld.art {
            FeldArt::Passwort => {
                // Leer heißt „unverändert“: der Wert im Tresor bleibt stehen.
                let wert = daten.get(&schluessel);
                if !wert.is_empty() {
                    profil.geheim.insert(feld.name.to_string(), wert);
                }
            }
            FeldArt::Schalter => {
                let an = daten.hat(&schluessel);
                profil.werte.insert(feld.name.to_string(), an.to_string());
            }
            _ => {
                profil
                    .werte
                    .insert(feld.name.to_string(), daten.get(&schluessel));
            }
        }
    }

    match zustand.profile.speichern(&profil) {
        Ok(()) => {
            zustand.gespeichert.store(true, Ordering::SeqCst);
            tracing::info!(profil = %name, "Profil gespeichert");
            zustand.zurueck(Meldung::gut(format!("Profil „{name}“ gespeichert.")))
        }
        Err(fehler) => {
            let fehler = zustand.ohne_geheimnisse(&fehler.to_string(), &profil);
            let mut formular = zustand.formular(&name, neu, Some(fehler));
            // Die Eingaben bleiben stehen – nur die Geheimnisse nicht.
            formular.beschreibung = profil.beschreibung.clone();
            for feld in zustand.profile.felder() {
                if feld.art == FeldArt::Passwort {
                    if profil.geheim.contains_key(feld.name) {
                        formular.gesetzt.insert(feld.name.to_string());
                    }
                    continue;
                }
                if let Some(wert) = profil.werte.get(feld.name) {
                    formular.werte.insert(feld.name.to_string(), wert.clone());
                }
            }
            zustand.seite(Some(formular))
        }
    }
}

async fn testen<W: Werkzeug>(State(zustand): State<Arc<Zustand<W>>>, koerper: Bytes) -> Response {
    zustand.anfrage();
    let daten = formdaten::Daten::lesen(&koerper);
    let name = daten.get("name").trim().to_string();
    let gewaehlt = (!name.is_empty()).then_some(name.as_str());

    let meldung = match zustand.profile.holen(gewaehlt) {
        // Die Texte der `ProfilFehler` nennen keine Geheimnisse.
        Err(fehler) => Meldung::fehler(fehler.to_string()),
        Ok(profil) => match zustand.werkzeug.testen(&profil).await {
            Ok(text) => {
                let text = zustand.ohne_geheimnisse(&text, &profil);
                tracing::info!(profil = %profil.name, "Verbindung geprüft: in Ordnung");
                Meldung::gut(format!("Verbindung geprüft – {}: {text}", profil.name))
            }
            Err(fehler) => {
                let text = zustand.ohne_geheimnisse(&format!("{fehler:#}"), &profil);
                tracing::warn!(profil = %profil.name, "Verbindung geprüft: fehlgeschlagen");
                Meldung::fehler(format!(
                    "Verbindung fehlgeschlagen – {}: {text}",
                    profil.name
                ))
            }
        },
    };
    zustand.zurueck(meldung)
}

async fn loeschen<W: Werkzeug>(State(zustand): State<Arc<Zustand<W>>>, koerper: Bytes) -> Response {
    zustand.anfrage();
    let name = formdaten::Daten::lesen(&koerper)
        .get("name")
        .trim()
        .to_string();
    match zustand.profile.loeschen(&name) {
        Ok(()) => {
            zustand.gespeichert.store(true, Ordering::SeqCst);
            tracing::info!(profil = %name, "Profil gelöscht");
            zustand.zurueck(Meldung::gut(format!(
                "Profil „{name}“ und sein Passwort sind gelöscht."
            )))
        }
        Err(fehler) => zustand.zurueck(Meldung::fehler(fehler.to_string())),
    }
}

async fn als_standard<W: Werkzeug>(
    State(zustand): State<Arc<Zustand<W>>>,
    koerper: Bytes,
) -> Response {
    zustand.anfrage();
    let name = formdaten::Daten::lesen(&koerper)
        .get("name")
        .trim()
        .to_string();
    match zustand.profile.als_standard(&name) {
        Ok(()) => {
            zustand.gespeichert.store(true, Ordering::SeqCst);
            zustand.zurueck(Meldung::gut(format!("„{name}“ ist jetzt der Standard.")))
        }
        Err(fehler) => zustand.zurueck(Meldung::fehler(fehler.to_string())),
    }
}

async fn fertig<W: Werkzeug>(State(zustand): State<Arc<Zustand<W>>>) -> Response {
    zustand.anfrage();
    let gespeichert = zustand.gespeichert.load(Ordering::SeqCst);
    zustand.beenden(Ausloeser::Fertig);
    Html(html::abschluss(zustand.profile.titel(), gespeichert)).into_response()
}

async fn stil() -> Response {
    let css = format!("{}\n{}", html::TOKENS, html::STIL);
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        [(header::CACHE_CONTROL, "no-store")],
        css,
    )
        .into_response()
}

async fn schrift(Path(datei): Path<String>) -> Response {
    match html::SCHRIFTEN
        .iter()
        .find(|(name, _)| *name == datei.as_str())
    {
        Some((_, inhalt)) => (
            [
                (header::CONTENT_TYPE, "font/ttf"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            *inhalt,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

// ------------------------------------------------------- Formulardaten

/// `application/x-www-form-urlencoded` ohne zusätzliche Abhängigkeit.
///
/// Die Feldnamen stehen im Schema des Werkzeugs, sind also zur Bauzeit nicht
/// bekannt – deshalb eine Karte statt eines `Deserialize`-Typs.
mod formdaten {
    use std::collections::BTreeMap;

    #[derive(Debug, Default)]
    pub struct Daten(BTreeMap<String, String>);

    impl Daten {
        pub fn lesen(koerper: &[u8]) -> Self {
            let text = String::from_utf8_lossy(koerper);
            let mut karte = BTreeMap::new();
            for paar in text.split('&') {
                if paar.is_empty() {
                    continue;
                }
                let (name, wert) = match paar.split_once('=') {
                    Some((name, wert)) => (name, wert),
                    None => (paar, ""),
                };
                karte.insert(dekodieren(name), dekodieren(wert));
            }
            Daten(karte)
        }

        pub fn get(&self, name: &str) -> String {
            self.0.get(name).cloned().unwrap_or_default()
        }

        pub fn hat(&self, name: &str) -> bool {
            self.0.contains_key(name)
        }
    }

    fn dekodieren(text: &str) -> String {
        let bytes = text.as_bytes();
        let mut raus = Vec::with_capacity(bytes.len());
        let mut stelle = 0;
        while stelle < bytes.len() {
            match bytes[stelle] {
                b'+' => {
                    raus.push(b' ');
                    stelle += 1;
                }
                b'%' if stelle + 2 < bytes.len() => {
                    let hex = std::str::from_utf8(&bytes[stelle + 1..stelle + 3]).ok();
                    match hex.and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                        Some(byte) => {
                            raus.push(byte);
                            stelle += 3;
                        }
                        None => {
                            raus.push(b'%');
                            stelle += 1;
                        }
                    }
                }
                byte => {
                    raus.push(byte);
                    stelle += 1;
                }
            }
        }
        String::from_utf8_lossy(&raus).to_string()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn liest_paare_und_dekodiert() {
            let daten = Daten::lesen(b"name=arbeit&f_url=https%3A%2F%2Fexample.test&leer=");
            assert_eq!(daten.get("name"), "arbeit");
            assert_eq!(daten.get("f_url"), "https://example.test");
            assert_eq!(daten.get("leer"), "");
            assert_eq!(daten.get("fehlt"), "");
            assert!(!daten.hat("fehlt"));
        }

        #[test]
        fn plus_ist_ein_leerzeichen_und_umlaute_kommen_an() {
            let daten = Daten::lesen(b"beschreibung=Mein+Zugang+f%C3%BCr+B%C3%BCro");
            assert_eq!(daten.get("beschreibung"), "Mein Zugang für Büro");
        }

        #[test]
        fn kaputte_sequenz_bleibt_stehen() {
            let daten = Daten::lesen(b"a=100%&b=%zz");
            assert_eq!(daten.get("a"), "100%");
            assert_eq!(daten.get("b"), "%zz");
        }

        #[test]
        fn schalter_ohne_wert() {
            let daten = Daten::lesen(b"f_aktiv=true&f_aus=");
            assert!(daten.hat("f_aktiv"));
            assert!(daten.hat("f_aus"));
            assert!(!daten.hat("f_fehlt"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_folgen_der_spec() {
        assert_eq!(Ende::Gespeichert.exit_code(), ExitCode::SUCCESS);
        assert_eq!(Ende::NichtsGespeichert.exit_code(), ExitCode::FAILURE);
        assert_eq!(Ende::Zeitlimit.exit_code(), ExitCode::FAILURE);
        assert_eq!(Ende::Abgebrochen.exit_code(), ExitCode::FAILURE);
    }

    #[test]
    fn anzeigewert_zeigt_kein_geheimnis_und_kein_falsch() {
        let feld = |art: FeldArt| crate::Feld {
            name: "x",
            titel: "X",
            art,
            pflicht: false,
            standard: None,
            hilfe: "",
        };
        assert_eq!(anzeigewert(&feld(FeldArt::Passwort), "s3hr"), None);
        assert_eq!(
            anzeigewert(&feld(FeldArt::Schalter), "true").as_deref(),
            Some("ja")
        );
        assert_eq!(anzeigewert(&feld(FeldArt::Schalter), "false"), None);
        assert_eq!(anzeigewert(&feld(FeldArt::Text), "  "), None);
        assert_eq!(
            anzeigewert(&feld(FeldArt::Url), " https://example.test ").as_deref(),
            Some("https://example.test")
        );
    }

    #[test]
    fn token_ist_lang_und_zufaellig() {
        let a = neues_token();
        let b = neues_token();
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|zeichen| zeichen.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}
