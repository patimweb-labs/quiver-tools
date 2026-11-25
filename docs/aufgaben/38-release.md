# 38 – Release der Tools für den Server

Ein Tag `vX.Y.Z` baut alle `quiver-*`-Programme nativ für Windows x64, macOS arm64 und x64, Linux x64 und arm64 (Läufer wie in
mcp-quiver/release.yml, Linux auf ubuntu-22.04) und legt sie als GitHub-Release ab:
- je System ein Archiv `quiver-tools-<version>-<system>-<arch>.tar.gz` mit den gestrippten Programmen und `LISTE.json`
  (Name, Version, Größe, SHA-256 je Programm), dazu `SHA256SUMS` über alle Archive.
- macOS ad hoc signiert, unter macOS geprüft, dass es startet. Windows ohne Konsolen-Abhängigkeiten.
- Version aus dem Workspace, Tag muss passen, `--locked`, Rechte minimal (nur der Release-Job schreibt), keine Geheimnisse nötig;
  `workflow_dispatch` und Pull Requests bauen nur.
- Eine Prüfung startet jedes Programm auf seinem System kurz (`eintrag` und MCP-`initialize`/`tools/list`).
- Doku kurz (de/en), actionlint sauber, keine Firmen- oder Personennamen. **Nicht committen, nicht pushen, kein Tag.**
