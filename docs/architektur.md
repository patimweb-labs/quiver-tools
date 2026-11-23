# Architektur

> Stand: 2026-11-23, nach Aufgabe 03. Gilt für den Kern (`crates/kern`) und alle
> Tools, die darauf aufsetzen. Festlegungen und Preise: [../SPEC.md](../SPEC.md).

## Richtung

```
crates/kern        Werkzeug-Trait, Zugang aus der Umgebung, Bibliothekseintrag, MCP-Hilfen, Gerüst
crates/<tool>      ein Programm quiver-<tool> je Tool
assets/            tokens.css und Schriften (Kopie aus mcp-quiver, nicht von Hand ändern)
```

`kern ← <tool>`: ein Tool kennt den Kern, der Kern kennt kein Tool. Alles, was ein
Tool dem Kern über sich sagt, steht in `Werkzeug`: Name, Titel, Beschreibung, Felder,
`testen`.

Die Oberfläche ist **quiver**. Dieses Repo liefert keine Seite und keinen Browser-Code
mehr; `assets/` bleibt als Vorlage für quiver und wird hier von keinem Programm
eingebunden.

## Der Kern in Schichten

| Modul | Aufgabe | Zustand |
|---|---|---|
| `umgebung` | `Zugang`: Felder aus `<NAME>_<FELD>` lesen, Pflichtfelder prüfen, `Profil` bauen | Umgebung des Prozesses |
| `profil` | `Profil { name, werte, geheim }`, `ProfilFehler`; `Debug` zeigt Geheimnisse nie | keiner |
| `eintrag` | `quiver_domain::Eintrag` als JSON: id, version, titel, beschreibung, laufzeit, felder | keiner |
| `mcp` | `zugang`, `profile_tool`, `profile_antwort`, `server_info`, `kuerzen` | keiner |
| `start` | `starten`, `aufruf_aus_args`, `Aufruf` (MCP, eintrag, testen, version, hilfe) | Prozess |

Die Schnittstelle (`lib.rs`) legt der Architekt fest; die Umsetzung ergänzt sie nur.

## Datenfluss des Zugangs

```
quiver-Fenster (Tools → <Titel>)
  → Bibliothekseintrag (quiver-outline eintrag)      Felder, quelle client/geheim
  → User trägt Werte ein                             offen: Client-Konfiguration, geheim: Schlüsselbund
  → quiver startet quiver-<tool> mit Umgebung        OUTLINE_URL, OUTLINE_API_KEY, …
  → umgebung::Zugang::neu(&Werkzeug)                 liest <NAME>_<FELD>, leere Werte fehlen
  → Zugang::profil()                                 alle Pflichtfelder da? → Profil `standard`
  → mcp::zugang                                      fehlt etwas → Hinweistext, kein MCP-Fehler
  → client::aus_profil                               Adresse + Schlüssel aus dem Profil
```

Was der Agent sieht, folgt aus der Umgebung: es gibt keinen zweiten Zustand, keine
Datei, keinen Tresor im Tool.

Regeln, die im Code als Kommentar stehen und hier gelten:

- **Nur die Umgebung zählt.** Ein Feldname `api_key` wird zur Variablen `OUTLINE_API_KEY`
  (`umgebung::schluessel`). Kein Konfigurationsordner, kein Schlüsselbund im Tool.
- **Leer ist nicht gesetzt.** `Zugang` verwirft Werte, die nur aus Leerraum bestehen.
- **Geheimes bleibt geheim.** `Profil::geheim` wird nie in eine Antwort geschrieben;
  `<tool>_profile` zeigt nur `Profil::werte`. Fehlertexte nennen höchstens Feldnamen.
- **Ein Zugang.** Es gibt kein `profil`-Feld in der Umgebung und keinen `profil`-Parameter
  an den Werkzeugen. Mehrere Zugänge wären mehrere Bibliothekseinträge.

## Der Bibliothekseintrag

`eintrag::eintrag(&Werkzeug, befehl)` erzeugt die Struktur, `quiver-<tool> eintrag`
schreibt sie als JSON auf stdout. `befehl` kommt aus `--befehl <pfad>`, sonst aus
`std::env::current_exe()`, sonst `quiver-<name>`.

```
laufzeit = { art: "stdio", befehl: <pfad>, umgebung: { "<NAME>_<FELD>": "${<feld>}" } }
felder   = [ { name, beschreibung: titel – hilfe, quelle: client|geheim, pflicht, standard } ]
```

Kein `einrichtung`-Feld: es gibt keine Seite, die quiver öffnen könnte. `aktiv` und
`argumente` fehlen absichtlich; beides hat in `quiver_domain` einen Standardwert.

## MCP

Jedes Werkzeug antwortet ohne Zugang mit dem Text aus `hinweis_zugang` – **kein**
MCP-Fehler. `mcp::zugang` bildet genau das ab. `mcp::profile_antwort` liefert den
Bericht für `<werkzeug>_profile` (Name `standard`, nicht-geheime Werte) oder denselben
Hinweis. Lange Texte über `mcp::kuerzen` an Wortgrenzen kürzen.

## Das Programm-Gerüst

`starten(werkzeug, |zugang| server)` wertet `std::env::args()` aus:

| Aufruf | Wirkung |
|---|---|
| ohne Argument | MCP über stdio (so startet quiver das Tool) |
| `eintrag [--befehl <pfad>]` | Bibliothekseintrag als JSON auf stdout |
| `testen` | Zugang aus der Umgebung prüfen, Meldung, Exit 0/1 |
| `--version` | Version auf stdout |
| `--help` | kurze Hilfe auf stdout |
| sonst | Meldung auf stderr, Exit 2 |

Begründung in [adr/0001-programm-geruest.md](adr/0001-programm-geruest.md) und
[adr/0002-zugang-ueber-quiver-felder.md](adr/0002-zugang-ueber-quiver-felder.md).
Logs und Meldungen gehen immer nach **stderr**; stdout gehört dem MCP-Protokoll.

## Ein Tool: `crates/outline`

Die Vorlage `pi-outline-wiki` (TypeScript) liefert die Semantik, nicht den Code
(SPEC §6). Die Schichten sind dieselben wie im Kern, nur eine Ebene tiefer:

| Modul | Aufgabe | Zustand |
|---|---|---|
| `werkzeug` | `Outline` (Name, Titel, Beschreibung, Felder, `testen`), `wurzel` (Adresse normalisieren) | keiner |
| `client` | der **einzige** Ort mit Netzverkehr: `POST <wurzel>/api/<methode>`, Bearer, 30 s | keiner |
| `domain` | was Outline liefert (`serde`), ProseMirror → Prosa | keiner |
| `format` | reine Funktionen: Daten → Text, gekürzt über `mcp::kuerzen` | keiner |
| `datei` | Titel → Dateiname, Schreiben ohne Ausbruch aus dem Zielordner | Dateien |
| `server` | die 14 MCP-Werkzeuge, Parameter als `schemars`-Typen | Zugang aus der Umgebung |

Datenfluss eines Aufrufs:

```
MCP tools/call
  → server::aufrufen        Name → Werkzeug, Argumente → Parametertyp
  → mcp::zugang             Zugang aus der Umgebung (fehlt er: Hinweistext, kein Fehler)
  → client::aufrufen        POST /api/<methode>, Fehler → OutlineFehler ohne Schlüssel
  → domain                  Antwort in Datenform
  → format                  Daten → knapper Text (max_zeichen, Seitenhinweis)
  → CallToolResult
```

Regeln, die dort gelten:

- **Kein Geheimnis im Text.** `client::aufrufen` ersetzt den Schlüssel in jedem
  Fehlertext durch `[geheim]`, auch wenn ein Server ihn zurückspiegelt.
- **`outline_profile` kommt aus dem Kern** (`mcp::profile_tool` /
  `mcp::profile_antwort`): bei jedem Tool dasselbe lesende Werkzeug.
- **Eine Suche, ein Wiki.** „Über alle Workspaces“ der Vorlage gibt es nicht mehr –
  dafür bräuchte es mehrere Zugänge.
- **Schreibende Werkzeuge** tragen `destructive_hint`, lesende `read_only_hint`
  (AGENTS.md §8).
- **Parameternamen sind deutsch** (`collection_id`, `max_zeichen`, `save_dir`); die
  **Werkzeugnamen** bleiben die der Vorlage, damit Skills und Gewohnheiten weiter passen.

## Tests

- Attrappen (axum) auf `127.0.0.1`, kein echter Dienst.
- Der Zugang kommt als Paar-Menge über `Zugang::mit_umgebung` – kein Prozesszustand,
  keine parallelen Umgebungsvariablen.
- `tests/ende_zu_ende.rs` startet `quiver-outline` als Kindprozess: einmal mit
  gesetzter Umgebung gegen die Attrappe, einmal ohne (Hinweistext), dazu `eintrag`
  und `testen`.
- Der Eintrag wird gegen eine Kopie von `quiver_domain::Eintrag` deserialisiert
  (`eintrag::tests`), damit Formatfehler auffallen, ohne das andere Projekt zu
  importieren.
