// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Daten → Text (Aufgabe 13).
//!
//! Reine Funktionen: sie bekommen eine Antwort von Jira (JSON) und geben
//! knappen Text zurück. Kein Netz, kein Zustand – darum gut zu prüfen und der
//! einzige Ort, an dem Formulierungen stehen.
//!
//! Drei Regeln: Antworten sind **knapp** (ein Agent zahlt Tokens), Ids und
//! Schlüssel stehen immer mit dabei (der Agent muss weiterarbeiten können), und
//! ein langer Text wird über `max_zeichen` gekürzt – mit Hinweis, wie viel fehlt.

use quiver_tools_kern::kuerzen;
use serde_json::Value;

use crate::adf;
use crate::daten;
use crate::sites::Site;

/// So viele Zeichen kommen höchstens aus einer Beschreibung oder einem Kommentar.
pub const MAX_ZEICHEN: usize = 4000;

/// So viele Einträge zeigt eine Liste standardmäßig.
pub const ANZAHL_STANDARD: usize = 25;

/// So viele Einträge zeigt eine Liste höchstens.
pub const ANZAHL_MAX: usize = 100;

// ------------------------------------------------------------- Hinweise

/// Token oder E-Mail gelten nicht (Aufgabe 12, Regel 7).
pub fn token_ungueltig(verbindung: &str) -> String {
    format!(
        "Token für {verbindung} ungültig oder abgelaufen – im quiver-Fenster erneuern \
         (Tools → Jira)."
    )
}

/// HTTP 403: das Konto darf das nicht.
pub fn keine_rechte(verbindung: &str, text: &str) -> String {
    format!(
        "Keine Rechte für {verbindung}: {text} – bitte die Projekt- und Vorgangsrechte des Kontos \
         prüfen."
    )
}

/// Der Schalter `schreiben` ist aus.
pub fn schreiben_aus(verbindung: &str) -> String {
    format!("Schreiben ist für {verbindung} aus – im quiver-Fenster einschalten.")
}

/// Ein Projekt liegt außerhalb des Feldes `nur`.
pub fn nur_verletzt(verbindung: &str, projekt: &str, erlaubt: &[String]) -> String {
    format!(
        "Projekt „{projekt}“ ist für {verbindung} nicht freigegeben – das Feld „nur“ erlaubt: {}. \
         Es wurde nichts geändert.",
        if erlaubt.is_empty() {
            "nichts".to_string()
        } else {
            erlaubt.join(", ")
        }
    )
}

/// Es gibt keine Site im quiver-Fenster.
pub fn keine_site() -> String {
    "Keine Jira-Site eingerichtet.".to_string()
}

/// `POST search/jql` nimmt laut Spezifikation nur **begrenzte** Abfragen.
pub fn suche_unbegrenzt(verbindung: &str) -> String {
    format!(
        "Jira nimmt für die Suche in {verbindung} nur eine begrenzte JQL an – eine Abfrage ohne \
         Einschränkung (nur `order by`) wird abgelehnt. Es wurde nicht gesucht. Bitte `projekt`, \
         `status`, `text` oder `zugewiesen_an_mich` angeben oder eine eigene `jql` mit mindestens \
         einer Bedingung, z. B. `project = ABC`."
    )
}

// ------------------------------------------------------------- Text

/// Einen langen Text kürzen und den Rest benennen.
pub fn text(text: &str, max_zeichen: usize) -> String {
    let text = text.trim();
    let (kurz, weggelassen) = kuerzen(text, max_zeichen);
    match weggelassen {
        Some(anzahl) => format!("{kurz}\n… ({anzahl} Zeichen weggelassen)"),
        None => kurz,
    }
}

/// Ein ADF-Dokument als Text, gekürzt.
fn adf_text(wert: Option<&Value>, max_zeichen: usize) -> String {
    match wert {
        Some(wert) => text(&adf::adf_zu_text(wert), max_zeichen),
        None => String::new(),
    }
}

/// Ein Datum mit Zeit, falls vorhanden.
fn datum(baum: &Value, pfad: &str) -> String {
    daten::datum(daten::text(baum, pfad)).unwrap_or_else(|| "-".to_string())
}

/// Die Zahl der gezeigten Einträge und – wenn bekannt – die Gesamtzahl.
fn kopf(anzahl: usize, gesamt: Option<i64>, wort: &str) -> String {
    match gesamt {
        Some(gesamt) if gesamt > anzahl as i64 => {
            format!("{anzahl} von {gesamt} {wort}:")
        }
        _ => format!("{anzahl} {wort}:"),
    }
}

/// Der Hinweis, dass es weitere Einträge gibt.
fn weitere(mehr: bool) -> &'static str {
    if mehr {
        "\n… es gibt weitere Einträge."
    } else {
        ""
    }
}

// ------------------------------------------------------------- Zugang

/// `jira_status`: die Sites ohne Token.
pub fn status(sites: &[Site]) -> String {
    if sites.is_empty() {
        return keine_site();
    }
    let zeilen: Vec<String> = sites
        .iter()
        .map(|site| format!("- {}", site.beschreibung()))
        .collect();
    format!("{} Site(s) für Jira:\n{}", sites.len(), zeilen.join("\n"))
}

/// `jira_testen`: wer angemeldet ist und an welcher Site.
pub fn testen(site: &Site, mich: &Value) -> String {
    let anzeige = daten::text_oder(mich, "displayName", "unbekannt");
    let mut text = format!("Anmeldung an {} erfolgreich: {}", site.name, anzeige);
    if let Some(email) = daten::text(mich, "emailAddress") {
        text.push_str(&format!(" <{email}>"));
    }
    if let Some(konto) = daten::text(mich, "accountId") {
        text.push_str(&format!(" (Konto {konto})"));
    }
    text.push_str(&format!(
        "\nAdresse: {} · Schreiben: {}",
        site.url,
        if site.schreiben { "an" } else { "aus" }
    ));
    text
}

// ------------------------------------------------------------- Lesen

/// `jira_projekte`.
pub fn projekte(site: &str, eintraege: &[Value], gesamt: Option<i64>, mehr: bool) -> String {
    if eintraege.is_empty() {
        return format!("Keine Projekte in {site} sichtbar.");
    }
    let zeilen: Vec<String> = eintraege
        .iter()
        .map(|projekt| {
            let mut zeile = format!(
                "- {} — {}",
                daten::text_oder(projekt, "key", "(ohne Schlüssel)"),
                daten::text_oder(projekt, "name", "(ohne Namen)")
            );
            let mut kennzeichen = Vec::new();
            if let Some(typ) = daten::text(projekt, "projectTypeKey") {
                kennzeichen.push(typ);
            }
            if let Some(leiter) = daten::text(projekt, "lead.displayName") {
                kennzeichen.push(format!("Leitung {leiter}"));
            }
            if !kennzeichen.is_empty() {
                zeile.push_str(&format!(" ({})", kennzeichen.join(", ")));
            }
            zeile
        })
        .collect();
    format!(
        "{} in {site}:\n{}{}",
        kopf(eintraege.len(), gesamt, "Projekt(e)").trim_end_matches(':'),
        zeilen.join("\n"),
        weitere(mehr)
    )
}

/// Ein Vorgang in einer Zeile: `ABC-1 [Bug] [Offen] Titel — zugewiesen: X`.
fn vorgang_zeile(vorgang: &Value) -> String {
    let mut zeile = format!(
        "{} [{}] [{}] {}",
        daten::text_oder(vorgang, "key", "?"),
        daten::text_oder(vorgang, "fields.issuetype.name", "?"),
        daten::text_oder(vorgang, "fields.status.name", "?"),
        daten::text_oder(vorgang, "fields.summary", "(ohne Titel)")
    );
    if let Some(zugewiesen) = daten::text(vorgang, "fields.assignee.displayName") {
        zeile.push_str(&format!(" — zugewiesen: {zugewiesen}"));
    }
    let aktualisiert = datum(vorgang, "fields.updated");
    if aktualisiert != "-" {
        zeile.push_str(&format!(" · aktualisiert: {aktualisiert}"));
    }
    zeile
}

/// `jira_suchen`: die Trefferliste und die nächste Seite.
pub fn suchen(eintraege: &[Value], gesamt: Option<i64>, naechste: Option<&str>) -> String {
    if eintraege.is_empty() {
        return "Keine Vorgänge gefunden.".to_string();
    }
    let zeilen: Vec<String> = eintraege
        .iter()
        .map(|vorgang| format!("- {}", vorgang_zeile(vorgang)))
        .collect();
    let mut text = format!(
        "{}\n{}",
        kopf(eintraege.len(), gesamt, "Vorgang/Vorgänge"),
        zeilen.join("\n")
    );
    match naechste.filter(|token| !token.trim().is_empty()) {
        Some(token) => text.push_str(&format!(
            "\n… es gibt weitere Vorgänge. Nächste Seite: `seite: \"{token}\"`."
        )),
        None => text.push_str("\nDas ist die letzte Seite."),
    }
    text
}

/// `jira_vorgang`: Kopf, Beschreibung, Links, Unteraufgaben und Eltern.
pub fn vorgang(site: &Site, key: &str, antwort: &Value, max_zeichen: usize) -> String {
    let key = daten::text_oder(antwort, "key", key);
    let mut text = format!(
        "{} [{}] [{}] {}",
        key,
        daten::text_oder(antwort, "fields.issuetype.name", "?"),
        daten::text_oder(antwort, "fields.status.name", "?"),
        daten::text_oder(antwort, "fields.summary", "(ohne Titel)")
    );

    let mut kopf_zeilen = Vec::new();
    let projekt = daten::text(antwort, "fields.project.key").map(|projekt| {
        match daten::text(antwort, "fields.project.name") {
            Some(name) => format!("{projekt} ({name})"),
            None => projekt,
        }
    });
    if let Some(projekt) = projekt {
        kopf_zeilen.push(format!("- Projekt: {projekt}"));
    }
    for (name, pfad) in [
        ("Zugewiesen", "fields.assignee.displayName"),
        ("Reporter", "fields.reporter.displayName"),
        ("Priorität", "fields.priority.name"),
    ] {
        if let Some(wert) = daten::text(antwort, pfad) {
            kopf_zeilen.push(format!("- {name}: {wert}"));
        }
    }
    let labels = daten::namen(antwort, "fields.labels");
    if !labels.is_empty() {
        kopf_zeilen.push(format!("- Labels: {}", labels.join(", ")));
    }
    let mut zeiten = vec![
        format!("erstellt: {}", datum(antwort, "fields.created")),
        format!("aktualisiert: {}", datum(antwort, "fields.updated")),
    ];
    if let Some(faellig) = daten::text(antwort, "fields.duedate") {
        zeiten.push(format!("fällig: {faellig}"));
    }
    kopf_zeilen.push(format!("- {}", zeiten.join(" · ")));
    kopf_zeilen.push(format!("- URL: {}", site.vorgang_url(&key)));
    text.push('\n');
    text.push_str(&kopf_zeilen.join("\n"));

    let beschreibung = adf_text(daten::wert(antwort, "fields.description"), max_zeichen);
    if !beschreibung.is_empty() {
        text.push_str(&format!("\n\n## Beschreibung\n{beschreibung}"));
    }

    let links = verknuepfungen(antwort);
    if !links.is_empty() {
        text.push_str("\n\n## Verknüpfungen");
        for link in links {
            text.push_str(&format!("\n- {link}"));
        }
    }

    let unteraufgaben = unteraufgaben(antwort);
    if !unteraufgaben.is_empty() {
        text.push_str("\n\n## Unteraufgaben");
        for unteraufgabe in unteraufgaben {
            text.push_str(&format!("\n- {unteraufgabe}"));
        }
    }

    if let Some(eltern) = daten::text(antwort, "fields.parent.key") {
        let mut zeile = format!(
            "\n\n## Eltern\n- {eltern} [{}] {}",
            daten::text_oder(antwort, "fields.parent.fields.status.name", "?"),
            daten::text_oder(antwort, "fields.parent.fields.summary", "(ohne Titel)")
        );
        if zeile.ends_with(' ') {
            zeile = zeile.trim_end().to_string();
        }
        text.push_str(&zeile);
    }

    text
}

/// Die Verknüpfungen eines Vorgangs als `blockiert ABC-2`.
fn verknuepfungen(antwort: &Value) -> Vec<String> {
    let mut ergebnis = Vec::new();
    for link in daten::liste(antwort, "fields.issuelinks") {
        let art = daten::text(link, "type.name").unwrap_or_else(|| "verknüpft".to_string());
        if let Some(ziel) = daten::text(link, "outwardIssue.key") {
            let aus = daten::text(link, "type.outward").unwrap_or(art);
            ergebnis.push(format!("{aus} {ziel}"));
        } else if let Some(ziel) = daten::text(link, "inwardIssue.key") {
            let ein = daten::text(link, "type.inward").unwrap_or(art);
            ergebnis.push(format!("{ein} {ziel}"));
        }
    }
    ergebnis
}

/// Die Unteraufgaben eines Vorgangs als `ABC-4 [Offen] Titel`.
fn unteraufgaben(antwort: &Value) -> Vec<String> {
    daten::liste(antwort, "fields.subtasks")
        .iter()
        .map(|unteraufgabe| {
            format!(
                "{} [{}] {}",
                daten::text_oder(unteraufgabe, "key", "?"),
                daten::text_oder(unteraufgabe, "fields.status.name", "?"),
                daten::text_oder(unteraufgabe, "fields.summary", "(ohne Titel)")
            )
        })
        .collect()
}

/// `jira_kommentare`.
pub fn kommentare(key: &str, antwort: &Value, max_zeichen: usize) -> String {
    let eintraege = daten::liste(antwort, "comments");
    if eintraege.is_empty() {
        return format!("Keine Kommentare zu {key}.");
    }
    let mut text = format!("{} Kommentar(e) zu {key}:", eintraege.len());
    for kommentar in eintraege {
        text.push_str(&format!(
            "\n\n**{}** {}{}",
            daten::text_oder(kommentar, "author.displayName", "unbekannt"),
            datum(kommentar, "created"),
            daten::text(kommentar, "id")
                .map(|id| format!(" · Kommentar {id}"))
                .unwrap_or_default()
        ));
        let inhalt = adf_text(daten::wert(kommentar, "body"), max_zeichen);
        if !inhalt.is_empty() {
            text.push('\n');
            text.push_str(&inhalt);
        }
    }
    text
}

/// `jira_uebergaenge`.
pub fn uebergaenge(key: &str, antwort: &Value) -> String {
    let eintraege = daten::liste(antwort, "transitions");
    if eintraege.is_empty() {
        return format!("Keine Übergänge für {key} sichtbar.");
    }
    let zeilen: Vec<String> = eintraege
        .iter()
        .map(|uebergang| {
            format!(
                "- {} {} → {}",
                daten::text_oder(uebergang, "id", "?"),
                daten::text_oder(uebergang, "name", "(ohne Namen)"),
                daten::text_oder(uebergang, "to.name", "?")
            )
        })
        .collect();
    format!(
        "{} Übergang/Übergänge für {key}:\n{}",
        eintraege.len(),
        zeilen.join("\n")
    )
}

/// `jira_felder_zum_anlegen`: die Vorgangstypen eines Projekts.
pub fn vorgangstypen(projekt: &str, antwort: &Value) -> String {
    // Die Spezifikation (`PageOfCreateMetaIssueTypes`) nennt die Liste `issueTypes`.
    let eintraege = daten::liste(antwort, "issueTypes");
    if eintraege.is_empty() {
        return format!("Keine Vorgangstypen in {projekt} sichtbar.");
    }
    let zeilen: Vec<String> = eintraege
        .iter()
        .map(|typ| {
            let unteraufgabe = if daten::wahr(typ, "subtask") {
                " · Unteraufgabe"
            } else {
                ""
            };
            format!(
                "- {} · Typ-ID {}{unteraufgabe}",
                daten::text_oder(typ, "name", "(ohne Namen)"),
                daten::text_oder(typ, "id", "?")
            )
        })
        .collect();
    format!(
        "{} Vorgangstyp(en) in {projekt}:\n{}\n\nBitte `vorgangstyp` angeben, um die Felder zu \
         sehen.",
        eintraege.len(),
        zeilen.join("\n")
    )
}

/// `jira_felder_zum_anlegen`: die Felder eines Vorgangstyps.
pub fn felder_zum_anlegen(projekt: &str, typ: &str, antwort: &Value) -> String {
    let Some(felder) = daten::wert(antwort, "fields").and_then(Value::as_object) else {
        return format!("Keine Felder für {typ} in {projekt} sichtbar.");
    };
    if felder.is_empty() {
        return format!("Keine Felder für {typ} in {projekt} sichtbar.");
    }
    let pflicht: usize = felder
        .values()
        .filter(|feld| daten::wahr(feld, "required"))
        .count();
    let mut zeilen = Vec::new();
    for (name, feld) in felder {
        let mut zeile = format!(
            "- {name}{}: {}",
            if daten::wahr(feld, "required") {
                " (Pflicht)"
            } else {
                ""
            },
            daten::text_oder(feld, "name", name)
        );
        if let Some(art) = daten::text(feld, "schema.type") {
            zeile.push_str(&format!(" · Typ {art}"));
        }
        let erlaubt: Vec<String> = daten::liste(feld, "allowedValues")
            .iter()
            .filter_map(|wert| daten::text(wert, ""))
            .collect();
        if !erlaubt.is_empty() {
            let zeigen = erlaubt
                .iter()
                .take(20)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            let rest = erlaubt.len().saturating_sub(20);
            zeile.push_str(&format!(
                " · erlaubt: {zeigen}{}",
                if rest > 0 {
                    format!(" … (+{rest})")
                } else {
                    String::new()
                }
            ));
        }
        zeilen.push(zeile);
    }
    format!(
        "{} Feld(er) für {typ} in {projekt} ({pflicht} Pflicht):\n{}",
        felder.len(),
        zeilen.join("\n")
    )
}

/// `jira_filter_liste`.
pub fn filter_liste(antwort: &Value, max_zeichen: usize) -> String {
    let eintraege = daten::liste(antwort, "values");
    if eintraege.is_empty() {
        return "Keine Filter gefunden.".to_string();
    }
    let zeilen: Vec<String> = eintraege
        .iter()
        .map(|filter| {
            format!(
                "- {} {} · {}",
                daten::text_oder(filter, "id", "?"),
                daten::text_oder(filter, "name", "(ohne Namen)"),
                text(&daten::text_oder(filter, "jql", "(ohne JQL)"), max_zeichen)
            )
        })
        .collect();
    format!("{} Filter:\n{}", eintraege.len(), zeilen.join("\n"))
}

/// `jira_filter`.
pub fn filter(antwort: &Value, max_zeichen: usize) -> String {
    let mut text = format!(
        "{} {}",
        daten::text_oder(antwort, "id", "?"),
        daten::text_oder(antwort, "name", "(ohne Namen)")
    );
    if let Some(besitzer) = daten::text(antwort, "owner.displayName") {
        text.push_str(&format!("\nBesitzer: {besitzer}"));
    }
    text.push_str(&format!(
        "\nJQL: {}",
        text_direkt(&daten::text_oder(antwort, "jql", "(ohne JQL)"), max_zeichen)
    ));
    if let Some(beschreibung) = daten::text(antwort, "description") {
        text.push_str(&format!(
            "\nBeschreibung: {}",
            text_direkt(&beschreibung, max_zeichen)
        ));
    }
    text
}

/// Wie [`text`], aber für eine Zeile gedacht (kein Zeilenumbruch-Hinweis).
fn text_direkt(wert: &str, max_zeichen: usize) -> String {
    let (kurz, weggelassen) = kuerzen(wert, max_zeichen);
    match weggelassen {
        Some(anzahl) => format!("{kurz} … ({anzahl} Zeichen weggelassen)"),
        None => kurz,
    }
}

/// `jira_verknuepfungsarten`.
pub fn verknuepfungsarten(antwort: &Value) -> String {
    let eintraege = daten::liste(antwort, "issueLinkTypes");
    if eintraege.is_empty() {
        return "Keine Verknüpfungsarten sichtbar.".to_string();
    }
    let zeilen: Vec<String> = eintraege
        .iter()
        .map(|art| {
            format!(
                "- {} (ID {}): {} / {}",
                daten::text_oder(art, "name", "?"),
                daten::text_oder(art, "id", "?"),
                daten::text_oder(art, "outward", "?"),
                daten::text_oder(art, "inward", "?")
            )
        })
        .collect();
    format!(
        "{} Verknüpfungsart(en):\n{}",
        eintraege.len(),
        zeilen.join("\n")
    )
}

/// `jira_person_suchen`.
pub fn personen(antwort: &Value) -> String {
    let eintraege = daten::liste(antwort, "");
    if eintraege.is_empty() {
        return "Keine Person gefunden – Jira findet nur Konten, die das Projekt sehen dürfen."
            .to_string();
    }
    let zeilen: Vec<String> = eintraege
        .iter()
        .map(|person| {
            let mut zeile = format!(
                "- {}",
                daten::text_oder(person, "displayName", "(ohne Namen)")
            );
            if let Some(email) = daten::text(person, "emailAddress") {
                zeile.push_str(&format!(" <{email}>"));
            }
            zeile.push_str(&format!(
                " · accountId {}",
                daten::text_oder(person, "accountId", "?")
            ));
            if !daten::wahr(person, "active") && daten::wert(person, "active").is_some() {
                zeile.push_str(" · inaktiv");
            }
            zeile
        })
        .collect();
    format!("{} Person(en):\n{}", eintraege.len(), zeilen.join("\n"))
}

// ------------------------------------------------------------- Schreiben

/// `jira_anlegen`.
pub fn angelegt(site: &Site, antwort: &Value) -> String {
    let key = daten::text_oder(antwort, "key", "?");
    format!("Vorgang angelegt: {key}\n{}", site.vorgang_url(&key))
}

/// `jira_aendern`.
pub fn geaendert(key: &str, felder: &[String]) -> String {
    format!("Vorgang {key} geändert:\n- {}", felder.join("\n- "))
}

/// `jira_uebergang`.
///
/// Gemeldet wird, was ausgeführt wurde **und** welcher Status jetzt gilt: der
/// Status wird nach dem Aufruf am Vorgang gelesen, nicht geraten.
pub fn uebergang(key: &str, id: &str, name: &str, neuer_status: Option<&str>) -> String {
    let mut text = format!("Vorgang {key}: Übergang {id} „{name}“ ausgeführt.");
    match neuer_status {
        Some(status) => text.push_str(&format!("\nNeuer Status: {status}")),
        None => text.push_str(
            "\nDen neuen Status konnte ich nicht lesen – bitte mit `jira_vorgang` prüfen.",
        ),
    }
    text
}

/// `jira_kommentieren`.
pub fn kommentiert(key: &str, antwort: &Value) -> String {
    match daten::text(antwort, "id") {
        Some(id) => format!("Kommentar {id} zu {key} angelegt."),
        None => format!("Kommentar zu {key} angelegt."),
    }
}

/// `jira_verknuepfen`: die Verknüpfung, **wie Jira sie zeigt**.
///
/// `gelesen` ist die Antwort von `GET issue/{vorgang}` nach dem Anlegen. Jira
/// beschreibt eine Verknüpfung in beide Richtungen: `outward` gilt von
/// `outwardIssue` zum anderen Vorgang, `inward` umgekehrt. Erwartet wird
/// „vorgang <outward> ziel“; steht es andersherum im Vorgang, sagt der Text das
/// **deutlich**.
pub fn verknuepft(vorgang: &str, ziel: &str, art: &str, gelesen: &Value) -> String {
    let mut text = format!("Vorgang {vorgang} mit {ziel} als „{art}“ verknüpft.");

    for link in daten::liste(gelesen, "fields.issuelinks") {
        let aus = daten::text(link, "outwardIssue.key");
        let ein = daten::text(link, "inwardIssue.key");
        let zeigt_auf_ziel = aus.as_deref() == Some(ziel) || ein.as_deref() == Some(ziel);
        if !zeigt_auf_ziel {
            continue;
        }
        let outward = daten::text(link, "type.outward").unwrap_or_else(|| art.to_string());
        let inward = daten::text(link, "type.inward").unwrap_or_else(|| art.to_string());
        let typ = daten::text(link, "type.name").unwrap_or_else(|| art.to_string());
        let id = daten::text(link, "id")
            .map(|id| format!(", Verknüpfungs-ID {id}"))
            .unwrap_or_default();

        if aus.as_deref() == Some(ziel) {
            text.push_str(&format!(
                "\nJira zeigt: {vorgang} {outward} {ziel} · bei {ziel}: {ziel} {inward} \
                 {vorgang} (Art „{typ}“{id})."
            ));
        } else {
            text.push_str(&format!(
                "\nACHTUNG – Jira zeigt die Verknüpfung andersherum: bei {vorgang} steht \
                 „{inward} {ziel}“, also {ziel} {outward} {vorgang} (Art „{typ}“{id}). Bitte in \
                 Jira prüfen und die Verknüpfung gegebenenfalls löschen."
            ));
        }
        return text;
    }

    text.push_str(&format!(
        "\nAm Vorgang {vorgang} ist die Verknüpfung nicht zu sehen – bitte den Stand in Jira \
         prüfen (nicht einfach wiederholen)."
    ));
    text
}

/// `jira_verknuepfen`, wenn sich die Verknüpfung am Vorgang nicht nachlesen ließ.
///
/// Angelegt ist angelegt: kein Fehler, aber ein klarer Auftrag, den Stand zu
/// prüfen statt blind zu wiederholen.
pub fn verknuepft_ungelesen(vorgang: &str, ziel: &str, art: &str, grund: &str) -> String {
    format!(
        "Vorgang {vorgang} mit {ziel} als „{art}“ verknüpft. Die Verknüpfung ließ sich am \
         Vorgang aber nicht nachlesen ({grund}) – bitte den Stand in Jira prüfen, nicht einfach \
         wiederholen."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use quiver_tools_kern::zugaenge::Zugang as Eintrag;
    use serde_json::json;

    fn site() -> Site {
        Site::aus_zugang(&Eintrag {
            name: "firma".into(),
            werte: [
                ("url", "https://firma.atlassian.net"),
                ("email", "patrick@example.test"),
                ("api_token", "token-4711"),
            ]
            .into_iter()
            .map(|(feld, wert)| (feld.to_string(), wert.to_string()))
            .collect(),
        })
        .unwrap()
    }

    #[test]
    fn die_hinweise_nennen_das_quiver_fenster() {
        assert_eq!(
            token_ungueltig("firma"),
            "Token für firma ungültig oder abgelaufen – im quiver-Fenster erneuern \
             (Tools → Jira)."
        );
        assert_eq!(
            schreiben_aus("firma"),
            "Schreiben ist für firma aus – im quiver-Fenster einschalten."
        );
        assert!(keine_rechte("firma", "verboten").contains("Keine Rechte"));
        let text = nur_verletzt("firma", "XYZ", &["ABC".to_string()]);
        assert!(text.contains("XYZ"), "{text}");
        assert!(text.contains("ABC"), "{text}");
        assert!(text.contains("nichts geändert"), "{text}");
        assert_eq!(keine_site(), "Keine Jira-Site eingerichtet.");
        let text = suche_unbegrenzt("firma");
        assert!(text.contains("nur eine begrenzte JQL"), "{text}");
        assert!(text.contains("nicht gesucht"), "{text}");
        assert!(text.contains("project = ABC"), "{text}");
    }

    #[test]
    fn der_status_nennt_die_site_ohne_token() {
        let text = status(&[site()]);
        assert!(text.contains("1 Site(s)"), "{text}");
        assert!(text.contains("firma"), "{text}");
        assert!(text.contains("Schreiben aus"), "{text}");
        assert!(!text.contains("token-4711"), "{text}");
        assert!(!text.contains("patrick@example.test"), "{text}");
        assert_eq!(status(&[]), "Keine Jira-Site eingerichtet.");
    }

    #[test]
    fn der_test_nennt_den_benutzer_und_kein_geheimnis() {
        let text = testen(
            &site(),
            &json!({ "displayName": "Patrick Weppelmann",
                "emailAddress": "patrick@example.test", "accountId": "acc-123" }),
        );
        assert!(text.contains("Anmeldung an firma erfolgreich"), "{text}");
        assert!(text.contains("Patrick Weppelmann"), "{text}");
        assert!(text.contains("acc-123"), "{text}");
        assert!(text.contains("Schreiben: aus"), "{text}");
        assert!(!text.contains("token-4711"), "{text}");
    }

    #[test]
    fn projekte_zaehlen_und_nennen_den_schluessel() {
        let liste = json!([
            { "key": "ABC", "name": "Alpha", "projectTypeKey": "software",
              "lead": { "displayName": "Patrick" } }
        ]);
        let text = projekte("firma", daten::liste(&liste, ""), Some(1), false);
        assert!(text.contains("1 Projekt(e) in firma"), "{text}");
        assert!(
            text.contains("- ABC — Alpha (software, Leitung Patrick)"),
            "{text}"
        );

        let text = projekte("firma", daten::liste(&liste, ""), Some(5), true);
        assert!(text.contains("1 von 5 Projekt(e)"), "{text}");
        assert!(text.contains("es gibt weitere Einträge"), "{text}");

        let text = projekte("firma", &[], None, false);
        assert!(text.contains("Keine Projekte in firma"), "{text}");
    }

    #[test]
    fn die_suche_nennt_schluessel_status_und_naechste_seite() {
        let liste = json!([
            { "key": "ABC-1", "fields": { "summary": "Login kaputt",
              "status": { "name": "In Progress" }, "issuetype": { "name": "Bug" },
              "assignee": { "displayName": "Patrick" },
              "updated": "2026-11-20T10:15:00.000+0000" } }
        ]);
        let text = suchen(daten::liste(&liste, ""), Some(1), Some("seite-2"));
        assert!(
            text.contains("- ABC-1 [Bug] [In Progress] Login kaputt — zugewiesen: Patrick"),
            "{text}"
        );
        assert!(text.contains("2026-11-20 10:15"), "{text}");
        assert!(text.contains("seite-2"), "{text}");

        let text = suchen(daten::liste(&liste, ""), Some(1), None);
        assert!(text.contains("letzte Seite"), "{text}");
        assert_eq!(suchen(&[], None, None), "Keine Vorgänge gefunden.");
    }

    #[test]
    fn ein_vorgang_hat_beschreibung_links_und_unteraufgaben() {
        let baum = json!({
            "key": "ABC-1",
            "fields": {
                "summary": "Login kaputt",
                "issuetype": { "name": "Bug" },
                "status": { "name": "In Progress" },
                "project": { "key": "ABC", "name": "Alpha" },
                "assignee": { "displayName": "Patrick" },
                "reporter": { "displayName": "Anna" },
                "priority": { "name": "High" },
                "labels": ["login", "bug"],
                "created": "2026-11-01T09:00:00.000+0000",
                "updated": "2026-11-20T10:15:00.000+0000",
                "duedate": "2026-11-30",
                "description": { "type": "doc", "version": 1, "content": [
                    { "type": "paragraph", "content": [ { "type": "text", "text": "Der Anmelden-Knopf tut nichts." } ] }
                ] },
                "issuelinks": [
                    { "type": { "name": "Blocks", "outward": "blockiert", "inward": "wird blockiert von" },
                      "outwardIssue": { "key": "ABC-2" } },
                    { "type": { "name": "Relates", "outward": "bezieht sich auf", "inward": "bezieht sich auf" },
                      "inwardIssue": { "key": "ABC-3" } }
                ],
                "subtasks": [ { "key": "ABC-4", "fields": { "summary": "Unteraufgabe",
                    "status": { "name": "To Do" } } } ],
                "parent": { "key": "ABC-0", "fields": { "summary": "Elternvorgang",
                    "status": { "name": "Offen" } } }
            }
        });
        let text = vorgang(&site(), "ABC-1", &baum, 4000);
        assert!(
            text.starts_with("ABC-1 [Bug] [In Progress] Login kaputt"),
            "{text}"
        );
        assert!(text.contains("- Projekt: ABC (Alpha)"), "{text}");
        assert!(text.contains("- Zugewiesen: Patrick"), "{text}");
        assert!(text.contains("- Labels: login, bug"), "{text}");
        assert!(text.contains("fällig: 2026-11-30"), "{text}");
        assert!(
            text.contains("- URL: https://firma.atlassian.net/browse/ABC-1"),
            "{text}"
        );
        assert!(
            text.contains("## Beschreibung\nDer Anmelden-Knopf tut nichts."),
            "{text}"
        );
        assert!(text.contains("- blockiert ABC-2"), "{text}");
        assert!(text.contains("- bezieht sich auf ABC-3"), "{text}");
        assert!(
            text.contains("## Unteraufgaben\n- ABC-4 [To Do] Unteraufgabe"),
            "{text}"
        );
        assert!(
            text.contains("## Eltern\n- ABC-0 [Offen] Elternvorgang"),
            "{text}"
        );
    }

    #[test]
    fn kommentare_und_uebergaenge_werden_gelesen() {
        let antwort = json!({ "comments": [
            { "id": "100", "author": { "displayName": "Anna" },
              "created": "2026-11-20T10:00:00.000+0000",
              "body": { "type": "doc", "version": 1, "content": [
                  { "type": "paragraph", "content": [ { "type": "text", "text": "Sieht gut aus" } ] } ] } }
        ] });
        let text = kommentare("ABC-1", &antwort, 4000);
        assert!(text.contains("1 Kommentar(e) zu ABC-1"), "{text}");
        assert!(
            text.contains("**Anna** 2026-11-20 10:00 · Kommentar 100"),
            "{text}"
        );
        assert!(text.contains("Sieht gut aus"), "{text}");
        assert_eq!(
            kommentare("ABC-1", &json!({}), 4000),
            "Keine Kommentare zu ABC-1."
        );

        let antwort = json!({ "transitions": [
            { "id": "11", "name": "Start Progress", "to": { "name": "In Progress" } }
        ] });
        let text = uebergaenge("ABC-1", &antwort);
        assert!(text.contains("1 Übergang/Übergänge für ABC-1"), "{text}");
        assert!(text.contains("- 11 Start Progress → In Progress"), "{text}");
    }

    #[test]
    fn felder_zum_anlegen_nennen_pflicht_und_erlaubte_werte() {
        let typen = json!({ "issueTypes": [ { "id": "10001", "name": "Bug", "subtask": false } ],
                           "total": 1 });
        let text = vorgangstypen("ABC", &typen);
        assert!(text.contains("1 Vorgangstyp(en) in ABC"), "{text}");
        assert!(text.contains("- Bug · Typ-ID 10001"), "{text}");

        let felder = json!({ "fields": {
            "summary": { "required": true, "name": "Summary", "schema": { "type": "string" } },
            "priority": { "required": false, "name": "Priority",
                          "allowedValues": [ { "name": "High" }, { "name": "Low" } ] }
        } });
        let text = felder_zum_anlegen("ABC", "Bug", &felder);
        assert!(
            text.contains("2 Feld(er) für Bug in ABC (1 Pflicht)"),
            "{text}"
        );
        assert!(
            text.contains("- summary (Pflicht): Summary · Typ string"),
            "{text}"
        );
        assert!(
            text.contains("- priority: Priority · erlaubt: High, Low"),
            "{text}"
        );
    }

    #[test]
    fn filter_verknuepfungsarten_und_personen_werden_gelesen() {
        let filter = json!({ "values": [
            { "id": "10000", "name": "Meine Bugs", "jql": "project = ABC" }
        ] });
        let text = filter_liste(&filter, 4000);
        assert!(
            text.contains("- 10000 Meine Bugs · project = ABC"),
            "{text}"
        );

        let text = super::filter(
            &json!({ "id": "10000", "name": "Meine Bugs", "jql": "project = ABC",
                     "description": "Alle Bugs", "owner": { "displayName": "Patrick" } }),
            4000,
        );
        assert!(text.starts_with("10000 Meine Bugs"), "{text}");
        assert!(text.contains("JQL: project = ABC"), "{text}");
        assert!(text.contains("Besitzer: Patrick"), "{text}");

        let arten = json!({ "issueLinkTypes": [
            { "id": "10000", "name": "Blocks", "outward": "blocks", "inward": "is blocked by" }
        ] });
        let text = verknuepfungsarten(&arten);
        assert!(
            text.contains("- Blocks (ID 10000): blocks / is blocked by"),
            "{text}"
        );

        let personen_baum = json!([
            { "accountId": "acc-123", "displayName": "Patrick", "active": true },
            { "accountId": "acc-456", "displayName": "Anna", "active": false }
        ]);
        let text = personen(&personen_baum);
        assert!(text.contains("2 Person(en)"), "{text}");
        assert!(text.contains("- Patrick · accountId acc-123"), "{text}");
        assert!(
            text.contains("- Anna · accountId acc-456 · inaktiv"),
            "{text}"
        );
    }

    #[test]
    fn die_schreibantworten_sind_knapp_und_nennen_die_keys() {
        let text = angelegt(&site(), &json!({ "id": "10009", "key": "ABC-9" }));
        assert!(text.contains("Vorgang angelegt: ABC-9"), "{text}");
        assert!(
            text.contains("https://firma.atlassian.net/browse/ABC-9"),
            "{text}"
        );

        assert_eq!(
            geaendert("ABC-1", &["summary → Neu".to_string()]),
            "Vorgang ABC-1 geändert:\n- summary → Neu"
        );
        assert_eq!(
            uebergang("ABC-1", "21", "Fertig", Some("Done")),
            "Vorgang ABC-1: Übergang 21 „Fertig“ ausgeführt.\nNeuer Status: Done"
        );
        assert!(uebergang("ABC-1", "21", "Fertig", None).contains("nicht lesen"));
        assert_eq!(
            kommentiert("ABC-1", &json!({ "id": "101" })),
            "Kommentar 101 zu ABC-1 angelegt."
        );
    }

    #[test]
    fn eine_verknuepfung_wird_so_gemeldet_wie_jira_sie_zeigt() {
        // Der Vorgang, von dem der Link ausgeht, sieht den `outward`-Text.
        let passend = json!({ "fields": { "issuelinks": [
            { "id": "70000",
              "type": { "name": "Blocks", "inward": "is blocked by", "outward": "blocks" },
              "outwardIssue": { "key": "ABC-2" } }
        ] } });
        let text = verknuepft("ABC-1", "ABC-2", "Blocks", &passend);
        assert!(
            text.contains("Vorgang ABC-1 mit ABC-2 als „Blocks“ verknüpft"),
            "{text}"
        );
        assert!(text.contains("Jira zeigt: ABC-1 blocks ABC-2"), "{text}");
        assert!(
            text.contains("bei ABC-2: ABC-2 is blocked by ABC-1"),
            "{text}"
        );
        assert!(text.contains("Verknüpfungs-ID 70000"), "{text}");

        // Andersherum: das muss der Text deutlich sagen.
        let verdreht = json!({ "fields": { "issuelinks": [
            { "id": "70000",
              "type": { "name": "Blocks", "inward": "is blocked by", "outward": "blocks" },
              "inwardIssue": { "key": "ABC-2" } }
        ] } });
        let text = verknuepft("ABC-1", "ABC-2", "Blocks", &verdreht);
        assert!(text.contains("ACHTUNG"), "{text}");
        assert!(text.contains("andersherum"), "{text}");
        assert!(text.contains("ABC-2 blocks ABC-1"), "{text}");

        // Gar nicht zu sehen: nicht wiederholen, sondern prüfen lassen.
        let leer = json!({ "fields": { "issuelinks": [] } });
        let text = verknuepft("ABC-1", "ABC-2", "Blocks", &leer);
        assert!(text.contains("nicht zu sehen"), "{text}");
        assert!(text.contains("nicht einfach wiederholen"), "{text}");
    }

    #[test]
    fn lange_texte_werden_gekuerzt_mit_hinweis() {
        let lang = "wort ".repeat(200);
        let kurz = text(&lang, 50);
        assert!(kurz.contains("Zeichen weggelassen"), "{kurz}");
        assert!(kurz.lines().next().unwrap().chars().count() <= 50);
        assert_eq!(text("  kurz  ", 50), "kurz");
    }
}
