# 18 – quiver-drawio

Zuerst `16-diagramme.md`. ADR 0012. Formatquellen (nur lesen): lokaler Arbeitsordner (`drawio`) (`mxfile.xsd`, Referenzen; `index.js` zeigt den Link).

- `mxfile > diagram(id,name) > mxGraphModel > root > mxCell`, Zellen 0 und 1, mehrere Seiten (`seite` als Id oder Name; mehrdeutig → Kandidaten; `drawio_seite_hinzufuegen`). **Komprimierte** `diagram`-Inhalte lesen (Base64 → raw inflate → URL-dekodieren, höchstens 20 MB entpackt) und unkomprimiert schreiben.
- XML sicher: DTD/Entities ablehnen, Tiefe und Größe begrenzt, Attribute sauber maskiert. `UserObject`/`object`-Hüllen und HTML-Labels erhalten. Neue Labels als Text (HTML maskiert). Stil-Zeichenkette einzeln ändern, Unbekanntes bleibt. Keine `link`s außer http(s)/mailto.
- Kanten mit `source`/`target` auf vorhandene Zellen; Entfernen eines Knotens entfernt oder löst seine Kanten **nur** mit ausdrücklicher Angabe, Kinder ebenso.
- `drawio_bild`: eigener Zeichner für rounded/rectangle, ellipse, rhombus, cylinder, note, text, swimlane/Container, Kanten; Unbekanntes als Rechteck, die Antwort nennt es.
- `drawio_link`: `https://app.diagrams.net/#R…` (Inhalt nur im Fragment, geht nicht an den Server; Verfahren aus `index.js`). Antwort sagt, dass der Link das **ganze Diagramm enthält**. Höchstens 1 MB.
- Kein Aufruf von draw.io Desktop (in „Offen“).

## Im Browser öffnen (Nachtrag, Wunsch des Nutzers)

- Alle Werkzeuge laufen **ohne Anmeldung**; es gibt keine Zugangsdaten.
- `drawio_oeffnen` (nicht lesend, nicht destruktiv): öffnet genau diesen `#R`-Link im Standardbrowser (Crate `open`, keine Shell). Nur der selbst gebaute Link, nie eine Adresse vom Agenten. Antwort: Änderungen im Browser landen **nicht** in der lokalen Datei; dort speichern und die Datei in den Arbeitsordner legen. Ist der Link zu lang für das Betriebssystem, Meldung mit dem Dateipfad statt Fehler.
- `drawio_link` liefert den Link nur als Text.
- Der Browser-Start liegt hinter einem Trait; Tests öffnen nie einen Browser.
