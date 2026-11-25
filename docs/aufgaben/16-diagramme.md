# 16 – quiver-excalidraw und quiver-drawio

Zwei Crates, **lokal auf Dateien**, reines Rust, ohne Node und ohne Netz: erst `crates/diagramm` (Unterbau), danach `crates/excalidraw` (17) und `crates/drawio` (18). Die offiziellen Server (excalidraw-mcp, drawio-mcp) sind nur Lesestoff für die Formate.

## crates/diagramm (Bibliothek)

**`ablage`** – der einzige Weg zu Dateien:
- Arbeitsordner aus dem Feld `ordner` (Pflicht, absolut, `~` wird aufgelöst; wird angelegt). Alle Pfade relativ darin, Unterordner erlaubt, Endung erzwungen (`.excalidraw` bzw. `.drawio`; Ausgaben `.svg`/`.png`).
- Abgelehnt: absolut, `..`, NUL, Backslash, Windows-Gerätenamen und `:`, versteckte Namen (`.…`), symbolische Links auf **jeder** Ebene (auch der Datei selbst). Nur normale Dateien, höchstens 10 MB lesen und schreiben.
- Anlegen nie über Vorhandenes (`create_new`). Ändern nur mit dem vom Agenten gelesenen `stand` (SHA-256 des Inhalts): stimmt er nicht → „inzwischen geändert – neu lesen“, nichts schreiben. Atomar (temporär + umbenennen), Dateisperre über Prozesse, vorher Sicherung `<name>.bak` (genau eine). Löschen gibt es nicht.

**`modell` + `layout`** – Beschreibung statt Koordinaten: `knoten [{id, text, form: rechteck|raute|ellipse|zylinder|notiz, farbe?, gruppe?}]`, `kanten [{von, nach, text?, art: pfeil|linie|gestrichelt}]`, `gruppen [{id, titel}]`, `richtung: unten|rechts`. Prüfung mit klaren Meldungen (doppelte Ids, unbekannte Enden, leere Ids; höchstens 300 Knoten, 600 Kanten, 2000 Zeichen je Text). Geschichtetes Layout mit Kreisbehandlung, Textmaß für die Knotengröße, **keine überlappenden Knoten**, Gruppen umschließen ihre Knoten; gleiche Eingabe → gleiche Ausgabe. Farbnamen aus fester Liste statt freier Werte.

**`zeichnen`** – eigenes Formenmodell → SVG (alles escaped, kein Skript, keine externen Verweise) und SVG → PNG mit `resvg` (Schrift eingebettet mit freier Lizenz samt Lizenzdatei; höchstens 2000 px Kante). Die Werkzeuge liefern das PNG als **MCP-Bildinhalt**, damit der Agent sein Ergebnis sieht.

Tests: jede Ablehnungsregel (auch Symlink-Wechsel), gleichzeitiges Ändern, Layout ohne Überlappung über viele zufällige Graphen (fester Startwert), XML-/SVG-Einschleusung über Texte, Grenzen. Prüfbefehl `cargo fmt --all --check && cargo clippy -p quiver-diagramm --all-targets -- -D warnings && cargo test -p quiver-diagramm`.

## Regeln für beide Werkzeuge

- Felder: `ordner` (Text, Pflicht) und `schreiben` (Schalter, **Standard an**: geschrieben wird nur im Arbeitsordner, nie über Vorhandenes und nur mit `stand`).
- `<tool>_status`, `_liste`, `_lesen` (knappe Beschreibung mit Ids und `stand`; `roh: true` für den Quelltext, gekürzt mit Hinweis), `_erstellen` (aus `modell`), `_aus_quelltext` (fertiges Format, geprüft), `_elemente_aendern` (Text/Farbe/Form/Position/Verbindung je Id), `_elemente_hinzufuegen`, `_elemente_entfernen`, `_bild` (PNG), `_exportieren` (SVG/PNG in den Ordner).
- Beim Ändern bleiben **unbekannte Felder und Elemente erhalten**; unberührte Elemente sind danach inhaltsgleich.
- Verhalten wie die anderen Werkzeuge (Kern, Annotationen, Schemas mit `"type": "object"`, kurze Antworten, ADR, README im Crate). Nur eigene Crates plus `Cargo.toml`/`Cargo.lock`/ADR anfassen. Nicht committen.
