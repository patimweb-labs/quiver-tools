// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Abnahme aus `docs/aufgaben/03-anmeldung-ueber-quiver.md`: Im MCP-Betrieb
//! öffnet `quiver-m365` **nie** einen Browser. Fehlt ein gültiges Refresh-Token,
//! antwortet jedes Tool (außer `abmelden`) mit dem Hinweis auf den quiver-Knopf –
//! und der Anmeldedienst sieht keinen `/authorize`-Aufruf.
//!
//! Alles gegen die Attrappen, nie gegen echte Microsoft-Server.

#[path = "../../anmeldung/tests/attrappe.rs"]
mod attrappe;
mod graph_attrappe;

use std::time::Duration;

use attrappe::{Attrappe, AttrappenBrowser};
use graph_attrappe::GraphAttrappe;
use m365_anmeldung::{EntraAnmeldung, SpeicherTresor};
use m365_graph::GraphClient;
use quiver_m365::server::{
    DateienSuchenAnfrage, MailLesenAnfrage, MailSendenAnfrage, MailsSuchenAnfrage,
    TerminAnlegenAnfrage, TermineAnfrage,
};
use quiver_m365::{Quiver, NICHT_ANGEMELDET_HINWEIS};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::ErrorData;

type Server = Quiver<EntraAnmeldung<SpeicherTresor, AttrappenBrowser>>;

/// Server im MCP-Betrieb: Browser abgeschaltet, Tresor leer, Browser-Attrappe zählt mit.
async fn aufbau() -> (Attrappe, GraphAttrappe, Server, AttrappenBrowser) {
    let anmelde = Attrappe::starten().await;
    let graph = GraphAttrappe::starten().await;

    let browser = AttrappenBrowser::neu();
    let mut konfig = attrappe::konfig(&anmelde);
    konfig.warten = Duration::from_millis(200);

    // `mit_festem_token` mit `None`: der Test hängt nicht an `M365_ACCESS_TOKEN`.
    let anmeldung =
        EntraAnmeldung::mit_festem_token(konfig, SpeicherTresor::neu(), browser.clone(), None)
            .expect("Anmeldung baut")
            .ohne_browser();
    let client = GraphClient::neu(graph.adresse.clone(), anmeldung).expect("Graph-Client baut");
    let server = Quiver::neu(client, "Europe/Berlin");

    (anmelde, graph, server, browser)
}

/// Die Antwort eines Tools ist entweder der Hinweis (kein Fehler) – oder ein Fehler.
fn hinweis(ergebnis: Result<String, ErrorData>) -> String {
    ergebnis.unwrap_or_else(|fehler| panic!("kein MCP-Fehler erwartet: {fehler}"))
}

#[tokio::test]
async fn ohne_token_kommt_der_hinweis_und_kein_browser() {
    let (anmelde, graph, server, browser) = aufbau().await;

    assert_eq!(hinweis(server.profil().await), NICHT_ANGEMELDET_HINWEIS);
    assert_eq!(
        hinweis(
            server
                .mails_suchen(Parameters(MailsSuchenAnfrage {
                    suche: Some("Urlaub".to_string()),
                    anzahl: None,
                }))
                .await
        ),
        NICHT_ANGEMELDET_HINWEIS
    );
    assert_eq!(
        hinweis(
            server
                .mail_lesen(Parameters(MailLesenAnfrage {
                    id: "m1".to_string(),
                }))
                .await
        ),
        NICHT_ANGEMELDET_HINWEIS
    );
    assert_eq!(
        hinweis(
            server
                .mail_senden(Parameters(MailSendenAnfrage {
                    an: vec!["bert@firma.de".to_string()],
                    cc: None,
                    betreff: "Lage".to_string(),
                    text: "Kurz.".to_string(),
                }))
                .await
        ),
        NICHT_ANGEMELDET_HINWEIS
    );
    assert_eq!(
        hinweis(
            server
                .termine(Parameters(TermineAnfrage {
                    von: "2026-11-27T10:00".to_string(),
                    bis: "2026-11-28T00:00".to_string(),
                }))
                .await
        ),
        NICHT_ANGEMELDET_HINWEIS
    );
    assert_eq!(
        hinweis(
            server
                .termin_anlegen(Parameters(TerminAnlegenAnfrage {
                    betreff: "Planung".to_string(),
                    start: "2026-12-01T09:00".to_string(),
                    ende: "2026-12-01T09:30".to_string(),
                    ort: None,
                    teilnehmer: None,
                }))
                .await
        ),
        NICHT_ANGEMELDET_HINWEIS
    );
    assert_eq!(
        hinweis(
            server
                .dateien_suchen(Parameters(DateienSuchenAnfrage {
                    suche: "Bericht".to_string(),
                    anzahl: None,
                }))
                .await
        ),
        NICHT_ANGEMELDET_HINWEIS
    );

    // Der Hinweis ist kein MCP-Fehler und sagt dem Agenten, was zu tun ist.
    assert!(NICHT_ANGEMELDET_HINWEIS.contains("quiver-Fenster"));
    assert!(NICHT_ANGEMELDET_HINWEIS.contains("Anmelden"));

    // Die Attrappe hat weder einen Browser gesehen noch eine Graph-Anfrage.
    assert_eq!(browser.aufrufe(), 0, "kein Browser-Aufruf");
    assert_eq!(anmelde.autorisierungen(), 0, "kein /authorize");
    assert_eq!(anmelde.token_aufrufe(), 0, "kein /token");
    assert!(
        graph.aufzeichnungen().is_empty(),
        "keine Anfrage an Graph: {:#?}",
        graph.aufzeichnungen()
    );
}

/// `abmelden` ist die eine Ausnahme: es braucht keine Anmeldung, nur den Tresor.
#[tokio::test]
async fn abmelden_geht_auch_ohne_anmeldung() {
    let (_anmelde, graph, server, browser) = aufbau().await;

    let text = hinweis(server.abmelden().await);
    assert!(text.contains("abgemeldet"), "{text}");
    assert_eq!(browser.aufrufe(), 0, "abmelden öffnet keinen Browser");
    assert!(
        graph.aufzeichnungen().is_empty(),
        "abmelden redet nicht mit Graph"
    );
}
