# quiver-diagramm

Der gemeinsame Unterbau der Diagramm-Werkzeuge `quiver-excalidraw` (Aufgabe 17) und
`quiver-drawio` (Aufgabe 18): **Ablage**, **Modell**, **Layout**, **Zeichnen**. Reines
Rust, keine Node-Laufzeit, kein Browser, kein Netz.

Dieses Crate ist eine **Bibliothek** und enthält kein Programm und kein MCP-Werkzeug. Die
Formate (`.excalidraw`, `.drawio`) kennt es nicht – die entstehen in den Werkzeug-Crates.
Festlegungen und Preis: [ADR 0013](../../docs/adr/0013-diagramme-lokal.md).

```
Modell (Beschreibung)  →  Layout (Koordinaten)  →  Zeichnen (SVG/PNG)
        modell                  layout                 zeichnen
                                                    ↗
Dateien  ─────────────────────────────────  ablage  (Arbeitsordner, stand, Sperre)
```

## Ablage – der einzige Weg zu Dateien

```rust
use quiver_diagramm::ablage::Ablage;

let ablage = Ablage::neu("~/Diagramme", "excalidraw")?;   // ~ wird aufgelöst, Ordner wird angelegt
let inhalt = ablage.lesen("entwurf.excalidraw")?;          // Inhalt { text, stand, groesse, geaendert }
let neu = ablage.aendern("entwurf.excalidraw", &neuer_text, &inhalt.stand)?;
println!("{}", neu);                                       // neuer stand (SHA-256, 64 Hexzeichen)
```

| Aufruf | Wirkung |
|---|---|
| `Ablage::neu(ordner, endung)` | Arbeitsordner öffnen/anlegen; `endung` ohne Punkt (`excalidraw`, `drawio`) |
| `wurzel()`, `endung()` | was gilt |
| `pfad(relativ)` | prüfen und den vollen Pfad geben (auch für Dateien, die es noch nicht gibt) |
| `ausgabe_pfad(relativ)` | dasselbe für Ausgaben, nur `.svg`/`.png` |
| `lesen(relativ)` | `Inhalt` mit `stand`; höchstens 10 MB, nur UTF-8, nur normale Dateien |
| `stand(relativ)` | nur der SHA-256 |
| `anlegen(relativ, text)` | **nur neu** (`create_new`), gibt den `stand` |
| `aendern(relativ, text, &stand)` | nur bei passendem `stand`; atomar, mit genau einer Sicherung `<name>.bak` |
| `ausgabe_schreiben(relativ, bytes)` | SVG/PNG schreiben, **nur neu** |
| `liste()` | alle Diagrammdateien (rekursiv, sortiert, ohne Verstecktes/Symlinks/`.bak`) |

Regeln, die die Ablage durchsetzt (Tests prüfen jede):

- **Arbeitsordner:** Pflicht, absolut, `~`/`~/…` wird aufgelöst, wird angelegt.
- **Abgelehnt:** absolute Pfade, `..`, NUL, Backslash, `:`, Windows-Gerätenamen (`CON`,
  `NUL`, `COM1`…, `LPT1`…), versteckte Namen (`.…`), leere Segmente (`//`, `/./`),
  symbolische Links auf **jeder** Ebene – auch die Datei selbst.
- **Nur normale Dateien**, höchstens **10 MB** je Lese- oder Schreibvorgang.
- **Unterordner** sind erlaubt (bis 16 Ebenen) und werden beim Anlegen erzeugt.
- **Endung erzwungen:** Diagramme `.excalidraw`/`.drawio`, Ausgaben `.svg`/`.png`.
- **Kein Überschreiben beim Anlegen**, **kein Löschen**.
- **Änderung nur mit `stand`:** stimmt er nicht, endet der Aufruf mit
  „inzwischen geändert – bitte neu lesen“ und schreibt nichts.
- **Sperre über Prozesse** (Datei `.sperre` im Arbeitsordner) und **atomares Schreiben**
  (temporär + umbenennen).
- **Fehlertexte** sagen, was falsch ist und was zu tun ist; sie enthalten keine
  Zugangsdaten.

## Modell – Beschreibung statt Koordinaten

```rust
use quiver_diagramm::modell::{Diagramm, Knoten, Kante, Gruppe, Form, Farbe, KanteArt, Richtung};

let mut diagramm = Diagramm::leer();
diagramm.knoten = vec![
    Knoten { form: Form::Ellipse, farbe: Some(Farbe::Blau), ..Knoten::neu("start", "Antrag") },
    Knoten { form: Form::Raute, ..Knoten::neu("pruefen", "Prüfung") },
];
diagramm.kanten = vec![Kante { text: Some("eingereicht".into()), art: KanteArt::Pfeil,
                                 ..Kante::neu("start", "pruefen") }];
diagramm.gruppen = vec![Gruppe::neu("vorgang", "Vorgang")];
diagramm.pruefen()?;
```

- **Knoten:** `id`, `text`, `form` (`rechteck|raute|ellipse|zylinder|notiz`),
  `farbe?` (feste Liste), `gruppe?`.
- **Kanten:** `von`, `nach`, `text?`, `art` (`pfeil|linie|gestrichelt`).
- **Gruppen:** `id`, `titel`.
- **Richtung:** `unten` (Standard) oder `rechts`.
- **JSON:** `Diagramm::aus_json(&str)` / `Diagramm::als_json()` – dasselbe Schema wie im
  Werkzeugaufruf.
- **`pruefen()` sammelt alle Verstöße** und nennt den Ort: doppelte und leere Ids,
  unbekannte Kantenenden und Gruppen, Selbstverbindungen, Steuerzeichen, Grenzen
  (**300 Knoten**, **600 Kanten**, **2000 Zeichen je Text**). `ModellFehler` gibt die
  Meldungen einzeln (`meldungen()`) und als Text (`Display`).
- Farben sind **Namen**, keine Werte: `Farbe::ALLE`, `Farbe::name()`, `Farbe::aus_name()`.

## Layout – wo die Teile liegen

```rust
let zeichnung = quiver_diagramm::layout::layout(&diagramm)?;   // Result<Zeichnung, ModellFehler>
let szene = quiver_diagramm::zeichnen::Szene::aus_zeichnung(&zeichnung);
std::fs::write("plan.svg", szene.svg())?;
let png = szene.png(2000)?;                                     // höchstens 2000 px je Kante
```

`Zeichnung` enthält `breite`, `hoehe`, `knoten: Vec<KnotenPlatz>` (Lage, Größe,
umgebrochene `zeilen`, `schriftgroesse`), `kanten: Vec<KantenLauf>` (Stützpunkte,
`beschriftung`) und `gruppen: Vec<GruppenRahmen>`.

- **Geschichtet:** Ränge über den längsten Weg, Kreise über Rückkanten, Reihenfolge über
  Baryzentrum (8 Runden, Gleichstand entscheidet der Modellindex). Gerechnet wird in einem
  Raster aus **Fluss** (in Leserichtung) und **Quere** (quer dazu); derselbe Code gilt für
  `unten` und `rechts`.
- **Textmaß** aus der eingebetteten Schrift; die Knotengröße folgt dem Text. Die Form
  vergrößert den Kasten (Raute ×2,0/×2,1, Ellipse ×1,45/×1,5, Zylinder +24,
  Notiz +12), damit der Text hineinpasst.
- **Keine überlappenden Knoten** (Test über viele zufällige Graphen mit festem Startwert).
- **Schichtdicke = breitester Knoten der Schicht** (nicht die Summe): zwischen zwei
  Schichten bleibt nur der Abstand, der wirklich nötig ist.
- **Gruppen sind Bänder.** Jede Gruppe belegt einen Streifen quer zur Leserichtung, der in
  ihren Schichten reserviert bleibt. Daraus folgen drei Zusagen, die Tests über viele
  zufällige Graphen prüfen: ein Rahmen **umschließt nur seine Mitglieder** (kein fremder
  Knoten berührt ihn), **zwei Rahmen überlappen sich nie**, und die Mitglieder liegen
  zusammen. Bänder, deren Schichten sich nicht überlappen, teilen sich einen Streifen –
  so bleibt das Bild kurz. Eine leere Gruppe bekommt einen kleinen Rahmen unter dem
  Diagramm.
- **Kanten** verlassen ihren Knoten an einer **eigenen Stelle** (parallele Kanten treten
  getrennt aus, je Kante eine Bahn im Zwischenraum), laufen zwischen den Schichten und
  steigen dort in den Zielknoten – bei Raute und Ellipse auf der schrägen bzw. runden
  Fläche, nicht am Kastenrand. Kein Weg führt durch einen fremden Knoten und keiner durch
  einen fremden Rahmen: ist ein Rahmen im Weg, sucht der Router im Raster einen Weg außen
  herum. Rückkanten und Sprünge laufen um den ganzen Inhalt (Kanal rechts bei `unten`,
  Kanal unten bei `rechts`).
- **Kantenbeschriftungen** sitzen mittig auf einem Stück **ihrer** Kante, weichen fremden
  Knoten, fremden Linien und anderen Beschriftungen aus und werden notfalls schmaler
  umbrochen. Reichlich Platz ist knapp: in sehr dichten Diagrammen kann eine Beschriftung
  eine fremde Linie kreuzen (der deckende Hintergrund hält sie lesbar).
- **Gleiche Eingabe → gleiche Ausgabe:** kein Zufall, keine Hash-Reihenfolge.
- Konstanten (`SCHRIFTGROESSE`, `ZEILENHOEHE`, Abstände, Ränder) sind öffentlich, damit
  Werkzeuge dieselben Maße verwenden.

## Zeichnen – Formenmodell, SVG, PNG

```rust
use quiver_diagramm::zeichnen::{Szene, Figur, Rechteck, Text, Ausrichtung};

let mut szene = Szene::leer(400.0, 200.0);
szene.formen.push(Figur::Rechteck { mass: Rechteck::neu(20.0, 20.0, 160.0, 60.0),
                                    radius: 8.0, farbe: Some(Farbe::Gruen) });
szene.formen.push(Figur::Text {
    mass: Rechteck::neu(20.0, 20.0, 160.0, 60.0),
    text: Text { zeilen: vec!["Hallo".into()], groesse: 14.0,
                 ausrichtung: Ausrichtung::Mitte, farbe: None },
});
let svg = szene.svg();       // eigenständiges SVG
let png = szene.png(2000)?;  // Vec<u8>, PNG
```

- **Figuren:** `Rechteck` (mit Radius), `Ellipse`, `Raute`, `Zylinder`, `Notiz`,
  `Container` (Rahmen mit Kopfzeile, z. B. Schwimmbahn/Frame), `Text`, `Linie` (gestrichelt,
  mit Pfeilspitze), `Platzhalter` (Bild, das die Bibliothek nicht zeichnet).
- **`Szene::aus_zeichnung`** legt hinter jede Kantenbeschriftung ein deckendes Rechteck
  (`BESCHRIFTUNG_RAND`), damit sie auf ihrer Linie lesbar bleibt.
- **SVG:** `&`, `<`, `>`, `"`, `'` werden maskiert, Steuerzeichen fallen weg; kein
  `<script>`, kein `<image>`, kein `href`, keine `url(…)`, kein externer Verweis. Ein
  Knotentext kann also kein Markup einschleusen.
- **PNG:** über `resvg`; die Schrift **IBM Plex Sans** ist eingebettet und wird aus den
  Bytes geladen (keine Systemschriften), die Kantenlänge ist auf **2000 px** begrenzt.
- **Farben:** `zeichnen::palette::toene(farbe)` – der **einzige Ort mit Farbwerten**.
- **Textmaß und Umbruch:** `zeichnen::schrift::{breite, zeilenhoehe, umbrechen}` –
  dieselbe Schrift und dieselben Zahlen wie im Layout.

Die Schrift liegt unter `schriften/PlexSans-Regular.ttf` mit
`schriften/OFL.txt` (SIL Open Font License 1.1, © IBM Corp.). Sie darf mitgeliefert
werden; die Lizenz muss dabei bleiben.

## Was die Werkzeuge (17/18) hier finden

| Bedarf | Aufruf |
|---|---|
| Datei lesen, Stand merken | `ablage.lesen` → `Inhalt.text` + `Inhalt.stand` |
| Ändern nur mit Stand | `ablage.aendern(..., &stand)` |
| Neu anlegen | `ablage.anlegen` (nie über Vorhandenes) |
| Bild bauen | eigene `Szene` füllen → `Szene::png(2000)` |
| Export schreiben | `ablage.ausgabe_schreiben("plan.svg", svg.as_bytes())` |
| Text messen und umbrechen | `zeichnen::schrift::breite` / `umbrechen` |
| Modell prüfen | `Diagramm::pruefen` (Meldungen unverändert weitergeben) |
| Agenten-Stand prüfen | `Stand::aus_hex(&text)` → `None` heißt „kein SHA-256“ |

## Tests

```bash
CARGO_TARGET_DIR=target-pi cargo test -p quiver-diagramm
```

- `tests/ablage.rs`: jede Ablehnungsregel (auch Symlinks auf Datei- und Ordnerebene),
  Anlegen ohne Überschreiben, Ändern mit falschem Stand, **gleichzeitiges Ändern** (zwei
  Threads, einer gewinnt), genau eine Sicherung, Größen- und Endungsgrenzen, Ausgaben.
- `tests/layout.rs`: **80 zufällige Graphen** (fester Startwert, beide Richtungen, alle
  Formen und Farben, Gruppen, Kreise): keine überlappenden Knoten, Text passt, Gruppen
  umschließen, kein Kantenpunkt in einem Knoten, gleiches Layout bei gleicher Eingabe.
  Dazu die Zusagen der Abnahme: Rahmen umschließen nur ihre Mitglieder und überlappen sich
  nie, Kanten meiden fremde Knoten und fremde Rahmen, parallele Kanten treten getrennt aus,
  Beschriftungen liegen frei auf ihrer Kante (mit deckendem Hintergrund), die Schichtdicke
  folgt dem breitesten Knoten – und alle Zusagen auf einmal für die beiden großen
  Abnahmebilder (`tests/common`, 25 Knoten mit drei Gruppen, Kreis mit Mehrfachkanten).
- `tests/zeichnen.rs`: XML-/SVG-Einschleusung über Texte, wohlgeformtes SVG ohne
  Ereignis-/Verweisattribute, PNG-Maße und -Grenze, eingebettete Schrift wird wirklich
  gezeichnet (dunkle Bildpunkte), Umbruch hält die Breite ein.
- Alle Tests laufen in temporären Ordnern (`tempfile`), keiner im Benutzerverzeichnis.

## Grenzen

Kein freies Layout, keine frei wählbaren Ports, keine Kreuzungsoptimierung über das
Baryzentrum hinaus, keine Bilder, keine Farbverläufe, kein Löschen. Beschriftungen werden
nach bestem Bemühen gesetzt: in sehr dichten Diagrammen kann eine fremde Linie unter einer
Beschriftung liegen. Was fehlt, steht in [docs/offen.md](../../docs/offen.md) und im ADR.
