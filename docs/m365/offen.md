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

- [ ] **Teams: Dateien in Kanälen** (`GET …/channels/{kanal}/filesFolder` + SharePoint-Laufwerk).
      Braucht `Files.Read.All` (vorhanden), aber eigene Pfad- und Fehlerbehandlung.
- [ ] **Teams: Präsenz** („wer ist online“). Verlangt `Presence.Read.All` – noch nicht in der
      App-Registrierung, deshalb bewusst nicht gebaut.
- [ ] **Teams: Nachrichten bearbeiten und löschen.** Kanal-Beiträge brauchen zusätzlich
      `ChannelMessage.ReadWrite`, Chat-Nachrichten laufen über `/users/{id}/chats/…` (die eigene
      User-Id, nicht `/me`). Nicht im Umfang von Aufgabe 05.
- [ ] **Mehrere Tenants und Kunden-Teams.** Der Tresor-Schlüssel ist `<tenant>/<client_id>`; ein
      zweites Konto im selben Tenant braucht eine eigene Kennung. Namen von Teams können in
      Kunden-Tenants mehrdeutig sein – die Tools melden das (Kandidaten, keine Aktion), aber eine
      Auswahl über den quiver-Eintrag fehlt noch.
- [ ] **Teams-Aufwand (bewusste Abkürzungen aus Aufgabe 05):**
      - Die Antwortanzahl je Kanalbeitrag kostet einen zusätzlichen `GET …/replies` pro sichtbarem
        Beitrag (gedeckelt auf 50). Graph liefert kein `replyCount`. Bei vielen Beiträgen sind das
        viele Aufrufe; besser wäre ein `$expand=replies($top=0;$count=true)`, sobald Graph das
        verlässlich anbietet.
      - `teams_eingang` liest die Lesemarke `viewpoint.lastMessageReadDateTime` aus der Chatliste.
        Graph dokumentiert `viewpoint` am einzelnen Chat; liefert die Liste sie nicht mit, gilt ein
        Chat mit Vorschau als ungelesen (lieber einmal zu viel zeigen als etwas verpassen).
      - Die Reaktionsnamen sind auf `like/heart/laugh/surprised/sad/angry` und die üblichen Emojis
        abgebildet; weitere Tenant-Reaktionen gehen unverändert durch.
- [ ] Tresor-Datei (ADR mcp-quiver 0001): optional Hauptpasswort (Argon2) beim Start.
