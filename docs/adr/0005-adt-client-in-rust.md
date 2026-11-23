# 0005 – ABAP: eigener ADT-Client in Rust statt Nachbau der Node-Bibliothek

> Stand: 2026-11-23. Aufgabe: [docs/aufgaben/07-abap-lesen.md](../../docs/aufgaben/07-abap-lesen.md).

## Kontext

Die Vorlage `pi-abap-fs` ist eine TypeScript-Erweiterung für pi; sie spricht SAP über die
MIT-lizenzierte Bibliothek `abap-adt-api`. `quiver-abap` ist ein eigenes Programm in
Rust und soll derselbe Werkzeugkasten sein: Objekte suchen und lesen, Tabellen ansehen,
Syntax prüfen, Unit-Tests und ATC laufen lassen, Dumps, Traces und Transporte lesen.

Drei Wege standen zur Wahl:

1. **Die Node-Bibliothek einbinden** (Sidecar, `napi`, Node-Prozess).
2. **Ein eigener Client in Rust**, Protokoll aus `abap-adt-api` abgelesen.
3. **Ein vorhandener Rust-Client** von crates.io.

## Entscheidung

**Weg 2: ein eigener, kleiner Client in Rust.** Die Anfragen sind abgelesen, nicht
übersetzt:

| Modul | Aufgabe | Vorbild in `abap-adt-api/build/` |
|---|---|---|
| `adt/client` | Basic-Auth, `sap-client`/`sap-language`, Cookies je System, CSRF, 401, `403 Required` → neu holen → einmal wiederholen | `AdtHTTP.js` |
| `adt/xml` | ein Baum aus `quick-xml`, Namensräume fallen weg | `utilities.js` (`fullParse`, `xmlArray`) |
| `adt/daten` | je Endpunkt ein reiner Leser XML → Daten | `api/*.js` |
| `adt/fehler` | `exc:exception` → Typ + Nachricht, sonst HTTP-Status | `AdtException.js` |
| `format` | reine Funktionen Daten → knapper Text | `src/formatting/formatters.ts` |

Die Endpunkte (Pfad, Abfraparameter, `Accept`, Rumpf) stehen als Konstanten und
Kommentare im Code; wer eine Frage zum Protokoll hat, findet die Antwort dort und in
`abap-adt-api`. Werkzeugnamen und Parameter folgen der Vorlage, damit Gewohnheiten und
Skills weiter passen; nur `max_zeichen` und `max_zeilen` sind deutsch, wie die Aufgabe
es verlangt.

**XML ohne `serde`.** Die Aufgabe nennt `quick-xml` „(+ serde)“; gebraucht wird `serde`
 für die Parameter der Werkzeuge, nicht für XML. ADT wechselt je Endpunkt Präfix und
Schreibweise (`adtcore:name` gegen `OBJECT_NAME`, Attribute gegen Elemente), und ein
Schema-Modell je Antwort wäre größer als die zwölf Felder, die wir lesen. `adt::xml`
liest deshalb in einen kleinen Baum und gibt Namensräume auf; die reinen Leser in
`adt::daten` greifen nur die Felder ab, die ein Werkzeug zeigt.

## Warum so

1. **Ein Programm, keine zweite Laufzeit.** `quiver-tools` ist ein Cargo-Workspace aus
   einem Binary je Tool (ADR 0001). Ein Node-Sidecar bräuchte Node auf jedem Rechner,
   eine zweite Fehlerbehandlung, einen zweiten Update-Pfad und eine IPC-Naht, an der
   Zugangsdaten vorbeikommen.
2. **Nur lesen, kleine Fläche.** Teil 1 braucht 19 lesende Aufrufe. Ein vollständiger
   Client (Debugger, Refactoring, abapGit, RAP-Generator) wäre zehnmal größer und
   müsste dauerhaft gegen neue SAP-Releases gepflegt werden. Was wir brauchen, sind
   wenige Pfade und XML-Felder.
3. **Kein Geheimnis im Text.** Anmeldung, Token und Cookies liegen an genau einer
   Stelle (`adt/client`); jede Antwort, jeder Fehler und jedes `Debug` ist auf
   Geheimnisfreiheit geprüft. Bei einer fremden Bibliothek wäre das nicht prüfbar.
4. **Prüfbar ohne SAP.** Weil die Anfragen im Code stehen, kann eine axum-Attrappe sie
   beantworten: CSRF, `403 Required`, 401, Cookies und ADT-Ausnahmen sind Testfälle –
   ohne ein einziges echtes System.
5. **Zertifikate ohne Schalter.** `reqwest` mit rustls und den Wurzeln des
   System-Zertifikatsspeichers (`rustls-native-certs`) erkennt Firmen-CAs;
   `zertifikat_pruefen` ist nur der Notausgang, nicht der Normalfall.

## Preis

- **Wir pflegen das Protokoll selbst.** Ändert SAP ein XML-Feld, merkt es niemand für
  uns. Dagegen stehen die Attrappe (sie hält die Erwartung fest) und die kleine Fläche.
- **`quick-xml` und `rustls-native-certs` sind neue Abhängigkeiten** (beide in der
  Aufgabe genannt). Ein fertiger Rust-ADT-Client wäre eine weitere, größere.
- **Zustandslos.** Teil 1 schickt keinen `X-sap-adt-sessiontype`. Teil 2
  (lock/write/activate/create) braucht eine **zustandsbehaftete** Sitzung, ein
  Lock-Handle und einen Transport; der Client bekommt dann eine Sitzung, die
  `adt/client` heute bewusst nicht hat.
- **Kein Abbruch, kein Fortschritt.** Ein Aufruf hat 120 s Zeit und liefert ganz oder
  gar nicht; sehr große Quelltexte kommen gekürzt (`max_zeichen`), nicht gestreamt.
- **Kein Token-Cache über Prozesse.** Jeder Start meldet sich neu an. Das kostet einen
  Aufruf je System und ist der Preis dafür, dass kein Token irgendwo liegt.

## Grenzen, die bewusst bleiben

- **Nur SELECT.** `abap_query` schickt die Abfrage an die Datenvorschau; SAP führt dort
  keinen schreibenden SQL aus. Ein „mal eben UPDATE“ gibt es nicht – auch nicht in
  Teil 2, weil Schreiben über ADT-Quelltexte läuft, nicht über SQL.
- **Unit-Tests und ATC sind lesend markiert.** Sie rechnen im System, ändern aber
  nichts an Objekten; `read_only_hint` bleibt darum wahr.
- **Transporte werden nur gelesen.** Anlegen, Freigeben und Löschen bleiben draußen.
- **Kein Debugger, kein Refactoring, kein abapGit.** Bewusst nicht Teil dieses Tools.
