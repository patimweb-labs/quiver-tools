// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Daten → Text (Aufgabe 15).
//!
//! Reine Funktionen: sie bekommen eine Antwort von Zephyr (JSON) und geben
//! knappen Text zurück. Kein Netz, kein Zustand – darum gut zu prüfen und der
//! einzige Ort, an dem Formulierungen stehen.
//!
//! Gelesen wird **nur**, was in der OpenAPI-Datei steht. Verweise
//! (`project`, `status`, `priority`, `testCase`, `testCycle`, `environment`,
//! `folder`) tragen dort allein `id` und `self`; ihre Namen kommen aus
//! [`crate::namen`] und stehen hier als fertige Zuordnung bereit. Wo keine
//! Zuordnung vorliegt, steht die Id – nie ein erfundener Name.
//!
//! Zwei Regeln aus [12-atlassian-gemeinsam.md](../../docs/aufgaben/12-atlassian-gemeinsam.md):
//! Antworten sind **knapp**, lange Texte werden über `max_zeichen` gekürzt (mit
//! Hinweis), und jede Liste nennt Anzahl, Ids/Schlüssel und ob es mehr gibt.

use quiver_tools_kern::kuerzen;
use serde_json::{json, Value};

use crate::daten;
use crate::namen::{self, Namen};
use crate::zugaenge::Zugang;

/// So viele Zeichen kommen höchstens aus einem langen Text.
pub const MAX_ZEICHEN: usize = 4000;

/// So viele Einträge zeigt eine Liste ohne `anzahl`.
pub const STANDARD_ANZAHL: u32 = 25;

/// So viele Einträge zeigt eine Liste höchstens.
pub const MAX_ANZAHL: u32 = 100;

// ------------------------------------------------------------- Hinweise

/// Das Token gilt nicht (401).
pub fn token_ungueltig(verbindung: &str) -> String {
    format!(
        "Token für {verbindung} ungültig oder abgelaufen – im quiver-Fenster unter Tools → Zephyr \
         erneuern."
    )
}

/// Das Token darf nicht (403).
pub fn keine_rechte(verbindung: &str, grund: &str) -> String {
    let grund = grund.trim();
    if grund.is_empty() {
        format!(
            "Keine Rechte für {verbindung} (HTTP 403) – bitte die Rechte des API-Tokens in Zephyr \
             prüfen."
        )
    } else {
        format!(
            "Keine Rechte für {verbindung} (HTTP 403): {grund} – bitte die Rechte des API-Tokens \
             in Zephyr prüfen."
        )
    }
}

/// Der Schalter `schreiben` ist aus.
pub fn schreiben_aus(verbindung: &str) -> String {
    format!("Schreiben ist für {verbindung} aus – im quiver-Fenster einschalten.")
}

/// `nur` erlaubt dieses Projekt nicht.
pub fn nur_verboten(verbindung: &str, projekt: &str, erlaubt: &[String]) -> String {
    format!(
        "Schreiben ist für {verbindung} auf die Projekte {} beschränkt – {projekt} ist nicht \
         erlaubt. Es wurde nichts geändert.",
        erlaubt.join(", ")
    )
}

/// Weder Aufruf noch Zugang nennen ein Projekt.
pub fn projekt_fehlt(verbindung: &str) -> String {
    format!(
        "Projekt fehlt – bitte `projekt` angeben oder beim Zugang „{verbindung}“ im quiver-Fenster \
         eintragen."
    )
}

/// Es gibt keinen Zugang im quiver-Fenster.
pub fn keine_zugaenge() -> String {
    "Kein Zephyr-Zugang eingerichtet.".to_string()
}

// ------------------------------------------------------------- Text

/// Einen langen Text kürzen – mit Hinweis, wie viel fehlt.
pub fn text_kuerzen(text: &str, max_zeichen: usize) -> String {
    let text = text.trim();
    match kuerzen(text, max_zeichen) {
        (kurz, Some(weggelassen)) => format!("{kurz}\n… ({weggelassen} Zeichen weggelassen)"),
        (kurz, None) => kurz,
    }
}

/// Ein Datum mit Zeit, falls vorhanden.
fn datum(baum: &Value, pfad: &str) -> String {
    daten::datum(daten::text(baum, pfad)).unwrap_or_else(|| "-".to_string())
}

/// `QA-T1 (id 11)` – Schlüssel und Id, damit der Agent weiterarbeiten kann.
fn kennung(baum: &Value) -> String {
    match (daten::text(baum, "key"), daten::text(baum, "id")) {
        (Some(key), Some(id)) => format!("{key} (id {id})"),
        (Some(key), None) => key,
        (None, Some(id)) => format!("(id {id})"),
        (None, None) => "(ohne Kennung)".to_string(),
    }
}

/// Wie [`kennung`], aber öffentlich für die Antwort eines Anlegens.
pub fn ausfuehrung_kennung(baum: &Value) -> String {
    kennung(baum)
}

/// `(id 11)` – ein Verweis, dessen Schlüssel das Schema nicht hergibt.
fn verweis(baum: &Value) -> String {
    daten::verweis_id(baum)
        .map(|id| format!("(id {id})"))
        .unwrap_or_else(|| "(ohne Id)".to_string())
}

/// ` · a · b`, oder nichts.
fn anhang(teile: &[String]) -> String {
    if teile.is_empty() {
        String::new()
    } else {
        format!(" · {}", teile.join(" · "))
    }
}

/// Die Kopfzeile einer Liste: Anzahl, Gesamtzahl und der Hinweis auf `ab`.
fn liste_kopf(art: &str, gezeigt: usize, baum: &Value, ab: u32) -> String {
    let mut kopf = format!("{gezeigt} {art}");
    let mehr = daten::mehr(baum, ab, gezeigt);
    if mehr {
        if let Some(gesamt) = daten::gesamt(baum) {
            kopf.push_str(&format!(" von {gesamt}"));
        }
        let weiter = ab as usize + gezeigt;
        kopf.push_str(&format!(" – weitere mit `ab` = {weiter}"));
    }
    kopf
}

/// Ein Statusname aus der Zuordnung; sonst die Id des Verweises.
fn status_name(verweis: &Value, namen: &Namen, art: &str, projekt: &str) -> String {
    if let Some(name) = namen.status_name(art, projekt, verweis) {
        return name;
    }
    match daten::verweis_id(verweis) {
        Some(id) => format!("(id {id})"),
        None => "(ohne Status)".to_string(),
    }
}

// ------------------------------------------------------------- Zugang

/// `zephyr_status`: die Zugänge ohne Token.
pub fn status(zugaenge: &[Zugang]) -> String {
    if zugaenge.is_empty() {
        return keine_zugaenge();
    }
    let zeilen: Vec<String> = zugaenge
        .iter()
        .map(|zugang| format!("- {}", zugang.beschreibung()))
        .collect();
    format!(
        "{} Zugang/Zugänge für Zephyr:\n{}",
        zugaenge.len(),
        zeilen.join("\n")
    )
}

/// `zephyr_testen`: gilt das Token, und was ist sichtbar?
pub fn testen(zugang: &Zugang, baum: &Value) -> String {
    let gesamt = daten::gesamt(baum).unwrap_or_else(|| daten::liste(baum, "values").len() as i64);
    let beispiel = daten::liste(baum, "values")
        .first()
        .and_then(|projekt| daten::text(projekt, "key"));
    let mut text = format!(
        "{} – Token gilt ({} Projekt(e) sichtbar",
        zugang.name, gesamt
    );
    if let Some(beispiel) = beispiel {
        text.push_str(&format!(", z. B. {beispiel}"));
    }
    text.push(')');
    text
}

// ------------------------------------------------------------- Listen

/// `zephyr_projekte`: `Project` hat `id`, `key`, `jiraProjectId`, `enabled`.
pub fn projekte(baum: &Value, ab: u32) -> String {
    let eintraege = daten::liste(baum, "values");
    if eintraege.is_empty() {
        return "Keine Projekte sichtbar.".to_string();
    }
    let zeilen: Vec<String> = eintraege
        .iter()
        .map(|projekt| {
            let mut teile = Vec::new();
            if let Some(jira) = daten::text(projekt, "jiraProjectId") {
                teile.push(format!("Jira-Projekt: {jira}"));
            }
            if let Some(aktiv) = daten::wert(projekt, "enabled").and_then(Value::as_bool) {
                if !aktiv {
                    teile.push("nicht aktiv".to_string());
                }
            }
            format!("- {}{}", kennung(projekt), anhang(&teile))
        })
        .collect();
    format!(
        "{}:\n{}",
        liste_kopf("Projekt(e)", eintraege.len(), baum, ab),
        zeilen.join("\n")
    )
}

/// `zephyr_testfaelle`.
pub fn testfaelle(baum: &Value, ab: u32, namen: &Namen, projekt: &str) -> String {
    let eintraege = daten::liste(baum, "values");
    if eintraege.is_empty() {
        return "Keine Testfälle gefunden.".to_string();
    }
    let zeilen: Vec<String> = eintraege
        .iter()
        .map(|testfall| testfall_zeile(testfall, namen, projekt))
        .collect();
    format!(
        "{}:\n{}",
        liste_kopf("Testfall/Testfälle", eintraege.len(), baum, ab),
        zeilen.join("\n")
    )
}

/// Eine Zeile eines Testfalls.
fn testfall_zeile(baum: &Value, namen: &Namen, projekt: &str) -> String {
    let mut teile = Vec::new();
    if let Some(name) = daten::text(baum, "name") {
        teile.push(name);
    }
    if let Some(status) = daten::wert(baum, "status") {
        teile.push(format!(
            "Status: {}",
            status_name(status, namen, namen::STATUS_TESTFALL, projekt)
        ));
    }
    if let Some(prio) = daten::wert(baum, "priority") {
        let prio = namen
            .prio_name(projekt, prio)
            .unwrap_or_else(|| format!("(id {})", daten::verweis_id(prio).unwrap_or_default()));
        teile.push(format!("Priorität: {prio}"));
    }
    if let Some(ordner) = daten::verweis_id(baum.get("folder").unwrap_or(&Value::Null)) {
        teile.push(format!("Ordner: {ordner}"));
    }
    format!("- {}{}", kennung(baum), anhang(&teile))
}

/// `zephyr_testfall`: ein Testfall mit Schritten oder Skript.
///
/// `skript` ist die Antwort von `GET /testcases/{key}/testscript` (oder `None`,
/// wenn der Testfall Schritte hat); `schritte` sind die Einträge aus
/// `GET /testcases/{key}/teststeps`.
pub fn testfall(
    baum: &Value,
    skript: Option<&Value>,
    schritte: &[Value],
    max_zeichen: usize,
    namen: &Namen,
    projekt: &str,
) -> String {
    let mut text = format!(
        "{} · {}",
        kennung(baum),
        daten::text_oder(baum, "name", "(ohne Namen)")
    );
    let mut kopf = vec![format!("Projekt: {projekt}")];
    if let Some(status) = daten::wert(baum, "status") {
        kopf.push(format!(
            "Status: {}",
            status_name(status, namen, namen::STATUS_TESTFALL, projekt)
        ));
    }
    if let Some(prio) = daten::wert(baum, "priority") {
        let prio = namen
            .prio_name(projekt, prio)
            .unwrap_or_else(|| format!("(id {})", daten::verweis_id(prio).unwrap_or_default()));
        kopf.push(format!("Priorität: {prio}"));
    }
    if let Some(ordner) = daten::verweis_id(baum.get("folder").unwrap_or(&Value::Null)) {
        kopf.push(format!("Ordner: {ordner}"));
    }
    if let Some(besitzer) = daten::text(baum, "owner.accountId") {
        kopf.push(format!("Besitzer: {besitzer}"));
    }
    if let Some(erstellt) = daten::text(baum, "createdOn") {
        kopf.push(format!(
            "Erstellt: {}",
            daten::datum(Some(erstellt)).unwrap_or_default()
        ));
    }
    text.push_str(&anhang(&kopf));

    for (titel, pfad) in [("Objektiv", "objective"), ("Vorbedingung", "precondition")] {
        if let Some(wert) = daten::text(baum, pfad) {
            text.push_str(&format!("\n{titel}: {}", text_kuerzen(&wert, max_zeichen)));
        }
    }

    if !schritte.is_empty() {
        text.push_str(&format!("\nSchritte ({}):", schritte.len()));
        for (nummer, schritt) in schritte.iter().enumerate() {
            let inhalt = schritt.get("inline").unwrap_or(&Value::Null);
            let beschreibung = daten::text_oder(inhalt, "description", "(ohne Beschreibung)");
            let mut teile = Vec::new();
            if let Some(erwartet) = daten::text(inhalt, "expectedResult") {
                teile.push(format!("erwartet: {erwartet}"));
            }
            if let Some(daten_text) = daten::text(inhalt, "testData") {
                teile.push(format!("Daten: {daten_text}"));
            }
            text.push_str(&format!(
                "\n{}. {beschreibung}{}",
                nummer + 1,
                anhang(&teile)
            ));
        }
    } else if let Some(skript) = skript {
        let art = daten::text_oder(skript, "type", "(ohne Art)");
        match daten::text(skript, "text") {
            Some(inhalt) => text.push_str(&format!(
                "\nSkript ({art}):\n{}",
                text_kuerzen(&inhalt, max_zeichen)
            )),
            None => text.push_str(&format!("\nSkript ({art}) ohne Text.")),
        }
    }
    text
}

/// `zephyr_testzyklen`.
pub fn testzyklen(baum: &Value, ab: u32, namen: &Namen, projekt: &str) -> String {
    let eintraege = daten::liste(baum, "values");
    if eintraege.is_empty() {
        return "Keine Testzyklen gefunden.".to_string();
    }
    let zeilen: Vec<String> = eintraege
        .iter()
        .map(|zyklus| format!("- {}", zyklus_zeile(zyklus, namen, projekt)))
        .collect();
    format!(
        "{}:\n{}",
        liste_kopf("Testzyklus/Testzyklen", eintraege.len(), baum, ab),
        zeilen.join("\n")
    )
}

/// Eine Zeile eines Testzyklus.
fn zyklus_zeile(baum: &Value, namen: &Namen, projekt: &str) -> String {
    let mut teile = Vec::new();
    if let Some(name) = daten::text(baum, "name") {
        teile.push(name);
    }
    if let Some(status) = daten::wert(baum, "status") {
        teile.push(format!(
            "Status: {}",
            status_name(status, namen, namen::STATUS_ZYKLUS, projekt)
        ));
    }
    teile.push(zeitraum(baum));
    format!("{}{}", kennung(baum), anhang(&teile))
}

/// Der geplante Zeitraum eines Zyklus: `2026-11-01 – 2026-11-14`.
fn zeitraum(baum: &Value) -> String {
    let von = daten::datum(daten::text(baum, "plannedStartDate"));
    let bis = daten::datum(daten::text(baum, "plannedEndDate"));
    match (von, bis) {
        (Some(von), Some(bis)) => format!("{von} – {bis}"),
        (Some(von), None) => format!("ab {von}"),
        (None, Some(bis)) => format!("bis {bis}"),
        (None, None) => "ohne Zeitraum".to_string(),
    }
}

/// `zephyr_testzyklus`: der Zyklus und die Ausführungen je Status.
///
/// `ausfuehrungen` sind die gelesenen Einträge; `gesamt` ist die Zahl, die
/// Zephyr nennt (für den Hinweis, wenn nicht alle gezählt wurden).
pub fn testzyklus(
    baum: &Value,
    ausfuehrungen: &[Value],
    gesamt: Option<i64>,
    namen: &Namen,
    projekt: &str,
) -> String {
    let mut text = format!(
        "{} · {}",
        kennung(baum),
        daten::text_oder(baum, "name", "(ohne Namen)")
    );
    let mut kopf = vec![format!("Projekt: {projekt}")];
    if let Some(status) = daten::wert(baum, "status") {
        kopf.push(format!(
            "Status: {}",
            status_name(status, namen, namen::STATUS_ZYKLUS, projekt)
        ));
    }
    kopf.push(format!("Zeitraum: {}", zeitraum(baum)));
    text.push_str(&anhang(&kopf));

    if ausfuehrungen.is_empty() {
        text.push_str("\nKeine Ausführungen.");
        return text;
    }

    let mut zaehler: Vec<(String, usize)> = Vec::new();
    for ausfuehrung in ausfuehrungen {
        let verweis = ausfuehrung
            .get("testExecutionStatus")
            .unwrap_or(&Value::Null);
        let name = namen
            .status_name(namen::STATUS_AUSFUEHRUNG, projekt, verweis)
            .unwrap_or_else(|| format!("(id {})", daten::verweis_id(verweis).unwrap_or_default()));
        match zaehler.iter_mut().find(|(vorhanden, _)| *vorhanden == name) {
            Some((_, anzahl)) => *anzahl += 1,
            None => zaehler.push((name, 1)),
        }
    }
    // Häufigste zuerst, bei Gleichstand alphabetisch.
    zaehler.sort_by(|(a_name, a_anzahl), (b_name, b_anzahl)| {
        b_anzahl.cmp(a_anzahl).then_with(|| a_name.cmp(b_name))
    });

    let gezaehlt = ausfuehrungen.len();
    let herkunft = match gesamt {
        Some(gesamt) if gesamt > gezaehlt as i64 => {
            format!("aus {gezaehlt} von {gesamt} gelesenen")
        }
        _ => format!("aus {gezaehlt}"),
    };
    text.push_str(&format!("\nAusführungen je Status ({herkunft}):"));
    for (name, anzahl) in zaehler {
        text.push_str(&format!("\n- {name}: {anzahl}"));
    }
    text
}

/// `zephyr_ausfuehrungen`.
pub fn ausfuehrungen(baum: &Value, ab: u32, namen: &Namen, projekt: &str) -> String {
    let eintraege = daten::liste(baum, "values");
    if eintraege.is_empty() {
        return "Keine Ausführungen gefunden.".to_string();
    }
    let mut text = format!(
        "{}:",
        liste_kopf("Ausführung(en)", eintraege.len(), baum, ab)
    );
    for ausfuehrung in eintraege {
        text.push_str(&format!(
            "\n- {}",
            ausfuehrung_zeile(ausfuehrung, namen, projekt)
        ));
    }
    text
}

/// Eine Zeile einer Ausführung.
fn ausfuehrung_zeile(baum: &Value, namen: &Namen, projekt: &str) -> String {
    let mut teile = Vec::new();
    if let Some(testfall) = baum.get("testCase") {
        teile.push(format!("Testfall: {}", verweis(testfall)));
    }
    let status = baum.get("testExecutionStatus").unwrap_or(&Value::Null);
    teile.push(format!(
        "Status: {}",
        status_name(status, namen, namen::STATUS_AUSFUEHRUNG, projekt)
    ));
    if let Some(zyklus) = baum.get("testCycle").filter(|wert| !wert.is_null()) {
        let zyklus = namen
            .zyklus_key_gelesen(zyklus)
            .unwrap_or_else(|| verweis(zyklus));
        teile.push(format!("Zyklus: {zyklus}"));
    }
    if let Some(umgebung) = baum.get("environment").filter(|wert| !wert.is_null()) {
        let umgebung = namen
            .umgebung_name_gelesen(umgebung)
            .unwrap_or_else(|| verweis(umgebung));
        teile.push(format!("Umgebung: {umgebung}"));
    }
    format!("{}{}", kennung(baum), anhang(&teile))
}

/// `zephyr_ausfuehrung`: eine Ausführung mit Kommentar.
pub fn ausfuehrung(baum: &Value, max_zeichen: usize, namen: &Namen, projekt: &str) -> String {
    let status = baum.get("testExecutionStatus").unwrap_or(&Value::Null);
    let mut text = format!(
        "{} · Status: {}",
        kennung(baum),
        status_name(status, namen, namen::STATUS_AUSFUEHRUNG, projekt)
    );
    let mut kopf = vec![format!("Projekt: {projekt}")];
    if let Some(testfall) = baum.get("testCase") {
        kopf.push(format!("Testfall: {}", verweis(testfall)));
    }
    if let Some(zyklus) = baum.get("testCycle").filter(|wert| !wert.is_null()) {
        let zyklus = namen
            .zyklus_key_gelesen(zyklus)
            .unwrap_or_else(|| verweis(zyklus));
        kopf.push(format!("Zyklus: {zyklus}"));
    }
    if let Some(umgebung) = baum.get("environment").filter(|wert| !wert.is_null()) {
        let umgebung = namen
            .umgebung_name_gelesen(umgebung)
            .unwrap_or_else(|| verweis(umgebung));
        kopf.push(format!("Umgebung: {umgebung}"));
    }
    text.push_str(&anhang(&kopf));

    if daten::text(baum, "actualEndDate").is_some() {
        text.push_str(&format!("\nBeendet: {}", datum(baum, "actualEndDate")));
    }
    if let Some(zeit) = daten::zahl(baum, "executionTime") {
        text.push_str(&format!("\nDauer: {zeit} ms"));
    }
    if let Some(automatisiert) = daten::wert(baum, "automated").and_then(Value::as_bool) {
        text.push_str(if automatisiert {
            "\nArt: automatisiert"
        } else {
            "\nArt: manuell"
        });
    }
    if let Some(kommentar) = daten::text(baum, "comment") {
        text.push_str(&format!(
            "\nKommentar: {}",
            text_kuerzen(&kommentar, max_zeichen)
        ));
    }
    text
}

/// `zephyr_status_liste`: die Status eines Projekts.
pub fn status_liste(baum: &Value, ab: u32) -> String {
    let eintraege = daten::liste(baum, "values");
    if eintraege.is_empty() {
        return "Keine Status gefunden.".to_string();
    }
    let zeilen: Vec<String> = eintraege
        .iter()
        .map(|status| {
            let mut teile = Vec::new();
            if daten::wahr(status, "default") {
                teile.push("Standard".to_string());
            }
            if daten::wahr(status, "archived") {
                teile.push("archiviert".to_string());
            }
            if let Some(beschreibung) = daten::text(status, "description") {
                teile.push(beschreibung);
            }
            let name = daten::text_oder(status, "name", "(ohne Namen)");
            let id = daten::text(status, "id")
                .map(|id| format!(" (id {id})"))
                .unwrap_or_default();
            format!("- {name}{id}{}", anhang(&teile))
        })
        .collect();
    format!(
        "{}:\n{}",
        liste_kopf("Status", eintraege.len(), baum, ab),
        zeilen.join("\n")
    )
}

/// `zephyr_ordner`: die Ordner eines Projekts.
pub fn ordner(baum: &Value, ab: u32) -> String {
    let eintraege = daten::liste(baum, "values");
    if eintraege.is_empty() {
        return "Keine Ordner gefunden.".to_string();
    }
    let zeilen: Vec<String> = eintraege
        .iter()
        .map(|ordner| {
            let mut teile = Vec::new();
            if let Some(name) = daten::text(ordner, "name") {
                teile.push(name);
            }
            if let Some(art) = daten::text(ordner, "folderType") {
                teile.push(format!("Art: {art}"));
            }
            if let Some(eltern) = daten::text(ordner, "parentId") {
                teile.push(format!("Eltern: {eltern}"));
            }
            let id = daten::text_oder(ordner, "id", "?");
            format!("- {id}{}", anhang(&teile))
        })
        .collect();
    format!(
        "{}:\n{}",
        liste_kopf("Ordner", eintraege.len(), baum, ab),
        zeilen.join("\n")
    )
}

// ------------------------------------------------------------- Schreiben

/// `zephyr_testfall_anlegen`.
///
/// Die Antwort ist ein `KeyedCreatedResource`: `id`, `self` und `key` – **kein**
/// Name und kein Status. Der Name kommt darum aus dem Aufruf.
pub fn testfall_angelegt(baum: &Value, name: &str, projekt: &str) -> String {
    if baum.is_null() {
        return format!(
            "Testfall „{name}“ angelegt (Projekt {projekt}) – die Antwort nannte keine Kennung. \
             Bitte in Zephyr nachsehen, statt den Aufruf zu wiederholen."
        );
    }
    format!(
        "Testfall „{name}“ angelegt: {} · Projekt: {projekt}",
        kennung(baum)
    )
}

/// `zephyr_zyklus_testfaelle_hinzufuegen` – auch bei einem Abbruch.
///
/// `angelegt` sind die erfolgreichen Paare (Testfallschlüssel, Antwort),
/// `gescheitert` der erste Fehlschlag und `nicht_versucht` die Testfälle
/// danach. Nichts wird wiederholt.
pub fn zyklus_hinzugefuegt(
    zyklus: &str,
    angelegt: &[(String, Value)],
    gescheitert: Option<(&str, String)>,
    nicht_versucht: &[String],
) -> String {
    let gesamt = angelegt.len() + usize::from(gescheitert.is_some()) + nicht_versucht.len();
    let mut text = format!(
        "{} von {gesamt} Testfall/Testfällen zu {zyklus} hinzugefügt:",
        angelegt.len()
    );
    if angelegt.is_empty() {
        text.push_str("\n- (keiner)");
    }
    for (testfall, antwort) in angelegt {
        let kennung = if antwort.is_null() {
            "(ohne Kennung)".to_string()
        } else {
            kennung(antwort)
        };
        text.push_str(&format!("\n- {testfall} → Ausführung {kennung}"));
    }
    if let Some((testfall, fehler)) = gescheitert {
        text.push_str(&format!("\n{testfall} scheiterte: {fehler}"));
        if nicht_versucht.is_empty() {
            text.push_str("\nNicht versucht: (keiner)");
        } else {
            text.push_str(&format!("\nNicht versucht: {}", nicht_versucht.join(", ")));
        }
    }
    text
}

/// `zephyr_ergebnis_eintragen`.
pub fn ergebnis_gesetzt(
    kennung: &str,
    status: &str,
    kommentar: Option<&str>,
    automatisiert: Option<bool>,
) -> String {
    let mut text = format!("Ergebnis für {kennung} gesetzt: Status {status}");
    if let Some(automatisiert) = automatisiert {
        text.push_str(if automatisiert {
            " · Art: automatisiert"
        } else {
            " · Art: manuell"
        });
    }
    if let Some(kommentar) = kommentar.map(str::trim).filter(|text| !text.is_empty()) {
        text.push_str(&format!(" · Kommentar: {}", text_kuerzen(kommentar, 200)));
    }
    text
}

/// Der Rumpf für `PUT /testexecutions/{id}`.
///
/// Die OpenAPI-Datei sagt zu diesem Aufruf: *„This operation only updates
/// specified fields in the payload and ignores `null` or `undefined` values.“*
/// Nicht gesendete Felder bleiben also stehen – der Rumpf bleibt darum klein:
/// `statusName`, der gelesene Wert von `automated` und, wenn angegeben, der
/// neue `comment`.
///
/// `automated` **muss** mit: das Feld hat laut Schema den Standard `true`.
/// Ohne den gelesenen Wert würde eine manuelle Ausführung umspringen.
/// `projectKey`, `testCaseKey`, `testCycleKey` und `customFields` kennt
/// `TestExecutionUpdate` dagegen gar nicht – sie dürfen nicht mit.
pub fn aktualisierung(gelesen: &Value, status: &str, kommentar: Option<&str>) -> Value {
    let mut rumpf = serde_json::Map::new();
    rumpf.insert("statusName".to_string(), json!(status));

    if let Some(automatisiert) = daten::wert(gelesen, "automated").and_then(Value::as_bool) {
        rumpf.insert("automated".to_string(), json!(automatisiert));
    }
    if let Some(neuer) = kommentar.map(str::trim).filter(|text| !text.is_empty()) {
        rumpf.insert("comment".to_string(), json!(neuer));
    }

    Value::Object(rumpf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn zugang() -> Zugang {
        Zugang {
            name: "kunde".into(),
            basis_url: "https://api.example.test/v2".into(),
            token: "token-4711".into(),
            projekt: Some("QA".into()),
            nur: vec!["QA".into()],
            schreiben: false,
        }
    }

    /// Ein Namensspeicher, wie ihn ein Aufruf nach seinen Zusatzabfragen hat.
    fn namen() -> Namen {
        let mut namen = Namen::neu();
        namen.setze_status(
            namen::STATUS_TESTFALL,
            "QA",
            &[(1, "Draft", false), (2, "Approved", false)],
        );
        namen.setze_status(namen::STATUS_ZYKLUS, "QA", &[(1, "Draft", true)]);
        namen.setze_status(
            namen::STATUS_AUSFUEHRUNG,
            "QA",
            &[(1, "Pass", true), (2, "Fail", false), (3, "Blocked", false)],
        );
        namen.setze_prio("QA", &[(2, "High")]);
        namen.setze_umgebung("5", Some("Test"));
        namen.setze_zyklus("31", Some("QA-R1"));
        namen
    }

    fn verweis_json(id: i64) -> Value {
        json!({ "id": id, "self": format!("https://api.example.test/v2/x/{id}") })
    }

    #[test]
    fn hinweise_nennen_die_verbindung_und_das_quiver_fenster() {
        assert!(token_ungueltig("kunde").contains("Tools → Zephyr"));
        assert!(token_ungueltig("kunde").contains("ungültig oder abgelaufen"));
        assert_eq!(
            schreiben_aus("kunde"),
            "Schreiben ist für kunde aus – im quiver-Fenster einschalten."
        );
        assert!(projekt_fehlt("kunde").contains("`projekt` angeben"));
        assert!(keine_zugaenge().contains("Kein Zephyr-Zugang"));
        assert!(keine_rechte("kunde", "").contains("(HTTP 403)"));
        assert!(keine_rechte("kunde", "scope").contains("scope"));
        assert!(nur_verboten("kunde", "OPS", &["QA".to_string()]).contains("nichts geändert"));
    }

    #[test]
    fn status_und_testen_zeigen_kein_token() {
        let text = status(&[zugang()]);
        assert!(text.contains("1 Zugang/Zugänge"), "{text}");
        assert!(text.contains("Schreiben aus"), "{text}");
        assert!(!text.contains("token-4711"), "{text}");

        let baum = json!({ "total": 2, "values": [ { "id": 1, "key": "QA" } ] });
        let text = testen(&zugang(), &baum);
        assert!(text.contains("Token gilt"), "{text}");
        assert!(text.contains("2 Projekt(e)"), "{text}");
        assert!(text.contains("z. B. QA"), "{text}");
        assert!(!text.contains("token-4711"), "{text}");
    }

    #[test]
    fn ein_projekt_hat_keinen_namen_nur_schluessel_und_id() {
        let baum = json!({
            "total": 1,
            "values": [ { "id": 1, "key": "QA", "jiraProjectId": 10000, "enabled": true } ]
        });
        let text = projekte(&baum, 0);
        assert!(text.contains("- QA (id 1) · Jira-Projekt: 10000"), "{text}");
        assert!(!text.contains("Qualität"), "{text}");

        let aus = json!({
            "total": 1,
            "values": [ { "id": 2, "key": "OPS", "jiraProjectId": 10001, "enabled": false } ]
        });
        assert!(projekte(&aus, 0).contains("nicht aktiv"));
    }

    #[test]
    fn eine_liste_nennt_anzahl_und_den_naechsten_schritt() {
        let baum = json!({
            "total": 137,
            "isLast": false,
            "values": [
                {
                    "id": 11, "key": "QA-T1", "name": "Login",
                    "status": verweis_json(1),
                    "priority": verweis_json(2),
                    "folder": verweis_json(12)
                }
            ]
        });
        let text = testfaelle(&baum, 25, &namen(), "QA");
        assert!(
            text.starts_with("1 Testfall/Testfälle von 137 – weitere mit `ab` = 26:"),
            "{text}"
        );
        assert!(
            text.contains("- QA-T1 (id 11) · Login · Status: Draft · Priorität: High · Ordner: 12"),
            "{text}"
        );

        // Ohne weitere Seite kein Hinweis.
        let kurz = json!({ "total": 1, "isLast": true, "values": [ { "key": "QA-T1" } ] });
        assert!(!testfaelle(&kurz, 0, &namen(), "QA").contains("`ab`"));
    }

    #[test]
    fn ein_unbekannter_name_wird_als_id_gezeigt_nie_erfunden() {
        let baum = json!({
            "total": 1,
            "values": [ { "id": 11, "key": "QA-T1", "name": "Login", "status": verweis_json(9) } ]
        });
        let text = testfaelle(&baum, 0, &namen(), "QA");
        assert!(text.contains("Status: (id 9)"), "{text}");
    }

    #[test]
    fn ein_testfall_zeigt_schritte_oder_skript_aus_eigenen_abfragen() {
        let baum = json!({
            "id": 11, "key": "QA-T1", "name": "Login prüfen",
            "project": verweis_json(1),
            "status": verweis_json(1),
            "priority": verweis_json(2),
            "folder": verweis_json(12),
            "objective": "Anmelden klappt",
            "precondition": "Benutzer besteht",
            "testScript": { "self": "https://api.example.test/v2/testcases/QA-T1/testscript" }
        });
        let schritte = vec![
            json!({ "id": 1, "inline": { "description": "Seite öffnen", "expectedResult": "Formular sichtbar" } }),
            json!({ "id": 2, "inline": { "description": "Anmelden", "testData": "paula/pw" } }),
        ];
        let text = testfall(&baum, None, &schritte, MAX_ZEICHEN, &namen(), "QA");
        assert!(
            text.starts_with("QA-T1 (id 11) · Login prüfen · Projekt: QA · Status: Draft"),
            "{text}"
        );
        assert!(text.contains("Objektiv: Anmelden klappt"), "{text}");
        assert!(text.contains("Vorbedingung: Benutzer besteht"), "{text}");
        assert!(text.contains("Schritte (2):"), "{text}");
        assert!(
            text.contains("1. Seite öffnen · erwartet: Formular sichtbar"),
            "{text}"
        );
        assert!(text.contains("2. Anmelden · Daten: paula/pw"), "{text}");

        let skript = json!({ "id": 7, "type": "plain", "text": "1. klicken\n2. prüfen" });
        let text = testfall(&baum, Some(&skript), &[], MAX_ZEICHEN, &namen(), "QA");
        assert!(text.contains("Skript (plain):\n1. klicken"), "{text}");
    }

    #[test]
    fn ein_zyklus_zaehlt_die_ausfuehrungen_je_status() {
        let zyklus = json!({
            "id": 31, "key": "QA-R1", "name": "Login-Zyklus",
            "project": verweis_json(1), "status": verweis_json(1),
            "plannedStartDate": "2026-11-01T00:00:00Z",
            "plannedEndDate": "2026-11-14T00:00:00Z"
        });
        let ausfuehrungen = vec![
            json!({ "id": 101, "testExecutionStatus": verweis_json(1) }),
            json!({ "id": 102, "testExecutionStatus": verweis_json(2) }),
            json!({ "id": 103, "testExecutionStatus": verweis_json(1) }),
        ];
        let text = testzyklus(&zyklus, &ausfuehrungen, Some(5), &namen(), "QA");
        assert!(
            text.starts_with(
                "QA-R1 (id 31) · Login-Zyklus · Projekt: QA · Status: Draft · \
                 Zeitraum: 2026-11-01 – 2026-11-14"
            ),
            "{text}"
        );
        assert!(
            text.contains("Ausführungen je Status (aus 3 von 5 gelesenen):"),
            "{text}"
        );
        assert!(text.contains("- Pass: 2"), "{text}");
        assert!(text.contains("- Fail: 1"), "{text}");

        assert!(testzyklus(&zyklus, &[], None, &namen(), "QA").contains("Keine Ausführungen."));
    }

    #[test]
    fn eine_ausfuehrung_nennt_status_zyklus_umgebung_und_kommentar() {
        let baum = json!({
            "id": 101, "key": "QA-E1",
            "project": verweis_json(1),
            "testCase": json!({ "id": 11, "self": "https://api.example.test/v2/testcases/QA-T1/versions/1" }),
            "testCycle": verweis_json(31),
            "testExecutionStatus": verweis_json(1),
            "environment": verweis_json(5),
            "executionTime": 120,
            "actualEndDate": "2026-11-20T10:15:00Z",
            "automated": false,
            "comment": "läuft"
        });
        let text = ausfuehrung(&baum, MAX_ZEICHEN, &namen(), "QA");
        assert!(text.starts_with("QA-E1 (id 101) · Status: Pass"), "{text}");
        assert!(
            text.contains("Projekt: QA · Testfall: (id 11) · Zyklus: QA-R1 · Umgebung: Test"),
            "{text}"
        );
        assert!(text.contains("Beendet: 2026-11-20 10:15"), "{text}");
        assert!(text.contains("Dauer: 120 ms"), "{text}");
        assert!(text.contains("Art: manuell"), "{text}");
        assert!(text.contains("Kommentar: läuft"), "{text}");

        // Ohne Zuordnung bleibt die Id stehen – kein erfundener Name.
        let text = ausfuehrung(&baum, MAX_ZEICHEN, &Namen::neu(), "QA");
        assert!(text.contains("Status: (id 1)"), "{text}");
        assert!(text.contains("Zyklus: (id 31)"), "{text}");
        assert!(text.contains("Umgebung: (id 5)"), "{text}");
    }

    #[test]
    fn eine_liste_von_ausfuehrungen_nennt_das_testziel_als_id() {
        let baum = json!({
            "total": 1,
            "values": [ {
                "id": 101, "key": "QA-E1",
                "testCase": json!({ "id": 11, "self": "https://x/testcases/QA-T1/versions/1" }),
                "testExecutionStatus": verweis_json(2),
                "testCycle": verweis_json(31)
            } ]
        });
        let text = ausfuehrungen(&baum, 0, &namen(), "QA");
        assert!(
            text.contains("- QA-E1 (id 101) · Testfall: (id 11) · Status: Fail · Zyklus: QA-R1"),
            "{text}"
        );
    }

    #[test]
    fn ein_abbruch_nennt_angelegt_gescheitert_und_nicht_versucht() {
        let angelegt = vec![("QA-T1".to_string(), json!({ "id": 201, "key": "QA-E2" }))];
        let text = zyklus_hinzugefuegt(
            "QA-R1",
            &angelegt,
            Some(("QA-T2", "HTTP 500: kaputt".to_string())),
            &["QA-T3".to_string()],
        );
        assert!(
            text.starts_with("1 von 3 Testfall/Testfällen zu QA-R1 hinzugefügt:"),
            "{text}"
        );
        assert!(
            text.contains("- QA-T1 → Ausführung QA-E2 (id 201)"),
            "{text}"
        );
        assert!(
            text.contains("QA-T2 scheiterte: HTTP 500: kaputt"),
            "{text}"
        );
        assert!(text.contains("Nicht versucht: QA-T3"), "{text}");

        // Ohne Fehlschlag keine Zeilen dafür.
        let text = zyklus_hinzugefuegt("QA-R1", &angelegt, None, &[]);
        assert!(text.starts_with("1 von 1 Testfall/Testfällen"), "{text}");
        assert!(!text.contains("Nicht versucht"), "{text}");

        // Eine leere Antwort ist Erfolg ohne Kennung.
        let text = zyklus_hinzugefuegt("QA-R1", &[("QA-T1".to_string(), Value::Null)], None, &[]);
        assert!(text.contains("Ausführung (ohne Kennung)"), "{text}");
    }

    #[test]
    fn ein_ergebnis_sendet_nur_felder_aus_test_execution_update() {
        let gelesen = json!({
            "id": 101, "key": "QA-E1",
            "project": verweis_json(1),
            "testCase": json!({ "id": 11, "self": "https://x/testcases/QA-T1/versions/1" }),
            "testCycle": verweis_json(31),
            "environment": verweis_json(5),
            "testExecutionStatus": verweis_json(2),
            "executionTime": 120,
            "actualEndDate": "2026-11-20T10:15:00Z",
            "comment": "alter Kommentar",
            "automated": false,
            "customFields": { "browser": "chrome" }
        });
        let rumpf = aktualisierung(&gelesen, "Pass", None);
        let felder: Vec<&String> = rumpf.as_object().unwrap().keys().collect();
        assert_eq!(felder, vec!["automated", "statusName"], "{rumpf}");
        assert_eq!(rumpf["statusName"], json!("Pass"));
        // `automated` hat den Standard `true` – der gelesene Wert muss mit.
        assert_eq!(rumpf["automated"], json!(false));
        // Was `TestExecutionUpdate` nicht kennt, geht nicht mit.
        for falsch in [
            "id",
            "key",
            "projectKey",
            "testCaseKey",
            "testCycleKey",
            "environmentName",
            "customFields",
        ] {
            assert!(rumpf.get(falsch).is_none(), "{falsch} in {rumpf}");
        }

        let rumpf = aktualisierung(&gelesen, "Pass", Some("neu"));
        assert_eq!(rumpf["comment"], json!("neu"));
        assert_eq!(rumpf["automated"], json!(false));

        // Ohne gelesenes `automated` wird nichts behauptet.
        let rumpf = aktualisierung(&json!({ "id": 1 }), "Pass", None);
        assert!(rumpf.get("automated").is_none(), "{rumpf}");

        assert_eq!(
            ergebnis_gesetzt("QA-E1", "Pass", Some("neu"), Some(false)),
            "Ergebnis für QA-E1 gesetzt: Status Pass · Art: manuell · Kommentar: neu"
        );

        // Die Anlege-Antwort ist ein `KeyedCreatedResource`: kein Name.
        assert_eq!(
            testfall_angelegt(&json!({ "id": 13, "key": "QA-T3" }), "Neuer Fall", "QA"),
            "Testfall „Neuer Fall“ angelegt: QA-T3 (id 13) · Projekt: QA"
        );
        assert!(testfall_angelegt(&Value::Null, "Neu", "QA").contains("keine Kennung"));
    }

    #[test]
    fn statusliste_und_ordner_werden_gelesen() {
        let baum = json!({
            "total": 1,
            "values": [
                { "id": 1, "name": "Pass", "default": true, "description": "bestanden" }
            ]
        });
        let text = status_liste(&baum, 0);
        assert!(
            text.contains("- Pass (id 1) · Standard · bestanden"),
            "{text}"
        );

        let baum = json!({
            "total": 1,
            "values": [ { "id": 12, "name": "Login", "folderType": "TEST_CASE", "parentId": 3 } ]
        });
        let text = ordner(&baum, 0);
        assert!(
            text.contains("- 12 · Login · Art: TEST_CASE · Eltern: 3"),
            "{text}"
        );
    }
}
