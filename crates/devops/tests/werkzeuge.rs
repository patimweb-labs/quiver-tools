// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Werkzeuge gegen die DevOps-Attrappe (SPEC §7, Aufgabe 09).
//!
//! Jeder Fall prüft eine Wirkung: was bei Azure DevOps gefragt wurde (von der
//! Attrappe aufgezeichnet) und was beim Agenten ankommt. Ein echtes DevOps ist
//! nie im Spiel.

mod attrappe;

use attrappe::{Attrappe, ITEM, LAUF, LOG, PAT_A, PAT_B, PIPELINE, PR, PROJEKT, REPO, TEAM};
use quiver_devops::organisationen::Sammlung;
use quiver_devops::{Devops, DevopsServer};
use quiver_tools_kern::zugaenge::Zugang as Eintrag;
use quiver_tools_kern::Zugang;
use rmcp::model::CallToolResult;
use rmcp::ErrorData as McpError;
use serde_json::{json, Value};

/// Eine Organisation für die Attrappe.
fn eintrag(
    name: &str,
    adresse: &str,
    projekt: Option<&str>,
    pat: &str,
    schreiben: bool,
) -> Eintrag {
    let mut werte: std::collections::BTreeMap<String, String> = [
        ("url", adresse),
        ("pat", pat),
        ("schreiben", if schreiben { "an" } else { "aus" }),
    ]
    .into_iter()
    .map(|(feld, wert)| (feld.to_string(), wert.to_string()))
    .collect();
    if let Some(projekt) = projekt {
        werte.insert("projekt".to_string(), projekt.to_string());
    }
    Eintrag {
        name: name.to_string(),
        werte,
    }
}

fn server(eintraege: Vec<Eintrag>) -> DevopsServer {
    DevopsServer::mit(
        Zugang::mit_umgebung(&Devops, |_| None),
        Ok(Sammlung::aus_zugaengen(eintraege).unwrap()),
    )
}

/// Eine Organisation mit festem Projekt; Schreiben ist an.
fn eine(adresse: &str) -> DevopsServer {
    server(vec![eintrag(
        "neoimpulse",
        adresse,
        Some(PROJEKT),
        PAT_A,
        true,
    )])
}

/// Eine Organisation mit festem Projekt; Schreiben ist **aus**.
fn eine_ohne_schreiben(adresse: &str) -> DevopsServer {
    server(vec![eintrag(
        "neoimpulse",
        adresse,
        Some(PROJEKT),
        PAT_A,
        false,
    )])
}

/// Zwei Organisationen: die erste mit Projekt, die zweite ohne (Projekt je Aufruf).
fn zwei(adresse: &str) -> DevopsServer {
    server(vec![
        eintrag("neoimpulse", adresse, Some(PROJEKT), PAT_A, true),
        eintrag("kunde", adresse, None, PAT_B, false),
    ])
}

async fn aufruf(
    server: &DevopsServer,
    name: &str,
    argumente: Value,
) -> Result<CallToolResult, McpError> {
    let objekt = argumente.as_object().cloned().unwrap_or_default();
    server.aufrufen(name, Some(objekt)).await
}

fn text(ergebnis: CallToolResult) -> String {
    ergebnis.content[0]
        .as_text()
        .expect("keine Textantwort")
        .text
        .clone()
}

async fn antwort(server: &DevopsServer, name: &str, argumente: Value) -> String {
    text(
        aufruf(server, name, argumente)
            .await
            .expect("unerwarteter Fehler"),
    )
}

/// Alle Antworten dieses Tests dürfen keinen PAT enthalten.
fn ohne_pat(text: &str) {
    assert!(!text.contains(PAT_A), "PAT in der Antwort: {text}");
    assert!(!text.contains(PAT_B), "PAT in der Antwort: {text}");
}

// ------------------------------------------------------------------ Zugang

#[tokio::test]
async fn status_zeigt_die_organisationen_ohne_pat() {
    let attrappe = Attrappe::starte().await;
    let server = zwei(&attrappe.adresse);

    let text = antwort(&server, "devops_status", json!({})).await;
    assert!(text.contains("2 Organisation(en)"), "{text}");
    assert!(text.contains("neoimpulse"), "{text}");
    assert!(text.contains("kunde"), "{text}");
    assert!(text.contains("Schreiben aus"), "{text}");
    assert!(text.contains("Schreiben an"), "{text}");
    ohne_pat(&text);
    // `devops_status` öffnet keine Verbindung.
    assert!(attrappe.aufzeichnung.alle().is_empty());
}

#[tokio::test]
async fn testen_meldet_sich_an_und_nennt_den_benutzer() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(&server, "devops_testen", json!({})).await;
    assert!(
        text.contains("Anmeldung an neoimpulse erfolgreich"),
        "{text}"
    );
    assert!(text.contains(attrappe::BENUTZER), "{text}");
    assert!(text.contains("Konto neoimpulse"), "{text}");
    ohne_pat(&text);

    // Genau eine Anfrage, mit Vorschau-`api-version` und Basic-Anmeldung.
    assert_eq!(attrappe.aufzeichnung.anzahl("connectionData"), 1);
    let aufruf = attrappe.aufzeichnung.letzter().unwrap();
    // `connectionData` ist Vorschau: mit `7.1` antwortet DevOps mit `400`.
    assert_eq!(aufruf.api_version(), Some(attrappe::VORSCHAU_VERSION));
    assert!(aufruf.anmeldung.unwrap().starts_with("Basic "));
}

#[tokio::test]
async fn testen_meldet_einen_400_auf_connectiondata_als_fehler() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    // Der Dienst antwortet mit `400` (falsche `api-version`, Vorschau): das ist
    // ein Fehler und keine Anmeldung als „unbekannt“.
    attrappe.verbindungsdaten_ablehnen();
    let fehler = aufruf(&server, "devops_testen", json!({}))
        .await
        .unwrap_err();
    assert!(fehler.message.contains("400"), "{}", fehler.message);
    assert!(fehler.message.contains("preview"), "{}", fehler.message);
    ohne_pat(&fehler.message);
}

#[tokio::test]
async fn ohne_zugang_kommt_der_hinweis_und_kein_fehler() {
    let server = server(vec![]);
    let text = antwort(&server, "devops_repos", json!({})).await;
    assert!(text.contains("Zugang fehlt"), "{text}");
    assert!(text.contains("Tools → DevOps"), "{text}");
}

#[tokio::test]
async fn eine_organisation_darf_ohne_namen_gewaehlt_werden() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);
    let text = antwort(&server, "devops_repos", json!({})).await;
    assert!(text.contains(REPO), "{text}");
}

#[tokio::test]
async fn zwei_organisationen_verlangen_einen_namen() {
    let attrappe = Attrappe::starte().await;
    let server = zwei(&attrappe.adresse);

    let fehler = aufruf(&server, "devops_repos", json!({}))
        .await
        .unwrap_err();
    assert!(
        fehler.message.contains("mehrere Organisationen"),
        "{}",
        fehler.message
    );
    assert!(fehler.message.contains("neoimpulse"), "{}", fehler.message);
    assert!(attrappe.aufzeichnung.alle().is_empty());

    // Unbekannt → Hinweis mit den bekannten Namen, ohne Anfrage.
    let fehler = aufruf(
        &server,
        "devops_repos",
        json!({ "verbindung": "gibtsnicht" }),
    )
    .await
    .unwrap_err();
    assert!(fehler.message.contains("gibtsnicht"), "{}", fehler.message);
    assert!(fehler.message.contains("kunde"), "{}", fehler.message);
    assert!(attrappe.aufzeichnung.alle().is_empty());

    // Mit Namen klappt es (die zweite Organisation braucht kein Projekt).
    let text = antwort(&server, "devops_projekte", json!({ "verbindung": "kunde" })).await;
    assert!(text.contains("2 Projekt(e)"), "{text}");
}

#[tokio::test]
async fn das_projekt_kommt_aus_dem_zugang_oder_aus_dem_aufruf() {
    let attrappe = Attrappe::starte().await;
    let server = zwei(&attrappe.adresse);

    // Aus dem Zugang.
    antwort(
        &server,
        "devops_repos",
        json!({ "verbindung": "neoimpulse" }),
    )
    .await;
    assert!(attrappe
        .aufzeichnung
        .zu("git/repositories")
        .last()
        .unwrap()
        .pfad
        .contains(&format!("/{PROJEKT}/")));

    // Aus dem Aufruf (die zweite Organisation hat keins).
    let text = antwort(
        &server,
        "devops_repos",
        json!({ "verbindung": "kunde", "projekt": "Anderes" }),
    )
    .await;
    assert!(text.contains(REPO), "{text}");
    assert!(attrappe
        .aufzeichnung
        .zu("git/repositories")
        .last()
        .unwrap()
        .pfad
        .contains("/Anderes/"));

    // Ohne beides: Hinweis, keine Anfrage. (Schalter aus, darum erst prüfen,
    // dass ein lesendes Werkzeug den Hinweis gibt.)
    let vorher = attrappe.aufzeichnung.alle().len();
    let text = antwort(
        &server,
        "devops_workitem_typen",
        json!({ "verbindung": "kunde" }),
    )
    .await;
    assert!(text.contains("Projekt fehlt"), "{text}");
    assert_eq!(attrappe.aufzeichnung.alle().len(), vorher);
}

#[tokio::test]
async fn ein_ungueltiger_pat_wird_nicht_wiederholt() {
    let attrappe = Attrappe::starte().await;
    attrappe.pat_ablehnen();
    let server = eine(&attrappe.adresse);

    let fehler = aufruf(&server, "devops_projekte", json!({}))
        .await
        .unwrap_err();
    assert!(
        fehler
            .message
            .contains("PAT für neoimpulse ungültig oder abgelaufen"),
        "{}",
        fehler.message
    );
    assert!(
        fehler.message.contains("Tools → DevOps erneuern"),
        "{}",
        fehler.message
    );
    ohne_pat(&fehler.message);
    // Genau ein Versuch.
    assert_eq!(attrappe.aufzeichnung.anzahl("projects"), 1);
}

#[tokio::test]
async fn eine_anmeldeseite_mit_203_gilt_als_ungueltiger_pat() {
    let attrappe = Attrappe::starte().await;
    attrappe.anmeldeseite();
    let server = eine(&attrappe.adresse);

    let fehler = aufruf(&server, "devops_projekte", json!({}))
        .await
        .unwrap_err();
    assert!(
        fehler.message.contains("PAT für neoimpulse"),
        "{}",
        fehler.message
    );
    assert_eq!(attrappe.aufzeichnung.anzahl("projects"), 1);
}

// ------------------------------------------------------------- Arbeitsitems

#[tokio::test]
async fn projekte_werden_gelesen() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(&server, "devops_projekte", json!({})).await;
    assert!(text.contains("2 Projekt(e)"), "{text}");
    assert!(text.contains("- SCODA (wellFormed, private)"), "{text}");
    ohne_pat(&text);
}

#[tokio::test]
async fn die_suche_baut_die_wiql_aus_den_komfort_parametern() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(
        &server,
        "devops_workitems_suchen",
        json!({
            "text": "Login",
            "status": "Active",
            "typ": "Bug",
            "zugewiesen_an_mich": true,
            "top": 10
        }),
    )
    .await;
    assert!(text.contains("2 Arbeitsitem(s)"), "{text}");
    assert!(
        text.contains("[User Story] [Active] Login kaputt"),
        "{text}"
    );
    ohne_pat(&text);

    let wiql = attrappe.aufzeichnung.erster("wit/wiql").unwrap();
    assert_eq!(wiql.methode, "POST");
    assert_eq!(wiql.api_version(), Some("7.1"));
    assert_eq!(wiql.feld("$top"), Some("10"));
    let abfrage = wiql.json()["query"].as_str().unwrap().to_string();
    assert!(
        abfrage.starts_with("SELECT [System.Id] FROM WorkItems"),
        "{abfrage}"
    );
    assert!(
        abfrage.contains("[System.TeamProject] = 'SCODA'"),
        "{abfrage}"
    );
    assert!(
        abfrage.contains("[System.WorkItemType] = 'Bug'"),
        "{abfrage}"
    );
    assert!(abfrage.contains("[System.State] = 'Active'"), "{abfrage}");
    assert!(abfrage.contains("[System.AssignedTo] = @Me"), "{abfrage}");
    assert!(
        abfrage.contains("[System.Title] CONTAINS 'Login'"),
        "{abfrage}"
    );

    // Danach die Felder über `workitemsbatch`.
    let batch = attrappe.aufzeichnung.erster("workitemsbatch").unwrap();
    assert_eq!(batch.methode, "POST");
    assert_eq!(batch.json()["ids"], json!([ITEM, 124]));
    assert!(batch.json()["fields"]
        .as_array()
        .unwrap()
        .contains(&json!("System.Title")));
}

#[tokio::test]
async fn eine_eigene_wiql_ersetzt_die_komfort_felder() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    antwort(
        &server,
        "devops_workitems_suchen",
        json!({ "wiql": "SELECT [System.Id] FROM WorkItems WHERE [System.Id] > 100", "text": "egal" }),
    )
    .await;
    let abfrage = attrappe.aufzeichnung.erster("wit/wiql").unwrap().json()["query"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        abfrage,
        "SELECT [System.Id] FROM WorkItems WHERE [System.Id] > 100"
    );
    // Standard-`top` ist 50.
    assert_eq!(
        attrappe
            .aufzeichnung
            .erster("wit/wiql")
            .unwrap()
            .feld("$top"),
        Some("50")
    );
}

#[tokio::test]
async fn ein_arbeitsitem_zeigt_beschreibung_kriterien_und_links() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(&server, "devops_workitem", json!({ "id": ITEM })).await;
    assert!(
        text.contains("#123 [User Story] [Active] Login kaputt"),
        "{text}"
    );
    assert!(text.contains("## Beschreibung"), "{text}");
    assert!(text.contains("Der Anmelden-Knopf tut nichts."), "{text}");
    assert!(text.contains("## Akzeptanzkriterien"), "{text}");
    assert!(text.contains("- Anmelden klappt"), "{text}");
    assert!(text.contains("- Eltern #100"), "{text}");
    assert!(!text.contains("<div>"), "{text}");
    ohne_pat(&text);

    // `$expand=relations` hängt an der Anfrage.
    let aufruf = attrappe.aufzeichnung.erster("wit/workitems/").unwrap();
    assert_eq!(aufruf.feld("$expand"), Some("relations"));
}

#[tokio::test]
async fn kommentare_und_typen_werden_gelesen() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(&server, "devops_workitem_kommentare", json!({ "id": ITEM })).await;
    assert!(text.contains("2 Kommentar(e) zu #123"), "{text}");
    assert!(text.contains("**Anna**"), "{text}");
    assert!(text.contains("Sieht gut aus"), "{text}");
    ohne_pat(&text);
    // Die Kommentar-Endpunkte brauchen die Vorschau-Version.
    assert_eq!(
        attrappe
            .aufzeichnung
            .erster("comments")
            .unwrap()
            .api_version(),
        Some("7.1-preview.4")
    );

    let text = antwort(&server, "devops_workitem_typen", json!({})).await;
    assert!(
        text.contains("- Bug (Microsoft.Bug) · Status: New, Active, Closed"),
        "{text}"
    );
}

#[tokio::test]
async fn iterationen_und_ihre_arbeitsitems_werden_gelesen() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(&server, "devops_iterationen", json!({ "team": TEAM })).await;
    assert!(text.contains("SCODA\\Sprint 1"), "{text}");
    assert!(text.contains("2026-11-01 – 2026-11-14"), "{text}");
    let aufruf = attrappe
        .aufzeichnung
        .erster("teamsettings/iterations")
        .unwrap();
    // Das Team steht kodiert im Pfad (`SCODA Team` → `SCODA%20Team`).
    assert!(
        aufruf
            .pfad
            .contains("/SCODA%20Team/_apis/work/teamsettings/iterations"),
        "{}",
        aufruf.pfad
    );
    assert_eq!(aufruf.feld("$timeframe"), None);

    // `aktuell` setzt `$timeframe=current`.
    antwort(
        &server,
        "devops_iterationen",
        json!({ "team": TEAM, "aktuell": true }),
    )
    .await;
    assert_eq!(
        attrappe
            .aufzeichnung
            .zu("teamsettings/iterations")
            .last()
            .unwrap()
            .feld("$timeframe"),
        Some("current")
    );

    let text = antwort(
        &server,
        "devops_iteration_workitems",
        json!({ "team": TEAM, "iteration": "sprint-1" }),
    )
    .await;
    assert!(text.contains("2 Arbeitsitem(s)"), "{text}");
    assert!(
        text.contains("Arbeitsitems der Iteration sprint-1"),
        "{text}"
    );
}

#[tokio::test]
async fn ohne_team_kommt_ein_hinweis_ohne_anfrage() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);
    let text = antwort(&server, "devops_iterationen", json!({})).await;
    assert!(text.contains("Team fehlt"), "{text}");
    assert!(attrappe.aufzeichnung.alle().is_empty());
}

// --------------------------------------------------------------- Repos & PRs

#[tokio::test]
async fn repos_und_branches_werden_gelesen() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(&server, "devops_repos", json!({})).await;
    assert!(
        text.contains("- scoda-web · Standardbranch refs/heads/main"),
        "{text}"
    );

    let text = antwort(&server, "devops_branches", json!({ "repo": REPO })).await;
    assert!(text.contains("- refs/heads/main · abcdef12"), "{text}");
    assert!(
        text.contains("- refs/heads/feature/login · 12345678"),
        "{text}"
    );
    let aufruf = attrappe.aufzeichnung.erster("/refs").unwrap();
    assert_eq!(aufruf.feld("filter"), Some("heads/"));
}

#[tokio::test]
async fn pull_requests_werden_gelesen_und_gefiltert() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(&server, "devops_pull_requests", json!({})).await;
    assert!(text.contains("2 Pull Request(s)"), "{text}");
    assert!(
        text.contains("#42 [active] Login reparieren · scoda-web · feature/login → main"),
        "{text}"
    );
    assert!(text.contains("#43 [active] Doku"), "{text}");
    let aufruf = attrappe.aufzeichnung.erster("pullrequests").unwrap();
    assert_eq!(aufruf.feld("searchCriteria.status"), Some("active"));

    // Repository-scoped Pfad.
    attrappe.aufzeichnung.alle();
    antwort(
        &server,
        "devops_pull_requests",
        json!({ "repo": REPO, "status": "completed" }),
    )
    .await;
    let aufruf = attrappe.aufzeichnung.letzter().unwrap();
    assert!(
        aufruf
            .pfad
            .contains(&format!("/repositories/{REPO}/pullrequests")),
        "{}",
        aufruf.pfad
    );
    assert_eq!(aufruf.feld("searchCriteria.status"), Some("completed"));

    // `ersteller` filtert lokal.
    let text = antwort(
        &server,
        "devops_pull_requests",
        json!({ "ersteller": "anna" }),
    )
    .await;
    assert!(text.contains("1 Pull Request(s)"), "{text}");
    assert!(text.contains("#43"), "{text}");
    assert!(!text.contains("#42"), "{text}");
}

#[tokio::test]
async fn ein_pull_request_zeigt_pruefer_arbeitsitems_und_commits() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(&server, "devops_pull_request", json!({ "id": PR })).await;
    assert!(text.contains("#42 [active] Login reparieren"), "{text}");
    assert!(text.contains("feature/login → main"), "{text}");
    assert!(text.contains("## Beschreibung\nBitte prüfen."), "{text}");
    assert!(text.contains("- Anna · abgestimmt"), "{text}");
    assert!(text.contains("- Ben · wartet auf Autor"), "{text}");
    assert!(
        text.contains("## Verknüpfte Arbeitsitems\n- #123"),
        "{text}"
    );
    assert!(text.contains("## Commits (1)"), "{text}");
    assert!(
        text.contains("abcdef12 Patrick Weppelmann 2026-11-20 09:30: Login reparieren"),
        "{text}"
    );
    ohne_pat(&text);

    // Drei Anfragen: PR, Commits, verknüpfte Arbeitsitems.
    assert_eq!(attrappe.aufzeichnung.alle().len(), 3);
    assert_eq!(attrappe.aufzeichnung.anzahl("/commits"), 1);
    assert_eq!(attrappe.aufzeichnung.anzahl("/workitems"), 1);
    assert_eq!(
        attrappe
            .aufzeichnung
            .erster("/commits")
            .unwrap()
            .feld("$top"),
        Some("20")
    );
}

#[tokio::test]
async fn threads_ohne_system_threads_werden_gelesen() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    // Ohne `repo`: erst den Pull Request holen, dann die Threads.
    let text = antwort(&server, "devops_pull_request_threads", json!({ "id": PR })).await;
    assert!(text.contains("1 Kommentar-Thread(s) an PR #42"), "{text}");
    assert!(text.contains("**Thread 1** (active)"), "{text}");
    assert!(text.contains("Bitte ändern"), "{text}");
    assert!(
        !text.contains("Thread 2"),
        "System-Thread fehlt nicht: {text}"
    );
    // Zwei Anfragen: den Pull Request holen, dann die Threads.
    assert_eq!(attrappe.aufzeichnung.alle().len(), 2);
    assert_eq!(attrappe.aufzeichnung.anzahl("/threads"), 1);
}

// ---------------------------------------------------------------- Pipelines

#[tokio::test]
async fn pipelines_laeufe_und_logs_werden_gelesen() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(&server, "devops_pipelines", json!({})).await;
    assert!(text.contains("- #7 scoda-ci · Ordner \\"), "{text}");

    let text = antwort(&server, "devops_laeufe", json!({})).await;
    assert!(
        text.contains("#1234 scoda-ci · completed/failed · main · 2026-11-20 10:00"),
        "{text}"
    );
    assert_eq!(
        attrappe
            .aufzeichnung
            .erster("build/builds")
            .unwrap()
            .feld("$top"),
        Some("10")
    );

    // Filter: Pipeline und Branch.
    antwort(
        &server,
        "devops_laeufe",
        json!({ "pipeline": PIPELINE, "branch": "main", "top": 5 }),
    )
    .await;
    let aufruf = attrappe.aufzeichnung.letzter().unwrap();
    assert_eq!(aufruf.feld("definitions"), Some("7"));
    assert_eq!(aufruf.feld("branchName"), Some("refs/heads/main"));
    assert_eq!(aufruf.feld("$top"), Some("5"));

    let text = antwort(&server, "devops_lauf", json!({ "lauf": LAUF })).await;
    assert!(text.contains("#1234 scoda-ci · completed/failed"), "{text}");
    assert!(text.contains("## Fehlgeschlagene Schritte (2)"), "{text}");
    assert!(text.contains("- Task npm test · failed · Log 5"), "{text}");
    assert!(!text.contains("Compile"), "{text}");

    let text = antwort(
        &server,
        "devops_lauf_log",
        json!({ "lauf": LAUF, "log": LOG, "zeilen": 3 }),
    )
    .await;
    assert!(
        text.contains("Log 5 von Lauf #1234 (letzte 3 von 30 Zeilen, 27 davor weggelassen)"),
        "{text}"
    );
    assert!(text.contains("Zeile 30"), "{text}");
    assert!(!text.contains("Zeile 27"), "{text}");
}

// -------------------------------------------------------------- Schreiben

#[tokio::test]
async fn schreiben_aus_macht_bei_allen_acht_werkzeugen_keine_anfrage() {
    let attrappe = Attrappe::starte().await;
    let server = eine_ohne_schreiben(&attrappe.adresse);

    for (name, argumente) in [
        (
            "devops_workitem_anlegen",
            json!({ "typ": "Bug", "titel": "Neu" }),
        ),
        (
            "devops_workitem_aendern",
            json!({ "id": ITEM, "status": "Closed" }),
        ),
        (
            "devops_workitem_kommentieren",
            json!({ "id": ITEM, "text": "hallo" }),
        ),
        (
            "devops_workitem_verknuepfen",
            json!({ "id": ITEM, "ziel": 100, "art": "eltern" }),
        ),
        (
            "devops_pull_request_anlegen",
            json!({ "repo": REPO, "quelle": "feature/x", "ziel": "main", "titel": "Neu" }),
        ),
        (
            "devops_pull_request_kommentieren",
            json!({ "repo": REPO, "id": PR, "text": "hallo" }),
        ),
        (
            "devops_pull_request_abstimmen",
            json!({ "repo": REPO, "id": PR, "stimme": "approve" }),
        ),
        ("devops_pipeline_starten", json!({ "pipeline": PIPELINE })),
    ] {
        let text = antwort(&server, name, argumente).await;
        assert!(
            text.contains("Schreiben ist für neoimpulse aus"),
            "{name}: {text}"
        );
        assert!(
            text.contains("im quiver-Fenster einschalten"),
            "{name}: {text}"
        );
    }
    assert!(
        attrappe.aufzeichnung.alle().is_empty(),
        "trotz „schreiben aus“ gab es Anfragen: {:?}",
        attrappe::aufruf_pfade(&attrappe.aufzeichnung)
    );
}

#[tokio::test]
async fn ein_arbeitsitem_wird_angelegt_und_geaendert() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(
        &server,
        "devops_workitem_anlegen",
        json!({
            "typ": "Bug",
            "titel": "Neuer Fehler",
            "beschreibung": "<div>Bitte prüfen</div>",
            "zugewiesen": "anna@example.test",
            "bereich": "SCODA\\Web",
            "iteration": "SCODA\\Sprint 1",
            "eltern": 100
        }),
    )
    .await;
    assert!(
        text.contains("Arbeitsitem angelegt: #124 [Bug] [New] Neuer Fehler"),
        "{text}"
    );
    ohne_pat(&text);

    let aufruf = attrappe.aufzeichnung.erster("wit/workitems/").unwrap();
    assert_eq!(aufruf.methode, "POST");
    assert_eq!(
        aufruf.content_type.as_deref(),
        Some("application/json-patch+json")
    );
    assert!(aufruf.pfad.ends_with("/workitems/$Bug"), "{}", aufruf.pfad);
    let rumpf = aufruf.json();
    let felder: Vec<(String, String)> = rumpf
        .as_array()
        .unwrap()
        .iter()
        .filter(|op| op["path"].as_str().unwrap().starts_with("/fields/"))
        .map(|op| {
            (
                op["path"].as_str().unwrap().to_string(),
                op["value"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert!(felder.contains(&("/fields/System.Title".into(), "Neuer Fehler".into())));
    assert!(felder.contains(&(
        "/fields/System.AssignedTo".into(),
        "anna@example.test".into()
    )));
    assert!(felder.contains(&("/fields/System.AreaPath".into(), "SCODA\\Web".into())));
    // Der Eltern-Link hängt als Relation an.
    let relation = rumpf
        .as_array()
        .unwrap()
        .iter()
        .find(|op| op["path"] == json!("/relations/-"))
        .expect("Eltern-Link fehlt");
    assert_eq!(
        relation["value"]["rel"],
        json!("System.LinkTypes.Hierarchy-Reverse")
    );
    assert!(relation["value"]["url"]
        .as_str()
        .unwrap()
        .ends_with("/_apis/wit/workItems/100"));

    // Ändern: PATCH mit `replace`.
    let text = antwort(
        &server,
        "devops_workitem_aendern",
        json!({ "id": ITEM, "status": "Closed", "felder": [ { "feld": "System.Priority", "wert": "1" } ] }),
    )
    .await;
    assert!(text.contains("Arbeitsitem #123 geändert"), "{text}");
    assert!(text.contains("System.State → Closed"), "{text}");
    assert!(text.contains("System.Priority → 1"), "{text}");

    let aufruf = attrappe
        .aufzeichnung
        .zu_mit("wit/workitems/", "PATCH")
        .pop()
        .unwrap();
    assert_eq!(
        aufruf.content_type.as_deref(),
        Some("application/json-patch+json")
    );
    assert!(aufruf
        .json()
        .as_array()
        .unwrap()
        .iter()
        .all(|op| op["op"] == json!("replace")));
}

#[tokio::test]
async fn kommentar_und_verknuepfung_werden_geschrieben() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(
        &server,
        "devops_workitem_kommentieren",
        json!({ "id": ITEM, "text": "Sieht gut aus" }),
    )
    .await;
    assert!(
        text.contains("Kommentar 4711 zu Arbeitsitem #123 angelegt"),
        "{text}"
    );
    let aufruf = attrappe
        .aufzeichnung
        .zu_mit("comments", "POST")
        .pop()
        .unwrap();
    assert_eq!(aufruf.json()["text"], json!("Sieht gut aus"));
    assert_eq!(aufruf.api_version(), Some("7.1-preview.4"));

    // Hinzufügen: `add` auf `/relations/-`.
    let text = antwort(
        &server,
        "devops_workitem_verknuepfen",
        json!({ "id": ITEM, "ziel": 100, "art": "eltern" }),
    )
    .await;
    assert!(
        text.contains("Link „eltern“ von #123 zu #100 hinzugefügt"),
        "{text}"
    );
    let aufruf = attrappe
        .aufzeichnung
        .zu_mit("wit/workitems/", "PATCH")
        .pop()
        .unwrap();
    let op = &aufruf.json()[0];
    assert_eq!(op["op"], json!("add"));
    assert_eq!(op["path"], json!("/relations/-"));
    assert_eq!(
        op["value"]["rel"],
        json!("System.LinkTypes.Hierarchy-Reverse")
    );

    // Entfernen: erst die Relationen holen, dann den Index löschen.
    let text = antwort(
        &server,
        "devops_workitem_verknuepfen",
        json!({ "id": ITEM, "ziel": 100, "art": "eltern", "entfernen": true }),
    )
    .await;
    assert!(
        text.contains("Link „eltern“ zwischen #123 und #100 entfernt"),
        "{text}"
    );
    let aufruf = attrappe
        .aufzeichnung
        .zu_mit("wit/workitems/", "PATCH")
        .pop()
        .unwrap();
    assert_eq!(
        aufruf.json(),
        json!([ { "op": "remove", "path": "/relations/0" } ])
    );
}

#[tokio::test]
async fn ein_pull_request_wird_angelegt_und_arbeitsitems_verknuepft() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(
        &server,
        "devops_pull_request_anlegen",
        json!({
            "repo": REPO,
            "quelle": "feature/login",
            "ziel": "main",
            "titel": "Login reparieren",
            "beschreibung": "Bitte prüfen.",
            "entwurf": true,
            "arbeitsitems": [ITEM]
        }),
    )
    .await;
    assert!(
        text.contains("Pull Request #43 angelegt: Login reparieren"),
        "{text}"
    );
    assert!(text.contains("(feature/login → main) · Entwurf"), "{text}");
    assert!(text.contains("1 Arbeitsitem(s) verknüpft"), "{text}");

    let aufruf = attrappe
        .aufzeichnung
        .zu_mit("pullrequests", "POST")
        .pop()
        .unwrap();
    assert!(
        aufruf
            .pfad
            .contains(&format!("/repositories/{REPO}/pullrequests")),
        "{}",
        aufruf.pfad
    );
    let rumpf = aufruf.json();
    assert_eq!(rumpf["sourceRefName"], json!("refs/heads/feature/login"));
    assert_eq!(rumpf["targetRefName"], json!("refs/heads/main"));
    assert_eq!(rumpf["isDraft"], json!(true));
    assert_eq!(rumpf["description"], json!("Bitte prüfen."));

    // Der Arbeitsitem-Link trägt den Artefakt-Pfad aus Projekt- und Repo-ID.
    let link = attrappe
        .aufzeichnung
        .zu_mit("wit/workitems/", "PATCH")
        .pop()
        .unwrap();
    let value = &link.json()[0]["value"];
    assert_eq!(value["rel"], json!("ArtifactLink"));
    assert_eq!(
        value["url"],
        json!(format!(
            "vstfs:///Git/PullRequestId/{}%2F{}%2F43",
            attrappe::PROJEKT_ID,
            attrappe::REPO_ID
        ))
    );
}

#[tokio::test]
async fn ein_pull_request_wird_kommentiert_und_abgestimmt() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(
        &server,
        "devops_pull_request_kommentieren",
        json!({ "repo": REPO, "id": PR, "text": "Bitte ändern" }),
    )
    .await;
    assert!(text.contains("Neuer Thread 7 an PR #42 angelegt"), "{text}");
    let aufruf = attrappe
        .aufzeichnung
        .zu_mit("threads", "POST")
        .pop()
        .unwrap();
    assert_eq!(
        aufruf.json()["comments"][0]["content"],
        json!("Bitte ändern")
    );

    let text = antwort(
        &server,
        "devops_pull_request_kommentieren",
        json!({ "repo": REPO, "id": PR, "text": "Erledigt", "thread": 7 }),
    )
    .await;
    assert!(
        text.contains("Antwort im Thread 7 an PR #42 angelegt"),
        "{text}"
    );
    let aufruf = attrappe
        .aufzeichnung
        .zu_mit("threads", "POST")
        .pop()
        .unwrap();
    assert!(
        aufruf.pfad.ends_with("/threads/7/comments"),
        "{}",
        aufruf.pfad
    );

    // Abstimmung: erst die Benutzer-ID, dann `PUT` auf den Prüfer.
    let text = antwort(
        &server,
        "devops_pull_request_abstimmen",
        json!({ "repo": REPO, "id": PR, "stimme": "approve" }),
    )
    .await;
    assert!(
        text.contains("Stimme für PR #42 gesetzt: abgestimmt"),
        "{text}"
    );
    assert_eq!(attrappe.aufzeichnung.anzahl("connectionData"), 1);
    let verbindung = attrappe.aufzeichnung.erster("connectionData").unwrap();
    // `connectionData` ist Vorschau: mit `7.1` käme ein `400` statt der ID.
    assert_eq!(verbindung.api_version(), Some(attrappe::VORSCHAU_VERSION));
    let aufruf = attrappe
        .aufzeichnung
        .zu_mit("reviewers/", "PUT")
        .pop()
        .unwrap();
    assert!(
        aufruf
            .pfad
            .ends_with(&format!("/reviewers/{}", attrappe::BENUTZER_ID)),
        "{}",
        aufruf.pfad
    );
    assert_eq!(aufruf.json()["vote"], json!(10));
}

#[tokio::test]
async fn eine_abstimmung_bricht_bei_400_auf_connectiondata_ab() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    // Antwortet `connectionData` mit `400`, gibt es keine Benutzer-ID: der
    // Fehler muss ankommen, und nichts darf als ID weiterlaufen.
    attrappe.verbindungsdaten_ablehnen();
    let fehler = aufruf(
        &server,
        "devops_pull_request_abstimmen",
        json!({ "repo": REPO, "id": PR, "stimme": "approve" }),
    )
    .await
    .unwrap_err();
    assert!(fehler.message.contains("400"), "{}", fehler.message);
    assert!(fehler.message.contains("preview"), "{}", fehler.message);
    ohne_pat(&fehler.message);

    // Genau ein Aufruf (die Benutzer-ID), keine Stimme.
    assert_eq!(attrappe.aufzeichnung.anzahl("connectionData"), 1);
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("reviewers/", "PUT"), 0);
    for aufruf in attrappe.aufzeichnung.alle() {
        assert!(!aufruf.pfad.contains("400"), "{}", aufruf.pfad);
    }
}

#[tokio::test]
async fn eine_pipeline_wird_gestartet() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let text = antwort(
        &server,
        "devops_pipeline_starten",
        json!({
            "pipeline": PIPELINE,
            "branch": "main",
            "variablen": { "umgebung": "test" },
            "parameter": { "debug": "true" }
        }),
    )
    .await;
    assert!(
        text.contains("Pipeline-Lauf #5678 gestartet: scoda-ci"),
        "{text}"
    );
    assert!(text.contains("inProgress"), "{text}");

    let aufruf = attrappe.aufzeichnung.zu_mit("runs", "POST").pop().unwrap();
    let rumpf = aufruf.json();
    assert_eq!(
        rumpf["resources"]["repositories"]["self"]["refName"],
        json!("refs/heads/main")
    );
    assert_eq!(rumpf["variables"]["umgebung"]["value"], json!("test"));
    assert_eq!(rumpf["templateParameters"]["debug"], json!("true"));
}

// ------------------------------------------------------- Wiederholung (429)

#[tokio::test]
async fn ein_lesender_aufruf_wiederholt_bei_429_hoechstens_zweimal() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    attrappe.zu_viele_anfragen(2);
    let text = antwort(&server, "devops_projekte", json!({})).await;
    assert!(text.contains("- SCODA"), "{text}");
    // Zwei abgelehnte Versuche plus der erfolgreiche.
    assert_eq!(attrappe.aufzeichnung.anzahl("projects"), 3);

    // Mehr als zwei Wiederholungen gibt es nicht: der dritte 429 bleibt stehen.
    attrappe.zu_viele_anfragen(3);
    let fehler = aufruf(&server, "devops_projekte", json!({}))
        .await
        .unwrap_err();
    assert!(fehler.message.contains("429"), "{}", fehler.message);
    assert!(
        fehler.message.contains("Wiederholungen"),
        "{}",
        fehler.message
    );
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("projects", "GET"), 6);
}

#[tokio::test]
async fn ein_anlegender_aufruf_wiederholt_bei_429_nicht() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    attrappe.zu_viele_anfragen(1);
    let fehler = aufruf(
        &server,
        "devops_workitem_anlegen",
        json!({ "typ": "Bug", "titel": "Neu" }),
    )
    .await
    .unwrap_err();
    assert!(fehler.message.contains("429"), "{}", fehler.message);
    // Genau ein Versuch – ob der Aufruf ankam, weiß nur der Server.
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("wit/workitems", "POST"), 1);
}

// ------------------------------------------------------------- Querschnitt

#[tokio::test]
async fn jede_anfrage_traegt_die_api_version_und_keinen_pat_in_der_antwort() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe.adresse);

    let antworten = vec![
        antwort(&server, "devops_testen", json!({})).await,
        antwort(&server, "devops_projekte", json!({})).await,
        antwort(&server, "devops_workitems_suchen", json!({})).await,
        antwort(&server, "devops_workitem", json!({ "id": ITEM })).await,
        antwort(&server, "devops_repos", json!({})).await,
        antwort(&server, "devops_pipelines", json!({})).await,
        antwort(&server, "devops_laeufe", json!({})).await,
    ];
    for text in &antworten {
        ohne_pat(text);
    }

    for aufruf in attrappe.aufzeichnung.alle() {
        let version = aufruf
            .api_version()
            .unwrap_or_else(|| panic!("keine api-version an {} {}", aufruf.methode, aufruf.pfad));
        assert!(version.starts_with("7.1"), "{}: {version}", aufruf.pfad);
        // `connectionData` braucht die Vorschau-Version; überall sonst gilt 7.1.
        if aufruf.pfad.contains("connectionData") {
            assert_eq!(version, attrappe::VORSCHAU_VERSION, "{}", aufruf.pfad);
        }
        // Der PAT steckt nur in der Kopfzeile.
        assert!(!aufruf.pfad.contains(PAT_A));
        assert!(!aufruf.rumpf.contains(PAT_A));
    }
}
