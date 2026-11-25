# 05 – quiver-ssh (SSH/SFTP) – Fassung 2

> Ersetzt die erste Fassung (Hosts aus `~/.ssh/config`). **Entscheidung des Nutzers (2026-11-23).**

Crate: `crates/ssh`, Programm `quiver-ssh`. Vorlage: `~/Documents/Workspaces/pi-ssh` (`src/`, `README.md`).

## Verbindungen: vom User im quiver-Fenster gepflegt

quiver kann mehrere Zugänge je Tool (`Eintrag.mehrfach`, siehe
`~/Documents/Workspace/mcp-quiver/crates/domain/src/lib.rs`: `Mehrfach`, `Zugang`, `MEHRFACH_UMGEBUNG`).
Der Client übergibt alle Verbindungen als JSON in `QUIVER_ZUGAENGE`:
`[{"name":"web-01","werte":{"host":…,"port":…,"benutzer":…,"passwort":…}}]`.

Ein Passwort gilt für beides: Die erste Anmeldung und Befehle mit `sudo` (Nachtrag 2026-11-25, siehe ADR
0003). Frühere Einträge mit `anmelde_passwort` und `sudo_passwort` werden unverändert weiter gelesen.

### Kern erweitern (additiv, neues Modul `zugaenge.rs`)

- `Werkzeug::mehrfach() -> Option<(&'static str, &'static str)>` (Titel, Einzahl), Standard `None`.
  Ist es gesetzt, gibt `eintrag` die Felder unter `mehrfach: { titel, einzahl, felder }` aus (nicht als `felder`
  des Eintrags; der Eintrag hat dann keine eigenen Felder und keine `umgebung`).
- `zugaenge() -> Vec<Zugang>` liest `QUIVER_ZUGAENGE` (fehlt → leer; kaputtes JSON → Fehler ohne Inhalt).
- `Debug` von `Zugang` zeigt Geheimes nie.

### Felder einer Verbindung

| Feld | Art | Hilfe |
|---|---|---|
| `benutzer` | Text, Pflicht | Anmeldename |
| `host` | Text, Pflicht | Hostname oder IP |
| `port` | Zahl | Standard 22 |
| `passwort` | Passwort | Für die Anmeldung und für sudo. Bleibt verschlüsselt auf diesem Gerät. |
| `schreiben` | Schalter, Standard aus | Erlaubt Befehle und Dateitransfer. |
| `austauschordner` | Ordner, optional | Nur darin werden Dateien hoch- und heruntergeladen. |

**Altbestand:** Gibt es im Eintrag noch ein `sudo_passwort`, wird es weiter benutzt. Sonst gilt das Passwort aus
`passwort` (bzw. aus dem früheren Feld `anmelde_passwort`) auch für `sudo`. Die beiden alten Felder erscheinen
nicht mehr im quiver-Fenster.

## Pflicht: nach einer Passwort-Anmeldung immer Schlüssel

Schlüssel je Verbindung: `<daten>/quiver-tools/ssh/schluessel/<name>_ed25519` (+ `.pub`), Rechte 0600/0700,
`<daten>` = `dirs::data_dir()` bzw. `QUIVER_SSH_DATEN` (Tests).

Ablauf bei jeder Verbindung:
1. **Schlüssel vorhanden** → nur Schlüssel-Anmeldung. Schlägt sie fehl → Fehler „Die Anmeldung mit dem Schlüssel an
   <name> wurde abgelehnt. Mit `ssh_schluessel_einrichten` lässt sich der Schlüssel neu einrichten – dafür wird das
   Passwort gebraucht.“ **Kein** stiller Rückfall aufs Passwort.
2. **Kein Schlüssel, aber Passwort** → Passwort-Anmeldung, dann sofort: ed25519-Schlüssel erzeugen,
   öffentlichen Teil in `~/.ssh/authorized_keys` des Benutzers eintragen (`~/.ssh` 700, Datei 600 anlegen, doppelten
   Eintrag vermeiden, Kommentar `quiver-ssh <name>`), **neue Verbindung nur mit Schlüssel** öffnen und prüfen,
   erst dann den Schlüssel als eingerichtet speichern. Klappt einer dieser Schritte nicht → Aufruf **abbrechen** mit
   klarer Meldung (z. B. wenn der Server die Schlüssel-Anmeldung abgeschaltet hat), Schlüsseldateien wieder
   entfernen. Die eigentliche Arbeit läuft erst danach, über die Schlüssel-Verbindung. Die Antwort sagt einmalig:
   „Schlüssel für <name> eingerichtet (SHA256:…). Die Anmeldung läuft ab jetzt über den Schlüssel. Das Passwort
   bleibt gespeichert, weil Befehle mit sudo es weiterhin brauchen; ohne sudo kannst du es im quiver-Fenster
   löschen.“
3. **Weder noch** → Hinweis „Für <name> fehlt das Passwort für die erste Anmeldung – bitte im quiver-Fenster unter
   Tools → SSH beim Eintrag „<name>“ das Feld „Passwort“ ausfüllen.“
4. `ssh_schluessel_einrichten` (destruktiv): erzwingt Schritt 2 mit dem Passwort (z. B. nach Server-Neuaufbau),
   ersetzt den alten Schlüssel erst nach erfolgreicher Prüfung.

## Host-Schlüssel

Eigene Datei `<daten>/quiver-tools/ssh/known_hosts`; zusätzlich `~/.ssh/known_hosts` lesen (gehashte Einträge).
Erste Verbindung zu einem Host: Schlüssel merken (TOFU) und in der Antwort den Fingerabdruck nennen.
**Geänderter** Host-Schlüssel → harter Fehler mit altem und neuem Fingerabdruck, nie automatisch überschreiben;
die Antwort nennt, welche Zeile in welcher Datei der User selbst löschen muss.

## sudo

`ssh_exec` mit `sudo: true`: Befehl als `sudo -S -p '' -- sh -c '<befehl>'` (sauber quotiert) ausführen, das
Passwort + Zeilenende über stdin schreiben. Nie in Befehlszeile, Ausgabe, Fehlertext oder Log. Fehlt es →
Hinweis „Für <name> ist kein Passwort hinterlegt – Befehle mit sudo brauchen eins.“ Falsches Passwort → klare
Meldung (sudo-Ausgabe auswerten), ohne das Passwort. Gilt ein eigenes `sudo_passwort` aus dem Altbestand, wird
nur dieses benutzt – kein stiller Rückfall aufs Anmelde-Passwort.

## Tools

| Tool | Hinweis |
|---|---|
| `ssh_status` | lesend: Verbindungen (Name, `benutzer@host:port`, Schlüssel eingerichtet ja/nein + Fingerabdruck, Passwort für sudo vorhanden ja/nein, Host-Schlüssel bekannt ja/nein) |
| `ssh_list` | lesend: Verzeichnis über SFTP |
| `ssh_exec` | destruktiv: `verbindung`, `befehl`, `sudo` (bool), `zeitlimit` (Standard 60 s, max 600), `max_zeichen` (8000) je stdout/stderr, Exit-Code |
| `ssh_upload`, `ssh_download` | destruktiv |
| `ssh_schluessel_einrichten` | destruktiv, siehe oben |

Jedes Tool nimmt `verbindung` (Name); bei genau einer Verbindung optional. Tunnel und `ssh_doctor` später
(`docs/offen.md`).

## Abhängigkeiten

`russh` + `russh-sftp`, `ssh-key` (Schlüssel, known_hosts), `dirs`. Nur reines Rust. Begründe Abweichungen.

## Tests

- **Test-SSH-Server mit russh** (Passwort und Schlüssel, `exec`, SFTP auf temp-Ordner, simuliertes
  `authorized_keys`, simuliertes `sudo -S`), kein echter Host. Pfade über `QUIVER_SSH_DATEN`, `HOME` auf temp.
- Fälle: erste Anmeldung mit Passwort → Schlüssel eingerichtet, zweite Verbindung **ohne** Passwort klappt;
  Server lehnt Schlüssel ab → Abbruch, keine Schlüsseldatei übrig; Schlüssel vorhanden, Server kennt ihn nicht
  mehr → Fehler ohne Passwort-Rückfall; geänderter Host-Schlüssel → Fehler; sudo mit richtigem/falschem/fehlendem
  Passwort aus dem einen Feld „Passwort“ (das Passwort taucht in keiner Ausgabe und in keinem Befehl auf);
  Altbestand mit eigenem `sudo_passwort` (wird benutzt, kein Rückfall aufs Anmelde-Passwort); `QUIVER_ZUGAENGE`
  leer → Hinweis.
- Ende-zu-Ende über stdio; `quiver-ssh eintrag` enthält `mehrfach` mit den sechs Feldern.

## Doku

README-Abschnitt quiver-ssh, `docs/architektur.md`, `docs/adr/0003-ssh-schluessel-pflicht.md`. Prüfbefehl grün.
