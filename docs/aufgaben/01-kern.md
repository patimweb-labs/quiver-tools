# 01 – Kern: Profile, Tresor, Einrichtungsseite, MCP-Hilfen

Crate: `crates/kern` (`quiver-tools-kern`). Öffentliche Schnittstelle in `src/lib.rs` (Werkzeug, Feld,
Profil, `hinweis_einrichten`) **nicht ändern**, nur ergänzen. Regeln: `SPEC.md` §2–§4, §7.

## Abhängigkeiten (nur diese)

`axum = "0.8"`, `tokio` (rt-multi-thread, macros, net, time, sync, signal), `serde`, `serde_json`,
`toml = "0.9"` (neueste, die baut), `dirs = "6"`, `keyring = "4"`, `open = "5"`, `rand = "0.9"`,
`anyhow`, `thiserror`, `tracing`, `html-escape = "0.2"`, `rmcp = { version = "3.11", features = ["server", "transport-io", "macros", "schemars"] }`.
Dev: `reqwest` (json, rustls), `tempfile = "3"`.

## Umsetzen

1. **Tresor:** Trait `Tresor` (lesen/schreiben/löschen), `Schluesselbund` (Dienst `quiver-<werkzeug>`,
   Schlüssel `<profil>/<feld>`), `SpeicherTresor`; Auswahl über `QUIVER_TOOLS_TRESOR=speicher`.
2. **Profilspeicher:** `Profile::laden(werkzeug, tresor)`: TOML unter
   `<QUIVER_TOOLS_DIR oder dirs::config_dir()/quiver-tools>/<name>/profile.toml`
   (`standard = "…"`, `[[profil]] name, beschreibung, [profil.werte]`). Methoden: `alle()`, `standard()`,
   `holen(Option<&str>) -> Result<Profil, ProfilFehler>` (lädt Geheimnisse aus dem Tresor), `speichern(profil)`,
   `loeschen(name)` (auch die Geheimnisse), `als_standard(name)`. Schreiben atomar (temp + rename), Datei 0600.
   Prüft Pflichtfelder gegen `Werkzeug::felder()`. Profilnamen: `a-z0-9-_`, 1–40 Zeichen.
3. **Einrichtungsseite:** `pub async fn einrichten<W: Werkzeug>(w: W) -> std::process::ExitCode` (SPEC §4):
   Token-Pfad, Liste, Formular aus dem Schema (Passwort-Felder: leer lassen = unverändert, „gesetzt“-Hinweis),
   Löschen mit Rückfrage, Standard setzen, „Verbindung testen“ (ruft `w.testen` mit dem **gespeicherten** Profil,
   zeigt Ergebnis), „Fertig“. Formulare per POST (`application/x-www-form-urlencoded`), danach Redirect (PRG).
   HTML-Ausgaben immer escapen. Optik: `assets/tokens.css` und Schriften per `include_str!`/`include_bytes!`
   einbetten und ausliefern; Bausteine wie in mcp-quiver (Kopf mit Wortmarke „<Titel> · Einrichtung“ und
   Segmentauswahl Theme, Karten, Hauptknopf/Knopf mit `--wp-size-control-md`, Hand-Cursor). Sieh dir zur
   Orientierung `~/Documents/Workspace/mcp-quiver/admin/src/bausteine/` und `docs/bilder/muster-vue-*.png` an.
   Browser öffnen über ein Trait `Browser` (echt: `open::that`; Test: nichts tun, URL zurückgeben).
4. **Programm-Gerüst:** `pub async fn starten<W, S>(w: W, server: impl FnOnce(Profile) -> S)` o. ä., das
   `std::env::args` auswertet: `einrichten` → Einrichtungsseite; `--version`; sonst MCP über stdio mit dem
   vom Tool gebauten rmcp-Server. Logs nach **stderr**. Die genaue Form legst du fest – sie muss für alle vier
   Tools passen; begründe sie in der Antwort.
5. **MCP-Hilfen:** gemeinsamer Parameter `profil: Option<String>`; Hilfsfunktion, die aus `ProfilFehler::Keins`
   den Text `hinweis_einrichten` macht (kein MCP-Fehler) und aus anderen Fehlern MCP-Fehler ohne Geheimnisse;
   eine Funktion, die das lesende Tool `<name>_profile` (Liste ohne Geheimnisse) bauen hilft;
   `kuerzen(text, max_zeichen) -> (String, Option<usize>)` an Wortgrenzen.

## Abnahme

- Tests: Profile anlegen/laden/löschen/Standard, Geheimnisse nur im Tresor (TOML-Datei enthält sie nicht),
  Pflichtfelder, Namensregeln, atomares Schreiben.
- Einrichtungsseite über HTTP (Test-Browser): falscher Token-Pfad → 404; Profil anlegen per POST → gespeichert;
  bearbeiten mit leerem Passwort lässt das Geheimnis stehen; **keine** Antwortseite enthält ein Geheimnis;
  „Verbindung testen“ zeigt die Meldung eines Test-Werkzeugs; „Fertig“ beendet mit Exit 0; HTML-Escaping
  (Profilbeschreibung mit `<script>`).
- Ein Beispiel-Werkzeug `examples/demo.rs` (zwei Felder, eines geheim), mit dem man die Seite von Hand ansehen
  kann: `QUIVER_TOOLS_TRESOR=speicher cargo run -p quiver-tools-kern --example demo -- einrichten`.
- Bildschirmfoto der Seite (dark und light) nach `docs/bilder/einrichtung-{dark,light}.png` – z. B. mit
  Playwright aus `~/Documents/Workspace/mcp-quiver/admin/node_modules` oder einem Headless-Browser deiner Wahl,
  **ohne** neue Rust-Abhängigkeit. Geht das nicht, schreib es in die Antwort.
- Prüfbefehl grün.
