// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Aus Daten wird Text (SPEC §6): knapp, ohne Emojis, ohne Nebenwirkung.
//!
//! Alles hier sind reine Funktionen – dieselben Daten ergeben denselben Text.
//! Lange Inhalte gehen durch [`quiver_tools_kern::kuerzen`], damit ein Agent
//! nicht für jede Seite Tokens bezahlt.

use quiver_tools_kern::kuerzen;

use crate::client::Seiten;
use crate::domain::{Dokument, Info, Knoten, Kommentar, Sammlung, Treffer};

/// Wie viele Zeichen ein Suchkontext höchstens belegt.
const KONTEXT_MAX: usize = 300;

/// Ein Datum aus Outline als `2026-09-02 10:11`; fehlt es, ein Strich.
pub fn kurzdatum(wert: Option<&str>) -> String {
    let Some(wert) = wert.map(str::trim).filter(|wert| !wert.is_empty()) else {
        return "-".to_string();
    };
    let zeichen: Vec<char> = wert.chars().collect();
    if zeichen.len() >= 16 && zeichen[10] == 'T' {
        let tag: String = zeichen[..10].iter().collect();
        let zeit: String = zeichen[11..16].iter().collect();
        return format!("{tag} {zeit}");
    }
    wert.to_string()
}

/// Zustand eines Dokuments als `[Entwurf]`, `[archiviert,Vorlage]` oder leer.
pub fn zustandszeichen(dokument: &Dokument) -> String {
    let mut teile: Vec<&str> = Vec::new();
    if let Some(zustand) = dokument.zustand() {
        teile.push(zustand);
    }
    if dokument.template {
        teile.push("Vorlage");
    }
    if teile.is_empty() {
        String::new()
    } else {
        format!(" [{}]", teile.join(","))
    }
}

/// Was `Werkzeug::testen` meldet (Unterbefehl `testen`).
pub fn info(info: &Info) -> String {
    let name = info
        .user
        .as_ref()
        .and_then(|nutzer| nutzer.name.as_deref())
        .unwrap_or("(unbekannt)");
    let team = info
        .team
        .as_ref()
        .and_then(|team| team.name.as_deref())
        .unwrap_or("(unbekannter Arbeitsbereich)");
    format!("Angemeldet als {name} in {team}")
}

/// Die Sammlungen eines Wikis.
pub fn sammlungen(sammlungen: &[Sammlung]) -> String {
    if sammlungen.is_empty() {
        return "Keine Sammlung gefunden. Der Benutzer des Schlüssels ist vielleicht in keiner Sammlung Mitglied.".to_string();
    }
    let mut zeilen = vec![format!("Sammlungen ({}):", sammlungen.len())];
    for sammlung in sammlungen {
        let beschreibung = sammlung
            .beschreibung
            .as_deref()
            .map(|text| text.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|text| !text.is_empty())
            .map(|text| format!(" – {}", kuerzen_roh(&text, 100)))
            .unwrap_or_default();
        zeilen.push(format!(
            "- {} (id {}){beschreibung}",
            sammlung.titel(),
            sammlung.id
        ));
    }
    zeilen.join("\n")
}

/// Der Dokumentbaum einer Sammlung.
pub fn struktur(wurzeln: &[Knoten], sammlung: Option<&str>) -> String {
    let kopf = match sammlung {
        Some(name) => format!("Dokumentbaum von „{name}“:"),
        None => "Dokumentbaum:".to_string(),
    };
    if wurzeln.is_empty() {
        return format!("{kopf}\n(keine Dokumente)");
    }
    let mut zeilen = vec![kopf];
    fn gehen(knoten: &[Knoten], tiefe: usize, zeilen: &mut Vec<String>) {
        for knoten in knoten {
            zeilen.push(format!(
                "{}- {} (id {})",
                "  ".repeat(tiefe),
                knoten.titel(),
                knoten.id
            ));
            gehen(&knoten.children, tiefe + 1, zeilen);
        }
    }
    gehen(wurzeln, 0, &mut zeilen);
    zeilen.join("\n")
}

/// Treffer einer Suche in **einem** Profil.
pub fn suche(treffer: &[Treffer], suche: &str, seiten: Option<&Seiten>) -> String {
    if treffer.is_empty() {
        return format!(
            "Kein Dokument passt zu „{suche}“. Weniger oder allgemeinere Wörter versuchen, oder den Sammlungsfilter weglassen."
        );
    }
    let mut zeilen = vec![format!(
        "Suche „{suche}“ – {} Treffer. Eine Seite mit outline_read lesen.",
        treffer.len()
    )];
    for (nummer, treffer) in treffer.iter().enumerate() {
        let dokument = &treffer.document;
        zeilen.push(format!(
            "{}. {}{}",
            nummer + 1,
            dokument.titel(),
            zustandszeichen(dokument)
        ));
        zeilen.push(format!(
            "   id {} | geändert {}",
            dokument.kennung(),
            kurzdatum(dokument.geaendert_am.as_deref())
        ));
        let kontext = treffer
            .context
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if !kontext.is_empty() {
            zeilen.push(format!("   {}", kuerzen_roh(&kontext, KONTEXT_MAX)));
        }
    }
    format!("{}{}", zeilen.join("\n"), weitere(seiten, treffer.len()))
}

/// Eine Liste von Dokumenten.
pub fn liste(dokumente: &[Dokument], ueberschrift: &str, seiten: Option<&Seiten>) -> String {
    if dokumente.is_empty() {
        return format!("{ueberschrift}: nichts gefunden.");
    }
    let mut zeilen = vec![format!("{ueberschrift} ({}):", dokumente.len())];
    for dokument in dokumente {
        let von = dokument
            .geaendert_von
            .as_ref()
            .and_then(|nutzer| nutzer.name.as_deref())
            .map(|name| format!(" von {name}"))
            .unwrap_or_default();
        zeilen.push(format!(
            "- {}{}",
            dokument.titel(),
            zustandszeichen(dokument)
        ));
        zeilen.push(format!(
            "  id {} | geändert {}{von}",
            dokument.kennung(),
            kurzdatum(dokument.geaendert_am.as_deref())
        ));
    }
    format!("{}{}", zeilen.join("\n"), weitere(seiten, dokumente.len()))
}

/// Ein Dokument mit Markdown-Rumpf, gekürzt auf `max_zeichen`.
///
/// `max_zeichen = 0` heißt „nicht kürzen“ (wie in der Vorlage).
pub fn dokument(dokument: &Dokument, max_zeichen: usize) -> String {
    let mut kopf = vec![format!(
        "Titel: {}{}",
        dokument.titel(),
        zustandszeichen(dokument)
    )];
    if !dokument.kennung().is_empty() {
        kopf.push(format!("Id: {}", dokument.kennung()));
    }
    if let Some(sammlung) = dokument.sammlung_id.as_deref() {
        kopf.push(format!("Sammlung: {sammlung}"));
    }
    if let Some(eltern) = dokument.eltern_id.as_deref() {
        kopf.push(format!("Elterndokument: {eltern}"));
    }
    if let Some(url) = dokument.url.as_deref() {
        kopf.push(format!("URL: {url}"));
    }
    let von = dokument
        .geaendert_von
        .as_ref()
        .and_then(|nutzer| nutzer.name.as_deref())
        .map(|name| format!(" von {name}"))
        .unwrap_or_default();
    kopf.push(format!(
        "Geändert: {}{von}",
        kurzdatum(dokument.geaendert_am.as_deref())
    ));

    let rumpf = if dokument.text.trim().is_empty() {
        "(diese Seite hat noch keinen Inhalt)".to_string()
    } else {
        dokument.text.clone()
    };
    let (rumpf, hinweis) = if max_zeichen == 0 {
        (rumpf, String::new())
    } else {
        let (gekuerzt, weggelassen) = kuerzen(&rumpf, max_zeichen);
        let hinweis = weggelassen
            .map(|weg| {
                format!(
                    "\n\n[… {weg} Zeichen gekürzt – outline_read mit größerem max_zeichen lesen]"
                )
            })
            .unwrap_or_default();
        (gekuerzt, hinweis)
    };

    format!("{}\n\n---\n\n{rumpf}{hinweis}", kopf.join("\n"))
}

/// Die Antwort nach dem Anlegen oder Ändern einer Seite.
pub fn gespeichert(dokument: &Dokument, aktion: &str) -> String {
    let zustand = if dokument.veroeffentlicht_am.is_some() {
        "veröffentlicht"
    } else {
        "Entwurf"
    };
    let mut zeilen = vec![format!(
        "Dokument {aktion} ({zustand}): {}",
        dokument.titel()
    )];
    if !dokument.kennung().is_empty() {
        zeilen.push(format!("Id: {}", dokument.kennung()));
    }
    if let Some(url) = dokument.url.as_deref() {
        zeilen.push(format!("URL: {url}"));
    }
    zeilen.join("\n")
}

/// Die Kommentare einer Seite, Antworten unter ihrem Elternkommentar.
pub fn kommentare(kommentare: &[Kommentar], dokument_id: &str) -> String {
    if kommentare.is_empty() {
        return format!("Keine Kommentare an Dokument {dokument_id}.");
    }
    let mut zeilen = vec![format!(
        "Kommentare an Dokument {dokument_id} ({}):",
        kommentare.len()
    )];
    fn schreiben(
        kommentar: &Kommentar,
        alle: &[Kommentar],
        tiefe: usize,
        zeilen: &mut Vec<String>,
    ) {
        let einzug = "  ".repeat(tiefe);
        let autor = kommentar
            .erstellt_von
            .as_ref()
            .and_then(|nutzer| nutzer.name.as_deref())
            .unwrap_or("unbekannt");
        let erledigt = if kommentar.erledigt_am.is_some() {
            " [erledigt]"
        } else {
            ""
        };
        zeilen.push(format!(
            "{einzug}- {autor} am {}{erledigt} (id {})",
            kurzdatum(kommentar.erstellt_am.as_deref()),
            kommentar.id
        ));
        let inhalt = kommentar.inhalt();
        for zeile in if inhalt.is_empty() {
            vec!["(leer)"]
        } else {
            inhalt.lines().collect()
        } {
            zeilen.push(format!("{einzug}  {zeile}"));
        }
        for antwort in alle
            .iter()
            .filter(|antwort| antwort.eltern_id.as_deref() == Some(kommentar.id.as_str()))
        {
            schreiben(antwort, alle, tiefe + 1, zeilen);
        }
    }
    for wurzel in kommentare.iter().filter(|kommentar| {
        kommentar.eltern_id.is_none()
            || !kommentare
                .iter()
                .any(|kandidat| Some(kandidat.id.as_str()) == kommentar.eltern_id.as_deref())
    }) {
        schreiben(wurzel, kommentare, 0, &mut zeilen);
    }
    zeilen.join("\n")
}

/// Der Hinweis, wie es weitergeht, wenn eine Seite voll war.
pub fn weitere(seiten: Option<&Seiten>, anzahl: usize) -> String {
    let Some(seiten) = seiten else {
        return String::new();
    };
    let offset = seiten.offset.unwrap_or(0);
    let limit = seiten.limit.unwrap_or(anzahl);
    if anzahl < limit {
        return String::new();
    }
    format!(
        "\n\nWeitere Treffer möglich: denselben Aufruf mit offset {} wiederholen.",
        offset + limit
    )
}

fn kuerzen_roh(text: &str, max: usize) -> String {
    let mut raus: String = text.chars().take(max).collect();
    if text.chars().count() > max {
        raus.push('…');
    }
    raus
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format as f;
    use serde_json::json;

    fn dokument() -> Dokument {
        serde_json::from_value(json!({
            "id": "d1",
            "title": "Betriebshandbuch",
            "collectionId": "c1",
            "parentDocumentId": "d0",
            "url": "https://wiki.example.test/doc/d1",
            "updatedAt": "2026-09-02T10:11:12.000Z",
            "publishedAt": "2026-09-01T08:00:00.000Z",
            "updatedBy": { "name": "Ada" },
            "text": "Zeile eins\nZeile zwei mit mehr Text"
        }))
        .unwrap()
    }

    #[test]
    fn datum_wird_kurz_und_lesbar() {
        assert_eq!(
            kurzdatum(Some("2026-09-02T10:11:12.000Z")),
            "2026-09-02 10:11"
        );
        assert_eq!(kurzdatum(Some("2026-09-02")), "2026-09-02");
        assert_eq!(kurzdatum(None), "-");
        assert_eq!(kurzdatum(Some("  ")), "-");
    }

    #[test]
    fn zustandszeichen_nennt_zustand_und_vorlage() {
        assert_eq!(zustandszeichen(&dokument()), "");
        let mut entwurf = dokument();
        entwurf.veroeffentlicht_am = None;
        assert_eq!(zustandszeichen(&entwurf), " [Entwurf]");
        entwurf.template = true;
        assert_eq!(zustandszeichen(&entwurf), " [Entwurf,Vorlage]");
    }

    #[test]
    fn info_ist_ein_satz_ohne_geheimnis() {
        let daten: Info = serde_json::from_value(json!({
            "user": { "name": "Ada", "email": "ada@example.test" },
            "team": { "name": "Firma", "url": "https://firma.getoutline.com" }
        }))
        .unwrap();
        assert_eq!(f::info(&daten), "Angemeldet als Ada in Firma");
    }

    #[test]
    fn sammlungen_mit_und_ohne_beschreibung() {
        let sammlungen: Vec<Sammlung> = serde_json::from_value(json!([
            { "id": "c1", "name": "Technik", "description": "Alles  Technische" },
            { "id": "c2", "name": "Ohne" }
        ]))
        .unwrap();
        let text = f::sammlungen(&sammlungen);
        assert!(text.contains("Sammlungen (2):"), "{text}");
        assert!(
            text.contains("- Technik (id c1) – Alles Technische"),
            "{text}"
        );
        assert!(text.contains("- Ohne (id c2)"), "{text}");

        assert!(f::sammlungen(&[]).contains("Keine Sammlung"));
    }

    #[test]
    fn struktur_zeigt_eltern_und_kinder() {
        let baum: Vec<Knoten> = serde_json::from_value(json!([
            { "id": "a", "title": "A", "children": [{ "id": "a1", "title": "A1", "children": [] }] }
        ]))
        .unwrap();
        let text = f::struktur(&baum, Some("Technik"));
        assert_eq!(
            text,
            "Dokumentbaum von „Technik“:\n- A (id a)\n  - A1 (id a1)"
        );
        assert!(f::struktur(&[], Some("Leer")).contains("(keine Dokumente)"));
    }

    #[test]
    fn suche_zeigt_id_und_kontext() {
        let treffer: Vec<Treffer> = serde_json::from_value(json!([
            {
                "ranking": 0.9,
                "context": "… hier   steht  der\nTreffer …",
                "document": { "id": "d1", "title": "Handbuch", "updatedAt": "2026-09-02T10:11:12.000Z" }
            }
        ]))
        .unwrap();
        let text = f::suche(&treffer, "handbuch", None);
        assert!(text.contains("Suche „handbuch“ – 1 Treffer"), "{text}");
        assert!(text.contains("1. Handbuch"), "{text}");
        assert!(text.contains("id d1 | geändert 2026-09-02 10:11"), "{text}");
        assert!(text.contains("… hier steht der Treffer …"), "{text}");
        assert!(f::suche(&[], "nichts", None).contains("Kein Dokument"));
    }

    #[test]
    fn liste_nennt_autor_und_kinder() {
        let mut kind = dokument();
        kind.id = "d2".to_string();
        kind.title = "Kind".to_string();
        let text = f::liste(&[dokument(), kind], "Zuletzt geändert", None);
        assert!(text.contains("Zuletzt geändert (2):"), "{text}");
        assert!(
            text.contains("- Betriebshandbuch\n  id d1 | geändert 2026-09-02 10:11 von Ada"),
            "{text}"
        );
        assert!(f::liste(&[], "Entwürfe", None).contains("nichts gefunden"));
    }

    #[test]
    fn dokument_hat_kopf_und_rumpf() {
        let text = f::dokument(&dokument(), 0);
        assert!(
            text.starts_with("Titel: Betriebshandbuch\nId: d1"),
            "{text}"
        );
        assert!(text.contains("Sammlung: c1"));
        assert!(text.contains("Elterndokument: d0"));
        assert!(text.contains("Geändert: 2026-09-02 10:11 von Ada"));
        assert!(text.contains("---\n\nZeile eins"), "{text}");
        assert!(!text.contains("gekürzt"));
    }

    #[test]
    fn dokument_kuerzt_mit_hinweis() {
        let text = f::dokument(&dokument(), 10);
        assert!(text.contains("---\n\nZeile\n"), "{text}");
        assert!(text.contains("Zeichen gekürzt"), "{text}");
        assert!(!text.contains("Zeile zwei"), "{text}");
    }

    #[test]
    fn dokument_ohne_inhalt_sagt_es() {
        let mut leer = dokument();
        leer.text = "   ".to_string();
        assert!(f::dokument(&leer, 0).contains("noch keinen Inhalt"));
    }

    #[test]
    fn gespeichert_nennt_zustand_und_id() {
        let text = f::gespeichert(&dokument(), "erstellt");
        assert!(text.starts_with("Dokument erstellt (veröffentlicht): Betriebshandbuch"));
        assert!(text.contains("Id: d1"));
        assert!(text.contains("URL: https://wiki.example.test/doc/d1"));

        let mut entwurf = dokument();
        entwurf.veroeffentlicht_am = None;
        entwurf.url = None;
        assert!(f::gespeichert(&entwurf, "geändert").contains("(Entwurf)"));
    }

    #[test]
    fn kommentare_verschachteln_antworten() {
        let kommentare: Vec<Kommentar> = serde_json::from_value(json!([
            {
                "id": "k1",
                "text": "Frage?",
                "createdAt": "2026-09-02T10:11:12.000Z",
                "createdBy": { "name": "Ada" }
            },
            {
                "id": "k2",
                "parentCommentId": "k1",
                "data": {
                    "type": "doc",
                    "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "Antwort" }] }]
                },
                "createdBy": { "name": "Bob" }
            }
        ]))
        .unwrap();
        let text = f::kommentare(&kommentare, "d1");
        assert!(text.contains("Kommentare an Dokument d1 (2):"), "{text}");
        assert!(
            text.contains("- Ada am 2026-09-02 10:11 (id k1)\n  Frage?"),
            "{text}"
        );
        assert!(text.contains("  - Bob am - (id k2)\n    Antwort"), "{text}");
        assert!(f::kommentare(&[], "d1").contains("Keine Kommentare"));
    }

    #[test]
    fn weitere_erscheint_nur_bei_voller_seite() {
        let seiten = Seiten {
            offset: Some(0),
            limit: Some(2),
            total: Some(9),
        };
        assert!(f::weitere(Some(&seiten), 2).contains("offset 2"));
        assert_eq!(f::weitere(Some(&seiten), 1), "");
        assert_eq!(f::weitere(None, 9), "");
    }
}
