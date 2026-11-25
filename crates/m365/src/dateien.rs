// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Dateien, Ordner, Bibliotheken und SharePoint-Listen für die Antworten
//! (docs/aufgaben/06-graph-erweiterung.md).
//!
//! Reine Funktionen ohne Netz: Was ist eine Textdatei, wie heißt ein Eintrag in
//! der Antwort, wie viel Text kommt höchstens zurück. Damit ist alles prüfbar,
//! ohne einen Tenant zu brauchen.

use m365_graph::{Laufwerk, LaufwerkEintrag, Liste, Listeneintrag, Website};
use serde_json::{json, Value};

use crate::teams::text_kuerzen;
use crate::zeit::Zeitzone;

/// Standard-Länge eines Dateiinhalts in der Antwort.
pub const STANDARD_MAX_ZEICHEN: usize = 20_000;
/// Mehr Text gibt `datei_lesen` nie heraus.
pub const HOECHSTENS_MAX_ZEICHEN: usize = 50_000;
/// Größer wird ein Text-Upload nie: darüber verlangt Graph eine Upload-Session.
pub const HOECHSTENS_UPLOAD: usize = 4 * 1024 * 1024;
/// So viele Listeneinträge holt ein Tool höchstens.
pub const HOECHSTENS_EINTRAEGE: u32 = 100;

/// Endungen, deren Inhalt reiner Text ist – die liest `datei_lesen` direkt.
const TEXT_ENDUNGEN: &[&str] = &[
    "txt", "text", "md", "markdown", "csv", "tsv", "json", "xml", "yaml", "yml", "ini", "cfg",
    "conf", "log", "sql", "html", "htm", "css", "js", "mjs", "ts", "tsx", "jsx", "rs", "py", "sh",
    "ps1", "bat", "toml", "tex", "r", "java", "c", "h", "cpp", "go", "rb", "php",
];

/// Endungen, die Graph nach PDF wandeln kann (`?format=pdf`).
const OFFICE_ENDUNGEN: &[&str] = &[
    "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp", "rtf",
];

/// `max_zeichen` begrenzen: Standard 20 000, immer zwischen 1 und 50 000.
pub fn max_zeichen_begrenzen(max_zeichen: Option<u32>) -> usize {
    max_zeichen
        .unwrap_or(STANDARD_MAX_ZEICHEN as u32)
        .clamp(1, HOECHSTENS_MAX_ZEICHEN as u32) as usize
}

/// Die Endung klein, ohne Punkt (`Bericht.PDF` → `pdf`).
pub fn endung(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, endung)| endung.trim().to_lowercase())
        .unwrap_or_default()
}

/// Ist der Inhalt dieser Datei reiner Text?
pub fn ist_textdatei(name: &str) -> bool {
    TEXT_ENDUNGEN.contains(&endung(name).as_str())
}

/// Ist das eine Office-Datei, die Graph als PDF ausgeben kann?
pub fn ist_office(name: &str) -> bool {
    OFFICE_ENDUNGEN.contains(&endung(name).as_str())
}

/// Ist das eine PDF-Datei? (Graph liefert sie nur als Binärinhalt.)
pub fn ist_pdf(name: &str) -> bool {
    endung(name) == "pdf"
}

/// Der Inhaltstyp für einen Text-Upload.
pub fn mime_typ(name: &str) -> &'static str {
    match endung(name).as_str() {
        "md" | "markdown" => "text/markdown; charset=utf-8",
        "csv" => "text/csv; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "html" | "htm" => "text/html; charset=utf-8",
        "xml" => "application/xml; charset=utf-8",
        "yaml" | "yml" => "application/yaml; charset=utf-8",
        _ => "text/plain; charset=utf-8",
    }
}

/// Was `datei_lesen` mit diesem Namen tun soll.
pub fn leseart(name: &str) -> Leseart {
    if ist_textdatei(name) {
        Leseart::Text
    } else if ist_office(name) {
        Leseart::Office
    } else if ist_pdf(name) {
        Leseart::Pdf
    } else {
        Leseart::Binaer
    }
}

/// Wie eine Datei gelesen wird – die Antwort folgt daraus, nicht aus Wünschen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leseart {
    /// Text direkt lesen und kürzen.
    Text,
    /// Office-Datei: Graph kann sie als PDF ausgeben, nicht als Text.
    Office,
    /// PDF: nur Metadaten.
    Pdf,
    /// Alles andere: nur Metadaten.
    Binaer,
}

/// Ein Laufwerkseintrag als knappes JSON – Ordner tragen `art: "ordner"`.
pub fn eintrag_json(eintrag: &LaufwerkEintrag, zone: &Zeitzone) -> Value {
    let mut wert = json!({
        "id": eintrag.id,
        "name": eintrag.name,
        "art": if eintrag.ist_ordner() { "ordner" } else { "datei" },
        "groesse": eintrag.size,
        "pfad": eintrag
            .parent_reference
            .as_ref()
            .and_then(|eltern| eltern.path.clone()),
        "link": eintrag.web_url,
        "geaendert": eintrag
            .last_modified_date_time
            .as_deref()
            .and_then(|zeit| zone.zeitpunkt(zeit, None)),
    });
    if eintrag.ist_ordner() {
        wert["unterordner"] = json!(eintrag
            .folder
            .as_ref()
            .and_then(|ordner| ordner.child_count));
    }
    if let Some(typ) = eintrag
        .file
        .as_ref()
        .and_then(|datei| datei.mime_type.clone())
    {
        wert["typ"] = json!(typ);
    }
    wert
}

/// Die Bibliothek als knappes JSON.
pub fn laufwerk_json(laufwerk: &Laufwerk) -> Value {
    json!({
        "id": laufwerk.id,
        "name": laufwerk.name,
        "art": laufwerk.drive_type,
        "link": laufwerk.web_url,
    })
}

/// Die Site als knappes JSON.
pub fn site_json(site: &Website) -> Value {
    json!({
        "id": site.id,
        "name": site.display_name.as_ref().or(site.name.as_ref()),
        "beschreibung": site.description,
        "link": site.web_url,
    })
}

/// Die Liste als knappes JSON.
pub fn liste_json(liste: &Liste) -> Value {
    json!({
        "id": liste.id,
        "name": liste.display_name.as_ref().or(liste.name.as_ref()),
        "beschreibung": liste.description,
        "vorlage": liste.list.as_ref().and_then(|art| art.template.clone()),
        "link": liste.web_url,
    })
}

/// Ein Listeneintrag als knappes JSON, die Spalten roh unter `felder`.
pub fn listeneintrag_json(eintrag: &Listeneintrag, zone: &Zeitzone) -> Value {
    json!({
        "id": eintrag.id,
        "geaendert": eintrag
            .last_modified_date_time
            .as_deref()
            .and_then(|zeit| zone.zeitpunkt(zeit, None)),
        "link": eintrag.web_url,
        "felder": eintrag.fields.clone().unwrap_or(Value::Null),
    })
}

/// Der Textinhalt für die Antwort: auf `max` Zeichen gekürzt, mit Hinweis.
///
/// Zweiter und dritter Wert: `gekuerzt` (nur wenn wirklich gekürzt wurde) und
/// die Länge des vollen Textes.
pub fn inhalt_kuerzen(inhalt: &str, max: usize) -> (String, Option<bool>, Option<usize>) {
    match text_kuerzen(inhalt, max) {
        (text, None) => (text, None, None),
        (text, Some(gesamt)) => (text, Some(true), Some(gesamt)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use m365_graph::{DateiArt, Ordner};

    #[test]
    fn text_und_office_werden_an_der_endung_erkannt() {
        assert!(ist_textdatei("notiz.md"));
        assert!(ist_textdatei("Daten.CSV"));
        assert!(ist_textdatei("config.json"));
        assert!(!ist_textdatei("bericht.docx"));
        assert!(!ist_textdatei("bild.png"));
        assert!(!ist_textdatei("ohne-endung"));

        assert!(ist_office("Bericht.DOCX"));
        assert!(ist_office("Tabelle.xlsx"));
        assert!(ist_pdf("anleitung.pdf"));
        assert!(!ist_pdf("anleitung.docx"));

        assert_eq!(endung("Bericht.PDF"), "pdf");
        assert_eq!(endung("ohne-endung"), "");
    }

    #[test]
    fn leseart_folgt_aus_dem_namen() {
        assert_eq!(leseart("a.md"), Leseart::Text);
        assert_eq!(leseart("a.docx"), Leseart::Office);
        assert_eq!(leseart("a.pdf"), Leseart::Pdf);
        assert_eq!(leseart("a.png"), Leseart::Binaer);
    }

    #[test]
    fn max_zeichen_hat_einen_standard_und_grenzen() {
        assert_eq!(max_zeichen_begrenzen(None), 20_000);
        assert_eq!(max_zeichen_begrenzen(Some(0)), 1);
        assert_eq!(max_zeichen_begrenzen(Some(123)), 123);
        assert_eq!(max_zeichen_begrenzen(Some(999_999)), 50_000);
    }

    #[test]
    fn inhalt_wird_an_der_wortgrenze_gekuerzt() {
        let (text, gekuerzt, gesamt) = inhalt_kuerzen("eins zwei drei vier", 16);
        assert_eq!(text, "eins zwei drei");
        assert_eq!(gekuerzt, Some(true));
        assert_eq!(gesamt, Some(19));

        let (text, gekuerzt, gesamt) = inhalt_kuerzen("kurz", 16);
        assert_eq!(text, "kurz");
        assert_eq!(gekuerzt, None);
        assert_eq!(gesamt, None);
    }

    #[test]
    fn eintrag_json_unterscheidet_ordner_und_datei() {
        let zone = Zeitzone::neu("Europe/Berlin");
        let ordner = LaufwerkEintrag {
            id: "o1".to_string(),
            name: Some("Projekte".to_string()),
            folder: Some(Ordner {
                child_count: Some(3),
            }),
            parent_reference: Some(m365_graph::Eltern {
                path: Some("/drive/root:".to_string()),
            }),
            ..LaufwerkEintrag::default()
        };
        let wert = eintrag_json(&ordner, &zone);
        assert_eq!(wert["art"], "ordner");
        assert_eq!(wert["unterordner"], 3);
        assert!(wert.get("typ").is_none());

        let datei = LaufwerkEintrag {
            id: "d1".to_string(),
            name: Some("notiz.md".to_string()),
            size: Some(12),
            file: Some(DateiArt {
                mime_type: Some("text/markdown".to_string()),
            }),
            last_modified_date_time: Some("2026-11-20T08:00:00Z".to_string()),
            ..LaufwerkEintrag::default()
        };
        let wert = eintrag_json(&datei, &zone);
        assert_eq!(wert["art"], "datei");
        assert_eq!(wert["typ"], "text/markdown");
        assert_eq!(wert["groesse"], 12);
        assert_eq!(wert["geaendert"], "2026-11-20T09:00:00+01:00");
        assert!(wert.get("unterordner").is_none());
    }

    #[test]
    fn mime_typ_passt_zur_endung() {
        assert_eq!(mime_typ("a.md"), "text/markdown; charset=utf-8");
        assert_eq!(mime_typ("a.csv"), "text/csv; charset=utf-8");
        assert_eq!(mime_typ("a.json"), "application/json; charset=utf-8");
        assert_eq!(mime_typ("a.xyz"), "text/plain; charset=utf-8");
    }
}
