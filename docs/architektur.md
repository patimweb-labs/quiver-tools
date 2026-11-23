# Architektur

> Stand: 2026-11-23, nach Aufgabe 02. Gilt für den Kern (`crates/kern`) und alle
> Tools, die darauf aufsetzen. Festlegungen und Preise: [../SPEC.md](../SPEC.md).

## Richtung

```
crates/kern        Werkzeug-Trait, Profilspeicher, Tresor, Einrichtungsseite, MCP-Hilfen
crates/<tool>      ein Programm quiver-<tool> je Tool
assets/            tokens.css und Schriften (Kopie aus mcp-quiver, nicht von Hand ändern)
```

`kern ← <tool>`: ein Tool kennt den Kern, der Kern kennt kein Tool. Alles, was ein
Tool dem Kern über sich sagt, steht in `Werkzeug`: Name, Titel, Felder, `testen`.

## Der Kern in Schichten

| Modul | Aufgabe | Zustand |
|---|---|---|
| `tresor` | `Tresor` (lesen/schreiben/löschen), `Schluesselbund`, `SpeicherTresor` | Geheimnisse |
| `profil` | `Profile` (die Profile **eines** Werkzeugs), `Profil`, `ProfilFehler` | `profile.toml` |
| `seite` | `einrichten`, `einrichten_mit*`, axum-Routen, Formulare | Laufzeit der Seite |
| `seite::html` | `Sicht` → HTML; `TOKENS`, `STIL`, `SCHRIFTEN` per `include_*!` | keiner (reine Funktion) |
| `browser` | `Browser`-Trait, `EchterBrowser`, `Merker` (Test) | keiner |
| `mcp` | `ProfilWahl`, `zugang`, `profile_tool`, `profile_antwort`, `kuerzen` | keiner |
| `start` | `starten`, `aufruf_aus_args`, `Aufruf` | Prozess |

Die Schnittstelle (`lib.rs`) legt der Architekt fest; die Umsetzung ergänzt sie nur.

## Datenfluss der Einrichtungsseite

```
HTTP-POST (urlencoded)
  → seite::speichern        zerlegt die Formulardaten (eigener Parser, keine Abhängigkeit)
  → Profil { werte, geheim }
  → Profile::pruefen        Pflichtfelder gegen Werkzeug::felder()
  → Profile::speichern      Werte atomar in profile.toml (0600), Geheimnisse in den Tresor
  → 303 zurück auf /e/<token>/ (PRG)
  → seite::html::seite(Sicht)
```

Die Sicht ist ein reines Datenmodell (`Sicht`, `ProfilZeile`, `Formular`, `Meldung`).
`html::seite` rechnet nichts, es maskiert nur und schreibt Bausteine. Was die Seite
zeigt, folgt aus dem Speicher – es gibt keinen zweiten Zustand in der Darstellung.

Regeln, die im Code als Kommentar stehen und hier gelten:

- **Einmal-Token im Pfad** (`/e/<32 Hex>/…`): jede andere Adresse ist 404. Damit kann
  keine fremde Seite im Browser Profile ändern (§4 der SPEC).
- **Nur POST ändert etwas**, jede Antwort ist eine Umleitung (PRG). „Fertig“ beendet
  die Seite: Exit 0, wenn etwas gespeichert wurde, sonst 1.
- **30 Minuten ohne Anfrage** beenden die Seite (Exit 1); jede Anfrage setzt die Uhr.
- **Geheimnisse**: Passwortfelder werden nie mit Wert gerendert, nur mit „gesetzt“.
  Ein leeres Feld beim Bearbeiten lässt den gespeicherten Wert stehen. Vor jeder
  Ausgabe prüft `ohne_geheimnisse`, ob ein Werkzeug doch ein Geheimnis in seinen
  Meldungstext geschrieben hat, und ersetzt es.

## Optik

`assets/tokens.css` und die beiden Plex-Sans-Schnitte kommen per `include_str!` /
`include_bytes!` in das Programm und werden unter `/e/<token>/stil.css` und
`/e/<token>/schrift/<datei>` ausgeliefert. `src/seite/stil.css` benutzt ausschließlich
`var(--wp-…)`; Hex-Werte gibt es nur in `tokens.css`. Theme: `dark`, `light`, `mono`,
Standard nach `prefers-color-scheme`, Wahl im `localStorage` (`quiver-thema`).
Hand-Cursor steht zentral auf `button`, `a`, `select`, `input[type=checkbox]`, `label`.

## Profile und Geheimnisse

```
<QUIVER_TOOLS_DIR oder dirs::config_dir()/quiver-tools>/<werkzeug>/profile.toml   (0600)

standard = "arbeit"

[[profil]]
name = "arbeit"
beschreibung = "Wiki im Büro"
[profil.werte]
url = "https://wiki.example.test"
```

Geheime Felder (`FeldArt::Passwort`) stehen **nicht** in der Datei, sondern im Tresor
unter Dienst `quiver-<werkzeug>` und Schlüssel `<profil>/<feld>`.
`QUIVER_TOOLS_TRESOR=speicher` legt sie in den Arbeitsspeicher (Tests, Ausprobieren).

Profilnamen: 1–40 Zeichen aus `a-z`, `0-9`, `-`, `_`. Geschrieben wird über eine
Zwischendatei und `rename` – es gibt nie eine halbe Datei.

## MCP

Jedes Tool nimmt `profil: Option<String>` (ohne Angabe: Standardprofil) und antwortet
ohne eingerichtetes Profil mit dem Text aus `hinweis_einrichten` – **kein** MCP-Fehler.
`mcp::zugang` bildet genau das ab und macht aus anderen Fehlern einen MCP-Fehler ohne
Geheimnisse. `mcp::profile_antwort` liefert die Liste für `<werkzeug>_profile` (Name,
Beschreibung, Standard, nicht-geheime Werte). Lange Texte über `mcp::kuerzen` an
Wortgrenzen kürzen.

## Das Programm-Gerüst

`starten(werkzeug, |profile| server)` wertet `std::env::args()` aus:

| Aufruf | Wirkung |
|---|---|
| ohne Argument | MCP über stdio (so startet quiver das Tool) |
| `einrichten` | Einrichtungsseite im Browser, endet mit Exit 0/1 |
| `--version` | Version auf stdout |
| `--help` | kurze Hilfe auf stdout |
| sonst | Meldung auf stderr, Exit 2 |

Begründung in [adr/0001-programm-geruest.md](adr/0001-programm-geruest.md). Logs und
Meldungen gehen immer nach **stderr**; stdout gehört dem MCP-Protokoll.

## Ein Tool: `crates/outline`

Die Vorlage `pi-outline-wiki` (TypeScript) liefert die Semantik, nicht den Code
(SPEC §6). Die Schichten sind dieselben wie im Kern, nur eine Ebene tiefer:

| Modul | Aufgabe | Zustand |
|---|---|---|
| `werkzeug` | `Outline` (Name, Titel, Felder, `testen`), `wurzel` (Adresse normalisieren) | keiner |
| `client` | der **einzige** Ort mit Netzverkehr: `POST <wurzel>/api/<methode>`, Bearer, 30 s | keiner |
| `domain` | was Outline liefert (`serde`), ProseMirror → Prosa | keiner |
| `format` | reine Funktionen: Daten → Text, gekürzt über `mcp::kuerzen` | keiner |
| `datei` | Titel → Dateiname, Schreiben ohne Ausbruch aus dem Zielordner | Dateien |
| `server` | die 14 MCP-Werkzeuge, Parameter als `schemars`-Typen | Profilspeicher |

Datenfluss eines Aufrufs:

```
MCP tools/call
  → server::aufrufen        Name → Werkzeug, Argumente → Parametertyp
  → mcp::zugang            Profil holen (ohne Profil: Hinweistext, kein Fehler)
  → client::aufrufen       POST /api/<methode>, Fehler → OutlineFehler ohne Schlüssel
  → domain                 Antwort in Datenform
  → format                 Daten → knapper Text (max_zeichen, Seitenhinweis)
  → CallToolResult
```

Regeln, die dort gelten:

- **Kein Geheimnis im Text.** `client::aufrufen` ersetzt den Schlüssel in jedem
  Fehlertext durch `[geheim]`, auch wenn ein Server ihn zurückspiegelt.
- **`outline_profile` kommt aus dem Kern** (`mcp::profile_tool` /
  `mcp::profile_antwort`): bei jedem Tool dasselbe lesende Werkzeug.
- **`alle_profile`** bildet „alle Workspaces“ der Vorlage ab: eine Suche über alle
  Profile, jeder Treffer mit Profilnamen; ein kaputtes Profil verschluckt die anderen
  nicht, sein Fehler steht daneben.
- **Schreibende Werkzeuge** tragen `destructive_hint`, lesende `read_only_hint`
  (AGENTS.md §8).
- **Parameternamen sind deutsch** (`collection_id`, `max_zeichen`, `save_dir`), wie
  `profil` im Kern; die **Werkzeugnamen** bleiben die der Vorlage, damit Skills und
  Gewohnheiten weiter passen.

## Einrichtungsseite: Aktionen

Die Profilliste hat eine **feste Aktionsspalte**: jede Zeile trägt dieselben vier
Aktionen in derselben Reihenfolge (`Bearbeiten`, `Testen`, `Als Standard`, `Löschen`).
Beim Standardprofil steht „Als Standard“ gesperrt da, statt zu fehlen – sonst schiebt
sich die Spalte von Zeile zu Zeile. Alle Nebenaktionen sind normale Knöpfe mit Rahmen
(`--wp-size-control-md`), „Löschen“ trägt den Fehlerton, „Speichern“/„Fertig“ sind
Hauptknöpfe. Was eine Zeile an Aktionen hat, entscheidet `zeilenknoepfe` im Modell,
nicht ein `if` in der Darstellung.
