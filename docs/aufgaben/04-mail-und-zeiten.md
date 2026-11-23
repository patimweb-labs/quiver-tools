# 04 – Mails kompakter, Zeiten mit Zeitzone

Aus dem ersten echten Test (`docs/offen.md`). Crates: `crates/graph`, `crates/mcp`.

## A – `mail_lesen` liefert zu viel

Heute: der ganze Verlauf (im Test 15 379 Zeichen).

1. Standard: nur der **neue Teil** der Mail – Graph-Feld `uniqueBody` (mit
   `Prefer: outlook.body-content-type="text"`, `$select=…,uniqueBody`).
2. Neuer optionaler Parameter `verlauf: bool` (Standard `false`). `true` → `body` wie bisher.
3. Neuer optionaler Parameter `max_zeichen` (Standard **4000**, höchstens 50 000). Längerer Text wird
   an einer Wortgrenze gekürzt; die Antwort hat dann `"gekuerzt": true` und `"laenge_gesamt": <n>`.
   Sonst fehlen beide Felder.
4. Leerzeilen-Wüsten zusammenfassen: mehr als zwei Leerzeilen hintereinander → zwei.
5. Beschreibungen der Parameter (schemars) so, dass ein Agent versteht, wann er `verlauf` braucht.

## B – Zeiten mit Zeitzone

Heute: `termine` gibt `2026-11-23T08:40:00.0000000` aus, ohne Zone.

1. Alle Zeiten in Antworten (`termine`: start/ende; `mails_suchen`/`mail_lesen`: datum;
   `dateien_suchen`: geaendert; `termin_anlegen`: Rückgabe) als **RFC 3339 mit Versatz** in der Zeitzone
   aus `M365_ZEITZONE`, ohne Sekundenbruchteile: `2026-11-23T08:40:00+01:00`.
2. Graph liefert bei `calendarView` Ortszeit + `timeZone`, bei Mails UTC (`…Z`). Beides in die Zielzone
   umrechnen. Für die Umrechnung mit Sommerzeit: `jiff = "0.2"` (bringt die Zeitzonendatenbank mit,
   keine weitere Abhängigkeit) – oder, wenn du `time` behalten willst, `time-tz`. Nimm **eine** und begründe
   sie in der Antwort.
3. Ganztägige Termine: nur Datum (`2026-11-24`), `ganztags: true`.
4. Eingaben (`von`, `bis`, `start`, `ende`) bleiben wie sie sind (Ortszeit in `M365_ZEITZONE`), dürfen jetzt
   aber auch einen Versatz tragen (`…+01:00`, `…Z`).

## Abnahme

- Graph-Attrappe erweitern (`uniqueBody`, lange Mail, Zeitwerte in UTC und mit `timeZone`).
- Tests: Standard ohne Verlauf, `verlauf: true`, Kürzung an Wortgrenze mit `gekuerzt`/`laenge_gesamt`,
  Leerzeilen; Zeitumrechnung Winter (+01:00) und Sommer (+02:00), ganztags, Eingabe mit Versatz.
- `docs/offen.md`: die zwei Punkte abhaken. README: Parameter von `mail_lesen` ergänzen.
- Prüfbefehl grün.
