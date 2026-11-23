// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Abnahme aus `docs/aufgaben/02-graph-tools.md`: je Tool ein Erfolgsfall
//! (Pfad, Query, Body, Antwortform), 401 → Erneuern + Wiederholung,
//! Graph-Fehler → MCP-Fehler ohne Token, `AnmeldungLaeuft` → Hinweistext.
//!
//! Alles gegen die Attrappen: Anmelde-Attrappe aus Aufgabe 01 und die
//! Graph-Attrappe daneben. Nie echte Microsoft-Server, nie der echte Schlüsselbund.

#[path = "../../anmeldung/tests/attrappe.rs"]
mod attrappe;
mod graph_attrappe;

use std::sync::Arc;
use std::time::{Duration, Instant};

use attrappe::{Attrappe, AttrappenBrowser};
use graph_attrappe::GraphAttrappe;
use m365_anmeldung::{EntraAnmeldung, SpeicherTresor, Tresor};
use m365_graph::GraphClient;
use quiver_m365::server::{
    DateienSuchenAnfrage, MailLesenAnfrage, MailSendenAnfrage, MailsSuchenAnfrage,
    TerminAnlegenAnfrage, TermineAnfrage,
};
use quiver_m365::{Quiver, ANMELDUNG_HINWEIS};
use rmcp::handler::server::wrapper::Parameters;
use serde_json::Value;

/// Tresor, den der Test nach dem Abmelden noch lesen kann.
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

type AnmeldungUnterTest = EntraAnmeldung<GeteilterTresor, AttrappenBrowser>;
type Server = Quiver<AnmeldungUnterTest>;

/// Anmelde-Attrappe, Graph-Attrappe, fertiger Server, Tresor.
async fn aufbau() -> (Attrappe, GraphAttrappe, Server, GeteilterTresor) {
    aufbau_mit(Duration::from_secs(5), AttrappenBrowser::neu()).await
}

async fn aufbau_mit(
    warten: Duration,
    browser: AttrappenBrowser,
) -> (Attrappe, GraphAttrappe, Server, GeteilterTresor) {
    let anmelde = Attrappe::starten().await;
    let graph = GraphAttrappe::starten().await;

    let mut konfig = attrappe::konfig(&anmelde);
    konfig.warten = warten;
    let tresor = GeteilterTresor(Arc::new(SpeicherTresor::neu()));

    // Ohne `neu()`: der Test hängt nicht an `M365_ACCESS_TOKEN`.
    let anmeldung = EntraAnmeldung::mit_festem_token(konfig, tresor.clone(), browser, None)
        .expect("Anmeldung baut");
    let client = GraphClient::neu(graph.adresse.clone(), anmeldung).expect("Graph-Client baut");
    let server = Quiver::neu(client, "Europe/Berlin");
    (anmelde, graph, server, tresor)
}

fn json(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|fehler| panic!("kein JSON ({fehler}): {text}"))
}

#[tokio::test]
async fn profil_liefert_name_und_abteilung() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(&server.profil().await.expect("profil"));
    assert_eq!(wert["name"], "Anna Muster");
    assert_eq!(wert["mail"], "anna@firma.de");
    assert_eq!(wert["abteilung"], "Entwicklung");

    let anfrage = graph.einzige();
    assert_eq!(anfrage.methode, "GET");
    assert_eq!(anfrage.pfad, "/v1.0/me");
    assert!(anfrage
        .query("$select")
        .expect("$select")
        .contains("displayName"));
    assert_eq!(anfrage.kopf("authorization"), Some("Bearer at-1"));
}

#[tokio::test]
async fn mails_suchen_ohne_suchtext_liest_den_posteingang() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .mails_suchen(Parameters(MailsSuchenAnfrage {
                suche: None,
                anzahl: None,
            }))
            .await
            .expect("mails"),
    );
    let liste = wert.as_array().expect("Liste");
    assert_eq!(liste.len(), 1);
    assert_eq!(liste[0]["id"], "m1");
    assert_eq!(liste[0]["betreff"], "Lage");
    assert_eq!(liste[0]["von"], "Anna <anna@firma.de>");
    assert_eq!(
        liste[0]["datum"], "2026-11-27T10:00:00+01:00",
        "Datum mit Versatz in M365_ZEITZONE"
    );
    assert_eq!(liste[0]["vorschau"], "Kurz");
    // Der Volltext gehört in `mail_lesen`, nicht in die Liste.
    assert!(liste[0].get("text").is_none());

    let anfrage = graph.einzige();
    assert_eq!(anfrage.pfad, "/v1.0/me/mailFolders/inbox/messages");
    assert_eq!(anfrage.query("$top"), Some("10"));
    assert_eq!(anfrage.query("$orderby"), Some("receivedDateTime desc"));
    assert!(anfrage.query("$search").is_none());
}

#[tokio::test]
async fn mails_suchen_mit_suchtext_und_grenze() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .mails_suchen(Parameters(MailsSuchenAnfrage {
                suche: Some("  Urlaub ".to_string()),
                anzahl: Some(500),
            }))
            .await
            .expect("mails"),
    );
    assert_eq!(wert.as_array().expect("Liste").len(), 2);

    let anfrage = graph.einzige();
    assert_eq!(anfrage.pfad, "/v1.0/me/messages");
    assert_eq!(
        anfrage.query("$search"),
        Some("\"Urlaub\""),
        "in Anführungszeichen"
    );
    assert_eq!(anfrage.query("$top"), Some("50"), "höchstens 50");
}

#[tokio::test]
async fn mail_lesen_holt_den_neuen_teil_statt_des_ganzen_verlaufs() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .mail_lesen(Parameters(MailLesenAnfrage {
                id: "m1".to_string(),
                verlauf: None,
                max_zeichen: None,
            }))
            .await
            .expect("mail"),
    );
    assert_eq!(wert["id"], "m1");
    assert_eq!(wert["betreff"], "Lage");
    assert_eq!(wert["an"], serde_json::json!(["bert@firma.de"]));
    assert_eq!(wert["cc"], serde_json::json!(["Clara <clara@firma.de>"]));
    assert_eq!(
        wert["text"],
        graph_attrappe::NEUER_TEIL,
        "Standard: nur der neue Teil, ohne zitierten Verlauf"
    );
    assert_eq!(wert["datum"], "2026-11-27T10:00:00+01:00");
    assert_eq!(wert["link"], "https://outlook.office365.com/mail/m1");
    assert!(wert.get("gekuerzt").is_none(), "nichts gekürzt");
    assert!(wert.get("laenge_gesamt").is_none());

    let anfrage = graph.einzige();
    assert_eq!(anfrage.pfad, "/v1.0/me/messages/m1");
    assert_eq!(
        anfrage.kopf("prefer"),
        Some("outlook.body-content-type=\"text\"")
    );
    let felder = feldliste(&anfrage);
    assert!(felder.contains(&"uniqueBody"), "$select: {felder:?}");
    assert!(
        !felder.contains(&"body"),
        "der ganze Verlauf wird gar nicht erst geholt: {felder:?}"
    );
}

#[tokio::test]
async fn mail_lesen_mit_verlauf_holt_den_ganzen_text() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .mail_lesen(Parameters(MailLesenAnfrage {
                id: "m1".to_string(),
                verlauf: Some(true),
                max_zeichen: None,
            }))
            .await
            .expect("mail"),
    );
    assert_eq!(wert["text"], graph_attrappe::GANZER_TEXT);

    let anfrage = graph.einzige();
    let felder = feldliste(&anfrage);
    assert!(felder.contains(&"body"), "$select: {felder:?}");
    assert!(!felder.contains(&"uniqueBody"), "$select: {felder:?}");
}

/// `$select` als Liste einzelner Felder (`id,subject,…`).
fn feldliste(anfrage: &graph_attrappe::Aufzeichnung) -> Vec<&str> {
    anfrage
        .query("$select")
        .expect("$select")
        .split(',')
        .collect()
}

/// Ohne `max_zeichen` sind es 4000 Zeichen, gekürzt an einer Wortgrenze.
#[tokio::test]
async fn mail_lesen_kuerzt_standardmaessig_auf_4000_zeichen() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .mail_lesen(Parameters(MailLesenAnfrage {
                id: "lang".to_string(),
                verlauf: None,
                max_zeichen: None,
            }))
            .await
            .expect("mail"),
    );
    let text = wert["text"].as_str().expect("Text");
    let voll = graph_attrappe::langer_text();

    assert_eq!(wert["gekuerzt"], true);
    assert_eq!(wert["laenge_gesamt"], voll.chars().count());
    assert!(text.chars().count() <= 4000, "zu lang: {}", text.len());
    assert!(text.chars().count() > 3900, "zu kurz gekürzt");
    assert!(
        voll.starts_with(text),
        "die Kürzung ist der Anfang des Textes"
    );
    // Hinter einem ganzen Wort: das nächste Zeichen im Original ist ein
    // Leerzeichen, nicht mitten im Wort.
    let naechstes = voll[text.len()..].chars().next().expect("Rest");
    assert_eq!(naechstes, ' ');
    assert!(
        text.chars().last().expect("Text").is_ascii_alphanumeric(),
        "endet mit einem Wort: {:?}",
        &text[text.len().saturating_sub(20)..]
    );
    assert!(graph.einzige().query("$select").is_some());
}

#[tokio::test]
async fn mail_lesen_kuerzt_auf_max_zeichen() {
    let (_anmelde, _graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .mail_lesen(Parameters(MailLesenAnfrage {
                id: "lang".to_string(),
                verlauf: None,
                max_zeichen: Some(20),
            }))
            .await
            .expect("mail"),
    );
    // "Wort1 Wort2 Wort3 Wo" → zurück bis zum Wortende.
    assert_eq!(wert["text"], "Wort1 Wort2 Wort3");
    assert_eq!(wert["gekuerzt"], true);
    assert_eq!(
        wert["laenge_gesamt"],
        graph_attrappe::langer_text().chars().count()
    );
}

#[tokio::test]
async fn mail_lesen_ist_bei_kurzem_text_nicht_gekuerzt() {
    let (_anmelde, _graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .mail_lesen(Parameters(MailLesenAnfrage {
                id: "m1".to_string(),
                verlauf: None,
                max_zeichen: Some(50_000),
            }))
            .await
            .expect("mail"),
    );
    assert_eq!(wert["text"], graph_attrappe::NEUER_TEIL);
    assert!(wert.get("gekuerzt").is_none());
    assert!(wert.get("laenge_gesamt").is_none());
}

#[tokio::test]
async fn mail_lesen_fasst_leerzeilen_zusammen() {
    let (_anmelde, _graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .mail_lesen(Parameters(MailLesenAnfrage {
                id: "leerzeilen".to_string(),
                verlauf: None,
                max_zeichen: None,
            }))
            .await
            .expect("mail"),
    );
    assert_eq!(wert["text"], "Zeile 1\n\n\nZeile 2\n\n\nZeile 3");
    assert!(wert.get("gekuerzt").is_none());
}

#[tokio::test]
async fn mail_senden_schickt_den_richtigen_body() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .mail_senden(Parameters(MailSendenAnfrage {
                an: vec!["bert@firma.de".to_string()],
                cc: Some(vec!["clara@firma.de".to_string()]),
                betreff: "Lage".to_string(),
                text: "Kurz.".to_string(),
            }))
            .await
            .expect("gesendet"),
    );
    assert_eq!(wert["gesendet"], true);

    let anfrage = graph.einzige();
    assert_eq!(anfrage.methode, "POST");
    assert_eq!(anfrage.pfad, "/v1.0/me/sendMail");
    let body = anfrage.body();
    assert_eq!(body["message"]["subject"], "Lage");
    assert_eq!(body["message"]["body"]["contentType"], "Text");
    assert_eq!(body["message"]["body"]["content"], "Kurz.");
    assert_eq!(
        body["message"]["toRecipients"][0]["emailAddress"]["address"],
        "bert@firma.de"
    );
    assert_eq!(
        body["message"]["ccRecipients"][0]["emailAddress"]["address"],
        "clara@firma.de"
    );
    assert_eq!(body["saveToSentItems"], true);
}

#[tokio::test]
async fn termine_fragt_den_kalender_mit_zeitzone() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .termine(Parameters(TermineAnfrage {
                von: "2026-11-27T10:00".to_string(),
                bis: "2026-11-28T00:00".to_string(),
            }))
            .await
            .expect("termine"),
    );
    let liste = wert.as_array().expect("Liste");
    assert_eq!(liste.len(), 4, "Attrappe liefert vier Termine");

    // Winterzeit: +01:00, ohne Sekundenbruchteile.
    let winter = &liste[0];
    assert_eq!(winter["betreff"], "Jour fixe");
    assert_eq!(winter["ort"], "Raum 1");
    assert_eq!(
        winter["teilnehmer"],
        serde_json::json!(["Bert <bert@firma.de>"])
    );
    assert_eq!(winter["start"], "2026-11-27T10:00:00+01:00");
    assert_eq!(winter["ende"], "2026-11-27T11:00:00+01:00");
    assert_eq!(winter["ganztags"], false);

    // Sommerzeit: +02:00.
    assert_eq!(liste[1]["start"], "2026-07-15T08:40:00+02:00");
    assert_eq!(liste[1]["ende"], "2026-07-15T09:40:00+02:00");

    // Ganztags: nur Datum.
    assert_eq!(liste[2]["ganztags"], true);
    assert_eq!(liste[2]["start"], "2026-11-24");
    assert_eq!(liste[2]["ende"], "2026-11-25");

    // Ortszeit mit fremder Zone (UTC) wird umgerechnet.
    assert_eq!(liste[3]["start"], "2026-11-27T10:00:00+01:00");

    let anfrage = graph.einzige();
    assert_eq!(anfrage.pfad, "/v1.0/me/calendarView");
    assert_eq!(anfrage.query("startDateTime"), Some("2026-11-27T10:00:00"));
    assert_eq!(anfrage.query("endDateTime"), Some("2026-11-28T00:00:00"));
    assert_eq!(
        anfrage.kopf("prefer"),
        Some("outlook.timezone=\"Europe/Berlin\"")
    );
}

/// Eingaben mit Versatz werden zur Ortszeit der Zone – der Zeitpunkt bleibt.
#[tokio::test]
async fn termine_nimmt_eingaben_mit_versatz() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    server
        .termine(Parameters(TermineAnfrage {
            von: "2026-11-27T10:00:00+02:00".to_string(),
            bis: "2026-11-28T00:00:00Z".to_string(),
        }))
        .await
        .expect("termine");

    let anfrage = graph.einzige();
    assert_eq!(
        anfrage.query("startDateTime"),
        Some("2026-11-27T09:00:00"),
        "10:00+02:00 ist 09:00 in Berlin"
    );
    assert_eq!(
        anfrage.query("endDateTime"),
        Some("2026-11-28T01:00:00"),
        "00:00 UTC ist 01:00 in Berlin"
    );
    assert_eq!(
        anfrage.kopf("prefer"),
        Some("outlook.timezone=\"Europe/Berlin\"")
    );
}

#[tokio::test]
async fn termin_anlegen_schickt_zeit_ort_und_teilnehmer() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .termin_anlegen(Parameters(TerminAnlegenAnfrage {
                betreff: "Planung".to_string(),
                start: "2026-12-01T09:00".to_string(),
                ende: "2026-12-01T09:30".to_string(),
                ort: Some("Raum 2".to_string()),
                teilnehmer: Some(vec!["bert@firma.de".to_string()]),
            }))
            .await
            .expect("angelegt"),
    );
    assert_eq!(wert["id"], "neu-1");
    assert_eq!(wert["betreff"], "Planung");
    assert_eq!(
        wert["start"], "2026-12-01T09:00:00+01:00",
        "Rückgabe mit Versatz"
    );
    assert_eq!(wert["ende"], "2026-12-01T09:30:00+01:00");
    assert_eq!(wert["link"], "https://outlook.office365.com/calendar/neu-1");

    let anfrage = graph.einzige();
    assert_eq!(anfrage.methode, "POST");
    assert_eq!(anfrage.pfad, "/v1.0/me/events");
    let body = anfrage.body();
    assert_eq!(body["start"]["dateTime"], "2026-12-01T09:00:00");
    assert_eq!(body["start"]["timeZone"], "Europe/Berlin");
    assert_eq!(body["end"]["dateTime"], "2026-12-01T09:30:00");
    assert_eq!(body["location"]["displayName"], "Raum 2");
    assert_eq!(
        body["attendees"][0]["emailAddress"]["address"],
        "bert@firma.de"
    );
    assert_eq!(body["attendees"][0]["type"], "required");
}

#[tokio::test]
async fn termin_anlegen_nimmt_eingaben_mit_versatz() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .termin_anlegen(Parameters(TerminAnlegenAnfrage {
                betreff: "Planung".to_string(),
                start: "2026-12-01T09:00:00+02:00".to_string(),
                ende: "2026-12-01T10:00:00Z".to_string(),
                ort: None,
                teilnehmer: None,
            }))
            .await
            .expect("angelegt"),
    );

    let body = graph.einzige().body().clone();
    // Berlin ist im Dezember +01:00: 09:00+02:00 ist 08:00 Ortszeit.
    assert_eq!(body["start"]["dateTime"], "2026-12-01T08:00:00");
    assert_eq!(body["start"]["timeZone"], "Europe/Berlin");
    // 10:00 UTC ist 11:00 in Berlin.
    assert_eq!(body["end"]["dateTime"], "2026-12-01T11:00:00");
    assert_eq!(wert["start"], "2026-12-01T08:00:00+01:00");
    assert_eq!(wert["ende"], "2026-12-01T11:00:00+01:00");
}

#[tokio::test]
async fn dateien_suchen_nutzt_die_graph_suche() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .dateien_suchen(Parameters(DateienSuchenAnfrage {
                suche: "Bericht".to_string(),
                anzahl: None,
            }))
            .await
            .expect("dateien"),
    );
    let liste = wert.as_array().expect("Liste");
    assert_eq!(liste.len(), 1);
    assert_eq!(liste[0]["name"], "Bericht.pdf");
    assert_eq!(liste[0]["pfad"], "/drive/root:/Projekte");
    assert_eq!(liste[0]["link"], "https://onedrive.example/bericht");
    assert_eq!(
        liste[0]["geaendert"], "2026-11-20T09:00:00+01:00",
        "geändert mit Versatz"
    );

    let anfrage = graph.einzige();
    assert_eq!(anfrage.pfad, "/v1.0/me/drive/root/search(q='Bericht')");
    assert_eq!(anfrage.query("$top"), Some("10"));
}

#[tokio::test]
async fn abmelden_loescht_das_refresh_token_ohne_graph() {
    let (_anmelde, graph, server, tresor) = aufbau().await;

    server.profil().await.expect("erst anmelden");
    assert!(
        tresor
            .lesen(attrappe::SCHLUESSEL)
            .expect("Tresor lesbar")
            .is_some(),
        "nach der Anmeldung liegt ein Refresh-Token im Tresor"
    );

    let wert = json(&server.abmelden().await.expect("abgemeldet"));
    assert_eq!(wert["abgemeldet"], true);
    assert!(
        tresor
            .lesen(attrappe::SCHLUESSEL)
            .expect("Tresor lesbar")
            .is_none(),
        "das Refresh-Token ist weg"
    );
    assert_eq!(
        graph.aufzeichnungen().len(),
        1,
        "abmelden redet nicht mit Graph"
    );
}

#[tokio::test]
async fn nach_401_wird_einmal_erneuert_und_wiederholt() {
    let (anmelde, graph, server, _tresor) = aufbau().await;
    graph.einmal_401();

    let wert = json(&server.profil().await.expect("zweiter Versuch klappt"));
    assert_eq!(wert["name"], "Anna Muster");

    let alle = graph.aufzeichnungen();
    assert_eq!(alle.len(), 2, "genau eine Wiederholung");
    assert_eq!(alle[0].kopf("authorization"), Some("Bearer at-1"));
    assert_eq!(
        alle[1].kopf("authorization"),
        Some("Bearer at-2"),
        "mit erneuertem Token"
    );
    assert_eq!(anmelde.autorisierungen(), 1, "kein neuer Browser");
}

#[tokio::test]
async fn nach_429_wird_nach_retry_after_wiederholt() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;
    graph.einmal_429(1);

    let beginn = Instant::now();
    json(&server.profil().await.expect("klappt nach dem Warten"));
    assert!(
        beginn.elapsed() >= Duration::from_millis(900),
        "hat auf Retry-After gewartet"
    );
    assert_eq!(graph.aufzeichnungen().len(), 2);
}

#[tokio::test]
async fn graph_fehler_kommt_als_mcp_fehler_ohne_token() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;
    graph.dauerfehler(403, "ErrorAccessDenied", "Access is denied.");

    let fehler = server.profil().await.expect_err("muss scheitern");
    let text = format!("{fehler}");
    assert!(text.contains("ErrorAccessDenied"), "{text}");
    assert!(text.contains("Access is denied."), "{text}");
    for geheim in ["at-1", "at-2", "rt-1"] {
        assert!(
            !text.contains(geheim),
            "Token {geheim} im Fehlertext: {text}"
        );
    }
}

#[tokio::test]
async fn laufende_anmeldung_ist_ein_hinweis_und_kein_fehler() {
    // Browser kommt nie zurück, das Tool wartet nur 200 ms.
    let (_anmelde, graph, server, _tresor) =
        aufbau_mit(Duration::from_millis(200), AttrappenBrowser::stumm()).await;

    let text = server.profil().await.expect("kein Fehler");
    assert_eq!(text, ANMELDUNG_HINWEIS);
    assert!(
        graph.aufzeichnungen().is_empty(),
        "ohne Token geht keine Anfrage an Graph"
    );
}
