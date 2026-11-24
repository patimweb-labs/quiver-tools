# 03 – Anmeldung über Befehle, nie durch einen Agenten

**Entscheidung des Nutzers:** Die Microsoft-Anmeldung löst der User im quiver-Fenster aus
(Knopf „Anmelden“), **nicht** ein Agent über einen Tool-Aufruf. mcp-quiver ruft dafür drei
Unterbefehle von `quiver-m365` auf (mit derselben Umgebung wie der MCP-Prozess).

## Unterbefehle (`crates/mcp`, `befehle.rs`)

- `quiver-m365 anmelden`: Browser-Ablauf aus `crates/anmeldung` (Wartezeit hier **5 min** statt 45 s),
  danach `GET /me` und Ausgabe des Kontos auf stdout (eine Zeile, `userPrincipalName`). Exit 0 = angemeldet.
  Fehler → stderr, Exit 2.
- `quiver-m365 status`: **ohne** Browser. Refresh-Token vorhanden und Erneuern klappt → stdout erste Zeile
  = Konto (aus `GET /me`), Exit 0. Kein Refresh-Token oder `invalid_grant` → Exit 1. Netz-/sonstiger Fehler → Exit 2.
- `quiver-m365 abmelden`: Refresh-Token löschen, Exit 0.

## MCP-Betrieb ändern

- Im MCP-Betrieb (ohne Unterbefehl) öffnet `quiver-m365` **nie** einen Browser. Fehlt ein gültiges
  Refresh-Token, antwortet jedes Tool (außer `abmelden`) mit dem Text:
  „Nicht angemeldet – bitte im quiver-Fenster unter Tools → Microsoft 365 auf „Anmelden“ klicken.“
  (kein MCP-Fehler, damit der Agent es dem User sagt).
- Umsetzung: neuer `Browser`-Typ `KeinBrowser` (liefert einen eigenen Fehler) bzw. eine Option an
  `EntraAnmeldung`, die den Browser-Ablauf abschaltet und sofort `AnmeldeFehler::NichtAngemeldet`
  (neu, additiv in `lib.rs`) zurückgibt. Wer `quiver-m365` ohne quiver benutzt, kann mit
  `M365_BROWSER=erlaubt` das alte Verhalten (Browser aus dem Tool heraus) wählen.
- Tool `abmelden` bleibt, Annotation `read_only_hint = false`, `destructive_hint = false`.

## Abnahme

- Tests gegen die Attrappen: `anmelden` (Test-Browser) → Exit 0 + Konto; `status` danach Exit 0;
  `status` ohne Token Exit 1 und **kein** Browseraufruf; `abmelden` → `status` Exit 1.
- MCP-Test: ohne Token liefert `profil` den Hinweistext, und die Attrappe sieht **keinen** Authorize-Aufruf.
- `M365_BROWSER=erlaubt` → altes Verhalten (ein bestehender Test reicht).
- README: Abschnitt „Anmeldung“ auf den quiver-Knopf umstellen, Unterbefehle dokumentieren,
  mcp-quiver-Eintrag mit `"anmeldung": {"anmelden":["anmelden"],"status":["status"],"abmelden":["abmelden"]}`.
- Prüfbefehl grün.
