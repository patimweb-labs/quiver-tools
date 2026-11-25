// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Konfiguration aus der Umgebung (SPEC.md §3), soweit nicht in
//! [`m365_anmeldung::Konfig`]: Graph-Adresse und Zeitzone.

/// Standard-Graph-Adresse (Tests: Attrappe).
pub const STANDARD_GRAPH: &str = "https://graph.microsoft.com/v1.0";
/// Standard-Zeitzone für Termine (SPEC.md §4).
pub const STANDARD_ZEITZONE: &str = "Europe/Berlin";

/// Wert einer Umgebungsvariablen; getrimmt, leere Werte gelten als nicht gesetzt.
pub fn wert(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|roh| roh.trim().to_string())
        .filter(|gekuerzt| !gekuerzt.is_empty())
}

/// Wert oder Standard – die reine Regel, damit sie ohne Umgebung prüfbar ist.
pub fn wert_oder(roh: Option<String>, standard: &str) -> String {
    roh.unwrap_or_else(|| standard.to_string())
}

/// `M365_GRAPH_URL`, Standard [`STANDARD_GRAPH`].
pub fn graph_url() -> String {
    wert_oder(wert("M365_GRAPH_URL"), STANDARD_GRAPH)
}

/// `M365_ZEITZONE`, Standard [`STANDARD_ZEITZONE`].
pub fn zeitzone() -> String {
    wert_oder(wert("M365_ZEITZONE"), STANDARD_ZEITZONE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wert_oder_nimmt_standard_bei_leerem_wert() {
        assert_eq!(wert_oder(None, "x"), "x");
        assert_eq!(wert_oder(Some("y".to_string()), "x"), "y");
    }

    /// Name, den kein Test setzt – so hängt das Ergebnis nicht an der Umgebung.
    #[test]
    fn wert_ignoriert_leere_und_getrimmte_werte() {
        assert_eq!(wert("QUIVER_M365_GIBT_ES_NICHT"), None);
    }

    #[test]
    fn standards_stehen_in_der_spec() {
        assert_eq!(STANDARD_GRAPH, "https://graph.microsoft.com/v1.0");
        assert_eq!(STANDARD_ZEITZONE, "Europe/Berlin");
    }
}

// ---------------------------------------------------------------------------
// Schreibschutz je Bereich (Härtung Paket 4)
// ---------------------------------------------------------------------------

/// Ein Bereich, in dem geschrieben werden kann, mit Schalter und Beschriftung.
///
/// Jeder Schalter ist im Zweifel **aus**: fehlt er oder ist er leer, ändert das
/// Programm nichts. Ein lesendes Werkzeug braucht keinen Schalter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bereich {
    /// Kurzname, z. B. `mail`.
    pub name: &'static str,
    /// Name der Umgebungsvariablen (Feld im quiver-Fenster).
    pub variabale: &'static str,
    /// Beschriftung des Feldes im quiver-Fenster.
    pub titel: &'static str,
}

/// Alle Bereiche mit eigenem Schalter.
pub const BEREICHE: &[Bereich] = &[
    Bereich {
        name: "mail",
        variabale: "M365_SCHREIBEN_MAIL",
        titel: "Mail senden erlauben",
    },
    Bereich {
        name: "kalender",
        variabale: "M365_SCHREIBEN_KALENDER",
        titel: "Kalender ändern erlauben",
    },
    Bereich {
        name: "dateien",
        variabale: "M365_SCHREIBEN_DATEIEN",
        titel: "Dateien ändern erlauben",
    },
    Bereich {
        name: "teams",
        variabale: "M365_SCHREIBEN_TEAMS",
        titel: "Teams schreiben erlauben",
    },
    Bereich {
        name: "planner",
        variabale: "M365_SCHREIBEN_PLANNER",
        titel: "Planner und To Do ändern erlauben",
    },
    Bereich {
        name: "kontakte",
        variabale: "M365_SCHREIBEN_KONTAKTE",
        titel: "Kontakte ändern erlauben",
    },
    Bereich {
        name: "onenote",
        variabale: "M365_SCHREIBEN_ONENOTE",
        titel: "OneNote ändern erlauben",
    },
    Bereich {
        name: "allgemein",
        variabale: "M365_SCHREIBEN",
        titel: "Schreiben erlauben",
    },
];

/// Der Bibliothekseintrag für quiver: Anmeldung über den Knopf im Fenster und
/// je Bereich ein Schalter, Standard aus.
pub fn eintrag(befehl: Option<&str>) -> serde_json::Value {
    let mut umgebung = serde_json::Map::new();
    umgebung.insert("RUST_LOG".into(), "info".into());
    let mut felder = Vec::new();
    for bereich in BEREICHE
        .iter()
        .filter(|bereich| bereich.name != "allgemein")
    {
        let feld = format!("schreiben_{}", bereich.name);
        umgebung.insert(bereich.variabale.into(), format!("${{{feld}}}").into());
        felder.push(serde_json::json!({
            "name": feld,
            "titel": bereich.titel,
            "beschreibung": "Standard: aus. Lesen geht immer; ohne diesen Schalter ändert ein Agent hier nichts.",
            "art": "schalter",
            "quelle": "client",
            "pflicht": false,
            "standard": "aus",
        }));
    }
    serde_json::json!({
        "id": "microsoft-365",
        "version": env!("CARGO_PKG_VERSION"),
        "titel": "Microsoft 365",
        "beschreibung": "Mail, Kalender, Teams, Dateien, Planner, Kontakte und OneNote über Microsoft Graph – mit einer Anmeldung im Browser.",
        "herkunft": "eigen",
        "laufzeit": {
            "art": "stdio",
            "befehl": befehl.unwrap_or("quiver-m365"),
            "argumente": [],
            "umgebung": umgebung,
        },
        "felder": felder,
        "anmeldung": {
            "anmelden": ["anmelden"],
            "status": ["status"],
            "abmelden": ["abmelden"],
        },
    })
}

/// Ein Schalter aus der Umgebung: nur ein ausdrückliches Einschalten gilt.
pub fn schalter(name: &str) -> bool {
    schalter_mit(name, |name| std::env::var(name).ok())
}

/// Wie [`schalter`], aus einer beliebigen Quelle (Tests).
pub fn schalter_mit(name: &str, lesen: impl Fn(&str) -> Option<String>) -> bool {
    matches!(
        lesen(name)
            .map(|roh| roh.trim().to_string())
            .filter(|roh| !roh.is_empty())
            .map(|roh| roh.to_ascii_lowercase()),
        Some(wert) if matches!(wert.as_str(), "an" | "ja" | "1" | "true" | "ein" | "on")
    )
}

/// Ein Bereich nach Namen.
pub fn bereich(name: &str) -> &'static Bereich {
    BEREICHE
        .iter()
        .find(|bereich| bereich.name == name)
        .unwrap_or(&BEREICHE[BEREICHE.len() - 1])
}

/// Zu welchem Bereich gehört ein Werkzeug? Alles Unbekannte gilt als
/// „allgemein“ – im Zweifel zu.
pub fn bereich_fuer(werkzeug: &str) -> &'static str {
    let name = werkzeug.to_ascii_lowercase();
    let passt = |praefixe: &[&str]| praefixe.iter().any(|praefix| name.starts_with(praefix));
    if passt(&["mail_"]) {
        "mail"
    } else if passt(&["termin"]) {
        "kalender"
    } else if passt(&["datei", "ordner_", "sharepoint_"]) {
        "dateien"
    } else if passt(&["teams_"]) {
        "teams"
    } else if passt(&["planner_", "todo_"]) {
        "planner"
    } else if passt(&["kontakt", "personen_"]) {
        "kontakte"
    } else if passt(&["onenote_"]) {
        "onenote"
    } else {
        "allgemein"
    }
}

/// Darf in diesem Bereich geschrieben werden?
pub fn schreiben_erlaubt(bereich: &str) -> bool {
    schreiben_erlaubt_mit(bereich, |name| std::env::var(name).ok())
}

/// Wie [`schreiben_erlaubt`], aus einer beliebigen Quelle (Tests).
///
/// Der allgemeine Schalter `M365_SCHREIBEN` gilt für alle Bereiche – so lässt
/// sich das Schreiben mit einem Feld freigeben, ohne sieben zu setzen.
pub fn schreiben_erlaubt_mit(bereich: &str, lesen: impl Fn(&str) -> Option<String>) -> bool {
    let eintrag = bereich_als(bereich);
    schalter_mit(eintrag.variabale, &lesen)
        || (eintrag.name != "allgemein" && schalter_mit("M365_SCHREIBEN", &lesen))
}

/// Wie [`bereich`], aber ohne den Platzhalter für unbekannte Namen.
fn bereich_als(name: &str) -> &'static Bereich {
    BEREICHE
        .iter()
        .find(|bereich| bereich.name == name)
        .unwrap_or(&BEREICHE[BEREICHE.len() - 1])
}

/// Der Hinweis, den ein gesperrtes Werkzeug zurückgibt.
pub fn hinweis_gesperrt(werkzeug: &str, bereich: &str) -> String {
    let eintrag = bereich_als(bereich);
    format!(
        "„{werkzeug}“ ändert etwas. Für Microsoft 365 ist das nicht freigegeben – bitte im \
         quiver-Fenster unter Tools → Microsoft 365 das Feld „{}“ einschalten. Ohne diesen \
         Schalter geht keine Anfrage hinaus.",
        eintrag.titel
    )
}

#[cfg(test)]
mod schreibschutz_tests {
    use super::*;

    #[test]
    fn der_eintrag_hat_je_bereich_einen_schalter_standard_aus() {
        let eintrag = eintrag(Some("/opt/quiver-m365"));
        assert_eq!(eintrag["id"], "microsoft-365");
        assert_eq!(eintrag["laufzeit"]["befehl"], "/opt/quiver-m365");
        let felder = eintrag["felder"].as_array().unwrap();
        assert_eq!(felder.len(), BEREICHE.len() - 1);
        for feld in felder {
            assert_eq!(feld["art"], "schalter");
            assert_eq!(feld["standard"], "aus");
            let platzhalter = format!("${{{}}}", feld["name"].as_str().unwrap());
            assert!(eintrag["laufzeit"]["umgebung"]
                .as_object()
                .unwrap()
                .values()
                .any(|wert| wert == &serde_json::json!(platzhalter)));
        }
    }

    fn aus<'a>(werte: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            werte
                .iter()
                .find(|(schluessel, _)| *schluessel == name)
                .map(|(_, wert)| (*wert).to_string())
        }
    }

    #[test]
    fn jeder_bereich_hat_einen_eigenen_schalter_und_ist_im_zweifel_aus() {
        assert!(!schreiben_erlaubt_mit("mail", aus(&[])));
        assert!(!schreiben_erlaubt_mit(
            "mail",
            aus(&[("M365_SCHREIBEN_MAIL", "aus")])
        ));
        assert!(schreiben_erlaubt_mit(
            "mail",
            aus(&[("M365_SCHREIBEN_MAIL", "an")])
        ));
        // Der allgemeine Schalter gilt für alle Bereiche.
        assert!(schreiben_erlaubt_mit(
            "teams",
            aus(&[("M365_SCHREIBEN", "an")])
        ));
        // Ein Bereichsschalter gilt nicht für einen anderen.
        assert!(!schreiben_erlaubt_mit(
            "teams",
            aus(&[("M365_SCHREIBEN_MAIL", "an")])
        ));
    }

    #[test]
    fn werkzeuge_werden_ihrem_bereich_zugeordnet() {
        assert_eq!(bereich_fuer("mail_senden"), "mail");
        assert_eq!(bereich_fuer("mail_loeschen"), "mail");
        assert_eq!(bereich_fuer("termin_anlegen"), "kalender");
        assert_eq!(bereich_fuer("termin_loeschen"), "kalender");
        assert_eq!(bereich_fuer("datei_hochladen"), "dateien");
        assert_eq!(bereich_fuer("sharepoint_listeneintrag_anlegen"), "dateien");
        assert_eq!(bereich_fuer("teams_kanal_senden"), "teams");
        assert_eq!(bereich_fuer("planner_aufgabe_anlegen"), "planner");
        assert_eq!(bereich_fuer("todo_aufgabe_erledigen"), "planner");
        assert_eq!(bereich_fuer("kontakt_anlegen"), "kontakte");
        assert_eq!(bereich_fuer("onenote_seite_anlegen"), "onenote");
        // Unbekanntes gilt als allgemein – im Zweifel zu.
        assert_eq!(bereich_fuer("irgendwas"), "allgemein");
        assert!(!schreiben_erlaubt_mit("allgemein", aus(&[])));
    }

    #[test]
    fn der_hinweis_nennt_das_feld_und_die_quelle() {
        let text = hinweis_gesperrt("mail_senden", "mail");
        assert!(text.contains("mail_senden"), "{text}");
        assert!(text.contains("Mail senden erlauben"), "{text}");
        assert!(text.contains("quiver-Fenster"), "{text}");
        assert!(text.contains("Microsoft 365"), "{text}");
    }
}
