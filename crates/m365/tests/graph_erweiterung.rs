// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Abnahme aus `docs/aufgaben/06-graph-erweiterung.md`: je Tool ein Erfolgsfall,
//! mehrdeutige Namen ohne Aktion, Planner nur mit ETag, Upload-Grenze, 429 beim
//! POST ohne zweiten Versuch, fehlende Zustimmung als Hinweis.
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
    DateiHochladenAnfrage, DateiLesenAnfrage, DateienListeAnfrage, OrdnerAnlegenAnfrage,
    SharepointBibliothekenAnfrage, SharepointListenAnfrage, SharepointListeneintraegeAnfrage,
    SharepointListeneintragAendernAnfrage, SharepointListeneintragAnlegenAnfrage,
    SharepointSitesAnfrage,
};
use quiver_m365::kontakte_tools::{
    KontaktAnlegenAnfrage, KontakteSuchenAnfrage, PersonenSuchenAnfrage,
};
use quiver_m365::onenote_tools::{
    OneNoteAbschnitteAnfrage, OneNoteSeiteAnlegenAnfrage, OneNoteSeiteLesenAnfrage,
    OneNoteSeitenAnfrage,
};
use quiver_m365::planner_tools::{
    PlannerAufgabeAendernAnfrage, PlannerAufgabeAnlegenAnfrage, PlannerAufgabenAnfrage,
    PlannerPlaeneAnfrage, TodoAufgabeAnlegenAnfrage, TodoAufgabeErledigenAnfrage,
    TodoAufgabenAnfrage,
};
use quiver_m365::{Quiver, ZUSTIMMUNG_ERWEITERT_HINWEIS};
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

    let anmeldung =
        EntraAnmeldung::mit_festem_token(konfig, tresor.clone(), AttrappenBrowser::neu(), None)
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

// ---------------------------------------------------------------------------
// SharePoint und OneDrive
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sharepoint_sites_sucht_und_zeigt_ids() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .sharepoint_sites(Parameters(SharepointSitesAnfrage {
                suche: "Projekt".to_string(),
                anzahl: None,
            }))
            .await
            .expect("Sites"),
    );
    let liste = wert.as_array().expect("Liste");
    assert_eq!(liste.len(), 3);
    assert_eq!(liste[0]["id"], graph_attrappe::SITE_NORD);
    assert_eq!(liste[0]["name"], "Projekt Nord");

    let anfrage = graph.einzige();
    assert_eq!(anfrage.pfad, "/v1.0/sites");
    assert_eq!(anfrage.query("search"), Some("Projekt"));
}

#[tokio::test]
async fn sharepoint_bibliotheken_loest_die_site_ueber_den_namen_auf() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .sharepoint_bibliotheken(Parameters(SharepointBibliothekenAnfrage {
                site: " Projekt Nord ".to_string(),
            }))
            .await
            .expect("Bibliotheken"),
    );
    let liste = wert["bibliotheken"].as_array().expect("Liste");
    assert_eq!(liste.len(), 2);
    assert_eq!(liste[0]["id"], graph_attrappe::DRIVE_DOKU);
    assert_eq!(liste[0]["name"], "Dokumente");

    let pfade: Vec<String> = graph
        .aufzeichnungen()
        .into_iter()
        .map(|anfrage| anfrage.pfad)
        .collect();
    assert_eq!(pfade.len(), 2);
    assert_eq!(pfade[0], "/v1.0/sites");
    assert!(
        pfade[1].starts_with("/v1.0/sites/") && pfade[1].ends_with("/drives"),
        "{pfade:?}"
    );
}

#[tokio::test]
async fn mehrdeutige_site_nennt_kandidaten_und_tut_nichts() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let fehler = server
        .sharepoint_bibliotheken(Parameters(SharepointBibliothekenAnfrage {
            site: "Projekt".to_string(),
        }))
        .await
        .expect_err("mehrdeutig");
    let text = fehlertext(fehler);
    assert!(text.contains("mehrdeutig"), "{text}");
    assert!(text.contains("Projekt Nord"), "{text}");
    assert!(text.contains("Projekt Nord Intern"), "{text}");
    assert!(text.contains("nichts geändert"), "{text}");

    let alle = graph.aufzeichnungen();
    assert_eq!(alle.len(), 1, "nur die Sitesuche: {alle:#?}");
    assert_eq!(alle[0].methode, "GET");
}

#[tokio::test]
async fn dateien_liste_zeigt_den_ordnerinhalt() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .dateien_liste(Parameters(DateienListeAnfrage {
                site: None,
                bibliothek: None,
                ordner: Some("Projekte".to_string()),
                anzahl: Some(10),
            }))
            .await
            .expect("Ordnerinhalt"),
    );
    let liste = wert.as_array().expect("Liste");
    assert_eq!(liste.len(), 4);
    assert_eq!(liste[0]["art"], "ordner");
    assert_eq!(liste[0]["name"], "Projekte");
    assert_eq!(liste[1]["art"], "datei");
    assert_eq!(liste[1]["typ"], "text/markdown");

    let anfrage = graph.einzige();
    assert_eq!(anfrage.pfad, "/v1.0/me/drive/root:/Projekte:/children");
    assert_eq!(anfrage.query("$top"), Some("10"));
}

#[tokio::test]
async fn datei_lesen_folgt_der_weiterleitung_ohne_token() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .datei_lesen(Parameters(DateiLesenAnfrage {
                datei: "notiz.md".to_string(),
                site: None,
                bibliothek: None,
                max_zeichen: None,
            }))
            .await
            .expect("Datei"),
    );
    assert_eq!(wert["name"], "notiz.md");
    assert_eq!(wert["inhaltstyp"], "text");
    assert_eq!(wert["text"], graph_attrappe::NOTIZ_INHALT);

    let alle = graph.aufzeichnungen();
    assert_eq!(alle.len(), 3, "Metadaten, /content, Download: {alle:#?}");
    assert_eq!(alle[0].pfad, "/v1.0/me/drive/root:/notiz.md");
    assert!(
        alle[1].pfad.ends_with("/items/item-notiz/content"),
        "{alle:#?}"
    );
    assert!(alle[2].pfad.starts_with("/download/"), "{alle:#?}");
    assert!(
        alle[2].kopf("authorization").is_none(),
        "der vor-signierte Download-Link bekommt kein Token"
    );
    assert!(
        alle[0].kopf("authorization").is_some(),
        "die Metadaten schon"
    );
}

#[tokio::test]
async fn datei_lesen_kuerzt_text_und_sagt_es() {
    let (_anmelde, _graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .datei_lesen(Parameters(DateiLesenAnfrage {
                datei: "notiz.md".to_string(),
                site: None,
                bibliothek: None,
                max_zeichen: Some(9),
            }))
            .await
            .expect("Datei"),
    );
    assert_eq!(wert["text"], "Zeile");
    assert_eq!(wert["gekuerzt"], true);
    assert!(wert["laenge_gesamt"].as_u64().unwrap_or(0) > 9, "{wert}");
}

#[tokio::test]
async fn datei_lesen_office_geht_hoechstens_als_pdf() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .datei_lesen(Parameters(DateiLesenAnfrage {
                datei: "bericht.docx".to_string(),
                site: None,
                bibliothek: None,
                max_zeichen: None,
            }))
            .await
            .expect("Datei"),
    );
    assert_eq!(wert["inhaltstyp"], "office");
    assert_eq!(wert["als_pdf"], true);
    assert!(wert.get("text").is_none(), "kein Text aus Office: {wert}");
    assert!(
        wert["hinweis"].as_str().unwrap_or_default().contains("PDF"),
        "{wert}"
    );

    let inhalt = graph
        .aufzeichnungen()
        .into_iter()
        .filter(|anfrage| anfrage.pfad.ends_with("/content"))
        .count();
    assert_eq!(inhalt, 1, "genau die PDF-Umwandlung, kein Blindflug");
}

#[tokio::test]
async fn datei_lesen_pdf_liefert_nur_metadaten() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .datei_lesen(Parameters(DateiLesenAnfrage {
                datei: "anleitung.pdf".to_string(),
                site: None,
                bibliothek: None,
                max_zeichen: None,
            }))
            .await
            .expect("Datei"),
    );
    assert_eq!(wert["inhaltstyp"], "pdf");
    assert!(wert.get("text").is_none());
    assert_eq!(
        graph.aufzeichnungen().len(),
        1,
        "nur die Metadaten, kein Download einer Binärdatei"
    );
}

#[tokio::test]
async fn datei_hochladen_setzt_rename_als_standard() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .datei_hochladen(Parameters(DateiHochladenAnfrage {
                name: "Notiz.md".to_string(),
                inhalt: "# Neu".to_string(),
                ordner: Some("Projekte".to_string()),
                site: None,
                bibliothek: None,
                verhalten: None,
            }))
            .await
            .expect("hochgeladen"),
    );
    assert_eq!(wert["id"], "neu-datei");
    assert_eq!(wert["name"], "Notiz.md");
    assert_eq!(wert["verhalten"], "rename");

    let post = graph.einzige();
    assert_eq!(post.methode, "PUT");
    assert_eq!(post.pfad, "/v1.0/me/drive/root:/Projekte/Notiz.md:/content");
    assert_eq!(
        post.query("@microsoft.graph.conflictBehavior"),
        Some("rename")
    );
    assert_eq!(post.roher_body.as_deref(), Some("# Neu"));
    assert_eq!(
        post.kopf("content-type"),
        Some("text/markdown; charset=utf-8")
    );
}

#[tokio::test]
async fn datei_hochladen_ueber_4_mb_wird_ohne_anfrage_abgelehnt() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let zu_gross = "a".repeat(4 * 1024 * 1024 + 1);
    let fehler = server
        .datei_hochladen(Parameters(DateiHochladenAnfrage {
            name: "gross.txt".to_string(),
            inhalt: zu_gross,
            ordner: None,
            site: Some("Projekt Nord".to_string()),
            bibliothek: None,
            verhalten: None,
        }))
        .await
        .expect_err("zu groß");
    let text = fehlertext(fehler);
    assert!(text.contains("4 MB"), "{text}");
    assert!(
        graph.aufzeichnungen().is_empty(),
        "keine einzige Anfrage – auch die Site wird nicht aufgelöst"
    );
}

#[tokio::test]
async fn ordner_anlegen_in_der_bibliothek() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .ordner_anlegen(Parameters(OrdnerAnlegenAnfrage {
                name: "Archiv".to_string(),
                ordner: None,
                site: Some("Projekt Nord".to_string()),
                bibliothek: Some("Dokumente".to_string()),
            }))
            .await
            .expect("angelegt"),
    );
    assert_eq!(wert["id"], "neu-ordner");
    assert_eq!(wert["name"], "Archiv");
    assert_eq!(wert["art"], "ordner");

    let alle = graph.aufzeichnungen();
    let post = alle
        .iter()
        .find(|anfrage| anfrage.methode == "POST")
        .expect("POST");
    assert_eq!(
        post.pfad,
        format!("/v1.0/drives/{}/root/children", graph_attrappe::DRIVE_DOKU)
    );
    assert_eq!(post.body()["name"], "Archiv");
    assert_eq!(post.body()["@microsoft.graph.conflictBehavior"], "fail");
}

#[tokio::test]
async fn sharepoint_listen_und_eintraege() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .sharepoint_listen(Parameters(SharepointListenAnfrage {
                site: "Projekt Nord".to_string(),
            }))
            .await
            .expect("Listen"),
    );
    let listen = wert["listen"].as_array().expect("Listen");
    assert_eq!(listen.len(), 2);
    assert_eq!(listen[0]["id"], graph_attrappe::LISTE_AUFGABEN);
    assert_eq!(listen[0]["vorlage"], "genericList");

    let wert = json(
        &server
            .sharepoint_listeneintraege(Parameters(SharepointListeneintraegeAnfrage {
                site: "Projekt Nord".to_string(),
                liste: "Aufgaben".to_string(),
                anzahl: Some(10),
                filter: Some("fields/Status eq 'offen'".to_string()),
            }))
            .await
            .expect("Einträge"),
    );
    let eintraege = wert["eintraege"].as_array().expect("Einträge");
    assert_eq!(eintraege.len(), 2);
    assert_eq!(eintraege[0]["felder"]["Titel"], "Erster");
    assert_eq!(eintraege[0]["felder"]["Status"], "offen");

    let letzte = graph.aufzeichnungen().pop().expect("Anfrage");
    assert!(letzte.pfad.ends_with("/items"), "{letzte:#?}");
    assert_eq!(letzte.query("$expand"), Some("fields"));
    assert_eq!(letzte.query("$filter"), Some("fields/Status eq 'offen'"));
}

#[tokio::test]
async fn listeneintrag_anlegen_und_aendern() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .sharepoint_listeneintrag_anlegen(Parameters(SharepointListeneintragAnlegenAnfrage {
                site: graph_attrappe::SITE_NORD.to_string(),
                liste: graph_attrappe::LISTE_AUFGABEN.to_string(),
                felder: json!({"Titel": "Dritter", "Status": "offen"}),
            }))
            .await
            .expect("angelegt"),
    );
    assert_eq!(wert["id"], "3");
    assert_eq!(wert["felder"]["Titel"], "Dritter");

    let post = graph.einzige();
    assert_eq!(post.methode, "POST");
    assert_eq!(post.body()["fields"]["Titel"], "Dritter");

    let wert = json(
        &server
            .sharepoint_listeneintrag_aendern(Parameters(SharepointListeneintragAendernAnfrage {
                site: graph_attrappe::SITE_NORD.to_string(),
                liste: graph_attrappe::LISTE_AUFGABEN.to_string(),
                eintrag: "1".to_string(),
                felder: json!({"Status": "fertig"}),
            }))
            .await
            .expect("geändert"),
    );
    assert_eq!(wert["geaendert"], true);
    assert_eq!(wert["felder"]["Status"], "fertig");

    let alle = graph.aufzeichnungen();
    let patch = alle.last().expect("PATCH");
    assert_eq!(patch.methode, "PATCH");
    assert!(
        patch.pfad.ends_with("/items/1/fields"),
        "Pfad: {}",
        patch.pfad
    );
    assert_eq!(patch.body()["Status"], "fertig");
}

#[tokio::test]
async fn leere_felder_werden_abgelehnt() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let fehler = server
        .sharepoint_listeneintrag_anlegen(Parameters(SharepointListeneintragAnlegenAnfrage {
            site: graph_attrappe::SITE_NORD.to_string(),
            liste: graph_attrappe::LISTE_AUFGABEN.to_string(),
            felder: json!({}),
        }))
        .await
        .expect_err("leer");
    assert!(fehlertext(fehler).contains("JSON-Objekt"));
    assert!(graph.aufzeichnungen().is_empty(), "keine Anfrage");
}

// ---------------------------------------------------------------------------
// Planner und To Do
// ---------------------------------------------------------------------------

#[tokio::test]
async fn planner_plaene_gesamt_und_je_gruppe() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .planner_plaene(Parameters(PlannerPlaeneAnfrage {
                gruppe: None,
                anzahl: None,
            }))
            .await
            .expect("Pläne"),
    );
    let plaene = wert.as_array().expect("Liste");
    assert_eq!(plaene.len(), 2);
    assert_eq!(plaene[0]["id"], graph_attrappe::PLAN_NORD);
    assert_eq!(plaene[0]["titel"], "Projekt Nord");
    assert_eq!(graph.einzige().pfad, "/v1.0/me/planner/plans");

    let wert = json(
        &server
            .planner_plaene(Parameters(PlannerPlaeneAnfrage {
                gruppe: Some("Entwicklung".to_string()),
                anzahl: None,
            }))
            .await
            .expect("Pläne der Gruppe"),
    );
    assert_eq!(wert.as_array().expect("Liste").len(), 2);

    let alle = graph.aufzeichnungen();
    assert_eq!(alle[0].pfad, "/v1.0/me/planner/plans");
    assert_eq!(alle[1].pfad, "/v1.0/groups");
    assert_eq!(
        alle[2].pfad, "/v1.0/groups/group-1/planner/plans",
        "die Gruppe wird zur Id aufgelöst"
    );
}

#[tokio::test]
async fn planner_aufgaben_mit_buckets_und_filter() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .planner_aufgaben(Parameters(PlannerAufgabenAnfrage {
                plan: Some("Projekt Nord".to_string()),
                bucket: Some("Erledigt".to_string()),
                anzahl: None,
            }))
            .await
            .expect("Aufgaben"),
    );
    let aufgaben = wert["aufgaben"].as_array().expect("Aufgaben");
    assert_eq!(aufgaben.len(), 1, "nur der Bucket „Erledigt“");
    assert_eq!(aufgaben[0]["id"], "task-2");
    assert_eq!(aufgaben[0]["bucket"], "Erledigt");

    let buckets: Vec<&str> = wert["buckets"]
        .as_array()
        .expect("Buckets")
        .iter()
        .filter_map(|bucket| bucket["name"].as_str())
        .collect();
    assert_eq!(buckets, vec!["Zu erledigen", "Erledigt"]);
    assert!(
        graph
            .aufzeichnungen()
            .iter()
            .any(|anfrage| anfrage.pfad == "/v1.0/planner/plans/plan-1/buckets"),
        "die Buckets werden zum Plan geholt"
    );
}

#[tokio::test]
async fn planner_aufgabe_anlegen_mit_faelligkeit_und_zuweisung() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .planner_aufgabe_anlegen(Parameters(PlannerAufgabeAnlegenAnfrage {
                plan: graph_attrappe::PLAN_NORD.to_string(),
                titel: "Angebot schreiben".to_string(),
                bucket: Some("Zu erledigen".to_string()),
                faellig: Some("2026-11-30".to_string()),
                zugewiesen: Some("bert@firma.de".to_string()),
            }))
            .await
            .expect("angelegt"),
    );
    assert_eq!(wert["id"], "task-neu");
    assert_eq!(wert["angelegt"], true);
    assert_eq!(wert["bucket"], "Zu erledigen");

    let alle = graph.aufzeichnungen();
    let post = alle
        .iter()
        .find(|anfrage| anfrage.methode == "POST")
        .expect("POST");
    assert_eq!(post.pfad, "/v1.0/planner/tasks");
    assert_eq!(post.body()["planId"], graph_attrappe::PLAN_NORD);
    assert_eq!(post.body()["bucketId"], graph_attrappe::BUCKET_OFFEN);
    assert_eq!(post.body()["dueDateTime"], "2026-11-30T00:00:00Z");
    assert!(
        post.body()["assignments"].get("u-bert").is_some(),
        "die Person wird zur Benutzer-Id: {}",
        post.body()
    );
}

#[tokio::test]
async fn planner_aufgabe_aendern_mit_etag_setzt_if_match() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .planner_aufgabe_aendern(Parameters(PlannerAufgabeAendernAnfrage {
                aufgabe: graph_attrappe::AUFGABE_MIT_ETAG.to_string(),
                fortschritt: Some(100),
                faellig: Some("2026-12-01".to_string()),
                zugewiesen: None,
            }))
            .await
            .expect("geändert"),
    );
    assert_eq!(wert["geaendert"], true);
    assert_eq!(wert["fortschritt"], 100);

    let alle = graph.aufzeichnungen();
    assert_eq!(alle.len(), 2, "erst lesen, dann ändern: {alle:#?}");
    assert_eq!(alle[0].pfad, "/v1.0/planner/tasks/task-1");
    let patch = &alle[1];
    assert_eq!(patch.methode, "PATCH");
    assert_eq!(
        patch.kopf("if-match"),
        Some("W/\"task-1\""),
        "Planner verlangt den ETag in If-Match"
    );
    assert_eq!(patch.body()["percentComplete"], 100);
    assert_eq!(patch.body()["dueDateTime"], "2026-12-01T00:00:00Z");
}

#[tokio::test]
async fn planner_aufgabe_ohne_etag_erzeugt_keinen_patch() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let fehler = server
        .planner_aufgabe_aendern(Parameters(PlannerAufgabeAendernAnfrage {
            aufgabe: graph_attrappe::AUFGABE_OHNE_ETAG.to_string(),
            fortschritt: Some(50),
            faellig: None,
            zugewiesen: None,
        }))
        .await
        .expect_err("kein ETag");
    let text = fehlertext(fehler);
    assert!(text.contains("ohne ETag"), "{text}");
    assert!(text.contains("nichts geändert"), "{text}");

    let alle = graph.aufzeichnungen();
    assert_eq!(alle.len(), 1, "nur das Lesen: {alle:#?}");
    assert_eq!(alle[0].methode, "GET");
    assert!(!alle.iter().any(|anfrage| anfrage.methode == "PATCH"));
}

#[tokio::test]
async fn planner_aufgabe_aendern_ohne_felder_tut_nichts() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let fehler = server
        .planner_aufgabe_aendern(Parameters(PlannerAufgabeAendernAnfrage {
            aufgabe: graph_attrappe::AUFGABE_MIT_ETAG.to_string(),
            fortschritt: None,
            faellig: None,
            zugewiesen: None,
        }))
        .await
        .expect_err("nichts zu ändern");
    assert!(fehlertext(fehler).contains("nichts zu ändern"));
    assert!(graph.aufzeichnungen().is_empty(), "keine Anfrage");
}

#[tokio::test]
async fn mehrdeutige_person_verhindert_das_aendern() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let fehler = server
        .planner_aufgabe_aendern(Parameters(PlannerAufgabeAendernAnfrage {
            aufgabe: graph_attrappe::AUFGABE_MIT_ETAG.to_string(),
            fortschritt: None,
            faellig: None,
            zugewiesen: Some("Anna".to_string()),
        }))
        .await
        .expect_err("mehrdeutig");
    let text = fehlertext(fehler);
    assert!(text.contains("mehrdeutig"), "{text}");
    assert!(text.contains("Anna Muster"), "{text}");
    assert!(text.contains("Anna Anders"), "{text}");

    let alle = graph.aufzeichnungen();
    assert_eq!(alle.len(), 1, "nur die Personensuche: {alle:#?}");
    assert!(alle.iter().all(|anfrage| anfrage.methode == "GET"));
}

#[tokio::test]
async fn todo_listen_aufgaben_und_nur_offen() {
    let (_anmelde, _graph, server, _tresor) = aufbau().await;

    let wert = json(&server.todo_listen().await.expect("Listen"));
    let listen = wert.as_array().expect("Liste");
    assert_eq!(listen[0]["id"], graph_attrappe::TODO_LISTE);
    assert_eq!(listen[0]["name"], "Aufgaben");

    let wert = json(
        &server
            .todo_aufgaben(Parameters(TodoAufgabenAnfrage {
                liste: "Aufgaben".to_string(),
                nur_offen: Some(true),
                anzahl: None,
            }))
            .await
            .expect("Aufgaben"),
    );
    let aufgaben = wert["aufgaben"].as_array().expect("Aufgaben");
    assert_eq!(aufgaben.len(), 1, "die erledigte Aufgabe fällt weg");
    assert_eq!(aufgaben[0]["titel"], "Rechnung prüfen");
    assert_eq!(aufgaben[0]["text"], "Bis Freitag");
}

#[tokio::test]
async fn todo_listen_ohne_top_und_select_ueber_next_link() {
    // `/me/todo/lists` verträgt weder `$top` noch `$select` (invalidRequest);
    // geblättert wird über `@odata.nextLink` (Aufgabe 07).
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(&server.todo_listen().await.expect("Listen"));
    let ids: Vec<&str> = wert
        .as_array()
        .expect("Liste")
        .iter()
        .filter_map(|liste| liste["id"].as_str())
        .collect();
    assert_eq!(
        ids,
        vec![
            graph_attrappe::TODO_LISTE,
            graph_attrappe::TODO_LISTE_EINKAUF
        ],
        "beide Seiten über den nextLink"
    );

    let abfragen: Vec<graph_attrappe::Aufzeichnung> = graph
        .aufzeichnungen()
        .into_iter()
        .filter(|anfrage| anfrage.pfad == "/v1.0/me/todo/lists")
        .collect();
    assert_eq!(abfragen.len(), 2, "zwei Seiten: {abfragen:#?}");
    for anfrage in &abfragen {
        assert!(anfrage.query("$top").is_none(), "kein $top: {anfrage:#?}");
        assert!(
            anfrage.query("$select").is_none(),
            "kein $select: {anfrage:#?}"
        );
    }
    assert_eq!(abfragen[1].query("seite"), Some("2"));
}

#[tokio::test]
async fn todo_aufgaben_ohne_select_mit_top() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    json(
        &server
            .todo_aufgaben(Parameters(TodoAufgabenAnfrage {
                liste: graph_attrappe::TODO_LISTE.to_string(),
                nur_offen: None,
                anzahl: None,
            }))
            .await
            .expect("Aufgaben"),
    );

    let abfrage = graph
        .aufzeichnungen()
        .into_iter()
        .find(|anfrage| anfrage.pfad.ends_with("/tasks"))
        .expect("Aufgabenliste");
    assert!(
        abfrage.query("$select").is_none(),
        "kein $select auf Aufgaben: {abfrage:#?}"
    );
    assert_eq!(abfrage.query("$top"), Some("20"), "$top ist erlaubt");
}

#[tokio::test]
async fn todo_aufgabe_anlegen_und_erledigen() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .todo_aufgabe_anlegen(Parameters(TodoAufgabeAnlegenAnfrage {
                liste: graph_attrappe::TODO_LISTE.to_string(),
                titel: "Kaffee bestellen".to_string(),
                faellig: Some("2026-11-30".to_string()),
                wichtigkeit: Some("hoch".to_string()),
                text: Some("Für das Team".to_string()),
            }))
            .await
            .expect("angelegt"),
    );
    assert_eq!(wert["id"], "todo-neu");
    assert_eq!(wert["angelegt"], true);

    let post = graph.einzige();
    assert_eq!(post.methode, "POST");
    assert_eq!(post.body()["importance"], "high");
    assert_eq!(post.body()["dueDateTime"]["timeZone"], "Europe/Berlin");
    assert_eq!(
        post.body()["dueDateTime"]["dateTime"],
        "2026-11-30T00:00:00"
    );
    assert_eq!(post.body()["body"]["content"], "Für das Team");

    let wert = json(
        &server
            .todo_aufgabe_erledigen(Parameters(TodoAufgabeErledigenAnfrage {
                liste: graph_attrappe::TODO_LISTE.to_string(),
                aufgabe: graph_attrappe::TODO_AUFGABE.to_string(),
                erledigt: None,
            }))
            .await
            .expect("erledigt"),
    );
    assert_eq!(wert["erledigt"], true);
    assert_eq!(wert["status"], "completed");

    let alle = graph.aufzeichnungen();
    let patch = alle.last().expect("PATCH");
    assert_eq!(patch.methode, "PATCH");
    assert_eq!(
        patch.pfad,
        format!(
            "/v1.0/me/todo/lists/{}/tasks/{}",
            graph_attrappe::TODO_LISTE,
            graph_attrappe::TODO_AUFGABE
        )
    );
    assert_eq!(patch.body()["status"], "completed");
}

// ---------------------------------------------------------------------------
// Kontakte, Personen, OneNote
// ---------------------------------------------------------------------------

#[tokio::test]
async fn kontakte_suchen_und_anlegen() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .kontakte_suchen(Parameters(KontakteSuchenAnfrage {
                suche: "Bert".to_string(),
                anzahl: None,
            }))
            .await
            .expect("Kontakte"),
    );
    let liste = wert.as_array().expect("Liste");
    assert_eq!(liste[0]["name"], "Bert Beispiel");
    assert_eq!(liste[0]["mails"], json!(["Bert <bert@firma.de>"]));
    assert_eq!(liste[0]["telefone"], json!(["+49 30 1234"]));

    let anfrage = graph.einzige();
    assert_eq!(anfrage.pfad, "/v1.0/me/contacts");
    assert_eq!(anfrage.query("$search"), Some("\"Bert\""));

    let wert = json(
        &server
            .kontakt_anlegen(Parameters(KontaktAnlegenAnfrage {
                name: "Clara Claudia".to_string(),
                vorname: Some("Clara".to_string()),
                nachname: Some("Claudia".to_string()),
                mails: Some(vec!["clara@firma.de".to_string()]),
                telefone: Some(vec!["+49 30 99".to_string()]),
                mobil: None,
                firma: Some("Beispiel GmbH".to_string()),
                position: None,
            }))
            .await
            .expect("angelegt"),
    );
    assert_eq!(wert["angelegt"], true);
    assert_eq!(wert["name"], "Clara Claudia");

    let alle = graph.aufzeichnungen();
    let post = alle.last().expect("POST");
    assert_eq!(post.methode, "POST");
    assert_eq!(post.pfad, "/v1.0/me/contacts");
    assert_eq!(post.body()["givenName"], "Clara");
    assert_eq!(
        post.body()["emailAddresses"][0]["address"],
        "clara@firma.de"
    );
}

#[tokio::test]
async fn personen_suchen_zeigt_abteilung_und_telefon() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .personen_suchen(Parameters(PersonenSuchenAnfrage {
                suche: "Bert".to_string(),
                anzahl: None,
            }))
            .await
            .expect("Personen"),
    );
    let liste = wert.as_array().expect("Liste");
    assert_eq!(liste[0]["name"], "Bert Beispiel");
    assert_eq!(liste[0]["abteilung"], "Vertrieb");
    assert_eq!(liste[0]["mails"], json!(["bert@firma.de"]));
    assert_eq!(liste[0]["telefone"], json!(["+49 30 1234 (business)"]));

    let anfrage = graph.einzige();
    assert_eq!(anfrage.pfad, "/v1.0/me/people");
    assert_eq!(anfrage.query("$search"), Some("\"Bert\""));
}

#[tokio::test]
async fn onenote_notizbuecher_abschnitte_und_seiten() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(&server.onenote_notizbuecher().await.expect("Notizbücher"));
    assert_eq!(wert[0]["id"], graph_attrappe::NOTIZBUCH);
    assert_eq!(wert[0]["name"], "Arbeit");
    // OneNote kennt kein `webUrl`; der Link steht unter `links.oneNoteWebUrl.href`
    // (Aufgabe 07). Die Attrappe lehnt `webUrl` in `$select` mit 400 ab.
    assert_eq!(wert[0]["link"], "https://example/onenote/arbeit");

    let wert = json(
        &server
            .onenote_abschnitte(Parameters(OneNoteAbschnitteAnfrage {
                notizbuch: Some("Arbeit".to_string()),
            }))
            .await
            .expect("Abschnitte"),
    );
    assert_eq!(
        wert["notizbuch"],
        format!("Arbeit ({})", graph_attrappe::NOTIZBUCH)
    );
    assert_eq!(wert["abschnitte"][0]["name"], "Besprechungen");
    assert_eq!(
        wert["abschnitte"][0]["link"],
        "https://example/onenote/besprechungen"
    );

    let wert = json(
        &server
            .onenote_seiten(Parameters(OneNoteSeitenAnfrage {
                abschnitt: "Besprechungen".to_string(),
                anzahl: None,
            }))
            .await
            .expect("Seiten"),
    );
    assert_eq!(wert["seiten"][0]["id"], graph_attrappe::ONENOTE_SEITE);
    assert_eq!(wert["seiten"][0]["titel"], "Jour fixe");
    // Auch bei Seiten kommt der Link aus `links.oneNoteWebUrl.href` (Aufgabe 08).
    assert_eq!(
        wert["seiten"][0]["link"],
        "https://example/onenote/jour-fixe"
    );

    let pfade: Vec<String> = graph
        .aufzeichnungen()
        .into_iter()
        .map(|anfrage| anfrage.pfad)
        .collect();
    assert_eq!(
        pfade,
        vec![
            "/v1.0/me/onenote/notebooks".to_string(),
            "/v1.0/me/onenote/notebooks".to_string(),
            format!(
                "/v1.0/me/onenote/notebooks/{}/sections",
                graph_attrappe::NOTIZBUCH
            ),
            "/v1.0/me/onenote/sections".to_string(),
            format!(
                "/v1.0/me/onenote/sections/{}/pages",
                graph_attrappe::ABSCHNITT
            ),
        ]
    );

    // Kein `webUrl` in `$select` – sonst antwortet Graph mit 400.
    let selects: Vec<String> = graph
        .aufzeichnungen()
        .into_iter()
        .filter(|anfrage| {
            anfrage.pfad == "/v1.0/me/onenote/notebooks"
                || anfrage.pfad.ends_with("/sections")
                || anfrage.pfad.ends_with("/pages")
        })
        .filter_map(|anfrage| anfrage.query("$select").map(str::to_string))
        .collect();
    assert_eq!(selects.len(), 5, "fünf Listenabfragen mit $select");
    for select in &selects {
        assert!(
            !select.contains("webUrl"),
            "kein webUrl in $select: {select}"
        );
    }
}

#[tokio::test]
async fn onenote_seite_lesen_wandelt_html_und_kuerzt() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .onenote_seite_lesen(Parameters(OneNoteSeiteLesenAnfrage {
                seite: graph_attrappe::ONENOTE_SEITE.to_string(),
                max_zeichen: None,
            }))
            .await
            .expect("Seite"),
    );
    assert_eq!(wert["titel"], "Jour fixe");
    assert_eq!(wert["text"], "Erste Zeile\nZweite Zeile");
    // Der Link der Seite kommt aus `links.oneNoteWebUrl.href` (Aufgabe 08).
    assert_eq!(wert["link"], "https://example/onenote/jour-fixe");

    let kopf = graph.aufzeichnungen().remove(0);
    let select = kopf.query("$select").expect("$select");
    assert!(
        !select.contains("webUrl"),
        "kein webUrl in $select: {select}"
    );

    let wert = json(
        &server
            .onenote_seite_lesen(Parameters(OneNoteSeiteLesenAnfrage {
                seite: graph_attrappe::ONENOTE_SEITE.to_string(),
                max_zeichen: Some(13),
            }))
            .await
            .expect("Seite"),
    );
    assert_eq!(wert["text"], "Erste Zeile");
    assert_eq!(wert["gekuerzt"], true);
    assert!(wert["laenge_gesamt"].as_u64().unwrap_or(0) > 13);
}

#[tokio::test]
async fn onenote_seite_anlegen_escapet_titel_und_text() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;

    let wert = json(
        &server
            .onenote_seite_anlegen(Parameters(OneNoteSeiteAnlegenAnfrage {
                abschnitt: "Besprechungen".to_string(),
                titel: "Bericht <1>".to_string(),
                text: "Milch & Brot\n<b>fett</b>".to_string(),
            }))
            .await
            .expect("angelegt"),
    );
    assert_eq!(wert["angelegt"], true);
    assert_eq!(
        wert["abschnitt"],
        format!("Besprechungen ({})", graph_attrappe::ABSCHNITT)
    );

    let alle = graph.aufzeichnungen();
    let post = alle.last().expect("POST");
    assert_eq!(post.methode, "POST");
    assert_eq!(post.kopf("content-type"), Some("text/html"));
    let html = post.roher_body.as_deref().expect("HTML-Körper");
    assert!(html.contains("<title>Bericht &lt;1&gt;</title>"), "{html}");
    assert!(
        html.contains("Milch &amp; Brot<br>&lt;b&gt;fett&lt;/b&gt;"),
        "{html}"
    );
    assert!(!html.contains("<b>fett</b>"), "kein rohes HTML: {html}");
}

// ---------------------------------------------------------------------------
// Regeln: 429, Zustimmung
// ---------------------------------------------------------------------------

#[tokio::test]
async fn nach_429_wird_ein_post_nicht_wiederholt() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;
    graph.einmal_429(0);

    let fehler = server
        .ordner_anlegen(Parameters(OrdnerAnlegenAnfrage {
            name: "Archiv".to_string(),
            ordner: None,
            site: None,
            bibliothek: None,
        }))
        .await
        .expect_err("429 bleibt ein Fehler");
    assert!(fehlertext(fehler).contains("429"), "Graph-Fehler 429");

    let alle = graph.aufzeichnungen();
    assert_eq!(alle.len(), 1, "genau ein POST: {alle:#?}");
    assert_eq!(alle[0].methode, "POST");
}

#[tokio::test]
async fn ein_lesender_aufruf_wiederholt_nach_429() {
    let (_anmelde, graph, server, _tresor) = aufbau().await;
    graph.einmal_429(0);

    json(
        &server
            .sharepoint_sites(Parameters(SharepointSitesAnfrage {
                suche: "Projekt".to_string(),
                anzahl: None,
            }))
            .await
            .expect("klappt nach dem Warten"),
    );
    assert_eq!(graph.aufzeichnungen().len(), 2, "einmal wiederholt");
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

    let text = server
        .sharepoint_sites(Parameters(SharepointSitesAnfrage {
            suche: "Projekt".to_string(),
            anzahl: None,
        }))
        .await
        .expect("Hinweis, kein Fehler");
    assert_eq!(text, ZUSTIMMUNG_ERWEITERT_HINWEIS);
    assert!(text.contains("SharePoint"), "{text}");
    assert!(text.contains("Anmelden"), "{text}");

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
async fn die_attrappe_haelt_die_onenote_und_todo_regeln_ein() {
    // Ohne diese Regeln bliebe der Live-Test-Fehler im Test unsichtbar (Aufgabe 07).
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
            .get(
                "/me/onenote/notebooks",
                &[("$select", "id,displayName,webUrl".to_string())],
            )
            .await,
    );
    assert_eq!(status_code, 400, "kein webUrl bei Notizbüchern");
    assert!(code.contains("BadRequest"), "{code}");

    let (status_code, _) = status(
        client
            .get(
                "/me/onenote/sections",
                &[("$select", "id,displayName,webUrl".to_string())],
            )
            .await,
    );
    assert_eq!(status_code, 400, "kein webUrl bei Abschnitten");

    // Auch Seiten kennen kein `webUrl` (Aufgabe 08).
    let (status_code, text) = match client
        .get::<Value>(
            &format!("/me/onenote/sections/{}/pages", graph_attrappe::ABSCHNITT),
            &[("$select", "id,title,webUrl".to_string())],
        )
        .await
    {
        Err(m365_graph::GraphFehler::Graph { status, text, .. }) => (status, text),
        anderes => panic!("400 erwartet, kam: {anderes:?}"),
    };
    assert_eq!(status_code, 400, "kein webUrl bei Seiten");
    assert!(text.contains("microsoft.graph.onenotePage"), "{text}");

    let (status_code, _) = status(
        client
            .get(
                &format!("/me/onenote/pages/{}", graph_attrappe::ONENOTE_SEITE),
                &[("$select", "id,title,webUrl".to_string())],
            )
            .await,
    );
    assert_eq!(status_code, 400, "kein webUrl bei einer Seite");

    for (name, wert) in [("$top", "10"), ("$select", "id,displayName")] {
        let (status_code, code) = status(
            client
                .get("/me/todo/lists", &[(name, wert.to_string())])
                .await,
        );
        assert_eq!(status_code, 400, "{name} auf todo/lists");
        assert!(code.contains("invalidRequest"), "{code}");
    }

    let (status_code, _) = status(
        client
            .get(
                &format!("/me/todo/lists/{}/tasks", graph_attrappe::TODO_LISTE),
                &[("$select", "id,title".to_string())],
            )
            .await,
    );
    assert_eq!(status_code, 400, "kein $select auf Aufgaben");
}
