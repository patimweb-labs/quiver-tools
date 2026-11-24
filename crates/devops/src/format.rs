// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Daten → Text (Aufgabe 09).
//!
//! Reine Funktionen: sie bekommen eine Antwort von Azure DevOps (JSON) und
//! geben knappen Text zurück. Kein Netz, kein Zustand – darum gut zu prüfen und
//! der einzige Ort, an dem Formulierungen stehen.
//!
//! Zwei Regeln: Antworten sind **knapp** (ein Agent zahlt Tokens), und ein
//! langer Text aus HTML wird über `max_zeichen` gekürzt – mit Hinweis, wie viel
//! fehlt.

use quiver_tools_kern::kuerzen;
use serde_json::Value;

use crate::daten;
use crate::html;
use crate::organisationen::Organisation;

/// So viele Zeichen kommen höchstens aus einer Beschreibung oder einem Kommentar.
pub const MAX_ZEICHEN: usize = 4000;

/// So viele Commits zeigt `devops_pull_request` höchstens.
pub const MAX_COMMITS: usize = 20;

/// So viele Zeilen zeigt `devops_lauf_log` standardmäßig.
pub const MAX_ZEILEN: usize = 200;

// ------------------------------------------------------------- Hinweise

/// Kein PAT, abgelaufen oder abgelehnt.
pub fn pat_ungueltig(verbindung: &str) -> String {
    format!(
        "PAT für {verbindung} ungültig oder abgelaufen – im quiver-Fenster unter Tools → DevOps \
         erneuern."
    )
}

/// Der Schalter `schreiben` ist aus.
pub fn schreiben_aus(verbindung: &str) -> String {
    format!("Schreiben ist für {verbindung} aus – im quiver-Fenster einschalten.")
}

/// Weder Aufruf noch Zugang nennen ein Projekt.
pub fn projekt_fehlt(verbindung: &str) -> String {
    format!(
        "Projekt fehlt – bitte `projekt` angeben oder beim Zugang „{verbindung}“ im quiver-Fenster \
         eintragen."
    )
}

/// Ein Team wird gebraucht, ist aber nirgends angegeben.
pub fn team_fehlt() -> String {
    "Team fehlt – bitte `team` angeben (Azure DevOps zeigt die Teams unter Project settings → \
     Teams)."
        .to_string()
}

/// Es gibt keine Organisation im quiver-Fenster.
pub fn keine_organisation() -> String {
    "Keine Azure-DevOps-Organisation eingerichtet.".to_string()
}

// ------------------------------------------------------------- Text

/// HTML-Text (Beschreibung, Kommentar) als Text, gekürzt.
pub fn beschreibung(html_text: &str, max_zeichen: usize) -> String {
    let text = html::text_aus_html(html_text);
    let (kurz, weggelassen) = kuerzen(&text, max_zeichen);
    match weggelassen {
        Some(anzahl) => format!("{kurz}\n… ({anzahl} Zeichen weggelassen)"),
        None => kurz,
    }
}

/// Die letzten `zeilen` Zeilen eines Textes, mit Hinweis auf den Rest.
pub fn ende(text: &str, zeilen: usize) -> (String, usize, usize) {
    let alle: Vec<&str> = text.lines().collect();
    let gesamt = alle.len();
    let zeilen = zeilen.max(1);
    if gesamt <= zeilen {
        return (alle.join("\n"), gesamt, 0);
    }
    (alle[gesamt - zeilen..].join("\n"), gesamt, gesamt - zeilen)
}

/// Eine Größe in Bytes lesbar machen: `4,2 MB`, `812 kB`.
fn groesse(bytes: i64) -> String {
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

/// Ein Datum mit Zeit, falls vorhanden.
fn datum(baum: &Value, pfad: &str) -> String {
    daten::datum(daten::text(baum, pfad)).unwrap_or_else(|| "-".to_string())
}

// ------------------------------------------------------------- Zugang

/// `devops_status`: die Organisationen ohne PAT.
pub fn status(organisationen: &[Organisation]) -> String {
    if organisationen.is_empty() {
        return keine_organisation();
    }
    let zeilen: Vec<String> = organisationen
        .iter()
        .map(|organisation| format!("- {}", organisation.beschreibung()))
        .collect();
    format!(
        "{} Organisation(en) für DevOps:\n{}",
        organisationen.len(),
        zeilen.join("\n")
    )
}

/// `devops_testen`: wer angemeldet ist und zu welcher Organisation.
pub fn testen(organisation: &Organisation, baum: &Value) -> String {
    let anzeige = daten::text(baum, "authenticatedUser.displayName")
        .or_else(|| daten::text(baum, "authenticatedUser.providerDisplayName"))
        .unwrap_or_else(|| "unbekannt".to_string());
    let konto = daten::text(baum, "authenticatedUser.properties.Account.$value");
    let mut text = format!(
        "Anmeldung an {} erfolgreich: {}",
        organisation.name, anzeige
    );
    if let Some(konto) = konto {
        text.push_str(&format!(" (Konto {konto})"));
    }
    text.push_str(&format!("\nAdresse: {}", organisation.url));
    if let Some(projekt) = &organisation.projekt {
        text.push_str(&format!(" · Projekt: {projekt}"));
    }
    text.push_str(&format!(
        " · Schreiben: {}",
        if organisation.schreiben { "an" } else { "aus" }
    ));
    text
}

// ------------------------------------------------------------- Arbeitsitems

/// `devops_projekte`.
pub fn projekte(verbindung: &str, baum: &Value) -> String {
    let eintraege = daten::liste(baum, "value");
    if eintraege.is_empty() {
        return format!("Keine Projekte in {verbindung} sichtbar.");
    }
    let zeilen: Vec<String> = eintraege
        .iter()
        .map(|projekt| {
            let mut zeile = format!("- {}", daten::text_oder(projekt, "name", "(ohne Namen)"));
            let mut kennzeichen = Vec::new();
            if let Some(zustand) = daten::text(projekt, "state") {
                kennzeichen.push(zustand);
            }
            if let Some(sichtbarkeit) = daten::text(projekt, "visibility") {
                kennzeichen.push(sichtbarkeit);
            }
            if !kennzeichen.is_empty() {
                zeile.push_str(&format!(" ({})", kennzeichen.join(", ")));
            }
            zeile
        })
        .collect();
    format!(
        "{} Projekt(e) in {verbindung}:\n{}",
        eintraege.len(),
        zeilen.join("\n")
    )
}

/// Ein Arbeitsitem in einer Zeile: `#123 [Bug] [Active] Titel — zugewiesen: X`.
fn item_zeile(item: &Value) -> String {
    let mut zeile = format!(
        "#{} [{}] [{}] {}",
        daten::text_oder(item, "id", "?"),
        daten::text_oder(item, "fields.System.WorkItemType", "?"),
        daten::text_oder(item, "fields.System.State", "?"),
        daten::text_oder(item, "fields.System.Title", "(ohne Titel)")
    );
    if let Some(zugewiesen) = daten::text(item, "fields.System.AssignedTo") {
        zeile.push_str(&format!(" — zugewiesen: {zugewiesen}"));
    }
    zeile
}

/// `devops_workitems_suchen` und `devops_iteration_workitems`: die Trefferliste.
pub fn arbeitsitems(ueberschrift: &str, baum: &Value) -> String {
    let eintraege = daten::liste(baum, "value");
    if eintraege.is_empty() {
        return format!("{ueberschrift}\nKeine Arbeitsitems gefunden.");
    }
    let zeilen: Vec<String> = eintraege
        .iter()
        .map(|item| format!("- {}", item_zeile(item)))
        .collect();
    format!(
        "{ueberschrift}\n{} Arbeitsitem(s):\n{}",
        eintraege.len(),
        zeilen.join("\n")
    )
}

/// `devops_workitem`: ein Arbeitsitem mit Beschreibung, Kriterien und Links.
pub fn arbeitsitem(baum: &Value, max_zeichen: usize) -> String {
    let mut text = format!(
        "#{} [{}] [{}] {}",
        daten::text_oder(baum, "id", "?"),
        daten::text_oder(baum, "fields.System.WorkItemType", "?"),
        daten::text_oder(baum, "fields.System.State", "?"),
        daten::text_oder(baum, "fields.System.Title", "(ohne Titel)")
    );
    if let Some(revision) = daten::zahl(baum, "rev") {
        text.push_str(&format!(" (Revision {revision})"));
    }
    let mut kopf = Vec::new();
    for (name, pfad) in [
        ("Bereich", "fields.System.AreaPath"),
        ("Iteration", "fields.System.IterationPath"),
        ("Zugewiesen", "fields.System.AssignedTo"),
        ("Priorität", "fields.System.Priority"),
        ("Schweregrad", "fields.Microsoft.VSTS.Common.Severity"),
        ("Tags", "fields.System.Tags"),
    ] {
        if let Some(wert) = daten::text(baum, pfad) {
            kopf.push(format!("- {name}: {wert}"));
        }
    }
    kopf.push(format!(
        "- Geändert: {}",
        datum(baum, "fields.System.ChangedDate")
    ));
    if let Some(url) = daten::text(baum, "_links.html.href") {
        kopf.push(format!("- URL: {url}"));
    }
    text.push('\n');
    text.push_str(&kopf.join("\n"));

    for (titel, pfad) in [
        ("Beschreibung", "fields.System.Description"),
        (
            "Akzeptanzkriterien",
            "fields.Microsoft.VSTS.Common.AcceptanceCriteria",
        ),
        ("Reproduktion", "fields.Microsoft.VSTS.TCM.ReproSteps"),
    ] {
        if let Some(inhalt) = daten::text(baum, pfad) {
            let kurz = beschreibung(&inhalt, max_zeichen);
            if !kurz.is_empty() {
                text.push_str(&format!("\n\n## {titel}\n{kurz}"));
            }
        }
    }

    let links = links(baum);
    if !links.is_empty() {
        text.push_str("\n\n## Links");
        for link in links {
            text.push_str(&format!("\n- {link}"));
        }
    }
    text
}

/// Die Links eines Arbeitsitems als `Eltern #100`, `Kind #101`, `verwandt #102`.
fn links(baum: &Value) -> Vec<String> {
    daten::liste(baum, "relations")
        .iter()
        .filter_map(|relation| {
            let typ = daten::text(relation, "rel")?;
            let art = match typ.as_str() {
                "System.LinkTypes.Hierarchy-Reverse" => "Eltern",
                "System.LinkTypes.Hierarchy-Forward" => "Kind",
                "System.LinkTypes.Related" => "verwandt",
                "System.LinkTypes.Duplicate-Forward" => "Duplikat von",
                "System.LinkTypes.Duplicate-Reverse" => "dupliziert durch",
                "System.LinkTypes.Dependency-Forward" => "Nachfolger",
                "System.LinkTypes.Dependency-Reverse" => "Vorgänger",
                "AttachedFile" => "Anhang",
                _ => typ.as_str(),
            };
            let ziel = daten::text(relation, "url").unwrap_or_default();
            let nummer = ziel.rsplit('/').next().unwrap_or_default();
            let nummer = nummer.split('?').next().unwrap_or(nummer);
            if nummer.chars().all(|zeichen| zeichen.is_ascii_digit()) && !nummer.is_empty() {
                Some(format!("{art} #{nummer}"))
            } else {
                Some(format!("{art} {ziel}"))
            }
        })
        .collect()
}

/// `devops_workitem_kommentare`.
pub fn kommentare(id: u64, baum: &Value, max_zeichen: usize) -> String {
    let eintraege = daten::liste(baum, "comments");
    if eintraege.is_empty() {
        return format!("Keine Kommentare zu #{id}.");
    }
    let mut text = format!("{} Kommentar(e) zu #{id}:", eintraege.len());
    for kommentar in eintraege {
        text.push_str(&format!(
            "\n\n**{}** {}",
            daten::text_oder(kommentar, "createdBy.displayName", "unbekannt"),
            datum(kommentar, "createdDate")
        ));
        let inhalt = daten::text(kommentar, "text").unwrap_or_default();
        text.push('\n');
        text.push_str(&beschreibung(&inhalt, max_zeichen));
    }
    text
}

/// `devops_workitem_typen`.
pub fn arbeitsitem_typen(baum: &Value) -> String {
    let eintraege = daten::liste(baum, "value");
    if eintraege.is_empty() {
        return "Keine Arbeitsitem-Typen sichtbar.".to_string();
    }
    let zeilen: Vec<String> = eintraege
        .iter()
        .map(|typ| {
            let mut zeile = format!(
                "- {} ({})",
                daten::text_oder(typ, "name", "?"),
                daten::text_oder(typ, "referenceName", "?")
            );
            let status: Vec<String> = daten::liste(typ, "states")
                .iter()
                .filter_map(|status| daten::text(status, "name"))
                .collect();
            if !status.is_empty() {
                zeile.push_str(&format!(" · Status: {}", status.join(", ")));
            }
            zeile
        })
        .collect();
    format!(
        "{} Arbeitsitem-Typ(en):\n{}",
        eintraege.len(),
        zeilen.join("\n")
    )
}

/// `devops_iterationen`.
pub fn iterationen(baum: &Value) -> String {
    let eintraege = daten::liste(baum, "value");
    if eintraege.is_empty() {
        return "Keine Iterationen gefunden.".to_string();
    }
    let zeilen: Vec<String> = eintraege
        .iter()
        .map(|iteration| {
            let mut zeile = format!("- {}", daten::text_oder(iteration, "path", "(ohne Pfad)"));
            let start = datum(iteration, "attributes.startDate");
            let ende = datum(iteration, "attributes.finishDate");
            if start != "-" || ende != "-" {
                zeile.push_str(&format!(" · {start} – {ende}"));
            }
            if let Some(zeitraum) = daten::text(iteration, "attributes.timeFrame") {
                zeile.push_str(&format!(" · {zeitraum}"));
            }
            if let Some(id) = daten::text(iteration, "id") {
                zeile.push_str(&format!(" · {id}"));
            }
            zeile
        })
        .collect();
    format!("{} Iteration(en):\n{}", eintraege.len(), zeilen.join("\n"))
}

// ------------------------------------------------------------- Repos

/// `devops_repos`.
pub fn repos(baum: &Value) -> String {
    let eintraege = daten::liste(baum, "value");
    if eintraege.is_empty() {
        return "Keine Repositories gefunden.".to_string();
    }
    let zeilen: Vec<String> = eintraege
        .iter()
        .map(|repo| {
            let mut zeile = format!("- {}", daten::text_oder(repo, "name", "(ohne Namen)"));
            if let Some(branch) = daten::text(repo, "defaultBranch") {
                zeile.push_str(&format!(" · Standardbranch {branch}"));
            }
            if let Some(bytes) = daten::zahl(repo, "size") {
                zeile.push_str(&format!(" · {}", groesse(bytes)));
            }
            zeile
        })
        .collect();
    format!("{} Repository(s):\n{}", eintraege.len(), zeilen.join("\n"))
}

/// `devops_branches`.
pub fn branches(repo: &str, baum: &Value) -> String {
    let eintraege = daten::liste(baum, "value");
    if eintraege.is_empty() {
        return format!("Keine Branches in {repo}.");
    }
    let zeilen: Vec<String> = eintraege
        .iter()
        .map(|branch| {
            let mut zeile = format!("- {}", daten::text_oder(branch, "name", "(ohne Namen)"));
            if let Some(sha) = daten::text(branch, "objectId") {
                let kurz: String = sha.chars().take(8).collect();
                zeile.push_str(&format!(" · {kurz}"));
            }
            if let Some(ersteller) = daten::text(branch, "creator.displayName") {
                zeile.push_str(&format!(" · {ersteller}"));
            }
            zeile
        })
        .collect();
    format!(
        "{} Branch(es) in {repo}:\n{}",
        eintraege.len(),
        zeilen.join("\n")
    )
}

// ------------------------------------------------------------- Pull Requests

/// Der kurze Branch-Name ohne `refs/heads/`.
fn branch_kurz(name: &str) -> String {
    name.strip_prefix("refs/heads/").unwrap_or(name).to_string()
}

/// `devops_pull_requests`.
pub fn pull_requests(baum: &Value) -> String {
    let eintraege = daten::liste(baum, "value");
    if eintraege.is_empty() {
        return "Keine Pull Requests gefunden.".to_string();
    }
    let zeilen: Vec<String> = eintraege
        .iter()
        .map(|pr| {
            let mut zeile = format!(
                "- #{} [{}] {}",
                daten::text_oder(pr, "pullRequestId", "?"),
                daten::text_oder(pr, "status", "?"),
                daten::text_oder(pr, "title", "(ohne Titel)")
            );
            if let Some(repo) = daten::repo_von_pull_request(pr) {
                zeile.push_str(&format!(" · {repo}"));
            }
            zeile.push_str(&format!(
                " · {} → {}",
                branch_kurz(&daten::text_oder(pr, "sourceRefName", "?")),
                branch_kurz(&daten::text_oder(pr, "targetRefName", "?"))
            ));
            if let Some(ersteller) = daten::text(pr, "createdBy.displayName") {
                zeile.push_str(&format!(" · {ersteller}"));
            }
            if daten::wahr(pr, "isDraft") {
                zeile.push_str(" · Entwurf");
            }
            zeile
        })
        .collect();
    format!(
        "{} Pull Request(s):\n{}",
        eintraege.len(),
        zeilen.join("\n")
    )
}

/// Ein Vote-Wert als Text.
pub fn stimme_anzeige(wert: i64) -> &'static str {
    match wert {
        10 => "abgestimmt",
        5 => "abgestimmt mit Anmerkungen",
        0 => "ohne Stimme",
        -5 => "wartet auf Autor",
        -10 => "abgelehnt",
        _ => "unbekannt",
    }
}

/// `devops_pull_request`: Kopf, Prüfer, verknüpfte Arbeitsitems, Commits.
pub fn pull_request(baum: &Value, commits: &Value, items: &Value, max_zeichen: usize) -> String {
    let id = daten::text_oder(baum, "pullRequestId", "?");
    let mut text = format!(
        "#{} [{}] {}",
        id,
        daten::text_oder(baum, "status", "?"),
        daten::text_oder(baum, "title", "(ohne Titel)")
    );
    if daten::wahr(baum, "isDraft") {
        text.push_str(" (Entwurf)");
    }
    text.push('\n');
    let mut kopf = Vec::new();
    if let Some(repo) = daten::repo_von_pull_request(baum) {
        kopf.push(format!("- Repo: {repo}"));
    }
    kopf.push(format!(
        "- Branch: {} → {}",
        branch_kurz(&daten::text_oder(baum, "sourceRefName", "?")),
        branch_kurz(&daten::text_oder(baum, "targetRefName", "?"))
    ));
    kopf.push(format!(
        "- Erstellt von {} am {}",
        daten::text_oder(baum, "createdBy.displayName", "unbekannt"),
        datum(baum, "creationDate")
    ));
    if let Some(status) = daten::text(baum, "mergeStatus") {
        kopf.push(format!("- Zusammenführung: {status}"));
    }
    if let Some(url) = daten::text(baum, "_links.web.href") {
        kopf.push(format!("- URL: {url}"));
    }
    text.push_str(&kopf.join("\n"));

    if let Some(inhalt) = daten::text(baum, "description") {
        let kurz = beschreibung(&inhalt, max_zeichen);
        if !kurz.is_empty() {
            text.push_str(&format!("\n\n## Beschreibung\n{kurz}"));
        }
    }

    let pruefer = daten::liste(baum, "reviewers");
    if !pruefer.is_empty() {
        text.push_str("\n\n## Prüfer");
        for pruefer in pruefer {
            let stimme = daten::zahl(pruefer, "vote").unwrap_or(0);
            text.push_str(&format!(
                "\n- {} · {}",
                daten::text_oder(pruefer, "displayName", "unbekannt"),
                stimme_anzeige(stimme)
            ));
        }
    }

    let verknuepft = daten::liste(items, "value");
    if !verknuepft.is_empty() {
        text.push_str("\n\n## Verknüpfte Arbeitsitems");
        for item in verknuepft {
            text.push_str(&format!("\n- #{}", daten::text_oder(item, "id", "?")));
        }
    }

    let commits_liste = daten::liste(commits, "value");
    if !commits_liste.is_empty() {
        let anzahl = commits_liste.len().min(MAX_COMMITS);
        text.push_str(&format!("\n\n## Commits ({anzahl})"));
        for commit in commits_liste.iter().take(MAX_COMMITS) {
            let sha = daten::text_oder(commit, "commitId", "");
            let kurz: String = sha.chars().take(8).collect();
            text.push_str(&format!(
                "\n- {kurz} {} {}: {}",
                daten::text_oder(commit, "author.name", "unbekannt"),
                datum(commit, "author.date"),
                daten::text_oder(commit, "comment", "(ohne Nachricht)")
            ));
        }
        if commits_liste.len() > MAX_COMMITS {
            text.push_str(&format!(
                "\n… {} weitere Commits",
                commits_liste.len() - MAX_COMMITS
            ));
        }
    }
    text
}

/// `devops_pull_request_threads`: Kommentar-Threads, ohne System-Threads.
pub fn threads(pull_request: u64, baum: &Value, max_zeichen: usize) -> String {
    let alle = daten::liste(baum, "value");
    let threads: Vec<&Value> = alle
        .iter()
        .filter(|thread| !daten::ist_system_thread(thread))
        .collect();
    if threads.is_empty() {
        return format!("Keine Kommentar-Threads an PR #{pull_request}.");
    }
    let mut text = format!(
        "{} Kommentar-Thread(s) an PR #{pull_request}:",
        threads.len()
    );
    for thread in threads {
        text.push_str(&format!(
            "\n\n**Thread {}** ({})",
            daten::text_oder(thread, "id", "?"),
            daten::text_oder(thread, "status", "?")
        ));
        for kommentar in daten::liste(thread, "comments") {
            text.push_str(&format!(
                "\n- {} {}: {}",
                daten::text_oder(kommentar, "author.displayName", "unbekannt"),
                datum(kommentar, "publishedDate"),
                beschreibung(
                    &daten::text(kommentar, "content").unwrap_or_default(),
                    max_zeichen
                )
            ));
        }
    }
    text
}

// ------------------------------------------------------------- Pipelines

/// `devops_pipelines`.
pub fn pipelines(baum: &Value) -> String {
    let eintraege = daten::liste(baum, "value");
    if eintraege.is_empty() {
        return "Keine Pipelines gefunden.".to_string();
    }
    let zeilen: Vec<String> = eintraege
        .iter()
        .map(|pipeline| {
            let mut zeile = format!(
                "- #{} {}",
                daten::text_oder(pipeline, "id", "?"),
                daten::text_oder(pipeline, "name", "(ohne Namen)")
            );
            if let Some(ordner) = daten::text(pipeline, "folder") {
                zeile.push_str(&format!(" · Ordner {ordner}"));
            }
            zeile
        })
        .collect();
    format!("{} Pipeline(s):\n{}", eintraege.len(), zeilen.join("\n"))
}

/// Den Zustand eines Laufs als ein Wort.
fn lauf_zustand(lauf: &Value) -> String {
    let status = daten::text_oder(lauf, "status", "?");
    match daten::text(lauf, "result") {
        Some(ergebnis) => format!("{status}/{ergebnis}"),
        None => status,
    }
}

/// `devops_laeufe`.
pub fn laeufe(baum: &Value) -> String {
    let eintraege = daten::liste(baum, "value");
    if eintraege.is_empty() {
        return "Keine Läufe gefunden.".to_string();
    }
    let zeilen: Vec<String> = eintraege
        .iter()
        .map(|lauf| {
            let mut zeile = format!(
                "- #{} {} · {}",
                daten::text_oder(lauf, "id", "?"),
                daten::text_oder(lauf, "definition.name", "(ohne Pipeline)"),
                lauf_zustand(lauf)
            );
            if let Some(branch) = daten::text(lauf, "sourceBranch") {
                zeile.push_str(&format!(" · {}", branch_kurz(&branch)));
            }
            let start = daten::text(lauf, "startTime").or_else(|| daten::text(lauf, "queueTime"));
            if let Some(start) = daten::datum(start) {
                zeile.push_str(&format!(" · {start}"));
            }
            zeile
        })
        .collect();
    format!("{} Lauf/Läufe:\n{}", eintraege.len(), zeilen.join("\n"))
}

/// `devops_lauf`: Details und die fehlgeschlagenen Schritte der Zeitachse.
pub fn lauf(baum: &Value, zeitachse: Option<&Value>) -> String {
    let mut text = format!(
        "#{} {} · {}",
        daten::text_oder(baum, "id", "?"),
        daten::text_oder(baum, "definition.name", "(ohne Pipeline)"),
        lauf_zustand(baum)
    );
    let mut kopf = Vec::new();
    if let Some(nummer) = daten::text(baum, "buildNumber") {
        kopf.push(format!("- Nummer: {nummer}"));
    }
    if let Some(branch) = daten::text(baum, "sourceBranch") {
        kopf.push(format!("- Branch: {}", branch_kurz(&branch)));
    }
    if let Some(anfrage) = daten::text(baum, "requestedFor.displayName") {
        kopf.push(format!("- Angefordert von: {anfrage}"));
    }
    kopf.push(format!(
        "- Gestartet: {} · Beendet: {}",
        datum(baum, "startTime"),
        datum(baum, "finishTime")
    ));
    if let Some(url) = daten::text(baum, "_links.web.href") {
        kopf.push(format!("- URL: {url}"));
    }
    text.push('\n');
    text.push_str(&kopf.join("\n"));

    let Some(zeitachse) = zeitachse else {
        return text;
    };
    let fehler: Vec<&Value> = daten::liste(zeitachse, "records")
        .iter()
        .filter(|eintrag| {
            let ergebnis = daten::text(eintrag, "result");
            let anzahl = daten::zahl(eintrag, "errorCount").unwrap_or(0);
            ergebnis.as_deref() == Some("failed") || anzahl > 0
        })
        .collect();
    if fehler.is_empty() {
        text.push_str("\n\nKeine fehlgeschlagenen Schritte in der Zeitachse.");
        return text;
    }
    text.push_str(&format!(
        "\n\n## Fehlgeschlagene Schritte ({})",
        fehler.len()
    ));
    for eintrag in fehler {
        let mut zeile = format!(
            "\n- {} {} · {}",
            daten::text_oder(eintrag, "type", "?"),
            daten::text_oder(eintrag, "name", "?"),
            daten::text_oder(eintrag, "result", "?")
        );
        if let Some(log) = daten::text(eintrag, "log.id") {
            zeile.push_str(&format!(" · Log {log}"));
        }
        text.push_str(&zeile);
    }
    text
}

/// `devops_lauf_log`: das Ende des Logs.
pub fn lauf_log(run: u64, log: u64, text: &str, zeilen: usize) -> String {
    let (ende, gesamt, weggelassen) = ende(text, zeilen);
    if ende.is_empty() {
        return format!("Log {log} von Lauf #{run} ist leer.");
    }
    if weggelassen == 0 {
        format!("Log {log} von Lauf #{run} ({gesamt} Zeilen):\n{ende}")
    } else {
        format!(
            "Log {log} von Lauf #{run} (letzte {zeilen} von {gesamt} Zeilen, {weggelassen} davor \
             weggelassen):\n{ende}"
        )
    }
}

// ------------------------------------------------------------- Schreiben

/// Der beste Wert für einen Namen: der Titel, sonst die ID.
fn item_kopf(item: &Value) -> String {
    format!(
        "#{} [{}] [{}] {}",
        daten::text_oder(item, "id", "?"),
        daten::text_oder(item, "fields.System.WorkItemType", "?"),
        daten::text_oder(item, "fields.System.State", "?"),
        daten::text_oder(item, "fields.System.Title", "(ohne Titel)")
    )
}

/// `devops_workitem_anlegen`.
pub fn item_angelegt(item: &Value) -> String {
    let mut text = format!("Arbeitsitem angelegt: {}", item_kopf(item));
    if let Some(url) = daten::text(item, "_links.html.href") {
        text.push_str(&format!("\n{url}"));
    }
    text
}

/// `devops_workitem_aendern`.
pub fn item_geaendert(id: u64, felder: &[String]) -> String {
    format!("Arbeitsitem #{id} geändert:\n- {}", felder.join("\n- "))
}

/// `devops_workitem_kommentieren`.
pub fn kommentar_angelegt(id: u64, baum: &Value) -> String {
    match daten::text(baum, "id") {
        Some(kommentar) => format!("Kommentar {kommentar} zu Arbeitsitem #{id} angelegt."),
        None => format!("Kommentar zu Arbeitsitem #{id} angelegt."),
    }
}

/// `devops_workitem_verknuepfen`.
pub fn verknuepft(id: u64, art: &str, ziel: u64, entfernt: bool) -> String {
    if entfernt {
        format!("Link „{art}“ zwischen #{id} und #{ziel} entfernt.")
    } else {
        format!("Link „{art}“ von #{id} zu #{ziel} hinzugefügt.")
    }
}

/// `devops_pull_request_anlegen`.
pub fn pull_request_angelegt(baum: &Value) -> String {
    let mut text = format!(
        "Pull Request #{} angelegt: {} ({} → {})",
        daten::text_oder(baum, "pullRequestId", "?"),
        daten::text_oder(baum, "title", "(ohne Titel)"),
        branch_kurz(&daten::text_oder(baum, "sourceRefName", "?")),
        branch_kurz(&daten::text_oder(baum, "targetRefName", "?"))
    );
    if daten::wahr(baum, "isDraft") {
        text.push_str(" · Entwurf");
    }
    if let Some(url) = daten::text(baum, "_links.web.href") {
        text.push_str(&format!("\n{url}"));
    }
    text
}

/// `devops_pull_request_kommentieren`.
pub fn pr_kommentiert(pull_request: u64, thread: Option<String>, antwort: bool) -> String {
    match (thread, antwort) {
        (Some(thread), true) => {
            format!("Antwort im Thread {thread} an PR #{pull_request} angelegt.")
        }
        (Some(thread), false) => {
            format!("Neuer Thread {thread} an PR #{pull_request} angelegt.")
        }
        (None, _) => format!("Kommentar an PR #{pull_request} angelegt."),
    }
}

/// `devops_pull_request_abstimmen`.
pub fn abgestimmt(pull_request: u64, stimme: &str) -> String {
    format!("Stimme für PR #{pull_request} gesetzt: {stimme}.")
}

/// `devops_pipeline_starten`.
pub fn lauf_gestartet(baum: &Value) -> String {
    let mut text = format!(
        "Pipeline-Lauf #{} gestartet: {} · {}",
        daten::text_oder(baum, "id", "?"),
        daten::text_oder(baum, "definition.name", "(ohne Pipeline)"),
        lauf_zustand(baum)
    );
    if let Some(url) = daten::text(baum, "_links.web.href") {
        text.push_str(&format!("\n{url}"));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::organisationen::Organisation;
    use serde_json::json;

    fn organisation() -> Organisation {
        Organisation {
            name: "neoimpulse".into(),
            url: "https://dev.azure.com/neoimpulse".into(),
            projekt: Some("SCODA".into()),
            pat: "pat-4711".into(),
            schreiben: false,
        }
    }

    #[test]
    fn die_hinweise_nennen_das_quiver_fenster() {
        assert_eq!(
            pat_ungueltig("neoimpulse"),
            "PAT für neoimpulse ungültig oder abgelaufen – im quiver-Fenster unter Tools → DevOps \
             erneuern."
        );
        assert_eq!(
            schreiben_aus("kunde"),
            "Schreiben ist für kunde aus – im quiver-Fenster einschalten."
        );
        assert!(projekt_fehlt("kunde").contains("quiver-Fenster"));
        assert!(team_fehlt().contains("team"));
        assert!(keine_organisation().contains("Keine"));
    }

    #[test]
    fn der_status_nennt_die_organisation_ohne_pat() {
        let text = status(&[organisation()]);
        assert!(text.contains("1 Organisation(en)"), "{text}");
        assert!(text.contains("neoimpulse"), "{text}");
        assert!(text.contains("Projekt SCODA"), "{text}");
        assert!(text.contains("Schreiben aus"), "{text}");
        assert!(!text.contains("pat-4711"), "{text}");
        assert_eq!(status(&[]), "Keine Azure-DevOps-Organisation eingerichtet.");
    }

    #[test]
    fn die_beschreibung_kommt_aus_dem_html_und_wird_gekuerzt() {
        assert_eq!(
            beschreibung("<div>Erster</div><div>Zweiter</div>", 4000),
            "Erster\nZweiter"
        );
        let lang = format!("<p>{}</p>", "wort ".repeat(100));
        let kurz = beschreibung(&lang, 50);
        assert!(kurz.contains("Zeichen weggelassen"), "{kurz}");
        assert!(kurz.lines().next().unwrap().chars().count() <= 50);
    }

    #[test]
    fn das_ende_eines_logs_wird_genommen() {
        let text = "1\n2\n3\n4\n5";
        let (ende, gesamt, weggelassen) = ende(text, 2);
        assert_eq!(ende, "4\n5");
        assert_eq!(gesamt, 5);
        assert_eq!(weggelassen, 3);

        let text = lauf_log(12, 3, "a\nb\nc", 200);
        assert!(text.contains("Log 3 von Lauf #12 (3 Zeilen)"), "{text}");
        assert!(text.contains("a\nb\nc"), "{text}");

        let text = lauf_log(12, 3, "a\nb\nc", 1);
        assert!(
            text.contains("letzte 1 von 3 Zeilen, 2 davor weggelassen"),
            "{text}"
        );
        assert!(lauf_log(1, 1, "", 10).contains("ist leer"));
    }

    #[test]
    fn ein_arbeitsitem_hat_beschreibung_kriterien_und_links() {
        let item = json!({
            "id": 123,
            "rev": 4,
            "fields": {
                "System.WorkItemType": "User Story",
                "System.State": "Active",
                "System.Title": "Login kaputt",
                "System.AssignedTo": { "displayName": "Patrick" },
                "System.AreaPath": "SCODA\\Web",
                "System.IterationPath": "SCODA\\Sprint 1",
                "System.ChangedDate": "2026-11-20T10:15:00Z",
                "System.Description": "<div>Klicken geht nicht</div>",
                "Microsoft.VSTS.Common.AcceptanceCriteria": "<ul><li>Login klappt</li></ul>",
                "System.Tags": "bug; login"
            },
            "_links": { "html": { "href": "https://dev.azure.com/x/_workitems/edit/123" } },
            "relations": [
                { "rel": "System.LinkTypes.Hierarchy-Reverse", "url": "https://x/_apis/wit/workItems/100" },
                { "rel": "System.LinkTypes.Related", "url": "https://x/_apis/wit/workItems/101" }
            ]
        });
        let text = arbeitsitem(&item, 4000);
        assert!(
            text.starts_with("#123 [User Story] [Active] Login kaputt"),
            "{text}"
        );
        assert!(text.contains("- Zugewiesen: Patrick"), "{text}");
        assert!(text.contains("- Bereich: SCODA\\Web"), "{text}");
        assert!(
            text.contains("## Beschreibung\nKlicken geht nicht"),
            "{text}"
        );
        assert!(
            text.contains("## Akzeptanzkriterien\n- Login klappt"),
            "{text}"
        );
        assert!(text.contains("- Eltern #100"), "{text}");
        assert!(text.contains("- verwandt #101"), "{text}");
    }

    #[test]
    fn listen_zaehlen_und_nennen_den_zustand() {
        let items = json!({ "count": 1, "value": [ {
            "id": 7,
            "fields": {
                "System.WorkItemType": "Bug",
                "System.State": "New",
                "System.Title": "Absturz",
                "System.AssignedTo": { "displayName": "Anna" }
            }
        } ] });
        let text = arbeitsitems("Suche nach Bugs", &items);
        assert!(
            text.starts_with("Suche nach Bugs\n1 Arbeitsitem(s):"),
            "{text}"
        );
        assert!(
            text.contains("#7 [Bug] [New] Absturz — zugewiesen: Anna"),
            "{text}"
        );
        assert!(arbeitsitems("leer", &json!({ "value": [] })).contains("Keine Arbeitsitems"));

        let typen = json!({ "value": [ { "name": "Bug", "referenceName": "Microsoft.Bug",
            "states": [ { "name": "New" }, { "name": "Active" } ] } ] });
        let text = arbeitsitem_typen(&typen);
        assert!(
            text.contains("- Bug (Microsoft.Bug) · Status: New, Active"),
            "{text}"
        );

        let iteration_baum = json!({ "value": [ {
            "id": "guid-1",
            "path": "SCODA\\Sprint 1",
            "attributes": { "startDate": "2026-11-01T00:00:00Z", "finishDate": "2026-11-14T00:00:00Z",
                            "timeFrame": "current" }
        } ] });
        let text = super::iterationen(&iteration_baum);
        assert!(
            text.contains("SCODA\\Sprint 1 · 2026-11-01 – 2026-11-14 · current"),
            "{text}"
        );
    }

    #[test]
    fn pull_requests_und_ihre_threads_werden_gelesen() {
        let liste = json!({ "value": [ {
            "pullRequestId": 42, "title": "Login", "status": "active", "isDraft": true,
            "createdBy": { "displayName": "Patrick" },
            "sourceRefName": "refs/heads/feature/login", "targetRefName": "refs/heads/main",
            "repository": { "name": "scoda-web" }
        } ] });
        let text = pull_requests(&liste);
        assert!(
            text.contains(
                "#42 [active] Login · scoda-web · feature/login → main · Patrick · Entwurf"
            ),
            "{text}"
        );

        let pr = json!({
            "pullRequestId": 42, "title": "Login", "status": "active",
            "createdBy": { "displayName": "Patrick" },
            "creationDate": "2026-11-20T09:00:00Z",
            "sourceRefName": "refs/heads/feature/login", "targetRefName": "refs/heads/main",
            "repository": { "name": "scoda-web" }, "mergeStatus": "succeeded",
            "description": "<div>Bitte prüfen</div>",
            "reviewers": [ { "displayName": "Anna", "vote": 10 }, { "displayName": "Ben", "vote": -5 } ],
            "_links": { "web": { "href": "https://dev.azure.com/x/pr/42" } }
        });
        let commits = json!({ "value": [ { "commitId": "abcdef1234567890", "comment": "Erster Commit",
            "author": { "name": "Patrick", "date": "2026-11-20T09:30:00Z" } } ] });
        let items = json!({ "value": [ { "id": "123" } ] });
        let text = pull_request(&pr, &commits, &items, 4000);
        assert!(text.starts_with("#42 [active] Login"), "{text}");
        assert!(text.contains("feature/login → main"), "{text}");
        assert!(text.contains("## Beschreibung\nBitte prüfen"), "{text}");
        assert!(text.contains("- Anna · abgestimmt"), "{text}");
        assert!(text.contains("- Ben · wartet auf Autor"), "{text}");
        assert!(text.contains("- #123"), "{text}");
        assert!(
            text.contains("abcdef12 Patrick 2026-11-20 09:30: Erster Commit"),
            "{text}"
        );

        let thread_liste = json!({ "value": [
            { "id": 1, "status": "active", "comments": [ { "content": "<div>Bitte ändern</div>",
              "author": { "displayName": "Anna" }, "publishedDate": "2026-11-20T10:00:00Z" } ] },
            { "id": 2, "status": "closed", "properties": { "CodeReviewThreadType": { "$value": "VoteUpdate" } } }
        ] });
        let text = threads(42, &thread_liste, 4000);
        assert!(text.contains("1 Kommentar-Thread(s)"), "{text}");
        assert!(text.contains("**Thread 1** (active)"), "{text}");
        assert!(text.contains("Bitte ändern"), "{text}");
        assert!(!text.contains("Thread 2"), "{text}");
    }

    #[test]
    fn laeufe_und_zeitachse_werden_gelesen() {
        let laeufe_baum = json!({ "value": [ {
            "id": 1234, "buildNumber": "20261120.1", "status": "completed", "result": "failed",
            "sourceBranch": "refs/heads/main", "startTime": "2026-11-20T10:00:00Z",
            "definition": { "id": 7, "name": "scoda-ci" }
        } ] });
        let text = super::laeufe(&laeufe_baum);
        assert!(
            text.contains("#1234 scoda-ci · completed/failed · main · 2026-11-20 10:00"),
            "{text}"
        );

        let lauf_baum = json!({ "id": 1234, "status": "completed", "result": "failed",
            "buildNumber": "2026.1", "sourceBranch": "refs/heads/main",
            "requestedFor": { "displayName": "Patrick" },
            "startTime": "2026-11-20T10:00:00Z", "finishTime": "2026-11-20T10:05:00Z",
            "definition": { "name": "scoda-ci" } });
        let zeitachse = json!({ "records": [
            { "type": "Stage", "name": "Build", "result": "failed", "errorCount": 1 },
            { "type": "Job", "name": "Compile", "result": "succeeded", "errorCount": 0 },
            { "type": "Task", "name": "npm test", "result": "failed", "errorCount": 2,
              "log": { "id": 5 } }
        ] });
        let text = lauf(&lauf_baum, Some(&zeitachse));
        assert!(
            text.starts_with("#1234 scoda-ci · completed/failed"),
            "{text}"
        );
        assert!(text.contains("## Fehlgeschlagene Schritte (2)"), "{text}");
        assert!(text.contains("- Stage Build · failed"), "{text}");
        assert!(text.contains("- Task npm test · failed · Log 5"), "{text}");
        assert!(!text.contains("Compile"), "{text}");

        let text = lauf(&lauf_baum, None);
        assert!(!text.contains("Zeitachse"), "{text}");
    }

    #[test]
    fn repos_branches_pipelines_werden_gelesen() {
        let repo_baum = json!({ "value": [ { "name": "scoda-web", "defaultBranch": "refs/heads/main",
                                            "size": 4096 } ] });
        assert!(repos(&repo_baum).contains("- scoda-web · Standardbranch refs/heads/main · 4 kB"));

        let branch_baum = json!({ "value": [ { "name": "refs/heads/main",
            "objectId": "abcdef1234567890", "creator": { "displayName": "Patrick" } } ] });
        assert!(
            branches("scoda-web", &branch_baum).contains("- refs/heads/main · abcdef12 · Patrick")
        );

        let pipeline_baum = json!({ "value": [ { "id": 7, "name": "scoda-ci", "folder": "\\" } ] });
        assert!(pipelines(&pipeline_baum).contains("- #7 scoda-ci · Ordner \\"));
    }

    #[test]
    fn kommentare_und_abstimmungen_werden_benannt() {
        let kommentar_baum = json!({ "comments": [ { "id": 1, "text": "<div>Sieht gut aus</div>",
            "createdBy": { "displayName": "Anna" }, "createdDate": "2026-11-20T10:00:00Z" } ] });
        let text = super::kommentare(123, &kommentar_baum, 4000);
        assert!(text.contains("1 Kommentar(e) zu #123"), "{text}");
        assert!(text.contains("**Anna** 2026-11-20 10:00"), "{text}");
        assert!(text.contains("Sieht gut aus"), "{text}");

        assert_eq!(stimme_anzeige(10), "abgestimmt");
        assert_eq!(stimme_anzeige(-10), "abgelehnt");
        assert_eq!(stimme_anzeige(0), "ohne Stimme");
        assert_eq!(
            abgestimmt(42, "abgestimmt"),
            "Stimme für PR #42 gesetzt: abgestimmt."
        );
        assert!(pr_kommentiert(42, Some("7".into()), true).contains("Antwort im Thread 7"));
        assert!(pr_kommentiert(42, Some("7".into()), false).contains("Neuer Thread 7"));
        assert!(pr_kommentiert(42, None, false).contains("Kommentar an PR #42"));
    }

    #[test]
    fn die_schreibantworten_sind_knapp() {
        let item = json!({ "id": 124, "fields": { "System.WorkItemType": "Bug",
            "System.State": "New", "System.Title": "Neu" },
            "_links": { "html": { "href": "https://x/124" } } });
        let text = item_angelegt(&item);
        assert!(
            text.contains("Arbeitsitem angelegt: #124 [Bug] [New] Neu"),
            "{text}"
        );
        assert!(text.contains("https://x/124"), "{text}");
        assert_eq!(
            item_geaendert(123, &["System.State → Closed".to_string()]),
            "Arbeitsitem #123 geändert:\n- System.State → Closed"
        );
        assert_eq!(
            kommentar_angelegt(123, &json!({ "id": 9 })),
            "Kommentar 9 zu Arbeitsitem #123 angelegt."
        );
        assert_eq!(
            verknuepft(123, "Eltern", 100, false),
            "Link „Eltern“ von #123 zu #100 hinzugefügt."
        );
        assert!(verknuepft(123, "Eltern", 100, true).contains("entfernt"));
        assert_eq!(
            lauf_gestartet(&json!({ "id": 5, "status": "inProgress",
                "definition": { "name": "ci" } })),
            "Pipeline-Lauf #5 gestartet: ci · inProgress"
        );
    }

    #[test]
    fn ein_pull_request_wird_angelegt_gemeldet() {
        let pr = json!({ "pullRequestId": 43, "title": "Neu",
            "sourceRefName": "refs/heads/feature", "targetRefName": "refs/heads/main",
            "isDraft": true });
        let text = pull_request_angelegt(&pr);
        assert!(
            text.contains("Pull Request #43 angelegt: Neu (feature → main) · Entwurf"),
            "{text}"
        );
    }

    #[test]
    fn die_organisation_und_der_test_nennen_keinen_pat() {
        let text = testen(
            &organisation(),
            &json!({ "authenticatedUser": { "displayName": "Patrick",
                "properties": { "Account": { "$value": "neoimpulse" } } } }),
        );
        assert!(text.contains("Patrick (Konto neoimpulse)"), "{text}");
        assert!(text.contains("Schreiben: aus"), "{text}");
        assert!(!text.contains("pat-4711"), "{text}");

        let projekt_baum = json!({ "value": [ { "name": "SCODA", "state": "wellFormed",
            "visibility": "private" } ] });
        assert!(projekte_text(&projekt_baum).contains("- SCODA (wellFormed, private)"));
    }

    /// `projekte` heißt wie die gleichnamige Funktion in `daten`; hier ein
    /// eindeutiger Name für den Test.
    fn projekte_text(baum: &Value) -> String {
        super::projekte("neoimpulse", baum)
    }
}
