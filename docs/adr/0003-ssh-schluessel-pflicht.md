# 0003 – SSH: Verbindungen im quiver-Fenster, Schlüssel statt Passwort

> Stand: 2026-11-23, Nachtrag 2026-11-25 (ein Passwort für Anmeldung und sudo).
> Aufgabe: [docs/aufgaben/05-ssh.md](../../docs/aufgaben/05-ssh.md) (Fassung 2).

## Kontext

Die erste Fassung der Aufgabe wollte die Hosts aus `~/.ssh/config` und Schlüssel in
`~/.ssh` ablegen. Beides gehört dem User und wird von `ssh` selbst gepflegt; ein Tool,
das dort schreibt, tritt neben OpenSSH und nicht dahinter. Der Nutzer hat darum
entschieden: **die Verbindungen pflegt der User im quiver-Fenster**, so wie die Felder
jedes anderen Tools – mehrere davon als Liste, weil ein SSH-Tool ohne mehrere Hosts
nutzlos ist.

Ein Passwort in einer Konfiguration ist ein Passwort auf der Platte. Es bleibt dort, so
lange der Eintrag existiert, und es wird bei jeder Verbindung wieder übertragen. Für
SSH gibt es dafür seit Jahrzehnten die bessere Antwort: einen Schlüssel.

## Entscheidung

**Mehrere Zugänge je Eintrag.** `Werkzeug::mehrfach()` liefert `Some((Titel, Einzahl))`;
`eintrag` schreibt die Felder dann unter `mehrfach` und der Eintrag hat weder eigene
Felder noch eine `umgebung`. quiver übergibt alle Verbindungen als JSON in
`QUIVER_ZUGAENGE`; der Kern liest sie in `zugaenge` (neu, additiv). Fehlt die Variable,
gibt es keine Verbindungen; kaputtes JSON ist ein Fehler **ohne Inhalt**, weil die
Variable Geheimnisse enthalten kann.

**Sechs Felder je Verbindung:** `benutzer`, `host`, `port` (Standard 22), `anmelde_passwort`,
`schreiben`, `austauschordner`. Das Passwort gilt für die Anmeldung **und** für `sudo`
(Nachtrag unten) und liegt im Schlüsselbund von quiver.

**Das Passwort dient der ersten Anmeldung und sudo.** Jede Verbindung läuft durch
`anmeldung::bereit`:

1. Schlüssel vorhanden → nur Schlüssel-Anmeldung. Scheitert sie, ist das ein Fehler mit
   Verweis auf `ssh_schluessel_einrichten` – **kein** stiller Rückfall aufs Passwort.
2. Kein Schlüssel, aber Passwort → Passwort-Anmeldung, ed25519-Schlüssel
   erzeugen, öffentlichen Teil in `~/.ssh/authorized_keys` eintragen (700/600, kein
   doppelter Eintrag, Kommentar `quiver-ssh <name>`), **neue Verbindung nur mit dem
   Schlüssel** prüfen, erst dann den Schlüssel übernehmen. Scheitert ein Schritt, wird
   abgebrochen und aufgeräumt.
3. Weder noch → Hinweis, das Passwort im quiver-Fenster zu hinterlegen.

`ssh_schluessel_einrichten` erzwingt Schritt 2 und ersetzt den alten Schlüssel erst,
nachdem eine Schlüssel-Verbindung geklappt hat.

**Eigene Dateien.** Schlüssel und `known_hosts` liegen unter
`<daten>/quiver-tools/ssh/` (`<daten>` = `QUIVER_SSH_DATEN` oder `dirs::data_dir()`),
nicht in `~/.ssh`. Gelesen wird zusätzlich `~/.ssh/known_hosts` (auch gehashte
Einträge), geschrieben nie. Erste Verbindung: TOFU mit Nennung des Fingerabdrucks.
Geänderter Host-Schlüssel: harter Fehler mit altem und neuem Fingerabdruck, Datei und
Zeile – die Entscheidung trifft der User.

**sudo ohne Leck.** `ssh_exec` mit `sudo: true` startet `sudo -S -p '' -- sh -c
'<befehl>'` und schreibt das Passwort über stdin. Fehlt es, kommt ein Hinweis; ist es
falsch, wird die sudo-Ausgabe ausgewertet und das gemeldet. Das Passwort steht nie in
Befehlszeile, Ausgabe, Fehlertext oder Log.

## Warum so

1. **Zugangsdaten bleiben dort, wo der User sie einträgt.** quiver zeigt die Felder,
   speichert Geheimes im Schlüsselbund und setzt die Umgebung. Das Tool kennt weder
   quiver noch einen Schlüsselbund.
2. **Das Passwort wird nur noch für sudo gebraucht.** Nach der ersten Anmeldung beweist
   der Ablauf die Schlüssel-Anmeldung, bevor er den Schlüssel behält. Wer kein sudo
   braucht, kann das Passwort danach löschen; wer sudo braucht, behält es im
   Schlüsselbund.
3. **Kein stiller Rückfall.** Ein Rückfall aufs Passwort würde einen kaputten Schlüssel
   verstecken und das Passwort dauerhaft aktuell halten. Ein Fehler mit Anleitung ist
   ehrlicher.
4. **Neben OpenSSH, nicht darin.** Eigene Dateien unter dem Datenordner kollidieren
   nicht mit `~/.ssh/config`, `authorized_keys` oder einem `ssh`-Aufruf des Users; die
   vorhandene `known_hosts` wird trotzdem gelesen, damit bereits besuchte Hosts bekannt
   sind.
5. **`ssh_schluessel_einrichten` ist ein Werkzeug, kein Setup-Tool.** Es ändert etwas am
   Server (destruktiv) und braucht das Passwort – darum ist es ein normaler, ausdrücklich
   aufgerufener Schritt, kein `*_setup`, das beim Start läuft.

## Nachtrag 2026-11-25: ein Passwort statt zwei

**Entscheidung (Nutzer):** Anmelde- und sudo-Passwort werden **ein** Feld „Passwort“.
Der gespeicherte Feldname bleibt `anmelde_passwort`, damit vorhandene Passwörter weiter gelten (quiver
übergibt nur Felder, die der Eintrag noch kennt). Hilfe im quiver-Fenster: „Für die Anmeldung und für sudo. Bleibt
verschlüsselt auf diesem Gerät.“

*Begründung:* In der Praxis ist es fast immer dasselbe Passwort – zwei Felder mit
demselben Wert sind eine Fehlerquelle, und ein leeres sudo-Feld führte zu „sudo-Passwort
fehlt“, obwohl das Passwort direkt daneben stand.

**Altbestand bleibt gültig.** Der Leser nimmt in dieser Reihenfolge:

1. `anmelde_passwort` – gilt für Anmeldung und sudo.
2. `sudo_passwort` (früher) – hat bei sudo Vorrang, falls es noch übergeben wird. Das Feld
   erscheint nicht mehr im quiver-Fenster.

`Debug` zeigt von `Verbindung` weiterhin nur, *ob* ein Passwort gesetzt ist. Das Passwort
geht ausschließlich über stdin an `sudo`, steht nie in Argumenten, Logs, Antworten oder
Fehlern, und eine abgelehnte Schlüssel-Anmeldung fällt **nicht** still auf das Passwort
zurück.

*Preis:* Wer für Anmeldung und sudo wirklich verschiedene Passwörter braucht, kann das
nur noch über einen alten Eintrag mit `sudo_passwort` abbilden. Das ist bewusst: die
Regel „ein Passwort“ ist der Normalfall, der Sonderfall bleibt lesbar, wird aber nicht
mehr angeboten.

## Abweichungen bei den Abhängigkeiten

- **`ssh-key` in der Version, die `russh` benutzt** (`=0.7.0-rc.12`). Eine eigene,
  neuere Version ergäbe zwei inkompatible `PublicKey`-Typen. Die Semantik ist die
  Bibliothek von RustCrypto; sie liefert Schlüsselerzeugung, OpenSSH-Format und
  SHA256-Fingerabdrücke.
- **`rand` zusätzlich.** `ssh-key` erzeugt Schlüssel nur mit einem übergebenen
  Zufallsgenerator und bringt keinen mit. Statt eine eigene RNG-Anbindung zu schreiben,
  kommt `rand` (dieselbe `rand_core`-Version wie `ssh-key`) dazu.
- **`russh`/`russh-sftp`.** Reines Rust, Client und Server; derselbe `russh`-Server
  dient als Test-Attrappe, damit kein Test je einen echten Host braucht. `ssh2`
  (die Vorlage) bindet libssh2 in C und wird bewusst nicht benutzt.

## Preis

- **Ein Schlüssel je Verbindung, ohne Passphrase.** Er liegt mit 0600 im Datenordner;
  wer Zugriff auf das Konto hat, hat auch den Schlüssel. Eine Passphrase würde einen
  weiteren Speicherort (Schlüsselbund) im Tool verlangen – dafür gibt es ihn nicht.
- **Der Schlüssel wird beim Ersetzen lokal neu erzeugt**, der alte Eintrag bleibt in der
  entfernten `authorized_keys` stehen. Aufräumen wäre ein weiterer destruktiver Schritt;
  bis dahin sammelt ein Host einen Eintrag je Neueinrichtung.
- **Nur ed25519.** Kein RSA, keine Zertifikate, kein Agent, keine Sprung-Hosts. Das deckt
  die üblichen Linux-Server ab und hält den Ablauf klein.
- **Kein Tunnel, kein `ssh_doctor`.** Beides steht in [offen.md](../offen.md); die
  Vorlage hat es, die Aufgabe (noch) nicht.
- **`testen` verbindet wirklich.** `quiver-ssh testen` öffnet jede eingetragene
  Verbindung – das ist die einzige ehrliche Prüfung, kostet aber einen Verbindungsaufbau
  je Host.
