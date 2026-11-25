# 0013 – Diagramme lokal: eine Bibliothek, Beschreibung statt Koordinaten

> Stand: 2026-11-24. Aufgabe: [docs/aufgaben/16-diagramme.md](../../docs/aufgaben/16-diagramme.md).
> Bereitet [0011-excalidraw.md](0011-excalidraw.md) und [0012-drawio.md](0012-drawio.md) vor.

## Kontext

Zwei Werkzeuge sollen Diagramme erzeugen und ändern: `quiver-excalidraw` (Format
`.excalidraw`) und `quiver-drawio` (Format `.drawio`). Beide arbeiten **lokal auf
Dateien**, rein in Rust, ohne Node, ohne Browser, ohne Netz. Die offiziellen Server
(excalidraw-mcp, drawio-mcp) sind Lesestoff für die Formate, keine Laufzeitabhängigkeit.

Damit standen vier Fragen am Anfang:

1. **Wo liegt der gemeinsame Teil?** Beide Werkzeuge brauchen dieselben Regeln für
   Dateien, dieselbe Art, ein Diagramm zu beschreiben, dasselbe Layout und dieselbe
   Bilderzeugung.
2. **Wie beschreibt ein Agent ein Diagramm?** Mit Koordinaten (wie die Formate selbst)
   oder mit Bedeutung (Knoten, Kanten, Gruppen, Richtung)?
3. **Womit wird gezeichnet?** Eigenes SVG und PNG, oder ein Fremdlaufzeit?
4. **Wie kommt ein Agent an Dateien, ohne Unsinn zu machen?** Er ist ein Modell: er
   schreibt Pfade falsch, überschreibt Arbeit und liest, was er nicht soll.

## Entscheidung

### 1. Eine Bibliothek `quiver-diagramm`, zwei Werkzeug-Crates

`crates/diagramm` ist eine reine Bibliothek (kein Programm, kein MCP) mit vier Modulen:

| Modul | Aufgabe |
|---|---|
| `ablage` | der einzige Weg zu Dateien: Arbeitsordner, relative Pfade, `stand`, Sperre, Sicherung |
| `modell` | was ein Diagramm ist: Knoten, Kanten, Gruppen, Richtung; Prüfung mit Meldungen |
| `layout` | wo die Teile liegen: geschichtetes Layout, Textmaß, keine Überlappung |
| `zeichnen` | wie es aussieht: eigenes Formenmodell → SVG → PNG (resvg) |

Die Werkzeuge bauen darauf ihre Formate. **Kein Werkzeug kennt das andere**, und die
Bibliothek kennt **kein** Format: Excalidraw-JSON und draw.io-XML entstehen erst in
`crates/excalidraw` und `crates/drawio`. Damit gilt weiter `kern ← <tool>` bzw. hier
`diagramm ← <tool>`.

### 2. Beschreibung statt Koordinaten

Ein Agent liefert:

```json
{ "richtung": "unten",
  "knoten": [{ "id": "start", "text": "Antrag", "form": "ellipse", "farbe": "blau" }],
  "kanten": [{ "von": "start", "nach": "pruefen", "text": "eingereicht" }],
  "gruppen": [{ "id": "vorgang", "titel": "Vorgang" }] }
```

**Warum nicht Koordinaten:** Ein Modell kann Lagen nicht zuverlässig schätzen. Mit
Koordinaten entstehen überlappende Kästen, Kanten durch Knoten und Diagramme, die je
nach Aufruf anders aussehen. Mit Bedeutung entsteht ein lesbares Bild, und der Agent
muss nur wissen, *was* zusammenhängt.

**Preis:** Freies Platzieren gibt es nicht. Wer eine bestimmte Anordnung will, braucht
die Werkzeuge aus 17/18 und deren Elementbefehle – oder er nimmt ein anderes Werkzeug.

### 3. Geschichtetes Layout mit Textmaß

`layout` vergibt Ränge (längster Weg), erkennt Kreise über Rückkanten, sortiert die
Schichten nach Baryzentrum und setzt sie. Die Knotengröße folgt dem **gemessenen Text**,
nicht einer festen Zahl; die Schrift dafür ist eingebettet. Weil jede Schicht eine eigene
Reihe ist und Knoten darin mit festem Abstand stehen, überlappen sich Knoten nie – das
prüft ein Test über viele zufällige Graphen.

Kanten laufen in den Zwischenräumen der Schichten. Was nicht zur nächsten Schicht führt
(Rückkante, Sprung), geht außen herum: bei „unten“ über einen Kanal rechts, bei „rechts“
über einen Kanal unten. So trifft keine Kante einen Knoten.

**Preis:** Das Layout ist bewusst einfach. Es gibt keine Kantenkreuzungsminimierung über
Baryzentrum hinaus, keine selbst gewählten Ports und keine Feinjustage. Rückkanten sehen
wie Schleifen um das ganze Diagramm aus.

### 4. Eigene Formen, eigenes SVG, PNG mit eingebetteter Schrift

`zeichnen` hat ein eigenes Formenmodell (`Szene` mit `Figur`): Rechteck, Ellipse, Raute,
Zylinder, Notiz, Container (Schwimmbahn/Frame), Text, Linie, Platzhalter. Daraus entsteht
SVG (alles maskiert, kein Skript, keine externen Verweise) und über `resvg` ein PNG.

Die Schrift ist **IBM Plex Sans**, eingebettet (`schriften/PlexSans-Regular.ttf`) mit
`schriften/OFL.txt` (SIL Open Font License 1.1). Keine Systemschriften: das Bild sieht
auf jedem Rechner gleich aus, und es gibt keinen stillen Rückfall auf irgendetwas.

Farben sind **semantische Namen** aus fester Liste (`blau`, `gruen`, `rot`, `gelb`,
`grau`, `violett`, `orange`, `tuerkis`); die konkreten Werte stehen an genau einer
Stelle (`zeichnen::palette`). Freie Hex-Werte nimmt das Modell nicht an.

**Warum nicht excalidraw-mcp/drawio-mcp oder ein Browser:** Beide Server sind
Netzdienste; ein Browser wäre eine zweite Laufzeit, die dieses Repo nicht hat. Ein
eigener Zeichner ist wenig Code, überall gleich und ohne Netz.

**Preis:** Der Zeichner kennt nur die Figuren, die in Aufgabe 16 stehen. Was ein Format
zusätzlich kann (Bilder, Pfeilspitzenvarianten, Stile), zeichnet er als Platzhalter oder
als Rechteck – und die Werkzeugantwort sagt es.

### 5. Dateien nur über die Ablage, mit Stand und Sperre

`ablage::Ablage` hat **einen** Arbeitsordner (Feld `ordner`, Pflicht, absolut, `~` wird
aufgelöst, wird angelegt). Alles darin ist relativ; abgelehnt werden absolute Pfade, `..`,
NUL, Backslash, `:`, Windows-Gerätenamen, versteckte Namen und symbolische Links auf jeder
Ebene. Gelesen und geschrieben werden nur normale Dateien bis 10 MB.

- **Anlegen** nie über Vorhandenes (`create_new`).
- **Ändern** nur mit dem `stand` (SHA-256 des gelesenen Inhalts); stimmt er nicht, wird
  nichts geschrieben. Geschrieben wird atomar (temporär + umbenennen), vorher entsteht
  genau eine Sicherung `<name>.bak`.
- **Sperre** über eine Datei im Arbeitsordner (`fd-lock`, `flock`/`LockFileEx`), damit
  zwei Prozesse sich nicht in die Quere kommen.
- **Löschen gibt es nicht.**

**Warum `stand` statt „einfach schreiben“:** Der Agent liest, denkt, schreibt. In der
Zwischenzeit kann der Mensch dieselbe Datei geändert haben. Mit `stand` endet der Aufruf
mit „inzwischen geändert – neu lesen“ statt mit überschriebener Arbeit.

**Warum kein Löschen:** Ein Modell, das Dateien löschen darf, ist ein Risiko ohne
Gegenwert; aufräumen kann der Mensch.

**Preis:** Ein Arbeitsordner je Werkzeug-Instanz, eine Endung je Ablage. `..` ist auch
auf Unix verboten, Backslash überall (Windows-Trenner), und ein `.bak` wird beim zweiten
Ändern überschrieben – genau eine Sicherung, wie gefordert. Unter Windows ist das
Ersetzen beim Umbenennen nicht atomar (dort fehlt `rename`-over-existing); das ist im
Code kommentiert.

## Folgen

- Aufgaben 17 und 18 bauen nur noch Format und Werkzeuge; Ablage, Modell, Layout und
  Zeichner sind fertig und getestet.
- `Layout` und `Szene` sind öffentlich: ein Werkzeug kann ein fremdes Format in eine
  `Szene` übersetzen und dieselbe `Szene::png` benutzen – so sieht `*_bild` in allen
  Werkzeugen gleich aus.
- Die Tests laufen nur in temporären Ordnern; kein Test schreibt in den Arbeitsordner
  des Users.

## Grenzen, die bewusst bleiben

- Kein freies Layout, keine Ports, keine Kreuzungsoptimierung jenseits des Baryzentrums.
- Kein Bild-Inhalt: `files`/Bilder werden als Platzhalter gezeichnet (Aufgabe 17 sagt es).
- Keine Farbverläufe, Schatten, Muster oder freien Farben.
- Kein Löschen, kein Verschieben, kein Umbenennen über die Ablage.
- Ein `.bak` je Datei, nicht eine Historie.
