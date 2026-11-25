// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der Schreibschutz, an **einer** Stelle (SPEC §2a, Härtung Paket 4).
//!
//! Ein Werkzeug trägt in MCP die Kennzeichnung „nur lesend“ oder nicht. Der
//! Kern liest diese Kennzeichnung aus der Werkzeugliste **des Programms**, die
//! quiver und jeder Client sowieso bekommt – nicht aus dem Aufruf und nicht
//! aus einer zweiten Liste, die jemand pflegen müsste. Daraus folgt:
//!
//! - Ein Werkzeug **ohne** `read_only_hint` ist gesperrt, solange der User im
//!   quiver-Fenster nicht das Feld „Schreiben erlauben“ (`schreiben`,
//!   Standard **aus**) eingeschaltet hat. Dann geht **keine** Anfrage hinaus.
//! - Der Hinweis nennt das Feld und den Ort im quiver-Fenster; ein vorhandener
//!   Zugang fällt damit nicht still aus, sondern sagt, was einzuschalten ist.
//! - Ein unbekanntes Werkzeug gilt als schreibend (im Zweifel zu).
//!
//! Jedes Programm ruft [`schreibschutz`] einmal am Anfang seines `call_tool`
//! auf. Der Vertragstest über alle Programme prüft, dass jedes Werkzeug
//! entweder als lesend gekennzeichnet oder gesperrt ist.

use rmcp::model::{CallToolResult, Tool};

/// Feldname des Schalters „Schreiben erlauben“ im quiver-Fenster.
pub const FELD_SCHREIBEN: &str = "schreiben";
/// Beschriftung dieses Schalters.
pub const TITEL_SCHREIBEN: &str = "Schreiben erlauben";

/// Ist dieses Werkzeug in der Liste als **nur lesend** gekennzeichnet?
///
/// Nur ein ausdrückliches `read_only_hint = true` zählt; ein fehlender Wert und
/// ein unbekannter Name gelten als schreibend.
pub fn ist_lesend(werkzeuge: &[Tool], name: &str) -> bool {
    werkzeuge
        .iter()
        .find(|werkzeug| werkzeug.name == name)
        .and_then(|werkzeug| werkzeug.annotations.as_ref())
        .and_then(|annotationen| annotationen.read_only_hint)
        == Some(true)
}

/// Der Text, den ein gesperrtes Werkzeug zurückgibt.
///
/// `titel` ist der Anzeigename des Werkzeugs (quiver-Fenster), `feld_titel` die
/// Beschriftung des Schalters, der einzuschalten ist (bei den meisten
/// „Schreiben erlauben“, bei Microsoft 365 der Bereich).
pub fn hinweis_gesperrt(werkzeug: &str, titel: &str, feld_titel: &str) -> String {
    format!(
        "„{werkzeug}“ ändert etwas. Für {titel} ist das nicht freigegeben – bitte im \
         quiver-Fenster unter Tools → {titel} das Feld „{feld_titel}“ einschalten. \
         Ohne diesen Schalter geht keine Anfrage hinaus."
    )
}

/// Die zentrale Prüfung: darf dieses Werkzeug jetzt laufen?
///
/// `Ok(())` heißt ja. `Err(antwort)` ist der fertige Hinweis an den Agenten –
/// ein Text, **kein** MCP-Fehler, und ohne Anfrage nach außen.
///
/// Ein **unbekannter** Name geht durch: der Verteiler antwortet darauf mit
/// „unbekanntes Werkzeug“ und schickt nichts nach außen. So bleibt der Hinweis
/// für Tippfehler weg, ohne dass eine Lücke entsteht.
pub fn schreibschutz(
    werkzeuge: &[Tool],
    name: &str,
    zugang_da: bool,
    schreiben: bool,
    titel: &str,
    feld_titel: &str,
) -> Result<(), CallToolResult> {
    let bekannt = werkzeuge.iter().any(|werkzeug| werkzeug.name == name);
    if !bekannt || !zugang_da || schreiben || ist_lesend(werkzeuge, name) {
        return Ok(());
    }
    Err(crate::antwort(hinweis_gesperrt(name, titel, feld_titel)))
}

/// Der Hinweis beim Start nach dem Update: „Schreiben ist jetzt aus“.
///
/// Gibt es in diesem Programm überhaupt ein schreibendes Werkzeug und ist der
/// Schalter aus, steht der Satz auf stderr (dort landet das Log, das quiver
/// vom Werkzeug sieht). Sonst ist das Ergebnis `None`.
pub fn start_hinweis(werkzeuge: &[Tool], schreiben: bool, titel: &str) -> Option<String> {
    if schreiben {
        return None;
    }
    if werkzeuge.iter().all(|werkzeug| {
        werkzeug
            .annotations
            .as_ref()
            .and_then(|annotationen| annotationen.read_only_hint)
            == Some(true)
    }) {
        return None;
    }
    Some(format!(
        "Hinweis: Schreiben ist für {titel} jetzt aus. Ändernde Werkzeuge antworten mit einem \
         Hinweis, bis im quiver-Fenster unter Tools → {titel} das Feld „{TITEL_SCHREIBEN}“ \
         eingeschaltet wird. Lesende Werkzeuge sind nicht betroffen."
    ))
}

/// Der Start-Hinweis aus dem Zugang: hat das Werkzeug ein Feld `schreiben` und
/// ist es aus, nennt der Satz das Feld. Ohne dieses Feld gibt es nichts zu
/// sagen (die mehrfachen Zugänge prüfen das selbst).
pub fn start_hinweis_fuer(zugang: &crate::Zugang) -> Option<String> {
    let hat_feld = zugang
        .felder()
        .iter()
        .any(|feld| feld.name == FELD_SCHREIBEN);
    if !hat_feld || crate::schalter_aus(zugang.wert(FELD_SCHREIBEN)) {
        return None;
    }
    Some(format!(
        "Hinweis: Schreiben ist für {} jetzt aus. Ändernde Werkzeuge antworten mit einem Hinweis, \
         bis im quiver-Fenster unter Tools → {} das Feld „{TITEL_SCHREIBEN}“ eingeschaltet wird. \
         Lesende Werkzeuge sind nicht betroffen.",
        zugang.titel(),
        zugang.titel()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::{JsonObject, ToolAnnotations};

    fn werkzeug(name: &str, lesend: Option<bool>) -> Tool {
        let schema: JsonObject =
            serde_json::from_value(serde_json::json!({"type": "object", "properties": {}}))
                .unwrap();
        Tool::new(name.to_string(), format!("{name} für Tests"), schema).annotate(
            ToolAnnotations::from_raw(Some(name.to_string()), lesend, None, lesend, Some(false)),
        )
    }

    #[test]
    fn lesende_werkzeuge_laufen_immer() {
        let liste = vec![werkzeug("a_read", Some(true)), werkzeug("a_write", None)];
        assert!(schreibschutz(&liste, "a_read", true, false, "A", TITEL_SCHREIBEN).is_ok());
        assert!(schreibschutz(&liste, "a_write", true, true, "A", TITEL_SCHREIBEN).is_ok());
    }

    #[test]
    fn schreibende_werkzeuge_sind_ohne_schalter_gesperrt() {
        let liste = vec![werkzeug("a_read", Some(true)), werkzeug("a_write", None)];
        let antwort = schreibschutz(
            &liste,
            "a_write",
            true,
            false,
            "A-Werkzeug",
            TITEL_SCHREIBEN,
        )
        .unwrap_err();
        let text = antwort.content[0].as_text().unwrap().text.clone();
        assert!(text.contains("a_write"), "{text}");
        assert!(text.contains("A-Werkzeug"), "{text}");
        assert!(text.contains(TITEL_SCHREIBEN), "{text}");
        assert!(text.contains("quiver-Fenster"), "{text}");
    }

    #[test]
    fn ohne_kennzeichnung_gilt_als_schreibend_und_unbekanntes_geht_durch() {
        let liste = vec![werkzeug("a_write", None), werkzeug("ohne", None)];
        assert!(schreibschutz(&liste, "a_write", true, false, "A", TITEL_SCHREIBEN).is_err());
        assert!(schreibschutz(&liste, "ohne", true, false, "A", TITEL_SCHREIBEN).is_err());
        // Unbekannt beantwortet der Verteiler selbst – ohne Anfrage nach außen.
        assert!(schreibschutz(&liste, "gibtsnicht", true, false, "A", TITEL_SCHREIBEN).is_ok());
        assert!(!ist_lesend(&liste, "gibtsnicht"));
        // Ohne Zugang kommt der Hinweis auf das quiver-Fenster, nicht der Schalter.
        assert!(schreibschutz(&liste, "a_write", false, false, "A", TITEL_SCHREIBEN).is_ok());
    }

    #[test]
    fn der_start_hinweis_nennt_das_feld_und_nur_bei_bedarf() {
        let mit_schreiben = vec![werkzeug("a_read", Some(true)), werkzeug("a_write", None)];
        let text = start_hinweis(&mit_schreiben, false, "A").expect("Hinweis");
        assert!(text.contains("Schreiben ist"), "{text}");
        assert!(text.contains(TITEL_SCHREIBEN), "{text}");
        assert_eq!(start_hinweis(&mit_schreiben, true, "A"), None);

        let nur_lesen = vec![werkzeug("a_read", Some(true))];
        assert_eq!(start_hinweis(&nur_lesen, false, "A"), None);
    }

    #[test]
    fn der_start_hinweis_kommt_aus_dem_zugang() {
        use crate::{Feld, FeldArt, Profil, Werkzeug};

        struct Attrappe;
        impl Werkzeug for Attrappe {
            fn name(&self) -> &'static str {
                "attrappe"
            }
            fn titel(&self) -> &'static str {
                "Attrappen-Tool"
            }
            fn beschreibung(&self) -> &'static str {
                "Nur für Tests."
            }
            fn felder(&self) -> Vec<Feld> {
                vec![Feld {
                    name: FELD_SCHREIBEN,
                    titel: TITEL_SCHREIBEN,
                    art: FeldArt::Schalter,
                    pflicht: false,
                    standard: Some("aus"),
                    hilfe: "Ändernde Werkzeuge erlauben.",
                    link: None,
                }]
            }
            async fn testen(&self, _profil: &Profil) -> anyhow::Result<String> {
                Ok("erreichbar".to_string())
            }
        }

        let ohne = crate::Zugang::mit_umgebung(&Attrappe, |_| None);
        let text = start_hinweis_fuer(&ohne).expect("Hinweis");
        assert!(text.contains(TITEL_SCHREIBEN), "{text}");

        let an = crate::Zugang::mit_umgebung(&Attrappe, |_| Some("an".to_string()));
        assert_eq!(start_hinweis_fuer(&an), None);
    }
}
