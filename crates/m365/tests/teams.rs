// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Abnahme aus `docs/aufgaben/05-teams.md`: Namen → Id, Mehrdeutiges ohne
//! Aktion, HTML → Text, kürzen, System-Nachrichten weg, Senden escaped HTML,
//! 429 beim Senden ohne zweiten POST, 429 beim Lesen mit Wiederholung,
//! fehlende Zustimmung als Hinweis (kein Browser, Token bleibt), Paginierung.
//!
//! Alles gegen die Attrappen – nie gegen echte Microsoft-Server.

#[path = "../../m365-anmeldung/tests/attrappe.rs"]
mod attrappe;
mod graph_attrappe;

use std::sync::Arc;

use attrappe::{Attrappe, AttrappenBrowser};
use graph_attrappe::GraphAttrappe;
use m365_anmeldung::{EntraAnmeldung, SpeicherTresor, Tresor};
use m365_graph::GraphClient;
use quiver_m365::teams_tools::{
    TeamsAlsGelesenAnfrage, TeamsChatAnlegenAnfrage, TeamsChatLesenAnfrage, TeamsChatSendenAnfrage,
    TeamsChatsAnfrage, TeamsEingangAnfrage, TeamsKanaeleAnfrage, TeamsKanalAntwortenAnfrage,
    TeamsKanalLesenAnfrage, TeamsKanalSendenAnfrage, TeamsMitgliederAnfrage,
    TeamsPersonFindenAnfrage, TeamsReagierenAnfrage, TeamsSuchenAnfrage, TeamsThreadLesenAnfrage,
};
use quiver_m365::{Quiver, ZUSTIMMUNG_HINWEIS};
use rmcp::handler::server::wrapper::Parameters;
use serde_json::{json, Value};

/// Tresor, den der Test auch nach dem Server-Aufbau noch lesen kann.
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
    let anmelde = Attrappe::starten().await;
    let graph = GraphAttrappe::starten().await;
    let konfig = attrappe::konfig(&anmelde);
    let tresor = GeteilterTresor(Arc::new(SpeicherTresor::neu()));

    // Fester Token im Format von Entra (JWT mit `tid`) – Aufgabe 07 braucht den
    // Mandanten für `markChatReadForUser`. Ohne `neu()`: der Test hängt nicht an
    // `M365_ACCESS_TOKEN`.
    let anmeldung = EntraAnmeldung::mit_festem_token(
        konfig,
        tresor.clone(),
        AttrappenBrowser::neu(),
        Some(graph_attrappe::access_token()),
    )
    .expect("Anmeldung baut");
    let client = GraphClient::neu(graph.adresse.clone(), anmeldung).expect("Graph-Client baut");
    let server = Quiver::neu(client, "Europe/Berlin");
    (anmelde, graph, server, tresor)
}

fn json(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|fehler| panic!("kein JSON ({fehler}): {text}"))
}

fn fehlertext(fehler: rmcp::model::ErrorData) -> String {
    format!("{fehler}")
}

#[tokio::test]
async fn teams_liste_zeigt_alle_teams() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(&server.teams_liste().await.expect("teams"));
    let liste = wert.as_array().expect("Liste");
    assert_eq!(liste.len(), 2);
    assert_eq!(liste[0]["id"], graph_attrappe::TEAM_ENTWICKLUNG);
    assert_eq!(liste[0]["name"], "Entwicklung");

    let anfrage = graph.einzige();
    assert_eq!(anfrage.pfad, "/v1.0/me/joinedTeams");
}

#[tokio::test]
async fn team_und_kanal_werden_ueber_den_namen_gefunden() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .teams_kanaele(Parameters(TeamsKanaeleAnfrage {
                team: " Entwicklung ".to_string(),
            }))
            .await
            .expect("Kanäle"),
    );
    let liste = wert.as_array().expect("Liste");
    assert_eq!(liste.len(), 2);
    assert_eq!(liste[0]["name"], "Allgemein");

    let pfade: Vec<String> = graph
        .aufzeichnungen()
        .into_iter()
        .map(|anfrage| anfrage.pfad)
        .collect();
    assert_eq!(
        pfade,
        vec![
            "/v1.0/me/joinedTeams".to_string(),
            format!("/v1.0/teams/{}/channels", graph_attrappe::TEAM_ENTWICKLUNG),
        ]
    );
}

#[tokio::test]
async fn senden_loest_namen_auf_und_escaped_html() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .teams_kanal_senden(Parameters(TeamsKanalSendenAnfrage {
                team: "Entwicklung".to_string(),
                kanal: "Allgemein".to_string(),
                text: "Milch & Brot <b>fett</b>\nZeile 2".to_string(),
                als_html: None,
            }))
            .await
            .expect("gesendet"),
    );
    assert_eq!(wert["id"], "neu-nachricht");
    assert_eq!(wert["gesendet"], true);
    assert_eq!(wert["link"], "https://teams.example/neu-nachricht");

    let alle = graph.aufzeichnungen();
    assert_eq!(alle.len(), 3, "Team, Kanal, dann der POST");
    let post = alle.last().expect("POST");
    assert_eq!(post.methode, "POST");
    assert_eq!(
        post.pfad,
        format!(
            "/v1.0/teams/{}/channels/{}/messages",
            graph_attrappe::TEAM_ENTWICKLUNG,
            graph_attrappe::KANAL_ALLGEMEIN
        )
    );
    assert_eq!(post.body()["body"]["contentType"], "html");
    assert_eq!(
        post.body()["body"]["content"],
        "Milch &amp; Brot &lt;b&gt;fett&lt;/b&gt;<br>Zeile 2"
    );
}

#[tokio::test]
async fn mehrdeutiger_chat_nennt_kandidaten_und_sendet_nichts() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let fehler = server
        .teams_chat_senden(Parameters(TeamsChatSendenAnfrage {
            chat: "Bert".to_string(),
            text: "Hallo".to_string(),
            als_html: None,
        }))
        .await
        .expect_err("mehrdeutig");
    let text = fehlertext(fehler);
    assert!(text.contains("mehrdeutig"), "{text}");
    assert!(text.contains(graph_attrappe::CHAT_PROJEKT), "{text}");
    assert!(text.contains(graph_attrappe::CHAT_REISE), "{text}");
    assert!(text.contains("nichts geändert"), "{text}");

    let alle = graph.aufzeichnungen();
    assert_eq!(alle.len(), 1, "nur die Chatliste, keine Aktion");
    assert_eq!(alle[0].methode, "GET");
}

#[tokio::test]
async fn kanal_lesen_wandelt_html_kuerzt_und_laesst_system_nachrichten_weg() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .teams_kanal_lesen(Parameters(TeamsKanalLesenAnfrage {
                team: "Entwicklung".to_string(),
                kanal: "Allgemein".to_string(),
                anzahl: Some(4),
                max_zeichen: Some(50),
            }))
            .await
            .expect("Kanäle"),
    );
    let liste = wert.as_array().expect("Liste");
    assert_eq!(liste.len(), 2, "System- und gelöschte Nachricht fehlen");

    let erster = &liste[0];
    assert_eq!(erster["id"], "m1");
    assert_eq!(erster["von"], "Bert Beispiel");
    assert_eq!(erster["text"], "Status grün", "HTML wird zu Text");
    assert_eq!(erster["anhaenge"], json!(["Anhang.pdf"]));
    assert_eq!(erster["antworten"], 2);
    assert!(erster.get("gekuerzt").is_none());
    let zeit = erster["zeit"].as_str().expect("Zeit");
    assert!(
        zeit.ends_with("+01:00") || zeit.ends_with("+02:00"),
        "RFC 3339 mit Versatz: {zeit}"
    );

    let zweiter = &liste[1];
    assert_eq!(zweiter["id"], "m3");
    assert_eq!(zweiter["gekuerzt"], true);
    assert!(
        zweiter["text"].as_str().expect("Text").chars().count() <= 50,
        "gekürzt auf max_zeichen"
    );
    assert!(zweiter["laenge_gesamt"].as_u64().unwrap_or(0) > 300);

    for verboten in ["m2", "m4"] {
        assert!(
            !liste.iter().any(|beitrag| beitrag["id"] == verboten),
            "{verboten} gehört nicht in die Antwort"
        );
    }

    // Zwei Seiten geholt (top = 4), weil Seite 1 nur zwei Beiträge hat.
    let seiten = graph
        .aufzeichnungen()
        .iter()
        .filter(|anfrage| anfrage.pfad.ends_with("/messages"))
        .count();
    assert_eq!(seiten, 2, "über @odata.nextLink geblättert");
}

#[tokio::test]
async fn paginierung_holt_nicht_mehr_seiten_als_top_verlangt() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    server
        .teams_kanal_lesen(Parameters(TeamsKanalLesenAnfrage {
            team: "Entwicklung".to_string(),
            kanal: "Allgemein".to_string(),
            anzahl: Some(2),
            max_zeichen: None,
        }))
        .await
        .expect("gelesen");

    let alle = graph.aufzeichnungen();
    assert!(
        !alle.iter().any(|anfrage| anfrage.query("seite").is_some()),
        "top=2 ist nach Seite 1 schon voll: {alle:#?}"
    );
}

#[tokio::test]
async fn thread_lesen_holt_beitrag_und_antworten() {
    let (_anmelde, _graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .teams_thread_lesen(Parameters(TeamsThreadLesenAnfrage {
                team: "Entwicklung".to_string(),
                kanal: "Allgemein".to_string(),
                nachricht: "m1".to_string(),
                max_zeichen: None,
            }))
            .await
            .expect("Thread"),
    );
    assert_eq!(wert["beitrag"]["id"], "m1");
    assert_eq!(wert["beitrag"]["text"], "Status grün");
    let antworten = wert["antworten"].as_array().expect("Antworten");
    assert_eq!(antworten.len(), 1, "die System-Antwort fehlt");
    assert_eq!(antworten[0]["id"], "r1");
    assert_eq!(antworten[0]["text"], "Alles gut.");
}

#[tokio::test]
async fn chat_senden_escaped_text_und_laesst_html_auf_wunsch_roh() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    server
        .teams_chat_senden(Parameters(TeamsChatSendenAnfrage {
            chat: graph_attrappe::CHAT_PROJEKT.to_string(),
            text: "a < b & c\nneu".to_string(),
            als_html: None,
        }))
        .await
        .expect("gesendet");
    server
        .teams_chat_senden(Parameters(TeamsChatSendenAnfrage {
            chat: graph_attrappe::CHAT_PROJEKT.to_string(),
            text: "<p>schon HTML</p>".to_string(),
            als_html: Some(true),
        }))
        .await
        .expect("gesendet");

    let alle = graph.aufzeichnungen();
    assert_eq!(alle.len(), 2, "die Id wird nicht aufgelöst");
    assert_eq!(alle[0].body()["body"]["content"], "a &lt; b &amp; c<br>neu");
    assert_eq!(alle[1].body()["body"]["content"], "<p>schon HTML</p>");
}

#[tokio::test]
async fn nach_429_wird_ein_senden_nicht_wiederholt() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;
    graph.einmal_429(0);

    let fehler = server
        .teams_chat_senden(Parameters(TeamsChatSendenAnfrage {
            chat: graph_attrappe::CHAT_PROJEKT.to_string(),
            text: "Hallo".to_string(),
            als_html: None,
        }))
        .await
        .expect_err("429 bleibt ein Fehler");
    assert!(fehlertext(fehler).contains("429"), "Graph-Fehler 429");

    let alle = graph.aufzeichnungen();
    assert_eq!(alle.len(), 1, "genau ein POST – keine doppelte Nachricht");
    assert_eq!(alle[0].methode, "POST");
}

#[tokio::test]
async fn nach_429_wird_ein_lesen_hoechstens_zweimal_wiederholt() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    graph.einmal_429(0);
    json(&server.teams_liste().await.expect("klappt nach dem Warten"));
    assert_eq!(graph.aufzeichnungen().len(), 2, "einmal wiederholt");

    graph.ab_jetzt_429(0);
    let fehler = server.teams_liste().await.expect_err("429 bleibt");
    assert!(fehlertext(fehler).contains("429"));
    assert_eq!(
        graph.aufzeichnungen().len(),
        2 + 3,
        "erster Versuch + zwei Wiederholungen"
    );
}

#[tokio::test]
async fn fehlende_zustimmung_ist_ein_hinweis_ohne_browser_und_ohne_tokenverlust() {
    let anmelde = Attrappe::starten().await;
    let graph = GraphAttrappe::starten().await;
    anmelde.zustimmung_fehlt();

    let tresor = GeteilterTresor(Arc::new(SpeicherTresor::neu()));
    tresor
        .schreiben(attrappe::SCHLUESSEL, "rt-1")
        .expect("Tresor schreibt");
    let anmeldung = EntraAnmeldung::mit_festem_token(
        attrappe::konfig(&anmelde),
        tresor.clone(),
        AttrappenBrowser::neu(),
        None,
    )
    .expect("Anmeldung baut");
    let client = GraphClient::neu(graph.adresse.clone(), anmeldung).expect("Graph-Client baut");
    let server = Quiver::neu(client, "Europe/Berlin");

    let text = server.teams_liste().await.expect("Hinweis, kein Fehler");
    assert_eq!(text, ZUSTIMMUNG_HINWEIS);
    assert!(text.contains("Für Teams fehlt die Zustimmung"), "{text}");

    assert_eq!(anmelde.autorisierungen(), 0, "kein Browser");
    assert_eq!(
        tresor
            .lesen(attrappe::SCHLUESSEL)
            .expect("Tresor lesbar")
            .as_deref(),
        Some("rt-1"),
        "das Refresh-Token bleibt"
    );
    assert!(
        graph.aufzeichnungen().is_empty(),
        "ohne Token geht keine Anfrage an Graph"
    );
}

#[tokio::test]
async fn graph_403_missing_scope_ist_derselbe_hinweis() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;
    graph.dauerfehler(
        403,
        "MissingScope",
        "Missing scope permissions on the request.",
    );

    let text = server.teams_liste().await.expect("Hinweis, kein Fehler");
    assert_eq!(text, ZUSTIMMUNG_HINWEIS);
}

#[tokio::test]
async fn personen_finden_fragt_microsoft_search_richtig() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .teams_person_finden(Parameters(TeamsPersonFindenAnfrage {
                suche: "bert".to_string(),
                anzahl: None,
            }))
            .await
            .expect("Personen"),
    );
    let liste = wert.as_array().expect("Liste");
    assert_eq!(liste[0]["name"], "Bert Beispiel");

    let anfrage = graph.einzige();
    assert_eq!(anfrage.pfad, "/v1.0/users");
    let suche = anfrage.query("$search").expect("$search");
    assert!(suche.contains("displayName:bert"), "{suche}");
    assert!(suche.contains("mail:bert"), "{suche}");
    assert_eq!(anfrage.kopf("consistencylevel"), Some("eventual"));
}

#[tokio::test]
async fn mitglieder_von_team_und_kanal() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .teams_mitglieder(Parameters(TeamsMitgliederAnfrage {
                team: "Entwicklung".to_string(),
                kanal: Some("Allgemein".to_string()),
            }))
            .await
            .expect("Mitglieder"),
    );
    assert_eq!(wert["wo"], "Entwicklung / Allgemein");
    assert_eq!(wert["mitglieder"].as_array().expect("Liste").len(), 3);

    let pfade: Vec<String> = graph
        .aufzeichnungen()
        .into_iter()
        .map(|anfrage| anfrage.pfad)
        .collect();
    let erwartet = format!(
        "/v1.0/teams/{}/channels/{}/members",
        graph_attrappe::TEAM_ENTWICKLUNG,
        graph_attrappe::KANAL_ALLGEMEIN
    );
    assert_eq!(pfade.last().map(String::as_str), Some(erwartet.as_str()));
}

#[tokio::test]
async fn chat_anlegen_mit_teilnehmer_und_erster_nachricht() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .teams_chat_anlegen(Parameters(TeamsChatAnlegenAnfrage {
                teilnehmer: vec!["bert@firma.de".to_string()],
                thema: Some("Wird bei 1:1 ignoriert".to_string()),
                nachricht: Some("Hallo Bert".to_string()),
            }))
            .await
            .expect("angelegt"),
    );
    assert_eq!(wert["id"], "19:chat-neu@thread.v2");
    assert_eq!(wert["art"], "oneOnOne");
    assert_eq!(wert["nachricht"], "neu-nachricht");

    let alle = graph.aufzeichnungen();
    let angelegt = alle
        .iter()
        .find(|anfrage| anfrage.methode == "POST" && anfrage.pfad == "/v1.0/chats")
        .expect("Chat angelegt");
    assert_eq!(angelegt.body()["chatType"], "oneOnOne");
    assert_eq!(
        angelegt.body()["members"][0]["user@odata.bind"],
        "https://graph.microsoft.com/v1.0/users('u-anna')",
        "die eigene Id, nicht /me (Aufgabe 07)"
    );
    assert_eq!(angelegt.body()["members"][0]["roles"], json!(["owner"]));
    assert_eq!(
        angelegt.body()["members"][1]["user@odata.bind"],
        "https://graph.microsoft.com/v1.0/users/u-bert"
    );
    assert!(
        angelegt.body().get("topic").is_none(),
        "ein 1:1-Chat hat kein Thema"
    );
    assert!(
        alle.iter()
            .any(|anfrage| anfrage.pfad == "/v1.0/chats/19:chat-neu@thread.v2/messages"),
        "die erste Nachricht geht in den neuen Chat"
    );
}

#[tokio::test]
async fn gruppenchat_bekommt_thema_und_mehrere_mitglieder() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .teams_chat_anlegen(Parameters(TeamsChatAnlegenAnfrage {
                teilnehmer: vec!["bert@firma.de".to_string(), "clara@firma.de".to_string()],
                thema: Some("Projekt Nord".to_string()),
                nachricht: None,
            }))
            .await
            .expect("angelegt"),
    );
    assert_eq!(wert["art"], "group");
    assert_eq!(wert["thema"], "Projekt Nord");

    let alle = graph.aufzeichnungen();
    let angelegt = alle
        .iter()
        .find(|anfrage| anfrage.methode == "POST" && anfrage.pfad == "/v1.0/chats")
        .expect("Chat angelegt");
    assert_eq!(angelegt.body()["chatType"], "group");
    assert_eq!(angelegt.body()["topic"], "Projekt Nord");
    assert_eq!(
        angelegt.body()["members"]
            .as_array()
            .expect("Mitglieder")
            .len(),
        3
    );
}

#[tokio::test]
async fn mehrdeutige_person_legt_keinen_chat_an() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let fehler = server
        .teams_chat_anlegen(Parameters(TeamsChatAnlegenAnfrage {
            teilnehmer: vec!["Anna".to_string()],
            thema: None,
            nachricht: None,
        }))
        .await
        .expect_err("mehrdeutig");
    let text = fehlertext(fehler);
    assert!(text.contains("mehrdeutig"), "{text}");
    assert!(text.contains("Anna Muster"), "{text}");
    assert!(text.contains("Anna Anders"), "{text}");

    let alle = graph.aufzeichnungen();
    assert_eq!(alle.len(), 1, "nur die Personensuche");
    assert!(alle.iter().all(|anfrage| anfrage.methode == "GET"));
}

#[tokio::test]
async fn reagieren_setzt_und_entfernt_die_reaktion() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .teams_reagieren(Parameters(TeamsReagierenAnfrage {
                chat: Some(graph_attrappe::CHAT_PROJEKT.to_string()),
                team: None,
                kanal: None,
                nachricht: "c1".to_string(),
                reaktion: "👍".to_string(),
                entfernen: None,
            }))
            .await
            .expect("reagiert"),
    );
    assert_eq!(wert["reaktion"], "like");
    assert_eq!(wert["reagiert"], true);

    let post = graph.einzige();
    assert_eq!(
        post.pfad,
        format!(
            "/v1.0/chats/{}/messages/c1/setReaction",
            graph_attrappe::CHAT_PROJEKT
        )
    );
    assert_eq!(post.body()["reactionType"], "like");
}

#[tokio::test]
async fn reagieren_im_kanal_nimmt_team_und_kanal() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    server
        .teams_reagieren(Parameters(TeamsReagierenAnfrage {
            chat: None,
            team: Some("Entwicklung".to_string()),
            kanal: Some("Allgemein".to_string()),
            nachricht: "m1".to_string(),
            reaktion: "heart".to_string(),
            entfernen: Some(true),
        }))
        .await
        .expect("reagiert");

    let alle = graph.aufzeichnungen();
    let post = alle.last().expect("POST");
    assert_eq!(
        post.pfad,
        format!(
            "/v1.0/teams/{}/channels/{}/messages/m1/unsetReaction",
            graph_attrappe::TEAM_ENTWICKLUNG,
            graph_attrappe::KANAL_ALLGEMEIN
        )
    );
    assert_eq!(post.body()["reactionType"], "heart");
}

#[tokio::test]
async fn als_gelesen_und_wieder_ungelesen() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .teams_als_gelesen(Parameters(TeamsAlsGelesenAnfrage {
                chat: graph_attrappe::CHAT_PROJEKT.to_string(),
                gelesen: None,
            }))
            .await
            .expect("gelesen"),
    );
    assert_eq!(wert["gelesen"], true);

    let alle = graph.aufzeichnungen();
    assert_eq!(alle.len(), 2, "die eigene Id, dann die Lesemarke");
    assert_eq!(alle[0].pfad, "/v1.0/me", "die eigene Id kommt aus /me");
    assert_eq!(
        alle[1].pfad,
        format!(
            "/v1.0/chats/{}/markChatReadForUser",
            graph_attrappe::CHAT_PROJEKT
        )
    );
    assert_eq!(alle[1].body()["user"]["id"], "u-anna");
    assert_eq!(
        alle[1].body()["user"]["tenantId"],
        graph_attrappe::MANDANT,
        "ohne Mandant antwortet Graph mit 400 (Aufgabe 07)"
    );

    server
        .teams_als_gelesen(Parameters(TeamsAlsGelesenAnfrage {
            chat: graph_attrappe::CHAT_PROJEKT.to_string(),
            gelesen: Some(false),
        }))
        .await
        .expect("ungelesen");
    let alle = graph.aufzeichnungen();
    assert_eq!(
        alle.last().expect("POST").pfad,
        format!(
            "/v1.0/chats/{}/markChatUnreadForUser",
            graph_attrappe::CHAT_PROJEKT
        )
    );
}

#[tokio::test]
async fn eingang_zeigt_ungelesene_chats_und_erwaehnungen() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .teams_eingang(Parameters(TeamsEingangAnfrage { tage: None }))
            .await
            .expect("Eingang"),
    );
    assert_eq!(wert["tage"], 3);

    let chats = wert["chats"].as_array().expect("Chats");
    let themen: Vec<&str> = chats
        .iter()
        .filter_map(|chat| chat["thema"].as_str())
        .collect();
    assert!(themen.contains(&"Projekt Nord"), "ungelesen: {themen:?}");
    assert!(
        themen.contains(&"Reisekosten"),
        "ohne Lesemarke gilt als ungelesen: {themen:?}"
    );
    assert!(
        !themen.contains(&"Mittagessen"),
        "gelesen und selbst geschrieben: {themen:?}"
    );

    let projekt = chats
        .iter()
        .find(|chat| chat["thema"] == "Projekt Nord")
        .expect("Projekt Nord");
    assert_eq!(projekt["von"], "Bert Beispiel");
    assert_eq!(projekt["vorschau"], "Kurze Frage zum Plan");

    let erwaehnungen = wert["erwaehnungen"].as_array().expect("Erwähnungen");
    assert_eq!(erwaehnungen.len(), 1, "nur die frische echte Erwähnung");
    assert_eq!(erwaehnungen[0]["id"], "s1");
    assert_eq!(erwaehnungen[0]["text"], "Der Plan steht");
    assert_eq!(erwaehnungen[0]["chat"], graph_attrappe::CHAT_PROJEKT);

    let recherche = graph
        .aufzeichnungen()
        .into_iter()
        .find(|anfrage| anfrage.pfad == "/v1.0/search/query")
        .expect("Suche nach Erwähnungen");
    assert_eq!(
        recherche.body()["requests"][0]["entityTypes"],
        json!(["chatMessage"])
    );
}

#[tokio::test]
async fn chats_zeigen_mitglieder_und_vorschau() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .teams_chats(Parameters(TeamsChatsAnfrage { anzahl: None }))
            .await
            .expect("Chats"),
    );
    let liste = wert.as_array().expect("Liste");
    assert_eq!(liste[0]["id"], graph_attrappe::CHAT_PROJEKT);
    assert_eq!(liste[0]["thema"], "Projekt Nord");
    assert_eq!(liste[0]["art"], "group");
    assert_eq!(
        liste[0]["mitglieder"],
        json!(["Anna Muster", "Bert Beispiel"])
    );
    assert_eq!(liste[0]["vorschau"], "Kurze Frage zum Plan");
    assert_eq!(liste[0]["von"], "Bert Beispiel");

    let anfrage = graph.einzige();
    assert_eq!(anfrage.pfad, "/v1.0/me/chats");
    let expand = anfrage.query("$expand").expect("$expand");
    assert!(expand.contains("members"), "{expand}");
    assert!(expand.contains("lastMessagePreview"), "{expand}");
}

#[tokio::test]
async fn chat_lesen_loest_das_thema_auf() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .teams_chat_lesen(Parameters(TeamsChatLesenAnfrage {
                chat: "Mittagessen".to_string(),
                anzahl: None,
                max_zeichen: None,
            }))
            .await
            .expect("Nachrichten"),
    );
    let liste = wert.as_array().expect("Liste");
    assert_eq!(liste.len(), 1, "die System-Nachricht fehlt");
    assert_eq!(liste[0]["id"], "n1");
    assert_eq!(liste[0]["text"], "Kurze Frage zum Plan");

    let pfade: Vec<String> = graph
        .aufzeichnungen()
        .into_iter()
        .map(|anfrage| anfrage.pfad)
        .collect();
    assert_eq!(
        pfade,
        vec![
            "/v1.0/me/chats".to_string(),
            format!("/v1.0/chats/{}/messages", graph_attrappe::CHAT_MITTAG),
        ]
    );
}

#[tokio::test]
async fn suchen_liefert_treffer_mit_ort_und_ohne_markierungen() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .teams_suchen(Parameters(TeamsSuchenAnfrage {
                suche: "Plan".to_string(),
                anzahl: None,
                max_zeichen: None,
            }))
            .await
            .expect("Suche"),
    );
    let liste = wert.as_array().expect("Liste");
    assert_eq!(liste.len(), 3);

    assert_eq!(liste[0]["id"], "s1");
    assert_eq!(liste[0]["text"], "Der Plan steht");
    assert_eq!(liste[0]["chat"], graph_attrappe::CHAT_PROJEKT);
    assert_eq!(liste[1]["team"], graph_attrappe::TEAM_ENTWICKLUNG);
    assert_eq!(liste[1]["kanal"], graph_attrappe::KANAL_ALLGEMEIN);

    let anfrage = graph.einzige();
    assert_eq!(anfrage.methode, "POST");
    assert_eq!(anfrage.pfad, "/v1.0/search/query");
    assert_eq!(
        anfrage.body()["requests"][0]["query"]["queryString"],
        "Plan"
    );
    assert_eq!(
        anfrage.body()["requests"][0]["entityTypes"][0],
        "chatMessage"
    );
}

#[tokio::test]
async fn kanal_antwort_ohne_text_sendet_nichts() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let fehler = server
        .teams_kanal_antworten(Parameters(TeamsKanalAntwortenAnfrage {
            team: "Entwicklung".to_string(),
            kanal: "Allgemein".to_string(),
            nachricht: "m1".to_string(),
            text: "   ".to_string(),
            als_html: None,
        }))
        .await
        .expect_err("Text fehlt");
    assert!(fehlertext(fehler).contains("text fehlt"));
    assert!(
        graph
            .aufzeichnungen()
            .iter()
            .all(|anfrage| anfrage.methode == "GET"),
        "kein POST ohne Text"
    );
}

// ---------------------------------------------------------------------------
// Regeln aus dem Live-Test (docs/m365/aufgaben/07-fehler-aus-dem-livetest.md)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn liste_und_kanaele_kommen_ohne_top_aus() {
    // Graph verbietet `$top` auf `/me/joinedTeams` und `/teams/{id}/channels`;
    // die Attrappe antwortet darauf mit 400. Ohne `$top` klappt beides.
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    json(&server.teams_liste().await.expect("Teams"));
    json(
        &server
            .teams_kanaele(Parameters(TeamsKanaeleAnfrage {
                team: "Entwicklung".to_string(),
            }))
            .await
            .expect("Kanäle"),
    );

    let alle = graph.aufzeichnungen();
    assert_eq!(alle.len(), 3, "Teams, Namenssuche, Kanäle: {alle:#?}");
    for anfrage in &alle {
        assert!(
            anfrage.query("$top").is_none(),
            "kein $top auf {}: {anfrage:#?}",
            anfrage.pfad
        );
    }
}

#[tokio::test]
async fn eigene_id_statt_me_beim_chat_anlegen() {
    // `user@odata.bind` mit `…/me` lehnt Graph ab ('user@odata.bind' field is
    // missing). Die eigene Id kommt aus `/me` und wird als `users('<id>')` gebunden.
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    json(
        &server
            .teams_chat_anlegen(Parameters(TeamsChatAnlegenAnfrage {
                teilnehmer: vec!["bert@firma.de".to_string()],
                thema: None,
                nachricht: None,
            }))
            .await
            .expect("angelegt"),
    );

    let alle = graph.aufzeichnungen();
    let angelegt = alle
        .iter()
        .find(|anfrage| anfrage.methode == "POST" && anfrage.pfad == "/v1.0/chats")
        .expect("POST /chats");
    let bindungen: Vec<&str> = angelegt.body()["members"]
        .as_array()
        .expect("Mitglieder")
        .iter()
        .filter_map(|mitglied| mitglied["user@odata.bind"].as_str())
        .collect();
    assert_eq!(
        bindungen,
        vec![
            "https://graph.microsoft.com/v1.0/users('u-anna')",
            "https://graph.microsoft.com/v1.0/users/u-bert",
        ]
    );
    assert!(
        bindungen.iter().all(|bindung| !bindung.ends_with("/me")),
        "keine Bindung an /me: {bindungen:?}"
    );
}

#[tokio::test]
async fn als_gelesen_ohne_tid_im_token_sagt_was_zu_tun_ist() {
    // Ein von Hand gesetztes Token ohne `tid` darf keinen rohen 400 von Graph
    // erzeugen – der Satz sagt, was zu tun ist, und es wird nichts geändert.
    let anmelde = Attrappe::starten().await;
    let graph = GraphAttrappe::starten().await;
    let tresor = GeteilterTresor(Arc::new(SpeicherTresor::neu()));
    let anmeldung = EntraAnmeldung::mit_festem_token(
        attrappe::konfig(&anmelde),
        tresor.clone(),
        AttrappenBrowser::neu(),
        Some("at-ohne-tid".to_string()),
    )
    .expect("Anmeldung baut");
    let client = GraphClient::neu(graph.adresse.clone(), anmeldung).expect("Graph-Client baut");
    let server = Quiver::neu(client, "Europe/Berlin");

    let fehler = server
        .teams_als_gelesen(Parameters(TeamsAlsGelesenAnfrage {
            chat: graph_attrappe::CHAT_PROJEKT.to_string(),
            gelesen: None,
        }))
        .await
        .expect_err("ohne Mandant kein Lesen");
    let text = fehlertext(fehler);
    assert!(text.contains("tid"), "{text}");
    assert!(text.contains("Anmelden"), "{text}");
    assert!(
        !graph
            .aufzeichnungen()
            .iter()
            .any(|anfrage| anfrage.pfad.ends_with("/markChatReadForUser")),
        "es wurde nichts geändert"
    );
}

#[tokio::test]
async fn mitglieder_bei_403_nennen_das_fehlende_recht() {
    // Ohne `TeamMember.Read.All` antwortet Graph mit 403. Statt des Rohfehlers
    // kommt der Satz, der zum Anmelden schickt (Aufgabe 07).
    let (_anmelde, graph, server, _tresor) = aufbau().await;
    graph.dauerfehler(
        403,
        "Authorization_RequestDenied",
        "Insufficient privileges to complete the operation.",
    );

    // Eine Id: die Namenssuche würde selbst an 403 scheitern.
    let text = server
        .teams_mitglieder(Parameters(TeamsMitgliederAnfrage {
            team: "11111111-1111-1111-1111-111111111111".to_string(),
            kanal: None,
        }))
        .await
        .expect("Hinweis, kein Fehler");
    assert_eq!(text, quiver_m365::MITGLIEDER_HINWEIS);
    assert!(text.contains("TeamMember.Read.All"), "{text}");
    assert!(text.contains("Anmelden"), "{text}");
}

#[tokio::test]
async fn die_attrappe_haelt_die_teams_regeln_von_graph_ein() {
    // Ohne diese Regeln wäre keiner der Tests oben rot geworden: die Attrappe
    // muss die 400er des Live-Tests selbst erzeugen (Aufgabe 07).
    let anmelde = Attrappe::starten().await;
    let graph = GraphAttrappe::starten().await;
    let anmeldung = EntraAnmeldung::mit_festem_token(
        attrappe::konfig(&anmelde),
        SpeicherTresor::neu(),
        AttrappenBrowser::neu(),
        Some(graph_attrappe::access_token()),
    )
    .expect("Anmeldung baut");
    let client = GraphClient::neu(graph.adresse.clone(), anmeldung).expect("Graph-Client baut");

    let status = |ergebnis: Result<Value, m365_graph::GraphFehler>| match ergebnis {
        Err(m365_graph::GraphFehler::Graph { status, code, .. }) => (status, code),
        anderes => panic!("400 erwartet, kam: {anderes:?}"),
    };

    let (status_code, code) = status(
        client
            .get("/me/joinedTeams", &[("$top", "100".to_string())])
            .await,
    );
    assert_eq!(status_code, 400);
    assert!(code.contains("BadRequest"), "{code}");

    let (status_code, _) = status(
        client
            .get(
                &format!("/teams/{}/channels", graph_attrappe::TEAM_ENTWICKLUNG),
                &[("$top", "100".to_string())],
            )
            .await,
    );
    assert_eq!(status_code, 400, "kein $top auf channels");

    let (status_code, _) = status(
        client
            .post::<Value>(
                &format!(
                    "/chats/{}/markChatReadForUser",
                    graph_attrappe::CHAT_PROJEKT
                ),
                &json!({"user": {"id": "u-anna"}}),
            )
            .await,
    );
    assert_eq!(status_code, 400, "ohne tenantId kein Lesen");

    let (status_code, _) = status(
        client
            .post::<Value>(
                "/chats",
                &json!({"chatType": "oneOnOne", "members": [{
                    "@odata.type": "#microsoft.graph.aadUserConversationMember",
                    "roles": ["owner"],
                    "user@odata.bind": "https://graph.microsoft.com/v1.0/me",
                }]}),
            )
            .await,
    );
    assert_eq!(status_code, 400, "keine Bindung an /me");

    let (status_code, _) = status(
        client
            .post::<Value>(
                "/chats",
                &json!({"chatType": "oneOnOne", "members": [{
                    "@odata.type": "#microsoft.graph.aadUserConversationMember",
                    "roles": ["owner"],
                }]}),
            )
            .await,
    );
    assert_eq!(status_code, 400, "'user@odata.bind' fehlt");
}
