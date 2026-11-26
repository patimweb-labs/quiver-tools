# Changelog

Alle nennenswerten Änderungen an quiver-tools. Das Format folgt
[Keep a Changelog](https://keepachangelog.com/de/1.1.0/), die Versionen
[Semantic Versioning](https://semver.org/lang/de/).

Pflegt `release-please` aus den Conventional Commits; von Hand kommt nur der
Eintrag zur ersten Version hierher. Der Tag ist `v<Version>` und muss zur
Version in `[workspace.package]` passen.

## [0.2.1](https://github.com/patimweb-labs/quiver-tools/compare/v0.2.0...v0.2.1) (2026-11-26)


### Behoben

* **m365:** startet ohne Anwendungs-ID und sagt, woher sie kommt ([e7a2fd8](https://github.com/patimweb-labs/quiver-tools/commit/e7a2fd80f756a3d71eac596706e5048c7f33479f))
* **m365:** startet ohne Anwendungs-ID und sagt, woher sie kommt ([a7d19f3](https://github.com/patimweb-labs/quiver-tools/commit/a7d19f39c2b851cc0597785e2a837e393c583840))

## [0.2.0](https://github.com/patimweb-labs/quiver-tools/compare/v0.1.0...v0.2.0) (2026-11-26)


### ⚠ BREAKING CHANGES

* **m365:** Ohne M365_CLIENT_ID startet quiver-m365 nicht mehr.

### Neu

* **m365:** Anwendungs-ID kommt vom Server statt aus dem Programm ([7836361](https://github.com/patimweb-labs/quiver-tools/commit/7836361a0bb0b0aa345dc9de7620d0fb8ac8c24c))

## [0.1.0](https://github.com/patimweb-labs/quiver-tools/releases/tag/v0.1.0) (2026-11-25)

Die erste Fassung: MCP-Werkzeuge als einzelne Programme, je System ein Archiv
(macOS `aarch64` und `x86_64`, Linux `x86_64` und `aarch64`, Windows `x86_64`)
mit `LISTE.json` und `SHA256SUMS`.

### Neu

* Kern: Werkzeug-Schnittstelle, Zugang aus der Umgebung, Bibliothekseintrag, MCP-Hilfen
* Outline, E-Mail, SSH, ABAP, ZEP, Microsoft 365, Jira, Confluence, Zephyr, Azure DevOps
* Diagramme lokal: Excalidraw, draw.io und ein eigenes Format
* `verteilung/`: bauen, strippen, ad hoc signieren (macOS), paketieren und jedes Programm vor der Veröffentlichung starten
