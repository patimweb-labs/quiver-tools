// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Abnahme aus `docs/aufgaben/09-leere-antworten-und-loeschen.md`.
//!
//! Zwei Dinge werden hier geprüft:
//!
//! * **204 mit leerem Rumpf ist ein Erfolg.** Graph antwortet auf `PATCH`,
//!   `setReaction`, `markChatReadForUser` und `DELETE` ohne Inhalt. Früher hat
//!   das Tool daraus `invalid type: null, expected struct …` gemacht – ein
//!   Fehlschlag, der keiner war. Jeder Fall hier wäre mit dem alten Code rot.
//! * **Löschen** für alles, was m365 anlegen kann: je Tool ein Erfolgsfall,
//!   destruktiv, mit Id und mit der Meldung, was verschwunden ist.
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
use quiver_m365::dateien_tools::{
    DateiLoeschenAnfrage, SharepointListeneintragAendernAnfrage,
    SharepointListeneintragLoeschenAnfrage,
};
use quiver_m365::kontakte_tools::KontaktLoeschenAnfrage;
use quiver_m365::onenote_tools::OneNoteSeiteLoeschenAnfrage;
use quiver_m365::planner_tools::{
    PlannerAufgabeAendernAnfrage, PlannerAufgabeLoeschenAnfrage, TodoAufgabeErledigenAnfrage,
    TodoAufgabeLoeschenAnfrage,
};
use quiver_m365::server::{MailLoeschenAnfrage, TerminLoeschenAnfrage};
use quiver_m365::teams_tools::{
    TeamsAlsGelesenAnfrage, TeamsNachrichtLoeschenAnfrage, TeamsReagierenAnfrage,
};
use quiver_m365::{Quiver, KEIN_SCHREIBRECHT_HINWEIS};
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

    // Fester Token im Format von Entra (JWT mit `tid`) – `markChatReadForUser`
    // braucht den Mandanten aus dem Token.
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

/// Die Methoden der aufgezeichneten Anfragen, in der Reihenfolge ihres Eintreffens.
fn methoden(graph: &GraphAttrappe) -> Vec<String> {
    graph
        .aufzeichnungen()
        .into_iter()
        .map(|anfrage| anfrage.methode)
        .collect()
}

// ---------------------------------------------------------------------------
// 1. 204 mit leerem Rumpf ist ein Erfolg
// ---------------------------------------------------------------------------

#[tokio::test]
async fn planner_aendern_nimmt_204_ohne_rumpf_als_erfolg() {
    // Der Live-Test (Aufgabe 09) zeigte: der PATCH landet, das Tool meldet aber
    // `invalid type: null, expected struct PlannerAufgabe`.
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .planner_aufgabe_aendern(Parameters(PlannerAufgabeAendernAnfrage {
                aufgabe: graph_attrappe::AUFGABE_MIT_ETAG.to_string(),
                fortschritt: Some(100),
                faellig: None,
                zugewiesen: None,
            }))
            .await
            .expect("204 ohne Rumpf ist Erfolg"),
    );
    assert_eq!(wert["geaendert"], true);
    assert_eq!(
        wert["fortschritt"], 100,
        "den Stand holt das Tool per GET: {wert}"
    );

    assert_eq!(
        methoden(&graph),
        vec!["GET", "PATCH", "GET"],
        "erst das ETag lesen, dann ändern, dann den Stand lesen"
    );
}

#[tokio::test]
async fn todo_erledigen_nimmt_204_ohne_rumpf_als_erfolg() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .todo_aufgabe_erledigen(Parameters(TodoAufgabeErledigenAnfrage {
                liste: graph_attrappe::TODO_LISTE.to_string(),
                aufgabe: graph_attrappe::TODO_AUFGABE.to_string(),
                erledigt: None,
            }))
            .await
            .expect("204 ohne Rumpf ist Erfolg"),
    );
    assert_eq!(wert["erledigt"], true);
    assert_eq!(wert["status"], "completed");

    let alle = graph.aufzeichnungen();
    assert_eq!(
        alle.last().expect("GET danach").methode,
        "GET",
        "der neue Stand kommt aus einem GET: {alle:#?}"
    );
}

#[tokio::test]
async fn listeneintrag_aendern_nimmt_204_ohne_rumpf_als_erfolg() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    // Site und Liste als Id: dann gibt es keine Namenssuche, nur den PATCH und
    // den GET auf die Felder.
    let wert = json(
        &server
            .sharepoint_listeneintrag_aendern(Parameters(SharepointListeneintragAendernAnfrage {
                site: graph_attrappe::SITE_NORD.to_string(),
                liste: graph_attrappe::LISTE_AUFGABEN.to_string(),
                eintrag: "1".to_string(),
                felder: json!({"Status": "fertig"}),
            }))
            .await
            .expect("204 ohne Rumpf ist Erfolg"),
    );
    assert_eq!(wert["geaendert"], true);
    assert_eq!(
        wert["felder"]["Status"], "fertig",
        "der Stand danach kommt aus einem GET: {wert}"
    );

    assert_eq!(methoden(&graph), vec!["PATCH", "GET"]);
    let patch = &graph.aufzeichnungen()[0];
    assert!(
        patch.pfad.ends_with("/items/1/fields"),
        "gepatcht werden die Felder, nicht der Eintrag: {}",
        patch.pfad
    );
}

#[tokio::test]
async fn reagieren_nimmt_204_ohne_rumpf_als_erfolg() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .teams_reagieren(Parameters(TeamsReagierenAnfrage {
                chat: Some(graph_attrappe::CHAT_PROJEKT.to_string()),
                team: None,
                kanal: None,
                nachricht: "c1".to_string(),
                reaktion: "like".to_string(),
                entfernen: None,
            }))
            .await
            .expect("204 ohne Rumpf ist Erfolg"),
    );
    assert_eq!(wert["reagiert"], true);
    assert_eq!(wert["reaktion"], "👍");
    assert_eq!(methoden(&graph), vec!["POST"]);
}

#[tokio::test]
async fn als_gelesen_nimmt_204_ohne_rumpf_als_erfolg() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .teams_als_gelesen(Parameters(TeamsAlsGelesenAnfrage {
                chat: graph_attrappe::CHAT_PROJEKT.to_string(),
                gelesen: None,
            }))
            .await
            .expect("204 ohne Rumpf ist Erfolg"),
    );
    assert_eq!(wert["gelesen"], true);
    assert_eq!(methoden(&graph), vec!["GET", "POST"]);
}

// ---------------------------------------------------------------------------
// 2. Löschen – je Tool ein Erfolgsfall
// ---------------------------------------------------------------------------

#[tokio::test]
async fn termin_loeschen_loescht_und_nennt_den_betreff() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .termin_loeschen(Parameters(TerminLoeschenAnfrage {
                id: "t1".to_string(),
            }))
            .await
            .expect("gelöscht"),
    );
    assert_eq!(wert["geloescht"], true);
    assert_eq!(wert["betreff"], "Jour fixe");

    let alle = graph.aufzeichnungen();
    assert_eq!(alle[0].methode, "GET");
    assert_eq!(alle[0].pfad, "/v1.0/me/events/t1");
    assert_eq!(alle[1].methode, "DELETE");
    assert_eq!(alle[1].pfad, "/v1.0/me/events/t1");
}

#[tokio::test]
async fn mail_loeschen_legt_in_den_papierkorb() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .mail_loeschen(Parameters(MailLoeschenAnfrage {
                id: "m1".to_string(),
            }))
            .await
            .expect("im Papierkorb"),
    );
    assert_eq!(wert["geloescht"], true);
    assert_eq!(wert["papierkorb"], true);
    assert_eq!(wert["betreff"], "Lage");

    // Kein DELETE: `move` nach `deleteditems` – die Mail bleibt wiederherstellbar.
    let alle = graph.aufzeichnungen();
    assert!(
        alle.iter().all(|anfrage| anfrage.methode != "DELETE"),
        "eine Mail wird nicht endgültig gelöscht: {alle:#?}"
    );
    let verschieben = alle.last().expect("POST");
    assert_eq!(verschieben.methode, "POST");
    assert_eq!(verschieben.pfad, "/v1.0/me/messages/m1/move");
    assert_eq!(verschieben.body()["destinationId"], "deleteditems");
}

#[tokio::test]
async fn datei_loeschen_loescht_in_den_papierkorb() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .datei_loeschen(Parameters(DateiLoeschenAnfrage {
                datei: "notiz.md".to_string(),
                site: None,
                bibliothek: None,
            }))
            .await
            .expect("gelöscht"),
    );
    assert_eq!(wert["geloescht"], true);
    assert_eq!(wert["name"], "notiz.md");
    assert_eq!(wert["papierkorb"], true);

    let alle = graph.aufzeichnungen();
    let loeschen = alle.last().expect("DELETE");
    assert_eq!(loeschen.methode, "DELETE");
    assert_eq!(
        loeschen.pfad,
        format!("/v1.0/me/drive/items/{}", graph_attrappe::ITEM_TEXT)
    );
}

#[tokio::test]
async fn planner_aufgabe_loeschen_nimmt_das_etag_mit() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .planner_aufgabe_loeschen(Parameters(PlannerAufgabeLoeschenAnfrage {
                aufgabe: graph_attrappe::AUFGABE_MIT_ETAG.to_string(),
            }))
            .await
            .expect("gelöscht"),
    );
    assert_eq!(wert["geloescht"], true);
    assert_eq!(wert["titel"], "Angebot schreiben");

    let alle = graph.aufzeichnungen();
    assert_eq!(alle[0].methode, "GET");
    let loeschen = &alle[1];
    assert_eq!(loeschen.methode, "DELETE");
    assert_eq!(
        loeschen.pfad,
        format!("/v1.0/planner/tasks/{}", graph_attrappe::AUFGABE_MIT_ETAG)
    );
    assert_eq!(
        loeschen.kopf("if-match"),
        Some("W/\"task-1\""),
        "Planner löscht nur mit If-Match"
    );
}

#[tokio::test]
async fn planner_loeschen_ohne_etag_loescht_nichts() {
    // Ohne ETag gibt es keinen DELETE – lieber eine Meldung als ein blindes Löschen.
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let fehler = server
        .planner_aufgabe_loeschen(Parameters(PlannerAufgabeLoeschenAnfrage {
            aufgabe: graph_attrappe::AUFGABE_OHNE_ETAG.to_string(),
        }))
        .await
        .expect_err("kein ETag");
    let text = fehlertext(fehler);
    assert!(text.contains("ohne ETag"), "{text}");
    assert!(text.contains("nichts gelöscht"), "{text}");

    let alle = graph.aufzeichnungen();
    assert_eq!(alle.len(), 1, "nur das Lesen: {alle:#?}");
    assert!(alle.iter().all(|anfrage| anfrage.methode == "GET"));
}

#[tokio::test]
async fn todo_aufgabe_loeschen_nennt_liste_und_titel() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .todo_aufgabe_loeschen(Parameters(TodoAufgabeLoeschenAnfrage {
                liste: graph_attrappe::TODO_LISTE.to_string(),
                aufgabe: "Rechnung prüfen".to_string(),
            }))
            .await
            .expect("gelöscht"),
    );
    assert_eq!(wert["geloescht"], true);
    assert_eq!(wert["titel"], "Rechnung prüfen");

    let loeschen = graph
        .aufzeichnungen()
        .into_iter()
        .find(|anfrage| anfrage.methode == "DELETE")
        .expect("DELETE");
    assert_eq!(
        loeschen.pfad,
        format!(
            "/v1.0/me/todo/lists/{}/tasks/{}",
            graph_attrappe::TODO_LISTE,
            graph_attrappe::TODO_AUFGABE
        )
    );
}

#[tokio::test]
async fn onenote_seite_loeschen_nennt_den_titel() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .onenote_seite_loeschen(Parameters(OneNoteSeiteLoeschenAnfrage {
                seite: graph_attrappe::ONENOTE_SEITE.to_string(),
            }))
            .await
            .expect("gelöscht"),
    );
    assert_eq!(wert["geloescht"], true);
    assert_eq!(wert["titel"], "Jour fixe");

    let loeschen = graph
        .aufzeichnungen()
        .into_iter()
        .find(|anfrage| anfrage.methode == "DELETE")
        .expect("DELETE");
    assert_eq!(
        loeschen.pfad,
        format!("/v1.0/me/onenote/pages/{}", graph_attrappe::ONENOTE_SEITE)
    );
}

#[tokio::test]
async fn kontakt_loeschen_nennt_den_namen() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .kontakt_loeschen(Parameters(KontaktLoeschenAnfrage {
                kontakt: "k1".to_string(),
            }))
            .await
            .expect("gelöscht"),
    );
    assert_eq!(wert["geloescht"], true);
    assert_eq!(wert["name"], "Bert Beispiel");

    let loeschen = graph
        .aufzeichnungen()
        .into_iter()
        .find(|anfrage| anfrage.methode == "DELETE")
        .expect("DELETE");
    assert_eq!(loeschen.pfad, "/v1.0/me/contacts/k1");
}

#[tokio::test]
async fn listeneintrag_loeschen_nennt_den_titel() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .sharepoint_listeneintrag_loeschen(Parameters(SharepointListeneintragLoeschenAnfrage {
                site: graph_attrappe::SITE_NORD.to_string(),
                liste: graph_attrappe::LISTE_AUFGABEN.to_string(),
                eintrag: "1".to_string(),
            }))
            .await
            .expect("gelöscht"),
    );
    assert_eq!(wert["geloescht"], true);
    assert_eq!(wert["titel"], "Erster");

    let loeschen = graph
        .aufzeichnungen()
        .into_iter()
        .find(|anfrage| anfrage.methode == "DELETE")
        .expect("DELETE");
    assert!(
        loeschen.pfad.starts_with("/v1.0/sites/") && loeschen.pfad.ends_with("/items/1"),
        "Pfad: {}",
        loeschen.pfad
    );
    assert!(
        loeschen.pfad.contains(graph_attrappe::LISTE_AUFGABEN),
        "die Liste steht im Pfad: {}",
        loeschen.pfad
    );
}

#[tokio::test]
async fn teams_nachricht_loeschen_ist_ein_soft_delete() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .teams_nachricht_loeschen(Parameters(TeamsNachrichtLoeschenAnfrage {
                chat: Some(graph_attrappe::CHAT_PROJEKT.to_string()),
                team: None,
                kanal: None,
                nachricht: "c1".to_string(),
            }))
            .await
            .expect("gelöscht"),
    );
    assert_eq!(wert["geloescht"], true);
    assert_eq!(wert["wiederherstellbar"], true);

    let post = graph.einzige();
    assert_eq!(
        post.pfad,
        format!(
            "/v1.0/chats/{}/messages/c1/softDelete",
            graph_attrappe::CHAT_PROJEKT
        )
    );
}

#[tokio::test]
async fn teams_nachricht_loeschen_im_kanal_nimmt_team_und_kanal() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    server
        .teams_nachricht_loeschen(Parameters(TeamsNachrichtLoeschenAnfrage {
            chat: None,
            team: Some("Entwicklung".to_string()),
            kanal: Some("Allgemein".to_string()),
            nachricht: "m1".to_string(),
        }))
        .await
        .expect("gelöscht");

    let post = graph
        .aufzeichnungen()
        .into_iter()
        .find(|anfrage| anfrage.methode == "POST")
        .expect("POST");
    assert_eq!(
        post.pfad,
        format!(
            "/v1.0/teams/{}/channels/{}/messages/m1/softDelete",
            graph_attrappe::TEAM_ENTWICKLUNG,
            graph_attrappe::KANAL_ALLGEMEIN
        )
    );
}

// ---------------------------------------------------------------------------
// 3. Schreibgeschützte Spalten und 403
// ---------------------------------------------------------------------------

#[tokio::test]
async fn schreibgeschuetzte_spalten_werden_gefiltert() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .sharepoint_listeneintrag_aendern(Parameters(SharepointListeneintragAendernAnfrage {
                site: graph_attrappe::SITE_NORD.to_string(),
                liste: graph_attrappe::LISTE_AUFGABEN.to_string(),
                eintrag: "1".to_string(),
                felder: json!({
                    "Titel": "Neu",
                    "LinkTitle": "Neu",
                    "ID": 1,
                    "Created": "2026-11-27T09:00:00Z",
                    "Modified": "2026-11-27T09:00:00Z",
                    "Author": {"id": "u-anna"},
                    "Editor": {"id": "u-anna"},
                    "_UIVersionString": "2.0",
                }),
            }))
            .await
            .expect("gefiltert und geändert"),
    );

    let patch = graph
        .aufzeichnungen()
        .into_iter()
        .find(|anfrage| anfrage.methode == "PATCH")
        .expect("PATCH");
    assert_eq!(
        patch.body(),
        &json!({"Titel": "Neu"}),
        "nur die schreibbare Spalte geht an Graph"
    );

    assert_eq!(wert["geaendert"], true);
    let ignoriert: Vec<&str> = wert["ignoriert"]
        .as_array()
        .expect("ignoriert")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    for spalte in [
        "LinkTitle",
        "ID",
        "Created",
        "Modified",
        "Author",
        "Editor",
        "_UIVersionString",
    ] {
        assert!(ignoriert.contains(&spalte), "{spalte} fehlt: {ignoriert:?}");
    }
    assert!(
        wert["hinweis"].as_str().is_some(),
        "der Hinweis sagt, warum: {wert}"
    );
}

#[tokio::test]
async fn nur_schreibgeschuetzte_spalten_ergeben_keine_anfrage() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let fehler = server
        .sharepoint_listeneintrag_aendern(Parameters(SharepointListeneintragAendernAnfrage {
            site: graph_attrappe::SITE_NORD.to_string(),
            liste: graph_attrappe::LISTE_AUFGABEN.to_string(),
            eintrag: "1".to_string(),
            felder: json!({"ID": 1, "Modified": "2026-11-27T09:00:00Z"}),
        }))
        .await
        .expect_err("nur schreibgeschützt");
    let text = fehlertext(fehler);
    assert!(text.contains("schreibgeschützte"), "{text}");
    assert!(text.contains("nichts geändert"), "{text}");
    assert!(graph.aufzeichnungen().is_empty(), "keine Anfrage an Graph");
}

#[tokio::test]
async fn ein_403_auf_einer_liste_ergibt_den_hinweis() {
    // Ein 403 auf einer HR-Liste heißt „die Liste ist schreibgeschützt“, nicht
    // „der Code ist kaputt“ (Aufgabe 09).
    let (_anmelde, graph, server, _tresor) = aufbau().await;
    graph.dauerfehler(403, "accessDenied", "Access denied");

    let antwort = server
        .sharepoint_listeneintrag_aendern(Parameters(SharepointListeneintragAendernAnfrage {
            site: graph_attrappe::SITE_NORD.to_string(),
            liste: graph_attrappe::LISTE_AUFGABEN.to_string(),
            eintrag: "1".to_string(),
            felder: json!({"Status": "fertig"}),
        }))
        .await
        .expect("Hinweis statt Fehler");
    assert_eq!(antwort, KEIN_SCHREIBRECHT_HINWEIS);
}

#[tokio::test]
async fn ohne_id_und_ohne_nachricht_wird_nichts_geloescht() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    assert!(server
        .termin_loeschen(Parameters(TerminLoeschenAnfrage {
            id: "  ".to_string(),
        }))
        .await
        .is_err());
    assert!(server
        .mail_loeschen(Parameters(MailLoeschenAnfrage { id: String::new() }))
        .await
        .is_err());
    assert!(server
        .teams_nachricht_loeschen(Parameters(TeamsNachrichtLoeschenAnfrage {
            chat: None,
            team: None,
            kanal: None,
            nachricht: "m1".to_string(),
        }))
        .await
        .is_err());
    assert!(
        graph.aufzeichnungen().is_empty(),
        "ohne Angabe geht keine Anfrage an Graph"
    );
}
