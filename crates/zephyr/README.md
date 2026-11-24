# quiver-zephyr

Zephyr Scale Cloud (REST v2) als MCP-Tool für mcp-quiver. Aufgabe:
[docs/aufgaben/15-zephyr.md](../../docs/aufgaben/15-zephyr.md), gemeinsame Regeln:
[docs/aufgaben/12-atlassian-gemeinsam.md](../../docs/aufgaben/12-atlassian-gemeinsam.md),
Korrekturen nach der Abnahme: [KORREKTUREN.md](KORREKTUREN.md), Entscheidung:
[ADR 0010](../../docs/adr/0010-zephyr.md).

## Was es kann

Der User legt im quiver-Fenster unter **Tools → Zephyr** einen oder mehrere Zugänge an.
Jedes Werkzeug nimmt `verbindung`; bei genau einem Zugang darf der Name fehlen.

| Werkzeug | Art | was es tut |
|---|---|---|
| `zephyr_status` | lesend | die Zugänge: Name, Basisadresse, Projekt, `nur`, Schreiben – ohne Token, ohne Anfrage |
| `zephyr_testen` | lesend | prüft das Token mit `/projects?maxResults=1` und nennt die sichtbaren Projekte |
| `zephyr_projekte` | lesend | Projekte mit Schlüssel, Id und Jira-Bezug (`anzahl`, `ab`) |
| `zephyr_testfaelle` | lesend | Testfälle eines Projekts (Status- und Prioritätsnamen aufgelöst) |
| `zephyr_testfall` | lesend | ein Testfall mit Schritten (`/teststeps`) oder Skript (`/testscript`) |
| `zephyr_testzyklen` | lesend | Testzyklen eines Projekts |
| `zephyr_testzyklus` | lesend | ein Testzyklus **mit Zählung der Ausführungen je Status** |
| `zephyr_ausfuehrungen` | lesend | Ausführungen eines Projekts, optional eines Zyklus |
| `zephyr_ausfuehrung` | lesend | eine Ausführung mit Status, Zeitraum, Dauer, Art, Kommentar |
| `zephyr_status_liste` | lesend | die erlaubten Status eines Projekts, `art` = `TEST_EXECUTION` (Standard), `TEST_CASE`, `TEST_CYCLE`, `TEST_PLAN` |
| `zephyr_ordner` | lesend | Ordner (`TEST_CASE`, `TEST_PLAN`, `TEST_CYCLE`) |
| `zephyr_testfall_anlegen` | verändernd | legt einen Testfall an |
| `zephyr_zyklus_testfaelle_hinzufuegen` | verändernd | ordnet Testfälle einem Zyklus zu (je Testfall eine Ausführung) |
| `zephyr_ergebnis_eintragen` | verändernd | Status und Kommentar an einer Ausführung |
| `zephyr_profile` | lesend | aus dem Kern; bei einer Zugangsliste dasselbe wie `zephyr_status` |

Dazu die Kern-Unterbefehle: `quiver-zephyr eintrag` (Bibliothekseintrag für quiver),
`quiver-zephyr testen` (Zugänge aus `QUIVER_ZUGAENGE` prüfen), `--help`, `--version`.

## Felder eines Zugangs

| Feld | Art | Standard | Bedeutung |
|---|---|---|---|
| `basis_url` | Url | `https://api.zephyrscale.smartbear.com/v2` | Basisadresse der REST-API v2 |
| `api_token` | Passwort, Pflicht | – | Bearer-Token; im Jira-Profil unter „Zephyr API Access Tokens“ |
| `projekt` | Text | – | Standardprojekt(schlüssel); leer heißt: je Aufruf angeben |
| `nur` | Text | – | kommagetrennte Projektschlüssel, in denen geschrieben werden darf; leer heißt: alle |
| `schreiben` | Schalter | `aus` | erlaubt die drei verändernden Werkzeuge |

`basis_url` darf nur `https://` sein. Ausnahme ist Loopback (`127.0.0.1`, `localhost`,
`[::1]`), damit die Attrappe ohne TLS läuft. Geprüft wird über `url::Url`: Adressen mit
Benutzer oder Passwort (`user@host`), mit Abfrage (`?`) oder Fragment (`#`) werden
abgelehnt. Das Token steht nur im `Authorization: Bearer`-Kopf und in keiner Antwort,
keinem Fehler, keinem Log und keinem `Debug`.

## Vertrag mit der OpenAPI (Zephyr Scale REST v2)

Maßgeblich ist die OpenAPI-Datei der API (OpenAPI 3.0.3, 67 Pfade). **Jede** Anfrage –
Pfad, Methode, Parametername, Pflichtfeld im Rumpf – und **jedes** gelesene Antwortfeld
steht dort. Die Attrappe antwortet genau in diesen Formen und lehnt unbekannte
Abfrageparameter, unbekannte Rumpffelder, falsche Methoden und fehlende Pflichtfelder mit
`400` ab; unbekannte Objekte sind `404`.

Wichtig für alles Lesen: **Verweise tragen nur `id` und `self`.** `ProjectLink`,
`StatusLink`, `PriorityLink`, `FolderLink`, `EnvironmentLink`, `TestCycleLink` und
`TestCaseVersionLink` haben keinen `key` und keinen `name`. Namen kommen deshalb aus den
Endpunkten, die das Schema dafür nennt (`/statuses`, `/priorities`, `/projects/{id}`,
`/environments/{id}`, `/testcycles/{id}`).

### Lesende Werkzeuge

| Werkzeug | Methode | Pfad | Abfrage | gelesene Felder |
|---|---|---|---|---|
| `zephyr_status` | – | – | – | nur die Zugangsliste, keine Anfrage |
| `zephyr_testen` | GET | `/projects` | `maxResults=1` | `total`, `values[].key` |
| `zephyr_projekte` | GET | `/projects` | `maxResults`, `startAt` | `values[].id/.key/.jiraProjectId/.enabled`, `total`, `isLast`, `next` |
| `zephyr_testfaelle` | GET | `/testcases` | `projectKey`, `maxResults`, `startAt` | `values[].id/.key/.name`, `.status.id`, `.priority.id`, `.folder.id` |
| | GET | `/statuses` | `projectKey`, `statusType=TEST_CASE`, `maxResults=100`, `startAt` | `values[].id/.name` |
| | GET | `/priorities` | `projectKey`, `maxResults=100`, `startAt` | `values[].id/.name` |
| `zephyr_testfall` | GET | `/testcases/{key}` | – | `id`, `key`, `name`, `project.id`, `status.id`, `priority.id`, `folder.id`, `owner.accountId`, `createdOn`, `objective`, `precondition` |
| | GET | `/projects/{id}` | – | `key` |
| | GET | `/testcases/{key}/testscript` | – | `type`, `text` (422 = der Testfall hat Schritte) |
| | GET | `/testcases/{key}/teststeps` | `maxResults` | `values[].inline.description/.expectedResult/.testData` (422 = der Testfall ist ein Skript) |
| | GET | `/statuses`, `/priorities` | wie oben | wie oben |
| `zephyr_testzyklen` | GET | `/testcycles` | `projectKey`, `maxResults`, `startAt` | `values[].id/.key/.name`, `.status.id`, `.plannedStartDate`, `.plannedEndDate` |
| | GET | `/statuses` | `projectKey`, `statusType=TEST_CYCLE` | `values[].id/.name` |
| `zephyr_testzyklus` | GET | `/testcycles/{id-or-key}` | – | `id`, `key`, `name`, `project.id`, `status.id`, `plannedStartDate`, `plannedEndDate` |
| | GET | `/projects/{id}` | – | `key` |
| | GET | `/testexecutions` | `projectKey`, `testCycle` (Schlüssel aus der Antwort, nicht die Id), `maxResults=100`, `startAt` (max. 5 Seiten) | `values[].testExecutionStatus.id`, `total` |
| | GET | `/statuses` | `projectKey`, `statusType=TEST_CYCLE` / `TEST_EXECUTION` | `values[].id/.name` |
| `zephyr_ausfuehrungen` | GET | `/testexecutions` | `projectKey`, optional `testCycle`, `maxResults`, `startAt` | `values[].id/.key`, `.testCase.id`, `.testExecutionStatus.id`, `.testCycle.id`, `.environment.id`, `total`, `isLast`, `next` |
| | GET | `/statuses` | `projectKey`, `statusType=TEST_EXECUTION` | `values[].id/.name` |
| | GET | `/environments/{id}` | – | `name` |
| | GET | `/testcycles/{id}` | – | `key` |
| `zephyr_ausfuehrung` | GET | `/testexecutions/{id-or-key}` | – | `id`, `key`, `project.id`, `testCase.id`, `testExecutionStatus.id`, `testCycle.id`, `environment.id`, `actualEndDate`, `executionTime`, `automated`, `comment` |
| | GET | `/projects/{id}`, `/statuses`, `/environments/{id}`, `/testcycles/{id}` | wie oben | wie oben |
| `zephyr_status_liste` | GET | `/statuses` | `projectKey`, `statusType`, `maxResults`, `startAt` | `values[].id/.name/.default/.archived/.description` |
| `zephyr_ordner` | GET | `/folders` | `projectKey`, `folderType`, `maxResults`, `startAt` | `values[].id/.name/.folderType/.parentId` |

### Verändernde Werkzeuge

| Werkzeug | Methode | Pfad | Rumpf | gelesene Felder |
|---|---|---|---|---|
| `zephyr_testfall_anlegen` | GET | `/projects/{id-or-key}` | – | `key` (prüft zugleich, ob es das Projekt gibt) |
| | POST | `/testcases` | `projectKey`, `name`, `objective`?, `precondition`?, `folderId`? (`TestCaseInput`) | `id`, `key` (`KeyedCreatedResource`) |
| `zephyr_zyklus_testfaelle_hinzufuegen` | GET | `/testcycles/{id-or-key}` | – | `project.id`, `key` |
| | GET | `/projects/{id}` | – | `key` |
| | GET | `/statuses` | `projectKey`, `statusType=TEST_EXECUTION` | `values[].name/.default` |
| | GET | `/testcases/{key}` (je Testfall) | – | `key`, `project.id` – Existenz und Projekt |
| | GET | `/projects/{id}` (je Testfall, gecacht) | – | `key` |
| | POST | `/testexecutions` | `projectKey`, `testCaseKey`, `testCycleKey`, `statusName`, `environmentName`? (`TestExecutionInput`) | `id` (`CreatedResource` – **kein** Schlüssel) |
| `zephyr_ergebnis_eintragen` | GET | `/testexecutions/{id-or-key}` | – | `id`, `key`, `project.id`, `automated` |
| | GET | `/projects/{id}` | – | `key` |
| | GET | `/statuses` | `projectKey`, `statusType=TEST_EXECUTION` | `values[].name` |
| | PUT | `/testexecutions/{id}` | `statusName`, `automated`, `comment`? (`TestExecutionUpdate`) | 2xx ohne Rumpf ist Erfolg |

Die Attrappe ist der Prüfstein: sie kennt nur diese Pfade, nur diese Parameter und nur
diese Rumpffelder. Ein zusätzliches Feld im Rumpf ist dort ein `400`, ein zusätzlicher
Abfrageparameter ebenso.

## Sicherheit (Regeln aus Aufgabe 12)

- **Keine Weiterleitung:** der HTTP-Client folgt keiner `3xx` (`redirect::Policy::none`);
  eine Folgeseite (`next`) wird abgelehnt, **ohne** weitere Anfrage, wenn ihr **Ursprung**
  abweicht – Schema, Host **und** Port werden verglichen (`url::Url`), `https` und Port 443
  gelten als derselbe Ursprung.
- **Werte in Pfaden** werden geprüft **und** prozent-kodiert. `.`, `..` und leere Werte
  werden **ohne Anfrage** abgelehnt: `url`/`reqwest` normalisieren `%2E%2E` trotzdem zu
  `..`, ein kodierter Wert allein genügt also nicht. Zusätzlich wird die **fertige**
  Adresse zerlegt und mit dem gebauten Pfad verglichen; weicht sie ab, geht nichts raus.
  `/`, `?` und `#` in Werten werden kodiert (`a/b?c#d` → `a%2Fb%3Fc%23d`).
- **`schreiben` je Zugang, Standard aus:** ist der Schalter aus, antworten die drei
  verändernden Werkzeuge mit dem Hinweis und **ohne** Anfrage.
- **`nur`:** ist es gesetzt, wird außerhalb der erlaubten Projekte nicht geschrieben. Den
  Projektschlüssel liest das Werkzeug **vom Server** (`GET /projects/{id}`) und vergleicht
  ohne Groß-/Kleinschreibung. Beim Zuordnen wird **jeder** Testfall vorher gelesen: gibt es
  ihn, und gehört er in dasselbe Projekt?
- **401** → „Token für <verbindung> ungültig oder abgelaufen – im quiver-Fenster
  erneuern“, kein Wiederholen. **403** → Hinweis auf die Rechte des Tokens.
- **429/503** nur bei **lesenden** Aufrufen, höchstens zweimal, mit `Retry-After`.
  Verlangt der Server mehr als 30 s, wird sofort aufgegeben statt gewartet. Ein
  verändernder Aufruf wird **nie** wiederholt.
- **Leerer Rumpf (`204`/`2xx`)** nach einer Änderung ist Erfolg.
- **Unlesbarer Rumpf** ist ein Fehler und nie „leer“: bei einem lesenden Aufruf scheitert
  er („das ist ein Fehler, keine leere Antwort“), bei einer Änderung heißt es „ausgeführt,
  Antwort nicht lesbar – Stand prüfen“.
- **Keine Antwort** auf eine Änderung (Zeitüberschreitung, Abbruch) heißt „Ausgang unklar –
  Stand prüfen, nicht einfach wiederholen“.

## Annahmen zur API

Was die OpenAPI-Datei offen lässt, steht hier – mit dem Preis.

1. **`PUT /testexecutions/{id}` ergänzt, es ersetzt nicht.** Die Beschreibung des Aufrufs
   sagt: *„This operation only updates specified fields in the payload and ignores `null`
   or `undefined` values.“* Nicht gesendete Felder bleiben also stehen. `zephyr_ergebnis_eintragen`
   sendet darum nur `statusName`, `automated` und – wenn angegeben – `comment`.
   `automated` **muss** mit: das Feld hat laut Schema den Standard `true`, ohne den
   gelesenen Wert würde eine manuelle Ausführung umspringen. `projectKey`, `testCaseKey`,
   `testCycleKey` und `customFields` kennt `TestExecutionUpdate` **nicht** – sie dürfen
   nicht mit, die Attrappe weist sie mit `400` ab.
2. **Statusnamen sind pro Projekt und Art.** Geprüft wird gegen
   `GET /statuses?projectKey=…&statusType=…` (alle Seiten, bis 20 × 100 Einträge); gesendet
   wird die Schreibweise des Servers. Für Ergebnisse und Zuordnungen ist die Art
   `TEST_EXECUTION`.
3. **`statusName` ist Pflicht.** Fehlt er beim Zuordnen, gilt nur dann ein Standard, wenn
   `/statuses` einen nennt (`default: true`). Sonst endet der Aufruf mit den erlaubten
   Namen und ohne Änderung.
4. **Projekt und Zyklus kommen vom Server.** `GET /projects/{id}` nennt den Schlüssel,
   `GET /testcycles/{id-or-key}` den Zyklusschlüssel. Gesendet wird immer der gelesene
   Wert – der Agent darf auch eine Id angegeben haben.
5. **Zephyr nennt keinen Benutzernamen.** `zephyr_testen` meldet darum „Token gilt“ und
   die Zahl der sichtbaren Projekte (`/projects?maxResults=1`, wie in der Aufgabe).
6. **Ein Testfall hat Schritte *oder* ein Skript.** `GET /testcases/{key}/testscript`
   antwortet mit `422 TestCaseHasStepsError`, `GET /testcases/{key}/teststeps` mit
   `422 TestCaseIsScriptError`. Beides wird als Auskunft gelesen, nicht als Fehler.
7. **Eine angelegte Ausführung hat keinen Schlüssel.** `POST /testexecutions` antwortet mit
   `CreatedResource` (`id`, `self`), nicht mit `KeyedCreatedResource` – die Antwort des
   Werkzeugs nennt darum nur die Id. Eine angelegte **Testfall**-Antwort ist
   `KeyedCreatedResource`; auch sie nennt weder Namen noch Status, der Name in der Antwort
   ist der übergebene.
8. **`testCase` einer Ausführung ist ein Versionsverweis** (`TestCaseVersionLink`: `id`,
   `self`) – die API gibt keinen Schlüssel her. In Ausführungslisten steht darum
   `Testfall: (id 11)`; den Schlüssel liefert `zephyr_testfaelle`.
9. **Zählung je Status ist begrenzt.** `zephyr_testzyklus` liest höchstens 5 Seiten à 100
   Ausführungen und sagt, wenn nicht alle gezählt wurden („aus N von M gelesenen“).
10. **Listengröße:** `anzahl` ist standardmäßig 25 und höchstens 100 (die API erlaubt bis
   1000 – die gemeinsame Regel aus Aufgabe 12 deckelt bei 100); `ab` ist die Startposition
   und endet bei 1 000 000 (`startAt` der API).
11. **Verweise werden je Id einmal aufgelöst.** Umgebungen und Testzyklen einer
    Ausführungsliste kosten je distinct Id eine Anfrage (`/environments/{id}`,
    `/testcycles/{id}`); nach 50 Verweisen bleibt die Id stehen.

## Tests

```bash
CARGO_TARGET_DIR=target-pi \
  cargo fmt --all --check && \
  cargo clippy -p quiver-zephyr --all-targets -- -D warnings && \
  cargo test -p quiver-zephyr
```

Die Attrappe (`tests/attrappe`) ist ein axum-Programm und **streng**: unbekannte Pfade und
Abfrageparameter sind `400`, ein falscher `Authorization`-Kopf ist `401`, Pflichtfelder im
Rumpf werden geprüft, und jeder Rumpf darf nur Felder aus dem Schema enthalten. Sie kann
`401`, `403`, `429`, `503`, eine Weiterleitung, eine Folgeseite mit fremdem Host und
kleinere Seiten (`seite_max`) melden. `tests/attrappe/defekt.rs` antwortet mit
abgeschnittenen Rümpfen und ohne Antwort, damit die drei Fehlerfälle aus Regel 3 prüfbar
sind. Kein Test spricht mit einem echten Zephyr.

## Offen

- **Zephyr Squad und Zephyr Scale Data Center** werden nicht unterstützt – nur Scale Cloud
  mit REST v2 (Bearer). Squad und DC haben andere Endpunkte und eine andere Anmeldung.
- **Testschritte ändern** gibt es nicht (nur lesen beim Testfall); auch das Anlegen mit
  Schritten fehlt.
- **Testfälle ändern oder löschen**, Zyklen anlegen, Ausführungen löschen: nicht in
  Aufgabe 15.
- **Anhänge, Labels, Komponenten, benutzerdefinierte Felder** beim Anlegen: nur
  `objective`, `precondition` und `folderId` werden gesetzt. `customFields` gehen beim
  Ergebnis-Eintragen nicht mit – `TestExecutionUpdate` kennt sie nicht.
- **Der Testfallschlüssel einer Ausführung** lässt sich aus der API nicht ableiten (siehe
  Annahme 7). Ein Umweg über `/testcases?projectKey=…` (alle Seiten) wäre ein Vielfaches an
  Anfragen; er ist bewusst nicht eingebaut.
- **`zephyr_ergebnis_eintragen` braucht vier Anfragen** (Ausführung lesen, Projekt lesen,
  Status prüfen, `PUT`). Ein Cache je Projekt und Art wäre eine Anfrage weniger, könnte
  aber veralten.
- **`tracing`-Ereignisse** des Werkzeugs sind stumm, weil der Kern keinen Subscriber setzt.
