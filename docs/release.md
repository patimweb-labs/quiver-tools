# Release der Programme

Die Version kommt aus den Conventional Commits: `release-please` führt einen
Release-PR, in dem Version (in `Cargo.toml` und `Cargo.lock`) und `CHANGELOG.md`
stehen. Wird dieser PR gemergt, legt derselbe Workflow-Lauf Tag und Release an,
baut die fünf Archive, prüft sie und hängt sie samt `SHA256SUMS` an. Der Weg über
einen Tag von Hand bleibt daneben bestehen.

Warum Tag und Bau im selben Lauf: einen Tag, den `GITHUB_TOKEN` anlegt, löst
**keinen** neuen Workflow aus. `release-please` erzeugt den Tag also in dem Lauf,
der danach bauen muss.

## Was ein Lauf tut

| Auslöser | Ergebnis |
|---|---|
| Push nach `main` ohne neues Release | nur der Release-PR wird geführt; es wird nichts gebaut |
| Merge des Release-PR (Push nach `main` mit neuem Release) | Tag und Release anlegen, fünf Archive bauen, prüfen, `SHA256SUMS` und Archive anhängen |
| Tag `vX.Y.Z` (von Hand) | dasselbe, Tag und Release kommen von Hand |
| „Run workflow“ | nur bauen; die Archive liegen als Artefakte am Lauf |
| Pull Request | nur bauen, und nur wenn `verteilung/**`, der Workflow oder die Release-Konfiguration sich ändern |

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

## Wer die Version anhebt

`release-please` liest die Commits seit dem letzten Release und schlägt die
nächste Version vor. `bump-minor-pre-major: true` gilt bis 1.0: ein `feat` hebt
die zweite Stelle (0.1.0 → 0.2.0), ein `fix` die dritte (0.1.0 → 0.1.1). Ein
`feat!`/`BREAKING CHANGE` hebt bis 1.0 ebenfalls nur die zweite Stelle, danach
die erste.

| Datei | Rolle |
|---|---|
| `release-please-config.json` | Release-Typ, Tag-Form (`v<version>`, ohne Komponente), Abschnitte des Changelogs, die Dateien mit der Version |
| `.release-please-manifest.json` | der Stand, der zuletzt veröffentlicht wurde — heute `0.1.0` |
| `CHANGELOG.md` | die Historie; von Hand steht hier nur der Eintrag zu `0.1.0` |
| `Cargo.toml` (`[workspace.package] version`) | die eine Version, die alle Kisten erben |
| `Cargo.lock` | dieselbe Version je Kiste, sonst bricht `--locked` |

Die Version wird **nur** über `release-please` angehoben; von Hand ist sie ein
Fehler, weil `Cargo.lock` und `CHANGELOG.md` sonst auseinanderlaufen. Was der
Agent nicht kann: der Release-PR, den `GITHUB_TOKEN` anlegt, löst keine
`pull_request`-Läufe aus — die Prüfungen aus `pruefen.yml` laufen auf ihm nicht
(siehe `docs/offen.md`).

## Der Tag von Hand

Ein Tag bleibt möglich, etwa für eine Fassung aus einem Stand, der noch nicht
über den Release-PR lief:

1. Version in `Cargo.toml` anheben; ein `cargo build` zieht `Cargo.lock` nach.
   Beides gehört in denselben Commit (und `CHANGELOG.md` mit dem Eintrag).
2. Über Branch und Pull Request nach `main` — `main` bleibt lauffähig.
3. `git tag vX.Y.Z` setzen und den Tag pushen.
4. Der Lauf baut alle Archive, prüft sie und legt das Release an. Passt der Tag
   nicht zur Version, bricht er vor dem ersten Bau ab.

## Eine veröffentlichte Version bleibt, wie sie ist

Vor dem Bau fragt `verteilung/release-ziel.sh`, ob es zu dem Tag schon ein
Release mit Dateien gibt:

- **kein Release** → wird angelegt,
- **Release ohne Dateien** (der Bau ist früher gescheitert) → wird ergänzt,
- **Release mit Dateien** → Abbruch: dieselbe Version ein zweites Mal ist ein
  Fehler. Es wird weder neu gebaut noch mit `--clobber` überschrieben. Für eine
  neue Fassung steigt die Version, und das macht der Release-PR.

Die erste Fassung **`v0.1.0`** ist auf dem Weg vor `release-please` entstanden
und bleibt, wie sie ist: `.release-please-manifest.json` führt sie als
veröffentlicht, `bootstrap-sha` markiert für den ersten Lauf den Commit, bis zu
dem die Historie als veröffentlicht gilt. `gh release upload` läuft ohne
`--clobber`, damit auch ein Fehlgriff am Tag nichts überschreibt.

Was ein Lauf mit Tag und Release vorhat, lässt sich ohne CI durchspielen:
`verteilung/release-ziel.sh` liest die Ereignisumgebung und gibt
`version=`, `tag=` und `veroeffentlichen=` aus (siehe „Lokal“).

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

`permissions` ist oben leer; jeder Job nimmt sich Lesen. Schreiben dürfen nur zwei
Jobs: `release_please` (`contents: write`, `pull-requests: write`, damit er PR, Tag
und Release führt) und `veroeffentlichen` (`contents: write`, damit Archive und
`SHA256SUMS` ans Release kommen). Der Workflow braucht **keine Geheimnisse** —
auch nicht auf Pull Requests, wo keine ausgewertet werden.

## Lokal

Dieselben Schritte, ohne CI:

```bash
verteilung/version.sh                  # Version aus [workspace.package]
verteilung/release-ziel.sh             # was dieser Lauf täte (braucht EREIGNIS, REF, …)
verteilung/paket.sh                    # bauen, strippen, signieren, dist/*.tar.gz
verteilung/pruefen.sh dist/quiver-tools-*.tar.gz
```

Die Ereignisregeln lassen sich ohne CI prüfen: `release-ziel.sh` liest
`EREIGNIS`, `REF`, `RP_RELEASED`, `RP_TAG` und `REPOSITORY` aus der Umgebung und
fragt `gh api` nach einem bestehenden Release. Mit einer `gh`-Attrappe im `PATH`
spielt man die fünf Fälle (Push nach `main` mit und ohne Release, Tag, Pull
Request, „Run workflow“) durch.

`CARGO_TARGET_DIR` wird beachtet (die Aufgaben bauen nach `target-pi/`), sonst gilt
`target/`. `dist/` steht in `.gitignore`.

`actionlint` prüft den Workflow; die Ergänzung für den Intel-Läufer steht in
`.github/actionlint.yaml`.
