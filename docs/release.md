# Release der Programme

Ein Tag `vX.Y.Z` baut alle `quiver-*`-Programme für jedes unterstützte System und
legt sie als GitHub-Release ab. Die Version steht in `[workspace.package]` der
`Cargo.toml`; der Tag muss dazu passen, sonst bricht der Lauf mit einer Meldung ab
(sonst hieße das Release anders als die Programme in ihrer eigenen Antwort).

## Was ein Lauf tut

| Auslöser | Ergebnis |
|---|---|
| Tag `vX.Y.Z` (von Hand) | bauen, prüfen, Release anlegen und Archive anhängen |
| „Run workflow“ | nur bauen; die Archive liegen als Artefakte am Lauf |
| Pull Request | nur bauen, und nur wenn `verteilung/**` oder der Workflow sich ändern |

Gebaut wird auf fünf Läufern, je System ein Archiv:

| System | Architektur | Läufer |
|---|---|---|
| macOS | `aarch64`, `x86_64` | `macos-latest`, `macos-15-intel` |
| Windows | `x86_64` | `windows-2025` |
| Linux | `x86_64`, `aarch64` | `ubuntu-22.04`, `ubuntu-22.04-arm` |

Der Name ist `quiver-tools-<version>-<system>-<arch>.tar.gz`, also etwa
`quiver-tools-0.1.0-macos-aarch64.tar.gz` (die Aufgabe sagt „arm64/x64“; im Namen
steht die Rust-Schreibweise `aarch64`/`x86_64`, damit er zur Zielplattform passt).

Im Archiv liegen die Programme **flach** nebeneinander, dazu `LISTE.json`. Der quiver-Server
übernimmt die Archive und liefert jedem Gerät die zugewiesenen Programme:

```json
{
  "version": "0.1.0",
  "system": "macos",
  "arch": "aarch64",
  "programme": [
    { "name": "quiver-abap", "version": "0.1.0", "groesse": 8123456, "sha256": "…" }
  ]
}
```

Dazu kommt `SHA256SUMS` über **alle** Archive; es liegt neben ihnen am Release,
nicht in ihnen.

## Einen Release machen

1. Version in `Cargo.toml` unter `[workspace.package]` anheben; ein
   `cargo build` zieht `Cargo.lock` nach. Beides gehört in denselben Commit.
2. Über Branch und Pull Request nach `main` — `main` bleibt lauffähig.
3. `git tag vX.Y.Z` setzen und den Tag pushen.
4. Der Lauf baut alle Archive, prüft sie und legt das Release an. Passt der Tag
   nicht zur Version, bricht er vor dem ersten Bau ab.

## Nativ, nicht über Kreuz

Jedes System baut auf seinem eigenen Läufer. Rust unter Emulation (etwa aarch64 auf
einem x86_64-Läufer) braucht eine Stunde statt Minuten, und ein Cross-Compile von
`aws-lc-sys` und `russh` wäre eine Fehlerquelle ohne Gewinn.

Linux baut auf **22.04** (glibc 2.35): ein Programm, das nur auf dem neuesten Image
läuft, hilft niemandem.

## Strippen, Signieren, Laufzeit

- **Gestrippt** wird mit `strip` (macOS: `strip -x`, damit die dynamischen Symbole
  bleiben). Windows braucht das nicht: dort liegen die Debug-Informationen in einer
  eigenen `.pdb`, das Programm selbst trägt keine.
- **macOS wird ad hoc signiert** (`codesign --force --sign -`, nach dem Strippen,
  weil die Signatur über den Bytes liegt). Es gibt kein Apple-Konto und kein
  Zertifikat; Gatekeeper fragt beim ersten Start nach einer Bestätigung. Das steht
  so im Release-Text.
- **Windows braucht keine zusätzliche Laufzeit**: gebaut mit
  `-C target-feature=+crt-static`, also statischer C-Runtime. Damit hängt das
  Programm weder am VC++-Redistributable noch an MSYS- oder Cygwin-DLLs; es läuft
  allein. Das Subsystem bleibt „Konsole“, weil MCP über stdin/stdout läuft — ein
  `windows`-Subsystem würde beides kappen.
- `--locked`: gebaut wird genau der Stand aus `Cargo.lock`, damit ein Release
  reproduzierbar ist.

## Die Prüfung vor der Veröffentlichung

`verteilung/pruefen.sh` arbeitet auf dem **Archiv**, nicht auf `target/` — geprüft
wird, was ausgeliefert wird:

1. Archiv entpacken, `LISTE.json` gegen die Programme halten (jedes Programm mit
   Name und SHA-256, keine Lücke in beide Richtungen).
2. Je Programm `--version`: Name und Version müssen stimmen.
3. Je Programm `eintrag`: der Bibliothekseintrag kommt als JSON (`name`, `titel`,
   `laufzeit`).
4. Je Programm MCP über Pipes: `initialize` und `tools/list` antworten. Das ist der
   eigentliche Startbeweis — das Programm spricht über stdin/stdout, ohne Konsole
   und ohne Zugang; fehlt der Zugang, antworten die Werkzeuge mit dem Hinweis statt
   mit einem Fehler.

Ein Wächter bricht nach 30 s ab: ein Programm, das auf ein offenes stdin wartet,
darf den Lauf nicht festhalten (`timeout` gibt es nicht auf jedem System).

## Rechte und Geheimnisse

`permissions` ist oben leer; jeder Job nimmt sich Lesen, **schreiben darf allein**
der Job `veroeffentlichen` (`contents: write`). Der Workflow braucht **keine
Geheimnisse** — auch nicht auf Pull Requests, wo keine ausgewertet werden.

## Lokal

Dieselben Schritte, ohne CI:

```bash
verteilung/version.sh                  # Version aus [workspace.package]
verteilung/paket.sh                    # bauen, strippen, signieren, dist/*.tar.gz
verteilung/pruefen.sh dist/quiver-tools-*.tar.gz
```

`CARGO_TARGET_DIR` wird beachtet (die Aufgaben bauen nach `target-pi/`), sonst gilt
`target/`. `dist/` steht in `.gitignore`.

`actionlint` prüft den Workflow; die Ergänzung für den Intel-Läufer steht in
`.github/actionlint.yaml`.
