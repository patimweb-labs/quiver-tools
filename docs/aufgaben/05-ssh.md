# 05 – quiver-ssh (SSH/SFTP)

Crate: `crates/ssh`, Programm `quiver-ssh`. Vorlage: `~/Documents/Workspaces/pi-ssh` (`src/`, `README.md`).
Regeln wie bei Outline/E-Mail (Zugang über Felder aus der Umgebung, kein `ssh_setup`).

## Architekten-Entscheidung: Hosts aus `~/.ssh/config`

SSH hat meist viele Hosts; ein quiver-Eintrag je Host wäre lästig. Darum:

1. **Hosts aus `~/.ssh/config`** (`Host`, `HostName`, `User`, `Port`, `IdentityFile`, `ProxyJump` nur lesen und
   melden, nicht unterstützen) – Anmeldung per **ssh-agent** (`SSH_AUTH_SOCK`) oder `IdentityFile`
   (unverschlüsselte Schlüssel; verschlüsselte nur über den Agenten). Kein Passwort nötig, nichts in quiver.
2. **Optional ein Passwort-Host über Felder** (`SSH_HOST`, `SSH_PORT` 22, `SSH_BENUTZER`, `SSH_PASSWORT` geheim,
   `SSH_NAME` Standard `passwort-host`). Erscheint wie ein weiterer Host.
3. Jedes Tool nimmt `host` (Name aus der Config oder `SSH_NAME`). Ohne `host` und mit genau einem Host → der.
4. **Host-Schlüssel:** immer gegen `~/.ssh/known_hosts` prüfen (gehashte Einträge unterstützen). Unbekannt →
   Fehlertext „Host-Schlüssel von <host> ist unbekannt (Fingerabdruck SHA256:…). Bitte einmal im Terminal
   `ssh <host>` aufrufen und bestätigen.“ Geändert → harter Fehler, **nie** automatisch akzeptieren.
   Der Agent kann keine Host-Schlüssel annehmen.
5. `quiver-ssh testen` / `Werkzeug::testen`: listet die Hosts aus der Config und ob der Agent Schlüssel hat.

## Abhängigkeiten

`russh` (Client) + `russh-sftp`, `ssh-key` (known_hosts, Schlüssel), `ssh2-config` **oder** eigener kleiner
Parser für die nötigen Schlüsselwörter (begründen). Nur reines Rust, kein libssh/OpenSSL.

## Tools

| Tool | Hinweis |
|---|---|
| `ssh_status` | lesend: Hosts (Name, Adresse, Benutzer, Anmeldeart, ob in known_hosts) – ohne Geheimnisse |
| `ssh_list` | lesend: Verzeichnis über SFTP (Name, Größe, Rechte, Datum) |
| `ssh_download` | `destructive_hint` (schreibt lokal), Zielpfad Pflicht |
| `ssh_exec` | `destructive_hint`; Befehl, Zeitlimit (Standard 60 s, höchstens 600 s), stdout/stderr je `max_zeichen` (Standard 8000), Exit-Code |
| `ssh_upload` | `destructive_hint` |
| `ssh_keygen` | `destructive_hint`; ed25519 in `~/.ssh/` (Name wählbar, nie überschreiben), gibt Pfad + öffentlichen Schlüssel aus |
| `ssh_authorize` | `destructive_hint`; nur für den Passwort-Host: öffentlichen Schlüssel in `authorized_keys` eintragen, mit Schlüssel neu anmelden und prüfen; danach Hinweis, den Host in `~/.ssh/config` einzutragen und das Passwort im quiver-Fenster zu löschen (das Tool ändert die quiver-Felder **nicht**) |

`ssh_doctor` und `ssh_tunnel` vorerst **nicht** (Tunnel = langlebige Prozesse, eigene Aufgabe) – in `docs/offen.md`.

## Tests

- Config-Parser und known_hosts (gehasht/klar, unbekannt, geändert) als Unit-Tests.
- **Test-SSH-Server mit russh** im Test (Passwort- und Schlüssel-Anmeldung, `exec`, SFTP-Subsystem mit temp-Ordner)
  – kein echter Host. `HOME` bzw. Config-/known_hosts-Pfade über Umgebung (`QUIVER_SSH_CONFIG`,
  `QUIVER_SSH_KNOWN_HOSTS`) auf temp-Dateien umbiegen.
- Ende-zu-Ende über stdio wie bei Outline.

## Doku

README-Abschnitt quiver-ssh (Hosts aus der Config, Passwort-Host, Host-Schlüssel), `docs/architektur.md`,
`docs/adr/0003-ssh-hosts-aus-ssh-config.md`. Prüfbefehl grün.
