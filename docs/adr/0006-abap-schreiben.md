# 0006 – ABAP schreiben: Sitzung im Prozess, Schalter `schreiben` mit Standard aus, Kundennamensraum

> Stand: 2026-11-23. Aufgabe: [docs/aufgaben/08-abap-schreiben.md](../../docs/aufgaben/08-abap-schreiben.md).
> Baut auf [0005-adt-client-in-rust.md](0005-adt-client-in-rust.md) auf.

## Kontext

Teil 1 (`quiver-abap`) liest ABAP über SAP ADT und ist bewusst **zustandslos**: jeder
Aufruf holt Token und Cookies, arbeitet und ist fertig. Teil 2 braucht sechs schreibende
Werkzeuge (`abap_lock`, `abap_unlock`, `abap_write`, `abap_activate`,
`abap_create_object`, `abap_set_text_elements`). Damit kommen drei Fragen, die Teil 1
nicht hatte:

1. **Wo lebt ein Lock-Handle?** SAP kennt einen Handle nur in der Sitzung, die ihn
   geholt hat (Cookie `SAP_SESSIONID_*` plus `X-sap-adt-sessiontype: stateful`).
2. **Wie verhindert man Schaden?** Ein Agent, der ein Produktivsystem bearbeitet, darf
   nicht „mal eben“ eine SAP-Standardklasse überschreiben.
3. **Was passiert beim Abbruch?** Ein Prozess kann mitten im Schreiben enden – dann
   bleiben Sperren im System stehen und blockieren Kollegen.

## Entscheidung

### 1. Eine stateful Sitzung je System, im Prozess

`adt::sitzung::Sitzungen` hält je System **eine** `Sitzung`: einen `Client` mit
`stateful` (Cookies, CSRF-Token) und eine Tabelle `objectUrl → lockHandle`.
`abap_lock`, `abap_write`, `abap_unlock`, `abap_activate` und
`abap_set_text_elements` laufen darüber; die lesenden Werkzeuge bleiben zustandslos und
bekommen nie eine Sitzung zu sehen.

Die Sitzung entsteht beim ersten schreibenden Aufruf (ohne Anfrage – `Client::neu` ist
lokal) und lebt bis zum Prozessende. Ein Lock-Handle, das nicht in dieser Tabelle steht,
wird **ohne** Anfrage an SAP abgelehnt: „Lock-Handle unbekannt – erst abap_lock
aufrufen“.

Läuft die Sitzung ab (`400`/`401` mit „Session timed out“), wird sie samt Lock-Tabelle
verworfen; die Antwort sagt „Sitzung abgelaufen, Locks verloren – erneut abap_lock“.
Ein Schreibvorgang wird **nie** stillschweigend wiederholt: der Agent entscheidet, ob er
erneut sperrt und schreibt.

### 2. Schalter `schreiben` je System, Standard **aus**

Das System bekommt ein siebtes Feld `schreiben` (Schalter, Standard `aus`, letztes Feld
der Liste). Ist es aus, antwortet **jedes** schreibende Werkzeug mit dem Hinweis auf das
quiver-Fenster und schickt **keine** Anfrage an SAP.

### 3. Nur Kundennamensraum

Geschrieben, angelegt und gesperrt werden nur Objekte mit `Z*`, `Y*` oder
`/…/`-Namensraum; bei `abap_create_object` zusätzlich nur Pakete mit derselben Regel
(`$TMP` ist erlaubt). Alles andere wird ohne Anfrage abgelehnt. `source` ist auf 2 MB
begrenzt, ebenfalls vor der Anfrage geprüft.

### 4. Locks werden freigegeben

- `abap_write`/`abap_set_text_elements` **ohne** `lockHandle`: sperren, schreiben,
  entsperren – das Entsperren läuft auch, wenn das Schreiben scheitert (wie `finally` in
  der Vorlage).
- `abap_write` **mit** `lockHandle`: die Sperre bleibt stehen, der Aufrufer besitzt sie.
- Beim Prozessende (stdin zu oder normales Ende) gibt `Drop` des Servers alle bekannten
  Locks frei, auf Unix zusätzlich ein SIGTERM-Wächter. Frist: 5 s; Fehler nur ins Log
  (`tracing`), nie in eine Antwort.

## Warum so

1. **Das Protokoll lässt keine Wahl.** Ein Lock-Handle ohne die zugehörige Sitzung ist
   wertlos; SAP beantwortet ein Schreiben dann mit `423`. Entweder hält der Prozess die
   Sitzung, oder jeder Schreibvorgang bräuchte einen Neuanlauf von Sperren bis
   Aktivieren in einem einzigen Aufruf – dann gäbe es aber kein `abap_lock`/`abap_write`
   mehr, wie die Vorlage sie hat.
2. **Standard aus ist die einzige sichere Vorgabe.** Der User schaltet bewusst ein, für
   welches System er schreiben will. Ein vergessenes Feld kann so keinen Produktivstand
   verändern; der Preis ist ein Klick im quiver-Fenster.
3. **Der Namensraum ist die Grenze, die SAP selbst zieht.** `Z*`/`Y*`/`/…/` sind die
   Objekte, die Kunden gehören. Eine Prüfung im Tool ist billig und wirkt auch dann,
   wenn ein Agent einen Namen raten muss; sie ersetzt keine Berechtigung im System.
4. **Kein stilles Wiederholen.** Nach einem Sitzungsablauf ist unklar, ob der
   Schreibvorgang ankam. Ein automatischer zweiter Versuch könnte einen halben Stand
   doppelt schreiben – der Agent bekommt stattdessen die Wahrheit und entscheidet.
5. **Freigeben ist Pflicht.** Stehen gebliebene Sperren blockieren andere Benutzer; sie
   sind der teuerste Fehler dieses Werkzeugs. Darum `finally`-Verhalten, plus Freigabe
   beim Prozessende.

## Preis

- **Prozesszustand.** Die Sitzung lebt im Speicher; ein Neustart des Programms verliert
  Sitzung und Lock-Tabelle. Offene Sperren gibt der Prozess beim Ende frei – wird er
  hart abgeschossen (`SIGKILL`, Stromausfall), bleibt eine Sperre stehen und muss im SAP
  (SM12) gelöscht werden. Das steht in [offen.md](../offen.md).
- **Ein Schreiber je System.** Zwei parallele Schreibvorgänge teilen sich die Sitzung;
  ein zweites `abap_lock` auf dasselbe Objekt antwortet mit einem SAP-Fehler. Das ist die
  Semantik der Vorlage, nicht mehr und nicht weniger.
- **Der Schalter ist kein Schutz gegen Fehler.** Wer `schreiben` einschaltet, erlaubt
  einem Agenten echte Änderungen. Der Namensraum hält SAP-Standard draußen, nicht
  schlechten ABAP-Code; Aktivieren und Transport bleiben Aufgabe des Menschen.
- **SIGTERM-Wächter nur auf Unix.** Auf anderen Plattformen bleibt das normale Ende, für
  das `Drop` aufräumt; der Wächter ist ein `#[cfg(unix)]`-Zusatz und nicht durch einen
  Test abgedeckt (ein Signal lässt sich im Kindprozess-Test nicht portabel senden).

## Grenzen, die bewusst bleiben

- **Kein Anlegen von Transporten.** `transport`/`corrNr` wird mitgegeben; Aufträge
  anlegen, freigeben und löschen bleibt draußen.
- **Keine Inaktiv-Liste, kein Löschen, kein Umbenennen, kein Refactoring.** Die Vorlage
  hat mehr; Aufgabe 08 nennt sechs Werkzeuge.
- **Keine Wiederaufnahme nach Sitzungsablauf.** Kein automatisches Neusperren.
- **Ein Quelltext je Aufruf, höchstens 2 MB.** Kein Streaming, kein Teil-Schreiben.
