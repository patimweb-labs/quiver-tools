# 0011 – Excalidraw-Dateien als MCP-Werkzeug

> Stand: 2026-11-24. Aufgaben: [docs/aufgaben/17-excalidraw.md](../../docs/aufgaben/17-excalidraw.md),
> Unterbau [0013-diagramme-lokal.md](0013-diagramme-lokal.md).

## Kontext

`quiver-excalidraw` soll Diagramme als `.excalidraw` erzeugen und ändern: ein Agent
beschreibt, was zusammenhängt, das Werkzeug schreibt eine Datei, die auf
[excalidraw.com](https://excalidraw.com) oder in Obsidian aufgeht – und der Agent sieht
sein Ergebnis als Bild.

Vier Fragen waren zu entscheiden:

1. **Wie treu muss das Format sein?** Excalidraw lädt auch halbe Dateien, weil
   `restore.ts` fehlende Felder auffüllt. Ein Werkzeug, das sich darauf verlässt,
   erzeugt Dateien, die beim Öffnen anders aussehen als berechnet.
2. **Was passiert mit dem, was das Werkzeug nicht kennt?** Eine Datei kann Bilder,
   eingebettete Schriften, `customData`, handgezeichnete Striche und Elementarten
   enthalten, die es in diesem Crate nicht gibt.
3. **Womit wird gezeichnet?** excalidraw-mcp ist ein Netzdienst; ein Browser wäre eine
   zweite Laufzeit.
4. **Wie kommt die Datei zum Menschen?** Excalidraw kennt keinen Link, der nur den Inhalt
   im Fragment trägt.

## Entscheidung

### 1. Ein eigenes Crate, das nur das Format kennt

`crates/excalidraw` (Programm `quiver-excalidraw`) benutzt `crates/diagramm` und kennt
sonst nichts. Die Bibliothek kennt kein Format; dieses Crate kennt kein Layout. Es gilt
weiter `diagramm ← <werkzeug>`, wie `kern ← <werkzeug>`.

| Modul | Aufgabe |
|---|---|
| `datei` | das Format: `Datei`, `Element`, `Binding`, Rest-Maps |
| `pruefen` | die strenge Prüfung gegen die Formatquellen |
| `bauen` | Modell → Elemente; Formen, Texte, Pfeile, Randpunkte |
| `aendern` | Text, Farbe, Form, Lage, Verbinden, Lösen, Entfernen |
| `bild` | Datei → `zeichnen::Szene` → SVG/PNG |
| `farben` | Namen ↔ Werte, Formen, Kantenstile |
| `browser` | wer excalidraw.com öffnet (hinter einem Trait) |
| `format` | reine Funktionen: Daten → Text |
| `server` | die elf MCP-Werkzeuge |

### 2. Volle Pflichtfelder, beidseitige Bindungen, strenge Prüfung beim Abgeben

Jedes Element trägt die **25 Pflichtfelder** der Quellen (`newElement.ts`,
`restore.ts`): `id, type, x, y, width, height, angle, strokeColor, backgroundColor,
fillStyle, strokeWidth, strokeStyle, roughness, opacity, seed, version, versionNonce,
index, isDeleted, groupIds, frameId, boundElements, updated, link, locked`. `version` und
`versionNonce` steigen bei jeder Änderung, `updated` ist der Zeitpunkt.

Text in einer Form steht **beidseitig**: `text.containerId` → Form und
`form.boundElements[{type:"text",id}]` → Text. Ein Pfeil steht **beidseitig**:
`arrow.startBinding/endBinding.elementId` → Element und
`element.boundElements[{type:"arrow",id}]` → Pfeil. Entfernen löst die Gegenseite mit auf,
Verschieben zieht Texte und Pfeilenden mit.

`excalidraw_aus_quelltext` prüft das **am rohen JSON** (`pruefen::pruefen`), nicht an der
großzügig gelesenen Struktur: fehlende Pflichtfelder, doppelte oder leere Ids, negative
Maße, `opacity` außerhalb 0–100, `link` außerhalb `https`/`http`/`mailto`, Bindungen, die
nur auf einer Seite stehen oder ins Leere zeigen. Die Meldungen nennen den Ort
(`elements[2] (id „a“): …`) und kommen gesammelt, damit ein Aufruf reicht.

**Warum streng, obwohl Excalidraw tolerant ist:** Ein Agent, der Felder vergisst, bekommt
sonst ein Bild, das beim Öffnen anders aussieht als berechnet. Die Toleranz bleibt beim
**Lesen** fremder Dateien – dort gelten die Standardwerte von `restore.ts`, damit ein
altes Bild nicht unlesbar wird.

### 3. Unbekanntes bleibt stehen

`Element` hat die Felder typisiert, die das Werkzeug liest und schreibt; alles andere
landet in einer Rest-Map (`#[serde(flatten)]`) und geht unverändert zurück. Dasselbe gilt
für die Datei selbst (`appState`, `files`, fremde Schlüssel) und für Elemente, deren Art
das Crate nicht kennt (`embeddable`, `iframe`, `stickynote`, `magicframe` …).

**Warum:** Ein Mensch arbeitet in derselben Datei. Ein Werkzeug, das beim Ändern eines
Textes ein eingebettetes Bild oder ein selbst geschriebenes Feld verliert, ist unbrauchbar
– auch wenn die Änderung „nur“ einen Text betraf.

**Preis:** Die Reihenfolge der Schlüssel im JSON folgt der Struktur, nicht der Eingabe
(inhaltlich gleich). Unbekannte Felder werden nicht interpretiert, also auch nicht
geprüft.

### 4. Selbst zeichnen, mit derselben Schrift wie das Layout

`excalidraw_bild` und `excalidraw_exportieren` bauen aus der Datei eine
`diagramm::zeichnen::Szene` und lassen sie als SVG oder PNG (resvg, „IBM Plex Sans“
eingebettet) ausgeben. Gezeichnet werden `rectangle`, `diamond`, `ellipse`, `line`,
`arrow`, `freedraw` und `frame`; `image` wird zum Platzhalter. Was übrig bleibt, **nennt
die Antwort** mit Art und Id.

**Warum kein excalidraw-mcp und kein Browser:** Beide sind eine zweite Laufzeit und ein
Netzdienst; beides hat dieses Repo nicht. Eigene Formen sind wenig Code und überall gleich.

**Preis:** Das Bild ist eine Annäherung. Die Bibliothek kennt nur die feste Farbpalette,
Linien ohne Farbe und ohne Strichstärke, keine Bilder und keine handgezeichnete
Strichlage. Farben aus excalidraw.com werden auf den nächsten Ton abgebildet; Details
stehen in der [README des Crates](../../crates/excalidraw/README.md#offen).

### 5. Öffnen: feste Adresse, kein Hochladen

`excalidraw_oeffnen` startet den Standardbrowser mit `https://excalidraw.com` und nennt
den vollen Pfad der Datei mit dem Hinweis, sie in das Fenster zu ziehen. Es gibt keinen
Link, der den Inhalt nur im Fragment trägt (`#json=` lädt hoch) – darum wird nichts
hochgeladen. `nur_link: true` öffnet nichts und antwortet nur.

Der Browser liegt hinter einem Trait (`browser::Browser`). Die Tests setzen einen
Browser ein, der nur mitschreibt; **kein Test öffnet einen Browser.**

**Preis:** Ein Klick mehr für den Menschen (Datei hineinziehen). Dafür verlässt kein
Inhalt das Gerät, und es gibt keinen Zugang, der ablaufen könnte.

### 6. Dateien nur über die Ablage

Der Arbeitsordner (Feld `ordner`), relative Pfade, `stand` (SHA-256) vor jeder Änderung,
atomares Schreiben, genau eine Sicherung, Sperre über Prozesse, kein Löschen von Dateien:
alles aus `diagramm::ablage`, nichts davon hier noch einmal. Der Schalter `schreiben`
(Standard **an**) steht davor: ist er aus, antworten alle verändernden Werkzeuge mit dem
Hinweis und schreiben nichts.

## Folgen

- `quiver-excalidraw` hat **keine** Zugangsdaten. Der Bibliothekseintrag hat zwei Felder;
  es gibt nichts, was in einem Log, einer Antwort oder einem Test auftauchen könnte.
- Aufgabe 18 (`quiver-drawio`) kann die Struktur übernehmen: `datei`, `pruefen`,
  `bauen`, `aendern`, `bild`, `format`, `server`.
- Die Prüfung ist doppelt nützlich: sie ist die Abnahme für `_aus_quelltext` und, als
  Selbstprüfung, die Bremse vor jedem Schreiben – ein Fehler im eigenen Bauen fällt auf,
  bevor eine Datei entsteht.

## Grenzen, die bewusst bleiben

- Kein Hochladen, kein gemeinsames Bearbeiten, kein Excalidraw-Plus.
- Kein freies Platzieren auf Koordinaten: `_erstellen` nimmt ein Modell, das Layout rechnet.
- `freedraw` nur als Linienzug; Druckstärken werden erhalten, aber nicht gezeichnet.
- Zylinder und Notiz werden zum Rechteck (Excalidraw kennt sie nicht).
- `index` bleibt `null`; die Reihenfolge ist die Reihenfolge in `elements`.
