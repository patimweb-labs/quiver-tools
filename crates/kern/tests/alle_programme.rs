// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der Vertrag über **alle** Programme (Härtung Paket 4, Schreibschutz).
//!
//! Jedes MCP-Werkzeug trägt die Kennzeichnung „nur lesend“ oder nicht. Der Kern
//! sperrt alles, was nicht lesend gekennzeichnet ist, ohne den Schalter
//! „Schreiben erlauben“ – aber nur, wenn das Programm einen solchen Schalter
//! überhaupt kennt. Genau das prüft dieser Test für jedes Programm:
//!
//! - Jedes Werkzeug trägt eine Kennzeichnung (`read_only_hint` ist gesetzt).
//! - Gibt es ein schreibendes Werkzeug, hat der Zugang ein Feld `schreiben`
//!   (bei mehreren Zugängen: in der Liste der Felder).
//! - Zusätzlich gilt: kein Werkzeug ist „lesend“ gekennzeichnet, obwohl es
//!   ändert (die Liste unten nennt die bekannten lesenden Werkzeuge).
//!
//! Ein neues schreibendes Werkzeug ohne Schalter oder ein lesendes Werkzeug mit
//! Nebenwirkung fällt damit hier auf.

use quiver_tools_kern::{Feld, Werkzeug as _, Zugang, FELD_SCHREIBEN};

/// Ein Programm mit seinem Werkzeug und seinen MCP-Werkzeugen.
struct Programm {
    titel: &'static str,
    felder: Vec<Feld>,
    werkzeuge: Vec<rmcp::model::Tool>,
}

fn ohne_zugang<W: quiver_tools_kern::Werkzeug>(werkzeug: &W) -> Zugang {
    Zugang::mit_umgebung(werkzeug, |_| None)
}

fn programme() -> Vec<Programm> {
    let outline = quiver_outline::Outline;
    let email = quiver_email::Email;
    let ssh = quiver_ssh::Ssh;
    let abap = quiver_abap::Abap;
    let devops = quiver_devops::Devops;
    let jira = quiver_jira::Jira;
    let confluence = quiver_confluence::Confluence;
    let zephyr = quiver_zephyr::Zephyr;

    vec![
        Programm {
            titel: "Outline",
            felder: outline.felder(),
            werkzeuge: quiver_outline::OutlineServer::neu(ohne_zugang(&outline)).werkzeuge(),
        },
        Programm {
            titel: "E-Mail",
            felder: email.felder(),
            werkzeuge: quiver_email::EmailServer::neu(ohne_zugang(&email)).werkzeuge(),
        },
        Programm {
            titel: "SSH",
            felder: ssh.felder(),
            werkzeuge: quiver_ssh::SshServer::neu(ohne_zugang(&ssh)).werkzeuge(),
        },
        Programm {
            titel: "ABAP",
            felder: abap.felder(),
            werkzeuge: quiver_abap::AbapServer::neu(ohne_zugang(&abap)).werkzeuge(),
        },
        Programm {
            titel: "DevOps",
            felder: devops.felder(),
            werkzeuge: quiver_devops::DevopsServer::neu(ohne_zugang(&devops)).werkzeuge(),
        },
        Programm {
            titel: "Jira",
            felder: jira.felder(),
            werkzeuge: quiver_jira::JiraServer::neu(ohne_zugang(&jira)).werkzeuge(),
        },
        Programm {
            titel: "Confluence",
            felder: confluence.felder(),
            werkzeuge: quiver_confluence::ConfluenceServer::neu(ohne_zugang(&confluence))
                .werkzeuge(),
        },
        Programm {
            titel: "Zephyr",
            felder: zephyr.felder(),
            werkzeuge: quiver_zephyr::ZephyrServer::neu(ohne_zugang(&zephyr)).werkzeuge(),
        },
    ]
}

fn ist_lesend(werkzeug: &rmcp::model::Tool) -> Option<bool> {
    werkzeug
        .annotations
        .as_ref()
        .and_then(|annotationen| annotationen.read_only_hint)
}

/// Die Hilfetexte stehen im quiver-Fenster neben dem Feld und werden von
/// Menschen gelesen: verständliches Deutsch, neutrale Beispiele und keine
/// Entwicklersprache aus dem Testbetrieb.
#[test]
fn die_hilfetexte_sind_verstaendlich_und_neutral() {
    /// Wörter, die im quiver-Fenster nichts zu suchen haben.
    const VERBOTEN: [&str; 6] = [
        "attrappe", "gepusht", "firma.de", "kunde.de", "@gmail", "proton",
    ];

    for programm in programme() {
        for feld in &programm.felder {
            assert!(
                !feld.hilfe.trim().is_empty(),
                "{}: „{}“ hat keine Hilfe",
                programm.titel,
                feld.name
            );
            let hilfe = feld.hilfe.to_lowercase();
            for wort in VERBOTEN {
                assert!(
                    !hilfe.contains(wort),
                    "{}: „{}“ enthält „{wort}“: {}",
                    programm.titel,
                    feld.name,
                    feld.hilfe
                );
            }
            // Ein Beispiel mit Adresse zeigt auf example.test oder einen Platzhalter,
            // nie auf eine echte Firma.
            if hilfe.contains("z. b.") && hilfe.contains("://") {
                assert!(
                    hilfe.contains("example.test") || hilfe.contains("<firma>"),
                    "{}: „{}“ nennt ein echtes Beispiel: {}",
                    programm.titel,
                    feld.name,
                    feld.hilfe
                );
            }
        }
    }
}

#[test]
fn jedes_werkzeug_ist_lesend_gekennzeichnet_oder_hat_einen_schreibschalter() {
    for programm in programme() {
        let schreibend: Vec<&str> = programm
            .werkzeuge
            .iter()
            .filter(|werkzeug| ist_lesend(werkzeug) != Some(true))
            .map(|werkzeug| werkzeug.name.as_ref())
            .collect();

        for werkzeug in &programm.werkzeuge {
            assert!(
                ist_lesend(werkzeug).is_some(),
                "{}: „{}“ trägt keine Kennzeichnung",
                programm.titel,
                werkzeug.name
            );
        }

        if schreibend.is_empty() {
            continue;
        }
        assert!(
            programm
                .felder
                .iter()
                .any(|feld| feld.name == FELD_SCHREIBEN),
            "{} hat schreibende Werkzeuge ({schreibend:?}), aber kein Feld „{FELD_SCHREIBEN}“",
            programm.titel
        );
        // Der Schalter ist im Zweifel aus.
        let schalter = programm
            .felder
            .iter()
            .find(|feld| feld.name == FELD_SCHREIBEN)
            .expect("geprüft");
        assert_eq!(
            schalter.standard,
            Some("aus"),
            "{}: der Schreibschalter muss im Zweifel aus sein",
            programm.titel
        );
    }
}

#[test]
fn die_bekannten_lesenden_werkzeuge_sind_wirklich_lesend() {
    // Kein Werkzeug mit Nebenwirkung darf hier stehen. Wer eines aufnimmt,
    // muss begründen, warum es nichts ändert.
    let lesende: &[(&str, &str)] = &[
        ("Outline", "outline_read"),
        ("Outline", "outline_search"),
        ("Outline", "outline_list"),
        ("Outline", "outline_structure"),
        ("Outline", "outline_collections"),
        ("Outline", "outline_comments"),
        ("E-Mail", "email_read"),
        ("E-Mail", "email_fetch"),
        ("E-Mail", "email_search"),
        ("E-Mail", "email_list_mailboxes"),
        ("SSH", "ssh_status"),
        ("SSH", "ssh_list"),
    ];

    for programm in programme() {
        for (titel, name) in lesende {
            if *titel != programm.titel {
                continue;
            }
            let werkzeug = programm
                .werkzeuge
                .iter()
                .find(|werkzeug| werkzeug.name == *name)
                .unwrap_or_else(|| panic!("{}: „{name}“ fehlt", programm.titel));
            assert_eq!(
                ist_lesend(werkzeug),
                Some(true),
                "{}: „{name}“ muss als lesend gekennzeichnet sein",
                programm.titel
            );
            // Schreibende Werkzeuge dürfen hier nicht auftauchen.
            assert!(
                !werkzeug
                    .annotations
                    .as_ref()
                    .and_then(|a| a.destructive_hint)
                    .unwrap_or(false),
                "{}: „{name}“ ist als zerstoerend gekennzeichnet",
                programm.titel
            );
        }
    }
}

#[test]
fn ohne_schreibschalter_ist_jedes_schreibende_werkzeug_gesperrt() {
    // Der Kern entscheidet nur über die Kennzeichnung – dieselbe Regel für alle
    // Programme. Ein schreibendes Werkzeug ist ohne Schalter immer gesperrt.
    for programm in programme() {
        for werkzeug in &programm.werkzeuge {
            if ist_lesend(werkzeug) == Some(true) {
                continue;
            }
            let antwort = quiver_tools_kern::schutz::schreibschutz(
                &programm.werkzeuge,
                &werkzeug.name,
                true,
                false,
                programm.titel,
                quiver_tools_kern::TITEL_SCHREIBEN,
            );
            assert!(
                antwort.is_err(),
                "{}: „{}“ wäre ohne Schalter aufrufbar",
                programm.titel,
                werkzeug.name
            );
        }
    }
}
