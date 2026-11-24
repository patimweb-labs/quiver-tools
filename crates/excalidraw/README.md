# quiver-excalidraw

`quiver-excalidraw` ist das MCP-Werkzeug für **Excalidraw-Dateien** (`.excalidraw`). Ein
Agent erzeugt, liest und ändert Diagramme in **einem Arbeitsordner** und sieht sein
Ergebnis als **PNG**. Reines Rust, ohne Node, ohne Netz, ohne Browser im Hintergrund.

Aufgabe: [docs/aufgaben/17-excalidraw.md](../../docs/aufgaben/17-excalidraw.md).
Festlegungen und Preis: [ADR 0011](../../docs/adr/0011-excalidraw.md).
Der gemeinsame Unterbau (Ablage, Modell, Layout, Zeichnen) liegt in
[`crates/diagramm`](../diagramm/README.md) und wird **nur benutzt**.

## Felder im quiver-Fenster

| Feld | Art | Pflicht | Standard |
|---|---|---|---|
| `ordner` | Text | ja | – (voller Pfad, `~` wird aufgelöst, wird angelegt) |
| `schreiben` | Schalter | nein | **an** |

Es gibt **keine Zugangsdaten**. Alles, was das Werkzeug anfasst, liegt im Arbeitsordner;
Pfade sind relativ dazu und verlassen ihn nie (kein `..`, kein absoluter Pfad, keine
Symlinks, keine versteckten Namen – die Regeln setzt `diagramm::ablage` durch).

`quiver-excalidraw testen` öffnet den Arbeitsordner und zählt die Dateien.

## Die elf Werkzeuge

| Werkzeug | ändert | Wirkung |
|---|---|---|
| `excalidraw_status` | – | Arbeitsordner, Anzahl der Dateien, Schalter |
| `excalidraw_liste` | – | alle Dateien (auch in Unterordnern) mit Größe und Zeit |
| `excalidraw_lesen` | – | knappe Beschreibung je Element (Id, Art, Text, Lage, Bindungen) und `stand`; `roh: true` für den Quelltext (gekürzt) |
| `excalidraw_bild` | – | PNG als MCP-**Bildinhalt**; die Antwort nennt, was nicht gezeichnet wurde |
| `excalidraw_oeffnen` | – | öffnet `https://excalidraw.com` im Standardbrowser; `nur_link: true` öffnet nichts |
| `excalidraw_erstellen` | ja | aus `modell`: `knoten`, `kanten`, `gruppen`, `richtung` |
| `excalidraw_aus_quelltext` | ja | eine fertige Datei, **streng geprüft** |
| `excalidraw_elemente_aendern` | ja | je Id: `text`, `farbe`, `form`, `x`/`y`/`breite`/`hoehe`, `verbindungen` |
| `excalidraw_elemente_hinzufuegen` | ja | `rectangle`, `diamond`, `ellipse`, `text`, `line`, `arrow`, `frame`, `image` |
| `excalidraw_elemente_entfernen` | ja | entfernt Elemente und löst beide Seiten der Bindung auf |
| `excalidraw_exportieren` | ja | SVG oder PNG in den Arbeitsordner |
| `excalidraw_profile` | – | der Zugang aus der Umgebung (kein Geheimnis, weil es keins gibt) |

Jedes verändernde Werkzeug braucht den Schalter `schreiben` **und** den `stand`, den der
Agent gelesen hat. Ist der Schalter aus, kommt der Hinweis und es wird nichts geschrieben;
stimmt der `stand` nicht, endet der Aufruf mit „inzwischen geändert – neu lesen“ und
schreibt nichts. Anlegen überschreibt nie (`create_new`), Löschen gibt es nicht.

## Das Modell (Beschreibung statt Koordinaten)

```json
{ "richtung": "unten",
  "knoten": [ { "id": "start", "text": "Antrag", "form": "ellipse", "farbe": "blau" },
              { "id": "pruefen", "text": "Prüfung", "form": "raute", "gruppe": "vorgang" } ],
  "kanten": [ { "von": "start", "nach": "pruefen", "text": "eingereicht" } ],
  "gruppen": [ { "id": "vorgang", "titel": "Vorgang" } ] }
```

Das Layout rechnet `quiver-diagramm` (geschichtet, keine Überlappung, Textmaß aus der
eingebetteten Schrift). Was daraus in der Datei wird:

| Modell | Datei |
|---|---|
| Knoten `rechteck` / `raute` / `ellipse` | `rectangle` / `diamond` / `ellipse` |
| Knoten `zylinder`, `notiz` | `rectangle` (Excalidraw kennt beides nicht – siehe „Offen“) |
| Text eines Knotens | `text` mit `containerId` **und** dem Knoten in `boundElements` |
| Kante `pfeil` / `gestrichelt` | `arrow`, `strokeStyle: dashed` bei gestrichelt, `endArrowhead: arrow` |
| Kante `linie` | `line` ohne Pfeilspitze |
| Beschriftung einer Kante | `text` mit `containerId` = Pfeil |
| Gruppe | `frame` mit `name`; die Mitglieder tragen `frameId` |
| Farbe `blau` … | Füll- und Strichfarbe aus der Palette der Bibliothek |

Ids: `knoten-<id>`, `kante-<n>`, `gruppe-<id>`, `<element>-text`, `kante-<n>-beschriftung`.

## Die Datei

```json
{ "type": "excalidraw", "version": 2, "source": "https://github.com/patimweb-labs/quiver-tools",
  "elements": [ … ], "appState": { "viewBackgroundColor": "#ffffff" }, "files": { … } }
```

Jedes Element trägt die **25 Pflichtfelder** aus den Formatquellen (`id`, `type`, `x`, `y`,
`width`, `height`, `angle`, `strokeColor`, `backgroundColor`, `fillStyle`, `strokeWidth`,
`strokeStyle`, `roughness`, `opacity`, `seed`, `version`, `versionNonce`, `index`,
`isDeleted`, `groupIds`, `frameId`, `boundElements`, `updated`, `link`, `locked`) und was
seine Art braucht (`text`, `originalText`, `containerId`, `fontSize`, `fontFamily`,
`textAlign`, `verticalAlign`, `lineHeight`, `autoResize`, `points`, `startBinding`,
`endBinding`, `endArrowhead`, `roundness`, `name`).

- **Beidseitige Bindungen.** Text in einer Form steht über `containerId` **und** in
  `boundElements` der Form; ein Pfeil steht über `startBinding`/`endBinding` **und** in
  `boundElements` beider Enden. Entfernen löst die Gegenseite mit auf, Verschieben zieht
  Texte und Pfeilenden mit, Verbinden ändert nur die Art, wenn es die Verbindung schon gibt.
- **`version`/`versionNonce` steigen bei jeder Änderung**, `updated` ist der Zeitpunkt.
- **`link` nur `https`, `http`, `mailto`.**
- **Unbekanntes bleibt stehen.** Felder, die diese Kiste nicht liest (`customData`,
  `pressures`, `polygon`, …) und ganze Elemente, deren Art sie nicht kennt
  (`embeddable`, `iframe`, `stickynote`, …) wandern beim Lesen in eine Rest-Map und beim
  Schreiben unverändert zurück. `appState` und `files` (eingebettete Bilder) werden nicht
  angefasst.
- **`excalidraw_aus_quelltext` prüft streng** (`pruefen::pruefen`): Pflichtteile der Datei,
  Pflichtfelder je Element, doppelte oder leere Ids, `opacity` 0–100, `link`-Schema, und
  jede Bindung auf beiden Seiten. Die Meldungen nennen den Ort
  (`elements[2] (id „a“): Pflichtfeld „versionNonce“ fehlt`).
- **`excalidraw_lesen` ist großzügiger** als die Prüfung: fehlende Felder bekommen die
  Standardwerte, die Excalidraw selbst beim Laden vergibt (`restore.ts`), damit ein altes
  Bild nicht unlesbar wird.

## Das Bild

`excalidraw_bild` (und `excalidraw_exportieren`) zeichnet **selbst**, über
`diagramm::zeichnen` mit der eingebetteten Schrift „IBM Plex Sans“:

| Art | Bild |
|---|---|
| `rectangle`, `diamond`, `ellipse` | Form mit der Füllfarbe aus der Datei |
| `line`, `arrow`, `freedraw` | Linienzug, gestrichelt, mit Pfeilspitze |
| `text` | Textblock mit Ausrichtung (`left`/`center`/`right`) |
| `frame` | gestrichelter Rahmen mit Überschrift |
| `image` | Platzhalter mit der Aufschrift „Bild“ |
| alles andere | **nicht gezeichnet** – die Antwort nennt Art und Id |

Gezeichnet wird in der Reihenfolge der Datei, Rahmen zuerst; Texte stehen **nach** ihrer
Form in der Datei und liegen darum über der Fläche. `max_kante` begrenzt das PNG
(Standard 1600, höchstens 2000 px).

## Im Browser öffnen

Excalidraw kennt keinen Link, der den Inhalt nur im Fragment trägt (`#json=` lädt hoch).
Darüber **nichts** hochzuladen, ist der Punkt: `excalidraw_oeffnen` startet den
Standardbrowser mit der festen Adresse `https://excalidraw.com` und nennt den **vollen
Pfad** der Datei mit dem Hinweis „Datei in das Fenster ziehen“. Der Browser liegt hinter
einem Trait (`browser::Browser`); die Tests öffnen nie einen.

## Sicherheit

- Kein Netz, keine Anmeldung, kein Token – es gibt nichts zu stehlen.
- Kein `..`, kein absoluter Pfad, keine Symlinks auf jeder Ebene, kein Schreiben über
  Vorhandenes, kein Löschen von Dateien; höchstens 10 MB je Datei.
- Änderungen nur mit dem gelesenen `stand` (SHA-256), atomar, mit genau einer Sicherung
  `<name>.bak` und einer Dateisperre über Prozesse (alles aus `diagramm::ablage`).
- Texte werden im SVG maskiert; ein Knotentext kann kein Markup oder Skript einschleusen.

## Tests

```bash
CARGO_TARGET_DIR=target-pi cargo test -p quiver-excalidraw
```

- Einheitstests in jedem Modul: Pflichtfelder, Bindungen beidseitig, Reihenfolge der
  Elemente, Randpunkte, Kalenderdatum, Farbabbildung, Werkzeugfelder.
- `tests/datei.rs`: **die aus den Formatquellen abgeleitete Prüfung** gegen erzeugte
  Dateien (alle Formen, alle Farben, Gruppen, beide Richtungen, 39 Kanten), unbekannte
  Felder und Bilder überleben eine Änderung, `link`-Regeln, Grenzen der Prüfung.
- `tests/werkzeuge.rs`: alle elf Werkzeuge von Anfang bis Ende in **temporären Ordnern** –
  erstellen/lesen/ändern, falscher `stand`, Schalter aus, hinzufügen/entfernen mit
  aufgelösten Bindungen, Bild und Export, `oeffnen` mit einem zählenden Browser statt
  eines echten, Ausbruch aus dem Arbeitsordner, kaputte Datei.
- Kein Test öffnet einen Browser, keiner schreibt ins Benutzerverzeichnis.

## Offen

Was an der Schnittstelle zu `quiver-diagramm` fehlt, ist hier gelöst und gehört
irgendwann in die Bibliothek:

- **Farben nur als Namen.** `zeichnen::Figur` nimmt `Option<Farbe>`; Excalidraw-Dateien
  tragen `#rrggbb`. `farben::farbe_aus_hex` bildet einen Wert auf den nächsten Ton der
  Palette ab (exakt, sonst per Abstand; Grau/Schwarz/Weiß bleiben neutral). Eigene Farben
  aus excalidraw.com erscheinen darum in den Tönen der Bibliothek, nicht in ihrem
  Originalwert. Eine `Figur`-Variante mit freiem Farbwert würde das lösen.
- **Linien haben keine Farbe und keine Strichstärke.** `Figur::Linie` zeichnet immer mit
  `palette::LINIE` und `palette::STRICH`; ein rot gestrichelter Pfeil wird grau.
- **Zylinder und Notiz gibt es nicht.** Excalidraw kennt nur `rectangle`, `diamond` und
  `ellipse`; Modellknoten der Form `zylinder` oder `notiz` werden zum Rechteck.
- **`Szene::png` nennt die Pixelmaße nicht.** `Bild::pixel` rechnet die fünf Zeilen der
  Bibliothek nach, damit die Antwort „360×474 px“ stimmen kann.
- **Bilder zeichnet die Bibliothek nicht.** `Figur::Platzhalter` ist alles, was bleibt;
  ein echtes `image`-Element wird zum Kasten mit der Aufschrift „Bild“.
- **`Figur::Text` setzt die Zeilenhöhe fest** (`layout::ZEILENHOHE`, 14 × 1,35) und
  ignoriert `Text::groesse`; bei 16 px Schrift sitzen die Zeilen etwas enger als in
  Excalidraw.
- **`freedraw` nur als Linienzug**: Druckstärken (`pressures`) werden nicht gezeichnet,
  aber unverändert in der Datei erhalten.
- **Bindungsschema v1.** Neue Pfeile tragen `{ elementId, focus, gap }`; Excalidraw rechnet
  das beim Laden in sein neueres Schema (`mode`, `fixedPoint`) um. Nach dem Verschieben in
  Excalidraw kann ein Ende minimal anders sitzen als hier berechnet.
- **`index` bleibt `null`.** Die Reihenfolge ist die Reihenfolge in `elements`; die
  Bruchzahl-Indizes, die Excalidraw beim Umsortieren schreibt, werden erhalten, aber nicht
  selbst erzeugt.
- **Kein Hochladen, kein gemeinsames Bearbeiten.** `excalidraw_oeffnen` öffnet nur die
  leere Seite; wer teilen will, tut das von Hand auf excalidraw.com.
