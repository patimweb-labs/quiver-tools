# 0009 – Confluence Cloud: Sites als Liste, v2 plus CQL, Schreiben mit Standard aus

> Stand: 2026-11-24. Aufgabe: [docs/aufgaben/14-confluence.md](../../docs/aufgaben/14-confluence.md).
> Baut auf [0002-zugang-ueber-quiver-felder.md](0002-zugang-ueber-quiver-felder.md) und
> [0007-devops-mit-pat.md](0007-devops-mit-pat.md) auf; die gemeinsamen Regeln der drei
> Atlassian-Tools stehen in [12-atlassian-gemeinsam.md](../../docs/aufgaben/12-atlassian-gemeinsam.md).

## Kontext

`quiver-confluence` soll Confluence Cloud bedienen: Bereiche, Seiten, Unterseiten,
Kommentare, Anhänge, Labels und die Suche. Die Vorlage
`pi-confluence-multiproject` (TypeScript) tut das mit `fetch` über v2 und v1; für ein
eigenes Rust-Programm gibt es kein SDK, die REST-API ist aber vollständig dokumentiert.

Vier Fragen standen am Anfang:

1. **Wie meldet sich das Tool an?** Confluence Cloud kennt Basic (E-Mail + API-Token),
   OAuth 3LO und Forge-Apps.
2. **Wie viele Zugänge?** Ein Nutzer arbeitet in der eigenen Firma **und** beim Kunden,
   jeweils mit eigener Cloud-Site.
3. **Welche API?** Inhalte, Bereiche, Kommentare und Labels liegen in v2; die Suche über
   CQL und „wer ist angemeldet“ nur in der älteren API.
4. **Darf ein Agent schreiben?** Seiten anlegen und ändern, Kommentare und Labels sind
   echte Änderungen an einem System, in dem andere Menschen arbeiten.

## Entscheidung

### 1. Basic mit E-Mail und API-Token

Jede Site ist ein Zugang mit `url`, `email`, `api_token`, optional `nur` und dem Schalter
`schreiben`. Die Anmeldung ist HTTP Basic mit der E-Mail als Benutzer und dem Token als
Passwort (`Authorization: Basic base64("email:token")`) – genau das, was die Vorlage
`buildBasicAuthHeader` tut. Ein API-Token gilt für alle Atlassian-Produkte des Kontos;
der Hilfe-Link zeigt darum auf dieselbe Seite wie bei Jira
(`https://id.atlassian.com/manage-profile/security/api-tokens`).

**Warum nicht OAuth:** Ein 3LO-Ablauf bräuchte eine App-Registrierung, einen Browser, eine
Rückleitung und ein Token mit Ablauf und Refresh. Der Nutzer stellt ein API-Token in
Sekunden selbst aus; es liegt in der Tresor-Datei von quiver und wird von dort als
Umgebungsvariable gelesen. **Preis:** ein langlebiges Geheimnis mit den Rechten des
Besitzers; erneuern muss es der Mensch.

### 2. Eine Liste von Sites (`mehrfach`)

Wie `quiver-devops` pflegt der User eine Liste: `mehrfach = ("Sites", "Site")`, quiver
übergibt sie als JSON in `QUIVER_ZUGAENGE`. Jedes Werkzeug nimmt `verbindung`; bei genau
einer Site darf der Name fehlen. Ein Bibliothekseintrag je Kunde wäre unübersichtlich,
und die Felder sind für alle Sites dieselben.

### 3. v2 als Basis, v1 nur wo nötig

Inhalte, Bereiche, Unterseiten, Kommentare, Anhänge und Labels laufen über
`${url}/wiki/api/v2`. Zwei Dinge gibt es dort nicht:

- die **Suche über CQL** (`/wiki/rest/api/search`),
- **„wer ist angemeldet“** (`/wiki/rest/api/user/current`).

Beides bleibt in der älteren API; ein eigener Suchparser oder ein „wer bin ich“-Ersatz
über eine beliebige Liste wäre schlechter als zwei klar benannte Endpunkte. Die
Annahmen dazu stehen in [crates/confluence/README.md](../../crates/confluence/README.md).

### 4. Schreiben nur mit ausdrücklichem Schalter, Standard **aus**

Das vierte Feld ist `schreiben` (Schalter, Standard `aus`). Ist es aus, antworten alle
vier verändernden Werkzeuge mit „Schreiben ist für <Site> aus – im quiver-Fenster
einschalten.“ und schicken **keine** Anfrage.

**Warum Standard aus:** eine Kunden-Site ist fast immer ein Produktivsystem. Nichts zu
sagen muss die sichere Antwort sein. Der Preis ist ein Klick im quiver-Fenster, bevor der
Agent das erste Mal kommentiert.

### 5. `nur`: Bereiche, die der Server bestätigt

Das fünfte Feld `nur` (Text, kommagetrennt) begrenzt das **Schreiben** auf
Bereichsschlüssel. Ist es gesetzt, liest das Werkzeug den Bereich **vom Server**:

- beim Anlegen über `GET /spaces?keys=…` (die Antwort liefert Id und Schlüssel),
- beim Ändern, Kommentieren und Labeln über die Seite (`spaceId`) und
  `GET /spaces/{spaceId}`.

Dem Agenten wird nicht geglaubt: er nennt beim Anlegen einen Schlüssel, aber die
Freigabe entscheidet der zurückgegebene Schlüssel; beim Ändern steht der Bereich nur in
der Seite. Verglichen wird ohne Groß-/Kleinschreibung.

**Preis:** eine zusätzliche Anfrage je Schreibvorgang, wenn `nur` gesetzt ist; ohne `nur`
entfällt sie.

### 6. Ändern verliert nichts: `version` ist Pflicht

`confluence_seite_aendern` liest die Seite (Version, Titel, Status, Storage-Inhalt),
vergleicht die vom Agenten genannte `version` mit der auf dem Server und schreibt nur bei
Gleichheit – mit `version.number + 1`, dem bisherigen Titel und dem bisherigen Status,
sofern der Aufruf sie nicht ausdrücklich ändert. Weicht die Version ab oder meldet der
Server `409`, kommt „Seite wurde inzwischen geändert – neu lesen“; geschrieben wird
nichts.

**Warum die Version vom Agenten und nicht automatisch:** Ein automatisches Nachziehen
wäre ein Wettlauf – zwischen Lesen und Schreiben kann der Mensch im Editor speichern.
Wenn der Agent die Version mitgibt, die er tatsächlich gelesen hat, ist die Absicht
eindeutig, und ein Konflikt wird zur Meldung statt zu einem stillen Überschreiben. Der
Preis ist ein Pflichtparameter, den nur `confluence_seite` liefert.

### 7. `text` wird maskiert, `storage` nicht angefasst

Der Agent liefert entweder `text` oder `storage` – genau eines. `text` wird **HTML-maskiert**
(`&`, `<`, `>`, `"`, `'`) und in Absätze gesetzt (Leerzeile = Absatz, Zeilenumbruch =
`<br />`); `storage` geht unverändert an Confluence, weil der Agent dann ausdrücklich das
Storage-Format liefert. Ein zweiter Weg (Markdown → Storage, ADF → Storage) wäre ein
eigener Konverter und eine eigene Fehlerquelle; er steht in „Offen“.

### 8. Grenzen, die im Client liegen

- **Keine Weiterleitung** (`redirect::Policy::none`): die Basic-Kopfzeile geht nur an den
  eingetragenen Host. Ein `302` wird zur Fehlermeldung.
- **Fremde Folgeseiten** (`_links.next` mit anderem Schema oder Host) werden **ohne
  Anfrage** abgelehnt; verglichen wird Schema plus `host:port`, case-insensitiv.
- **Werte im Pfad** werden prozent-kodiert, auch `/`, `.`, `?` und `#` – ein Wert des
  Agenten kann keinen anderen Endpunkt ansprechen.
- **401/403** sind endgültig und werden nicht wiederholt. **429/503** werden höchstens
  zweimal und **nur bei GET** wiederholt (`Retry-After`, höchstens 30 s). Ein Anlegen oder
  Ändern wird nie wiederholt: ob es ankam, weiß nur der Server.
- **204 ohne Rumpf** nach einer Änderung ist Erfolg, kein Lesefehler.
- Texte aus dem Storage-Format werden zu Text und über `max_zeichen` gekürzt (Standard
  4000); Listen zeigen `anzahl` (Standard 25, höchstens 100) und sagen, ob es mehr gibt.

## Preis

- **Ein Token je Site, vom Menschen gepflegt.** Läuft es ab, antwortet das Tool mit dem
  Hinweis auf das quiver-Fenster; es erneuert nichts selbst.
- **Die Rechte des Tokens sind die Rechte des Tools.** Wer ein Token mit Schreibrechten
  hinterlegt, gibt sie dem Agenten – `schreiben` ist die zweite Bremse, keine
  Rechtetrennung. Wer trennen will, gibt ein reines Lese-Token.
- **`nur` ist eine Grenze im Tool, nicht in Confluence.** Wer die Bereichsliste umgeht,
  indem er ein Token mit Schreibrechten in einem anderen Bereich hinterlegt, wird vom
  Tool nicht daran gehindert – die Prüfung gilt für dieses Programm.
- **Kommentar-Autoren sind Konto-Ids.** v2 liefert `authorId`, keinen Anzeigenamen; ein
  Name bräuchte eine Anfrage je Kommentar.
- **Seiten lesen kostet zwei Anfragen** (Seite und Bereich). Ein Cache je Site wäre
  sparsamer, könnte aber einen veralteten Schlüssel für die `nur`-Prüfung liefern.

## Grenzen, die bewusst bleiben

Nicht übernommen und in [crates/confluence/README.md](../../crates/confluence/README.md)
unter „Offen“ vermerkt: Confluence Data Center, OAuth/Forge, Anhänge hoch- und
herunterladen, Inline-Kommentare, Löschen/Verschieben, Versionen und Wiederherstellen,
Blogposts sowie Markdown → Storage.
