# 03 – Zugang aus der Umgebung statt Einrichtungsseite

**Entscheidung des Nutzers (2026-11-23):** Keine Einrichtung im Browser. Zugangsdaten (Basis-URL, Token …)
gibt der User **im quiver-Fenster** ein – über die Felder des Bibliothekseintrags (Quelle `client` bzw.
`geheim`; quiver legt Geheimes in den Schlüsselbund und setzt beim Start die Umgebung des Tools).
Die Feldbeschreibung kommt aus der Extension (`Werkzeug::felder()`), der Client bleibt allgemein.

## Kern (`crates/kern`)

1. **Profil aus der Umgebung:** Für jedes Feld aus `Werkzeug::felder()` liest der Kern
   `<NAME>_<FELD>` (Großbuchstaben, z. B. `OUTLINE_URL`, `OUTLINE_API_KEY`). Sind alle Pflichtfelder gesetzt,
   gibt es genau ein Profil `standard` (Geheimes aus der Umgebung, nicht aus dem Tresor). Fehlen Pflichtfelder,
   antwortet jedes Tool mit dem Text: „Zugang fehlt – bitte im quiver-Fenster unter Tools → <Titel> die
   Angaben eintragen.“ (kein MCP-Fehler). Leere Werte gelten als fehlend.
2. **Neuer Unterbefehl `quiver-<tool> eintrag [--befehl <pfad>]`:** gibt den Bibliothekseintrag für mcp-quiver
   als JSON aus (Format: `quiver_domain::Eintrag`, siehe
   `~/Documents/Workspace/mcp-quiver/crates/domain/src/lib.rs`):
   `id` = Name, `titel`, `beschreibung` (neue Methode `Werkzeug::beschreibung()`), `version` = Crate-Version,
   `herkunft: "eigen"`, `laufzeit: { art: "stdio", befehl: <pfad oder aktuelles Programm>, umgebung: { "<NAME>_<FELD>": "${<feld>}" … } }`,
   `felder`: je Feld `{ name, beschreibung: titel + " – " + hilfe, quelle: "geheim" bei Passwort sonst "client", pflicht, standard }`.
   Kein `einrichtung`-Feld.
3. **Einrichtungsseite entfernen:** `einrichten`-Unterbefehl, `seite`, `browser` und ihre Tests raus; ebenso
   der Profilspeicher in Datei/Tresor, falls er danach ungenutzt ist (ausgenommen, was Tests der Tools noch
   brauchen). `<tool>_profile` bleibt als lesendes Tool und zeigt das eine Profil ohne Geheimnisse (oder
   „kein Zugang“). Der Parameter `profil` an den Tools entfällt.
4. `Werkzeug::testen` bleibt (für einen späteren „Testen“-Knopf), wird aber nicht mehr aufgerufen. Neuer
   Unterbefehl `quiver-<tool> testen` (liest die Umgebung, gibt die Meldung aus, Exit 0/1) – nützlich zum Prüfen.

## Outline (`crates/outline`)

- Felder bleiben `url` und `api_key`, Umgebung also `OUTLINE_URL`, `OUTLINE_API_KEY`.
- Suche „über alle Workspaces“ entfällt (es gibt ein Profil); Parameter entfernen.
- Tests auf Umgebung umstellen; Ende-zu-Ende mit gesetzter Umgebung und ohne (Hinweistext).

## Doku

`SPEC.md` §2–§4 neu fassen (Zugang über quiver-Felder, keine Einrichtungsseite), README und
`docs/architektur.md` anpassen; `docs/adr/0002-zugang-ueber-quiver-felder.md` mit Begründung (Nutzerentscheidung,
Preis: vorerst ein Zugang je Tool).

## Abnahme

`quiver-outline eintrag` liefert gültiges JSON, das `quiver_domain::Eintrag` deserialisieren kann
(Test mit einer Kopie der Struktur oder per Schema-Vergleich). Prüfbefehl grün.
