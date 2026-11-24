# 10 – Feldart und Titel im Bibliothekseintrag

mcp-quiver kennt jetzt `Feld.art` und `Feld.titel` (Aufgabe 16 dort). `quiver-<tool> eintrag`
gibt beide aus, damit der Client Checkboxen, Zahlen usw. richtig zeigt.

## Änderungen

- `crates/kern/src/eintrag.rs` → `als_feld`:
  - `art` aus `FeldArt` mappen: Text→`text`, Passwort→`passwort`, Zahl→`zahl`,
    Schalter→`schalter`, Url→`url`, Auswahl→`auswahl{optionen:[{wert,anzeige}]}`
  - `titel` = `feld.titel`
  - `beschreibung` wird zu **nur der Hilfe**, denn der Titel steht jetzt eigens.
  - `Quelle` bleibt wie bisher.
- Das gilt für alle Tools mit `eintrag`: Felder **und** `mehrfach.felder`.
- quiver-m365 hat kein `kern`-Eintrag. Falls es dort eine eigene Ausgabe gibt, gilt dasselbe.

## Tests

- Je Art die richtige JSON-Ausgabe.
- `quiver-abap eintrag`: `zertifikat_pruefen` und `schreiben` haben `art: "schalter"`, `mandant`
  hat `titel: "Mandant"`.
- Die bestehenden Eintrags-Tests werden angepasst.

**Prüfbefehl grün. Nicht committen.**
