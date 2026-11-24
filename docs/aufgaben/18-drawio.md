# 18 – quiver-drawio

Zuerst `16-diagramme.md`. ADR 0012. Formatquellen (nur lesen): /private/tmp/claude-501/-Users-patrickweppelmann-Documents-Workspace/51619e8b-8735-441c-bc49-6a3f3919e02d/scratchpad/diagramme/drawio (`mxfile.xsd`, Referenzen; `index.js` zeigt den Link).

- `mxfile > diagram(id,name) > mxGraphModel > root > mxCell`, Zellen 0 und 1, mehrere Seiten (`seite` als Id oder Name; mehrdeutig → Kandidaten; `drawio_seite_hinzufuegen`). **Komprimierte** `diagram`-Inhalte lesen (Base64 → raw inflate → URL-dekodieren, höchstens 20 MB entpackt) und unkomprimiert schreiben.
- XML sicher: DTD/Entities ablehnen, Tiefe und Größe begrenzt, Attribute sauber maskiert. `UserObject`/`object`-Hüllen und HTML-Labels erhalten. Neue Labels als Text (HTML maskiert). Stil-Zeichenkette einzeln ändern, Unbekanntes bleibt. Keine `link`s außer http(s)/mailto.
- Kanten mit `source`/`target` auf vorhandene Zellen; Entfernen eines Knotens entfernt oder löst seine Kanten **nur** mit ausdrücklicher Angabe, Kinder ebenso.
- `drawio_bild`: eigener Zeichner für rounded/rectangle, ellipse, rhombus, cylinder, note, text, swimlane/Container, Kanten; Unbekanntes als Rechteck, die Antwort nennt es.
- `drawio_link`: `https://app.diagrams.net/#R…` (Inhalt nur im Fragment, geht nicht an den Server; Verfahren aus `index.js`). Antwort sagt, dass der Link das **ganze Diagramm enthält**. Höchstens 1 MB. Es wird nichts geöffnet.
- Kein Aufruf von draw.io Desktop (in „Offen“).
