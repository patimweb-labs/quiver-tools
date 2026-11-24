# 11 – ZEP: zuverlässig automatisch anmelden, Tests nie im echten Zustand

## Befund (2026-11-24)

- Ein Unit-Test in `crates/zep/src/server.rs` hat in den **echten** Zustand des Users
  geschrieben: `ZepServer::neu(zugang("https://zep.example.test/…"))` nutzt
  `zustandsordner()` und `DateiTresor::standard()`, also `~/.local/state/quiver-zep`.
- Dort lag danach `routine-gesperrt.json` mit dem Grund „zep.example.test … nicht erreichbar“.
- Ergebnis: Die automatische Anmeldung beim Start war gesperrt, und der User musste jedes Mal
  auf „Anmelden“ klicken.

## 1. Tests nie im echten Zustand

- `zustandsordner()` liefert unter `cfg(test)` einen temporären Ordner je Prozess, z. B.
  `std::env::temp_dir()/quiver-zep-test-<pid>`. Damit gilt das auch für `DateiTresor::standard()`.
- Integrationstests setzen weiterhin `ZEP_ZUSTAND`.
- Prüfe **alle Crates** (`zep`, `m365*`, `ssh`, `abap`, `devops`, `email`, `outline`) auf
  Tests, die echte Ordner oder Tresore berühren können, z. B. `dirs::data_dir`,
  `dirs::config_dir`, `zustandsordner`, `DateiTresor::standard`, `HOME`. Schließe sie genauso.
- Ein Wächter-Test je betroffenem Crate stellt sicher, dass `zustandsordner()` im Test **nicht**
  unter `HOME` liegt.

## 2. Sperre nur nach echtem Fehlversuch

Die Routine sperrt sich heute bei **jedem** Fehlschlag, auch wenn ZEP gar nicht erreichbar war
oder die Seite unbekannt ist. Künftig gilt:

- **Gesperrt wird nur, wenn Benutzer und Passwort tatsächlich gesendet wurden** und die Anmeldung
  danach nicht zum Token führte, also bei `Abgelehnt` oder einer unbekannten Seite **nach** dem
  Absenden. Nur das zählt bei ZEP als Fehlversuch, denn ZEP sperrt nach 5.
- Fehler **vor** dem Absenden sperren **nicht**: Netz, Metadaten, Registrierung, unbekannte
  Seite vor dem Formular, fremder Host. Sie werden gemeldet, und beim nächsten Bedarf wird es
  erneut versucht, höchstens alle 60 s (Merker im Speicher, nicht in der Datei).
- Dafür gibt die Routine zurück, ob die Zugangsdaten gesendet wurden, z. B. als Feld im
  Ergebnis oder als eigene Fehlerart.

## 3. Anmelden beim Start

Ist beim Start des MCP-Prozesses kein gültiges Token da, die Routine nicht gesperrt und sind
Benutzer und Passwort gesetzt, meldet sich quiver-zep **sofort** im Hintergrund an. Das passiert
nicht erst beim ersten Tool-Aufruf. `status` zeigt danach „Angemeldet“.

## Tests

- Ein Netzfehler vor dem Absenden ergibt keine Sperrdatei, und ein zweiter Versuch nach der
  Wartezeit klappt.
- Ein falsches Passwort ergibt eine Sperre, und es gibt genau einen Login-POST (wie bisher).
- Eine unbekannte Seite vor dem Formular ergibt keine Sperre.
- Beim Start wird ohne Token automatisch angemeldet, und die Attrappe zählt genau einen
  Login-POST.
- Der Wächter-Test aus Teil 1.

**Prüfbefehl grün. Nicht committen.**
