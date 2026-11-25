# Changelog

Alle nennenswerten Änderungen an quiver-tools. Das Format folgt
[Keep a Changelog](https://keepachangelog.com/de/1.1.0/), die Versionen
[Semantic Versioning](https://semver.org/lang/de/).

Pflegt `release-please` aus den Conventional Commits; von Hand kommt nur der
Eintrag zur ersten Version hierher. Der Tag ist `v<Version>` und muss zur
Version in `[workspace.package]` passen.

## [0.1.0](https://github.com/patimweb-labs/quiver-tools/releases/tag/v0.1.0) (2026-11-25)

Die erste Fassung: MCP-Werkzeuge als einzelne Programme, je System ein Archiv
(macOS `aarch64` und `x86_64`, Linux `x86_64` und `aarch64`, Windows `x86_64`)
mit `LISTE.json` und `SHA256SUMS`.

### Neu

* Kern: Werkzeug-Schnittstelle, Zugang aus der Umgebung, Bibliothekseintrag, MCP-Hilfen
* Outline, E-Mail, SSH, ABAP, ZEP, Microsoft 365, Jira, Confluence, Zephyr, Azure DevOps
* Diagramme lokal: Excalidraw, draw.io und ein eigenes Format
* `verteilung/`: bauen, strippen, ad hoc signieren (macOS), paketieren und jedes Programm vor der Veröffentlichung starten
