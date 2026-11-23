# AGENTS.md — Vorlage für alle Weppelmann-Projekte

> Diese Datei liegt in **jedem** Repo (UI-Bibliothek und Apps). Sie ist die verbindliche
> Arbeitsanweisung für Menschen und Agenten. Abweichungen nur mit Eintrag in
> `docs/adr/` und Vermerk hier. Der projektspezifische Teil ist §9.
>
> **Vermerk (ADR 0002):** quiver-tools hat **keine** eigene Oberfläche. Zugänge gibt der User im
> quiver-Fenster über die Felder des Bibliothekseintrags ein; die Tools beschreiben ihre Felder
> (`Werkzeug::felder`) und lesen die Werte aus der Umgebung. Die UI-Regeln (§2.6, 2.7, 2.11, 2.12)
> gelten hier nicht.

## 1. Worum es geht

Eigene MCP-Tools für mcp-quiver in Rust (Outline, E-Mail, SSH, ABAP), Nachfolger der pi-Erweiterungen.
Zugänge gibt der User im quiver-Fenster ein (Felder des Eintrags), nie ein Agent. Details: [SPEC.md](SPEC.md).

## 2. Nicht verhandelbar

1. **Datenorientiert.** Das Datenmodell ist die einzige Wahrheit, die Oberfläche ist
   `render(model) → Sicht`. Kein Zustand in der Darstellung, der nicht im Modell steht.
2. **Zustände als Summentypen.** `enum Zustand { Leer, Laedt, Fertig(Daten), Fehler(Text) }`
   statt Fächer aus Booleans. Ein Rust-`enum` mit `serde` und ein TS-Diskriminator
   beschreiben denselben Vertrag.
3. **Sichtbarkeit folgt aus dem Modell.** „Einblenden" ist keine Anweisung, sondern eine
   Folge des Zustands. Kein `show()`, kein `display:none` als Logik, keine `v-if`-Kette
   für Geschäftslogik.
4. **Keine Logik in der Darstellung.** Rechnen, laden, filtern in `domain` (Rust) oder in
   reinen TS-Funktionen — beide mit Unit-Tests.
5. **SIMPLE** (Wrigstad): immer ein lauffähiges Programm, nach jeder Änderung bauen,
   Tests laufen lassen, Problem rekursiv verkleinern. Cheats sind erlaubt, aber
   ticketpflichtig, lokal begrenzt und werden abgebaut.
6. **Keine Farben, Größen oder Abstände im Code.** Nur semantische Tokens
   (`var(--wp-color-text-default)` in Vue, `f.text_default` / `thema::SPACE_4` in egui).
   Hex-Werte, Pixel-Magie und Grundwerte in Ansichten sind Fehler.
7. **Drei Themes, sonst nichts:** `dark`, `light`, `mono`. Keine Runtime-Themes,
   keine Nutzer-Themes, kein Theme-Editor.
8. **Tests sind Teil der Lieferung.** Unit, Integration, Vertrag, Rundlauf.
   Ein Fix ohne Test ist ein Fix auf Probe.
9. **Keine Secrets** in Code, Commits, Logs oder Testdaten. Konfiguration über
   Umgebung, Beispiel in `.env.example`.
10. **Plattformneutral.** Kein `cfg(target_os)` und keine Pfadannahmen im Frontend.
    Unterschiede kommen als Daten (`platform` im Modell → `data-platform` am Root).
    Tastenkürzel über `Mod`, nie fest `Ctrl`.
11. **1:1 gleiches Aussehen.** Client (egui) und Admin (Vue) sehen identisch aus.
    Beide erzeugen ihre Themes aus denselben Token-Dateien; Bausteine heißen gleich
    und haben gleiche Maße.
12. **Hand-Cursor auf allem Klickbaren.** In egui nicht Standard: die Bausteine setzen
    `CursorIcon::PointingHand` zentral, ein Oberflächentest prüft es. In Vue
    `cursor: pointer` im Baustein, nie in der Ansicht.

## 3. Struktur

```
crates/kern       Werkzeug-Trait, Zugang aus der Umgebung, Eintrag für quiver, MCP-Hilfen → Unit-Tests
crates/<tool>     ein Programm quiver-<tool> je Tool                            → Integration (Attrappe)
```

Richtung: `kern ← <tool>`. Tools kennen einander nicht.

## 4. Befehle

```bash
# alles prüfen (das ist der Befehl vor jedem Commit)
cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo test --workspace
```

`cargo test` **mit** `--workspace`: ohne das prüft Cargo nur das Wurzelpaket.
Weitere Befehle kommen mit dem Code.

## 5. Definition of Done

- [ ] Tests grün (Unit, Integration gegen die Attrappe)
- [ ] `clippy -D warnings` sauber
- [ ] **Kein Token** in Log, Fehlertext, Tool-Antwort, Testausgabe oder Datei
- [ ] Dokumentation angepasst: `README.md`, `SPEC.md`, `docs/`
- [ ] Cheats dieses PRs als Ticket vermerkt

## 6. Commits und Releases

- Conventional Commits (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `build:`, `ci:`).
- `release-please` mit `bump-minor-pre-major: true`.
- Kleine Commits, ein Gedanke pro Commit, deutschsprachige Betreffzeile erlaubt.
- **Immer über einen Branch und einen Pull Request.** `main` ist immer lauffähig.

## 7. Dokumentation

- `README.md` (Englisch) und `README.de.md`: was die App tut, für wen, Screenshots
  (dark + light), Installation je Plattform, ein Absatz zu Daten und Datenschutz.
- `docs/architektur.md`, `docs/datenfluss.md`, `docs/adr/NNNN-titel.md`.
- `SPEC.md`: jede Festlegung mit Begründung und Preis.
- Screenshots werden **erzeugt** (egui: `egui_kittest`, Vue: Playwright), nicht von
  Hand abgelegt.

## 8. Projektspezifisch

- **Zugangsdaten:** kommen als Umgebung von quiver (Geheimes liegt dort in der Tresor-Datei); nie in Dateien,
  Logs oder Antworten.
- **Keine `*_setup`-Tools.** Felder beschreibt `Werkzeug::felder()`, `quiver-<tool> eintrag` gibt den
  Bibliothekseintrag aus. Hilfe-Links (z. B. „Token erstellen“) über `Feld::link`.
- **Vorlagen** liegen unter `~/Documents/Workspaces/pi-*`; Semantik übernehmen, nicht den Code übersetzen.
- **Verändernde Tools** tragen `destructiveHint`, lesende `readOnlyHint`.
