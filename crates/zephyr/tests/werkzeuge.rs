// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Werkzeuge gegen die Zephyr-Attrappe (SPEC §7, Aufgabe 15).
//!
//! Jeder Fall prüft eine Wirkung: was bei Zephyr gefragt wurde (von der
//! Attrappe aufgezeichnet) und was beim Agenten ankommt. Ein echtes Zephyr ist
//! nie im Spiel. Die Attrappe antwortet in den Formen der OpenAPI-Datei –
//! Verweise tragen nur `id` und `self` – und lehnt jedes Feld im Rumpf ab, das
//! dort nicht steht. Darum prüfen diese Tests auch den **Vertrag**.

mod attrappe;

use attrappe::defekt::{Art, Defekt};
use attrappe::{
    Attrappe, AUSFUEHRUNG, AUSFUEHRUNG_OPS, ORDNER, PROJEKT, PROJEKT_2, TESTFALL, TESTFALL_2,
    TOKEN_A, TOKEN_B, UMGEBUNG_NAME, ZYKLUS, ZYKLUS_OPS,
};
use quiver_tools_kern::zugaenge::Zugang as Eintrag;
use quiver_tools_kern::Zugang;
use quiver_zephyr::client::{Anfrage, Client};
use quiver_zephyr::zugaenge::Sammlung;
use quiver_zephyr::{Zephyr, ZephyrServer};
use rmcp::model::CallToolResult;
use rmcp::ErrorData as McpError;
use serde_json::{json, Value};

/// Ein Zugang für die Attrappe.
fn eintrag(
    name: &str,
    basis: &str,
    projekt: Option<&str>,
    nur: Option<&str>,
    token: &str,
    schreiben: bool,
) -> Eintrag {
    let mut werte: std::collections::BTreeMap<String, String> = [
        ("basis_url", basis),
        ("api_token", token),
        ("schreiben", if schreiben { "an" } else { "aus" }),
    ]
    .into_iter()
    .map(|(feld, wert)| (feld.to_string(), wert.to_string()))
    .collect();
    if let Some(projekt) = projekt {
        werte.insert("projekt".to_string(), projekt.to_string());
    }
    if let Some(nur) = nur {
        werte.insert("nur".to_string(), nur.to_string());
    }
    Eintrag {
        name: name.to_string(),
        werte,
    }
}

fn server(eintraege: Vec<Eintrag>) -> ZephyrServer {
    ZephyrServer::mit(
        Zugang::mit_umgebung(&Zephyr, |_| None),
        Ok(Sammlung::aus_zugaengen(eintraege).unwrap()),
    )
}

/// Ein Zugang mit festem Projekt; Schreiben ist an.
fn eine(attrappe: &Attrappe) -> ZephyrServer {
    server(vec![eintrag(
        "kunde",
        &attrappe.basis_url(),
        Some(PROJEKT),
        None,
        TOKEN_A,
        true,
    )])
}

/// Ein Zugang mit festem Projekt; Schreiben ist **aus**.
fn eine_ohne_schreiben(attrappe: &Attrappe) -> ZephyrServer {
    server(vec![eintrag(
        "kunde",
        &attrappe.basis_url(),
        Some(PROJEKT),
        None,
        TOKEN_A,
        false,
    )])
}

/// Ein Zugang auf die kaputte Attrappe.
fn eine_kaputte(defekt: &Defekt) -> ZephyrServer {
    server(vec![eintrag(
        "kunde",
        &defekt.basis_url(),
        Some(PROJEKT),
        None,
        TOKEN_A,
        true,
    )])
}

/// Zwei Zugänge: der erste mit Projekt, der zweite ohne (Projekt je Aufruf).
fn zwei(attrappe: &Attrappe) -> ZephyrServer {
    server(vec![
        eintrag(
            "kunde",
            &attrappe.basis_url(),
            Some(PROJEKT),
            None,
            TOKEN_A,
            true,
        ),
        eintrag("ops", &attrappe.basis_url(), None, None, TOKEN_B, false),
    ])
}

async fn aufruf(
    server: &ZephyrServer,
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

async fn antwort(server: &ZephyrServer, name: &str, argumente: Value) -> String {
    text(
        aufruf(server, name, argumente)
            .await
            .expect("unerwarteter Fehler"),
    )
}

async fn fehlertext(server: &ZephyrServer, name: &str, argumente: Value) -> String {
    aufruf(server, name, argumente)
        .await
        .expect_err("erwartet wurde ein Fehler")
        .message
        .to_string()
}

/// Keine Antwort dieses Tests darf ein Token enthalten.
fn ohne_token(text: &str) {
    assert!(!text.contains(TOKEN_A), "Token in der Antwort: {text}");
    assert!(!text.contains(TOKEN_B), "Token in der Antwort: {text}");
}

// ------------------------------------------------------------------ Zugang

#[tokio::test]
async fn status_zeigt_die_zugaenge_ohne_token() {
    let attrappe = Attrappe::starte().await;
    let server = zwei(&attrappe);

    let text = antwort(&server, "zephyr_status", json!({})).await;
    assert!(text.contains("2 Zugang/Zugänge"), "{text}");
    assert!(text.contains("kunde"), "{text}");
    assert!(text.contains("ops"), "{text}");
    assert!(text.contains("Schreiben aus"), "{text}");
    assert!(text.contains("Schreiben an"), "{text}");
    ohne_token(&text);
    // `zephyr_status` öffnet keine Verbindung.
    assert!(attrappe.aufzeichnung.alle().is_empty());
}

#[tokio::test]
async fn testen_prueft_das_token_und_zeigt_die_projekte() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe);

    let text = antwort(&server, "zephyr_testen", json!({})).await;
    assert!(text.contains("1 Zugang/Zugänge erreichbar"), "{text}");
    assert!(text.contains("kunde – Token gilt"), "{text}");
    assert!(text.contains("2 Projekt(e) sichtbar"), "{text}");
    assert!(text.contains("z. B. QA"), "{text}");
    ohne_token(&text);

    // Genau eine Anfrage auf `/projects` mit `maxResults=1`.
    assert_eq!(attrappe.aufzeichnung.anzahl("projects"), 1);
    let aufruf = attrappe.aufzeichnung.letzter().unwrap();
    assert_eq!(aufruf.feld("maxResults"), Some("1"));
    assert!(aufruf.anmeldung.unwrap().starts_with("Bearer "));
    assert!(!aufruf.rumpf.contains(TOKEN_A));
}

#[tokio::test]
async fn ohne_zugang_kommt_der_hinweis_und_kein_fehler() {
    let server = server(vec![]);
    for (name, argumente) in [
        ("zephyr_status", json!({})),
        ("zephyr_testen", json!({})),
        ("zephyr_projekte", json!({})),
        ("zephyr_testfaelle", json!({})),
    ] {
        let text = antwort(&server, name, argumente).await;
        assert!(text.contains("Zugang fehlt"), "{name}: {text}");
        assert!(text.contains("Tools → Zephyr"), "{name}: {text}");
    }
}

#[tokio::test]
async fn ein_zugang_darf_ohne_namen_gewaehlt_werden() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe);
    let text = antwort(&server, "zephyr_testfaelle", json!({})).await;
    assert!(text.contains(TESTFALL), "{text}");
}

#[tokio::test]
async fn zwei_zugaenge_verlangen_einen_namen() {
    let attrappe = Attrappe::starte().await;
    let server = zwei(&attrappe);

    let fehler = aufruf(&server, "zephyr_projekte", json!({}))
        .await
        .unwrap_err();
    assert!(
        fehler.message.contains("mehrere Zugänge"),
        "{}",
        fehler.message
    );
    assert!(fehler.message.contains("kunde"), "{}", fehler.message);
    assert!(attrappe.aufzeichnung.alle().is_empty());

    let fehler = aufruf(
        &server,
        "zephyr_projekte",
        json!({ "verbindung": "gibtsnicht" }),
    )
    .await
    .unwrap_err();
    assert!(fehler.message.contains("gibtsnicht"), "{}", fehler.message);
    assert!(fehler.message.contains("ops"), "{}", fehler.message);
    assert!(attrappe.aufzeichnung.alle().is_empty());

    // Mit Namen klappt es (der zweite Zugang braucht kein Projekt).
    let text = antwort(&server, "zephyr_projekte", json!({ "verbindung": "OPS" })).await;
    assert!(text.contains("2 Projekt(e)"), "{text}");
}

#[tokio::test]
async fn das_projekt_kommt_aus_dem_zugang_oder_aus_dem_aufruf() {
    let attrappe = Attrappe::starte().await;
    let server = zwei(&attrappe);

    // Aus dem Zugang.
    antwort(
        &server,
        "zephyr_testfaelle",
        json!({ "verbindung": "kunde" }),
    )
    .await;
    assert_eq!(
        attrappe
            .aufzeichnung
            .zu("testcases")
            .last()
            .unwrap()
            .feld("projectKey"),
        Some(PROJEKT)
    );

    // Aus dem Aufruf (der zweite Zugang hat keins).
    let text = antwort(
        &server,
        "zephyr_testfaelle",
        json!({ "verbindung": "ops", "projekt": PROJEKT_2 }),
    )
    .await;
    assert!(text.contains("OPS-T1"), "{text}");
    assert_eq!(
        attrappe
            .aufzeichnung
            .zu("testcases")
            .last()
            .unwrap()
            .feld("projectKey"),
        Some(PROJEKT_2)
    );

    // Ohne beides: Hinweis, keine Anfrage.
    let vorher = attrappe.aufzeichnung.alle().len();
    let text = antwort(&server, "zephyr_testfaelle", json!({ "verbindung": "ops" })).await;
    assert!(text.contains("Projekt fehlt"), "{text}");
    assert_eq!(attrappe.aufzeichnung.alle().len(), vorher);
}

// --------------------------------------------------------------- Sicherheit

#[tokio::test]
async fn ein_ungueltiges_token_wird_nicht_wiederholt() {
    let attrappe = Attrappe::starte().await;
    attrappe.token_ablehnen();
    let server = eine(&attrappe);

    let fehler = aufruf(&server, "zephyr_projekte", json!({}))
        .await
        .unwrap_err();
    assert!(
        fehler
            .message
            .contains("Token für kunde ungültig oder abgelaufen"),
        "{}",
        fehler.message
    );
    assert!(
        fehler.message.contains("Tools → Zephyr erneuern"),
        "{}",
        fehler.message
    );
    ohne_token(&fehler.message);
    // Genau ein Versuch.
    assert_eq!(attrappe.aufzeichnung.anzahl("projects"), 1);
}

#[tokio::test]
async fn ein_403_nennt_die_fehlenden_rechte() {
    let attrappe = Attrappe::starte().await;
    attrappe.keine_rechte();
    let server = eine(&attrappe);

    let fehler = aufruf(&server, "zephyr_projekte", json!({}))
        .await
        .unwrap_err();
    assert!(
        fehler.message.contains("Keine Rechte für kunde (HTTP 403)"),
        "{}",
        fehler.message
    );
    assert!(
        fehler.message.contains("Rechte des API-Tokens"),
        "{}",
        fehler.message
    );
    ohne_token(&fehler.message);
    assert_eq!(attrappe.aufzeichnung.anzahl("projects"), 1);
}

#[tokio::test]
async fn ein_lesender_aufruf_wiederholt_bei_429_hoechstens_zweimal() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe);

    attrappe.zu_viele_anfragen(2, 0);
    let text = antwort(&server, "zephyr_projekte", json!({})).await;
    assert!(text.contains("QA (id 1)"), "{text}");
    // Zwei abgelehnte Versuche plus der erfolgreiche.
    assert_eq!(attrappe.aufzeichnung.anzahl("projects"), 3);

    // Mehr als zwei Wiederholungen gibt es nicht: der dritte 429 bleibt stehen.
    attrappe.zu_viele_anfragen(3, 0);
    let fehler = aufruf(&server, "zephyr_projekte", json!({}))
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
async fn ein_lesender_aufruf_wiederholt_auch_bei_503() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe);

    attrappe.nicht_verfuegbar(1, 0);
    let text = antwort(&server, "zephyr_projekte", json!({})).await;
    assert!(text.contains("QA (id 1)"), "{text}");
    assert_eq!(attrappe.aufzeichnung.anzahl("projects"), 2);
}

#[tokio::test]
async fn eine_zu_lange_wartezeit_wird_nicht_abgewartet() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe);

    attrappe.zu_viele_anfragen(1, 60);
    let beginn = std::time::Instant::now();
    let fehler = aufruf(&server, "zephyr_projekte", json!({}))
        .await
        .unwrap_err();
    assert!(beginn.elapsed() < std::time::Duration::from_secs(5));
    assert!(fehler.message.contains("60 s"), "{}", fehler.message);
    assert!(
        fehler.message.contains("nicht wiederholt"),
        "{}",
        fehler.message
    );
    assert_eq!(attrappe.aufzeichnung.anzahl("projects"), 1);
}

#[tokio::test]
async fn ein_schreibender_aufruf_wiederholt_bei_429_nicht() {
    let attrappe = Attrappe::starte().await;
    // Direkt am Client: die verändernden Werkzeuge lesen vorher, darum wäre
    // hier nicht zu erkennen, welcher Aufruf wiederholt wurde.
    let zugang = quiver_zephyr::zugaenge::Zugang::aus_zugang(&eintrag(
        "kunde",
        &attrappe.basis_url(),
        Some(PROJEKT),
        None,
        TOKEN_A,
        true,
    ))
    .unwrap();
    let client = Client::neu(&zugang).unwrap();

    attrappe.zu_viele_anfragen(1, 0);
    let fehler = client
        .anfrage(&Anfrage::post("testcases", json!({}).to_string()))
        .await
        .unwrap_err();
    assert!(fehler.to_string().contains("429"), "{fehler}");
    assert!(fehler.to_string().contains("nicht wiederholt"), "{fehler}");
    // Genau ein Versuch – ob der Aufruf ankam, weiß nur der Server.
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("testcases", "POST"), 1);
}

#[tokio::test]
async fn eine_weiterleitung_wird_nicht_verfolgt() {
    let attrappe = Attrappe::starte().await;
    attrappe.weiterleitung();
    let server = eine(&attrappe);

    let fehler = aufruf(&server, "zephyr_projekte", json!({}))
        .await
        .unwrap_err();
    assert!(
        fehler.message.contains("Weiterleitung"),
        "{}",
        fehler.message
    );
    assert!(
        fehler.message.contains("folgt ihr nicht"),
        "{}",
        fehler.message
    );
    ohne_token(&fehler.message);
    // Keine zweite Anfrage: das Token bleibt beim eingetragenen Host.
    assert_eq!(attrappe.aufzeichnung.anzahl("projects"), 1);
}

#[tokio::test]
async fn eine_fremde_folgeseite_wird_abgelehnt() {
    let attrappe = Attrappe::starte().await;
    attrappe.fremde_folgeseite();
    let server = eine(&attrappe);

    // Mit `anzahl` 1 gibt es eine Folgeseite – und die zeigt auf einen fremden
    // Host.
    let fehler = aufruf(&server, "zephyr_projekte", json!({ "anzahl": 1 }))
        .await
        .unwrap_err();
    assert!(
        fehler.message.contains("fremden Host"),
        "{}",
        fehler.message
    );
    assert!(
        fehler.message.contains("fremder.example.test"),
        "{}",
        fehler.message
    );
    // Die Folgeseite wurde nicht geholt.
    assert_eq!(attrappe.aufzeichnung.anzahl("projects"), 1);
}

#[tokio::test]
async fn nur_https_oder_loopback_ist_erlaubt() {
    let falsch = Sammlung::aus_zugaengen(vec![eintrag(
        "kunde",
        "http://api.example.test/v2",
        Some(PROJEKT),
        None,
        TOKEN_A,
        true,
    )]);
    let fehler = falsch.unwrap_err().to_string();
    assert!(fehler.contains("https://"), "{fehler}");
    assert!(fehler.contains("127.0.0.1"), "{fehler}");

    let richtig = Sammlung::aus_zugaengen(vec![eintrag(
        "kunde",
        "https://api.example.test/v2",
        Some(PROJEKT),
        None,
        TOKEN_A,
        true,
    )]);
    assert!(richtig.is_ok());

    // Loopback über http ist für die Attrappe erlaubt.
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe);
    let text = antwort(&server, "zephyr_projekte", json!({})).await;
    assert!(text.contains(PROJEKT), "{text}");
}

#[tokio::test]
async fn adressen_mit_benutzer_oder_abfrage_werden_abgelehnt() {
    for falsch in [
        "http://evil.test\\@127.0.0.1/v2",
        "https://127.0.0.1@evil.test/v2",
        "https://benutzer:geheim@api.example.test/v2",
        "https://api.example.test/v2?projekt=QA",
        "https://api.example.test/v2#frag",
    ] {
        let sammlung = Sammlung::aus_zugaengen(vec![eintrag(
            "kunde",
            falsch,
            Some(PROJEKT),
            None,
            TOKEN_A,
            true,
        )]);
        assert!(sammlung.is_err(), "{falsch} sollte abgelehnt werden");
    }
}

#[tokio::test]
async fn punkt_und_punktpunkt_und_leere_werte_kommen_nicht_an() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe);

    for falsch in [".", "..", " . ", "", "  ", "../QA-T1", "a/../b"] {
        let fehler = fehlertext(&server, "zephyr_testfall", json!({ "schluessel": falsch })).await;
        assert!(fehler.contains("nichts gesendet"), "{falsch}: {fehler}");
    }
    // Keine einzige Anfrage: die Prüfung steht vor dem Netz.
    assert!(
        attrappe.aufzeichnung.alle().is_empty(),
        "{:?}",
        attrappe::aufruf_pfade(&attrappe.aufzeichnung)
    );

    // Ein Schlüssel, der nur einen Punkt enthält, ist erlaubt.
    let fehler = fehlertext(
        &server,
        "zephyr_testfall",
        json!({ "schluessel": "QA-T1.2" }),
    )
    .await;
    assert!(fehler.contains("404"), "{fehler}");
}

#[tokio::test]
async fn werte_im_pfad_werden_prozent_kodiert() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe);

    let fehler = fehlertext(
        &server,
        "zephyr_testfall",
        json!({ "schluessel": "a/b?c#d" }),
    )
    .await;
    assert!(fehler.contains("404"), "{fehler}");

    let aufruf = attrappe.aufzeichnung.erster("testcases").unwrap();
    assert_eq!(aufruf.pfad, "/v2/testcases/a%2Fb%3Fc%23d");
    assert!(aufruf.abfrage.is_empty(), "{:?}", aufruf.abfrage);
}

#[tokio::test]
async fn ein_unlesbarer_rumpf_ist_ein_fehler_und_keine_leere_antwort() {
    let defekt = Defekt::starte(Art::Lesen).await;
    let server = eine_kaputte(&defekt);

    let fehler = fehlertext(&server, "zephyr_projekte", json!({})).await;
    assert!(fehler.contains("nicht lesen"), "{fehler}");
    assert!(fehler.contains("keine leere Antwort"), "{fehler}");
    // „Keine Projekte sichtbar“ wäre eine Behauptung über den Bestand.
    assert!(!fehler.contains("Keine Projekte"), "{fehler}");
}

#[tokio::test]
async fn eine_aenderung_mit_unlesbarer_antwort_ist_ausgefuehrt_nicht_lesbar() {
    let defekt = Defekt::starte(Art::Aenderung).await;
    let server = eine_kaputte(&defekt);

    let fehler = fehlertext(
        &server,
        "zephyr_ergebnis_eintragen",
        json!({ "ausfuehrung": AUSFUEHRUNG, "status": "Pass" }),
    )
    .await;
    assert!(fehler.contains("ausgeführt"), "{fehler}");
    assert!(fehler.contains("nicht lesen"), "{fehler}");
    assert!(
        fehler.contains("statt den Aufruf zu wiederholen"),
        "{fehler}"
    );
}

#[tokio::test]
async fn eine_aenderung_ohne_antwort_laesst_den_ausgang_offen() {
    let defekt = Defekt::starte(Art::AenderungOhneAntwort).await;
    let server = eine_kaputte(&defekt);

    let fehler = fehlertext(
        &server,
        "zephyr_ergebnis_eintragen",
        json!({ "ausfuehrung": AUSFUEHRUNG, "status": "Pass" }),
    )
    .await;
    assert!(fehler.contains("Ausgang unklar"), "{fehler}");
    assert!(
        fehler.contains("Stand prüfen, nicht einfach wiederholen"),
        "{fehler}"
    );
}

#[tokio::test]
async fn schreiben_aus_macht_bei_allen_drei_werkzeugen_keine_anfrage() {
    let attrappe = Attrappe::starte().await;
    let server = eine_ohne_schreiben(&attrappe);

    for (name, argumente) in [
        (
            "zephyr_testfall_anlegen",
            json!({ "name": "Neu", "objektiv": "x" }),
        ),
        (
            "zephyr_zyklus_testfaelle_hinzufuegen",
            json!({ "zyklus": ZYKLUS, "testfaelle": [TESTFALL] }),
        ),
        (
            "zephyr_ergebnis_eintragen",
            json!({ "ausfuehrung": AUSFUEHRUNG, "status": "Pass" }),
        ),
    ] {
        let text = antwort(&server, name, argumente).await;
        assert!(
            text.contains("Schreiben ist für kunde aus"),
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
async fn nur_beschraenkt_das_schreiben_auf_erlaubte_projekte() {
    let attrappe = Attrappe::starte().await;
    // Erlaubt ist nur QA; das Projekt kommt trotzdem vom Server.
    let server = server(vec![eintrag(
        "kunde",
        &attrappe.basis_url(),
        Some(PROJEKT),
        Some("qa"),
        TOKEN_A,
        true,
    )]);

    // Testfall im gesperrten Projekt: `GET /projects/OPS` wird gelesen, dann
    // kommt die Absage – ohne POST.
    let text = antwort(
        &server,
        "zephyr_testfall_anlegen",
        json!({ "name": "Neu", "projekt": PROJEKT_2 }),
    )
    .await;
    assert!(text.contains("beschränkt"), "{text}");
    assert!(text.contains(PROJEKT_2), "{text}");
    assert!(text.contains("nichts geändert"), "{text}");
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("testcases", "POST"), 0);

    // Ergebnis im gesperrten Projekt: die Ausführung wird gelesen, dann die
    // Absage – ohne PUT.
    let text = antwort(
        &server,
        "zephyr_ergebnis_eintragen",
        json!({ "ausfuehrung": AUSFUEHRUNG_OPS, "status": "In Arbeit" }),
    )
    .await;
    assert!(text.contains("beschränkt"), "{text}");
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("testexecutions", "PUT"), 0);

    // Zyklus im gesperrten Projekt: der Zyklus wird gelesen, dann die Absage.
    let text = antwort(
        &server,
        "zephyr_zyklus_testfaelle_hinzufuegen",
        json!({ "zyklus": ZYKLUS_OPS, "testfaelle": ["OPS-T1"] }),
    )
    .await;
    assert!(text.contains("beschränkt"), "{text}");
    assert_eq!(
        attrappe.aufzeichnung.anzahl_mit("testexecutions", "POST"),
        0
    );

    // Im erlaubten Projekt (klein geschrieben in `nur`) klappt es.
    let text = antwort(
        &server,
        "zephyr_testfall_anlegen",
        json!({ "name": "Neuer Fall" }),
    )
    .await;
    assert!(text.contains("angelegt: QA-T3"), "{text}");
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("testcases", "POST"), 1);
}

// ------------------------------------------------------------------ Lesen

#[tokio::test]
async fn projekte_werden_gelesen_und_geblättert() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe);

    let text = antwort(&server, "zephyr_projekte", json!({ "anzahl": 1 })).await;
    assert!(
        text.starts_with("1 Projekt(e) von 2 – weitere mit `ab` = 1:"),
        "{text}"
    );
    // `Project` hat keinen Namen – nur Schlüssel, Id und Jira-Bezug.
    assert!(text.contains("- QA (id 1) · Jira-Projekt: 10000"), "{text}");

    let aufruf = attrappe.aufzeichnung.erster("projects").unwrap();
    assert_eq!(aufruf.feld("maxResults"), Some("1"));
    assert_eq!(aufruf.feld("startAt"), Some("0"));

    let text = antwort(&server, "zephyr_projekte", json!({ "anzahl": 1, "ab": 1 })).await;
    assert!(text.starts_with("1 Projekt(e):"), "{text}");
    assert!(text.contains("- OPS (id 2)"), "{text}");
    assert_eq!(
        attrappe
            .aufzeichnung
            .zu("projects")
            .last()
            .unwrap()
            .feld("startAt"),
        Some("1")
    );

    // `anzahl` wird auf 100 begrenzt.
    antwort(&server, "zephyr_projekte", json!({ "anzahl": 500 })).await;
    assert_eq!(
        attrappe
            .aufzeichnung
            .zu("projects")
            .last()
            .unwrap()
            .feld("maxResults"),
        Some("100")
    );
}

#[tokio::test]
async fn testfaelle_und_ein_testfall_werden_gelesen() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe);

    let text = antwort(&server, "zephyr_testfaelle", json!({})).await;
    assert!(text.starts_with("2 Testfall/Testfälle:"), "{text}");
    assert!(
        text.contains(
            "- QA-T1 (id 11) · Login prüfen · Status: Draft · Priorität: High · Ordner: 12"
        ),
        "{text}"
    );
    assert!(text.contains("- QA-T2 (id 12) · Logout prüfen"), "{text}");
    ohne_token(&text);

    // Status und Priorität sind Verweise: je ein Aufruf, nicht je Eintrag.
    assert_eq!(attrappe.aufzeichnung.anzahl("statuses"), 1);
    assert_eq!(attrappe.aufzeichnung.anzahl("priorities"), 1);
    assert_eq!(
        attrappe
            .aufzeichnung
            .zu("statuses")
            .last()
            .unwrap()
            .feld("statusType"),
        Some("TEST_CASE")
    );

    let text = antwort(
        &server,
        "zephyr_testfall",
        json!({ "schluessel": TESTFALL }),
    )
    .await;
    assert!(
        text.starts_with("QA-T1 (id 11) · Login prüfen · Projekt: QA · Status: Draft"),
        "{text}"
    );
    assert!(text.contains("Priorität: High"), "{text}");
    assert!(text.contains("Besitzer: anna-4711"), "{text}");
    assert!(text.contains("Objektiv: Anmelden klappt"), "{text}");
    assert!(text.contains("Vorbedingung: Benutzer besteht"), "{text}");
    assert!(text.contains("Schritte (2):"), "{text}");
    assert!(
        text.contains("1. Seite öffnen · erwartet: Formular sichtbar"),
        "{text}"
    );
    assert!(text.contains("2. Anmelden · Daten: paula/pw"), "{text}");
    ohne_token(&text);

    // Die Schritte kommen aus `/teststeps`, das Skript aus `/testscript`.
    assert!(attrappe.aufzeichnung.anzahl("teststeps") >= 1);
    // Das Projekt kommt aus dem Verweis der Antwort (`GET /projects/1`).
    let projekt_aufrufe: Vec<_> = attrappe
        .aufzeichnung
        .zu("projects")
        .into_iter()
        .filter(|aufruf| aufruf.pfad.ends_with("/projects/1"))
        .collect();
    assert_eq!(projekt_aufrufe.len(), 1, "{projekt_aufrufe:?}");

    // Das Skript eines anderen Testfalls.
    let text = antwort(
        &server,
        "zephyr_testfall",
        json!({ "schluessel": TESTFALL_2 }),
    )
    .await;
    assert!(text.contains("Skript (plain):\n1. Abmelden"), "{text}");
    assert!(!text.contains("Schritte"), "{text}");
}

#[tokio::test]
async fn testzyklen_und_ein_zyklus_mit_zaehlung_werden_gelesen() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe);

    let text = antwort(&server, "zephyr_testzyklen", json!({})).await;
    assert!(text.starts_with("1 Testzyklus/Testzyklen:"), "{text}");
    assert!(
        text.contains(&format!(
            "- {ZYKLUS} (id 31) · Login-Zyklus · Status: Draft · 2026-11-01 – 2026-11-14"
        )),
        "{text}"
    );

    let text = antwort(
        &server,
        "zephyr_testzyklus",
        json!({ "schluessel": ZYKLUS }),
    )
    .await;
    assert!(
        text.starts_with(&format!("{ZYKLUS} (id 31) · Login-Zyklus · Projekt: QA")),
        "{text}"
    );
    assert!(text.contains("Ausführungen je Status (aus 2):"), "{text}");
    assert!(text.contains("- Pass: 1"), "{text}");
    assert!(text.contains("- Fail: 1"), "{text}");

    // Gezählt wird über `testexecutions` mit `testCycle` und Projekt vom Server.
    let aufruf = attrappe.aufzeichnung.zu("testexecutions").pop().unwrap();
    assert_eq!(aufruf.feld("testCycle"), Some(ZYKLUS));
    assert_eq!(aufruf.feld("projectKey"), Some(PROJEKT));

    // Keine Projektliste: das Projekt kommt aus `GET /projects/1`.
    assert!(
        attrappe
            .aufzeichnung
            .zu("projects")
            .iter()
            .all(|aufruf| aufruf.pfad.ends_with("/projects/1")),
        "{:?}",
        attrappe::aufruf_pfade(&attrappe.aufzeichnung)
    );
}

#[tokio::test]
async fn ein_zyklus_laesst_sich_auch_per_id_zaehlen() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe);

    // Der Agent nennt die Id; `testCycle` der API ist ein **Schlüssel**-Filter,
    // gesendet wird darum der Schlüssel aus `GET /testcycles/31`.
    let text = antwort(&server, "zephyr_testzyklus", json!({ "schluessel": "31" })).await;
    assert!(text.starts_with(&format!("{ZYKLUS} (id 31)")), "{text}");
    let aufruf = attrappe
        .aufzeichnung
        .zu_mit("testexecutions", "GET")
        .pop()
        .unwrap();
    assert_eq!(aufruf.feld("testCycle"), Some(ZYKLUS));
}

#[tokio::test]
async fn ausfuehrungen_und_eine_ausfuehrung_werden_gelesen() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe);

    let text = antwort(&server, "zephyr_ausfuehrungen", json!({})).await;
    assert!(text.starts_with("2 Ausführung(en):"), "{text}");
    // `testCase` ist ein Verweis auf eine Version – die API gibt keinen
    // Schlüssel her, also steht die Id da.
    assert!(
        text.contains(&format!(
            "- {AUSFUEHRUNG} (id 101) · Testfall: (id 11) · Status: Pass · Zyklus: {ZYKLUS} · \
             Umgebung: {UMGEBUNG_NAME}"
        )),
        "{text}"
    );
    assert!(
        text.contains("- QA-E2 (id 102) · Testfall: (id 12) · Status: Fail"),
        "{text}"
    );

    // Umgebung und Zyklus werden je Id **einmal** geholt.
    assert_eq!(
        attrappe
            .aufzeichnung
            .zu("environments")
            .iter()
            .filter(|aufruf| aufruf.pfad.ends_with("/environments/5"))
            .count(),
        1
    );
    assert_eq!(
        attrappe
            .aufzeichnung
            .zu("testcycles")
            .iter()
            .filter(|aufruf| aufruf.pfad.ends_with("/testcycles/31"))
            .count(),
        1
    );

    // Nur ein Zyklus.
    let text = antwort(&server, "zephyr_ausfuehrungen", json!({ "zyklus": ZYKLUS })).await;
    assert!(text.starts_with("2 Ausführung(en):"), "{text}");
    assert_eq!(
        attrappe
            .aufzeichnung
            .zu("testexecutions")
            .last()
            .unwrap()
            .feld("testCycle"),
        Some(ZYKLUS)
    );

    let text = antwort(
        &server,
        "zephyr_ausfuehrung",
        json!({ "ausfuehrung": AUSFUEHRUNG }),
    )
    .await;
    assert!(
        text.starts_with(&format!("{AUSFUEHRUNG} (id 101) · Status: Pass")),
        "{text}"
    );
    assert!(text.contains(&format!("Zyklus: {ZYKLUS}")), "{text}");
    assert!(text.contains("Dauer: 120 ms"), "{text}");
    assert!(text.contains("Art: manuell"), "{text}");
    assert!(text.contains("Kommentar: läuft"), "{text}");
    ohne_token(&text);
}

#[tokio::test]
async fn statusliste_und_ordner_werden_gelesen() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe);

    // Ohne `art` sind es die Status der Ausführungen – die, die ein Ergebnis
    // annehmen.
    let text = antwort(&server, "zephyr_status_liste", json!({})).await;
    assert!(text.starts_with("5 Status:"), "{text}");
    assert!(text.contains("- Pass (id 1) · Standard"), "{text}");
    assert!(text.contains("- Not Executed (id 4)"), "{text}");
    assert_eq!(
        attrappe
            .aufzeichnung
            .zu("statuses")
            .last()
            .unwrap()
            .feld("statusType"),
        Some("TEST_EXECUTION")
    );

    // Eine andere Art wird als `statusType` geschickt.
    let text = antwort(
        &server,
        "zephyr_status_liste",
        json!({ "art": "test_case" }),
    )
    .await;
    assert!(text.starts_with("3 Status:"), "{text}");
    assert_eq!(
        attrappe
            .aufzeichnung
            .zu("statuses")
            .last()
            .unwrap()
            .feld("statusType"),
        Some("TEST_CASE")
    );

    let text = antwort(&server, "zephyr_ordner", json!({})).await;
    assert!(text.starts_with("2 Ordner:"), "{text}");
    assert!(
        text.contains(&format!("- {ORDNER} · Login · Art: TEST_CASE · Eltern: 3")),
        "{text}"
    );
    assert_eq!(
        attrappe
            .aufzeichnung
            .zu("folders")
            .last()
            .unwrap()
            .feld("folderType"),
        Some("TEST_CASE")
    );

    // Art wechseln; die Attrappe prüft den Parameter.
    let text = antwort(&server, "zephyr_ordner", json!({ "art": "test_plan" })).await;
    assert!(text.contains("Plan A"), "{text}");
    assert_eq!(
        attrappe
            .aufzeichnung
            .zu("folders")
            .last()
            .unwrap()
            .feld("folderType"),
        Some("TEST_PLAN")
    );

    let fehler = aufruf(&server, "zephyr_ordner", json!({ "art": "quatsch" }))
        .await
        .unwrap_err();
    assert!(
        fehler.message.contains("TEST_CASE, TEST_PLAN, TEST_CYCLE"),
        "{}",
        fehler.message
    );
}

#[tokio::test]
async fn ein_name_auf_einer_spaeteren_seite_wird_gefunden() {
    let attrappe = Attrappe::starte().await;
    // Jede Seite hat nur zwei Einträge: „In Arbeit“ ist der fünfte Status.
    attrappe.seite_max(2);
    let server = eine(&attrappe);

    // „gibt es nicht“ wäre falsch: der Name steht auf Seite 3.
    let text = antwort(
        &server,
        "zephyr_ergebnis_eintragen",
        json!({ "ausfuehrung": AUSFUEHRUNG, "status": "in arbeit" }),
    )
    .await;
    assert!(text.contains("Status In Arbeit"), "{text}");
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("testexecutions", "PUT"), 1);
    // Drei Seiten: 2 + 2 + 1.
    assert_eq!(attrappe.aufzeichnung.anzahl("statuses"), 3);

    // Und ein wirklich unbekannter Status nennt alle fünf.
    let fehler = fehlertext(
        &server,
        "zephyr_ergebnis_eintragen",
        json!({ "ausfuehrung": AUSFUEHRUNG, "status": "Grün" }),
    )
    .await;
    assert!(
        fehler.contains("Pass, Fail, Blocked, Not Executed, In Arbeit"),
        "{fehler}"
    );
}

// -------------------------------------------------------------- Schreiben

#[tokio::test]
async fn ein_testfall_wird_angelegt() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe);

    let text = antwort(
        &server,
        "zephyr_testfall_anlegen",
        json!({
            "name": "Neuer Fall",
            "objektiv": "Anmelden klappt",
            "vorbedingung": "Benutzer besteht",
            "ordner": ORDNER
        }),
    )
    .await;
    assert!(
        text.contains("Testfall „Neuer Fall“ angelegt: QA-T3 (id 13) · Projekt: QA"),
        "{text}"
    );
    ohne_token(&text);

    let aufruf = attrappe
        .aufzeichnung
        .zu_mit("testcases", "POST")
        .pop()
        .unwrap();
    assert_eq!(aufruf.content_type.as_deref(), Some("application/json"));
    let rumpf = aufruf.json();
    assert_eq!(rumpf["projectKey"], json!(PROJEKT));
    assert_eq!(rumpf["name"], json!("Neuer Fall"));
    assert_eq!(rumpf["objective"], json!("Anmelden klappt"));
    assert_eq!(rumpf["precondition"], json!("Benutzer besteht"));
    assert_eq!(rumpf["folderId"], json!(ORDNER));
    // Nur Felder aus `TestCaseInput`.
    assert_eq!(rumpf.as_object().unwrap().keys().count(), 5, "{rumpf}");

    // Vorher wurde **das Projekt** gelesen – `GET /projects/QA`, keine Liste.
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("projects", "GET"), 1);
    assert_eq!(
        attrappe.aufzeichnung.erster("projects").unwrap().pfad,
        "/v2/projects/QA"
    );
}

#[tokio::test]
async fn ein_projekt_das_der_server_nicht_kennt_wird_abgelehnt() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe);

    let fehler = fehlertext(
        &server,
        "zephyr_testfall_anlegen",
        json!({ "name": "Neu", "projekt": "GIBTSNICHT" }),
    )
    .await;
    assert!(
        fehler.contains("gibt es im Zugang „kunde“ nicht"),
        "{fehler}"
    );
    assert!(fehler.contains("404"), "{fehler}");
    assert!(fehler.contains("nichts angelegt"), "{fehler}");
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("testcases", "POST"), 0);
}

#[tokio::test]
async fn testfaelle_werden_einem_zyklus_hinzugefuegt() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe);

    let text = antwort(
        &server,
        "zephyr_zyklus_testfaelle_hinzufuegen",
        json!({
            "zyklus": ZYKLUS,
            "testfaelle": [TESTFALL, TESTFALL_2],
            "status": "pass",
            "umgebung": UMGEBUNG_NAME
        }),
    )
    .await;
    assert!(
        text.starts_with(&format!(
            "2 von 2 Testfall/Testfällen zu {ZYKLUS} hinzugefügt:"
        )),
        "{text}"
    );
    // `201 CreatedResource` nennt keinen Schlüssel – die Id genügt.
    assert!(text.contains("- QA-T1 → Ausführung (id 201)"), "{text}");
    assert!(text.contains("- QA-T2 → Ausführung (id 202)"), "{text}");
    ohne_token(&text);

    // Der Statusname wurde gegen `/statuses` geprüft und groß geschrieben.
    assert_eq!(
        attrappe
            .aufzeichnung
            .zu("statuses")
            .last()
            .unwrap()
            .feld("statusType"),
        Some("TEST_EXECUTION")
    );
    let aufrufe = attrappe.aufzeichnung.zu_mit("testexecutions", "POST");
    assert_eq!(aufrufe.len(), 2);
    let rumpf = aufrufe[0].json();
    assert_eq!(rumpf["projectKey"], json!(PROJEKT));
    assert_eq!(rumpf["testCaseKey"], json!(TESTFALL));
    assert_eq!(rumpf["testCycleKey"], json!(ZYKLUS));
    assert_eq!(rumpf["statusName"], json!("Pass"));
    assert_eq!(rumpf["environmentName"], json!(UMGEBUNG_NAME));

    // Jeder Testfall wurde **vorher** gelesen (Existenz und Projekt).
    let gelesen: Vec<String> = attrappe
        .aufzeichnung
        .zu_mit("testcases", "GET")
        .into_iter()
        .map(|aufruf| aufruf.pfad)
        .collect();
    assert!(
        gelesen.contains(&format!("/v2/testcases/{TESTFALL}")),
        "{gelesen:?}"
    );
    assert!(
        gelesen.contains(&format!("/v2/testcases/{TESTFALL_2}")),
        "{gelesen:?}"
    );
}

#[tokio::test]
async fn der_zyklusschluessel_kommt_vom_server() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe);

    // Der Agent nennt die **Id** des Zyklus; gesendet wird der Schlüssel, den
    // `GET /testcycles/31` liefert.
    let text = antwort(
        &server,
        "zephyr_zyklus_testfaelle_hinzufuegen",
        json!({ "zyklus": "31", "testfaelle": [TESTFALL], "status": "Pass" }),
    )
    .await;
    assert!(text.contains(&format!("zu {ZYKLUS} hinzugefügt")), "{text}");
    let rumpf = attrappe
        .aufzeichnung
        .zu_mit("testexecutions", "POST")
        .pop()
        .unwrap()
        .json();
    assert_eq!(rumpf["testCycleKey"], json!(ZYKLUS));
    assert_eq!(rumpf["testCaseKey"], json!(TESTFALL));
}

#[tokio::test]
async fn ohne_status_gilt_der_standard_aus_statuses() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe);

    let text = antwort(
        &server,
        "zephyr_zyklus_testfaelle_hinzufuegen",
        json!({ "zyklus": ZYKLUS, "testfaelle": [TESTFALL] }),
    )
    .await;
    assert!(text.contains("1 von 1"), "{text}");
    // `Pass` ist der Standard in `/statuses` für Ausführungen.
    assert_eq!(
        attrappe
            .aufzeichnung
            .zu_mit("testexecutions", "POST")
            .pop()
            .unwrap()
            .json()["statusName"],
        json!("Pass")
    );
}

#[tokio::test]
async fn ein_unbekannter_status_wird_beim_zuordnen_abgelehnt() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe);

    let fehler = fehlertext(
        &server,
        "zephyr_zyklus_testfaelle_hinzufuegen",
        json!({ "zyklus": ZYKLUS, "testfaelle": [TESTFALL], "status": "Grün" }),
    )
    .await;
    assert!(
        fehler.contains("Den Status „Grün“ gibt es im Projekt QA"),
        "{fehler}"
    );
    assert!(
        fehler.contains("Pass, Fail, Blocked, Not Executed, In Arbeit"),
        "{fehler}"
    );
    assert!(fehler.contains("nichts geändert"), "{fehler}");
    assert_eq!(
        attrappe.aufzeichnung.anzahl_mit("testexecutions", "POST"),
        0
    );
}

#[tokio::test]
async fn ein_abbruch_nennt_angelegt_gescheitert_und_nicht_versucht() {
    let attrappe = Attrappe::starte().await;
    attrappe.fehler_bei(TESTFALL_2);
    let server = eine(&attrappe);

    let text = antwort(
        &server,
        "zephyr_zyklus_testfaelle_hinzufuegen",
        json!({
            "zyklus": ZYKLUS,
            "testfaelle": [TESTFALL, TESTFALL_2, "QA-T9"]
        }),
    )
    .await;
    assert!(
        text.starts_with(&format!(
            "1 von 3 Testfall/Testfällen zu {ZYKLUS} hinzugefügt:"
        )),
        "{text}"
    );
    assert!(text.contains("- QA-T1 → Ausführung (id 201)"), "{text}");
    assert!(
        text.contains("QA-T2 scheiterte: Zephyr antwortete mit HTTP 500"),
        "{text}"
    );
    assert!(text.contains("Nicht versucht: QA-T9"), "{text}");
    ohne_token(&text);

    // Nichts wird wiederholt: ein POST je versuchtem Testfall.
    assert_eq!(
        attrappe.aufzeichnung.anzahl_mit("testexecutions", "POST"),
        2
    );
}

#[tokio::test]
async fn ein_testfall_den_es_nicht_gibt_bricht_ohne_ausfuehrung_ab() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe);

    let text = antwort(
        &server,
        "zephyr_zyklus_testfaelle_hinzufuegen",
        json!({ "zyklus": ZYKLUS, "testfaelle": [TESTFALL, "QA-T9", "QA-T8"] }),
    )
    .await;
    assert!(text.starts_with("1 von 3"), "{text}");
    assert!(
        text.contains("QA-T9 scheiterte: den Testfall gibt es nicht"),
        "{text}"
    );
    assert!(text.contains("Nicht versucht: QA-T8"), "{text}");
    // Für den unbekannten Testfall wurde keine Ausführung angelegt.
    assert_eq!(
        attrappe.aufzeichnung.anzahl_mit("testexecutions", "POST"),
        1
    );
}

#[tokio::test]
async fn ein_ergebnis_wird_eingetragen_ohne_felder_zu_verlieren() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe);

    let text = antwort(
        &server,
        "zephyr_ergebnis_eintragen",
        json!({ "ausfuehrung": AUSFUEHRUNG, "status": "pass", "kommentar": "neu" }),
    )
    .await;
    assert_eq!(
        text,
        "Ergebnis für QA-E1 (id 101) gesetzt: Status Pass · Art: manuell · Kommentar: neu"
    );
    ohne_token(&text);

    // Vorher gelesen, dann `PUT` auf die Id.
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("testexecutions", "GET"), 1);
    let aufruf = attrappe
        .aufzeichnung
        .zu_mit("testexecutions", "PUT")
        .pop()
        .unwrap();
    assert!(
        aufruf.pfad.ends_with("/testexecutions/101"),
        "{}",
        aufruf.pfad
    );
    let rumpf = aufruf.json();
    // Genau `TestExecutionUpdate`: der Status, `automated` (Standard `true`)
    // und der Kommentar. Alles andere kennt das Schema nicht.
    assert_eq!(
        rumpf,
        json!({ "statusName": "Pass", "automated": false, "comment": "neu" })
    );

    // Ohne neuen Kommentar bleibt der gelesene stehen (`2xx` ist Erfolg).
    let text = antwort(
        &server,
        "zephyr_ergebnis_eintragen",
        json!({ "ausfuehrung": "101", "status": "Fail" }),
    )
    .await;
    assert_eq!(
        text,
        "Ergebnis für QA-E1 (id 101) gesetzt: Status Fail · Art: manuell"
    );
    let rumpf = attrappe
        .aufzeichnung
        .zu_mit("testexecutions", "PUT")
        .pop()
        .unwrap()
        .json();
    assert_eq!(rumpf, json!({ "statusName": "Fail", "automated": false }));
}

#[tokio::test]
async fn ein_unbekannter_status_wird_ohne_aenderung_abgelehnt() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe);

    let fehler = fehlertext(
        &server,
        "zephyr_ergebnis_eintragen",
        json!({ "ausfuehrung": AUSFUEHRUNG, "status": "Grün" }),
    )
    .await;
    assert!(
        fehler.contains("Den Status „Grün“ gibt es im Projekt QA für Ausführungen nicht"),
        "{fehler}"
    );
    assert!(
        fehler.contains("Erlaubt sind: Pass, Fail, Blocked, Not Executed, In Arbeit"),
        "{fehler}"
    );
    assert!(fehler.contains("nichts geändert"), "{fehler}");
    assert_eq!(attrappe.aufzeichnung.anzahl_mit("testexecutions", "PUT"), 0);
}

// ------------------------------------------------------------- Querschnitt

#[tokio::test]
async fn keine_antwort_enthaelt_ein_token() {
    let attrappe = Attrappe::starte().await;
    let server = eine(&attrappe);

    let antworten = vec![
        antwort(&server, "zephyr_status", json!({})).await,
        antwort(&server, "zephyr_testen", json!({})).await,
        antwort(&server, "zephyr_projekte", json!({})).await,
        antwort(&server, "zephyr_testfaelle", json!({})).await,
        antwort(
            &server,
            "zephyr_testfall",
            json!({ "schluessel": TESTFALL }),
        )
        .await,
        antwort(&server, "zephyr_testzyklen", json!({})).await,
        antwort(
            &server,
            "zephyr_testzyklus",
            json!({ "schluessel": ZYKLUS }),
        )
        .await,
        antwort(&server, "zephyr_ausfuehrungen", json!({})).await,
        antwort(
            &server,
            "zephyr_ausfuehrung",
            json!({ "ausfuehrung": AUSFUEHRUNG }),
        )
        .await,
        antwort(&server, "zephyr_status_liste", json!({})).await,
        antwort(&server, "zephyr_ordner", json!({})).await,
        antwort(
            &server,
            "zephyr_ergebnis_eintragen",
            json!({ "ausfuehrung": AUSFUEHRUNG, "status": "Pass" }),
        )
        .await,
    ];
    for text in &antworten {
        ohne_token(text);
    }

    // Das Token steckt nur in der Kopfzeile.
    for aufruf in attrappe.aufzeichnung.alle() {
        assert!(aufruf.anmeldung.unwrap().starts_with("Bearer "));
        assert!(!aufruf.pfad.contains(TOKEN_A));
        assert!(!aufruf.rumpf.contains(TOKEN_A));
    }
}

#[tokio::test]
async fn nur_beschraenkt_auch_das_lesen() {
    let attrappe = Attrappe::starte().await;
    // Erlaubt ist nur QA.
    let server = server(vec![eintrag(
        "kunde",
        &attrappe.basis_url(),
        Some(PROJEKT),
        Some("qa"),
        TOKEN_A,
        true,
    )]);

    // Lesen im gesperrten Projekt: Hinweis, keine Anfrage an Zephyr.
    let text = antwort(
        &server,
        "zephyr_testfaelle",
        json!({ "projekt": PROJEKT_2 }),
    )
    .await;
    assert!(text.contains("beschränkt"), "{text}");
    assert!(text.contains(PROJEKT_2), "{text}");
    assert_eq!(attrappe.aufzeichnung.anzahl("testcases"), 0);

    // Im freigegebenen Projekt wird gelesen.
    let text = antwort(&server, "zephyr_testfaelle", json!({})).await;
    assert!(text.contains("Testfall"), "{text}");
    assert_eq!(attrappe.aufzeichnung.anzahl("testcases"), 1);
}
