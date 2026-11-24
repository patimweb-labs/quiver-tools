# 09 – Leere Antworten richtig auswerten, Löschen für alles, was m365 anlegt

Befunde aus dem Live-Test (2026-11-24, Runde 3).

## 1. Falscher Fehler nach erfolgreichem Schreiben (wichtig)

`planner_aufgabe_aendern` schreibt korrekt, meldet danach aber
`invalid type: null, expected struct PlannerAufgabe`. Der Grund: Graph antwortet auf den PATCH
mit **204 und leerem Rumpf**. `auswerten()` (m365-graph `client.rs` ~463) macht daraus `Null`,
und `patch_mit_kopf` will trotzdem eine `PlannerAufgabe` lesen. Für einen Agenten ist das
gefährlich: Es sieht aus wie ein Fehlschlag, obwohl die Änderung ausgeführt wurde.

- Der Graph-Client unterscheidet „Antwort mit Inhalt“ und „ohne Inhalt“, z. B. mit
  `patch_ohne_antwort` bzw. `Option<T>`.
- Bei 204 oder leerem Rumpf ist der Aufruf **erfolgreich**. Das Tool liest den Stand danach
  einmal per GET und meldet ihn, oder es meldet schlicht „geändert“.
- Prüfe **alle** PATCH-, POST- und DELETE-Aufrufe in m365 auf dasselbe Muster: Planner, To Do,
  SharePoint-Listeneintrag, Kontakte, Teams setReaction und markChatRead.
- Die Attrappe antwortet wie Graph mit 204 und leerem Rumpf. Je Fall gibt es einen Test, der mit
  dem alten Code rot wäre.

## 2. 403 bei Listeneinträgen

`sharepoint_listeneintrag_aendern` ergab 403 `accessDenied` auf einer HR-Liste. Das sind
vermutlich echte Rechte der Liste und kein Fehler im Code. Prüfe trotzdem:
- Wir patchen `…/items/{id}/fields` (richtig) und nicht `…/items/{id}`.
- Schreibgeschützte Felder wie `LinkTitle`, `ID`, `Created`, `Modified`, `Author`, `Editor` und
  `_…` werden vor dem Senden **herausgefiltert**, mit Hinweis.
- Bei 403 gibt es einen klaren Hinweis: „Kein Schreibrecht auf diese Liste (SharePoint-Rechte
  der Liste).“

## 3. Löschen

Für alles, was m365 anlegen kann, kommt ein Löschen-Tool dazu. Alle sind **destruktiv**
(`destructive_hint`), nehmen eine Id, werden nie wiederholt und melden genau, was gelöscht wurde
(Titel bzw. Name).

| Tool | Graph |
|---|---|
| `termin_loeschen` | `DELETE /me/events/{id}` |
| `mail_loeschen` | `POST /me/messages/{id}/move` nach `deleteditems`, also in den Papierkorb und **nicht** endgültig |
| `datei_loeschen` | `DELETE /drives/{d}/items/{id}`, Papierkorb von OneDrive/SharePoint, gilt auch für Ordner |
| `planner_aufgabe_loeschen` | `DELETE /planner/tasks/{id}` mit **If-Match**-ETag |
| `todo_aufgabe_loeschen` | `DELETE /me/todo/lists/{l}/tasks/{t}` |
| `onenote_seite_loeschen` | `DELETE /me/onenote/pages/{id}` |
| `kontakt_loeschen` | `DELETE /me/contacts/{id}` |
| `sharepoint_listeneintrag_loeschen` | `DELETE /sites/{s}/lists/{l}/items/{id}` |
| `teams_nachricht_loeschen` | `POST …/messages/{id}/softDelete`, nur eigene Nachrichten, wiederherstellbar |

Dazu `docs/m365/README.md` und `docs/m365/offen.md` ergänzen. Löschen steht dann nicht mehr als
offen in der Liste.

## Tests

- 204 mit leerem Rumpf bei allen Änderungen ist ein Erfolg.
- Je Löschen-Tool ein Erfolg.
- Planner-Löschen ohne ETag führt zu keinem DELETE.
- Schreibgeschützte Felder werden gefiltert.
- 403 ergibt den Hinweis.
- **Regression:** Alle bisherigen 50 Tools stehen unverändert in `tools/list`.

**Prüfbefehl grün. Nicht committen.**
