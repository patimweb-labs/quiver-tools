# 09 – quiver-devops (Azure DevOps mit PAT, mehrere Organisationen)

Neues Crate `crates/devops`, Programm `quiver-devops`. Vorlage zum Nachlesen, nicht zum
Übersetzen: `~/Documents/Workspaces/pi-azure-devops` (`src/tools/*.ts`, `src/safety/`,
`src/auth/pat.ts`, `README.md`). Die REST-API ist **Azure DevOps 7.1**
(`https://learn.microsoft.com/rest/api/azure/devops/`). Die Aufrufe baust du in Rust mit
`reqwest` neu, ein SDK gibt es nicht.

## Zugänge: Liste im quiver-Fenster (`Werkzeug::mehrfach`)

`mehrfach = ("Organisationen", "Organisation")`, `QUIVER_ZUGAENGE`, wie quiver-ssh und
quiver-abap. Ein Zugang ist eine Organisation, eigene oder eines Kunden, optional mit festem
Projekt. Felder in dieser Reihenfolge:

| Feld | Art | Hilfe |
|---|---|---|
| `url` | Url, Pflicht | z. B. `https://dev.azure.com/neoimpulse` oder ein Azure DevOps Server `https://tfs.kunde.de/tfs/DefaultCollection` |
| `projekt` | Text | Standardprojekt; leer heißt, das Projekt wird je Aufruf angegeben |
| `pat` | Passwort, Pflicht | Personal Access Token, erzeugt in DevOps unter Benutzereinstellungen → Personal access tokens. Rechte nach Bedarf (Work Items, Code, Build: lesen bzw. schreiben). |
| `schreiben` | Schalter, **Standard aus** | Erlaubt Anlegen und Ändern (Work Items, Pull Requests, Kommentare, Pipelines starten) für diese Organisation |

Hilfe-Link am Feld `pat`: `{url}/_usersSettings/tokens` (wie der Outline-Link, `Feld.link`).

## Grundregeln

- Jedes Tool nimmt `verbindung` (Name des Zugangs), bei genau einem Zugang optional, dazu
  `projekt`, das sonst aus dem Zugang kommt. Fehlt beides, gibt es einen Hinweistext.
- Auth: `Authorization: Basic base64(":" + PAT)`. Der PAT erscheint **nie** in Antwort, Fehler
  oder Log. `Debug` vom Zugang zeigt ihn nicht.
- `api-version=7.1` an jede Anfrage. JSON-Patch mit `Content-Type:
  application/json-patch+json`.
- **401, oder 203 mit HTML** (DevOps leitet bei ungültigem PAT zur Anmeldung um) → „PAT für
  <verbindung> ungültig oder abgelaufen – im quiver-Fenster unter Tools → DevOps erneuern.“
  Kein Wiederholen.
- **Schreiben aus** → jedes verändernde Tool antwortet **ohne Anfrage**: „Schreiben ist für
  <verbindung> aus – im quiver-Fenster einschalten.“
- 429: `Retry-After` beachten, höchstens 2 Wiederholungen, **nur bei lesenden** Aufrufen.
- Antworten knapp. Beschreibungen und Kommentare kommen als Text aus dem HTML, gekürzt mit
  `max_zeichen` (Standard 4000).

## Tools

Lesend (`read_only_hint`):

| Tool | API |
|---|---|
| `devops_status` | Zugänge ohne PAT: Name, URL, Projekt, schreiben ja/nein |
| `devops_testen` | `GET _apis/connectionData` → wer ist angemeldet, Organisation |
| `devops_projekte` | `GET _apis/projects` |
| `devops_workitems_suchen` | WIQL (`POST {projekt}/_apis/wit/wiql`, `top` Standard 50, max 200), danach Felder per `workitemsbatch`. Dazu der Komfort `text`/`zugewiesen_an_mich`/`status`/`typ`, aus dem WIQL gebaut wird, wenn `wiql` fehlt |
| `devops_workitem` | `GET _apis/wit/workitems/{id}?$expand=relations` mit Beschreibung, Akzeptanzkriterien und Links |
| `devops_workitem_kommentare` | `GET {projekt}/_apis/wit/workItems/{id}/comments` |
| `devops_workitem_typen` | `GET {projekt}/_apis/wit/workitemtypes` (Namen, Status) |
| `devops_iterationen` | `GET {projekt}/{team}/_apis/work/teamsettings/iterations`, optional `aktuell` |
| `devops_iteration_workitems` | `GET …/iterations/{id}/workitems` plus Felder |
| `devops_repos` | `GET {projekt}/_apis/git/repositories` |
| `devops_branches` | `GET …/repositories/{repo}/refs?filter=heads/` |
| `devops_pull_requests` | `GET {projekt}/_apis/git/pullrequests?searchCriteria.status=` (Standard active), optional Repo und Ersteller |
| `devops_pull_request` | Details, Reviewer, verknüpfte Work Items, Commits (höchstens 20) |
| `devops_pull_request_threads` | Kommentar-Threads, ohne System-Threads |
| `devops_pipelines` | `GET {projekt}/_apis/pipelines` |
| `devops_laeufe` | `GET {projekt}/_apis/build/builds?$top=` (Standard 10), Filter Pipeline und Branch |
| `devops_lauf` | Details und Timeline (fehlgeschlagene Schritte) |
| `devops_lauf_log` | Log eines Schritts, **das Ende** (Standard 200 Zeilen) |

Verändernd (`destructive_hint`, brauchen `schreiben`):

| Tool | API |
|---|---|
| `devops_workitem_anlegen` | `POST {projekt}/_apis/wit/workitems/${typ}`: Titel, Beschreibung, zugewiesen, Bereich, Iteration, Eltern |
| `devops_workitem_aendern` | `PATCH _apis/wit/workitems/{id}`: Felder als Liste `{feld, wert}` plus Komfort Status, Titel, zugewiesen |
| `devops_workitem_kommentieren` | `POST …/comments` |
| `devops_workitem_verknuepfen` | Links hinzufügen oder entfernen (Eltern/Kind/verwandt) |
| `devops_pull_request_anlegen` | Quelle, Ziel, Titel, Beschreibung, Work Items, Entwurf |
| `devops_pull_request_kommentieren` | neuer Thread oder Antwort auf einen Thread |
| `devops_pull_request_abstimmen` | Vote (approve, approve-with-suggestions, wait, reject, reset) |
| `devops_pipeline_starten` | `POST {projekt}/_apis/pipelines/{id}/runs` mit Branch und Variablen |

Nicht übernommen, sondern in `docs/offen.md`: Testpläne, Kapazität, Board-Spalten, Policies,
Autocomplete, Läufe abbrechen und wiederholen, Artefakte.

## Tests (nur Attrappe, nie ein echtes DevOps)

Eine axum-Attrappe einer Organisation mit Basic-Auth-Prüfung, zwei Zugängen (zwei
Organisationen, eine davon mit festem Projekt). Fälle:

- je Tool ein Erfolg
- Zugang wählen: einer → optional, zwei → Pflicht, unbekannt → Hinweis
- Projekt aus dem Zugang oder je Aufruf
- PAT ungültig (401) und 203 mit HTML → Hinweis, kein zweiter Versuch
- `schreiben` aus → 0 Anfragen bei allen 8 verändernden Tools
- JSON-Patch mit richtigem Content-Type
- 429 beim Lesen wird wiederholt, beim Anlegen nicht
- WIQL aus den Komfort-Parametern
- PAT in keiner Ausgabe
- `quiver-devops eintrag` mit `mehrfach` und den vier Feldern, Link am PAT
- Ende-zu-Ende über stdio: alle Tools mit Annotationen, jedes Schema mit `"type": "object"`

## Doku

- README-Abschnitt quiver-devops
- `docs/architektur.md`
- `docs/adr/0007-devops-mit-pat.md`: warum PAT statt Entra, nämlich Kundenorganisationen in
  fremden Tenants und Azure DevOps Server; warum `schreiben` mit Standard aus
- Workspace-Mitglied in `Cargo.toml`, Prüfbefehl grün:
  `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`

**Nicht committen**, das macht Claude nach der Abnahme.
