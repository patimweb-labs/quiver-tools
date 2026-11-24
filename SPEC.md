# SPEC – quiver-tools

> Stand: Entwurf 1, 2026-11-23 (Aufgabe 03). Jede Festlegung mit Begründung und Preis.

## 1. Zweck

Eigene MCP-Tools für die [mcp-quiver](https://github.com/patimweb-labs/mcp-quiver)-Bibliothek, in Rust,
als Nachfolger der pi-Erweiterungen `pi-outline-wiki`, `pi-email`, `pi-ssh`, `pi-abap-fs` und
`pi-azure-devops`.
Jedes Tool ist ein eigenes Programm (MCP über stdio); alle teilen sich einen Kern.

| Tool | Programm | Vorlage | Reihenfolge |
|---|---|---|---|
| Outline-Wiki | `quiver-outline` | `~/Documents/Workspaces/pi-outline-wiki` | 1 |
| E-Mail (IMAP/SMTP) | `quiver-email` | `~/Documents/Workspaces/pi-email` | 2 |
| SSH/SFTP | `quiver-ssh` | `~/Documents/Workspaces/pi-ssh` | 3 |
| ABAP (SAP ADT) | `quiver-abap` | `~/Documents/Workspaces/pi-abap-fs` | 4 |
| Azure DevOps | `quiver-devops` | `~/Documents/Workspaces/pi-azure-devops` | 5 |

## 2. Zugangsdaten gehen nie durch den Agenten

**Festlegung (Nutzer, 2026-11-23):** Keine Einrichtung im Browser. Die Felder eines Zugangs trägt der
User **im quiver-Fenster** ein – unter Tools → <Titel>. quiver liest die Feldbeschreibung aus dem
Bibliothekseintrag (`Werkzeug::felder()`), der Client bleibt allgemein.
*Begründung:* Passwörter und API-Keys erreichen nie ein Modell, und das Tool braucht dafür weder Seite
noch Browser. *Preis:* vorerst genau ein Zugang je Tool (siehe unten) – außer bei SSH, ABAP und
DevOps, die eine Liste pflegen (§2a).

- Die Felder eines Zugangs stehen im Bibliothekseintrag. Offene Felder (Quelle `client`, z. B. eine
  Adresse) liegen in der Client-Konfiguration, geheime Felder (`geheim`, Feldart `Passwort`) im
  Tresor-Datei des Geräts und werden nie gepusht.
- quiver setzt beim Start die Umgebung des Werkzeugs: `<NAME>_<FELD>` in Großbuchstaben, z. B.
  `OUTLINE_URL` und `OUTLINE_API_KEY`. Der Kern liest nur diese Umgebung; er kennt weder quiver noch
  die Tresor-Datei.
- **Genau ein Zugang `standard`.** Sind alle Pflichtfelder gesetzt, gibt es dieses eine Profil
  (Geheimes kommt aus der Umgebung, nicht aus einem Tresor). Leere Werte gelten als fehlend.
- Fehlt ein Pflichtfeld, antwortet **jedes** Tool mit dem Text
  „Zugang fehlt – bitte im quiver-Fenster unter Tools → <Titel> die Angaben eintragen.“ (Text, **kein**
  MCP-Fehler). Es gibt keinen `profil`-Parameter mehr.
- Ein Lese-Tool `<tool>_profile` zeigt den Zugang **ohne** Geheimnisse (Name, nicht-geheime Felder wie
  Adresse oder Host) oder denselben Hinweis. Damit weiß der Agent, was es gibt.
- *Preis:* mehrere Wikis, Postfächer oder Hosts je Tool sind nicht möglich; sie kämen als mehrere
  Bibliothekseinträge (eigene Crate-Instanz) oder später als zusätzliche Umgebungsnamen.

### 2a. Ausnahme: mehrere gleichartige Zugänge

`quiver-ssh`, `quiver-abap` und `quiver-devops` pflegen statt eines Zugangs eine Liste
(`Werkzeug::mehrfach` = Titel und Einzahl, z. B. „Organisationen“/„Organisation“). Der Eintrag trägt
die Felder dann unter `mehrfach` und hat selbst keine `felder` und keine `umgebung`; quiver übergibt
alle Zugänge als JSON in `QUIVER_ZUGAENGE`. Jedes Werkzeug nimmt den Namen (`verbindung` bzw.
`system`); bei genau einem Zugang darf er fehlen.

*Begründung:* ein Nutzer arbeitet mit mehreren Hosts, SAP-Systemen oder DevOps-Organisationen (eigene
und Kundenorganisationen, oft in fremden Tenants oder auf einem Server). Ein Bibliothekseintrag je
Host wäre unübersichtlich. *Preis:* die Geheimnisse liegen als JSON in **einer** Umgebungsvariablen –
der Kern liest sie also alle, nicht nur die des gewählten Zugangs; kaputtes JSON ist ein Fehler ohne
Inhalt. Ein Feld `schreiben` (Schalter, Standard **aus**) erlaubt verändernde Werkzeuge je Zugang; ist
es aus, antworten sie mit einem Hinweis **ohne** Anfrage.

## 3. Wo die Werte liegen

- Offene Werte: Client-Konfiguration von quiver; sie erscheinen in `<tool>_profile`.
- Geheime Werte: Tresor-Datei des Geräts, gesetzt von quiver. Das Tool sieht sie nur als
  Umgebungsvariable im eigenen Prozess.
- Nie in Log, Fehlertext, Tool-Antwort, Kommandozeile oder einer Datei des Tools. `Debug`-Ausgaben des
  Kerns zeigen Geheimnisse nur als Feldnamen.
- `Werkzeug::testen` bleibt bestehen (für einen späteren „Testen“-Knopf in quiver), wird aber von
  keinem MCP-Weg aufgerufen. `quiver-<tool> testen` prüft den Zugang von Hand: es liest die Umgebung,
  gibt die Meldung aus und endet mit Exit 0 (erreichbar) oder 1 (fehlt/fehlgeschlagen).

## 4. Das Programm und der Bibliothekseintrag

`quiver-<tool>` ist ein Programm mit diesen Aufrufen:

| Aufruf | Wirkung |
|---|---|
| ohne Argument | MCP über stdio – so startet quiver das Tool |
| `eintrag [--befehl <pfad>]` | der Bibliothekseintrag für mcp-quiver als JSON auf stdout |
| `testen` | Zugang aus der Umgebung prüfen, Meldung, Exit 0/1 |
| `--version` | Version auf stdout |
| `--help` | kurze Hilfe auf stdout |
| sonst | Meldung auf stderr, Exit 2 |

Der Eintrag hat das Format von `quiver_domain::Eintrag`:

- `id` = Name des Werkzeugs (`outline`), `titel`, `beschreibung` (`Werkzeug::beschreibung()`),
  `version` = Crate-Version, `herkunft` = `eigen`.
- `laufzeit` = `{ art: "stdio", befehl: <--befehl oder laufendes Programm>, umgebung: { "<NAME>_<FELD>":
  "${<feld>}" … } }`.
- `felder`: je Feld `{ name, titel, beschreibung: nur die Hilfe, art, quelle: "geheim" bei
  Passwort sonst "client", pflicht, standard }`. `titel` ist die Anzeige, `art` die
  Darstellung: `text`, `passwort`, `zahl`, `schalter`, `url` oder
  `auswahl { optionen: [ { wert, anzeige } ] }` (snake_case, wie `quiver_domain::FeldArt`).
- **Kein `einrichtung`-Feld**: es gibt keine Seite, die quiver öffnen könnte.

## 5. Aufbau

```
crates/kern       Werkzeug-Trait, Zugang aus der Umgebung, Bibliothekseintrag, MCP-Hilfen, Programm-Gerüst
crates/outline    quiver-outline   (Aufgabe 02)
crates/email      quiver-email     (später)
crates/ssh        quiver-ssh       (Aufgabe 05)
crates/abap       quiver-abap      (Aufgaben 07 und 08)
crates/devops     quiver-devops    (Aufgabe 09)
assets/           tokens.css und Schriften aus mcp-quiver (für die Oberfläche von quiver, nicht dieses Repos)
```

Richtung: `kern ← <tool>`. Tools kennen einander nicht.

## 6. Tools übernehmen

Semantik, Parameter und Ausgaben folgen der Vorlage (TypeScript-Quelltext lesen), mit diesen Regeln:
- Namen bleiben (`outline_search` usw.), damit bestehende Gewohnheiten und Skills passen.
- `*_setup` entfällt, `*_status`/`*_profile` werden zu einem lesenden `<tool>_profile` (§2).
- Verändernde Tools tragen `destructive_hint = true`, lesende `read_only_hint = true`.
- Antworten knapp (Agenten zahlen Tokens), lange Inhalte mit `max_zeichen` und Kürzungshinweis.

## 7. Tests

Gegen Attrappen (axum) – nie gegen echte Server. Die Umgebung wird als Paar-Menge übergeben
(`Zugang::mit_umgebung`, kein Prozesszustand), der Kindprozess-Test setzt sie als echte Variablen und
prüft beide Fälle: gesetzte Umgebung und fehlende (Hinweistext). `eintrag` wird gegen eine Kopie von
`quiver_domain::Eintrag` deserialisiert.
