# 17 – quiver-excalidraw

Zuerst `16-diagramme.md`. ADR 0011. Formatquellen (nur lesen): lokaler Arbeitsordner (`excalidraw`).

- Gültige Datei: `type "excalidraw"`, `version 2`, `source`, `elements`, `appState`, `files`. Elemente mit **allen** Pflichtfeldern nach den Quellen (`id, type, x, y, width, height, angle, strokeColor, backgroundColor, fillStyle, strokeWidth, strokeStyle, roughness, opacity, seed, version, versionNonce, index, isDeleted, groupIds, frameId, boundElements, updated, link, locked` …). `version`/`versionNonce` bei Änderung erhöhen.
- Text in Formen über `containerId` + `boundElements` (beidseitig), Pfeile über `startBinding`/`endBinding` **und** `boundElements`. Entfernen löst die Gegenseite mit auf; Verschieben zieht gebundene Texte und Pfeile mit.
- `link` nur `https`/`http`/`mailto`. Bilder (`files`) bleiben unberührt erhalten.
- `excalidraw_bild`: rectangle, diamond, ellipse, line, arrow, text, freedraw, frame; image als Platzhalter. Nicht Gezeichnetes nennt die Antwort.
- Test: erzeugte Dateien gegen eine aus den Quellen abgeleitete Prüfung (Pflichtfelder, gegenseitige Bindungen). Öffnen/Teilen über excalidraw.com würde Inhalte hochladen – gibt es **nicht** (README: Datei dort von Hand öffnen).

## Im Browser öffnen (Nachtrag, Wunsch des Nutzers)

- Alle Werkzeuge laufen **ohne Anmeldung**; es gibt keine Zugangsdaten.
- `excalidraw_oeffnen` (nicht lesend, nicht destruktiv): öffnet `https://excalidraw.com` im Standardbrowser (Crate `open`, feste Adresse, keine Shell) und nennt den vollen Pfad mit dem Hinweis „Datei in das Fenster ziehen“. Excalidraw kennt keinen Link, der den Inhalt nur im Fragment trägt (`#json=` lädt hoch) – darum kein Hochladen. `nur_link: true` öffnet nichts und antwortet nur.
- Der Browser-Start liegt hinter einem Trait; Tests öffnen nie einen Browser.
