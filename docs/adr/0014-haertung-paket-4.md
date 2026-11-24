// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

# ADR 0014 – Härtung Paket 4: Schreibschutz, Netz, Dateien, Tresor

**Datum:** 2026-11-24
**Status:** entschieden

## Zusammenhang

Eine unabhängige Sicherheitsprüfung der 16 Crates fand Bruchstellen dort, wo
Agenten-Eingaben ungeprüft in Adressen oder Protokollzeilen wandern und wo das
Konzept „Schreibschalter“ nicht zu Ende geführt war. Die zwei schwersten Befunde
(K1 ABAP-Adresse, H1 IMAP-Befehlszeile) sind bereits behoben (`a129f8a`). Dieses
ADR hält die Entscheidungen für den Rest fest.

## Entscheidungen

### 1. Schreibschutz zentral im Kern

Die Prüfung liest die Kennzeichnung `read_only_hint` aus der Werkzeugliste **des
Programms** (dieselbe Liste, die `tools/list` ausliefert) und sperrt alles, was
nicht ausdrücklich als lesend gekennzeichnet ist. Der Hinweis nennt Feld und Ort im
quiver-Fenster; es geht keine Anfrage hinaus.

*Warum nicht je Werkzeug?* Eine Prüfung, die jedes Werkzeug selbst aufrufen muss,
kann vergessen werden – genau das war der Befund. Zentral heißt hier: eine Funktion
(`kern::schutz::schreibschutz`), ein Aufruf je Programm am Anfang des Verteilers,
plus ein Vertragstest über alle Programme.

*Preis:* Nach dem Update ist Schreiben überall aus; der User muss es je Zugang
einschalten. Der Hinweis und die Startmeldung („Schreiben ist jetzt aus“) nennen
das Feld, damit kein Zugang still ausfällt.

Für ZEP gilt zusätzlich eine **feste Liste** lesender Werkzeugnamen im Programm.
Die Kennzeichnung der Gegenseite allein würde einem Vermittler erlauben, ein
Schreibwerkzeug als lesend durchzureichen.

### 2. Ein HTTP-Client, eine Adressregel

`kern::net` liefert den einen Client (keine Weiterleitung, 60 s Frist, 8 MB je
Antwort) und die eine Adressregel (`https` immer, `http` nur Loopback oder Feld
„Unverschlüsselt erlauben“, Standard aus) sowie die Zertifikatsregel (abschalten
nur Loopback, sonst eigenes CA-Zertifikat).

*Warum ein Feld statt einer starren Regel?* Ein bestehender Zugang mit `http`
(Testbrücke, interner Host) soll nicht still ausfallen – er soll sagen, welches
Feld einzuschalten ist. Der sichere Zustand bleibt der Standard, und unverschlüsselte
Adressen stehen als Warnung im Status und im Log.

### 3. Schreiben nur über eine Ablage

Dateien entstehen nur über `kern::Ablage`: relative Namen, kein `..`, keine
absoluten Pfade, keine Symlinks auf jeder Ebene, `create_new`. `outline_read`
schreibt nicht mehr (`save_dir` entfällt); Export geht in den Arbeitsordner.
SSH-Transfer nur im Austauschordner, Download überschreibt nie.

### 4. Tresor: kein neuer Schlüssel bei vorhandenen Daten

Fehlt `tresor.schluessel`, während `tresor.json` existiert, ist das ein Fehler;
beide Dateien bleiben unberührt. Ein neuer Schlüssel würde die vorhandenen Werte
endgültig unlesbar machen, ohne dass es jemand merkt.

### 5. Attrappen in Dienstform

Die DevOps-Attrappe antwortet nach der REST-Spezifikation 7.1 (`count`, `value`,
`url`, `_links`, `rev`), die ABAP-Attrappe mit echtem ADT-XML (relative URIs,
Versionsverweise). Ein Werkzeug, das absolute Adressen oder eine erfundene Hülle
erwartet, fällt damit im Test auf – die Lehre aus dem 404.

### 6. Fuzz und Vertrag als Tests

Ein Fuzz-Test mit festem Startwert (20 000 Eingaben) über alle Leser stellt sicher,
dass kein Dateiinhalt das Programm beenden kann. Ein Vertragstest über alle
Programme prüft die Schreibschutz-Invariante. Beide sind Teil des Prüfbefehls.

## Folgen

- `crates/kern` hat Dev-Abhängigkeiten auf die Werkzeug-Crates (nur für den
  Vertragstest; Cargo erlaubt diesen Kreis über Dev-Abhängigkeiten).
- `crates/kern` hängt jetzt an `reqwest` und `url`.
- Microsoft 365 prüft den Schreibschutz im Programm selbst (es nutzt den Kern
  nicht); die Begründung und die Bereichsliste stehen in `crates/m365/src/umgebung.rs`.
- Nach dem Update ist Schreiben aus – das ist gewollt und wird beim ersten Start
  sowie in jeder Sperre erklärt.
