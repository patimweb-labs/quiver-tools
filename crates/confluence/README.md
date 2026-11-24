# quiver-confluence

Confluence Cloud als MCP-Tool: Bereiche, Seiten, Unterseiten, Kommentare,
Anhänge und Labels lesen, Inhalte über CQL suchen, Seiten und Kommentare
anlegen und ändern. Ein Programm je Aufruf, MCP über stdio – so startet
mcp-quiver es.

Aufgabe: [docs/aufgaben/14-confluence.md](../../docs/aufgaben/14-confluence.md),
gemeinsame Regeln: [docs/aufgaben/12-atlassian-gemeinsam.md](../../docs/aufgaben/12-atlassian-gemeinsam.md),
Entscheidung: [docs/adr/0009-confluence.md](../../docs/adr/0009-confluence.md).

## Aufruf

```bash
quiver-confluence                       # MCP über stdio
quiver-confluence eintrag [--befehl X]  # Bibliothekseintrag für quiver als JSON
quiver-confluence testen                # jede eingetragene Site anmelden
quiver-confluence --version
quiver-confluence --help
```

Die Sites legt der User im quiver-Fenster unter **Tools → Confluence** an
(`mehrfach` = „Sites“/„Site“). quiver übergibt sie als JSON in
`QUIVER_ZUGAENGE`. Felder einer Site:

| Feld | Art | Pflicht | Bedeutung |
|---|---|---|---|
| `url` | URL | ja | `https://firma.atlassian.net` – ohne `/wiki` |
| `email` | Text | ja | E-Mail des Atlassian-Kontos (Teil der Basic-Anmeldung) |
| `api_token` | Passwort | ja | API-Token von `id.atlassian.com` (gilt für alle Atlassian-Produkte) |
| `nur` | Text | nein | erlaubte Bereichsschlüssel, kommagetrennt; gilt nur fürs Schreiben |
| `schreiben` | Schalter | nein | Standard **aus**; ohne ihn antworten die verändernden Werkzeuge ohne Anfrage |

Jedes Werkzeug nimmt `verbindung`; bei genau einer Site darf der Name fehlen
(Groß-/Kleinschreibung egal).

## Werkzeuge

### Lesend (`readOnlyHint`)

| Werkzeug | Wirkung |
|---|---|
| `confluence_status` | die eingetragenen Sites – ohne Token und ohne E-Mail, ohne Verbindung |
| `confluence_testen` | wer ist angemeldet (ältere API) |
| `confluence_bereiche` | Bereiche (Spaces), optional `schluessel` (kommagetrennt) |
| `confluence_suchen` | CQL über die Suche der älteren API; Komfort `text` und `bereich` werden maskiert zur Abfrage |
| `confluence_seite` | per `id` oder `bereich` + `titel`; Inhalt als Text, `als_storage` für das Original |
| `confluence_unterseiten` | die Unterseiten einer Seite |
| `confluence_kommentare` | die Fußkommentare einer Seite |
| `confluence_anhaenge` | die Anhänge einer Seite – nur die Liste, kein Herunterladen |
| `confluence_labels` | die Labels einer Seite |

### Verändernd (`destructiveHint`, brauchen `schreiben`)

| Werkzeug | Wirkung |
|---|---|
| `confluence_seite_anlegen` | legt eine Seite an (Bereich, Titel, Text, optional Elternseite) |
| `confluence_seite_aendern` | ändert Titel und Inhalt; `version` ist Pflicht, `anhaengen` hängt an |
| `confluence_kommentieren` | schreibt einen Fußkommentar |
| `confluence_label_hinzufuegen` | hängt ein globales Label an (ältere API) |

### Vertrag mit der offiziellen OpenAPI

Maßgeblich sind die offiziellen Dokumente für Confluence Cloud
(`confluence-v2.json` für `/wiki/api/v2`, `confluence-v1.json` für
`/wiki/rest/api`); die Vorlage ist nur Material. Jede Anfrage (Pfad, Methode,
Parametername, Pflichtfelder im Rumpf) und **jedes gelesene Antwortfeld** steht
dort. Die Attrappe antwortet in genau diesen Formen und lehnt unbekannte
Parameter, falsche Methoden und fehlende Pflichtfelder mit `400` ab.

Pfade ohne führendes `/wiki` sind relativ zu `${url}/wiki/api/v2`.

| Werkzeug | Methode | Pfad | Parameter / Rumpf | gelesene Antwortfelder |
|---|---|---|---|---|
| `confluence_status` | – | – | – | nur `QUIVER_ZUGAENGE` |
| `confluence_testen` | GET | `/wiki/rest/api/user/current` | – | `displayName`, `publicName`, `accountId` |
| `confluence_bereiche` | GET | `/wiki/api/v2/spaces` | query `keys`, `limit`, `cursor` | `results[].id/key/name/type/status`, `_links.next` |
| `confluence_suchen` | GET | `/wiki/rest/api/search` | query `cql` (Pflicht), `limit`, `cursor` | `results[].content.id/type/title/space.key`, `results[].title`, `results[].entityType`, `results[].excerpt`, `_links.next` |
| `confluence_seite` (Id) | GET | `/wiki/api/v2/pages/{id}` | query `body-format=storage` | `id`, `title`, `status`, `spaceId`, `version.number`, `version.createdAt`, `body.storage.value`, `_links.webui` |
| `confluence_seite` (Titel) | GET | `/wiki/api/v2/spaces` → `/wiki/api/v2/pages` → `/wiki/api/v2/spaces/{spaceId}` | query `keys`+`limit`; `space-id`, `title`, `body-format`, `limit` | wie oben, dazu `results[]`, `key` |
| `confluence_unterseiten` | GET | `/wiki/api/v2/pages/{id}/direct-children` | query `limit`, `cursor` | `results[].id/title/type/status`, `_links.next` |
| `confluence_kommentare` | GET | `/wiki/api/v2/pages/{id}/footer-comments` | query `body-format=storage`, `limit`, `cursor` | `results[].id`, `results[].version.authorId`, `results[].version.createdAt`, `results[].body.storage.value`, `_links.next` |
| `confluence_anhaenge` | GET | `/wiki/api/v2/pages/{id}/attachments` | query `limit`, `cursor` | `results[].id/title/mediaType/fileSize/version.number`, `_links.next` |
| `confluence_labels` | GET | `/wiki/api/v2/pages/{id}/labels` | query `limit`, `cursor` | `results[].id/name/prefix`, `_links.next` |
| `confluence_seite_anlegen` | GET + POST | `/wiki/api/v2/spaces` → `/wiki/api/v2/pages/{eltern}` → `/wiki/api/v2/pages` | query `keys`+`limit`; Rumpf `spaceId` (**Pflicht**), `status`, `title`, `body.representation`+`body.value`, `parentId` | `results[].id/key`; `spaceId` der Elternseite; `id`, `title`, `status`, `version.number`, `_links.webui` |
| `confluence_seite_aendern` | GET + PUT | `/wiki/api/v2/pages/{id}` (GET) → `/wiki/api/v2/pages/{id}` (PUT), ggf. `/wiki/api/v2/spaces/{spaceId}` | query `body-format=storage`; Rumpf `id`, `status`, `title`, `body`, `version` (**alle Pflicht**) | `version.number`, `title`, `status`, `body.storage.value`, `spaceId`; `key` |
| `confluence_kommentieren` | GET + POST | `/wiki/api/v2/pages/{id}` → `/wiki/api/v2/footer-comments` | Rumpf `pageId`, `body` | `spaceId`; `id` |
| `confluence_label_hinzufuegen` | GET + POST | `/wiki/api/v2/pages/{id}` → `/wiki/rest/api/content/{id}/label` | Rumpf `[{ prefix, name }]` (beide **Pflicht**) | `spaceId`; `results[].id/name/prefix` |

Nicht gelesen (obwohl die API sie liefert): `email` des Benutzers, `downloadLink`
und `webuiLink` der Anhänge sowie alle `_links` außer `next` und `webui`.

### Annahmen, die die Spezifikation offen lässt

- **Die Basisadresse ist `schema://host[:port]`.** `url` darf keinen Pfad, keine
  Abfrage und kein Fragment enthalten; `/wiki` hängt das Tool selbst an.
- **`_links.next` ist laut Spezifikation eine *relative* URL**
  (`MultiEntityLinks.next`). Sie wird mit `Url::join` gegen die Site-Adresse
  aufgelöst; die Suche (v1) trägt ihre Parameter in derselben Zeichenkette.
- **`POST /pages/{id}/labels` gibt es in v2 nicht** (dort nur `GET`). Labels
  schreibt darum die ältere API: `POST /wiki/rest/api/content/{id}/label` mit
  `[{ "prefix": "global", "name": … }]` (`LabelCreate`, `prefix` und `name`
  sind Pflicht).
- **`body` in `POST /pages` ist laut Spezifikation optional**; die Attrappe
  verlangt es, weil das Werkzeug immer einen Inhalt schickt.
- **`space-id` und `title` in `GET /pages` sind laut Spezifikation optional**;
  die Attrappe verlangt beide, weil der Titel-Weg des Werkzeugs immer beide
  schickt (sonst wäre es eine ungefilterte Liste).
- **`version.number` beim Ändern** muss die gelesene Version sein; die API
  verlangt eine ganze Zahl, das Werkzeug eine Zahl ab 1 und prüft den Überlauf.
- **`status` beim Ändern** ist laut Spezifikation `current` oder `draft`;
  andere Werte lehnt das Werkzeug ohne Anfrage ab.

Dazu kommt `confluence_profile` aus dem Kern (zeigt die Sites ohne Geheimnisse).

Lange Texte werden über `max_zeichen` gekürzt (Standard 4000, mit Hinweis, wie
viel fehlt); Listen nehmen `anzahl` (Standard 25, höchstens 100) und sagen, ob
es mehr gibt. Jede Antwort nennt die Ids und Schlüssel, mit denen der Agent
weiterarbeiten kann.

## Sicherheit

Die Regeln aus [12-atlassian-gemeinsam.md](../../docs/aufgaben/12-atlassian-gemeinsam.md)
sind im Code durchgesetzt und je einzeln getestet:

1. **Nur https.** `http://` gilt ausschließlich für `127.0.0.1`, `localhost`
   und `[::1]` (die Attrappe). Geprüft wird über `url::Url`; Adressen mit
   Benutzerteil, Pfad, Abfrage oder Fragment werden abgelehnt (auch
   `http://evil.test\\@127.0.0.1/` – der URL-Leser sieht dort den Host
   `evil.test`).
2. **Nur der eingetragene Host.** Der Client folgt keiner Weiterleitung
   (`redirect::Policy::none`). Jede vollständige Adresse (auch `_links.next`)
   wird über `url::Url` geparst und mit Schema, Host und Port der Site
   verglichen; ein Benutzerteil oder Fragment lehnt der Client ab – **ohne**
   Anfrage.
3. **Kein Geheimnis in einer Antwort.** E-Mail und Token stehen nur in der
   Basic-Kopfzeile an den eigenen Host. `Debug` von `Site` und `Client` zeigt
   nur, *ob* beides gesetzt ist; `confluence_status` nennt beides nie.
4. **Werte im Pfad sind kodiert.** `kodieren` lässt nur `A-Z a-z 0-9 - _ ~`
   stehen; `/`, `.`, `?` und `#` werden prozent-kodiert, ein Wert des Agenten
   kann also keinen anderen Endpunkt ansprechen.
5. **Leere Werte und Punkt-Segmente werden vorher abgelehnt.** `%2E%2E`
   normalisiert der URL-Leser trotzdem zu `..`; `pfad_wert` verbietet darum
   leere Werte, `.`, `..` und `...` **ohne** Anfrage, und `ziel` vergleicht die
   Segmentzahl der fertigen `Url` mit dem gebauten Pfad. Geprüft wird also die
   fertige Adresse, nicht nur die Kodierung.
6. **Ein unlesbarer Rumpf ist ein Fehler, nie „leer“.** Scheitert
   `text().await` bei einem lesenden Aufruf, kommt „Antwort nicht lesbar“. Bei
   einer Änderung mit 2xx heißt es „ausgeführt, Antwort nicht lesbar“; kommt
   gar keine Antwort (Zeitüberschreitung, Abbruch), heißt es „Ausgang unklar –
   Stand prüfen, nicht einfach wiederholen“.
7. **`schreiben` je Site, Standard aus.** Ist es aus, antworten alle vier
   verändernden Werkzeuge mit dem Hinweis und **ohne** Anfrage.
8. **`nur` je Site.** Ist es gesetzt, wird ein Schreiben außerhalb der
   freigegebenen Bereiche ohne verändernde Anfrage abgelehnt. Den Schlüssel
   liest das Werkzeug **vom Server** (Bereich der Seite bzw. `spaces?keys=…`),
   nie aus dem Aufruf; verglichen wird ohne Groß-/Kleinschreibung. Der erste
   Treffer von `spaces?keys=…` zählt nur, wenn sein `key` dem gesuchten
   entspricht; fehlt `key` in der Antwort, ist das ein Fehler (die Id ist kein
   Schlüssel).
9. **401/403/429/503.** 401 → „Token für <Site> ungültig oder abgelaufen – im
   quiver-Fenster erneuern“, kein Wiederholen. 403 → Hinweis auf fehlende
   Rechte. 429 und 503 werden **nur bei GET** und höchstens zweimal wiederholt
   (`Retry-After`, höchstens 30 s).
10. **204 ist Erfolg.** Ein leerer Rumpf nach einer Änderung ist kein
    Lesefehler.

### Ändern verliert nichts

`confluence_seite_aendern` liest die Seite zuerst (Version, Titel, Status,
Inhalt), vergleicht die vom Agenten genannte `version` mit der auf dem Server
und schreibt nur bei Gleichheit mit `version.number + 1`. Weicht sie ab oder
antwortet der Server mit `409`, kommt „Seite wurde inzwischen geändert – neu
lesen“ und es wird **nichts** überschrieben.

Vier Fälle brechen **ohne** `PUT` ab, weil sie Daten verlieren würden:

- **`anhaengen: true`, aber die Antwort nennt keinen `body.storage.value`:**
  ein Anhängen an einen leeren alten Inhalt würde die Seite ersetzen. Es kommt
  ein Fehler, kein `PUT`. Mit `text` bleibt der alte Storage-Inhalt
  **bytegleich**, der neue kommt direkt danach.
- **Kein `title` in der Antwort und keiner im Aufruf:** es wird nie
  „(ohne Titel)“ gesendet.
- **Kein `status` in der Antwort:** der Status ist Pflichtfeld im `PUT`-Rumpf,
  also wird nichts geschrieben.
- **`version` ist keine ganze Zahl ab 1** oder `version.number + 1` läuft über:
  Parameterfehler bzw. Abbruch vor der Anfrage.

`status` wird nur laut Spezifikation gesendet (`current` oder `draft`); ohne
Angabe bleibt der gelesene Status, eine veröffentlichte Seite wird also nie
unbeabsichtigt zum Entwurf. `eltern` beim Anlegen wird gelesen: liegt die
Elternseite in einem anderen Bereich, kommt eine Meldung und **kein** `POST`.

`text` wird HTML-maskiert in Absätze gesetzt (`&`, `<`, `>`, `"`, `'`), `storage`
geht unverändert an Confluence. Genau eines von beiden ist Pflicht.

## Annahmen zur API (Confluence Cloud)

- **Basis ist die API v2** (`/wiki/api/v2`). Die **Suche über CQL** liegt nur in
  der älteren API (`/wiki/rest/api/search`), ebenso „wer ist angemeldet“
  (`/wiki/rest/api/user/current`); beides wird so benutzt.
- **Seiteninhalte** kommen mit `?body-format=storage`. Ein Seitenaufruf macht
  eine zweite Anfrage (`/spaces/{spaceId}`), damit die Antwort den
  **Bereichsschlüssel** statt nur der Bereichs-Id nennt.
- **Kommentare** sind Fußkommentare (`/pages/{id}/footer-comments`), keine
  Inline-Kommentare. Die v2-Antwort trägt den Autor nur als
  `version.authorId` (Konto-Id), nicht als Anzeigename – ein Name bräuchte eine
  weitere Anfrage je Kommentar.
- **Anhänge** werden nur gelistet; es gibt keinen Upload und keinen Download.
- **Neue Seiten** entstehen mit `status: "current"`; `parentId` setzt die
  Elternseite (`eltern`), die vorher gelesen und auf denselben Bereich geprüft
  wird.
- **Labels** werden global geschrieben (`prefix: "global"`) und über die
  ältere API angehängt; gelesen werden sie über v2.
- **Folgeseiten**: Der Client blättert selbst weiter, bis `anzahl` erreicht ist
  oder der Server keine `_links.next` mehr liefert (höchstens 20 Seiten).
- **Datumsangaben** werden auf `JJJJ-MM-TT` bzw. `JJJJ-MM-TT hh:mm` gekürzt.
- **Storage-Format → Text** ist ein eigener, getesteter Leser (Tags,
  Entities, Listen, Tabellen, Makros); Absatzgrenzen werden zu Zeilen. Der
  Rückweg ist verlustfrei für Text, nicht für Auszeichnung.

## Tests

```bash
cargo fmt --all --check
cargo clippy -p quiver-confluence --all-targets -- -D warnings
cargo test -p quiver-confluence
```

Geprüft wird ausschließlich gegen die Attrappe (`tests/attrappe/mod.rs`, axum):
ein Erfolg je Werkzeug, jede Sicherheitsregel einzeln (https, Weiterleitung,
fremde Folgeseite, Geheimnisse, Prozent-Kodierung, Punkt-Segmente, `schreiben`
aus, `nur`, 401, 403, 429/503 nur bei GET, 204) und der Kindprozess über stdio
samt `quiver-confluence eintrag`.

Dazu kommen die Regeln dieser Runde, jede mit einem Test, der mit dem alten
Code rot war: `anhaengen` ohne alten Inhalt, fehlender Titel, `version` 0 und
Überlauf, `status` außerhalb der Spezifikation, falscher bzw. fehlender
Bereichsschlüssel in der Antwort, Elternseite im fremden Bereich, Label über
die ältere API und ein unlesbarer Antwortrumpf. Für den unlesbaren Rumpf hat die
Attrappe einen winzigen Rohserver, dessen `Content-Length` nicht zum Rumpf
passt; für „keine Antwort“ prüft ein Unit-Test einen geschlossenen Port. Die Attrappe ist streng: unbekannter Pfad oder
Parameter `400`, falscher `Authorization`-Kopf `401`, fehlende Pflichtfelder
`400`. Kein Test spricht je mit einem echten Confluence.

## Offen

- **Confluence Data Center / Server** (REST v1, `/rest/api`) ist nicht
  unterstützt; das Tool spricht Cloud (v2 plus die beiden v1-Endpunkte).
- **OAuth (3LO) und Forge** gibt es nicht; die Anmeldung ist Basic mit
  E-Mail und API-Token.
- **Anhänge hochladen und herunterladen** fehlt (Aufgabe nennt nur die Liste).
- **Inline-Kommentare** und das Beantworten von Kommentaren fehlen.
- **Seiten löschen, verschieben, umbenennen** sowie
  **Versionen/Wiederherstellen** fehlen. `status: "draft"` ist erlaubt und
  getestet, aber ein Entwurf ist kein eigener Werkzeugweg.
- **Blogposts** und **Inhalte außerhalb von Seiten** zeigt nur die Suche; eigene
  Werkzeuge dafür gibt es nicht.
- **Kommentar-Autoren** erscheinen als Konto-Id (`authorId`), nicht als Name –
  v2 liefert keinen Anzeigenamen.
- **Seiten lesen kostet zwei Anfragen** (Seite plus Bereich für den Schlüssel).
  Ein Cache je Site wäre eine Anfrage weniger, hätte aber einen veralteten
  Bereichsschlüssel zur Folge.
- **Seitenlimits der API**: Die Suche erlaubt höchstens 100 Treffer je Seite, v2
  höchstens 250; das Tool fragt nie mehr als 100 an.
- **`tracing`-Ereignisse** des Werkzeugs sind stumm, weil der Kern keinen
  Subscriber setzt.
- **Confluence-Editor-Inhalte** (Aufgabenlisten, Erweiterungen, Makro-Parameter)
  verlieren beim Lesen ihre Struktur; der Text bleibt, die Auszeichnung nicht.
