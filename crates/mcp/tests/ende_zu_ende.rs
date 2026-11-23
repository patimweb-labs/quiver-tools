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
const TOOLS: &[&str] = &[
    "profil",
    "mails_suchen",
    "mail_lesen",
    "mail_senden",
    "termine",
    "termin_anlegen",
    "dateien_suchen",
    "abmelden",
];

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
async fn list_tools_hat_alle_acht_und_profil_liefert_den_namen() {
    let graph = GraphAttrappe::starten().await;
    let transport = TokioChildProcess::new(kindprozess(&graph)).expect("Kindprozess startet");
    let client = ().serve(transport).await.expect("MCP-Verbindung steht");

    let tools = client.list_all_tools().await.expect("list_tools");
    let mut namen: Vec<&str> = tools.iter().map(|tool| tool.name.as_ref()).collect();
    namen.sort_unstable();
    let mut erwartet = TOOLS.to_vec();
    erwartet.sort_unstable();
    assert_eq!(namen, erwartet, "genau die acht Tools aus SPEC.md §4");

    // Annotationen: verändernd nur `mail_senden` und `termin_anlegen`;
    // `abmelden` ist weder lesend noch zerstörend (Aufgabe 03).
    for tool in &tools {
        let annotations = tool.annotations.as_ref().expect("Annotationen gesetzt");
        let veraendernd = matches!(tool.name.as_ref(), "mail_senden" | "termin_anlegen");
        if veraendernd {
            assert_eq!(
                annotations.destructive_hint,
                Some(true),
                "{} ist verändernd",
                tool.name
            );
        } else if tool.name == "abmelden" {
            assert_eq!(
                annotations.read_only_hint,
                Some(false),
                "abmelden löscht das Refresh-Token"
            );
            assert_eq!(
                annotations.destructive_hint,
                Some(false),
                "abmelden zerstört keine Daten bei Graph"
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
