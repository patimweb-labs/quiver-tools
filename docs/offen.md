# Offene Punkte

- [ ] Outline: `insecureTls` der Vorlage (selbstsignierte Zertifikate) fehlt – für selbst gehostete Instanzen ggf. als Schalter-Feld nachrüsten.
- [ ] Parameternamen gemischt (deutsch/englisch, z. B. `collection_id`, `max_zeichen`); einheitlich festlegen.
- [ ] Kein `tracing-subscriber` im Kern – `tracing`-Ereignisse sind stumm.
- [ ] Mehrere Zugänge je Tool: heute genau ein Zugang `standard`. Ein zweites Wiki oder Postfach braucht einen zweiten Bibliothekseintrag.
- [ ] „Testen“-Knopf in quiver: `Werkzeug::testen` und `quiver-<tool> testen` sind da, quiver ruft sie noch nicht auf.
- [ ] `README.de.md` fehlt.
- [ ] E-Mail: PDF-Text aus Anhängen (Vorlage `pdf-reader.ts`) fehlt vorerst; `email_read` nennt Anhänge nur.
- [ ] E-Mail: `email_send` kennt weder HTML-Rumpf (`html`) noch lokale Anhänge (`attachmentPaths`) der Vorlage.
- [ ] E-Mail: `email_read` lädt keine Anhänge herunter (`downloadDir` der Vorlage fehlt).
- [ ] E-Mail: IMAP prüft gegen `webpki-roots`, nicht gegen den System-Speicher. Für Server mit Firmen-CA `rustls-native-certs` nachziehen (ADR 0003).
- [ ] E-Mail: Gmail wird über die Domain (`gmail.com`, `googlemail.com`) erkannt, damit keine zweite Kopie in „Gesendet“ landet. Andere Anbieter, die selbst ablegen, bekommen die Kopie doppelt.
- [ ] E-Mail: STARTTLS (IMAP) ist eigener Code und nur über den ignorierten `imap_echt`-Test gegen einen echten Server geprüft.
- [ ] E-Mail: mehrere Nachrichten je Aufruf (UID-Bereiche, Massen-Flaggen) gibt es nicht; die Vorlage auch nicht.
