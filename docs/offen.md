# Offene Punkte

- [ ] Outline: `insecureTls` der Vorlage (selbstsignierte Zertifikate) fehlt – für selbst gehostete Instanzen ggf. als Schalter-Feld nachrüsten.
- [ ] Parameternamen gemischt (deutsch/englisch, z. B. `collection_id`, `max_zeichen`); einheitlich festlegen.
- [ ] Kein `tracing-subscriber` im Kern – `tracing`-Ereignisse sind stumm.
- [ ] Mehrere Zugänge je Tool: Outline, E-Mail und ABAP haben weiterhin genau einen Zugang `standard`. Nur SSH nutzt `mehrfach` (`QUIVER_ZUGAENGE`). Ein zweites Wiki braucht einen zweiten Bibliothekseintrag.
- [ ] „Testen“-Knopf in quiver: `Werkzeug::testen` und `quiver-<tool> testen` sind da, quiver ruft sie noch nicht auf.
- [ ] `README.de.md` fehlt.
- [ ] SSH: Tunnel (`ssh_tunnel`) fehlen – die Vorlage `pi-ssh` hat lokale und entfernte Portweiterleitungen, Aufgabe 05 nicht.
- [ ] SSH: `ssh_doctor` (Diagnose: Host erreichbar, Schlüssel akzeptiert, SFTP verfügbar) fehlt.
- [ ] SSH: Wird ein Schlüssel ersetzt, bleibt der alte Eintrag in der entfernten `authorized_keys` stehen; ein Aufräumschritt (Kommentar `quiver-ssh <name>` entfernen) fehlt.
- [ ] SSH: Der Schlüssel hat keine Passphrase; er liegt mit 0600 im Datenordner. Eine Passphrase bräuchte einen Schlüsselbund-Zugriff im Tool.
- [ ] SSH: Nur ed25519 – kein RSA, keine Zertifikate, kein Agent, keine Sprung-Hosts (`ProxyJump`).
- [ ] SSH: `ssh_list` zeigt keine Änderungszeiten (die SFTP-Attrappe und das Format lassen sie aus); die Vorlage zeigt das Datum.
