# SPEC – quiver-tools

> Stand: Entwurf 0, 2026-11-23. Jede Festlegung mit Begründung und Preis.

## 1. Zweck

Eigene MCP-Tools für die [mcp-quiver](https://github.com/patimweb-labs/mcp-quiver)-Bibliothek, in Rust,
als Nachfolger der pi-Erweiterungen `pi-outline-wiki`, `pi-email`, `pi-ssh` und `pi-abap-fs`.
Jedes Tool ist ein eigenes Programm (MCP über stdio); alle teilen sich einen Kern.

| Tool | Programm | Vorlage | Reihenfolge |
|---|---|---|---|
| Outline-Wiki | `quiver-outline` | `~/Documents/Workspaces/pi-outline-wiki` | 1 |
| E-Mail (IMAP/SMTP) | `quiver-email` | `~/Documents/Workspaces/pi-email` | 2 |
| SSH/SFTP | `quiver-ssh` | `~/Documents/Workspaces/pi-ssh` | 3 |
| ABAP (SAP ADT) | `quiver-abap` | `~/Documents/Workspaces/pi-abap-fs` | 4 |

## 2. Zugangsdaten gehen nie durch den Agenten

**Festlegung (Nutzer, 2026-11-23):** Die `*_setup`-Tools der Vorlagen fallen weg. Zugänge (Profile) richtet
der User auf einer **Einrichtungsseite** ein, die das Tool selbst im Browser öffnet; quiver startet sie über
den Knopf „Einrichten“ (`Eintrag.einrichtung = { einrichten: ["einrichten"] }`).
*Begründung:* Passwörter und API-Keys erreichen nie ein Modell. *Preis:* Jedes Tool braucht eine Seite – darum
erzeugt der Kern sie aus einer Feldbeschreibung.

- Ein Tool hat **beliebig viele Profile** (mehrere Wikis, Postfächer, Hosts, SAP-Systeme) und ein **Standardprofil**.
- Jedes MCP-Tool nimmt optional `profil` (Name); ohne → Standardprofil.
- Ein Lese-Tool `<tool>_profile` listet Profile **ohne** Geheimnisse (Name, Beschreibung, Standard ja/nein,
  nicht-geheime Felder wie Host oder URL). Damit weiß der Agent, was es gibt.
- Fehlt ein Profil, antwortet jedes Tool: „Noch kein Zugang eingerichtet – bitte im quiver-Fenster unter
  Tools → <Titel> auf „Einrichten“ klicken.“ (Text, kein MCP-Fehler.)

## 3. Profile und Geheimnisse

- Nicht-geheime Werte: `<konfig>/quiver-tools/<tool>/profile.toml` (`dirs::config_dir()`;
  `QUIVER_TOOLS_DIR` verlegt den Ordner, für Tests).
- Geheime Werte (Feldart `Passwort`): Schlüsselbund, Dienst `quiver-<tool>`, Schlüssel `<profil>/<feld>`;
  `QUIVER_TOOLS_TRESOR=speicher` → nur im Speicher (Tests).
- Nie in Logs, Fehlertexten, Tool-Antworten, nie zurück an den Browser (dort steht nur „gesetzt“).

## 4. Die Einrichtungsseite (`quiver-<tool> einrichten`)

- axum auf `127.0.0.1:0`; die URL enthält ein zufälliges Einmal-Token als Pfad (`/e/<token>/…`), alle anderen
  Pfade → 404. So kann keine andere Seite im Browser Profile ändern (kein CSRF über fremde Tabs).
- Öffnet den Browser selbst (`open`), endet bei „Fertig“ (Exit 0, wenn etwas gespeichert wurde, sonst 1) oder
  nach 30 min ohne Anfrage (Exit 1).
- Inhalt: Liste der Profile (Standard markiert), Profil anlegen/bearbeiten/löschen, als Standard setzen,
  **„Verbindung testen“** (ruft `Werkzeug::testen`, zeigt Erfolg oder Fehlertext), „Fertig“.
- Formulare werden aus dem Feldschema erzeugt (Text, Passwort, Zahl, Schalter, URL, Auswahl), serverseitig als
  HTML, ohne Build-Schritt und ohne JS-Framework (wenig Inline-JS ist erlaubt).
- **Optik wie quiver**: `assets/tokens.css` (Kopie aus mcp-quiver, *nicht von Hand ändern*), IBM Plex,
  Themes dark/light/mono (Standard nach `prefers-color-scheme`, Umschalter oben), Abstände und Maße nur über
  `var(--wp-…)`, **Hand-Cursor auf allem Klickbaren**.

## 5. Aufbau

```
crates/kern       Werkzeug-Trait, Feldschema, Profilspeicher, Tresor, Einrichtungsseite, MCP-Hilfen
crates/outline    quiver-outline   (Aufgabe 02)
crates/email      quiver-email     (später)
crates/ssh        quiver-ssh       (später)
crates/abap       quiver-abap      (später)
assets/           tokens.css, Schriften (aus mcp-quiver)
```

Richtung: `kern ← <tool>`. Tools kennen einander nicht.

## 6. Tools übernehmen

Semantik, Parameter und Ausgaben folgen der Vorlage (TypeScript-Quelltext lesen), mit diesen Regeln:
- Namen bleiben (`outline_search` usw.), damit bestehende Gewohnheiten und Skills passen.
- `*_setup` entfällt, `*_status`/`*_profile` werden zu einem lesenden `<tool>_profile` (§2).
- Verändernde Tools tragen `destructive_hint = true`, lesende `read_only_hint = true`.
- Antworten knapp (Agenten zahlen Tokens), lange Inhalte mit `max_zeichen` und Kürzungshinweis.

## 7. Tests

Gegen Attrappen (axum) – nie gegen echte Server. Die Einrichtungsseite wird mit HTTP-Anfragen getestet
(Token-Pfad, Formulare, Geheimnisse kommen nie zurück).
