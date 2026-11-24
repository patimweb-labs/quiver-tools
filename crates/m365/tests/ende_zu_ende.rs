// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Ende zu Ende: `quiver-m365` als Kindprozess, MCP über stdio, Graph gegen die
//! Attrappe. Der „Browser“ wird mit `M365_ACCESS_TOKEN` umgangen, der Tresor ist
//! `M365_TRESOR=speicher` – nie die echte Tresor-Datei.

mod graph_attrappe;

use graph_attrappe::GraphAttrappe;
use rmcp::model::CallToolRequestParams;
use rmcp::transport::TokioChildProcess;
use rmcp::ServiceExt;
use serde_json::Value;

/// Die acht Tools aus SPEC.md §4.
const TOOLS_SPEC: &[&str] = &[
    "profil",
    "mails_suchen",
    "mail_lesen",
    "mail_senden",
    "termine",
    "termin_anlegen",
    "dateien_suchen",
    "abmelden",
];

/// Die 16 Teams-Tools aus docs/aufgaben/05-teams.md.
const TOOLS_TEAMS: &[&str] = &[
    "teams_eingang",
    "teams_liste",
    "teams_kanaele",
    "teams_kanal_lesen",
    "teams_thread_lesen",
    "teams_kanal_senden",
    "teams_kanal_antworten",
    "teams_chats",
    "teams_chat_lesen",
    "teams_chat_senden",
    "teams_chat_anlegen",
    "teams_suchen",
    "teams_mitglieder",
    "teams_person_finden",
    "teams_reagieren",
    "teams_als_gelesen",
];

/// Die 26 Tools aus docs/aufgaben/06-graph-erweiterung.md.
const TOOLS_ERWEITERUNG: &[&str] = &[
    "sharepoint_sites",
    "sharepoint_bibliotheken",
    "dateien_liste",
    "datei_lesen",
    "datei_hochladen",
    "ordner_anlegen",
    "sharepoint_listen",
    "sharepoint_listeneintraege",
    "sharepoint_listeneintrag_anlegen",
    "sharepoint_listeneintrag_aendern",
    "planner_plaene",
    "planner_aufgaben",
    "planner_aufgabe_anlegen",
    "planner_aufgabe_aendern",
    "todo_listen",
    "todo_aufgaben",
    "todo_aufgabe_anlegen",
    "todo_aufgabe_erledigen",
    "kontakte_suchen",
    "kontakt_anlegen",
    "personen_suchen",
    "onenote_notizbuecher",
    "onenote_abschnitte",
    "onenote_seiten",
    "onenote_seite_lesen",
    "onenote_seite_anlegen",
];

/// Die neun Löschen-Tools aus docs/aufgaben/09-leere-antworten-und-loeschen.md.
const TOOLS_LOESCHEN: &[&str] = &[
    "termin_loeschen",
    "mail_loeschen",
    "datei_loeschen",
    "planner_aufgabe_loeschen",
    "todo_aufgabe_loeschen",
    "onenote_seite_loeschen",
    "kontakt_loeschen",
    "sharepoint_listeneintrag_loeschen",
    "teams_nachricht_loeschen",
];

/// Verändert das Tool etwas bei Graph?
fn ist_destruktiv(name: &str) -> bool {
    matches!(
        name,
        "mail_senden"
            | "termin_anlegen"
            | "teams_kanal_senden"
            | "teams_kanal_antworten"
            | "teams_chat_senden"
            | "teams_chat_anlegen"
            | "teams_reagieren"
            | "datei_hochladen"
            | "ordner_anlegen"
            | "sharepoint_listeneintrag_anlegen"
            | "sharepoint_listeneintrag_aendern"
            | "planner_aufgabe_anlegen"
            | "planner_aufgabe_aendern"
            | "todo_aufgabe_anlegen"
            | "todo_aufgabe_erledigen"
            | "kontakt_anlegen"
            | "onenote_seite_anlegen"
            // Die neun Löschen-Tools aus Aufgabe 09 – alle destruktiv.
            | "termin_loeschen"
            | "mail_loeschen"
            | "datei_loeschen"
            | "planner_aufgabe_loeschen"
            | "todo_aufgabe_loeschen"
            | "onenote_seite_loeschen"
            | "kontakt_loeschen"
            | "sharepoint_listeneintrag_loeschen"
            | "teams_nachricht_loeschen"
    )
}

fn kindprozess(graph: &GraphAttrappe) -> tokio::process::Command {
    let mut befehl = tokio::process::Command::new(env!("CARGO_BIN_EXE_quiver-m365"));
    befehl
        .env("M365_GRAPH_URL", &graph.adresse)
        .env("M365_ACCESS_TOKEN", "token-von-quiver")
        .env("M365_TRESOR", "speicher")
        // Zeigt ins Leere: ohne Token darf die Anmeldung gar nicht erst greifen.
        .env("M365_AUTHORITY", "http://127.0.0.1:9")
        .env("M365_CLIENT_ID", "test-client")
        .env("M365_TENANT", "test-tenant")
        .env("M365_ZEITZONE", "Europe/Berlin")
        .env("RUST_LOG", "warn");
    befehl
}

fn text_aus(ergebnis: &rmcp::model::CallToolResult) -> String {
    ergebnis
        .content
        .iter()
        .filter_map(|block| match block {
            rmcp::model::ContentBlock::Text(text) => Some(text.text.clone()),
            _ => None,
        })
        .collect::<Vec<String>>()
        .join("\n")
}

#[tokio::test]
async fn list_tools_hat_alle_tools_und_profil_liefert_den_namen() {
    let graph = GraphAttrappe::starten().await;
    let transport = TokioChildProcess::new(kindprozess(&graph)).expect("Kindprozess startet");
    let client = ().serve(transport).await.expect("MCP-Verbindung steht");

    let tools = client.list_all_tools().await.expect("list_tools");
    let mut namen: Vec<&str> = tools.iter().map(|tool| tool.name.as_ref()).collect();
    namen.sort_unstable();
    let mut erwartet: Vec<&str> = TOOLS_SPEC
        .iter()
        .chain(TOOLS_TEAMS.iter())
        .chain(TOOLS_ERWEITERUNG.iter())
        .chain(TOOLS_LOESCHEN.iter())
        .copied()
        .collect();
    erwartet.sort_unstable();
    assert_eq!(
        namen, erwartet,
        "die acht Tools aus SPEC.md §4, die 16 aus Aufgabe 05, die 26 aus Aufgabe 06 und die neun Löschen-Tools aus Aufgabe 09"
    );

    // Regression: die bisherigen 50 Tools stehen weiter mit denselben Namen in
    // der Liste (nichts umbenannt, nichts entfernt) – Aufgabe 09 ergänzt nur.
    for alt in TOOLS_SPEC
        .iter()
        .chain(TOOLS_TEAMS.iter())
        .chain(TOOLS_ERWEITERUNG.iter())
    {
        assert!(
            tools.iter().any(|tool| tool.name == *alt),
            "{alt} fehlt – bestehende Tools dürfen nicht verschwinden"
        );
    }

    // Die Löschen-Tools nehmen eine Id und sind destruktiv.
    for loeschen in TOOLS_LOESCHEN {
        let tool = tools
            .iter()
            .find(|tool| tool.name == *loeschen)
            .unwrap_or_else(|| panic!("{loeschen} fehlt"));
        let eigenschaften = tool
            .input_schema
            .get("properties")
            .and_then(|wert| wert.as_object())
            .expect("inputSchema hat properties");
        assert!(
            !eigenschaften.is_empty(),
            "{loeschen} nimmt mindestens eine Angabe"
        );
    }

    // Jedes Eingabeschema ist ein Objekt (Aufgabe 05) – sonst kann ein Client
    // die Parameter nicht prüfen.
    for tool in &tools {
        assert_eq!(
            tool.input_schema.get("type").and_then(|wert| wert.as_str()),
            Some("object"),
            "{}: inputSchema ohne type=object",
            tool.name
        );
    }

    // Annotationen: verändernd nur die schreibenden Tools; `abmelden` und
    // `teams_als_gelesen` sind weder lesend noch zerstörend.
    for tool in &tools {
        let annotations = tool.annotations.as_ref().expect("Annotationen gesetzt");
        if ist_destruktiv(&tool.name) {
            assert_eq!(
                annotations.destructive_hint,
                Some(true),
                "{} ist verändernd",
                tool.name
            );
        } else if matches!(tool.name.as_ref(), "abmelden" | "teams_als_gelesen") {
            assert_eq!(
                annotations.read_only_hint,
                Some(false),
                "{} ändert Zustand",
                tool.name
            );
            assert_eq!(
                annotations.destructive_hint,
                Some(false),
                "{} zerstört keine Daten bei Graph",
                tool.name
            );
        } else {
            assert_eq!(
                annotations.read_only_hint,
                Some(true),
                "{} ist nur lesend",
                tool.name
            );
        }
    }

    // Parameter sind deutsch benannt und beschrieben (schemars aus den Doc-Kommentaren).
    let mails = tools
        .iter()
        .find(|tool| tool.name == "mails_suchen")
        .expect("mails_suchen ist dabei");
    let eigenschaften = mails
        .input_schema
        .get("properties")
        .and_then(|wert| wert.as_object())
        .expect("inputSchema hat properties");
    for feld in ["suche", "anzahl"] {
        let beschreibung = eigenschaften
            .get(feld)
            .and_then(|wert| wert.get("description"))
            .and_then(|wert| wert.as_str())
            .unwrap_or_else(|| panic!("{feld} hat keine Beschreibung"));
        assert!(
            beschreibung.contains("Mails") || beschreibung.contains("höchstens"),
            "{feld}: {beschreibung}"
        );
    }

    let antwort = client
        .call_tool(CallToolRequestParams::new("profil"))
        .await
        .expect("profil");
    let wert: Value = serde_json::from_str(&text_aus(&antwort)).expect("JSON");
    assert_eq!(wert["name"], "Anna Muster");
    assert_eq!(wert["mail"], "anna@firma.de");

    let anfrage = graph.einzige();
    assert_eq!(anfrage.pfad, "/v1.0/me");
    assert_eq!(
        anfrage.kopf("authorization"),
        Some("Bearer token-von-quiver"),
        "das gereichte Token wird benutzt"
    );

    client.cancel().await.expect("Verbindung schließt");
}

#[test]
fn version_steht_auf_stdout() {
    let ausgabe = std::process::Command::new(env!("CARGO_BIN_EXE_quiver-m365"))
        .arg("--version")
        .env("RUST_LOG", "warn")
        .output()
        .expect("Programm startet");
    assert!(ausgabe.status.success());
    let text = String::from_utf8_lossy(&ausgabe.stdout);
    assert!(
        text.contains(env!("CARGO_PKG_VERSION")),
        "Version fehlt: {text}"
    );
    assert!(text.contains("quiver-m365"), "{text}");
}

#[test]
fn unbekanntes_argument_wird_abgelehnt() {
    let ausgabe = std::process::Command::new(env!("CARGO_BIN_EXE_quiver-m365"))
        .arg("quatsch")
        .env("RUST_LOG", "warn")
        .output()
        .expect("Programm startet");
    assert!(!ausgabe.status.success());
    let fehler = String::from_utf8_lossy(&ausgabe.stderr);
    assert!(fehler.contains("quatsch"), "{fehler}");
}
