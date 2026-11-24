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
      - Die Reaktionsnamen sind auf die Wörter `like/love/laugh/surprised/sad/angry` und die
        üblichen Emojis abgebildet (Wort → Emoji, denn Graph will als `reactionType` das Unicode-Emoji);
        andere Emojis gehen unverändert durch, alles Übrige wird mit einer Meldung abgelehnt.
- [ ] **Aufgabe 06 – bewusste Abkürzungen:**
      - **Office-/PDF-Text:** `datei_lesen` kann Office-Dateien nur als PDF anfordern
        (`?format=pdf`) und **nicht** in Text wandeln; PDF und Binäres liefern nur Metadaten.
        Eine echte Textextraktion bräuchte eine zusätzliche Bibliothek (z. B. `pdf-extract`) –
        bewusst nicht eingebaut, um klein zu bleiben.
      - **Upload über 4 MB:** `datei_hochladen` lehnt größere Inhalte ab, statt eine Upload-Session
        (`createUploadSession`, mehrere `PUT`-Teile) zu fahren. Für Code- und Notizdateien reicht 4 MB.
      - **Listeneinträge ohne ETag:** `sharepoint_listeneintrag_aendern` schickt kein `If-Match`;
        bei gleichzeitigen Änderungen gewinnt der letzte Schreiber. Planner dagegen verlangt ETag
        und bekommt ihn.
      - **Planner:** kein Anlegen/Ändern von Plänen und Buckets, kein Löschen von Aufgaben
        (`DELETE /planner/tasks/{id}` bräuchte ein eigenes destruktives Tool).
      - **To Do:** keine Checklistenpunkte (`checklistItems`), keine Anhänge, keine Wiederholungen.
      - **SharePoint:** Listenspalten werden roh als `felder` ausgegeben; besondere Typen
        (Personen, Lookup, verwaltete Metadaten) werden nicht aufgelöst.
      - **Download-Grenze:** Der Client liest höchstens 8 MB Antwort in den Speicher; größere
        Textdateien werden abgeschnitten, statt gestreamt zu werden.
- [ ] Tresor-Datei (ADR mcp-quiver 0001): optional Hauptpasswort (Argon2) beim Start.
