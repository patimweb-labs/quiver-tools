# Offene Punkte

Aus dem ersten echten Test gegen den Tenant von neoimpulse.de (2026-11-23, nur lesende Tools).

- [ ] `mail_lesen` liefert den ganzen Verlauf (im Test 15 379 Zeichen). Für Agenten zu viel Kontext:
      `uniqueBody` statt `body` anbieten und/oder Parameter `max_zeichen` (Standard z. B. 4000) mit Hinweis „gekürzt“.
- [ ] `termine` gibt Zeiten ohne Zeitzone aus (`2026-11-23T08:40:00.0000000`). Sie gelten in `M365_ZEITZONE`,
      das steht aber nirgends in der Antwort: Feld `zeitzone` ergänzen oder als `+01:00` ausgeben, Nachkommastellen weg.
- [ ] Noch nicht echt getestet: `mail_senden`, `termin_anlegen`, `abmelden` (verändernd, bewusst ausgelassen).
