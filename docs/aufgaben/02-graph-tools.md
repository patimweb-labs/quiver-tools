# 02 – Graph-Client und MCP-Tools

Crates: `crates/graph` (`m365-graph`) und `crates/mcp` (Programm `quiver-m365`).
Voraussetzung: Aufgabe 01 (Anmeldung). Tools und Regeln: `SPEC.md` §4.

## Abhängigkeiten (nur diese)

graph: `m365-anmeldung`, `reqwest`, `serde`, `serde_json`, `thiserror`, `anyhow`, `tracing`, `url = "2"`.
mcp: `m365-anmeldung`, `m365-graph`, `rmcp = { version = "3.11", features = ["server", "transport-io", "macros", "schemars"] }`
(Feature-Namen in rmcp 3.11 prüfen, Quelltext unter `~/.cargo/registry/src`), `tokio`, `serde`,
`serde_json`, `schemars` (die Version, die rmcp nutzt), `tracing`, `tracing-subscriber` (nach **stderr** –
stdout gehört MCP), `anyhow`, `time = { version = "0.3", features = ["formatting","parsing"] }`.
Dev: `axum` (Graph-Attrappe), `m365-anmeldung` mit `SpeicherTresor`.

## Graph-Client

- `GraphClient::neu(basis_url, anmeldung)`; `get<T>(pfad, query)`, `post<T>(pfad, body)`.
- Token holen über `Anmeldung::access_token`; `WartetAufBrowser` → eigener Fehler
  `GraphFehler::AnmeldungLaeuft` (das Tool macht daraus den Hinweistext aus SPEC §2).
- Graph antwortet 401 → `anmeldung.verwerfen()`, **einmal** wiederholen.
- Graph-Fehler (`{"error":{"code","message"}}`) → `GraphFehler::Graph { status, code, text }`.
- 429/503 mit `Retry-After` → einmal nach der genannten Zeit (höchstens 10 s) wiederholen.
- Modelle nur mit den Feldern, die die Tools zurückgeben (`$select` passend setzen).

## Tools (`crates/mcp`)

Genau die Tabelle aus SPEC §4. Parameter mit `schemars`-Beschreibungen auf Deutsch, Namen deutsch
(`an`, `cc`, `betreff`, `text`, `start`, `ende`, `ort`, `teilnehmer`, `suche`, `anzahl`, `von`, `bis`).
- Zeiten als ISO 8601 (`2026-11-27T10:00`), Zeitzone aus `M365_ZEITZONE` (Standard `Europe/Berlin`).
- `mails_suchen` ohne `suche` → neueste Mails im Posteingang. `anzahl` Standard 10, höchstens 50.
- `mail_senden` und `termin_anlegen` mit Annotation `destructive_hint = true`, die übrigen `read_only_hint = true`.
- Antworten: kompaktes JSON als Text.
- `AnmeldungLaeuft` → **kein** Fehler, sondern Text: „Anmeldung im Browser geöffnet – bitte anmelden und
  den Aufruf danach wiederholen.“
- `main`: Konfig aus Umgebung, Tresor `Schluesselbund` (oder `SpeicherTresor` bei `M365_TRESOR=speicher`),
  Browser `OeffneBrowser`, rmcp über stdio. `quiver-m365 --version` gibt die Version aus.

## Abnahme

- Integrationstests mit Graph-Attrappe (axum) + Anmelde-Attrappe aus Aufgabe 01:
  je Tool ein Erfolgsfall (richtiger Pfad, Query/Body, Antwortform), 401 → Erneuern + Wiederholung,
  Graph-Fehler → MCP-Fehler ohne Token, `AnmeldungLaeuft` → Hinweistext.
- Ein Ende-zu-Ende-Test startet `quiver-m365` als Kindprozess (rmcp-Client über stdio,
  `env!("CARGO_BIN_EXE_quiver-m365")`, Umgebung auf die Attrappen, `M365_TRESOR=speicher`,
  Test-„Browser“ über `M365_ACCESS_TOKEN` umgehen) → `list_tools` hat alle 8 Tools, `profil` liefert den Namen.
- README: Installation, Konfiguration, Eintrag für mcp-quiver, Verweis auf `docs/entra-app.md`.
- Prüfbefehl grün.
