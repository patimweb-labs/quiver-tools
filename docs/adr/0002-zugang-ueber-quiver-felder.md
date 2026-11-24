# 0002 – Zugang über quiver-Felder statt Einrichtungsseite

> Stand: 2026-11-23. Aufgabe: [docs/aufgaben/03-zugang-aus-umgebung.md](../../docs/aufgaben/03-zugang-aus-umgebung.md).

## Kontext

Nach Aufgabe 02 richtete der User Zugänge auf einer eigenen Seite ein: `quiver-<tool>
einrichten` startete axum auf `127.0.0.1`, öffnete den Browser und schrieb die Profile in
`<konfig>/quiver-tools/<tool>/profile.toml`, die Geheimnisse in den Schlüsselbund. Dafür
gab es im Kern die Module `seite` (Routen, Formulare, HTML), `browser` (Seite öffnen) und
`tresor` (Schlüsselbund), zusammen etwa 1.900 Zeilen samt Tests, und im Bibliothekseintrag
das Feld `einrichtung`.

Neu ist die Oberfläche von quiver selbst: ein Bibliothekseintrag nennt seine Felder
(`quelle` `client` oder `geheim`), quiver zeigt sie unter **Tools → <Titel>** an, legt
Geheimes in den Schlüsselbund und setzt beim Start des Tools die Umgebung
(`<NAME>_<FELD>`, z. B. `OUTLINE_URL`, `OUTLINE_API_KEY`). Damit gibt es für eine zweite
Oberfläche im Browser keinen Grund mehr.

## Entscheidung

Die Einrichtungsseite fällt weg. Der Kern liest den Zugang aus der Umgebung
(`umgebung::Zugang`), prüft die Pflichtfelder aus `Werkzeug::felder()` und bildet genau
ein Profil `standard`. `quiver-<tool> eintrag` gibt den Bibliothekseintrag als JSON im
Format von `quiver_domain::Eintrag` aus; quiver liest daraus die Felder und die Laufzeit.
`quiver-<tool> testen` prüft den Zugang von Hand. Werkzeuge nehmen keinen
`profil`-Parameter mehr.

Entfernt wurden: `seite` (inkl. `html`, `stil.css`), `browser`, `tresor`, der
Profilspeicher `Profile` samt `profile.toml`, der Unterbefehl `einrichten`, das
`einrichtung`-Feld im Eintrag, der `profil`-Parameter und die Suche über alle Profile.
`Werkzeug::testen` bleibt (für einen späteren „Testen“-Knopf in quiver).

## Warum so

1. **Eine Oberfläche statt zwei.** quiver zeigt die Felder des Eintrags bereits; eine
   zweite Seite müsste dieselben Regeln (Pflichtfelder, Geheimnisse, Optik, Themes,
   Hand-Cursor) noch einmal umsetzen und pflegen.
2. **Weniger Angriffsfläche.** Kein lokaler HTTP-Server, kein Einmal-Token, kein
   Browser-Start im Tool. Geheimnisse kommen nur als Umgebungsvariable im Prozess an.
3. **Der Vertrag ist der Eintrag.** `quiver_domain::Eintrag` beschreibt Felder und
   Laufzeit; das Tool fügt nichts hinzu, was quiver nicht ohnehin versteht.
4. **Der Kern bleibt dumm.** Er kennt weder quiver noch den Schlüsselbund; er liest
   `std::env` und prüft `Werkzeug::felder()`. Das ist testbar ohne Prozesszustand
   (`Zugang::mit_umgebung`) und ohne Systemdienste.

## Preis

- **Vorerst ein Zugang je Tool.** Mehrere Wikis, Postfächer oder Hosts brauchen mehrere
  Bibliothekseinträge; ein `profil`-Parameter und eine Suche über alle Profile gibt es
  nicht mehr.
- **Die Umgebung ist die Schnittstelle.** Ein Tool kann seinen Zugang nicht zur Laufzeit
  wechseln; quiver muss den Prozess mit anderer Umgebung neu starten.
- **Kein „Verbindung testen“-Knopf im Tool.** `testen` ist ein Unterbefehl für die
  Kommandozeile; der Knopf käme von quiver, sobald es ihn gibt.
- **Einrichtung von Hand ist nicht mehr möglich.** Wer kein quiver benutzt, setzt die
  Umgebungsvariablen selbst (dokumentiert in `README.md`).
