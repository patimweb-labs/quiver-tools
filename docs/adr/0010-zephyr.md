# 0010 – Zephyr Scale Cloud: mehrere Zugänge, Bearer-Token, Ergebnisse ohne Feldverlust

> Stand: 2026-11-24. Aufgabe: [docs/aufgaben/15-zephyr.md](../../docs/aufgaben/15-zephyr.md).
> Baut auf [0002-zugang-ueber-quiver-felder.md](0002-zugang-ueber-quiver-felder.md) und
> [0007-devops-mit-pat.md](0007-devops-mit-pat.md) auf. Gemeinsame Regeln für Jira,
> Confluence und Zephyr: [docs/aufgaben/12-atlassian-gemeinsam.md](../../docs/aufgaben/12-atlassian-gemeinsam.md).

## Kontext

`quiver-zephyr` soll Zephyr Scale Cloud über die REST-API v2 bedienen: Testfälle,
Testzyklen, Ausführungen, Status und Ordner lesen, einen Testfall anlegen, Testfälle einem
Zyklus zuordnen und ein Ergebnis eintragen. Die Vorlage `pi-zephyr-testmanager`
(TypeScript, MIT) tut das über `fetch`; für Rust gibt es kein SDK, die REST-API v2 ist aber
dokumentiert.

Vier Fragen standen am Anfang:

1. **Wie meldet sich das Tool an?** Zephyr Scale Cloud nimmt ein API-Token aus dem
   Jira-Profil („Zephyr API Access Tokens“) als Bearer.
2. **Wie viele Zugänge?** Ein Nutzer arbeitet regelmäßig in mehreren Jira-/Zephyr-Projekten,
   oft bei verschiedenen Kunden.
3. **Darf ein Agent schreiben?** Testfälle anlegen, einem Zyklus zuordnen und Ergebnisse
   eintragen sind echte Änderungen an Testnachweisen, die andere Menschen lesen.
4. **Was passiert mit einem Ergebnis?** `PUT /testexecutions/{id}` könnte ersetzen oder
   ergänzen – die Vorlage zeigt nur `statusName` und `comment`.

## Entscheidung

### 1. Bearer-Token statt OAuth oder Jira-Login

Jeder Zugang trägt `basis_url`, `api_token`, optional `projekt`, optional `nur` und den
Schalter `schreiben`. Die Anmeldung ist `Authorization: Bearer <api_token>` gegen die
Basisadresse `https://api.zephyrscale.smartbear.com/v2`.

**Warum:**

- Zephyr Scale Cloud stellt keine OAuth-App für Dritte bereit; das API-Token ist der
  vorgesehene Weg und wird pro Nutzer im Jira-Profil erzeugt und widerrufen.
- Zephyr Squad und Data Center bleiben draußen: Squad hat eine andere API, Data Center
  betreibt eine eigene Instanz mit anderer Anmeldung. Ein gemeinsames Tool würde beide
  Wege verwässern (siehe „Offen“ in [crates/zephyr/README.md](../../crates/zephyr/README.md)).

### 2. Eine Liste von Zugängen (`mehrfach`)

`mehrfach = ("Zugänge", "Zugang")`; quiver übergibt die Einträge als JSON in
`QUIVER_ZUGAENGE`. Jedes Werkzeug nimmt `verbindung`; bei genau einem Zugang darf der Name
fehlen. Das Standardprojekt steht am Zugang oder im Aufruf; fehlt beides, antwortet das
Werkzeug mit einem Hinweis und **ohne** Anfrage.

### 3. Schreiben nur mit Schalter, `nur` als zweite Bremse

`schreiben` ist Standard **aus**. Ist er aus, antworten die drei verändernden Werkzeuge mit
„Schreiben ist für <verbindung> aus – im quiver-Fenster einschalten.“ und schicken keine
Anfrage. Das optionale Feld `nur` nennt erlaubte Projektschlüssel; ist es gesetzt, wird
außerhalb nicht geschrieben.

**Den Projektschlüssel glaubt das Werkzeug nicht dem Agenten**, sondern liest ihn vom
Server: beim Anlegen aus der Projektliste, beim Zuordnen aus dem Testzyklus, beim Ergebnis
aus der Ausführung. Verglichen wird ohne Groß-/Kleinschreibung. Die Absage kommt, **bevor**
eine verändernde Anfrage entsteht; die lesende Anfrage davor ist erlaubt und nötig.

### 4. Ergebnisse lesen, dann senden

`zephyr_ergebnis_eintragen` liest die Ausführung, prüft den Statusnamen gegen
`/statuses?projectKey=…` und sendet **alle gelesenen Felder** wieder mit (`projectKey`,
`testCaseKey`, `testCycleKey`, `environmentName`, `executionTime`, `actualStartDate`,
`actualEndDate`, `automated`, `customFields`, `comment`). Damit ist es gleichgültig, ob
`PUT` ersetzt oder ergänzt; die Annahme steht in der README. Ein unbekannter Status endet
mit den erlaubten Namen und ohne Änderung.

### 5. Grenzen im Client

- **Nur `https://`**, Ausnahme Loopback (`127.0.0.1`, `localhost`, `[::1]`) für die
  Attrappe.
- **Keine Weiterleitung** (`redirect::Policy::none`). Eine Antwort `3xx` und eine
  Folgeseite (`next`, `_links.next`) mit fremdem Host werden abgelehnt, bevor eine weitere
  Anfrage entsteht.
- **Werte in Pfaden** werden prozent-kodiert, auch `.`, `/`, `?` und `#`.
- **401** und **403** werden nicht wiederholt. **429/503** werden höchstens zweimal und nur
  bei lesenden Aufrufen wiederholt; `Retry-After` wird beachtet, aber mehr als 30 s nicht
  abgewartet. Verändernde Aufrufe werden nie wiederholt.
- **Ein leerer Rumpf (`204`)** nach einer Änderung ist Erfolg.

## Preis

- **Ein Token je Zugang, vom Menschen gepflegt.** Läuft es ab, antwortet das Werkzeug mit
  dem Hinweis auf das quiver-Fenster; es erneuert nichts selbst.
- **Die Rechte des Tokens sind die Rechte des Werkzeugs.** `schreiben` und `nur` sind
  Bremsen, keine Rechtetrennung. Wer wirklich trennen will, gibt ein Lesetoken.
- **Kein „wer bin ich“.** Zephyr hat keinen Endpunkt, der den Benutzer nennt; `testen`
  meldet darum nur, dass das Token gilt, und was sichtbar ist.
- **Ein Ergebnis kostet drei Anfragen** (lesen, Statusliste, `PUT`). Ein Cache je Projekt
  wäre schneller, könnte aber veralten.
- **Zählung je Status ist begrenzt** (5 Seiten à 100). Bei sehr großen Zyklen steht in der
  Antwort, aus wie vielen von wie vielen gelesenen Ausführungen gezählt wurde.
- **Testfall-Schritte und -Skripte werden nur gelesen.** Das Ändern bleibt offen.

## Grenzen, die bewusst bleiben

Nicht übernommen und in [crates/zephyr/README.md](../../crates/zephyr/README.md) unter
„Offen“ vermerkt: Zephyr Squad und Data Center, Testschritte ändern, Testfälle ändern oder
löschen, Zyklen anlegen, Anhänge und benutzerdefinierte Felder beim Anlegen, die
Jira-Anbindung eines Testfalls.
