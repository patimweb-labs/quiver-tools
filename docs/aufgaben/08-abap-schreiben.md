# 08 – quiver-abap, Teil 2: schreibende Tools

Crate: `crates/abap` (Teil 1 steht: ADT-Client, 19 lesende Tools, Systeme als Liste). Vorlage:
`~/Documents/Workspaces/pi-abap-fs/src/tools/` (`abap-lock.ts`, `abap-unlock.ts`, `abap-write.ts`,
`abap-activate.ts`, `abap-create-object.ts`, `abap-set-text-elements.ts`) und `src/client.ts`.
Das ADT-Protokoll steht im MIT-lizenzierten `abap-adt-api` unter
`~/Documents/Workspaces/pi-abap-fs/node_modules/abap-adt-api/build/` (`AdtHTTP.js`, `api/objectcontents.js`,
`api/activate.js`, `api/objectcreator.js`, `api/textelements.js`). **Dort lesen, wie die Anfragen
aussehen**, in Rust neu bauen. Nicht übersetzen.

## Neue Tools

| Tool | Annotation | Parameter (wie Vorlage) |
|---|---|---|
| `abap_lock` | nicht lesend, nicht destruktiv | `system`, `name` / `objectUrl`, `type`, `accessMode` (Standard `MODIFY`) → Lock-Handle |
| `abap_unlock` | nicht lesend, nicht destruktiv | `system`, `name` / `objectUrl`, `type`, `lockHandle` |
| `abap_write` | **destruktiv** | `system`, `name` / `objectUrl`, `type`, `source`, `transport`, `lockHandle` (optional) |
| `abap_activate` | **destruktiv** | `system`, `name` / `objectUrl`, `type`, `preauditRequested` (Standard true) |
| `abap_create_object` | **destruktiv** | `system`, `objectType` (CLAS/OC, INTF/OI, PROG/P, PROG/I, FUGR/F, TABL/DT, DTEL/DE, DOMA/DD, DDLS/DF), `name`, `parentName` (Paket), `description`, `transport` |
| `abap_set_text_elements` | **destruktiv** | `system`, `name` / `objectUrl`, `type`, `category` (symbols, selections, headings), `elements` [{id, text, maxLength?}], `transport`, `lockHandle` |

`abap_setup` gibt es **nicht**, denn Systeme pflegt der User im quiver-Fenster. `abap_profile`
liefert der Kern schon. Das Auflösen von `name`/`type` → `objectUrl`/Hauptinclude ist dasselbe wie
in `abap_read` und wird von dort wiederverwendet.

## Stateful Session (Kern von Teil 2)

- Ein Lock-Handle gilt in SAP nur in **der** Sitzung, die es geholt hat (Cookie
  `SAP_SESSIONID_*` + Kopf `X-sap-adt-sessiontype: stateful`).
- `quiver-abap` hält deshalb je System **eine stateful Sitzung** im Speicher: Cookies,
  CSRF-Token und eine Tabelle `objectUrl → lockHandle`. `abap_lock`, `abap_write`,
  `abap_unlock`, `abap_activate` und `abap_set_text_elements` laufen darüber.
- Lesende Tools bleiben stateless.
- **`abap_write` ohne `lockHandle`:** sperren, schreiben, entsperren, alles in derselben
  Sitzung. Das Entsperren läuft auch, wenn das Schreiben scheitert, wie `finally` in der Vorlage.
- **`abap_write` mit `lockHandle`:** Der Handle muss aus dieser Sitzung stammen, sonst gibt es
  eine klare Meldung „Lock-Handle unbekannt – erst abap_lock aufrufen“, ohne Anfrage an SAP. Das
  Lock bleibt danach stehen.
- Läuft die Sitzung ab, gibt es einen 400/401-Fehler mit „Session timed out“ oder ähnlich. Dann
  ist die Sitzung samt Lock-Tabelle verworfen, und die Meldung sagt „Sitzung abgelaufen, Locks
  verloren – erneut abap_lock“. **Kein** stilles Wiederholen eines Schreibvorgangs.
- Beim Beenden des Prozesses (stdin zu oder SIGTERM): alle bekannten Locks freigeben,
  höchstens 5 s, Fehler nur ins Log.
- CSRF: wie Teil 1, bei 403 `Required` einmal neu holen, **in der stateful Sitzung**.
- 401 → „Anmeldung an <system> abgelehnt“, nie wiederholen, sonst sperrt SAP den Benutzer.

## Schutz (Pflicht)

1. **Neues Feld je System `schreiben`** (Schalter, **Standard aus**), Hilfetext: „Erlaubt
   abap_write, abap_activate, abap_create_object, abap_set_text_elements und Locks auf diesem
   System. Für Produktivsysteme aus lassen.“ Ist es aus, antwortet jedes schreibende Tool
   **ohne** Anfrage an SAP: „Schreiben ist für <system> aus – im quiver-Fenster unter Tools →
   ABAP → Systeme → <system> ‚schreiben‘ einschalten.“ Das Feld kommt **ans Ende** der
   Feldliste, die vorhandenen Felder bleiben, wie sie sind.
2. **Kundennamensraum:** Geschrieben, angelegt oder gesperrt werden nur Objekte mit `Z*`, `Y*`
   oder `/…/`-Namensraum. Alles andere wird ohne Anfrage abgelehnt. Das gilt für den
   Objektnamen und bei `create_object` zusätzlich für das Paket, wobei `$TMP` als Paket erlaubt
   ist.
3. `source` bis 2 MB, sonst Fehler ohne Anfrage.
4. Passwort und Cookies erscheinen nie in Antwort, Fehler oder Log (wie Teil 1).

## Antworten (knapp)

- `abap_lock`: Handle, Objekt, ob ein Transport nötig ist (`CORRNR` / `IS_LOCAL` aus der
  Antwort), Hinweis „mit abap_unlock freigeben“.
- `abap_write`: „Quelltext geschrieben (<n> Zeichen). Lock freigegeben | Lock bleibt. Mit
  abap_activate aktivieren.“
- `abap_activate`: ob erfolgreich, dazu die Meldungen (Typ, Zeile, Text), höchstens 50.
- `abap_create_object`: URL des neuen Objekts.
- `abap_set_text_elements`: Anzahl geschriebener Elemente.

## Tests (nur Attrappe, nie ein echtes SAP)

Die axum-Attrappe aus Teil 1 wird erweitert. Lock, Write und Unlock gehen dort **nur mit dem
Cookie der Sitzung, die gesperrt hat**, eine andere Sitzung bekommt 403 oder 423. Fälle:

- Lock → Write mit Handle → Unlock: drei Aufrufe, dieselbe Sitzung, das Lock bleibt nach Write
  stehen.
- Write ohne Handle: genau Lock, PUT, Unlock. Scheitert PUT (Attrappe: 500), wird trotzdem
  entsperrt.
- Unbekannter Handle → Meldung ohne Anfrage.
- `schreiben` aus → jedes schreibende Tool meldet den Hinweis, die Attrappe zählt **0**
  Anfragen.
- Fremder Namensraum (`CL_ABAP_TYPEDESCR`, Paket `SAPBC`) → abgelehnt, 0 Anfragen. `$TMP` geht.
- Aktivierung mit Meldungen (Fehler in Zeile 3) → Meldung steht in der Antwort.
- `create_object` CLAS/OC und PROG/P → richtige Pfade und XML (aus `objectcreator.js`).
- `set_text_elements` → richtiges Format (aus `textelements.js`).
- Sitzung abgelaufen → klare Meldung, kein zweiter PUT.
- CSRF 403 in der stateful Sitzung → einmal neu holen, dann klappt es.
- 401 → kein zweiter Versuch.
- Beenden über stdio gibt offene Locks frei (die Attrappe zählt Unlocks).
- `quiver-abap eintrag` enthält das Feld `schreiben` als letztes Feld der Systeme.
- Ende-zu-Ende über stdio: `tools/list` hat 25 Tools plus `abap_profile`, die Annotationen
  stimmen.

## Doku

- README-Abschnitt quiver-abap um Teil 2 ergänzen, mit den Schutzregeln.
- `docs/architektur.md` ergänzen (stateful Sitzung).
- `docs/adr/0006-abap-schreiben.md`: Sitzung im Prozess, Schalter `schreiben` mit Standard aus,
  Kundennamensraum.
- `docs/offen.md`: was nicht umgesetzt ist, z. B. Transport anlegen, Inaktiv-Liste.
- Prüfbefehl grün: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings &&
  cargo test --workspace`.

**Nicht committen**, das macht Claude nach der Abnahme.
