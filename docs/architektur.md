# Architektur

> Stand: 2026-11-23, nach Aufgabe 07 und 08. Gilt für den Kern (`crates/kern`) und alle
> Tools, die darauf aufsetzen. Festlegungen und Preise: [../SPEC.md](../SPEC.md).

## Richtung

```
crates/kern        Werkzeug-Trait, Zugang aus der Umgebung, Bibliothekseintrag, MCP-Hilfen, Gerüst
                   net (ein HTTP-Client, Adress- und Zertifikatsregel), schutz (Schreibschutz),
                   ablage (Dateien nur im Arbeitsordner)
crates/<tool>      ein Programm quiver-<tool> je Tool
assets/            tokens.css und Schriften (Kopie aus mcp-quiver, nicht von Hand ändern)
```

`kern ← <tool>`: ein Tool kennt den Kern, der Kern kennt kein Tool. Alles, was ein
Tool dem Kern über sich sagt, steht in `Werkzeug`: Name, Titel, Beschreibung, Felder,
`testen`. Eine Ausnahme gibt es nur im Test: `crates/kern/tests/alle_programme.rs`
bindet die Werkzeug-Crates als **Dev**-Abhängigkeit ein, um den Schreibschutz über alle
Programme zu prüfen (siehe [adr/0014-haertung-paket-4.md](adr/0014-haertung-paket-4.md)).

Die Oberfläche ist **quiver**. Dieses Repo liefert keine Seite und keinen Browser-Code
mehr; `assets/` bleibt als Vorlage für quiver und wird hier von keinem Programm
eingebunden.

## Der Kern in Schichten

| Modul | Aufgabe | Zustand |
|---|---|---|
| `umgebung` | `Zugang`: Felder aus `<NAME>_<FELD>` lesen, Pflichtfelder prüfen, `Profil` bauen | Umgebung des Prozesses |
| `profil` | `Profil { name, werte, geheim }`, `ProfilFehler`; `Debug` zeigt Geheimnisse nie | keiner |
| `eintrag` | `quiver_domain::Eintrag` als JSON: id, version, titel, beschreibung, laufzeit, felder | keiner |
| `net` | ein HTTP-Client (keine Weiterleitung, Frist, 8 MB), Adress- und Zertifikatsregel | keiner |
| `schutz` | Schreibschutz aus der Werkzeugliste: was nicht `read_only_hint` ist, braucht `schreiben` | keiner |
| `ablage` | Dateien nur im Arbeitsordner: relative Namen, keine Symlinks, kein Überschreiben | Dateisystem |
| `mcp` | `zugang`, `profile_tool`, `profile_antwort`, `server_info`, `kuerzen` | keiner |
| `start` | `starten`, `aufruf_aus_args`, `Aufruf` (MCP, eintrag, testen, version, hilfe) | Prozess |
| `zugaenge` | `Zugang { name, werte }`, `zugaenge()`, `mit_umgebung()` aus `QUIVER_ZUGAENGE` | Umgebungsvariable |

Die Schnittstelle (`lib.rs`) legt der Architekt fest; die Umsetzung ergänzt sie nur.

## Datenfluss des Zugangs

```
quiver-Fenster (Tools → <Titel>)
  → Bibliothekseintrag (quiver-outline eintrag)      Felder, quelle client/geheim
  → User trägt Werte ein                             offen: Client-Konfiguration, geheim: Tresor-Datei
  → quiver startet quiver-<tool> mit Umgebung        OUTLINE_URL, OUTLINE_API_KEY, …
  → umgebung::Zugang::neu(&Werkzeug)                 liest <NAME>_<FELD>, leere Werte fehlen
  → Zugang::profil()                                 alle Pflichtfelder da? → Profil `standard`
  → mcp::zugang                                      fehlt etwas → Hinweistext, kein MCP-Fehler
  → client::aus_profil                               Adresse + Schlüssel aus dem Profil
```

Was der Agent sieht, folgt aus der Umgebung: es gibt keinen zweiten Zustand, keine
Datei, keinen Tresor im Tool.

Regeln, die im Code als Kommentar stehen und hier gelten:

- **Nur die Umgebung zählt.** Ein Feldname `api_key` wird zur Variablen `OUTLINE_API_KEY`
  (`umgebung::schluessel`). Kein Konfigurationsordner, keine Tresor-Datei im Tool.
- **Leer ist nicht gesetzt.** `Zugang` verwirft Werte, die nur aus Leerraum bestehen.
- **Geheimes bleibt geheim.** `Profil::geheim` wird nie in eine Antwort geschrieben;
  `<tool>_profile` zeigt nur `Profil::werte`. Fehlertexte nennen höchstens Feldnamen.
- **Ein Zugang.** Es gibt kein `profil`-Feld in der Umgebung und keinen `profil`-Parameter
  an den Werkzeugen. Mehrere Zugänge wären mehrere Bibliothekseinträge.
- **Ausnahme `mehrfach`.** Ein Werkzeug wie `quiver-ssh` pflegt statt eines Zugangs eine
  Liste benannter Verbindungen. Dann liefert `Werkzeug::mehrfach()` `Some((Titel,
  Einzahl))`, der Eintrag trägt die Felder unter `mehrfach` (und hat selbst keine
  `felder` und keine `umgebung`), und quiver übergibt alle Zugänge als JSON in
  `QUIVER_ZUGAENGE`. `zugaenge::zugaenge()` liest sie; kaputtes JSON ist ein Fehler
  **ohne Inhalt** (die Variable kann Geheimnisse enthalten).

## Der Bibliothekseintrag

`eintrag::eintrag(&Werkzeug, befehl)` erzeugt die Struktur, `quiver-<tool> eintrag`
schreibt sie als JSON auf stdout. `befehl` kommt aus `--befehl <pfad>`, sonst aus
`std::env::current_exe()`, sonst `quiver-<name>`.

```
laufzeit = { art: "stdio", befehl: <pfad>, umgebung: { "<NAME>_<FELD>": "${<feld>}" } }
felder   = [ { name, titel, beschreibung: hilfe, art, quelle: client|geheim, pflicht, standard } ]
```

`titel` ist die Anzeige, `beschreibung` nur die Hilfe. `art` folgt `quiver_domain::FeldArt`:
`text`, `passwort`, `zahl`, `schalter`, `url` oder
`{ auswahl: { optionen: [ { wert, anzeige } ] } }`. Auch `mehrfach.felder` trägt beide Felder.

Kein `einrichtung`-Feld: es gibt keine Seite, die quiver öffnen könnte. `aktiv` und
`argumente` fehlen absichtlich; beides hat in `quiver_domain` einen Standardwert.

Hat das Werkzeug `mehrfach` gesetzt, steht statt der Felder eine Liste im Eintrag:

```
felder  = []
laufzeit.umgebung = {}
mehrfach = { titel: "Verbindungen", einzahl: "Verbindung", felder: [ … ] }
```

## MCP

Jedes Werkzeug antwortet ohne Zugang mit dem Text aus `hinweis_zugang` – **kein**
MCP-Fehler. `mcp::zugang` bildet genau das ab. `mcp::profile_antwort` liefert den
Bericht für `<werkzeug>_profile` (Name `standard`, nicht-geheime Werte) oder denselben
Hinweis. Lange Texte über `mcp::kuerzen` an Wortgrenzen kürzen.

## Das Programm-Gerüst

`starten(werkzeug, |zugang| server)` wertet `std::env::args()` aus:

| Aufruf | Wirkung |
|---|---|
| ohne Argument | MCP über stdio (so startet quiver das Tool) |
| `eintrag [--befehl <pfad>]` | Bibliothekseintrag als JSON auf stdout |
| `testen` | Zugang aus der Umgebung prüfen, Meldung, Exit 0/1 |
| `--version` | Version auf stdout |
| `--help` | kurze Hilfe auf stdout |
| sonst | Meldung auf stderr, Exit 2 |

Begründung in [adr/0001-programm-geruest.md](adr/0001-programm-geruest.md) und
[adr/0002-zugang-ueber-quiver-felder.md](adr/0002-zugang-ueber-quiver-felder.md).
Logs und Meldungen gehen immer nach **stderr**; stdout gehört dem MCP-Protokoll.

## Ein Tool: `crates/outline`

Die Vorlage `pi-outline-wiki` (TypeScript) liefert die Semantik, nicht den Code
(SPEC §6). Die Schichten sind dieselben wie im Kern, nur eine Ebene tiefer:

| Modul | Aufgabe | Zustand |
|---|---|---|
| `werkzeug` | `Outline` (Name, Titel, Beschreibung, Felder, `testen`), `wurzel` (Adresse normalisieren) | keiner |
| `client` | der **einzige** Ort mit Netzverkehr: `POST <wurzel>/api/<methode>`, Bearer, 30 s | keiner |
| `domain` | was Outline liefert (`serde`), ProseMirror → Prosa | keiner |
| `format` | reine Funktionen: Daten → Text, gekürzt über `mcp::kuerzen` | keiner |
| `datei` | Titel → Dateiname, Schreiben ohne Ausbruch aus dem Zielordner | Dateien |
| `server` | die 14 MCP-Werkzeuge, Parameter als `schemars`-Typen | Zugang aus der Umgebung |

Datenfluss eines Aufrufs:

```
MCP tools/call
  → server::aufrufen        Name → Werkzeug, Argumente → Parametertyp
  → mcp::zugang             Zugang aus der Umgebung (fehlt er: Hinweistext, kein Fehler)
  → client::aufrufen        POST /api/<methode>, Fehler → OutlineFehler ohne Schlüssel
  → domain                  Antwort in Datenform
  → format                  Daten → knapper Text (max_zeichen, Seitenhinweis)
  → CallToolResult
```

Regeln, die dort gelten:

- **Kein Geheimnis im Text.** `client::aufrufen` ersetzt den Schlüssel in jedem
  Fehlertext durch `[geheim]`, auch wenn ein Server ihn zurückspiegelt.
- **`outline_profile` kommt aus dem Kern** (`mcp::profile_tool` /
  `mcp::profile_antwort`): bei jedem Tool dasselbe lesende Werkzeug.
- **Eine Suche, ein Wiki.** „Über alle Workspaces“ der Vorlage gibt es nicht mehr –
  dafür bräuchte es mehrere Zugänge.
- **Schreibende Werkzeuge** tragen `destructive_hint`, lesende `read_only_hint`
  (AGENTS.md §8).
- **Parameternamen sind deutsch** (`collection_id`, `max_zeichen`, `save_dir`); die
  **Werkzeugnamen** bleiben die der Vorlage, damit Skills und Gewohnheiten weiter passen.

## Ein zweites Tool: `crates/email`

Die Vorlage `pi-email` (TypeScript) liefert die Semantik, nicht den Code (SPEC §6).
Der Microsoft-/Graph-Teil der Vorlage fehlt: den macht `quiver-m365`. Hier gibt es nur
IMAP und SMTP.

| Modul | Aufgabe | Zustand |
|---|---|---|
| `werkzeug` | `Email` (Name, Titel, Felder, `testen`), `konto` (Felder → Vorgaben) | keiner |
| `domain` | Daten und reine Regeln: Adressen, Empfänger einer Antwort, `References`, Zitat, Suche | keiner |
| `fehler` | `MailFehler`; jeder Text verliert das Passwort | keiner |
| `postfach` | Naht `Postfach` und die echte Umsetzung mit `async-imap` + `tokio-rustls` | Verbindung je Aufruf |
| `versand` | Naht `Versand` und die echte Umsetzung mit `lettre` (rustls) | Verbindung je Aufruf |
| `format` | reine Funktionen: Daten → knapper Text, gekürzt über `mcp::kuerzen` | keiner |
| `server` | die zwölf MCP-Werkzeuge, Parameter als `schemars`-Typen | Zugang aus der Umgebung |

Datenfluss eines Aufrufs:

```
MCP tools/call
  → server::aufrufen        Name → Werkzeug, Argumente → Parametertyp
  → mcp::zugang             Zugang aus der Umgebung (fehlt er: Hinweistext, kein Fehler)
  → werkzeug::konto         Felder + Vorgaben (Ports, TLS, Zertifikat) → Konto
  → server::Fabrik          Echt → ImapPostfach / LettreVersand, Tests → Attrappe
  → Postfach / Versand      Naht: async-imap bzw. lettre, Fehler → MailFehler
  → format                  Daten → knapper Text
  → CallToolResult
```

Zwei Nähte statt eines Clients, weil zwei Protokolle im Spiel sind:

- **`Postfach`** (IMAP): Postfächer, Kopfzeilen, Suche, Nachricht, Flaggen,
  Verschieben, Löschen, APPEND, `\Sent` finden, `prüfen`.
- **`Versand`** (SMTP): senden, `prüfen`. Die Antwort trägt die fertigen Bytes, damit
  dieselbe Nachricht in den Ordner „Gesendet“ geht.

Regeln, die dort gelten:

- **Kein Geheimnis im Text.** `postfach` und `versand` ersetzen das Passwort in jedem
  Fehlertext durch `[geheim]`; `MailFehler::ohne_geheimnis` ist das zweite Netz.
  `Debug` von `Konto`, `ImapKonfiguration`, `SmtpKonfiguration` und `LettreVersand`
  zeigt `[geheim]`.
- **Zertifikat.** rustls prüft gegen `webpki-roots`. `zertifikat_pruefen = aus` gilt
  nur für `127.0.0.1`/`localhost`; jeder andere Host behält die Prüfung (mit Warnung).
- **Vorgaben.** `imap_port` 993 / `smtp_port` 587, `imap_sicherheit` `tls`,
  `smtp_sicherheit` `starttls`, `benutzer` = `adresse`. `limit` 20, höchstens 100;
  `email_read` kürzt nach `max_zeichen` (Standard 6000, `0` = ungekürzt).
- **Text statt HTML.** `mail-parser` liefert `text_bodies`; ein HTML-only-Teil wird
  dabei in Text umgewandelt. Anhänge werden nur genannt (Name, Typ, Größe).
- **Threading.** `In-Reply-To` und die ganze `References`-Kette kommen aus der
  Ursprungsnachricht; `Re:`/`Fwd:` werden nicht gestapelt.
- **Gesendet.** Nach dem Senden sucht das Tool den Ordner mit `\Sent` (RFC 6154),
  sonst einen bekannten Namen, und legt die Bytes dort ab (`\Seen`). Gmail bekommt
  keine zweite Kopie. Scheitert nur die Kopie, bleibt die Nachricht gesendet und die
  Antwort sagt es.
- **Werkzeugnamen** bleiben die der Vorlage; Parameter ebenso (`mailbox`, `limit`,
  `unseen`, `reply_all` …), nur `max_zeichen` ist deutsch wie in `quiver-outline`.

Die Wahl der IMAP-Crate und die eine zusätzliche Abhängigkeit stehen in
[adr/0003-imap-mit-async-imap.md](adr/0003-imap-mit-async-imap.md).

## Ein drittes Tool: `crates/zep` – ein Vermittler

`quiver-zep` baut ZEP **nicht** nach: es ist ein lokaler stdio-MCP, der den offiziellen
ZEP-MCP (Streamable HTTP, OAuth) anspricht und dessen Werkzeuge 1:1 durchreicht. Eigene
Aufgabe: angemeldet bleiben.

| Modul | Aufgabe | Zustand |
|---|---|---|
| `werkzeug` | `Zep` (Name, Titel, Felder, `testen`), `ZepKonfig` (Adresse, Benutzer, Passwort, Zustandsordner, Takt) | keiner |
| `metadaten` | `/.well-known/oauth-protected-resource…` und `…/oauth-authorization-server…` (RFC 9728/8414) | einmal je Lauf |
| `oauth` | Registrierung (DCR), Authorize-Adresse, Code-Tausch, Erneuerung | keiner |
| `pkce` | Verifier, Challenge (S256), `state` | keiner |
| `html` | Formularleser: Formulare, Felder, Auswahllisten, Knöpfe | keiner |
| `cookies` | Cookie-Speicher des Ablaufs (Host, Pfad, `Secure`) | je Ablauf |
| `routine` | der Ablauf ohne Browser: Formular, Zustimmung, Rückleitung abfangen, Code tauschen | je Ablauf |
| `browser` | `Browser`-Naht: `OeffneBrowser` (Notfall), `KeinBrowser` | keiner |
| `anmeldung` | Token, Tresor, Sperrschutz (nur nach echtem Fehlversuch), Hintergrund-Erneuerung, `status` | ein Token im Speicher, Refresh im Tresor |
| `tresor` | `Tresor`-Naht: `DateiTresor` (`tresor/` im Zustandsordner), `SpeicherTresor` | Refresh-Token, `client_id` |
| `sperre` | Sperrzustand als Datei (`routine-gesperrt.json`) | eine Datei |
| `vermittler` | rmcp-Client zum ZEP-MCP, 401 → erneuern → einmal wiederholen | eine Verbindung |
| `server` | der MCP-Server über stdio, Anmeldung und Liste beim Start, `list_changed` weitergeben | Werkzeugliste |
| `befehle` | `anmelden`, `anmelden --browser`, `status`, `abmelden`, `routine-pruefen` | keiner |

Datenfluss eines Aufrufs:

```
MCP tools/call                 quiver-zep anmelden
  → server::aufrufen             → befehle::ausfuehren
  → mcp::zugang (Umgebung)        → anmeldung::routine_lauf
  → vermittler::aufrufen          → routine::ausfuehren (HTML, Cookies, PKCE)
    → anmeldung::access_token     → oauth::code_tauschen
      (Speicher → Refresh → Routine)
    → rmcp-Client (Bearer) → ZEP-MCP
  → CallToolResult (1:1)
## Ein Tool: `crates/ssh`

Die Vorlage `pi-ssh` (TypeScript) liefert die Semantik. Aufgabe 05 ändert zwei Dinge
gegenüber den anderen Tools: **mehrere Verbindungen** (`mehrfach`) und die **Pflicht zur
Schlüssel-Anmeldung** nach der ersten Passwort-Anmeldung.

| Modul | Aufgabe | Zustand |
|---|---|---|
| `werkzeug` | `Ssh` (sechs Felder, `mehrfach`, `testen`), Namen der Feldkonstanten | keiner |
| `verbindung` | `QUIVER_ZUGAENGE` → geprüfte `Verbindung`en, Auswahl (`waehlen`) | Umgebungsvariable |
| `pfade` | `<daten>/quiver-tools/ssh/{schluessel,known_hosts}`, Rechte 0600/0700 | Dateien |
| `schluessel` | ed25519 erzeugen, laden, `uebernehmen`, `gleiches_material` | Dateien |
| `hostschluessel` | TOFU gegen eigene und `~/.ssh/known_hosts` (auch gehasht) | Dateien |
| `anmeldung` | der Ablauf: Schlüssel, erste Einrichtung, Aufräumen, Hinweise | Dateien + Netz |
| `client` | der **einzige** Ort mit Netzverkehr (`russh`, `russh-sftp`) | Sitzung |
| `format` | reine Funktionen: Daten → Text | keiner |
| `server` | die sechs MCP-Werkzeuge | Verbindungen aus der Umgebung |

Ablauf jeder Verbindung (`anmeldung::bereit`):

```
Verbindung gewählt
  ├── Schlüssel vorhanden?      → nur Schlüssel; scheitert das → Fehler (kein Rückfall)
  ├── sonst Passwort vorhanden? → Passwort → Schlüssel erzeugen → authorized_keys →
  │                               Prüf-Verbindung nur mit Schlüssel → erst dann übernehmen
  └── sonst                     → Hinweis: Passwort im quiver-Fenster eintragen
```

Regeln, die dort gelten:

- **Der Ablauf meldet, was er sieht.** `html::Seite::kurzfassung` nennt Formular- und
  Feld**namen**; Werte stehen in keinem Bericht, keinem Fehler und keinem Log.
- **Zugangsdaten gehen nur an den Authorization-Host.** Eine Weiterleitung auf einen
  fremden Host bricht ab, statt das Passwort dorthin zu tragen.
- **Nur ein echter Fehlversuch sperrt.** Gesperrt wird nur, wenn Benutzer und Passwort
  tatsächlich abgeschickt wurden und die Anmeldung danach nicht zum Token führte
  (`Abgelehnt`, oder eine unbekannte Seite **nach** dem Absenden) – nur das zählt bei
  ZEP, das nach fünf Fehlversuchen sperrt. Dann steht `routine-gesperrt.json` im
  Zustandsordner, und frei wird sie erst durch eine erfolgreiche Anmeldung über den
  Knopf (`anmelden`). Fehler **vor** dem Absenden (Netz, Metadaten, Registrierung,
  unbekannte Seite vor dem Formular, fremder Host) schreiben keine Sperrdatei: sie
  werden gemeldet, und der nächste Bedarf versucht es erneut – höchstens alle 60 s
  (Merker im Speicher, nicht in der Datei). Dafür gibt `routine::Routine` zurück, ob
  die Zugangsdaten gesendet wurden (`zugangsdaten_gesendet`).
- **Anmelden beim Start.** Ohne gültiges Token, ohne Sperre und mit Benutzer und
  Passwort meldet sich `quiver-zep` beim Start des MCP-Prozesses sofort im Hintergrund
  an (`server::starte_anmeldung`) – nicht erst beim ersten Werkzeugaufruf. Danach liegt
  das Refresh-Token im Tresor und `status` sagt „Angemeldet“.
- **Nur eine Erneuerung gleichzeitig.** Refresh und Routine liegen unter einer Sperre;
  sonst nähmen sich zwei Erneuerungen das rotierte Refresh-Token weg.
- **Der Browser ist Notfall.** `KeinBrowser` ist der Normalfall; nur `anmelden --browser`
  öffnet den Standardbrowser und lauscht auf `127.0.0.1`.
- **Werkzeuge unverändert.** Namen, Schemas und Annotationen kommen von ZEP; nur
  `zep_profile` ergänzt der Kern.

Warum Vermittler statt Nachbau, warum eine eigene Routine und wie der Sperrschutz
funktioniert: [adr/0004-zep-offizieller-mcp.md](adr/0004-zep-offizieller-mcp.md).
- **Ein Passwort, zwei Rollen.** `anmelde_passwort` dient der ersten Anmeldung und `sudo -S`
  (ADR 0003, Nachtrag). Ein Altbestand mit `anmelde_passwort`/`sudo_passwort` wird
  weiter gelesen; ein vorhandenes `sudo_passwort` hat Vorrang. `Debug` zeigt von
  `Verbindung` nur, *ob* eines gesetzt ist.
- **Erst prüfen, dann übernehmen.** Der neue Schlüssel liegt bis zur erfolgreichen
  Prüf-Verbindung als `.neu` neben dem Ziel; scheitert etwas, werden beide Dateien
  entfernt und der alte Schlüssel bleibt unberührt.
- **Host-Schlüssel sind eine Grenze.** Geändert heißt Abbruch mit beiden
  Fingerabdrücken, Datei und Zeile; geschrieben wird nur in die eigene `known_hosts`,
  `~/.ssh/known_hosts` wird nie verändert.
- **sudo ohne Leck.** `sudo -S -p '' -- sh -c '<befehl>'`, Passwort über stdin, nie in
  der Befehlszeile, der Ausgabe, einem Fehlertext oder einem Log.
- **Ein Aufruf, eine Sitzung.** Jedes Werkzeug öffnet die Verbindung, arbeitet und
  beendet sie wieder; es gibt keinen Verbindungspool und keinen Tunnel (siehe
  [offen.md](offen.md)).

## Tests

- Attrappen (axum) auf `127.0.0.1`, kein echter Dienst. Bei E-Mail stehen die
  Attrappen hinter den Traits `Postfach`/`Versand`; sie schreiben jeden Aufruf mit
  (Postfach, UID, Grenze, Empfänger, Flaggen) und geben feste Daten zurück.
- Der Zugang kommt als Paar-Menge über `Zugang::mit_umgebung` – kein Prozesszustand,
  keine parallelen Umgebungsvariablen.
- `tests/ende_zu_ende.rs` startet `quiver-outline` als Kindprozess: einmal mit
  gesetzter Umgebung gegen die Attrappe, einmal ohne (Hinweistext), dazu `eintrag`
  und `testen`.
- Der Eintrag wird gegen eine Kopie von `quiver_domain::Eintrag` deserialisiert
  (`eintrag::tests`), damit Formatfehler auffallen, ohne das andere Projekt zu
  importieren.
- SMTP wird zusätzlich gegen `lettre`s Stub-Transport geprüft: Kopfzeilen, Threading
  und Umschlag (CC/BCC im Umschlag, BCC **nicht** in den Kopfzeilen).
- `crates/zep/tests/attrappe` bildet den ZEP-MCP nach: Metadaten, DCR, Anmelde- und
  Zustimmungsseite mit CSRF und Cookies, Token mit Rotation und `invalid_grant`, dazu
  ein Mini-MCP-Server hinter `Bearer`, der auf Wunsch `tools/list_changed` schickt.
  Kein Test spricht mit zep-online.de; `ZEP_TRESOR=speicher` hält die Tresor-Datei aus
  dem Spiel.
- **Tests laufen nie im echten Zustand.** Wo ein Modul einen Ordner für Zustand wählt,
  liefert er unter `cfg(test)` einen temporären Ordner je Prozess – `zep::werkzeug::
  zustandsordner` (und damit der Sperrzustand und `DateiTresor::standard`),
  `m365_anmeldung::tresor` und `ssh::pfade::Pfade::aus_umgebung` (im Test ohne Home).
  Ein Wächter-Test je Crate prüft, dass dieser Ordner nicht unter `HOME` liegt; ein
  Test schreibt so nie eine `routine-gesperrt.json` in den Zustand des Users.
  Integrationstests setzen weiterhin `ZEP_ZUSTAND`/`QUIVER_SSH_DATEN`, weil die
  Bibliothek dort ohne `cfg(test)` läuft.
- `crates/email/tests/imap_echt.rs` ist `#[ignore]` und spricht nur auf Anforderung mit
  einem echten IMAP-Server (`EMAIL_TEST_*`, z. B. GreenMail im Docker). Ohne diese
  Umgebung geht kein Test ins Netz.
- Für SSH läuft ein echter `russh`-Server im Testprozess (`tests/attrappe`): Passwort-
  und Schlüssel-Anmeldung, `authorized_keys` im Speicher, ein simuliertes
  `sudo -S -p ''` und SFTP über einen temp-Ordner. `HOME` und `QUIVER_SSH_DATEN` zeigen
  auf temp-Ordner; die echte `~/.ssh` wird nie gelesen oder geschrieben.

## Plattformen: Linux, macOS, Windows

Die Werkzeuge laufen auf allen drei Systemen; die CI prüft alle drei
(`.github/workflows/pruefen.yml`). Kein Code verzweigt über `cfg(target_os)` –
die Unterschiede kommen aus `std::path` und aus Umgebungsvariablen. Nur drei
Dinge brauchen wirklich ein `cfg(unix)`:

| Was | Unix | Windows |
| --- | --- | --- |
| Dateirechte | `0600`/`0700` für Schlüssel (`ssh`), Tresor (`zep`, `m365-anmeldung`) und Zustandsordner | kein Gegenstück: es bleibt bei der ACL, die der Ordner vom Benutzerprofil erbt. `rechte`/`nur_ich` sind dort ein No-op, und `#[cfg(unix)]`-Tests prüfen den Modus |
| Prozessende | SIGTERM-Wächter in `quiver-abap`: offene SAP-Sperren werden freigegeben (höchstens 5 s), Ende mit 143 | kein SIGTERM. Locks gibt nur `Drop` beim normalen Ende (stdin zu) frei; ein hart beendeter Prozess (`TerminateProcess`) bekommt keine Gelegenheit dazu |
| Symlink-Tests | `std::os::unix::fs::symlink` | ohne Entwicklermodus oder Admin nicht anlegbar; die Fälle entfallen (`#[cfg(unix)]`), die Regel selbst prüft `pruefe_im_ordner` auch dort |

Alles andere ist plattformgleich gelöst, ohne `cfg`:

- **Pfade** entstehen nur über `Path`/`PathBuf` (`join`, `strip_prefix`,
  `canonicalize`). Relative Namen von Agenten tragen immer `/`; die Ablage zerlegt
  sie in Segmente und baut den Pfad mit `join` – der Trenner des Systems entsteht
  erst dort. Laufwerksangaben (`C:`), UNC-Pfade, Backslash, `..`, versteckte Namen
  und Windows-Gerätenamen (`CON`, `NUL`, `COM1`…) werden darum **auf jeder
  Plattform** abgelehnt.
- **Benutzerordner**: `HOME`, sonst `USERPROFILE`; `dirs::data_dir()` für den
  Datenordner. `~` und `~/…` werden ohne `cfg` aufgelöst.
- **Kindprozesse** starten über `env!("CARGO_BIN_EXE_…")` – auf Windows mit `.exe`,
  ohne Sonderfall im Test.
- **Zeilenenden**: gelesen wird über `BufRead::lines()` (schluckt `\r\n`), geschrieben
  mit `\n`; was ein Werkzeug schreibt, liest es byte-gleich zurück. Testdaten aus dem
  Repo werden nicht Zeile für Zeile erwartet.
- **Programmstart** (`open::that`) und **HTTP** (`reqwest` mit rustls) sind
  plattformneutral; die Zertifikatsprüfung gegen den Systemspeicher macht
  `rustls-platform-verifier` (Linux, macOS, Windows).
- **Ordner-Ausbruch**: `kern::Ablage` prüft jede existierende Ebene auf Symlinks
  **und** vergleicht den aufgelösten Elternordner mit der aufgelösten Wurzel. Der
  zweite Vergleich fängt Windows-Junctions („Mount Point“, `mklink /J`), die nicht
  auf jeder Fassung als symbolischer Link gemeldet werden. `diagramm::Ablage` prüft
  genauso.
- **Sperrdateien** (`fd_lock` in `diagramm`, `File::lock` in `zep` und
  `m365-anmeldung`) sind auf Windows verpflichtend statt nur beratend. Die Sperrdatei
  wird nie gelesen, darum ändert das nichts am Verhalten – es serialisiert nur
  strenger.
- **Schreiben** geht immer über eine temporäre Datei und ein anschließendes
  Umbenennen; damit liegt entweder der alte oder der neue Stand da. Auf Windows kann
  ein Umbenennen scheitern, solange ein anderes Handle die Zieldatei offen hält – dann
  kommt ein Fehler, kein halber Inhalt.

Geprüft wird das so: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`
und `cargo test --workspace` nativ auf Linux, macOS und Windows (CI). Lokal auf einem
Mac kommt für Windows `cargo clippy --workspace --all-targets --target
x86_64-pc-windows-gnu` mit `mingw-w64` hinzu; die Tests für Windows zeigt allein die
CI.

## Ein fünftes Tool: `crates/abap`

Die Vorlage `pi-abap-fs` (TypeScript, Bibliothek `abap-adt-api`) liefert die Semantik,
nicht den Code (SPEC §6). Die Anfragen sind bei `abap-adt-api` abgelesen und in Rust neu
gebaut ([adr/0005-adt-client-in-rust.md](adr/0005-adt-client-in-rust.md)); Teil 2
(Schreiben) steht in [adr/0006-abap-schreiben.md](adr/0006-abap-schreiben.md).

| Modul | Aufgabe | Zustand |
|---|---|---|
| `werkzeug` | `Abap` (sieben Felder, `mehrfach` = „Systeme“, `testen`) | keiner |
| `systeme` | `QUIVER_ZUGAENGE` → geprüfte `System`e, Auswahl (`waehlen`), Schalter `schreiben` | Umgebungsvariable |
| `adt::client` | der **einzige** Ort mit Netzverkehr: Basic-Auth, `sap-client`/`sap-language`, Cookies je System, CSRF, 401, `stateful` | Token + Cookies je Client |
| `adt::sitzung` | die stateful Sitzung je System: Lock-Tabelle, Sitzungsablauf, Freigabe beim Ende | Cookies, Token, Locks |
| `adt::xml` | `quick-xml` → `Knoten`, Namensräume fallen weg | keiner |
| `adt::daten` | je Endpunkt ein reiner Leser: XML → Daten (auch Sperre und Aktivierung) | keiner |
| `adt::fehler` | `AdtFehler`: Ausnahme, HTTP, 401, abgelaufene Sitzung – nie mit Geheimnis | keiner |
| `schreiben` | Schutzregeln und Rümpfe der schreibenden Werkzeuge | keiner |
| `format` | reine Funktionen: Daten → knapper Text, gekürzt über `mcp::kuerzen` | keiner |
| `server` | die 25 MCP-Werkzeuge (plus `abap_profile` aus dem Kern) | Systeme aus der Umgebung, Sitzungen |

Datenfluss eines **lesenden** Aufrufs (zustandslos):

```
MCP tools/call
  → server::aufrufen          Name → Werkzeug, Argumente → Parametertyp
  → server::waehlen           System aus QUIVER_ZUGAENGE (fehlt die Liste: Hinweistext)
  → adt::client::anfrage      Token (fetch) → Aufruf; 403 Required → Token neu, einmal wiederholen
  → adt::daten                XML → Daten
  → format                    Daten → knapper Text
  → CallToolResult
```

Datenfluss eines **schreibenden** Aufrufs (eine Sitzung je System):

```
MCP tools/call
  → server::schreiben_erlaubt  Schalter `schreiben`? Sonst Hinweis, **keine** Anfrage
  → server::namensraum_pruefen Z*, Y*, /…/ – sonst Ablehnung, **keine** Anfrage
  → Größe/Format prüfen        `source` ≤ 2 MB, Textelemente nach Vorlage
  → server::aufloesen          Name/Typ → objectUrl + Hauptinclude (lesend, zustandslos)
  → adt::sitzung::hole         eine Sitzung je System (Cookies, Token, Lock-Tabelle)
  → sitzung::sperren           POST <objekt-url>?_action=LOCK
  → sitzung::anfrage           PUT <hauptinclude>?lockHandle=…&corrNr=…
  → sitzung::entsperren        POST <objekt-url>?_action=UNLOCK (auch nach Fehler)
  → format                     Daten → knapper Text
  → CallToolResult
```

Regeln, die dort gelten:

- **Anmeldung ohne Wiederholung.** `401` heißt „Anmeldung abgelehnt“ und wird nicht
  wiederholt: SAP sperrt Benutzer nach zu vielen Fehlversuchen. Das Passwort steht nur
  im Basic-Auth-Kopf; `Debug` von `System`, `Client` und `Sitzung` zeigt höchstens, *ob*
  eines gesetzt ist.
- **CSRF genau einmal.** Ein `403` mit `x-csrf-token: Required` (oder eine CSRF-Meldung
  im Rumpf) holt ein neues Token und wiederholt den Aufruf **einmal**; ein zweiter
  Fehlschlag geht als Fehler hinaus. In einer Sitzung bleibt der Keks dabei gleich.
- **Cookies je Client.** `SAP_SESSIONID_*` liegt im Client, nicht im Prozess; zwei
  Systeme teilen sich nichts.
- **Eine Sitzung je System.** `abap_lock`, `abap_write`, `abap_unlock`,
  `abap_activate` und `abap_set_text_elements` laufen über dieselbe stateful Sitzung
  (`X-sap-adt-sessiontype: stateful`), weil SAP einen Lock-Handle nur dort kennt.
  Lesende Werkzeuge arbeiten weiter zustandslos.
- **Kein stilles Wiederholen.** Läuft die Sitzung ab (`400`/`401` mit „Session timed
  out“), wird sie samt Lock-Tabelle verworfen und der Aufruf endet mit
  `AdtFehler::SitzungAbgelaufen`. Ein Schreibvorgang wird nie nachgeschoben.
- **Locks werden freigegeben.** Selbst geholte Sperren gibt `abap_write`/
  `abap_set_text_elements` auch nach einem Fehler frei (wie `finally` in der Vorlage).
  Beim Prozessende (stdin zu, normales Ende) gibt `Drop` des Servers alle bekannten Locks
  frei, auf Unix zusätzlich der SIGTERM-Wächter – höchstens 5 s, Fehler nur ins Log.
- **Drei Schutzregeln vor jeder Anfrage.** `schreiben` je System (Standard aus),
  Kundennamensraum (`Z*`, `Y*`, `/…/`, Paket `$TMP` oder Kundenpaket) und `source` bis
  2 MB. Verstöße enden als Text bzw. Parameterfehler, **ohne** Anfrage an SAP.
- **Zertifikate.** `reqwest` mit rustls und den Wurzeln des System-Zertifikatsspeichers
  (`rustls-native-certs`); `zertifikat_pruefen = aus` ist der Notausgang.
- **Ausnahmen durchreichen.** `exc:exception` wird zu Typ und Nachricht; ein Agent soll
  die SAP-Meldung sehen, nicht `HTTP 400`.
- **`abap_profile` kommt aus dem Kern.** Bei einem Werkzeug mit `mehrfach` gibt es kein
  einzelnes Profil; das Werkzeug antwortet darum mit derselben Liste wie `abap_status`
  (Systeme ohne Passwort).
- **Grenzen.** `max_zeichen` (Standard 20000) bei `abap_read`, `max_zeilen` (Standard
  100, höchstens 1000) bei `abap_table`/`abap_query`, 50 Aktivierungsmeldungen.
- `crates/abap/tests/attrappe` bildet einen ADT-Server nach: CSRF-Token mit
  `403 Required`, Cookies je Sitzung, Basic-Auth, Sperren, Quelltext-PUT, Aktivierung,
  Anlegen und je Endpunkt eine Antwort. Sperren, Schreiben und Entsperren gehen dort nur
  mit dem Keks der Sitzung, die gesperrt hat; eine andere Sitzung bekommt `423`. Kein
  Test spricht mit einem echten SAP-System; das Passwort der Attrappe ist erfunden.

## Ein sechstes Tool: `crates/devops`

Die Vorlage `pi-azure-devops` (TypeScript, Bibliothek `azure-devops-node-api`) liefert
die Semantik, nicht den Code (SPEC §6). Die Aufrufe sind an der REST-Dokumentation
(Azure DevOps 7.1) und an der Vorlage abgelesen und in Rust mit `reqwest` neu gebaut
([adr/0007-devops-mit-pat.md](adr/0007-devops-mit-pat.md)).

| Modul | Aufgabe | Zustand |
|---|---|---|
| `werkzeug` | `Devops` (vier Felder, `mehrfach` = „Organisationen“, `testen`) | keiner |
| `organisationen` | `QUIVER_ZUGAENGE` → geprüfte `Organisation`en, Auswahl (`waehlen`), Schalter `schreiben`, Projekt aus Zugang oder Aufruf | Umgebungsvariable |
| `client` | der **einzige** Ort mit Netzverkehr: Basic-Auth (`":"` + PAT), `api-version`, `429` mit `Retry-After`, JSON-Patch-Kopf | keiner |
| `fehler` | `DevopsFehler`: 401, 203 mit HTML, HTTP, Netz, JSON – nie mit PAT | keiner |
| `daten` | JSON → Werte: Punktpfade (Felder mit Punkt!), Listen mit und ohne `value`, Datum | keiner |
| `html` | HTML → Text für Beschreibungen, Kriterien und Kommentare | keiner |
| `wiql` | WIQL aus den Komfort-Parametern (`text`, `status`, `typ`, `zugewiesen_an_mich`) | keiner |
| `format` | reine Funktionen: Daten → knapper Text, gekürzt über `mcp::kuerzen` | keiner |
| `server` | die 26 MCP-Werkzeuge (plus `devops_profile` aus dem Kern) | Organisationen aus der Umgebung |

Datenfluss eines **lesenden** Aufrufs:

```
MCP tools/call
  → server::aufrufen        Name → Werkzeug, Argumente → Parametertyp
  → server::waehlen         Organisation aus QUIVER_ZUGAENGE (fehlt die Liste: Hinweistext)
  → server::projekt         Projekt aus dem Aufruf, sonst aus dem Zugang, sonst Hinweis
  → client::anfrage         Basic(":"+PAT), api-version; 401/203 → Fehler; 429 → höchstens 2×
  → daten                   JSON → Werte
  → format                  Werte → knapper Text
  → CallToolResult
```

Datenfluss eines **verändernden** Aufrufs:

```
MCP tools/call
  → server::schreiben_erlaubt  Schalter `schreiben`? Sonst Hinweis, **keine** Anfrage
  → server::projekt            Projekt bekannt? Sonst Hinweis, **keine** Anfrage
  → client::anfrage            POST/PATCH/PUT; bei 429 **kein** Wiederholen
  → format                     kurze Bestätigung (angelegt/geändert/verknüpft …)
  → CallToolResult
```

Regeln, die dort gelten:

- **Der PAT steht nur in der Kopfzeile.** `Authorization: Basic base64(":" + PAT)`;
  `Debug` von `Organisation` und `Client` zeigt höchstens, *ob* einer gesetzt ist.
  Antworten, Fehlertexte und Logs nennen ihn nie.
- **`api-version` an jeder Anfrage.** `7.1`; die Kommentar-Endpunkte (`…/comments`)
  brauchen `7.1-preview.4` und `_apis/connectionData` braucht `7.1-preview.1`, weil
  DevOps beide noch als Vorschau führt (mit `7.1` antworten sie mit HTTP 400). Den
  `connectionData`-Aufruf baut allein `client::verbindungsdaten()`. Eine eigene
  `api-version` im Aufruf wird nicht verdoppelt.
- **401 und 203 mit HTML sind endgültig.** DevOps leitet bei ungültigem PAT auf die
  Anmeldung um; beides wird zu „PAT für <verbindung> ungültig oder abgelaufen“ und
  **nicht** wiederholt.
- **429 nur beim Lesen.** Höchstens zwei Wiederholungen, `Retry-After` beachtet (maximal
  10 s). Ein Anlegen oder Ändern wird nie wiederholt: ob es ankam, weiß nur der Server.
- **JSON-Patch mit eigenem Kopf.** `POST …/workitems/$Typ` und `PATCH …/workitems/{id}`
  tragen `Content-Type: application/json-patch+json`; ein zweiter `Content-Type` entsteht
  nicht.
- **Projekt aus zwei Quellen.** `projekt` am Zugang oder am Aufruf; fehlt beides, kommt
  ein Hinweis **ohne** Anfrage. Dasselbe gilt für `team` bei den Iterationen.
- **Arbeitsitem-Felder enthalten Punkte.** `daten::wert` behandelt nach `fields.` den
  Rest als **einen** Schlüssel (`System.Title`), nicht als Pfad.
- **Texte kommen als HTML.** Beschreibungen, Akzeptanzkriterien und Kommentare werden zu
  Text (`html`), dann über `max_zeichen` gekürzt (Standard 4000) – mit Hinweis, wie viel
  fehlt. Ein Log zeigt das **Ende** (`zeilen`, Standard 200).
- **System-Threads fallen weg.** `devops_pull_request_threads` lässt Threads aus, die
  DevOps selbst anlegt (Abstimmungen, Pushes) – sie tragen `CodeReviewThreadType`.
- **Ersteller-Filter lokal.** DevOps kennt in der Abfrage nur eine Ersteller-ID, keinen
  Anzeigenamen; `ersteller` filtert darum nach dem Anzeigenamen (Teiltreffer).
- **`devops_profile` kommt aus dem Kern.** Bei `mehrfach` gibt es kein einzelnes Profil;
  das Werkzeug antwortet mit derselben Liste wie `devops_status` (ohne PAT).
- `crates/devops/tests/attrappe` bildet eine Organisation nach: Basic-Anmeldung mit zwei
  gültigen PATs, alle benutzten Endpunkte, auf Wunsch `401`, `203` mit HTML und `429` mit
  `Retry-After`. Jeder Aufruf wird mitgeschrieben (Pfad, Methode, Abfrage, Rumpf,
  Kopfzeilen); kein Test spricht mit einem echten Azure DevOps, alle Token sind erfunden.
