# 0001 – Programm-Gerüst: `starten(werkzeug, |profile| server)`

> Stand: 2026-11-23. Aufgabe: [docs/aufgaben/01-kern.md](../../docs/aufgaben/01-kern.md) §4.

## Kontext

Jedes Tool ist ein Programm mit zwei Aufgaben. **MCP über stdio** ist der Normalfall:
quiver startet `quiver-<tool>` ohne Argument und spricht JSON-RPC auf stdin/stdout.
**Einrichtung** ist der Sonderfall: `quiver-<tool> einrichten` öffnet eine Seite im
Browser (SPEC §4). Beide Wege brauchen Teile des Werkzeugs, aber verschiedene:

- die Einrichtung braucht `Werkzeug` (Felder, `testen`) und den Profilspeicher;
- MCP braucht den Profilspeicher und den rmcp-Server, den nur das Tool bauen kann.

## Entscheidung

```rust
pub async fn starten<W, S, F>(werkzeug: W, server: F) -> ExitCode
where
    W: Werkzeug,
    S: rmcp::ServerHandler,
    F: FnOnce(Profile) -> S;
```

Der Kern wertet `std::env::args()` aus, entscheidet zwischen den Wegen und reicht im
MCP-Fall den geladenen `Profile`-Speicher (`Profile` = die Profile **eines** Werkzeugs)
an die mitgegebene Funktion. Die `main` eines Tools bleibt damit zwei Zeilen:

```rust
starten(Outline, |profile| OutlineServer::new(profile)).await
```

## Warum so

1. **Der Kern kennt kein Tool.** Der Server ist ein Typparameter, kein Trait-Objekt:
   `Werkzeug::testen` gibt ein `impl Future` zurück und ist darum nicht objekt-sicher –
   ein `Box<dyn Werkzeug>` ginge nicht.
2. **Die Profilwahl passiert je Aufruf, nicht beim Start.** Der Parameter `profil` gilt
   pro MCP-Tool-Aufruf (SPEC §2). Darum bekommt der Server den *Speicher*, nicht ein
   fertig geladenes Profil; `Profile::holen(Option<&str>)` entscheidet später. Ein beim
   Start geladenes Profil wäre nach dem ersten `einrichten` veraltet.
3. **Auch ohne Profil startbar.** Fehlt ein Profil, muss das Tool dennoch antworten
   („Noch kein Zugang eingerichtet“ statt Fehler). Ein `Result<Profile, _>` im Konstruktor
   hätte jedes Tool zu einer Fehlerbehandlung im `main` gezwungen, die es nicht braucht.
4. **stdout bleibt heilig.** Version, Hilfe und Fehler gehen nach stdout/stderr, nie
   dazwischen; das MCP-Protokoll hat stdout für sich.

## Preis

- Ein Tool darf seinen Server nicht selbst über stdio dienen – es muss `starten` rufen.
  Das ist gewollt (ein Weg, ein Verhalten), schließt aber Sonderfälle (z. B. einen
  eigenen HTTP-Transport) aus; die kämen als zusätzliche Funktion in den Kern.
- Der Profilspeicher wird einmal beim Start geladen (Ordner und Tresor). Wer den Ordner
  im laufenden Prozess wechselt, sieht das erst nach einem Neustart – Profile selbst
  werden bei jedem Aufruf neu gelesen.
- `--version` nennt die Version des Kerns; alle Crates teilen sich die
  Workspace-Version, deshalb ist das dieselbe Zahl.
- Ohne `tracing-subscriber` (nicht in den erlaubten Abhängigkeiten) sind
  `tracing`-Ereignisse stumm; sichtbare Meldungen gehen über `eprintln!` nach stderr.
