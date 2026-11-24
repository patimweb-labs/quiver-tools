# quiver-jira

`quiver-jira` ist das MCP-Tool für **Jira Cloud** (REST-API v3). Es liest Vorgänge,
Kommentare, Übergänge, Projekte, Filter, Personen und Verknüpfungsarten und ändert
Vorgänge – aber nur, wenn der Schalter „schreiben“ für die Site an ist und das Projekt im
Feld „nur“ liegt.

Das Programm ist ein eigener Prozess (MCP über stdio), kein Teil von quiver. Die
Zugangsdaten trägt der User im quiver-Fenster unter **Tools → Jira** ein; quiver übergibt
sie als Umgebung. Weder E-Mail noch API-Token erreichen je einen Agenten.

## Felder einer Site

Wie `quiver-devops` pflegt der User eine **Liste** von Sites
(`Werkzeug::mehrfach = ("Sites", "Site")`). quiver übergibt sie als JSON in
`QUIVER_ZUGAENGE`:

```json
[{"name":"firma","werte":{"url":"https://firma.atlassian.net","email":"…",
  "api_token":"…","nur":"ABC, DEF","schreiben":"an"}}]
```

| Feld | Art | Pflicht | Bedeutung |
|---|---|---|---|
| `url` | Url | ja | `https://firma.atlassian.net` (nur `https://`; Loopback darf `http://` sein) |
| `email` | Text | ja | E-Mail des Atlassian-Kontos |
| `api_token` | Passwort | ja | API-Token; Hilfe-Link auf die Token-Seite von Atlassian |
| `nur` | Text | nein | kommagetrennte Projektschlüssel; leer heißt „alle“ |
| `schreiben` | Schalter | nein | Standard **aus**; erlaubt die fünf verändernden Werkzeuge |

Jedes Werkzeug nimmt `verbindung` (Name der Site). Bei genau einer Site darf der Name
fehlen; die Schreibweise ist egal. Fehlt die Liste, antwortet jedes Werkzeug mit
„Zugang fehlt – bitte im quiver-Fenster unter Tools → Jira die Angaben eintragen.“ (Text,
kein MCP-Fehler).

## Werkzeuge

Lesend (`read_only_hint`):

| Werkzeug | API |
|---|---|
| `jira_status` | Sites ohne E-Mail und Token, mit `nur` und `schreiben` (keine Anfrage) |
| `jira_testen` | `GET /myself` – wer ist angemeldet |
| `jira_projekte` | `GET /project/search` mit Folgeseiten (`next`, nur am eigenen Host) |
| `jira_suchen` | `POST /search/jql` mit `nextPageToken`; Komfort `projekt`, `status`, `zugewiesen_an_mich`, `text` baut die JQL. Eine unbegrenzte JQL wird vorher abgelehnt |
| `jira_vorgang` | `GET /issue/{key}` mit Beschreibung (ADF → Text), Verknüpfungen, Unteraufgaben, Eltern |
| `jira_kommentare` | `GET /issue/{key}/comment` |
| `jira_uebergaenge` | `GET /issue/{key}/transitions` |
| `jira_felder_zum_anlegen` | `GET /issue/createmeta/{projekt}/issuetypes[/{typ}]` |
| `jira_filter_liste` | `GET /filter/search` |
| `jira_filter` | `GET /filter/{id}` |
| `jira_verknuepfungsarten` | `GET /issueLinkType` |
| `jira_person_suchen` | `GET /user/search` |

Verändernd (`destructive_hint`, brauchen `schreiben`):

| Werkzeug | API |
|---|---|
| `jira_anlegen` | `GET /project/{key}` (Schlüssel lesen, `nur` prüfen), dann `POST /issue`; `eltern` macht daraus eine Unteraufgabe und muss ebenfalls in `nur` liegen |
| `jira_aendern` | `PUT /issue/{key}` (204 ist Erfolg); `project` wird abgelehnt, doppelte Felder auch, `parent` nur innerhalb von `nur` |
| `jira_uebergang` | `GET /issue/{key}/transitions` (nur diese sind erlaubt), `POST …/transitions` (204 ist Erfolg), dann `GET /issue/{key}?fields=status` für den neuen Status; optional mit Kommentar |
| `jira_kommentieren` | `POST /issue/{key}/comment` |
| `jira_verknuepfen` | `POST /issueLink`, danach `GET /issue/{vorgang}` als Gegenprobe der Richtung; beide Vorgänge werden gegen `nur` geprüft |

Dazu kommt `jira_profile` aus dem Kern (zeigt die Sites ohne Geheimnisse).

## Vertrag mit der OpenAPI

Geprüft gegen `jira.json` (OpenAPI 3.0.1, Jira Cloud REST v3). **Jede** Anfrage
stimmt in Pfad, Methode und Parametername mit der Spezifikation überein, und jedes
**gelesene** Antwortfeld steht dort. Die Attrappe antwortet in genau diesen Formen und
lehnt unbekannte Pfade, unbekannte Abfrageparameter, eine falsche Methode und fehlende
Pflichtfelder im Rumpf mit `400` ab.

| Werkzeug | Methode | Pfad | Abfrage | Rumpf → gelesen |
|---|---|---|---|---|
| `jira_testen` | GET | `/myself` | – | – → `accountId`, `displayName`, `emailAddress` |
| `jira_projekte` | GET | `/project/search` | `maxResults`, `startAt` (nur über `next`) | – → `values[]` (`id`, `key`, `name`, `projectTypeKey`, `lead.displayName`), `total`, `isLast`, `next` |
| `jira_suchen` | POST | `/search/jql` | – | `jql`, `maxResults`, `fields`, `nextPageToken` → `issues[]` (`key`, `fields.summary/status.name/issuetype.name/assignee.displayName/updated`), `nextPageToken` |
| `jira_vorgang` | GET | `/issue/{issueIdOrKey}` | – | – → `key`, `fields` (`summary`, `status.name`, `issuetype.name`, `project.key/name`, `assignee.displayName`, `reporter.displayName`, `priority.name`, `labels`, `created`, `updated`, `duedate`, `description`, `issuelinks`, `subtasks`, `parent`) |
| `jira_kommentare` | GET | `/issue/{issueIdOrKey}/comment` | `maxResults`, `orderBy=created` | – → `comments[]` (`id`, `author.displayName`, `created`, `body`), `total` |
| `jira_uebergaenge` | GET | `/issue/{issueIdOrKey}/transitions` | – | – → `transitions[]` (`id`, `name`, `to.name`) |
| `jira_felder_zum_anlegen` | GET | `/issue/createmeta/{projectIdOrKey}/issuetypes` | – | – → `issueTypes[]` (`id`, `name`, `subtask`), `total` |
| `jira_felder_zum_anlegen` (mit Typ) | GET | `/issue/createmeta/{projectIdOrKey}/issuetypes/{issueTypeId}` | – | – → `fields{}` (`required`, `name`, `schema.type`, `allowedValues[].name`) |
| `jira_filter_liste` | GET | `/filter/search` | `maxResults`, `filterName` | – → `values[]` (`id`, `name`, `jql`, `owner.displayName`), `isLast`, `total` |
| `jira_filter` | GET | `/filter/{id}` | – | – → `id`, `name`, `jql`, `description`, `owner.displayName` |
| `jira_verknuepfungsarten` | GET | `/issueLinkType` | – | – → `issueLinkTypes[]` (`id`, `name`, `inward`, `outward`) |
| `jira_person_suchen` | GET | `/user/search` | `query`, `maxResults` | – → `accountId`, `displayName`, `emailAddress`, `active` |
| `jira_anlegen` | GET | `/project/{projectIdOrKey}` | – | – → `key` (der gesendete Schlüssel) |
| `jira_anlegen` | POST | `/issue` | – | `fields.project.key` (vom Server gelesen), `fields.issuetype.{id,name}`, `fields.summary`, `fields.description` (ADF), `fields.parent.key`, `fields.assignee.accountId`, freie Felder → `id`, `key` |
| `jira_aendern` | PUT | `/issue/{issueIdOrKey}` | – | `fields{…}` → `204` (kein Rumpf) |
| `jira_uebergang` | GET | `/issue/{issueIdOrKey}/transitions` | – | – → `transitions[]` (`id`, `name`, `to.name`) |
| `jira_uebergang` | POST | `/issue/{issueIdOrKey}/transitions` | – | `transition.id`, optional `update.comment[].add.body` (ADF) → `204` |
| `jira_uebergang` | GET | `/issue/{issueIdOrKey}` | `fields=status` | – → `fields.status.name` |
| `jira_kommentieren` | POST | `/issue/{issueIdOrKey}/comment` | – | `body` (ADF) → `id` |
| `jira_verknuepfen` | POST | `/issueLink` | – | `type.name`, `outwardIssue.key`, `inwardIssue.key`, optional `comment.body` (ADF) → `201` ohne Rumpf |
| `jira_verknuepfen` | GET | `/issue/{issueIdOrKey}` | – | – → `fields.issuelinks[]` (`id`, `type.name/inward/outward`, `outwardIssue.key`, `inwardIssue.key`) |
| alle ändernden Werkzeuge | GET | `/issue/{issueIdOrKey}` | `fields=project` | – → `fields.project.key` (Prüfung gegen `nur`) |

## Sicherheit

- **Anmeldung:** HTTP Basic `Authorization: Basic base64(email:api_token)` – E-Mail und
  Token stehen nur in dieser Kopfzeile, nie in Antwort, Fehler, Log oder `Debug`.
- **Adresse:** nur `https://`. Einzige Ausnahme ist Loopback (`127.0.0.1`, `localhost`,
  `[::1]`) für die Attrappe. Geprüft wird ausschließlich über `url::Url` (Schema, Host,
  Port), nicht über eine eigene Zerlegung; Zugangsdaten in der Adresse (`user@host`),
  Abfrage und Fragment werden abgelehnt. Damit fällt auch `http://evil.test\@127.0.0.1/`
  durch: der Host ist `evil.test`, nicht Loopback.
- **Keine Weiterleitung:** `redirect::Policy::none()`. Eine Folgeseite (`next`) wird vor
  der Anfrage gegen den eingetragenen Host geprüft (Schema, Host, Port über `url::Url`);
  ein fremder Host, Zugangsdaten oder ein Fragment werden abgelehnt, ohne ihn
  anzufragen.
- **Keine Cookies:** der Client hat keinen Cookie-Speicher und liest `Set-Cookie` nicht.
- **Pfade:** Werte vom Agenten werden prozent-kodiert (auch `/`, `?`, `#`, `%`, `\`);
  `.` und `..` (auch `%2E%2E`) sowie leere Werte werden **ohne Anfrage** abgelehnt, weil
  `url` solche Segmente sonst stillschweigend normalisiert. Geprüft wird am Ende die
  fertige `Url`, nicht nur die Kodierung.
- **Der Rumpf gehört zum Ergebnis:** lässt er sich bei einem lesenden Aufruf nicht
  lesen, ist das ein Fehler (nie „leer“); nach einer Änderung mit `2xx` heißt es
  „ausgeführt, Antwort nicht lesbar“; kommt auf eine Änderung **keine** Antwort
  (Zeitüberschreitung, Abbruch), heißt es „Ausgang unklar – Stand prüfen, nicht einfach
  wiederholen“, und es wird nie wiederholt.
- **`schreiben` je Site, Standard aus.** Ist er aus, antworten alle fünf verändernden
  Werkzeuge mit „Schreiben ist für <site> aus – im quiver-Fenster einschalten.“ **ohne**
  Anfrage.
- **`nur`:** ist das Feld gesetzt, wird vor jeder Änderung das Projekt **vom Server**
  gelesen (`GET issue/{key}?fields=project`) und ohne Groß-/Kleinschreibung verglichen.
  Außerhalb gibt es eine Antwort mit Hinweis und **keine** verändernde Anfrage. Das gilt
  auch für einen Elternvorgang (`eltern` bzw. `felder.parent`). Beim **Anlegen** wird
  `GET project/{key}` ohnehin immer gelesen – und genau dieser Schlüssel gesendet.
- **401** → „Token für <site> ungültig oder abgelaufen – im quiver-Fenster erneuern
  (Tools → Jira).“, kein Wiederholen. **403** → Hinweis auf fehlende Rechte.
- **429/503** wird nur bei **lesenden** Aufrufen wiederholt: höchstens zweimal,
  `Retry-After` höchstens 30 s. Ein Anlegen, Ändern oder Kommentieren wird nie
  wiederholt.
- **Leerer Rumpf** (204) nach einer Änderung ist Erfolg, kein Deserialisierungsfehler.
- Lange Texte werden über `max_zeichen` gekürzt (Standard 4000) **mit Hinweis**; Listen
  zeigen `anzahl` Einträge (Standard 25, höchstens 100) und sagen, ob es mehr gibt.

## API-Annahmen

Die Pfade und Antwortformen sind an der REST-API v3 von Jira Cloud abgelesen und gegen
die eigene Attrappe geprüft. Bewusste Annahmen – alles, was sich aus der Spezifikation
**nicht** klären lässt:

- **Suche** über `POST /search/jql` mit `nextPageToken` (die neue Suche; `startAt` der
  alten Suche gibt es hier nicht). Die Antwort trägt `issues`, `nextPageToken` und
  `isLast`. Der Rumpf ist `SearchAndReconcileRequestBean` (`additionalProperties: false`),
  gesendet werden nur `jql`, `maxResults`, `fields` und – beim Blättern –
  `nextPageToken`.
- **Begrenzte JQL:** Die Spezifikation verlangt für `search/jql` ausdrücklich eine
  „bounded query“ (Beispiel dort: `order by key desc` ist unbegrenzt). Eine JQL ohne
  Einschränkung wird darum **vor** der Anfrage mit einem verständlichen Hinweis
  abgelehnt. Wie Jira eine Grenze im Einzelfall zieht (z. B. `project = …`), steht nicht
  in der Spezifikation; geprüft wird nur, ob vor einer `order by`-Klausel überhaupt eine
  Bedingung steht.
- **JQL-Form:** gebaut wird reines JQL (`project = 'ABC' AND … ORDER BY updated DESC`).
  Die Spezifikation nennt für `jql` keine Grammatik, sondern verweist auf die
  JQL-Dokumentation.
- **`maxResults`:** `project/search` erlaubt höchstens 100 (Spezifikation), `search/jql`
  bis 5000. Das Werkzeug begrenzt Listen einheitlich auf `anzahl` (Standard 25, höchstens
  100) – die Grenzen der Spezifikation werden damit nie überschritten.
- **Projekte** über `GET /project/search` mit `values`, `total`, `isLast` und `next`.
- **createmeta** über die neuen Endpunkte
  `GET /issue/createmeta/{projekt}/issuetypes` (Antwort `issueTypes[]`, `total`) und
  `…/issuetypes/{typId}` (Antwort `fields{}`); der alte Endpunkt mit
  `expand=projects.issuetypes.fields` ist abgekündigt.
- **Filter** über `GET /filter/search` (`values`, `isLast`, `total`) und `GET /filter/{id}`.
- **Personen** über `GET /user/search?query=`; die `accountId` aus der Antwort ist der
  Wert für `zugewiesen` beim Anlegen und Ändern.
- **Kommentare** über `GET /issue/{key}/comment?orderBy=created` (`created` ist einer der
  drei erlaubten Werte der Spezifikation).
- **Verknüpfungsrichtung:** Die Spezifikation beschreibt `inward`/`outward` nur als
  Beschreibungstexte der Verknüpfungsart und verlangt beide Vorgänge. Gedeutet wird das
  so, wie Jira es anzeigt: `outwardIssue` ist die Seite, von der der `outward`-Text
  ausgeht, `inwardIssue` die Gegenseite. Bei `art` = „Blocks“ heißt das „vorgang blocks
  ziel“ und beim Ziel „ziel is blocked by vorgang“. Nach dem Anlegen liest das Werkzeug
  die Verknüpfung am Vorgang (`GET /issue/{vorgang}`) und meldet sie so, wie Jira sie
  zeigt; steht sie andersherum, sagt der Text das deutlich („ACHTUNG … andersherum“).
- **Übergänge:** `GET /issue/{key}/transitions` nennt die erlaubten Übergänge; das
  Werkzeug führt nur diese aus (ID oder Name, bei mehrdeutigem Namen kommen die
  Kandidaten). Danach liest es `GET /issue/{key}?fields=status` und meldet den neuen
  Status. Was ein Übergang am Status ändert, steht nur im Übergang selbst (`to.name`) –
  gemeldet wird deshalb der **gelesene** Status, nicht der erwartete.
- **Pflichtfelder im Rumpf:** Die Schemata (`IssueUpdateDetails`, `Comment`,
  `LinkIssueRequestJsonBean`) markieren kaum Felder als `required`; die echte API
  verlangt beim Anlegen `project`, `issuetype` und `summary`, beim Kommentar `body` und
  bei der Verknüpfung `type`, `inwardIssue` und `outwardIssue`. Die Attrappe prüft genau
  diese.
- **ADF:** Die Spezifikation beschreibt den Rumpf nur als „Atlassian Document Format“ und
  verweist auf die ADF-Beschreibung (Struktur des Dokuments); sie zählt die Knoten nicht
  auf. Geprüft wird darum gegen die dort genannten Knoten (`doc`, `paragraph`, `text`,
  `hardBreak`, Listen, `codeBlock`, …): ein unbekannter Knoten und ein `text`-Knoten ohne
  Inhalt werden **vor** dem Senden abgelehnt. Ein leerer Absatz (`paragraph` ohne
  `content`) und ein `doc` ohne Blöcke gelten als gültig; leere Listeneinträge und leere
  Codeblöcke werden gar nicht erst erzeugt.
- Ein **ADF-Dokument** wird beim Anlegen, Ändern, Kommentieren und Verknüpfen erzeugt.
  Text wird gedeutet als Absätze, harte Zeilenumbrüche (`hardBreak`), Aufzählungen
  (`- ` / `* `), numerierte Listen (`1. `) und Codeblöcke (```` ``` ````). Markdown wird
  nicht weiter gedeutet. Ein `description`-Wert in `felder` darf auch ein fertiges
  ADF-Objekt sein (es wird geprüft) oder Text (er wird zu ADF).
- **`POST /issueLink`** antwortet laut Spezifikation mit `201` ohne Rumpf; die
  Verknüpfungs-ID steht danach in `issuelinks[]` des Vorgangs.
- **Projektschlüssel beim Anlegen:** Gesendet wird der Schlüssel aus
  `GET /project/{key}` (die Schreibweise des Agenten ist egal, `abc` wird zu `ABC`).
  Dafür kostet jedes Anlegen eine zusätzliche lesende Anfrage; ohne sie ließe sich weder
  `nur` prüfen noch garantieren, dass das Projekt so heißt, wie Jira es nennt.
- Ein **Status** lässt sich nur über einen Workflow-Übergang ändern, nicht über
  `jira_aendern` – das ist Absicht (Jira erlaubt es nicht anders, und `jira_aendern`
  lehnt `project` ab).

## Aufbau

```
src/werkzeug.rs   Felder, mehrfach, testen
src/sites.rs      QUIVER_ZUGAENGE → geprüfte Sites (über url::Url), Auswahl, nur
src/client.rs     reqwest: Basic, keine Weiterleitung, next-Prüfung, Pfadprüfung, 429
src/fehler.rs     401, 403, HTTP, Netz, fremder Host, unlesbarer Rumpf, unklarer Ausgang
src/daten.rs      JSON → Werte (Punktpfade, Listen, Datum)
src/adf.rs        Text ⇄ Atlassian Document Format, Prüfung gegen die ADF-Knoten
src/jql.rs        JQL aus den Komfort-Parametern, maskiert, „begrenzt?“
src/uebergang.rs  welcher Workflow-Übergang gemeint ist
src/format.rs     reine Funktionen: Daten → Text
src/server.rs     die 17 MCP-Werkzeuge
```

## Tests

Nur gegen eine axum-Attrappe unter `tests/attrappe`, nie gegen ein echtes Jira. Die
Attrappe ist streng: unbekannte Pfade, unbekannte Abfrageparameter und eine falsche
Methode sind `400`, ein falscher `Authorization`-Kopf ist `401`, Pflichtfelder im Rumpf
werden geprüft, ADF-Rümpfe werden auf leere `text`-Knoten geprüft, und eine JQL ohne
Einschränkung lehnt sie wie die echte API ab. Sie kann `403`, `429` (mit `Retry-After`),
eine Weiterleitung und eine Folgeseite mit fremdem Host melden; ein Übergang ändert den
Status wirklich, eine angelegte Verknüpfung ist am Vorgang zu sehen (auf Wunsch auch
verdreht – die Gegenprobe zur gemeldeten Richtung).

```bash
cargo fmt --all --check
cargo clippy -p quiver-jira --all-targets -- -D warnings
cargo test -p quiver-jira
```

Geprüft werden unter anderem: je Werkzeug ein Erfolg, Auswahl der Site, `nur` (auch für
beide Seiten einer Verknüpfung und für Elternvorgänge), `schreiben` aus ohne Anfrage,
401/403, 429 nur lesend, keine Weiterleitung, fremde Folgeseite ohne Anfrage, `.`/`..`
im Pfad ohne Anfrage (geprüft an der fertigen `Url`), ein unlesbarer Rumpf (lesend =
Fehler, ändernd = „ausgeführt“), eine Änderung ohne Antwort („Ausgang unklar“, an einem
rohen Socket mit kurzem Zeitlimit), ADF in beide Richtungen samt Prüfung, freie Felder
als JSON, doppelte Felder, der vom Server gelesene Projektschlüssel, Übergänge nur aus
`GET …/transitions` samt neuem Status, die gemeldete Verknüpfungsrichtung und der
Kindprozess über stdio samt `quiver-jira eintrag`.

## Offen

- **OAuth (3LO)** für Atlassian Cloud fehlt. Das Tool meldet sich nur mit E-Mail und
  API-Token an (ADR 0008); ein OAuth-Ablauf bräuchte Browser, Konto und Token-Erneuerung.
- **Jira Data Center / Server** fehlt. Die REST-API dort weicht ab (kein ADF, andere
  Suche); heute gilt nur Jira Cloud.
- **Anhänge** werden weder gelesen noch hochgeladen; ein ADF-`media`-Knoten erscheint im
  Text nur als `[Anhang]`.
- **Benutzerdefinierte Felder** müssen als Name (`customfield_10010`) angegeben werden;
  eine Auflösung über `GET /field` (Name → ID) gibt es nicht.
- **Status** lässt sich nur über einen Workflow-Übergang ändern, nicht über
  `jira_aendern`; das ist Absicht (Jira erlaubt es nicht anders).
- **JQL** wird nur für die Komfort-Parameter gebaut. Für alles andere gibt der Agent
  `jql` fertig an. Geprüft wird nur, ob sie überhaupt eine Einschränkung hat (ob vor
  einer `order by`-Klausel eine Bedingung steht); eine Grammatik-Prüfung gibt es nicht –
  den Rest lehnt Jira mit `400` ab.
- **ADF-Prüfung** kennt die Knoten, die die ADF-Beschreibung heute nennt. Ein neuer
  Knoten von Atlassian müsste in `adf::KNOTEN` ergänzt werden, sonst lehnt das Werkzeug
  ihn ab (bewusst: lieber nicht senden als ungültig senden).
- **Ein Elternvorgang** wird nur geprüft, wenn `nur` gesetzt ist; ohne `nur` kostet
  `eltern` keine zusätzliche Anfrage. Ein angelegtes Projekt wird dagegen **immer**
  gelesen (einmal mehr, dafür stimmt der gesendete Schlüssel).
- **Folgeseiten** werden bei `jira_projekte` verfolgt. `jira_suchen` gibt den
  `nextPageToken` aus und nimmt ihn als `seite` wieder an, statt selbst zu blättern –
  so entscheidet der Agent, wie viele Seiten er bezahlt.
- **`jira_filter_liste`** nutzt `GET /filter/search` (in der Spezifikation nicht als
  abgekündigt markiert, in der Dokumentation aber als künftig ersetzt genannt); bei einer
  Abschaltung bleibt `/filter/my` als Ersatz (neue Aufgabe).
- **`tracing`**-Ereignisse des Werkzeugs sind stumm, weil der Kern keinen Subscriber
  setzt.
