# 0012 – draw.io: eigenes XML, Seiten entpacken, Link im Fragment

> Stand: 2026-11-24. Aufgabe: [docs/aufgaben/18-drawio.md](../../docs/aufgaben/18-drawio.md).
> Baut auf [0013-diagramme-lokal.md](0013-diagramme-lokal.md) auf (Ablage, Modell, Layout,
> Zeichner). Schwester: [0011-excalidraw.md](0011-excalidraw.md).

## Kontext

`quiver-drawio` soll `.drawio`-Dateien lesen und ändern, Diagramme aus einer Beschreibung
anlegen, sie als Bild zurückgeben, einen Link bauen und im Browser öffnen. Der Unterbau
steht: `crates/diagramm` liefert Ablage, Modell, Layout und den eigenen Zeichner. Für das
Format selbst steht **nichts** bereit – und es gibt vier Entscheidungen, die es prägen:

1. **Wie wird das XML gelesen?** draw.io schreibt `mxfile → diagram → mxGraphModel →
   root → mxCell`, mit `UserObject`-Hüllen, HTML-Labels und beliebig vielen Attributen.
   Dazu kommen **komprimierte** Seiten (Base64 + raw deflate + Prozentkodierung).
2. **Wie bleibt fremde Arbeit erhalten?** Ein Agent ändert einen Text; alles andere im
   Dokument – Stile, Geometrie, Metadaten, andere Seiten – muss danach noch da sein.
3. **Was darf ein Agent entfernen?** Knoten hängen an Kanten, Container an Kindern.
   Ein Löschen ohne Rückfrage reißt Lücken.
4. **Wie kommt das Bild in den Browser, ohne etwas hochzuladen?** draw.io kennt einen
   Link, der das ganze Diagramm im Fragment trägt.

## Entscheidung

### 1. Eigener, strenger XML-Leser statt einer Formatbibliothek

`crates/drawio/src/xml.rs` ist ein kleiner Baum (`Element`/`Knoten`) mit eigenem Leser und
Schreiber. Aufgabe 18 verlangt: **DTD und Entitäten ablehnen**, Tiefe und Größe begrenzen,
Attribute sauber maskieren. Ein Leser, der genau das kann, ist kleiner als die
Konfiguration einer fertigen Bibliothek – und er lässt weg, was gefährlich ist:

- `<!DOCTYPE`/`<!ENTITY` → Fehler; nur `&amp; &lt; &gt; &quot; &apos;` und
  Zeichenverweise werden aufgelöst, jede andere Entität ist ein Fehler mit Stelle.
- `CDATA`, mehrere Wurzeln, Text nach dem Wurzelelement → Fehler.
- Höchstens 64 Ebenen; die Größe begrenzt die Ablage (10 MB Datei) und der Entpacker
  (20 MB Seite).
- Geschrieben wird in der Reihenfolge der Datei; `"` und Zeilenumbrüche werden maskiert.

**Preis:** Namensräume, Verarbeitungsanweisungen mit Inhalt und Kommentar-Erhalt gibt es
nicht (Kommentare werden übersprungen). Das genügt draw.io, das keine Namensräume schreibt.

### 2. Seiten: komprimiert lesen, unkomprimiert schreiben

Eine Seite enthält entweder ein `<mxGraphModel>` oder komprimierten Text. Gelesen wird
beides: Base64 → raw inflate (`flate2`) → Prozentdekodierung (`encodeURIComponent`-
Gegenstück, selbst geschrieben), höchstens 20 MB entpackt. Geschrieben wird **immer
unkomprimiert**, und das `compressed`-Attribut verschwindet. Damit ist jede Datei nach
einer Änderung für Menschen und Werkzeuge gleich lesbar.

`seite` ist eine **Id oder ein Name**. Passt ein Name auf mehrere Seiten, antwortet das
Werkzeug mit den Kandidaten (Nummer, Name, Id) statt zu raten; eine neue Seite bekommt
`seite-1`, `seite-2`, … (`drawio_seite_hinzufuegen`).

**Preis:** Die Datei wird beim ersten Schreiben größer (unkomprimiert). Für draw.io ist das
das normale Format; ein paar hundert Zellen bleiben weit unter 10 MB.

### 3. Erhalten, was man nicht versteht

Der Baum bleibt die Wahrheit. Zellen werden über ihre Id gesucht und **einzeln** geändert:

- `UserObject`/`object`-Hüllen bleiben Hüllen; `label` bleibt `label`, die innere
  `mxCell` behält Stil und Geometrie.
- HTML-Labels werden **gelesen**, aber nicht neu geschrieben. Ein neuer Text wird als Text
  gesetzt – die XML-Maskierung macht aus `<b>` das sichtbare Zeichen `<b>`.
- Die Stil-Kette wird in ihre `;`-Teile zerlegt (`stil.rs`). Geändert werden nur das
  Formwort, `shape` und die Schlüssel der neuen Form; `sketch`, `fontFamily` oder was eine
  andere draw.io-Version sonst schreibt, bleibt stehen.
- Unbekannte Attribute an `mxfile`, `diagram`, `mxGraphModel` und Zellen, unbekannte
  Geometrie-Kinder (`mxPoint`, `Array`, `mxRectangle`) und unberührte Zellen werden
  inhaltsgleich wieder geschrieben.
- Farben sind semantische Namen; ihre Hex-Werte kommen aus `zeichnen::palette` – dieses
  Werkzeug erfindet keine Werte.

**Preis:** Der Baum ist kein zeichengetreues Abbild (Maskierung wird vereinheitlicht,
Kommentare fallen weg). „Inhaltsgleich“ heißt: dieselben Elemente, Attribute und Texte in
derselben Reihenfolge.

### 4. Prüfung vor dem Schreiben

`pruefe_modell` lehnt ab, was draw.io nicht laden würde: fehlende Zellen `0`/`1`, doppelte
Ids, mehr als 5000 Zellen, Kanten mit `source`/`target` auf unbekannte oder nicht-Knoten,
Links mit fremdem Schema (`javascript:` u. ä. – erlaubt sind nur `http(s)` und `mailto`).
`mx::aendern` prüft **alle** Änderungen, bevor es die erste anwendet; eine unbekannte Id
lässt das Dokument unverändert.

### 5. Entfernen nur mit ausdrücklicher Angabe

Ein Knoten mit Kanten wird **nicht** stillschweigend entfernt: `kanten` muss `entfernen`
oder `loesen` sagen (Kante behalten, `source`/`target` löschen). Dasselbe für Kinder:
`kinder: entfernen` (rekursiv, samt ihrer Kanten) oder `kinder: behalten` (hochziehen zum
nächsten bleibenden Elternteil). Fehlt die Angabe, endet der Aufruf mit den betroffenen
Ids, und es wird nichts geschrieben. Zellen `0` und `1` sind unantastbar.

**Warum:** Ein Modell kann die Folgen einer Löschung nicht übersehen. Die Rückfrage kostet
einen Aufruf; ein zerrissenes Diagramm kostet die Arbeit.

### 6. Bild mit dem eigenen Zeichner

`drawio_bild` und `drawio_exportieren` übersetzen Zellen in eine `Szene` aus
`quiver-diagramm` und zeichnen mit `resvg`. Gezeichnet werden die Formen aus Aufgabe 18:
Rechteck (`rounded`, `rectangle`), Ellipse, Raute, Zylinder (`shape=cylinder3`), Notiz
(`shape=note`), Text, Container/Schwimmbahn und Kanten (Stützpunkte, `dashed=1`,
`endArrow=none`). Absolute Lagen entstehen aus den Elternketten; HTML-Labels werden zu
Text; Füllfarben werden auf die feste Palette abgebildet.

**Unbekanntes wird als Rechteck gezeichnet, und die Antwort nennt den Namen.** Eine Kante
ohne Enden wird nicht gezeichnet und genannt.

**Preis:** Strich- und Schriftfarben, Pfeilvarianten, Bilder und Verläufe sieht das Bild
vereinfacht. Die Datei selbst bleibt vollständig.

### 7. Link im Fragment, Browser hinter einem Trait

`drawio_link` baut `https://app.diagrams.net/?grid=0&pv=0&border=10&edit=_blank#R…`:
`encodeURIComponent(XML)` → raw deflate → Base64 → prozentkodiert, wie in `index.js` der
Vorlage. **Der Inhalt steht nur im Fragment** und geht nie an einen Server; die Antwort
sagt das und nennt die Grenze von 1 MB.

`drawio_oeffnen` öffnet genau diesen Link über das Crate `open` (keine Shell). Änderungen
im Browser landen **nicht** in der lokalen Datei – die Antwort nennt den Pfad und den Weg
zurück. Ist der Link länger als 2000 Zeichen (Grenze des Startbefehls unter Windows), wird
nichts geöffnet; die Antwort nennt den Dateipfad statt eines Fehlers.

Der Browser-Start liegt hinter `browser::Browser`; die Tests setzen einen Aufzeichner ein
und öffnen **nie** einen Browser. Eine Adresse vom Agenten wird nie geöffnet – nur der
selbst gebaute Link.

**Warum kein `#create=`-Link und kein Hochladen:** `#create` lässt draw.io die Adresse nach
dem Erzeugen umschreiben und nachfragen („All changes will be lost“); ein Upload würde das
Diagramm an einen fremden Server geben. `#R` bleibt unverändert und lokal.

**Preis:** Sehr große Diagramme passen nicht in eine Adresse (1 MB, 2000 Zeichen für den
Startbefehl). Dann bleibt der Weg über die Datei.

## Folgen

- `quiver-drawio` benutzt `crates/diagramm` nur; das Format lebt im Werkzeug-Crate
  (`xml`, `mx`, `stil`, `formen`, `link`, `aus_modell`, `bild`).
- Die Regeln aus Aufgabe 16 gelten unverändert: Felder `ordner` und `schreiben`
  (Standard an), `stand` aus `drawio_lesen`, nie über Vorhandenes, kein Löschen von
  Dateien, Sicherung `.bak`, Sperre über Prozesse.
- Tests laufen nur in temporären Ordnern; kein Test öffnet einen Browser oder ein Netz.
- Was an der Schnittstelle von `crates/diagramm` fehlt (Farbe aus Hex, Strich-/Schriftfarbe,
  Kanten-Ids, Gruppen-Id gleich Knoten-Id), ist im Crate gelöst und in dessen
  `README.md` unter „Offen“ vermerkt.

## Grenzen, die bewusst bleiben

- Kein Aufruf von draw.io Desktop; kein Hochladen, kein Konto, kein Netz.
- Kein Erhalt von Kommentaren im XML, keine Namensräume.
- Bilder/`files` in der Datei werden als Platzhalter gezeichnet.
- Keine freien Farben, Verläufe, Schatten oder Pfeilvarianten im Bild.
- Ein `.bak` je Datei, keine Historie; Löschen gibt es nur für Zellen, nicht für Dateien.
