// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Reine Formatierung: Daten → Text (AGENTS.md §2.4).
//!
//! Kein Netz, kein Zustand. Antworten sind knapp – Agenten zahlen Tokens –,
//! und lange Inhalte tragen einen Kürzungshinweis.

use quiver_tools_kern::kuerzen;

use crate::adt::daten::{
    Abfrage, Aktivierung, Auftragsobjekt, Aufzeichnung, Aussage, Baumknoten, Dump, Hotspot,
    Objektart, Sperre, Syntaxmeldung, Testklasse, Textelement, Transport, Transportdetail, Treffer,
    Verwendung,
};
use crate::systeme::System;
use crate::werkzeug::TITEL;

/// Eine Liste von Systemen (ohne Passwort).
pub fn status(systeme: &[System]) -> String {
    if systeme.is_empty() {
        return "Kein SAP-System eingerichtet.".to_string();
    }
    let zeilen: Vec<String> = systeme
        .iter()
        .map(|system| format!("- {}", system.beschreibung()))
        .collect();
    format!("{} SAP-System(e):\n{}", systeme.len(), zeilen.join("\n"))
}

/// Das Ergebnis von `abap_test_connection`.
pub fn verbindung(system: &System, bereiche: &[String]) -> String {
    let mut text = format!(
        "Anmeldung an {} erfolgreich: {} als {} (Mandant {}, Sprache {}).",
        system.name,
        system.url,
        system.anmeldung(),
        system.mandant,
        system.sprache
    );
    if !bereiche.is_empty() {
        let kurz: Vec<&str> = bereiche.iter().take(8).map(String::as_str).collect();
        text.push_str(&format!(
            "\nADT-Bereiche ({}): {}",
            bereiche.len(),
            kurz.join(", ")
        ));
        if bereiche.len() > kurz.len() {
            text.push_str(", …");
        }
    }
    text
}

/// Die Treffer einer Suche.
pub fn suche(abfrage: &str, treffer: &[Treffer]) -> String {
    if treffer.is_empty() {
        return format!("Kein Objekt gefunden für „{abfrage}“.");
    }
    let zeilen: Vec<String> = treffer
        .iter()
        .map(|treffer| {
            let mut teile = vec![treffer.name.clone(), format!("type={}", treffer.typ)];
            if !treffer.paket.is_empty() {
                teile.push(format!("package={}", treffer.paket));
            }
            if !treffer.beschreibung.is_empty() {
                teile.push(treffer.beschreibung.clone());
            }
            format!("- {}  ({})", teile.join(" | "), treffer.uri)
        })
        .collect();
    format!(
        "{} Objekt(e) für „{abfrage}“:\n{}",
        treffer.len(),
        zeilen.join("\n")
    )
}

/// Die Struktur eines Objekts.
pub fn objektstruktur(objekt: &crate::adt::daten::Objekt) -> String {
    let mut text = format!("Name: {}", objekt.name);
    text.push_str(&format!("\nTyp: {}", objekt.typ));
    if !objekt.beschreibung.is_empty() {
        text.push_str(&format!("\nBeschreibung: {}", objekt.beschreibung));
    }
    text.push_str(&format!(
        "\nVerantwortlich: {}",
        ortfrage(&objekt.verantwortlich)
    ));
    text.push_str(&format!("\nSprache: {}", ortfrage(&objekt.sprache)));
    text.push_str(&format!("\nObjekt: {}", objekt.objekt_url));
    text.push_str(&format!("\nQuelltext: {}", objekt.quelltext_url));
    if !objekt.includes.is_empty() {
        let arten: Vec<&str> = objekt
            .includes
            .iter()
            .map(|include| include.art.as_str())
            .collect();
        text.push_str(&format!("\nIncludes: {}", arten.join(", ")));
    }
    text
}

/// Ein Fragezeichen, wenn ADT nichts geliefert hat.
fn ortfrage(wert: &str) -> &str {
    if wert.is_empty() {
        "?"
    } else {
        wert
    }
}

/// Ein Quelltext, gekürzt auf `max_zeichen`.
pub fn quelltext(name: &str, url: &str, quelle: &str, max_zeichen: usize) -> String {
    let (gekuerzt, weggelassen) = kuerzen(quelle, max_zeichen);
    let zeilen = quelle.lines().count();
    let mut text = format!("{name} ({url}, {zeilen} Zeile(n)):\n{gekuerzt}");
    if let Some(weggelassen) = weggelassen {
        text.push_str(&format!(
            "\n\n… {weggelassen} Zeichen weggelassen (max_zeichen={max_zeichen})."
        ));
    }
    text
}

/// Eine Wo-eingesetzt-Liste.
pub fn verwendungen(verwendungen: &[Verwendung]) -> String {
    if verwendungen.is_empty() {
        return "Keine Verwendung gefunden.".to_string();
    }
    let zeilen: Vec<String> = verwendungen
        .iter()
        .map(|verwendung| {
            let typ = if verwendung.typ.is_empty() {
                "?".to_string()
            } else {
                verwendung.typ.clone()
            };
            if verwendung.beschreibung.is_empty() {
                format!("- {} ({typ})", verwendung.name)
            } else {
                format!(
                    "- {} ({typ}) – {}",
                    verwendung.name, verwendung.beschreibung
                )
            }
        })
        .collect();
    format!(
        "{} Verwendung(en):\n{}",
        verwendungen.len(),
        zeilen.join("\n")
    )
}

/// Die durchsuchbaren Objektarten.
pub fn objektarten(arten: &[Objektart]) -> String {
    if arten.is_empty() {
        return "Keine Objektart verfügbar.".to_string();
    }
    let zeilen: Vec<String> = arten
        .iter()
        .map(|art| {
            let mut text = format!("- {}", art.name);
            if !art.typ.is_empty() {
                text.push_str(&format!(" ({})", art.typ));
            }
            if !art.beschreibung.is_empty() {
                text.push_str(&format!(" — {}", art.beschreibung));
            }
            if !art.verwendet_fuer.is_empty() {
                text.push_str(&format!(" [{}]", art.verwendet_fuer.join(", ")));
            }
            text
        })
        .collect();
    format!("{} Objektart(en):\n{}", arten.len(), zeilen.join("\n"))
}

/// Der Inhalt eines Pakets.
pub fn baumknoten(knoten: &[Baumknoten]) -> String {
    if knoten.is_empty() {
        return "Keine Objekte im Knoten gefunden.".to_string();
    }
    let zeilen: Vec<String> = knoten
        .iter()
        .map(|knoten| {
            let beschreibung = if knoten.beschreibung.is_empty() {
                String::new()
            } else {
                format!(" ({})", knoten.beschreibung)
            };
            format!("- {}  [{}]{beschreibung}", knoten.name, knoten.typ)
        })
        .collect();
    format!("{} Objekt(e):\n{}", knoten.len(), zeilen.join("\n"))
}

/// Eine Tabelle aus Spalten und Zeilen, ausgerichtet.
pub fn abfrage(abfrage: &Abfrage) -> String {
    if abfrage.zeilen.is_empty() {
        return "Die Abfrage hat keine Zeilen geliefert.".to_string();
    }
    let spalten = &abfrage.spalten;
    let breiten: Vec<usize> = spalten
        .iter()
        .enumerate()
        .map(|(index, name)| {
            abfrage
                .zeilen
                .iter()
                .filter_map(|zeile| zeile.get(index))
                .map(|wert| wert.chars().count())
                .chain(std::iter::once(name.chars().count()))
                .max()
                .unwrap_or(0)
        })
        .collect();

    let zeile_bauen = |zellen: &[String]| {
        zellen
            .iter()
            .enumerate()
            .map(|(index, wert)| {
                let breite = breiten.get(index).copied().unwrap_or(0);
                format!("{wert:breite$}")
            })
            .collect::<Vec<_>>()
            .join(" | ")
            .trim_end()
            .to_string()
    };

    let mut zeilen = vec![
        zeile_bauen(spalten),
        breiten
            .iter()
            .map(|breite| "-".repeat(*breite))
            .collect::<Vec<_>>()
            .join("-+-"),
    ];
    for zeile in &abfrage.zeilen {
        zeilen.push(zeile_bauen(zeile));
    }
    zeilen.push(format!("\n{} Zeile(n).", abfrage.zeilen.len()));
    format!(
        "{} Spalte(n): {}\n{}",
        spalten.len(),
        spalten.join(", "),
        zeilen.join("\n")
    )
}

/// Die Meldungen eines Syntax-Checks.
pub fn syntax(meldungen: &[Syntaxmeldung]) -> String {
    if meldungen.is_empty() {
        return "Der Syntax-Check hat nichts zu beanstanden.".to_string();
    }
    let zeilen: Vec<String> = meldungen
        .iter()
        .map(|meldung| {
            let schwere = if meldung.schwere.is_empty() {
                "?".to_string()
            } else {
                meldung.schwere.clone()
            };
            match meldung.zeile {
                Some(zeile) => format!("- [{schwere}] Zeile {zeile}: {}", meldung.text),
                None => format!("- [{schwere}] {}", meldung.text),
            }
        })
        .collect();
    format!("{} Meldung(en):\n{}", meldungen.len(), zeilen.join("\n"))
}

/// Das Ergebnis eines ABAP-Unit-Laufs.
pub fn unit_tests(klassen: &[Testklasse]) -> String {
    let methoden: Vec<&crate::adt::daten::Testmethode> = klassen
        .iter()
        .flat_map(|klasse| klasse.methoden.iter())
        .collect();
    if methoden.is_empty() {
        return "Keine Testklasse gefunden oder kein Test gelaufen.".to_string();
    }
    let fehlgeschlagen = methoden
        .iter()
        .filter(|methode| methode.ergebnis == "fehlgeschlagen")
        .count();
    let mut zeilen = vec![format!(
        "{} Testmethode(n), {} fehlgeschlagen.",
        methoden.len(),
        fehlgeschlagen
    )];
    for klasse in klassen {
        zeilen.push(format!(
            "Klasse: {} (risiko={})",
            klasse.name, klasse.risiko
        ));
        for methode in &klasse.methoden {
            zeilen.push(format!("  - {}: {}", methode.name, methode.ergebnis));
            for meldung in methode.meldungen.iter().take(5) {
                for zeile in meldung.lines() {
                    zeilen.push(format!("      {zeile}"));
                }
            }
        }
    }
    zeilen.join("\n")
}

/// Die Befunde einer ATC-Prüfung.
pub fn atc(befunde: &[crate::adt::daten::AtcBefund]) -> String {
    if befunde.is_empty() {
        return "Die ATC-Prüfung hat nichts beanstandet.".to_string();
    }
    let zeilen: Vec<String> = befunde
        .iter()
        .map(|befund| {
            let ort = match befund.zeile {
                Some(zeile) => format!(" (Zeile {zeile})"),
                None => String::new(),
            };
            format!(
                "- [P{}] {} ({}): {} — {}{ort}",
                befund.prio, befund.objekt, befund.typ, befund.pruefung, befund.nachricht
            )
        })
        .collect();
    format!("{} ATC-Befund(e):\n{}", befunde.len(), zeilen.join("\n"))
}

/// Die Kurzspeicherauszüge.
pub fn dumps(dumps: &[Dump]) -> String {
    if dumps.is_empty() {
        return "Kein Kurzspeicherauszug vorhanden.".to_string();
    }
    let zeilen: Vec<String> = dumps
        .iter()
        .map(|dump| {
            let id = if dump.id.is_empty() {
                String::new()
            } else {
                format!(" [{}]", dump.id)
            };
            format!("- {}{id}: {}", dump.art, dump.text)
        })
        .collect();
    format!(
        "{} Kurzspeicherauszug/-auszüge:\n{}",
        dumps.len(),
        zeilen.join("\n")
    )
}

/// Die Aufzeichnungen eines Benutzers.
pub fn aufzeichnungen(traces: &[Aufzeichnung]) -> String {
    if traces.is_empty() {
        return "Keine Aufzeichnung für diesen Benutzer gefunden.".to_string();
    }
    let zeilen: Vec<String> = traces
        .iter()
        .map(|trace| {
            let objekt = if trace.objekt.is_empty() {
                String::new()
            } else {
                format!(" ({})", trace.objekt)
            };
            format!(
                "- {}{objekt} [{}] {:.0}s — id={}",
                trace.titel, trace.zustand, trace.laufzeit, trace.id
            )
        })
        .collect();
    format!("{} Aufzeichnung(en):\n{}", traces.len(), zeilen.join("\n"))
}

/// Trefferliste und Aussagen einer Aufzeichnung.
pub fn trace_auswertung(hotspots: &[Hotspot], aussagen: &[Aussage]) -> String {
    let mut zeilen = Vec::new();
    if !aussagen.is_empty() {
        zeilen.push("Aussagen (aggregierter Aufrufbaum):".to_string());
        for aussage in aussagen.iter().take(50) {
            zeilen.push(format!(
                "  - {} — Aufrufe={}, brutto={:.1} ms",
                aussage.beschreibung, aussage.aufrufe, aussage.brutto
            ));
        }
    }
    if !hotspots.is_empty() {
        if !zeilen.is_empty() {
            zeilen.push(String::new());
        }
        zeilen.push("Trefferliste (Hot Spots):".to_string());
        let mut sortiert: Vec<&Hotspot> = hotspots.iter().collect();
        sortiert.sort_by(|a, b| {
            b.brutto
                .partial_cmp(&a.brutto)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for hotspot in sortiert.iter().take(25) {
            zeilen.push(format!(
                "  - {} — Aufrufe={}, brutto={:.1} ms ({:.0} %)",
                hotspot.beschreibung, hotspot.aufrufe, hotspot.brutto, hotspot.anteil
            ));
        }
    }
    if zeilen.is_empty() {
        return "Die Aufzeichnung hat keine Aussagen und keine Treffer.".to_string();
    }
    zeilen.join("\n")
}

/// Die Aufträge eines Benutzers.
pub fn transporte(transporte: &[Transport]) -> String {
    if transporte.is_empty() {
        return "Kein Transportauftrag für diesen Benutzer gefunden.".to_string();
    }
    let mut zeilen = Vec::new();
    let mut letzte_art = "";
    for transport in transporte {
        if transport.art != letzte_art {
            zeilen.push(format!("{}:", transport.art));
            letzte_art = &transport.art;
        }
        zeilen.push(format!(
            "  - {} [{}] {} (Ziel {}, Inhaber {})",
            transport.nummer,
            transport.status,
            transport.beschreibung,
            transport.ziel,
            transport.inhaber
        ));
    }
    format!(
        "{} Auftrag/Aufträge:\n{}",
        transporte.len(),
        zeilen.join("\n")
    )
}

/// Ein Auftrag mit Aufgaben und Objekten.
pub fn transportdetails(detail: &Transportdetail) -> String {
    let objekte = |objekte: &[Auftragsobjekt]| -> Vec<String> {
        objekte
            .iter()
            .map(|objekt| format!("{} ({})", objekt.name, objekt.typ))
            .collect()
    };

    let mut zeilen = vec![
        format!("Transport: {}", detail.nummer),
        format!("Beschreibung: {}", detail.beschreibung),
        format!("Status: {}", detail.status),
        format!("Inhaber: {}", detail.inhaber),
    ];
    if !detail.objekte.is_empty() {
        zeilen.push("Objekte:".to_string());
        for objekt in objekte(&detail.objekte) {
            zeilen.push(format!("  - {objekt}"));
        }
    }
    if !detail.aufgaben.is_empty() {
        zeilen.push("Aufgaben:".to_string());
        for aufgabe in &detail.aufgaben {
            zeilen.push(format!("  {}: {}", aufgabe.nummer, aufgabe.beschreibung));
            for objekt in objekte(&aufgabe.objekte) {
                zeilen.push(format!("    - {objekt}"));
            }
        }
    }
    zeilen.join("\n")
}

/// Die Textelemente eines Objekts.
pub fn textelemente(elemente: &[Textelement], kategorie: &str, programm: &str) -> String {
    if elemente.is_empty() {
        return format!("Keine Textelemente der Art „{kategorie}“ für {programm} gefunden.");
    }
    let zeilen: Vec<String> = elemente
        .iter()
        .map(|element| {
            let grenze = match element.max_laenge {
                Some(max) => format!(" (max {max})"),
                None => String::new(),
            };
            format!("- {} = {}{grenze}", element.id, element.text)
        })
        .collect();
    format!(
        "{} Textelement(e) der Art „{kategorie}“ für {programm}:\n{}",
        elemente.len(),
        zeilen.join("\n")
    )
}

// ------------------------------------------------- Schreiben (Teil 2)

/// Der Hinweis, wenn „schreiben“ für ein System aus ist (Aufgabe 08).
///
/// Wird **vor** jeder Anfrage an SAP geantwortet.
pub fn schreiben_aus(system: &str) -> String {
    format!(
        "Schreiben ist für {system} aus – im quiver-Fenster unter Tools → {TITEL} → Systeme → \
         {system} ‚schreiben‘ einschalten."
    )
}

/// Die Ablehnung eines Objekts außerhalb des Kundennamensraums.
pub fn namensraum(name: &str) -> String {
    format!(
        "„{name}“ liegt nicht im Kundennamensraum. Geschrieben, angelegt und gesperrt werden nur \
         Objekte mit Z*, Y* oder /…/ im Namen – SAP-Standard und fremde Namensräume bleiben \
         unberührt."
    )
}

/// `abap_lock`: Handle, Objekt, Transport und der Hinweis zum Freigeben.
pub fn sperre(objekt: &str, url: &str, sperre: &Sperre) -> String {
    let mut text = format!(
        "{objekt} ist gesperrt ({url}).\nLock-Handle: {}",
        sperre.handle
    );
    if sperre.lokal || sperre.auftrag.trim().is_empty() {
        text.push_str("\nTransport: nicht nötig (lokales Objekt).");
    } else {
        text.push_str(&format!(
            "\nTransport: {} – beim Schreiben als `transport` mitgeben.",
            sperre.auftrag
        ));
    }
    text.push_str(
        "\nFür abap_write als `lockHandle` mitgeben oder mit abap_unlock freigeben – der Handle \
         gilt nur in dieser Sitzung.",
    );
    text
}

/// `abap_unlock`: die Sperre ist weg.
pub fn entsperrt(objekt: &str, url: &str) -> String {
    format!("{objekt} ist freigegeben ({url}).")
}

/// `abap_write`: was geschrieben wurde und was mit der Sperre passiert ist.
pub fn geschrieben(name: &str, zeichen: usize, freigegeben: bool) -> String {
    let sperre = if freigegeben {
        "Lock freigegeben"
    } else {
        "Lock bleibt"
    };
    format!(
        "Quelltext geschrieben: {name} ({zeichen} Zeichen). {sperre}. Mit abap_activate \
         aktivieren."
    )
}

/// `abap_activate`: Erfolg und Meldungen (Typ, Zeile, Text), höchstens 50.
pub fn aktivierung(name: &str, ergebnis: &Aktivierung) -> String {
    let mut text = if ergebnis.erfolg {
        format!("{name} ist aktiviert.")
    } else {
        format!("Aktivierung von {name} nicht erfolgreich.")
    };
    if ergebnis.inaktiv > 0 {
        text.push_str(&format!(
            "\nAchtung: {} Objekt(e) blieben inaktiv.",
            ergebnis.inaktiv
        ));
    }
    if !ergebnis.meldungen.is_empty() {
        let gezeigt: Vec<String> = ergebnis
            .meldungen
            .iter()
            .take(50)
            .map(|meldung| {
                let art = if meldung.art.is_empty() {
                    "?"
                } else {
                    meldung.art.as_str()
                };
                let zeile = match meldung.zeile {
                    Some(zeile) => format!("Zeile {zeile}"),
                    None => "ohne Zeile".to_string(),
                };
                format!("- [{art}] {zeile}: {}", meldung.text)
            })
            .collect();
        text.push_str(&format!(
            "\nMeldungen ({}):\n{}",
            ergebnis.meldungen.len(),
            gezeigt.join("\n")
        ));
        if ergebnis.meldungen.len() > gezeigt.len() {
            text.push_str(&format!(
                "\n… {} weitere Meldung(en) nicht gezeigt.",
                ergebnis.meldungen.len() - gezeigt.len()
            ));
        }
    }
    text
}

/// `abap_create_object`: die Adresse des neuen Objekts.
pub fn neues_objekt(typ: &str, name: &str, paket: &str, url: &str) -> String {
    format!(
        "{typ} {name} in {paket} angelegt: {url}\nMit abap_write den Quelltext schreiben und \
         danach mit abap_activate aktivieren."
    )
}

/// `abap_set_text_elements`: wie viele Elemente geschrieben wurden.
pub fn textelemente_geschrieben(
    name: &str,
    kategorie: &str,
    anzahl: usize,
    freigegeben: bool,
) -> String {
    let sperre = if freigegeben {
        "Lock freigegeben"
    } else {
        "Lock bleibt"
    };
    format!("{anzahl} Textelement(e) der Art „{kategorie}“ für {name} geschrieben. {sperre}.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adt::daten::{AtcBefund, Aufgabe, Auftragsobjekt, Include, Objekt, Testmethode};
    use crate::systeme::System;

    fn system() -> System {
        System {
            name: "dev-100".into(),
            benutzer: "DEVELOPER".into(),
            url: "https://sap.example.test:44300".into(),
            mandant: "100".into(),
            sprache: "DE".into(),
            passwort: "geheim-4711".into(),
            zertifikat_pruefen: true,
            schreiben: false,
        }
    }

    #[test]
    fn status_nennt_die_systeme_ohne_passwort() {
        let text = status(&[system()]);
        assert!(text.starts_with("1 SAP-System(e):"), "{text}");
        assert!(text.contains("dev-100"), "{text}");
        assert!(text.contains("Mandant 100"), "{text}");
        assert!(text.contains("Schreiben aus"), "{text}");
        assert!(!text.contains("geheim-4711"), "{text}");
        assert_eq!(status(&[]), "Kein SAP-System eingerichtet.");
    }

    #[test]
    fn verbindung_und_quelltext_tragen_die_kerndaten() {
        let text = verbindung(&system(), &["Objekte".into(), "Daten".into()]);
        assert!(text.contains("Anmeldung an dev-100 erfolgreich"), "{text}");
        assert!(text.contains("DEVELOPER@"), "{text}");
        assert!(text.contains("ADT-Bereiche (2): Objekte, Daten"), "{text}");
        assert!(!text.contains("geheim-4711"), "{text}");

        let quelle = "REPORT zreport.\nWRITE 'Hallo'.\n";
        let text = quelltext("ZREPORT", "/source/main", quelle, 20);
        assert!(text.contains("2 Zeile(n)"), "{text}");
        assert!(text.contains("Zeichen weggelassen"), "{text}");

        let ungekuerzt = quelltext("ZREPORT", "/source/main", quelle, 1000);
        assert!(!ungekuerzt.contains("weggelassen"), "{ungekuerzt}");
    }

    #[test]
    fn suche_und_struktur_sind_knapp() {
        let treffer = vec![Treffer {
            name: "ZCL_DEMO".into(),
            typ: "CLAS/OC".into(),
            paket: "$TMP".into(),
            beschreibung: "Demo".into(),
            uri: "/sap/bc/adt/oo/classes/zcl_demo".into(),
        }];
        let text = suche("ZCL_*", &treffer);
        assert!(text.contains("1 Objekt(e) für „ZCL_*“"), "{text}");
        assert!(
            text.contains("- ZCL_DEMO | type=CLAS/OC | package=$TMP | Demo"),
            "{text}"
        );
        assert_eq!(suche("X", &[]), "Kein Objekt gefunden für „X“.");

        let objekt = Objekt {
            name: "ZCL_DEMO".into(),
            typ: "CLAS/OC".into(),
            beschreibung: "Demo".into(),
            verantwortlich: "DEVELOPER".into(),
            sprache: "DE".into(),
            objekt_url: "/sap/bc/adt/oo/classes/zcl_demo".into(),
            quelltext_url: "/sap/bc/adt/oo/classes/zcl_demo/includes/implementations".into(),
            includes: vec![
                Include {
                    art: "definitions".into(),
                    url: "d".into(),
                },
                Include {
                    art: "main".into(),
                    url: "m".into(),
                },
            ],
        };
        let text = objektstruktur(&objekt);
        assert!(text.contains("Name: ZCL_DEMO"), "{text}");
        assert!(text.contains("Includes: definitions, main"), "{text}");
        assert!(text.contains("Quelltext: "), "{text}");
    }

    #[test]
    fn eine_tabelle_wird_ausgerichtet() {
        let tabelle = Abfrage {
            spalten: vec!["CARRID".into(), "CONNID".into()],
            zeilen: vec![
                vec!["AA".into(), "17".into()],
                vec!["LH".into(), "0400".into()],
            ],
        };
        let text = abfrage(&tabelle);
        let zeilen: Vec<&str> = text.lines().collect();
        assert_eq!(zeilen[1], "CARRID | CONNID");
        assert_eq!(zeilen[2], "-------+-------");
        assert_eq!(zeilen[3], "AA     | 17");
        assert_eq!(zeilen[4], "LH     | 0400");
        assert!(text.contains("2 Zeile(n)."), "{text}");
        assert_eq!(
            super::abfrage(&Abfrage::default()),
            "Die Abfrage hat keine Zeilen geliefert."
        );
    }

    #[test]
    fn syntax_tests_atc_und_dumps() {
        let text = syntax(&[Syntaxmeldung {
            schwere: "E".into(),
            zeile: Some(12),
            text: "Feld unbekannt".into(),
        }]);
        assert_eq!(text, "1 Meldung(en):\n- [E] Zeile 12: Feld unbekannt");
        assert_eq!(syntax(&[]), "Der Syntax-Check hat nichts zu beanstanden.");

        let klassen = vec![Testklasse {
            name: "LTCL_DEMO".into(),
            risiko: "harmless".into(),
            methoden: vec![
                Testmethode {
                    name: "TEST_GUT".into(),
                    ergebnis: "bestanden".into(),
                    meldungen: Vec::new(),
                },
                Testmethode {
                    name: "TEST_SCHLECHT".into(),
                    ergebnis: "fehlgeschlagen".into(),
                    meldungen: vec!["Erwartet 1, war 2".into()],
                },
            ],
        }];
        let text = unit_tests(&klassen);
        assert!(
            text.starts_with("2 Testmethode(n), 1 fehlgeschlagen."),
            "{text}"
        );
        assert!(
            text.contains("Klasse: LTCL_DEMO (risiko=harmless)"),
            "{text}"
        );
        assert!(text.contains("      Erwartet 1, war 2"), "{text}");

        let befunde = vec![AtcBefund {
            objekt: "ZCL_DEMO".into(),
            typ: "CLAS/OC".into(),
            prio: 1,
            pruefung: "Sicherheit".into(),
            nachricht: "Fehlt".into(),
            zeile: Some(42),
        }];
        let text = atc(&befunde);
        assert!(
            text.contains("- [P1] ZCL_DEMO (CLAS/OC): Sicherheit — Fehlt (Zeile 42)"),
            "{text}"
        );

        let auszuege = vec![Dump {
            id: "4711".into(),
            art: "text".into(),
            text: "MESSAGE_TYPE_UNKNOWN".into(),
            autor: "DEVELOPER".into(),
        }];
        let text = dumps(&auszuege);
        assert!(
            text.contains("- text [4711]: MESSAGE_TYPE_UNKNOWN"),
            "{text}"
        );
    }

    #[test]
    fn traces_transporte_und_textelemente() {
        let traces = vec![Aufzeichnung {
            id: "0001".into(),
            titel: "Trace 1".into(),
            objekt: "ZREPORT".into(),
            zustand: "Aktiv".into(),
            laufzeit: 12.4,
        }];
        let text = aufzeichnungen(&traces);
        assert!(
            text.contains("- Trace 1 (ZREPORT) [Aktiv] 12s — id=0001"),
            "{text}"
        );

        let text = trace_auswertung(
            &[
                Hotspot {
                    beschreibung: "LOOP".into(),
                    aufrufe: 7,
                    brutto: 42.5,
                    anteil: 61.0,
                },
                Hotspot {
                    beschreibung: "SELECT".into(),
                    aufrufe: 3,
                    brutto: 99.0,
                    anteil: 30.0,
                },
            ],
            &[Aussage {
                beschreibung: "CALL METHOD".into(),
                aufrufe: 3,
                brutto: 1.5,
            }],
        );
        // Die Trefferliste ist nach Bruttozeit sortiert.
        let treffer_zeilen: Vec<&str> = text.lines().collect();
        assert!(
            treffer_zeilen
                .iter()
                .position(|zeile| zeile.contains("SELECT"))
                < treffer_zeilen
                    .iter()
                    .position(|zeile| zeile.contains("LOOP")),
            "{text}"
        );

        let auftraege = vec![
            Transport {
                art: "Workbench".into(),
                ziel: "A4H".into(),
                nummer: "DEVK900123".into(),
                status: "D".into(),
                beschreibung: "Demo".into(),
                inhaber: "DEVELOPER".into(),
            },
            Transport {
                art: "Customizing".into(),
                ziel: "A4H".into(),
                nummer: "A4HK900001".into(),
                status: "D".into(),
                beschreibung: "Tabellen".into(),
                inhaber: "DEVELOPER".into(),
            },
        ];
        let text = transporte(&auftraege);
        assert!(text.contains("Workbench:"), "{text}");
        assert!(text.contains("Customizing:"), "{text}");
        assert!(text.contains("DEVK900123 [D] Demo"), "{text}");

        let detail = Transportdetail {
            nummer: "DEVK900123".into(),
            beschreibung: "Demo".into(),
            status: "D".into(),
            inhaber: "DEVELOPER".into(),
            objekte: vec![Auftragsobjekt {
                name: "ZCL_DEMO".into(),
                typ: "CLAS/OC".into(),
            }],
            aufgaben: vec![Aufgabe {
                nummer: "DEVK900123".into(),
                beschreibung: "Aufgabe 1".into(),
                objekte: vec![Auftragsobjekt {
                    name: "ZCL_DEMO".into(),
                    typ: "CLAS/OC".into(),
                }],
            }],
        };
        let text = transportdetails(&detail);
        assert!(text.contains("Transport: DEVK900123"), "{text}");
        assert!(text.contains("  - ZCL_DEMO (CLAS/OC)"), "{text}");

        let text = textelemente(
            &[Textelement {
                id: "001".into(),
                text: "Anzahl".into(),
                max_laenge: Some(30),
            }],
            "symbols",
            "ZREPORT",
        );
        assert!(text.contains("- 001 = Anzahl (max 30)"), "{text}");
        assert_eq!(
            textelemente(&[], "symbols", "ZREPORT"),
            "Keine Textelemente der Art „symbols“ für ZREPORT gefunden."
        );
    }

    #[test]
    fn die_schreibenden_antworten_sind_knapp_und_deutlich() {
        let hinweis = schreiben_aus("dev-100");
        assert!(
            hinweis.contains("Schreiben ist für dev-100 aus"),
            "{hinweis}"
        );
        assert!(hinweis.contains("Tools → ABAP"), "{hinweis}");
        assert!(hinweis.contains("‚schreiben‘ einschalten"), "{hinweis}");

        let abgelehnt = namensraum("CL_ABAP_TYPEDESCR");
        assert!(abgelehnt.contains("Kundennamensraum"), "{abgelehnt}");
        assert!(abgelehnt.contains("CL_ABAP_TYPEDESCR"), "{abgelehnt}");
    }

    #[test]
    fn eine_sperre_und_ihre_freigabe_werden_erklaert() {
        let lokal = Sperre {
            handle: "LH4711".into(),
            auftrag: String::new(),
            lokal: true,
        };
        let text = sperre("ZCL_DEMO", "/sap/bc/adt/oo/classes/zcl_demo", &lokal);
        assert!(text.contains("Lock-Handle: LH4711"), "{text}");
        assert!(text.contains("nicht nötig (lokales Objekt)"), "{text}");
        assert!(text.contains("abap_unlock"), "{text}");

        let mit_auftrag = Sperre {
            handle: "LH0815".into(),
            auftrag: "DEVK900123".into(),
            lokal: false,
        };
        let text = sperre("ZCL_DEMO", "/sap/bc/adt/oo/classes/zcl_demo", &mit_auftrag);
        assert!(text.contains("Transport: DEVK900123"), "{text}");

        assert!(entsperrt("ZCL_DEMO", "/x").contains("ist freigegeben"));
    }

    #[test]
    fn geschrieben_nennt_zeichen_und_lock() {
        let text = geschrieben("ZCL_DEMO", 42, true);
        assert_eq!(
            text,
            "Quelltext geschrieben: ZCL_DEMO (42 Zeichen). Lock freigegeben. Mit abap_activate \
             aktivieren."
        );
        assert!(geschrieben("ZCL_DEMO", 1, false).contains("Lock bleibt"));
    }

    #[test]
    fn eine_aktivierung_zeigt_hoechstens_50_meldungen() {
        let meldung =
            |art: &str, zeile: Option<u32>, text: &str| crate::adt::daten::Aktivierungsmeldung {
                art: art.into(),
                zeile,
                text: text.into(),
            };
        let ergebnis = Aktivierung {
            erfolg: false,
            meldungen: vec![
                meldung("W", Some(1), "Nie gelesen."),
                meldung("E", Some(3), "Nicht erlaubt."),
            ],
            inaktiv: 2,
        };
        let text = aktivierung("ZCL_DEMO", &ergebnis);
        assert!(text.contains("nicht erfolgreich"), "{text}");
        assert!(text.contains("[E] Zeile 3: Nicht erlaubt."), "{text}");
        assert!(text.contains("[W] Zeile 1: Nie gelesen."), "{text}");
        assert!(text.contains("2 Objekt(e) blieben inaktiv"), "{text}");

        let viele = Aktivierung {
            erfolg: true,
            meldungen: (1..=60)
                .map(|nummer| meldung("I", None, &format!("Meldung {nummer}")))
                .collect(),
            inaktiv: 0,
        };
        let text = aktivierung("ZCL_DEMO", &viele);
        assert!(text.starts_with("ZCL_DEMO ist aktiviert."), "{text}");
        assert!(text.contains("Meldungen (60)"), "{text}");
        assert!(text.contains("Meldung 50"), "{text}");
        assert!(!text.contains("Meldung 51"), "{text}");
        assert!(text.contains("10 weitere Meldung(en)"), "{text}");
        assert!(text.contains("[I] ohne Zeile:"), "{text}");
    }

    #[test]
    fn die_antworten_zu_anlegen_und_textelementen_sind_knapp() {
        let text = neues_objekt(
            "CLAS/OC",
            "ZCL_NEU",
            "Z_PAKET",
            "/sap/bc/adt/oo/classes/zcl_neu",
        );
        assert!(
            text.contains("CLAS/OC ZCL_NEU in Z_PAKET angelegt"),
            "{text}"
        );
        assert!(text.contains("/sap/bc/adt/oo/classes/zcl_neu"), "{text}");
        assert!(text.contains("abap_activate"), "{text}");

        let text = textelemente_geschrieben("ZCL_DEMO", "symbols", 3, true);
        assert!(
            text.contains("3 Textelement(e) der Art „symbols“"),
            "{text}"
        );
        assert!(text.contains("Lock freigegeben"), "{text}");
        assert!(textelemente_geschrieben("ZCL_DEMO", "symbols", 1, false).contains("Lock bleibt"));
    }
}
