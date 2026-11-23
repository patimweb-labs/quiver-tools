// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Abnahme aus `docs/aufgaben/03-anmeldung-ueber-quiver.md`: die Unterbefehle
//! `anmelden`, `status` und `abmelden`, mit denen das quiver-Fenster die
//! Anmeldung auslöst.
//!
//! Alles gegen die Attrappen: Anmeldedienst und Graph. Nie login.microsoftonline.com,
//! nie der echte Schlüsselbund, nie ein echter Browser.

#[path = "../../anmeldung/tests/attrappe.rs"]
mod attrappe;
mod graph_attrappe;

use std::sync::Arc;
use std::time::Duration;

use attrappe::{Attrappe, AttrappenBrowser};
use graph_attrappe::GraphAttrappe;
use m365_anmeldung::{SpeicherTresor, Tresor};
use quiver_m365::unterbefehle::{abmelden, anmelden, status};

/// Tresor, den der Test zwischen den Unterbefehlen teilt (wie der echte im Schlüsselbund).
#[derive(Clone)]
struct GeteilterTresor(Arc<SpeicherTresor>);

impl Tresor for GeteilterTresor {
    fn lesen(&self, schluessel: &str) -> anyhow::Result<Option<String>> {
        self.0.lesen(schluessel)
    }

    fn schreiben(&self, schluessel: &str, wert: &str) -> anyhow::Result<()> {
        self.0.schreiben(schluessel, wert)
    }

    fn loeschen(&self, schluessel: &str) -> anyhow::Result<()> {
        self.0.loeschen(schluessel)
    }
}

fn leerer_tresor() -> GeteilterTresor {
    GeteilterTresor(Arc::new(SpeicherTresor::neu()))
}

fn gespeichert(tresor: &GeteilterTresor) -> bool {
    tresor
        .lesen(attrappe::SCHLUESSEL)
        .expect("Tresor lesbar")
        .is_some()
}

#[tokio::test]
async fn anmelden_status_abmelden_in_folge() {
    let anmelde = Attrappe::starten().await;
    let graph = GraphAttrappe::starten().await;
    let browser = AttrappenBrowser::neu();
    let tresor = leerer_tresor();
    let konfig = attrappe::konfig(&anmelde);

    // 1. `anmelden`: Browser-Ablauf, danach das Konto auf stdout, Exit 0.
    let angemeldet = anmelden(
        konfig.clone(),
        &graph.adresse,
        tresor.clone(),
        browser.clone(),
    )
    .await;
    assert_eq!(angemeldet.code, 0, "{angemeldet:?}");
    assert_eq!(angemeldet.stdout.as_deref(), Some("anna@firma.de"));
    assert!(angemeldet.stderr.is_none(), "{angemeldet:?}");
    assert_eq!(browser.aufrufe(), 1, "genau ein Browser-Aufruf");
    assert_eq!(anmelde.autorisierungen(), 1);
    assert!(gespeichert(&tresor), "Refresh-Token liegt im Tresor");

    // 2. `status`: Exit 0 mit demselben Konto – ohne Browser, ohne /authorize.
    let geprueft = status(konfig.clone(), &graph.adresse, tresor.clone()).await;
    assert_eq!(geprueft.code, 0, "{geprueft:?}");
    assert_eq!(geprueft.stdout.as_deref(), Some("anna@firma.de"));
    assert_eq!(browser.aufrufe(), 1, "status öffnet keinen Browser");
    assert_eq!(
        anmelde.autorisierungen(),
        1,
        "status fragt den Anmeldedienst nicht nach /authorize"
    );

    // 3. `abmelden`: Refresh-Token weg, Exit 0.
    let weg = abmelden(konfig.clone(), tresor.clone());
    assert_eq!(weg.code, 0, "{weg:?}");
    assert!(!gespeichert(&tresor), "Refresh-Token ist gelöscht");

    // 4. `status` danach: Exit 1, ohne Text und ohne Browser.
    let danach = status(konfig.clone(), &graph.adresse, tresor.clone()).await;
    assert_eq!(danach.code, 1, "{danach:?}");
    assert!(danach.stdout.is_none(), "{danach:?}");
    assert!(danach.stderr.is_none(), "{danach:?}");
    assert_eq!(browser.aufrufe(), 1, "auch jetzt kein Browser");
    assert_eq!(anmelde.autorisierungen(), 1);
}

#[tokio::test]
async fn status_ohne_token_ist_exit_1_ohne_browser_und_ohne_graph() {
    let anmelde = Attrappe::starten().await;
    let graph = GraphAttrappe::starten().await;

    let ausgang = status(attrappe::konfig(&anmelde), &graph.adresse, leerer_tresor()).await;

    assert_eq!(ausgang.code, 1, "{ausgang:?}");
    assert!(ausgang.stdout.is_none() && ausgang.stderr.is_none());
    assert_eq!(anmelde.autorisierungen(), 0, "kein /authorize");
    assert_eq!(anmelde.token_aufrufe(), 0, "kein /token");
    assert!(
        graph.aufzeichnungen().is_empty(),
        "ohne Token geht keine Anfrage an Graph"
    );
}

#[tokio::test]
async fn status_nach_widerrufenem_token_ist_exit_1() {
    let anmelde = Attrappe::starten().await;
    let graph = GraphAttrappe::starten().await;
    let tresor = leerer_tresor();
    let konfig = attrappe::konfig(&anmelde);

    let angemeldet = anmelden(
        konfig.clone(),
        &graph.adresse,
        tresor.clone(),
        AttrappenBrowser::neu(),
    )
    .await;
    assert_eq!(angemeldet.code, 0);

    // Der User hat die App in Entra entzogen: der Refresh scheitert mit `invalid_grant`.
    anmelde.widerrufen("rt-1");
    let geprueft = status(konfig, &graph.adresse, tresor.clone()).await;
    assert_eq!(geprueft.code, 1, "{geprueft:?}");
    assert!(!gespeichert(&tresor), "das wertlose Token ist gelöscht");
}

#[tokio::test]
async fn netzfehler_ist_exit_2() {
    let anmelde = Attrappe::starten().await;
    let graph = GraphAttrappe::starten().await;
    let tresor = leerer_tresor();

    // Erst richtig anmelden, damit ein Refresh-Token im Tresor liegt.
    let angemeldet = anmelden(
        attrappe::konfig(&anmelde),
        &graph.adresse,
        tresor.clone(),
        AttrappenBrowser::neu(),
    )
    .await;
    assert_eq!(angemeldet.code, 0);

    // Jetzt ist der Anmeldedienst nicht erreichbar (Port 9).
    let mut kaputt = attrappe::konfig(&anmelde);
    kaputt.authority = "http://127.0.0.1:9".to_string();
    kaputt.warten = Duration::from_millis(200);

    let geprueft = status(kaputt, &graph.adresse, tresor).await;
    assert_eq!(geprueft.code, 2, "{geprueft:?}");
    assert!(geprueft.stdout.is_none());
    assert!(
        geprueft.stderr.is_some(),
        "der Fehler steht auf stderr: {geprueft:?}"
    );
}

#[tokio::test]
async fn anmelden_mit_falschem_state_ist_exit_2_ohne_token_im_text() {
    let anmelde = Attrappe::starten().await;
    let graph = GraphAttrappe::starten().await;
    let tresor = leerer_tresor();

    let ausgang = anmelden(
        attrappe::konfig(&anmelde),
        &graph.adresse,
        tresor.clone(),
        AttrappenBrowser::mit_state("falsch"),
    )
    .await;

    assert_eq!(ausgang.code, 2, "{ausgang:?}");
    assert!(ausgang.stdout.is_none());
    let text = ausgang.stderr.expect("Fehlertext auf stderr");
    assert!(!gespeichert(&tresor), "nichts gespeichert");
    for geheim in ["at-1", "rt-1", attrappe::CODE] {
        assert!(!text.contains(geheim), "Geheimnis {geheim} im Text: {text}");
    }
}

/// Der Prozess selbst: Argument → Unterbefehl → Exit-Code.
#[test]
fn status_ohne_token_ist_als_prozess_exit_1() {
    let ausgabe = std::process::Command::new(env!("CARGO_BIN_EXE_quiver-m365"))
        .arg("status")
        .env("M365_TRESOR", "speicher")
        .env("M365_TENANT", attrappe::TENANT)
        .env("M365_CLIENT_ID", attrappe::CLIENT_ID)
        // Zeigt ins Leere: ohne Token darf gar nichts geredet werden.
        .env("M365_AUTHORITY", "http://127.0.0.1:9")
        .env("M365_GRAPH_URL", "http://127.0.0.1:9/v1.0")
        .env("M365_BROWSER", "erlaubt")
        .env("RUST_LOG", "warn")
        .output()
        .expect("Programm startet");

    assert_eq!(
        ausgabe.status.code(),
        Some(1),
        "nicht angemeldet ist Exit 1"
    );
    assert!(ausgabe.stdout.is_empty(), "nichts auf stdout");
}

/// `abmelden` funktioniert auch, wenn niemand angemeldet ist (nur löschen).
#[test]
fn abmelden_als_prozess_ist_exit_0() {
    let ausgabe = std::process::Command::new(env!("CARGO_BIN_EXE_quiver-m365"))
        .arg("abmelden")
        .env("M365_TRESOR", "speicher")
        .env("M365_TENANT", attrappe::TENANT)
        .env("M365_CLIENT_ID", attrappe::CLIENT_ID)
        .env("RUST_LOG", "warn")
        .output()
        .expect("Programm startet");

    assert_eq!(ausgabe.status.code(), Some(0), "{ausgabe:?}");
    let text = String::from_utf8_lossy(&ausgabe.stdout);
    assert!(text.contains("Abgemeldet"), "{text}");
}
