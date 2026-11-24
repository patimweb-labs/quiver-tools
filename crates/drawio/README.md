# quiver-drawio

draw.io-Dateien (`.drawio`) als MCP-Tool: **lesen, anlegen, ändern, zeichnen, verlinken,
im Browser öffnen**. Reines Rust, lokale Dateien, **keine Anmeldung, kein Netz**.

Für wen: für Agenten, die ein Ablauf- oder Architekturbild liefern oder ein vorhandenes
draw.io-Dokument pflegen sollen – und für Menschen, die danach in draw.io weiterarbeiten.
Festlegungen und Preis: [ADR 0012](../../docs/adr/0012-drawio.md), Unterbau:
[ADR 0013](../../docs/adr/0013-diagramme-lokal.md).

```
Modell (Knoten/Kanten/Gruppen) → Layout (quiver-diagramm) → draw.io-Zellen (.drawio)
                                                                    ↕
                                              eigener Zeichner → SVG/PNG
                                              #R-Link → Browser (nur im Fragment)
```

## Felder (quiver-Fenster)

| Feld | Art | Pflicht | Standard | Bedeutung |
|---|---|---|---|---|
| `ordner` | Text | ja | – | Arbeitsordner, absolut, `~` wird aufgelöst, wird angelegt |
| `schreiben` | Schalter | nein | **an** | erlaubt Anlegen/Ändern/Export; Lesen und Zeichnen gehen immer |

Es gibt **keine Zugangsdaten**. Alles geschieht in `ordner`; symbolische Links, `..`,
absolute Pfade und versteckte Namen sind gesperrt (`quiver-diagramm::ablage`, Aufgabe 16).

```bash
quiver-drawio            # MCP über stdio (so startet quiver es)
quiver-drawio eintrag    # Bibliothekseintrag für mcp-quiver
quiver-drawio testen     # Arbeitsordner prüfen (DRAWIO_ORDNER, DRAWIO_SCHREIBEN)
```

## Werkzeuge

Lesend (`read_only_hint`):

| Werkzeug | Antwort |
|---|---|
| `drawio_status` | Arbeitsordner, Zahl der Dateien, ob `schreiben` an ist |
| `drawio_liste` | alle `.drawio`-Dateien mit Größe (Unterordner mitgezählt) |
| `drawio_seiten` | Seiten einer Datei: Nummer, Id, Name, Zellen, komprimiert? |
| `drawio_lesen` | knappe Beschreibung: `stand`, Seiten, je Zelle Id/Art/Form/Elternteil/Text; `roh: true` liefert den Dateitext (gekürzt) |
| `drawio_bild` | PNG als MCP-Bildinhalt + was gezeichnet wurde |
| `drawio_link` | `#R`-Link als Text (ganzes Diagramm im Fragment) |

Verändernd (`destructive_hint`, brauchen `schreiben`; Änderungen an einer vorhandenen
Datei zusätzlich den `stand`):

| Werkzeug | Wirkung |
|---|---|
| `drawio_erstellen` | Datei aus einem Modell (Knoten, Kanten, Gruppen, Richtung); Layout rechnet `quiver-diagramm` |
| `drawio_aus_quelltext` | Datei aus fertigem XML (`<mxfile>` oder `<mxGraphModel>`), geprüft |
| `drawio_elemente_aendern` | Text, Farbe, Form, Lage, `quelle`/`ziel` je Zellen-Id |
| `drawio_elemente_hinzufuegen` | Knoten oder Kante anhängen; Ids vergibt das Werkzeug (`n1`, `n2`, …) |
| `drawio_elemente_entfernen` | Zellen entfernen; Kanten/Kinder nur mit ausdrücklicher Angabe |
| `drawio_seite_hinzufuegen` | Seite anhängen (Modell, `<mxGraphModel>` oder leer) |
| `drawio_exportieren` | SVG oder PNG in den Arbeitsordner (Endung entscheidet) |

Keins von beidem: `drawio_oeffnen` (öffnet den selbst gebauten Link im Standardbrowser;
nicht lesend, nicht destruktiv). Dazu `drawio_profile` aus dem Kern.

## Was das Format darf – und was nicht

**Sicher lesen.** Der eigene XML-Leser (`xml.rs`) lehnt DTDs und Entitäten ab, begrenzt die
Tiefe (64) und maskiert beim Schreiben `& < > "`, Zeilenumbrüche als `&#10;`. CDATA,
mehrere Wurzeln und fremde Entitäten sind Fehler mit Stelle. Dateien bis 10 MB (Ablage),
komprimierte Seiten bis 20 MB entpackt.

**Seiten.** `mxfile → diagram(id,name) → mxGraphModel → root → mxCell`, Zellen `0` (Wurzel)
und `1` (Ebene) sind Pflicht. `seite` nimmt die **Id** oder den **Namen**; passt ein Name
auf mehrere Seiten, nennt die Antwort die Kandidaten statt zu raten.

**Komprimierte Seiten** werden gelesen (Base64 → raw inflate → Prozentdekodierung) und
**immer unkomprimiert geschrieben**; das `compressed`-Attribut verschwindet dabei.

**Erhalten bleibt, was das Werkzeug nicht kennt:** `UserObject`/`object`-Hüllen mit ihren
Attributen, HTML-Labels (nur gelesen, nicht neu geschrieben), unbekannte Attribute an
`mxfile`, `diagram` und `mxGraphModel`, fremde Stil-Schlüssel, Geometrie-Kinder
(`mxPoint`, `Array`, `mxRectangle`) und Zellen, die niemand angefasst hat.

**Stile** werden als Kette einzelner Teile geändert (`stil.rs`): nur Formwort, `shape` und
die Schlüssel der neuen Form werden ersetzt, alles andere bleibt stehen.

**Neue Labels sind Text**, kein HTML: `<b>` wird beim Schreiben zu `&lt;b&gt;`.

**Kanten** dürfen nur auf vorhandene Knoten zeigen (`source`/`target`), sonst wird die
Datei abgelehnt. **Links** (`link` an der Hülle oder im Stil) nur `http://`, `https://`,
`mailto:`.

**Entfernen** ist vorsichtig: hängen Kanten an einer Zelle, braucht es
`kanten: "entfernen"` (mit) oder `kanten: "loesen"` (Kante behalten, Verbindung lösen);
haben Zellen Kinder, `kinder: "entfernen"` oder `kinder: "behalten"` (hochziehen). Ohne
Angabe wird **nichts** geschrieben. Zellen `0` und `1` sind unantastbar.

## Bild – eigener Zeichner

`drawio_bild` und `drawio_exportieren` zeichnen ohne Browser (`bild.rs` →
`quiver-diagramm::zeichnen`): Rechteck (`rounded`, `rectangle`), Ellipse, Raute, Zylinder
(`shape=cylinder3`), Notiz (`shape=note`), Text, Container/Schwimmbahn und Kanten
(Stützpunkte, `dashed=1`, `endArrow=none`). Die Schrift ist eingebettet, das PNG höchstens
2000 px je Kante.

Eine **unbekannte Form** (z. B. `shape=mxgraph.aws4.lambda`) wird als Rechteck gezeichnet,
und die Antwort nennt den Namen. Eine Kante ohne Anfang oder Ende wird nicht gezeichnet und
in der Antwort genannt. Füllfarben werden auf die feste Palette abgebildet (der Zeichner
kennt keine freien Werte); Strichfarben, Schriftfarben und Pfeilvarianten vereinfacht er.

## Link und Browser

`drawio_link` baut `https://app.diagrams.net/?grid=0&pv=0&border=10&edit=_blank#R…`:
`encodeURIComponent(XML)` → raw deflate → Base64 → prozentkodiert. **Der Inhalt steht nur
im Fragment** und geht nie an einen Server; die Antwort sagt, dass der Link das ganze
Diagramm enthält, und dass er höchstens 1 MB trägt.

`drawio_oeffnen` öffnet **genau diesen** Link über das Crate `open` (keine Shell, nie eine
Adresse vom Agenten). Änderungen im Browser landen **nicht** in der lokalen Datei – die
Antwort sagt, dass man dort speichern und die Datei zurück in den Arbeitsordner legen muss.
Ist der Link länger als 2000 Zeichen (Grenze des Startbefehls), wird nichts geöffnet: die
Antwort nennt den Dateipfad statt eines Fehlers. Der Browser-Start liegt hinter dem Trait
`browser::Browser`; **kein Test öffnet einen Browser**.

## Beispiel

```jsonc
// drawio_erstellen
{ "datei": "antrag.drawio",
  "modell": { "richtung": "unten",
    "knoten": [{ "id": "start", "text": "Antrag", "form": "ellipse" },
               { "id": "pruefen", "text": "Prüfung", "form": "raute" },
               { "id": "ende", "text": "Fertig", "farbe": "gruen" }],
    "kanten": [{ "von": "start", "nach": "pruefen" },
               { "von": "pruefen", "nach": "ende", "text": "ja" }] } }

// Antwort: „antrag.drawio“ angelegt (1 Seite(n), 5 Zelle(n)). stand: <64 Hexzeichen>

// drawio_elemente_aendern (stand aus der Antwort oder aus drawio_lesen)
{ "datei": "antrag.drawio", "stand": "…",
  "elemente": [{ "id": "pruefen", "farbe": "gelb", "text": "Prüfung & Freigabe" }] }
```

Ids aus dem Modell bleiben die Zellen-Ids; Kanten bekommen `kante-1`, `kante-2`, …
Zwei Gruppen mit derselben Id oder eine Gruppe mit der Id eines Knotens lehnt das Werkzeug
ab – draw.io braucht eindeutige Zellen-Ids.

## Tests und Prüfbefehl

```bash
CARGO_TARGET_DIR=$PWD/target-pi cargo fmt --all --check
CARGO_TARGET_DIR=$PWD/target-pi cargo clippy -p quiver-drawio --all-targets -- -D warnings
CARGO_TARGET_DIR=$PWD/target-pi cargo test -p quiver-drawio
```

- `tests/format.rs` – DTD/Entitäten, Hüllen und unbekannte Felder, komprimierte Seiten,
  Kanten/Links, `#R`-Link, Grenze 1 MB, gleiche Beschreibung → gleiches Dokument.
- `tests/werkzeuge.rs` – der Weg eines Agenten in einem **temporären** Ordner: anlegen,
  lesen, ändern (nur mit `stand`), hinzufügen, entfernen (nur ausdrücklich), Seite
  anhängen, Bild/Export, Link/Öffnen mit einem **aufzeichnenden** Browser, `schreiben: aus`,
  fehlender Ordner (Hinweis statt Fehler), zu langer Link.
- `src/*` – Einheitstests für XML, Stil, Formen, Seitenwahl, Zellen, Link-Kodierung,
  Zeichner.

Ein durchgehendes Beispiel (Modell → Datei, fremdes XML → Seite, beide Seiten als PNG,
Link) schreibt nur in den temporären Ordner:

```bash
CARGO_TARGET_DIR=$PWD/target-pi cargo run -p quiver-drawio --example beispiel
```

## Offen

An der Schnittstelle von `crates/diagramm` fehlte Folgendes; es ist **hier** gelöst und
gehört bei Gelegenheit in die Bibliothek (dieses Crate ändert sie nicht):

1. **Farbe aus einem Hex-Wert.** `zeichnen::palette` liefert nur Werte zu `Farbe`; die
   Rückrichtung (fillColor → nächste semantische Farbe) steht als `formen::farbe_aus_hex`
   hier. Sinnvoll wäre sie in `zeichnen`.
2. **Strich- und Schriftfarbe.** `Szene`/`Figur` kennen nur eine Füllfarbe je Form und keine
   Textfarbe; draw.io-Stile mit eigenem `strokeColor`/`fontColor` werden darum auf die
   Palette vereinfacht gezeichnet. Die Datei selbst bleibt unverändert.
3. **Kanten ohne Id im Modell.** `layout::KantenLauf` hat `von`/`nach`, aber keine Id; die
   Kanten-Zellen bekommen darum `kante-N` (Kollisionen mit Knoten-Ids werden geprüft).
4. **Gruppen-Id gleich Knoten-Id** lehnt `modell::pruefen` nicht ab (draw.io braucht
   eindeutige Zellen-Ids); die Prüfung steht in `aus_modell`.

Bewusst nicht enthalten (Aufgabe 18): **kein Aufruf von draw.io Desktop** – es gibt nur den
Browser-Link und die lokale Datei. Ebenfalls nicht enthalten: Bilder/`files` in der Datei
werden als Platzhalter gezeichnet, freie Farben und Farbverläufe gibt es nicht, und der
Export schreibt nie über Vorhandenes. Die Werkzeugliste im Wurzel-`README.md` und die
`SPEC.md` nennen `quiver-drawio` noch nicht (außerhalb der Dateien dieser Aufgabe).
