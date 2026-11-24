# 02 – quiver-outline (Outline-Wiki)

Crate: `crates/outline`, Programm `quiver-outline`. Baut auf `crates/kern` auf (`starten`, `Werkzeug`,
Profile, `zugang`, `profile_tool`, `kuerzen`). Vorlage: `~/Documents/Workspaces/pi-outline-wiki`
(`src/tools/`, `src/clients/`, `src/formatting/`, `README.md`) – **Semantik übernehmen, nicht den TS-Code
übersetzen** (SPEC §6).

## Abhängigkeiten

`quiver-tools-kern`, `rmcp` (wie im Kern), `tokio`, `reqwest` (json, rustls), `serde`, `serde_json`,
`anyhow`, `thiserror`, `tracing`, `schemars` (die Version von rmcp), `url = "2"`.
Dev: `axum` (Outline-Attrappe), `tempfile`.

## Werkzeug (Einrichtung)

- Name `outline`, Titel „Outline-Wiki“.
- Felder: `url` (Url, Pflicht, Hilfe: „Adresse deines Outline, z. B. https://firma.getoutline.com“),
  `api_key` (Passwort, Pflicht, Hilfe: „Einstellungen → API & Apps, beginnt mit ol_api_“).
- URL tolerant normalisieren wie die Vorlage (nackter Host, Schrägstrich am Ende, `/api`, `/mcp` → Wurzel).
- `testen`: `POST /api/auth.info` → „Angemeldet als <Name> in <Workspace>“.

## Tools

Alle aus der Vorlage **außer** `outline_setup` und `outline_status`; `outline_profile` wird das lesende
Profil-Tool aus dem Kern (Liste ohne Geheimnisse). Jedes Tool nimmt `profil` (optional).

| Tool | Hinweis |
|---|---|
| `outline_collections`, `outline_structure`, `outline_search`, `outline_list`, `outline_read`, `outline_comments`, `outline_profile` | `read_only_hint` |
| `outline_create`, `outline_update`, `outline_move`, `outline_archive`, `outline_delete`, `outline_comment` | `destructive_hint` |
| `outline_export` | schreibt lokale Dateien – `destructive_hint`; Zielordner muss angegeben werden |

- `outline_search` „alle Workspaces“ (Vorlage) = über alle Profile.
- `outline_read`: Markdown, mit `max_zeichen` (Standard 8000) und Kürzungshinweis aus dem Kern.
- Fehler von Outline → MCP-Fehler mit Status und Text, **nie** der API-Key.

## Einrichtungsseite nachbessern (crates/kern)

Beim Abnehmen von Aufgabe 01 aufgefallen:
1. **Einheitliche Knöpfe:** „Als Standard“, „Löschen“, „Abbrechen“, „Neu laden“ sind nackter Text ohne Rahmen,
   „Bearbeiten“/„Testen“ haben Rahmen. Alle Nebenaktionen als normaler Knopf (Rahmen, `--wp-size-control-md`),
   „Löschen“ als Knopf mit Fehler-Ton, Hauptaktionen (Speichern, Fertig) als Hauptknopf.
2. **Feste Aktionsspalte:** In der Profilliste stehen die Knöpfe je Zeile verschieden weit links, weil
   „Als Standard“ beim Standardprofil fehlt. Feste Spalte rechts, gleiche Reihenfolge, fehlende Aktion
   lässt ihren Platz frei (oder ist deaktiviert).
3. Bilder `docs/bilder/einrichtung-{dark,light}.png` neu erzeugen.

## Abnahme

- Outline-Attrappe (axum) mit den benutzten Endpunkten; je Tool ein Erfolgsfall (Pfad, Rumpf, Antwortform),
  Fehlerfall ohne Key im Text, ohne Profil → Hinweistext aus dem Kern, Profilwahl per `profil`.
- Ende-zu-Ende: `quiver-outline` als Kindprozess über stdio (`QUIVER_TOOLS_TRESOR=speicher`,
  `QUIVER_TOOLS_DIR` auf temp, Profil vorher über die Kern-API angelegt) → `list_tools` vollständig, ein Aufruf klappt.
- README: Abschnitt quiver-outline mit Tool-Tabelle und dem Eintrag für mcp-quiver (mit `"einrichtung": {"einrichten": ["einrichten"]}`).
- Prüfbefehl grün.
