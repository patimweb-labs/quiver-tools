// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Daten → Text (Aufgabe 14).
//!
//! Reine Funktionen: sie bekommen eine Antwort von Confluence (JSON) und geben
//! knappen Text zurück. Kein Netz, kein Zustand – darum gut zu prüfen und der
//! einzige Ort, an dem Formulierungen stehen.
//!
//! Drei Regeln: Antworten sind **knapp** (ein Agent zahlt Tokens), lange Texte
//! werden über `max_zeichen` gekürzt – mit Hinweis, wie viel fehlt –, und jede
//! Antwort nennt die Ids und Schlüssel, mit denen der Agent weiterarbeiten kann.

use quiver_tools_kern::kuerzen;
use serde_json::Value;

use crate::client::Auszug;
use crate::daten;
use crate::html;
use crate::sites::Site;

/// So viele Zeichen kommen höchstens aus einem Seitentext.
pub const MAX_ZEICHEN: usize = 4000;

/// So viele Einträge zeigt eine Liste ohne `anzahl`.
pub const ANZAHL: usize = 25;

/// So viele Einträge zeigt eine Liste höchstens.
pub const MAX_ANZAHL: usize = 100;

/// So viele Zeichen zeigt ein Suchtreffer als Auszug.
const MAX_AUSZUG: usize = 200;

// ------------------------------------------------------------- Hinweise

/// Token oder E-Mail gelten nicht.
pub fn token_ungueltig(verbindung: &str) -> String {
    format!(
        "Token für {verbindung} ungültig oder abgelaufen – im quiver-Fenster unter Tools → \
         Confluence erneuern."
    )
}

/// Das Konto darf die Aktion nicht (403).
pub fn keine_rechte(verbindung: &str) -> String {
    format!(
        "Fehlende Rechte für {verbindung} (403) – das Konto darf diese Aktion nicht. Bitte im \
         quiver-Fenster unter Tools → Confluence die Rechte prüfen."
    )
}

/// Der Schalter `schreiben` ist aus.
pub fn schreiben_aus(verbindung: &str) -> String {
    format!("Schreiben ist für {verbindung} aus – im quiver-Fenster einschalten.")
}

/// Das Feld `nur` erlaubt diesen Bereich nicht.
pub fn fremder_bereich(verbindung: &str, bereich: &str) -> String {
    format!(
        "Der Bereich „{bereich}“ ist für {verbindung} nicht freigegeben (Feld „nur“ im \
         quiver-Fenster) – es wurde nichts geändert."
    )
}

/// Die Seite wurde inzwischen geändert; es wurde nichts überschrieben.
pub fn neu_lesen(id: &str) -> String {
    format!(
        "Seite {id} wurde inzwischen geändert – neu lesen (confluence_seite) und mit der \
         aktuellen `version` erneut ändern. Es wurde nichts überschrieben."
    )
}

/// Die Elternseite liegt in einem anderen Bereich; es wurde nichts angelegt.
pub fn eltern_fremd(eltern: &str, bereich: &str, eltern_bereich: &str) -> String {
    format!(
        "Die Elternseite {eltern} liegt in einem anderen Bereich ({eltern_bereich}), die neue \
         Seite gehört nach {bereich}. Eine Seite hängt im selben Bereich wie ihre Elternseite – \
         es wurde nichts angelegt."
    )
}

/// Es gibt keine Site im quiver-Fenster.
pub fn keine_site() -> String {
    "Keine Confluence-Site eingerichtet.".to_string()
}

/// Ein Bereichsschlüssel wird gebraucht, ist aber nirgends angegeben.
pub fn bereich_fehlt() -> String {
    "Bereich fehlt – bitte `bereich` (Bereichsschlüssel) angeben; die Schlüssel zeigt \
     confluence_bereiche."
        .to_string()
}

// ------------------------------------------------------------- Text

/// Ein Text aus dem Storage-Format als Text, gekürzt.
pub fn inhalt(html_text: &str, max_zeichen: usize) -> String {
    let text = html::text_aus_html(html_text);
    let (kurz, weggelassen) = kuerzen(&text, max_zeichen);
    match weggelassen {
        Some(anzahl) => format!("{kurz}\n… ({anzahl} Zeichen weggelassen)"),
        None => kurz,
    }
}

/// Ein Datum mit Zeit, falls vorhanden.
fn datum(baum: &Value, pfad: &str) -> String {
    daten::datum(daten::text(baum, pfad)).unwrap_or_else(|| "-".to_string())
}

/// Eine Größe in Bytes lesbar machen: `4,2 MB`, `812 kB`.
fn groesse(bytes: Option<i64>) -> String {
    let Some(bytes) = bytes else {
        return "-".to_string();
    };
    if bytes <= 0 {
        return "0 B".to_string();
    }
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.0} kB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

/// Der Hinweis, dass der Server mehr hätte.
fn mehr_hinweis(mehr: bool, was: &str) -> String {
    if mehr {
        format!("\n… es gibt weitere {was} – `anzahl` erhöhen.")
    } else {
        String::new()
    }
}

/// Eine Adresse aus `_links.webui` (relativ) und der Site-Adresse.
fn webui(basis: &str, baum: &Value) -> Option<String> {
    let pfad = daten::text(baum, "_links.webui")?;
    if pfad.starts_with("http://") || pfad.starts_with("https://") {
        Some(pfad)
    } else {
        Some(format!("{basis}{pfad}"))
    }
}

// ------------------------------------------------------------- Zugang

/// `confluence_status`: die Sites ohne Token und ohne E-Mail.
pub fn status(sites: &[Site]) -> String {
    if sites.is_empty() {
        return keine_site();
    }
    let zeilen: Vec<String> = sites
        .iter()
        .map(|site| format!("- {}", site.beschreibung()))
        .collect();
    format!(
        "{} Site(s) für Confluence:\n{}",
        sites.len(),
        zeilen.join("\n")
    )
}

/// `confluence_testen`: wer angemeldet ist und zu welcher Site.
pub fn testen(site: &Site, baum: &Value) -> String {
    let anzeige = daten::text(baum, "displayName")
        .or_else(|| daten::text(baum, "publicName"))
        .unwrap_or_else(|| "unbekannt".to_string());
    let konto = daten::text(baum, "accountId");
    let mut text = format!("Anmeldung an {} erfolgreich: {}", site.name, anzeige);
    if let Some(konto) = konto {
        text.push_str(&format!(" (Konto {konto})"));
    }
    text.push_str(&format!("\nAdresse: {}", site.url));
    text.push_str(&format!(
        " · Bereiche: {} · Schreiben: {}",
        if site.nur.is_empty() {
            "alle".to_string()
        } else {
            site.nur.join(", ")
        },
        if site.schreiben { "an" } else { "aus" }
    ));
    text
}

// ------------------------------------------------------------- Bereiche

/// `confluence_bereiche`.
pub fn bereiche(auszug: &Auszug) -> String {
    if auszug.eintraege.is_empty() {
        return "Keine Bereiche sichtbar.".to_string();
    }
    let zeilen: Vec<String> = auszug
        .eintraege
        .iter()
        .map(|bereich| {
            let mut zeile = format!(
                "- {} · {}",
                daten::text_oder(bereich, "key", "(ohne Schlüssel)"),
                daten::text_oder(bereich, "name", "(ohne Namen)")
            );
            if let Some(id) = daten::text(bereich, "id") {
                zeile.push_str(&format!(" · id {id}"));
            }
            if let Some(art) = daten::text(bereich, "type") {
                zeile.push_str(&format!(" · {art}"));
            }
            if let Some(status) = daten::text(bereich, "status") {
                zeile.push_str(&format!(" · {status}"));
            }
            zeile
        })
        .collect();
    format!(
        "{} Bereich(e):\n{}{}",
        auszug.eintraege.len(),
        zeilen.join("\n"),
        mehr_hinweis(auszug.mehr, "Bereiche")
    )
}

// ------------------------------------------------------------- Suchen

/// `confluence_suchen`: CQL-Treffer als Liste.
pub fn suche(auszug: &Auszug) -> String {
    if auszug.eintraege.is_empty() {
        return "Keine Treffer.".to_string();
    }
    let zeilen: Vec<String> = auszug
        .eintraege
        .iter()
        .map(|treffer| {
            // v1-Suche: `content` trägt Id, Typ, Titel und Bereich.
            let id = daten::text(treffer, "content.id")
                .or_else(|| daten::text(treffer, "id"))
                .unwrap_or_else(|| "?".to_string());
            let typ = daten::text(treffer, "content.type")
                .or_else(|| daten::text(treffer, "entityType"))
                .unwrap_or_else(|| "?".to_string());
            let titel = daten::text(treffer, "content.title")
                .or_else(|| daten::text(treffer, "title"))
                .unwrap_or_else(|| "(ohne Titel)".to_string());
            let mut zeile = format!("- {id} [{typ}] {titel}");
            if let Some(bereich) = daten::text(treffer, "content.space.key")
                .or_else(|| daten::text(treffer, "resultGlobalContainer.title"))
            {
                zeile.push_str(&format!(" · Bereich {bereich}"));
            }
            if let Some(auszug_text) = daten::text(treffer, "excerpt") {
                let (kurz, _) = kuerzen(&html::text_aus_html(&auszug_text), MAX_AUSZUG);
                if !kurz.is_empty() {
                    zeile.push_str(&format!("\n  {kurz}"));
                }
            }
            zeile
        })
        .collect();
    format!(
        "{} Treffer:\n{}{}",
        auszug.eintraege.len(),
        zeilen.join("\n"),
        mehr_hinweis(auszug.mehr, "Treffer")
    )
}

// ------------------------------------------------------------- Seite

/// `confluence_seite`: Kopf, Bereich, Version und der Inhalt als Text – oder
/// unverändert als Storage-Format, wenn `als_storage` gesetzt ist.
pub fn seite(
    baum: &Value,
    basis: &str,
    bereich: Option<&str>,
    als_storage: bool,
    max_zeichen: usize,
) -> String {
    let mut text = format!(
        "{} {} [{}]",
        daten::text_oder(baum, "id", "?"),
        daten::text_oder(baum, "title", "(ohne Titel)"),
        daten::text_oder(baum, "status", "?")
    );
    let mut kopf = Vec::new();
    if let Some(schluessel) = bereich {
        let id = daten::text(baum, "spaceId").unwrap_or_else(|| "?".to_string());
        kopf.push(format!("- Bereich: {schluessel} ({id})"));
    } else if let Some(id) = daten::text(baum, "spaceId") {
        kopf.push(format!("- Bereich: {id}"));
    }
    if let Some(version) = daten::zahl(baum, "version.number") {
        kopf.push(format!("- Version: {version}"));
    }
    if daten::text(baum, "version.createdAt").is_some() {
        kopf.push(format!("- Geändert: {}", datum(baum, "version.createdAt")));
    }
    if let Some(url) = webui(basis, baum) {
        kopf.push(format!("- URL: {url}"));
    }
    text.push('\n');
    text.push_str(&kopf.join("\n"));

    let roh = daten::text(baum, "body.storage.value").unwrap_or_default();
    if als_storage {
        let (kurz, weggelassen) = kuerzen(&roh, max_zeichen);
        text.push_str("\n\n## Inhalt (Storage-Format)");
        if !kurz.is_empty() {
            text.push('\n');
            text.push_str(&kurz);
        }
        if let Some(anzahl) = weggelassen {
            text.push_str(&format!("\n… ({anzahl} Zeichen weggelassen)"));
        }
    } else {
        let gekuerzt = inhalt(&roh, max_zeichen);
        if !gekuerzt.is_empty() {
            text.push_str("\n\n## Inhalt\n");
            text.push_str(&gekuerzt);
        }
    }
    text
}

/// `confluence_unterseiten`.
pub fn unterseiten(seite: &str, auszug: &Auszug) -> String {
    if auszug.eintraege.is_empty() {
        return format!("Keine Unterseiten unter {seite}.");
    }
    let zeilen: Vec<String> = auszug
        .eintraege
        .iter()
        .map(|kind| {
            format!(
                "- {} {} [{}, {}]",
                daten::text_oder(kind, "id", "?"),
                daten::text_oder(kind, "title", "(ohne Titel)"),
                daten::text_oder(kind, "type", "?"),
                daten::text_oder(kind, "status", "?")
            )
        })
        .collect();
    format!(
        "{} Unterseite(n) unter {seite}:\n{}{}",
        auszug.eintraege.len(),
        zeilen.join("\n"),
        mehr_hinweis(auszug.mehr, "Unterseiten")
    )
}

/// `confluence_kommentare`.
pub fn kommentare(seite: &str, auszug: &Auszug, max_zeichen: usize) -> String {
    if auszug.eintraege.is_empty() {
        return format!("Keine Kommentare an {seite}.");
    }
    let mut text = format!("{} Kommentar(e) an {seite}:", auszug.eintraege.len());
    for kommentar in &auszug.eintraege {
        let autor =
            daten::text(kommentar, "version.authorId").unwrap_or_else(|| "unbekannt".to_string());
        let zeit = daten::text(kommentar, "version.createdAt")
            .or_else(|| daten::text(kommentar, "createdAt"));
        text.push_str(&format!(
            "\n\n**{}** {} · id {}",
            autor,
            daten::datum(zeit).unwrap_or_else(|| "-".to_string()),
            daten::text_oder(kommentar, "id", "?")
        ));
        let roh = daten::text(kommentar, "body.storage.value").unwrap_or_default();
        let gekuerzt = inhalt(&roh, max_zeichen);
        if !gekuerzt.is_empty() {
            text.push('\n');
            text.push_str(&gekuerzt);
        }
    }
    text.push_str(&mehr_hinweis(auszug.mehr, "Kommentare"));
    text
}

/// `confluence_anhaenge`: nur die Liste, kein Herunterladen.
pub fn anhaenge(seite: &str, auszug: &Auszug) -> String {
    if auszug.eintraege.is_empty() {
        return format!("Keine Anhänge an {seite}.");
    }
    let zeilen: Vec<String> = auszug
        .eintraege
        .iter()
        .map(|anhang| {
            let mut zeile = format!(
                "- {} · {} · {}",
                daten::text_oder(anhang, "title", "(ohne Namen)"),
                daten::text_oder(anhang, "mediaType", "?"),
                groesse(daten::zahl(anhang, "fileSize"))
            );
            if let Some(version) = daten::zahl(anhang, "version.number") {
                zeile.push_str(&format!(" · Version {version}"));
            }
            if let Some(id) = daten::text(anhang, "id") {
                zeile.push_str(&format!(" · id {id}"));
            }
            zeile
        })
        .collect();
    format!(
        "{} Anhang/Anhänge an {seite}:\n{}{}",
        auszug.eintraege.len(),
        zeilen.join("\n"),
        mehr_hinweis(auszug.mehr, "Anhänge")
    )
}

/// `confluence_labels`.
pub fn labels(seite: &str, auszug: &Auszug) -> String {
    if auszug.eintraege.is_empty() {
        return format!("Keine Labels an {seite}.");
    }
    let zeilen: Vec<String> = auszug
        .eintraege
        .iter()
        .map(|label| {
            let prefix = daten::text_oder(label, "prefix", "global");
            format!(
                "- {prefix}:{}",
                daten::text_oder(label, "name", "(ohne Namen)")
            )
        })
        .collect();
    format!(
        "{} Label(s) an {seite}:\n{}{}",
        auszug.eintraege.len(),
        zeilen.join("\n"),
        mehr_hinweis(auszug.mehr, "Labels")
    )
}

// ------------------------------------------------------------- Schreiben

/// Der Kopf einer Seite: `123 Titel [current]`.
fn seite_kopf(baum: &Value) -> String {
    format!(
        "{} {} [{}]",
        daten::text_oder(baum, "id", "?"),
        daten::text_oder(baum, "title", "(ohne Titel)"),
        daten::text_oder(baum, "status", "?")
    )
}

/// `confluence_seite_anlegen`.
pub fn seite_angelegt(baum: &Value, basis: &str) -> String {
    let mut text = format!("Seite angelegt: {}", seite_kopf(baum));
    if let Some(version) = daten::zahl(baum, "version.number") {
        text.push_str(&format!(" · Version {version}"));
    }
    if let Some(url) = webui(basis, baum) {
        text.push_str(&format!("\n{url}"));
    }
    text
}

/// `confluence_seite_aendern`.
pub fn seite_geaendert(id: &str, titel: &str, version: i64, anhaengen: bool) -> String {
    format!(
        "Seite {id} geändert: {titel} · Version {version}{}",
        if anhaengen {
            " (angehängt)"
        } else {
            " (Inhalt ersetzt)"
        }
    )
}

/// `confluence_kommentieren`.
pub fn kommentiert(seite: &str, baum: &Value) -> String {
    match daten::text(baum, "id") {
        Some(kommentar) => format!("Kommentar {kommentar} an Seite {seite} angelegt."),
        None => format!("Kommentar an Seite {seite} angelegt."),
    }
}

/// `confluence_label_hinzufuegen`.
pub fn label_angelegt(seite: &str, label: &str) -> String {
    format!("Label {label} an Seite {seite} hinzugefügt.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn site() -> Site {
        Site {
            name: "firma".into(),
            url: "https://firma.atlassian.net".into(),
            email: "alex@example.test".into(),
            token: "token-4711".into(),
            nur: vec!["ENG".into()],
            schreiben: false,
            ca_zertifikat: None,
        }
    }

    fn auszug(eintraege: Vec<Value>, mehr: bool) -> Auszug {
        Auszug { eintraege, mehr }
    }

    #[test]
    fn die_hinweise_nennen_das_quiver_fenster() {
        assert_eq!(
            token_ungueltig("firma"),
            "Token für firma ungültig oder abgelaufen – im quiver-Fenster unter Tools → \
             Confluence erneuern."
        );
        assert_eq!(
            schreiben_aus("firma"),
            "Schreiben ist für firma aus – im quiver-Fenster einschalten."
        );
        assert!(keine_rechte("firma").contains("Tools → Confluence"));
        assert!(fremder_bereich("firma", "DOC").contains("nicht freigegeben"));
        assert!(neu_lesen("123").contains("nichts überschrieben"));
        assert!(eltern_fremd("300", "ENG", "OPS").contains("nichts angelegt"));
        assert!(keine_site().contains("Keine"));
        assert!(bereich_fehlt().contains("confluence_bereiche"));
    }

    #[test]
    fn der_status_nennt_die_site_ohne_token_und_ohne_email() {
        let text = status(&[site()]);
        assert!(text.contains("1 Site(s)"), "{text}");
        assert!(text.contains("firma"), "{text}");
        assert!(text.contains("Bereiche ENG"), "{text}");
        assert!(text.contains("Schreiben aus"), "{text}");
        assert!(!text.contains("token-4711"), "{text}");
        assert!(!text.contains("alex@example.test"), "{text}");
        assert_eq!(status(&[]), "Keine Confluence-Site eingerichtet.");
    }

    #[test]
    fn der_test_nennt_benutzer_und_konto_aber_keine_email() {
        let text = testen(
            &site(),
            &json!({ "displayName": "Alex Beispiel", "accountId": "5f0c1e2a",
                     "email": "alex@example.test" }),
        );
        assert!(
            text.contains("Anmeldung an firma erfolgreich: Alex Beispiel (Konto 5f0c1e2a)"),
            "{text}"
        );
        assert!(text.contains("Schreiben: aus"), "{text}");
        assert!(!text.contains("alex@example.test"), "{text}");
        assert!(!text.contains("token-4711"), "{text}");

        // Ohne `displayName` zählt `publicName`.
        let text = testen(&site(), &json!({ "publicName": "pweppelmann" }));
        assert!(text.contains("pweppelmann"), "{text}");
    }

    #[test]
    fn bereiche_zaehlen_und_nennen_schluessel_und_id() {
        let liste = auszug(
            vec![json!({ "id": "10", "key": "ENG", "name": "Engineering",
                         "type": "global", "status": "current" })],
            false,
        );
        assert_eq!(
            bereiche(&liste),
            "1 Bereich(e):\n- ENG · Engineering · id 10 · global · current"
        );
        assert!(bereiche(&auszug(vec![], false)).contains("Keine Bereiche"));
        assert!(bereiche(&auszug(vec![json!({"key":"A","name":"A"})], true))
            .contains("es gibt weitere Bereiche"));
    }

    #[test]
    fn treffer_nennen_id_typ_bereich_und_auszug() {
        let liste = auszug(
            vec![json!({
                "content": { "id": "123", "type": "page", "title": "Anmeldung",
                             "space": { "key": "ENG" } },
                "excerpt": "<b>Der</b> Anmelden-Knopf tut nichts."
            })],
            true,
        );
        let text = suche(&liste);
        assert!(text.contains("1 Treffer:"), "{text}");
        assert!(
            text.contains("- 123 [page] Anmeldung · Bereich ENG"),
            "{text}"
        );
        assert!(text.contains("Der Anmelden-Knopf tut nichts."), "{text}");
        assert!(!text.contains("<b>"), "{text}");
        assert!(text.contains("es gibt weitere Treffer"), "{text}");
        assert!(suche(&auszug(vec![], false)).contains("Keine Treffer"));
    }

    #[test]
    fn eine_seite_zeigt_version_bereich_und_inhalt_als_text() {
        let seite_baum = json!({
            "id": "123", "title": "Anmeldung", "status": "current", "spaceId": "10",
            "version": { "number": 3, "createdAt": "2026-11-20T10:15:00Z" },
            "body": { "storage": { "value": "<p>Der Anmelden-Knopf tut nichts.</p>\
                                    <ul><li>Login klappt</li></ul>" } },
            "_links": { "webui": "/spaces/ENG/pages/123/Anmeldung" }
        });
        let text = seite(
            &seite_baum,
            "https://firma.atlassian.net",
            Some("ENG"),
            false,
            4000,
        );
        assert!(text.starts_with("123 Anmeldung [current]"), "{text}");
        assert!(text.contains("- Bereich: ENG (10)"), "{text}");
        assert!(text.contains("- Version: 3"), "{text}");
        assert!(text.contains("- Geändert: 2026-11-20 10:15"), "{text}");
        assert!(
            text.contains("- URL: https://firma.atlassian.net/spaces/ENG/pages/123/Anmeldung"),
            "{text}"
        );
        assert!(
            text.contains("## Inhalt\nDer Anmelden-Knopf tut nichts.\n- Login klappt"),
            "{text}"
        );
        assert!(!text.contains("<p>"), "{text}");

        // `als_storage` gibt das Original.
        let text = seite(&seite_baum, "https://x", Some("ENG"), true, 4000);
        assert!(text.contains("## Inhalt (Storage-Format)"), "{text}");
        assert!(text.contains("<p>Der Anmelden-Knopf"), "{text}");

        // Lange Texte werden gekürzt, mit Hinweis.
        let lang = format!("<p>{}</p>", "wort ".repeat(200));
        let seite_lang = json!({ "id": "1", "title": "T", "status": "current",
                                 "body": { "storage": { "value": lang } } });
        let text = seite(&seite_lang, "https://x", None, false, 50);
        assert!(text.contains("Zeichen weggelassen"), "{text}");
    }

    #[test]
    fn listen_zaehlen_und_kuerzen_mit_hinweis() {
        let kinder = auszug(
            vec![
                json!({ "id": "124", "type": "page", "title": "Unterpunkt", "status": "current" }),
            ],
            false,
        );
        assert_eq!(
            unterseiten("123", &kinder),
            "1 Unterseite(n) unter 123:\n- 124 Unterpunkt [page, current]"
        );
        assert!(unterseiten("123", &auszug(vec![], false)).contains("Keine Unterseiten"));

        let kommentare_liste = auszug(
            vec![json!({
                "id": "500",
                "version": { "authorId": "abc", "createdAt": "2026-11-20T11:00:00Z" },
                "body": { "storage": { "value": "<p>Sieht gut aus</p>" } }
            })],
            false,
        );
        let text = kommentare("123", &kommentare_liste, 4000);
        assert!(text.contains("1 Kommentar(e) an 123"), "{text}");
        assert!(text.contains("**abc** 2026-11-20 11:00 · id 500"), "{text}");
        assert!(text.contains("Sieht gut aus"), "{text}");

        let anhang_liste = auszug(
            vec![
                json!({ "id": "900", "title": "bild.png", "mediaType": "image/png",
                         "fileSize": 4096, "version": { "number": 2 } }),
            ],
            true,
        );
        let text = anhaenge("123", &anhang_liste);
        assert!(
            text.contains("- bild.png · image/png · 4 kB · Version 2 · id 900"),
            "{text}"
        );
        assert!(text.contains("es gibt weitere Anhänge"), "{text}");
        assert!(anhaenge("123", &auszug(vec![], false)).contains("Keine Anhänge"));

        let label_liste = auszug(
            vec![json!({ "id": "l1", "name": "anmeldung", "prefix": "global" })],
            false,
        );
        assert_eq!(
            labels("123", &label_liste),
            "1 Label(s) an 123:\n- global:anmeldung"
        );
        assert!(labels("123", &auszug(vec![], false)).contains("Keine Labels"));
    }

    #[test]
    fn die_schreibantworten_sind_knapp() {
        let neu = json!({ "id": "200", "title": "Neu", "status": "current",
                          "version": { "number": 1 },
                          "_links": { "webui": "/spaces/ENG/pages/200/Neu" } });
        let text = seite_angelegt(&neu, "https://firma.atlassian.net");
        assert_eq!(
            text,
            "Seite angelegt: 200 Neu [current] · Version 1\n\
             https://firma.atlassian.net/spaces/ENG/pages/200/Neu"
        );
        assert_eq!(
            seite_geaendert("123", "Anmeldung", 4, false),
            "Seite 123 geändert: Anmeldung · Version 4 (Inhalt ersetzt)"
        );
        assert!(seite_geaendert("123", "A", 4, true).contains("(angehängt)"));
        assert_eq!(
            kommentiert("123", &json!({ "id": "500" })),
            "Kommentar 500 an Seite 123 angelegt."
        );
        assert_eq!(
            kommentiert("123", &json!(null)),
            "Kommentar an Seite 123 angelegt."
        );
        assert_eq!(
            label_angelegt("123", "anmeldung"),
            "Label anmeldung an Seite 123 hinzugefügt."
        );
    }

    #[test]
    fn groessen_werden_lesbar() {
        assert_eq!(groesse(None), "-");
        assert_eq!(groesse(Some(0)), "0 B");
        assert_eq!(groesse(Some(812)), "812 B");
        assert_eq!(groesse(Some(4096)), "4 kB");
        assert_eq!(groesse(Some(4 * 1024 * 1024)), "4.0 MB");
    }
}
