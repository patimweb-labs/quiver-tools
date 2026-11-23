// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Einrichtungsseite über HTTP – ohne Schlüsselbund, ohne Browser, ohne
//! echten Dienst (SPEC §7).
//!
//! Jeder Test startet seine eigene Seite auf `127.0.0.1:0` mit eigenem Tresor
//! und eigenem Ordner; die Tests stören einander nicht.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use quiver_tools_kern::{
    einrichten_mit_profilen, Browser, Ende, Feld, FeldArt, Merker, Profil, Profile, Tresor,
    TresorFehler, Werkzeug,
};
use reqwest::header::CONTENT_TYPE;
use reqwest::StatusCode;

const GEHEIM: &str = "geheim-4711-kein-test-token";

/// Ein Werkzeug mit allen Feldarten, damit die Seite alles einmal rendert.
struct Attrappe;

impl Werkzeug for Attrappe {
    fn name(&self) -> &'static str {
        "attrappe"
    }

    fn titel(&self) -> &'static str {
        "Attrappen-Tool"
    }

    fn felder(&self) -> Vec<Feld> {
        vec![
            Feld {
                name: "url",
                titel: "Adresse",
                art: FeldArt::Url,
                pflicht: true,
                standard: None,
                hilfe: "Mit https://.",
            },
            Feld {
                name: "token",
                titel: "Token",
                art: FeldArt::Passwort,
                pflicht: true,
                standard: None,
                hilfe: "Steht in den Einstellungen.",
            },
            Feld {
                name: "umgebung",
                titel: "Umgebung",
                art: FeldArt::Auswahl(vec![
                    ("produktion".to_string(), "Produktion".to_string()),
                    ("test".to_string(), "Test".to_string()),
                ]),
                pflicht: false,
                standard: Some("test"),
                hilfe: "Nur eine Bezeichnung.",
            },
            Feld {
                name: "streng",
                titel: "Streng prüfen",
                art: FeldArt::Schalter,
                pflicht: false,
                standard: Some("true"),
                hilfe: "Ein Schalter.",
            },
            Feld {
                name: "tiefe",
                titel: "Tiefe",
                art: FeldArt::Zahl,
                pflicht: false,
                standard: None,
                hilfe: "Eine Zahl.",
            },
        ]
    }

    async fn testen(&self, profil: &Profil) -> anyhow::Result<String> {
        let url = profil.werte.get("url").map(String::as_str).unwrap_or("");
        if url.contains("kaputt") {
            // Absichtlich mit dem Geheimnis im Text: die Seite muss es ersetzen.
            anyhow::bail!(
                "Der Dienst antwortet nicht (Token {})",
                profil.geheim.get("token").cloned().unwrap_or_default()
            );
        }
        Ok(format!("Angemeldet als attrappe an {url}"))
    }
}

/// Ein eigener Tresor je Test: kein Schlüsselbund, keine gemeinsame Ablage.
#[derive(Default)]
struct EigenTresor {
    werte: Mutex<BTreeMap<(String, String), String>>,
}

fn griff(
    werte: &Mutex<BTreeMap<(String, String), String>>,
) -> std::sync::MutexGuard<'_, BTreeMap<(String, String), String>> {
    werte.lock().unwrap_or_else(|gift| gift.into_inner())
}

impl EigenTresor {
    fn wert(&self, dienst: &str, schluessel: &str) -> Option<String> {
        griff(&self.werte)
            .get(&(dienst.to_string(), schluessel.to_string()))
            .cloned()
    }
}

impl Tresor for EigenTresor {
    fn lesen(&self, dienst: &str, schluessel: &str) -> Result<Option<String>, TresorFehler> {
        Ok(self.wert(dienst, schluessel))
    }

    fn schreiben(&self, dienst: &str, schluessel: &str, wert: &str) -> Result<(), TresorFehler> {
        griff(&self.werte).insert(
            (dienst.to_string(), schluessel.to_string()),
            wert.to_string(),
        );
        Ok(())
    }

    fn loeschen(&self, dienst: &str, schluessel: &str) -> Result<(), TresorFehler> {
        griff(&self.werte).remove(&(dienst.to_string(), schluessel.to_string()));
        Ok(())
    }
}

/// Eine laufende Einrichtungsseite.
struct Lauf {
    /// Adresse mit Token, mit abschließendem `/`.
    url: String,
    ordner: tempfile::TempDir,
    tresor: Arc<EigenTresor>,
    ende: tokio::task::JoinHandle<Ende>,
    client: reqwest::Client,
    /// Alles, was je zurückkam – damit am Ende geprüft werden kann, dass nirgends
    /// ein Geheimnis steht.
    antworten: Mutex<Vec<String>>,
}

impl Lauf {
    async fn starte() -> Lauf {
        Self::starte_mit(Duration::from_secs(60)).await
    }

    async fn starte_mit(zeitlimit: Duration) -> Lauf {
        let ordner = tempfile::tempdir().unwrap();
        let tresor = Arc::new(EigenTresor::default());
        let profile = Profile::im_ordner(&Attrappe, tresor.clone(), ordner.path().to_path_buf());
        let merker = Arc::new(Merker::neu());

        let ende = tokio::spawn(einrichten_mit_profilen(
            Attrappe,
            profile,
            merker.clone() as Arc<dyn Browser>,
            zeitlimit,
        ));
        let url = merker.adresse().await;
        assert!(url.starts_with("http://127.0.0.1:"), "{url}");
        assert!(url.ends_with('/'), "{url}");

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();
        Lauf {
            url,
            ordner,
            tresor,
            ende,
            client,
            antworten: Mutex::new(Vec::new()),
        }
    }

    fn ziel(&self, rest: &str) -> String {
        format!("{}{}", self.url, rest)
    }

    /// Der Token-Pfad ohne abschließenden `/`, z. B. `/e/abc`.
    fn basis_ohne_schraegstrich(&self) -> String {
        self.url.trim_end_matches('/').to_string()
    }

    fn merken(&self, text: &str) {
        griff_antworten(&self.antworten).push(text.to_string());
    }

    async fn get(&self, rest: &str) -> (StatusCode, String) {
        let antwort = self.client.get(self.ziel(rest)).send().await.unwrap();
        let status = antwort.status();
        let text = antwort.text().await.unwrap();
        self.merken(&text);
        (status, text)
    }

    async fn get_absolut(&self, adresse: &str) -> (StatusCode, String) {
        let antwort = self.client.get(adresse).send().await.unwrap();
        let status = antwort.status();
        let text = antwort.text().await.unwrap();
        self.merken(&text);
        (status, text)
    }

    /// Ein Formular abschicken, so wie der Browser es tut.
    async fn post(&self, rest: &str, felder: &[(&str, &str)]) -> (StatusCode, String) {
        let koerper = felder
            .iter()
            .map(|(name, wert)| format!("{}={}", kodieren(name), kodieren(wert)))
            .collect::<Vec<_>>()
            .join("&");
        let antwort = self
            .client
            .post(self.ziel(rest))
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(koerper)
            .send()
            .await
            .unwrap();
        let status = antwort.status();
        let text = antwort.text().await.unwrap();
        self.merken(&text);
        (status, text)
    }

    fn datei(&self) -> String {
        std::fs::read_to_string(self.ordner.path().join("profile.toml")).unwrap_or_default()
    }

    fn alle_antworten(&self) -> Vec<String> {
        griff_antworten(&self.antworten).clone()
    }

    /// „Fertig“ drücken und auf das Ende warten. Verbraucht den Lauf.
    async fn fertig(self) -> Ende {
        let (status, _) = self.post("fertig", &[]).await;
        assert_eq!(status, StatusCode::OK);
        self.ende.await.unwrap()
    }

    /// Auf das Ende warten, ohne etwas zu schicken. Verbraucht den Lauf.
    async fn warte(self) -> Ende {
        self.ende.await.unwrap()
    }
}

fn griff_antworten(antworten: &Mutex<Vec<String>>) -> std::sync::MutexGuard<'_, Vec<String>> {
    antworten.lock().unwrap_or_else(|gift| gift.into_inner())
}

fn kodieren(text: &str) -> String {
    let mut raus = String::new();
    for byte in text.as_bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                raus.push(*byte as char)
            }
            b' ' => raus.push('+'),
            sonst => raus.push_str(&format!("%{sonst:02X}")),
        }
    }
    raus
}

/// Die Felder eines vollständigen Profils.
fn vollstaendig<'a>(name: &'a str, geheim: &'a str) -> Vec<(&'a str, &'a str)> {
    vec![
        ("name", name),
        ("beschreibung", "Mein Zugang"),
        ("f_url", "https://example.test"),
        ("f_token", geheim),
        ("f_umgebung", "produktion"),
        ("f_streng", "true"),
        ("f_tiefe", "3"),
    ]
}

#[tokio::test]
async fn falscher_token_pfad_ist_404() {
    let lauf = Lauf::starte().await;

    let ohne_token = lauf.url.split("/e/").next().unwrap().to_string();
    for adresse in [
        lauf.ziel("unbekannt"),
        lauf.basis_ohne_schraegstrich(),
        format!("{ohne_token}/e/falsch/"),
        format!("{ohne_token}/e/"),
        format!("{ohne_token}/"),
    ] {
        let (status, _) = lauf.get_absolut(&adresse).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{adresse}");
    }

    // Das Token ist ein anderes als „falsch“.
    assert_eq!(lauf.get("").await.0, StatusCode::OK);
    assert_eq!(lauf.fertig().await, Ende::NichtsGespeichert);
}

#[tokio::test]
async fn profil_anlegen_bearbeiten_und_testen() {
    let lauf = Lauf::starte().await;

    // Liste ist zunächst leer und lädt das Formular zum Anlegen.
    let (status, seite) = lauf.get("").await;
    assert_eq!(status, StatusCode::OK);
    assert!(seite.contains("Noch keins"));
    assert!(!seite.contains("Neues Profil"));

    let (status, seite) = lauf.get("neu").await;
    assert_eq!(status, StatusCode::OK);
    assert!(seite.contains("Neues Profil"));
    assert!(seite.contains("type=\"password\""));
    assert!(seite.contains("<select"));
    assert!(seite.contains("type=\"checkbox\""));

    // Anlegen: die Antwort leitet zurück und zeigt das gespeicherte Profil.
    let (status, seite) = lauf
        .post("speichern", &vollstaendig("arbeit", GEHEIM))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(seite.contains("arbeit"));
    assert!(seite.contains("gespeichert"));
    assert!(seite.contains("Standard"));

    // Die TOML-Datei nennt kein Geheimnis, der Tresor schon.
    assert!(lauf.datei().contains("https://example.test"));
    assert!(!lauf.datei().contains(GEHEIM));
    assert_eq!(
        lauf.tresor
            .wert("quiver-attrappe", "arbeit/token")
            .as_deref(),
        Some(GEHEIM)
    );

    // Bearbeiten mit leerem Passwort lässt das Geheimnis stehen.
    let mut ohne_geheimnis = vollstaendig("arbeit", "");
    ohne_geheimnis[1] = ("beschreibung", "Andere Beschreibung");
    let (status, seite) = lauf.post("speichern", &ohne_geheimnis).await;
    assert_eq!(status, StatusCode::OK);
    assert!(seite.contains("Andere Beschreibung"));
    assert_eq!(
        lauf.tresor
            .wert("quiver-attrappe", "arbeit/token")
            .as_deref(),
        Some(GEHEIM)
    );

    // Das Formular zeigt „gesetzt“, aber nie den Wert.
    let (_, seite) = lauf.get("bearbeiten?name=arbeit").await;
    assert!(seite.contains("Profil bearbeiten"));
    assert!(seite.contains("gesetzt – leer lassen"));
    assert!(!seite.contains(GEHEIM));
    assert!(!seite.contains("value=\"geheim"));

    // „Verbindung testen“ ruft das Werkzeug mit dem gespeicherten Profil.
    let (status, seite) = lauf.post("testen", &[("name", "arbeit")]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(seite.contains("Verbindung geprüft"), "{seite}");
    assert!(
        seite.contains("Angemeldet als attrappe an https://example.test"),
        "{seite}"
    );

    // Keine Antwort enthält je das Geheimnis.
    for antwort in lauf.alle_antworten() {
        assert!(!antwort.contains(GEHEIM), "Geheimnis in einer Antwort");
    }

    assert_eq!(lauf.fertig().await, Ende::Gespeichert);
}

#[tokio::test]
async fn testen_meldet_fehler_ohne_geheimnis() {
    let lauf = Lauf::starte().await;

    let mut felder = vollstaendig("kaputt", GEHEIM);
    felder[2] = ("f_url", "https://kaputt.test");
    lauf.post("speichern", &felder).await;

    let (status, seite) = lauf.post("testen", &[("name", "kaputt")]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(seite.contains("Verbindung fehlgeschlagen"));
    assert!(seite.contains("Der Dienst antwortet nicht"));
    // Das Werkzeug hat das Geheimnis in den Text geschrieben – die Seite nicht.
    assert!(!seite.contains(GEHEIM));
    assert!(seite.contains("[geheim]"));

    assert_eq!(lauf.fertig().await, Ende::Gespeichert);
}

#[tokio::test]
async fn pflichtfeld_fehlt_und_html_wird_maskiert() {
    let lauf = Lauf::starte().await;

    // Ohne Adresse: das Formular kommt mit Fehler zurück, die Eingaben bleiben.
    let (status, seite) = lauf
        .post(
            "speichern",
            &[
                ("name", "arbeit"),
                ("beschreibung", "<script>alert('x')</script>"),
                ("f_token", GEHEIM),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(seite.contains("Pflichtfeld fehlt"));
    assert!(seite.contains("&lt;script&gt;alert"));
    assert!(!seite.contains("<script>alert"));
    // Nichts gespeichert.
    assert!(lauf.datei().is_empty());
    assert_eq!(lauf.tresor.wert("quiver-attrappe", "arbeit/token"), None);

    // Ungültiger Name wird abgewiesen.
    let mut felder = vollstaendig("Arbeit", GEHEIM);
    felder[0] = ("name", "Arbeit");
    let (_, seite) = lauf.post("speichern", &felder).await;
    assert!(seite.contains("ist ungültig"));

    assert_eq!(lauf.fertig().await, Ende::NichtsGespeichert);
}

#[tokio::test]
async fn standard_setzen_und_loeschen_raeumt_den_tresor() {
    let lauf = Lauf::starte().await;

    for name in ["arbeit", "privat"] {
        lauf.post("speichern", &vollstaendig(name, GEHEIM)).await;
    }
    let (_, seite) = lauf.get("").await;
    assert_eq!(seite.matches("abzeichen").count(), 1);

    let (_, seite) = lauf.post("standard", &[("name", "privat")]).await;
    assert!(seite.contains("ist jetzt der Standard"));

    let (_, seite) = lauf.post("loeschen", &[("name", "privat")]).await;
    assert!(seite.contains("gelöscht"));
    assert!(!seite.contains("privat</span>"));
    assert_eq!(lauf.tresor.wert("quiver-attrappe", "privat/token"), None);
    assert_eq!(
        lauf.tresor
            .wert("quiver-attrappe", "arbeit/token")
            .as_deref(),
        Some(GEHEIM)
    );

    let (_, seite) = lauf.post("loeschen", &[("name", "gibtsnicht")]).await;
    assert!(seite.contains("gibt es nicht"));

    assert_eq!(lauf.fertig().await, Ende::Gespeichert);
}

#[tokio::test]
async fn statische_dateien_kommen_mit_den_tokens() {
    let lauf = Lauf::starte().await;

    let (status, css) = lauf.get("stil.css").await;
    assert_eq!(status, StatusCode::OK);
    assert!(css.contains("--wp-color-bg-canvas"));
    assert!(css.contains(".knopf--haupt"));
    assert!(css.contains("cursor: pointer"));
    assert!(css.contains("url(\"schrift/PlexSans-Regular.ttf\")"));
    // Keine Hex-Werte im eigenen Teil (die Tokens selbst sind Hex).
    let eigen = &css[css.find(".kopf").unwrap()..];
    assert!(!eigen.contains('#'), "Hex-Wert im eigenen CSS");

    let (status, schrift) = lauf.get("schrift/PlexSans-Regular.ttf").await;
    assert_eq!(status, StatusCode::OK);
    assert!(schrift.len() > 10_000);

    let (status, _) = lauf.get("schrift/boese.ttf").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Die Seite verweist auf das CSS neben ihr (Pfad mit Token).
    let (_, seite) = lauf.get("").await;
    let pfad = &lauf.url[lauf.url.find("/e/").unwrap()..];
    assert!(seite.contains(&format!("href=\"{pfad}stil.css\"")));
    assert!(!seite.contains(GEHEIM));

    assert_eq!(lauf.fertig().await, Ende::NichtsGespeichert);
}

#[tokio::test]
async fn zeitlimit_beendet_die_seite() {
    let lauf = Lauf::starte_mit(Duration::from_millis(300)).await;
    let ende = lauf.warte().await;
    assert_eq!(ende, Ende::Zeitlimit);
    assert_eq!(ende.exit_code(), std::process::ExitCode::FAILURE);
}

#[tokio::test]
async fn ohne_profil_zeigt_die_seite_einen_hinweis_statt_eines_fehlertexts() {
    let lauf = Lauf::starte().await;
    let (_, seite) = lauf.post("testen", &[("name", "")]).await;
    assert!(seite.contains("Noch kein Profil eingerichtet"));
    assert_eq!(lauf.fertig().await, Ende::NichtsGespeichert);
}
