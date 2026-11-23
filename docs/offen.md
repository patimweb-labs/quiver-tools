# Offene Punkte

Aus dem ersten echten Test gegen den Tenant von neoimpulse.de (2026-11-23, nur lesende Tools).

- [x] `mail_lesen` liefert den ganzen Verlauf (im Test 15 379 Zeichen). Für Agenten zu viel Kontext:
      `uniqueBody` statt `body` anbieten und/oder Parameter `max_zeichen` (Standard z. B. 4000) mit Hinweis „gekürzt“.
      → Aufgabe 04: standardmäßig `uniqueBody`, Parameter `verlauf` und `max_zeichen` (4000, max. 50 000),
      Antwort mit `gekuerzt`/`laenge_gesamt`, Leerzeilen zusammengefasst.
- [x] `termine` gibt Zeiten ohne Zeitzone aus (`2026-11-23T08:40:00.0000000`). Sie gelten in `M365_ZEITZONE`,
      das steht aber nirgends in der Antwort: Feld `zeitzone` ergänzen oder als `+01:00` ausgeben, Nachkommastellen weg.
      → Aufgabe 04: alle Zeiten in Antworten als RFC 3339 mit Versatz in `M365_ZEITZONE`, ganztägige Termine
      nur mit Datum und `ganztags: true`.
- [ ] Noch nicht echt getestet: `mail_senden`, `termin_anlegen`, `abmelden` (verändernd, bewusst ausgelassen).
