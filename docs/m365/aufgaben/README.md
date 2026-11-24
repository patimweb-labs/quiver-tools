# Aufgaben

Rollen: Claude ist Product Owner und Architekt, pi ist Entwickler. Jede Aufgabe
hier ist ein abgeschlossenes Paket mit Abnahmekriterien. Claude nimmt ab
(Diff lesen, Prüfbefehl) und committet.

## Regeln für jede Aufgabe

1. Lies zuerst `AGENTS.md` und `SPEC.md`. Die Regeln dort gelten.
2. **Nicht ändern ohne Rückfrage:** `crates/anmeldung/src/lib.rs` (die öffentliche Schnittstelle), `SPEC.md`,
   `AGENTS.md`, `Cargo.toml` im Wurzelverzeichnis (nur `[workspace.dependencies]`
   ergänzen ist erlaubt). Passt der Vertrag nicht, hör auf und schreib in
   deine Antwort, was fehlt.
3. Rust-Version: `rust-toolchain.toml` (1.100.0). Wenn `cargo --version` etwas
   anderes zeigt: `export PATH="$(dirname $(rustup which cargo --toolchain 1.100.0)):$PATH"`.
4. Prüfbefehl, muss am Ende grün sein:
   `cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo test --workspace`
5. Namen im Code deutsch wie im Rest des Projekts (ASCII: `ae`, `oe`, `ue`, `ss`).
6. Keine Commits, kein Push. Am Ende: kurze Liste, was du gemacht hast, und was offen ist.
7. Keine neuen Abhängigkeiten außer den in der Aufgabe genannten.
8. **Kein Token** in Log, Fehlertext, Tool-Antwort oder Testausgabe. Tests laufen nur gegen die Attrappe, nie gegen login.microsoftonline.com.
