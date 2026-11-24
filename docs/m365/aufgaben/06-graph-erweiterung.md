# 06 – quiver-m365: SharePoint/OneDrive, Planner/To Do, Kontakte/Personen, OneNote

Crates `crates/m365*` im Repo quiver-tools. Grundsatz: **m365 = alles, was über Microsoft Graph
läuft.**

**Was es schon gibt, bleibt unverändert:**
- 8 Tools für Mail, Kalender, `dateien_suchen`, `profil` und `abmelden`
- 16 Teams-Tools
- Anmeldung und Tresor-Datei

Keine Umbenennung, kein Löschen. Neue Tools kommen **dazu**, in eigenen Modulen nach dem Muster
von `teams.rs`/`teams_tools.rs`.

## Scopes

`STANDARD_SCOPES` ergänzen: `Sites.ReadWrite.All`, `Files.ReadWrite.All`, `Tasks.ReadWrite`,
`Group.Read.All`, `Contacts.ReadWrite`, `People.Read`, `Notes.ReadWrite.All`. In der Entra-App
sind sie eingetragen, und die Admin-Zustimmung ist erteilt.

`docs/m365/entra-app.md` bekommt dazu eine Zeile mit Datum 2026-11-24.

Fehlende Zustimmung wird behandelt wie bei Teams: ein Hinweis, das Refresh-Token bleibt, und es
öffnet sich kein Browser.

## Tools

Lesend heißt `read_only_hint`. Alles, was etwas anlegt, ändert oder löscht, ist **destruktiv**.

**SharePoint / OneDrive** (`/sites`, `/drives`, `/me/drive`):
- `sharepoint_sites`: Suche `?search=`.
- `sharepoint_bibliotheken`: Drives einer Site.
- `dateien_liste`: Ordnerinhalt eines Drives oder von OneDrive, mit Pfad oder Id.
- `datei_lesen`: Inhalt als Text.
  - Office-Dateien und PDF über `?format=pdf` bzw. den Textinhalt, wo es geht. Text, Markdown, CSV und JSON direkt.
  - Begrenzt mit `max_zeichen` (Standard 20 000), dazu ein Kürzungshinweis.
  - Binärdateien liefern nur Metadaten.
- `datei_hochladen` (destruktiv): Text-Inhalt bis 4 MB, einfacher PUT, Konflikt `rename` als Standard.
- `ordner_anlegen` (destruktiv).
- `sharepoint_listen`: Listen einer Site.
- `sharepoint_listeneintraege`: `$expand=fields`, `top`, Filter.
- `sharepoint_listeneintrag_anlegen` und `sharepoint_listeneintrag_aendern` (beide destruktiv).

**Planner / To Do:**
- `planner_plaene`: über `/me/planner/plans`, zusätzlich je Gruppe.
- `planner_aufgaben`: je Plan oder `/me/planner/tasks`, mit Buckets.
- `planner_aufgabe_anlegen` (destruktiv).
- `planner_aufgabe_aendern` (destruktiv): Fortschritt, Fälligkeit, zugewiesen. Mit **ETag/If-Match**, wie Planner es verlangt.
- `todo_listen`, `todo_aufgaben`.
- `todo_aufgabe_anlegen` und `todo_aufgabe_erledigen` (beide destruktiv).

**Kontakte / Personen:**
- `kontakte_suchen` (`/me/contacts`), `kontakt_anlegen` (destruktiv).
- `personen_suchen` (`/me/people?$search=`): Kollegen mit Abteilung, Telefon und Mail.

**OneNote:**
- `onenote_notizbuecher`, `onenote_abschnitte`, `onenote_seiten`.
- `onenote_seite_lesen`: HTML in Text, mit `max_zeichen`.
- `onenote_seite_anlegen` (destruktiv): Titel und Text werden als HTML-escaped eingesetzt.

## Regeln (wie Teams)

- Namen werden zu Ids aufgelöst. Bei Mehrdeutigkeit gibt es Kandidaten und **keine** Aktion.
- 429 und 503: höchstens 2 Wiederholungen nach `Retry-After`, **nur bei GET**.
- Was etwas anlegt oder ändert, wird nie automatisch wiederholt.
- Zeiten im Format RFC 3339 mit Zeitzone.
- Antworten knapp.

## Tests (nur Attrappen)

Die Graph-Attrappe wird um die neuen Pfade erweitert. Je Tool gibt es einen Erfolgsfall.
Zusätzlich:
- Mehrdeutige Namen.
- Planner ohne ETag: kein PATCH. Mit ETag: der If-Match-Header ist gesetzt.
- Upload über 4 MB wird ohne Anfrage abgelehnt.
- 429 bei einem POST: kein zweiter Versuch.
- Fehlende Zustimmung.
- **Regression:** Alle bisherigen 24 Tools stehen weiter in `tools/list`, mit gleichen Namen und gleichen Annotationen.
- Jedes Eingabeschema hat `"type": "object"`.

## Doku

`docs/m365/README.md` und `docs/m365/offen.md` ergänzen.

Prüfbefehl im Repo-Root muss grün sein:
`cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`.

**Nicht committen.**
