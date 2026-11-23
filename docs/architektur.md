# Architektur

> Stand: 2026-11-23, nach Aufgabe 06. Gilt für den Kern (`crates/kern`) und alle
> Stand: 2026-11-23, nach Aufgabe 05. Gilt für den Kern (`crates/kern`) und alle
> Tools, die darauf aufsetzen. Festlegungen und Preise: [../SPEC.md](../SPEC.md).

## Richtung

```
crates/kern        Werkzeug-Trait, Zugang aus der Umgebung, Bibliothekseintrag, MCP-Hilfen, Gerüst
crates/<tool>      ein Programm quiver-<tool> je Tool
assets/            tokens.css und Schriften (Kopie aus mcp-quiver, nicht von Hand ändern)
```

`kern ← <tool>`: ein Tool kennt den Kern, der Kern kennt kein Tool. Alles, was ein
Tool dem Kern über sich sagt, steht in `Werkzeug`: Name, Titel, Beschreibung, Felder,
`testen`.

Die Oberfläche ist **quiver**. Dieses Repo liefert keine Seite und keinen Browser-Code
mehr; `assets/` bleibt als Vorlage für quiver und wird hier von keinem Programm
eingebunden.

## Der Kern in Schichten

| Modul | Aufgabe | Zustand |
|---|---|---|
| `umgebung` | `Zugang`: Felder aus `<NAME>_<FELD>` lesen, Pflichtfelder prüfen, `Profil` bauen | Umgebung des Prozesses |
| `profil` | `Profil { name, werte, geheim }`, `ProfilFehler`; `Debug` zeigt Geheimnisse nie | keiner |
| `eintrag` | `quiver_domain::Eintrag` als JSON: id, version, titel, beschreibung, laufzeit, felder | keiner |
| `mcp` | `zugang`, `profile_tool`, `profile_antwort`, `server_info`, `kuerzen` | keiner |
| `start` | `starten`, `aufruf_aus_args`, `Aufruf` (MCP, eintrag, testen, version, hilfe) | Prozess |
| `zugaenge` | `Zugang { name, werte }`, `zugaenge()`, `mit_umgebung()` aus `QUIVER_ZUGAENGE` | Umgebungsvariable |

Die Schnittstelle (`lib.rs`) legt der Architekt fest; die Umsetzung ergänzt sie nur.

## Datenfluss des Zugangs

```
quiver-Fenster (Tools → <Titel>)
  → Bibliothekseintrag (quiver-outline eintrag)      Felder, quelle client/geheim
  → User trägt Werte ein                             offen: Client-Konfiguration, geheim: Schlüsselbund
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
  (`umgebung::schluessel`). Kein Konfigurationsordner, kein Schlüsselbund im Tool.
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
felder   = [ { name, beschreibung: titel – hilfe, quelle: client|geheim, pflicht, standard } ]
```

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
| `anmeldung` | Token, Tresor, Sperrschutz, Hintergrund-Erneuerung, `status` | ein Token im Speicher, Refresh im Tresor |
| `tresor` | `Tresor`-Naht: `Schluesselbund` (Dienst `quiver-zep`), `SpeicherTresor` | Refresh-Token, `client_id` |
| `sperre` | Sperrzustand als Datei (`routine-gesperrt.json`) | eine Datei |
| `vermittler` | rmcp-Client zum ZEP-MCP, 401 → erneuern → einmal wiederholen | eine Verbindung |
| `server` | der MCP-Server über stdio, Liste beim Start, `list_changed` weitergeben | Werkzeugliste |
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
| `werkzeug` | `Ssh` (fünf Felder, `mehrfach`, `testen`), Namen der Feldkonstanten | keiner |
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
  └── sonst                     → Hinweis: Anmelde-Passwort im quiver-Fenster eintragen
```

Regeln, die dort gelten:

- **Der Ablauf meldet, was er sieht.** `html::Seite::kurzfassung` nennt Formular- und
  Feld**namen**; Werte stehen in keinem Bericht, keinem Fehler und keinem Log.
- **Zugangsdaten gehen nur an den Authorization-Host.** Eine Weiterleitung auf einen
  fremden Host bricht ab, statt das Passwort dorthin zu tragen.
- **Ein Versuch, dann Sperre.** Jeder Fehlschlag der Routine schreibt
  `routine-gesperrt.json`; frei wird sie nur durch eine erfolgreiche Anmeldung über den
  Knopf (`anmelden`). Automatische Wege wiederholen nie.
- **Nur eine Erneuerung gleichzeitig.** Refresh und Routine liegen unter einer Sperre;
  sonst nähmen sich zwei Erneuerungen das rotierte Refresh-Token weg.
- **Der Browser ist Notfall.** `KeinBrowser` ist der Normalfall; nur `anmelden --browser`
  öffnet den Standardbrowser und lauscht auf `127.0.0.1`.
- **Werkzeuge unverändert.** Namen, Schemas und Annotationen kommen von ZEP; nur
  `zep_profile` ergänzt der Kern.

Warum Vermittler statt Nachbau, warum eine eigene Routine und wie der Sperrschutz
funktioniert: [adr/0004-zep-offizieller-mcp.md](adr/0004-zep-offizieller-mcp.md).
- **Zwei Passwörter, zwei Rollen.** `anmelde_passwort` dient nur der ersten Anmeldung,
  `sudo_passwort` nur `sudo -S`. `Debug` zeigt von `Verbindung` nur, *ob* eines gesetzt ist.
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
  Kein Test spricht mit zep-online.de; `ZEP_TRESOR=speicher` hält den Schlüsselbund aus
  dem Spiel.
- `crates/email/tests/imap_echt.rs` ist `#[ignore]` und spricht nur auf Anforderung mit
  einem echten IMAP-Server (`EMAIL_TEST_*`, z. B. GreenMail im Docker). Ohne diese
  Umgebung geht kein Test ins Netz.
- Für SSH läuft ein echter `russh`-Server im Testprozess (`tests/attrappe`): Passwort-
  und Schlüssel-Anmeldung, `authorized_keys` im Speicher, ein simuliertes
  `sudo -S -p ''` und SFTP über einen temp-Ordner. `HOME` und `QUIVER_SSH_DATEN` zeigen
  auf temp-Ordner; die echte `~/.ssh` wird nie gelesen oder geschrieben.
