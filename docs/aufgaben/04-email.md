# 04 – quiver-email (IMAP/SMTP)

Crate: `crates/email`, Programm `quiver-email`. Vorlage: `~/Documents/Workspaces/pi-email`
(`src/tools/`, `src/clients/imap-client.ts`, `smtp-client.ts`, `reply.ts`, `formatting/`, `README.md`).
**Nur IMAP/SMTP** – der Microsoft-/Graph-Teil der Vorlage (`graph-client.ts`, `oauth/`, `microsoft-login`)
entfällt, das macht `quiver-m365`. Zweck: „andere“ Postfächer (Gmail, Proton-Bridge, eigene Server).
Regeln wie in Aufgabe 02/03: Zugang aus der Umgebung über `Werkzeug::felder()`, kein `email_setup`,
ein Zugang je Eintrag.

## Abhängigkeiten

Kern-Abhängigkeiten wie bei Outline, dazu: `lettre` (SMTP, **rustls**, `tokio1`, `builder`),
`mail-parser` (Mails lesen), IMAP über `async-imap` mit `tokio` und `tokio-rustls` (+ `rustls-native-certs`
oder `webpki-roots`). **Nur rustls, kein OpenSSL/native-tls.** Wenn du eine andere IMAP-Crate für besser hältst,
begründe es in der Antwort.

## Felder (Umgebung `EMAIL_<FELD>`)

| Feld | Art | Standard | Hilfe |
|---|---|---|---|
| `adresse` | Text, Pflicht | – | Absenderadresse, z. B. du@gmail.com |
| `name` | Text | – | Anzeigename des Absenders |
| `imap_host` | Text, Pflicht | – | z. B. imap.gmail.com, Proton-Bridge: 127.0.0.1 |
| `imap_port` | Zahl | 993 | |
| `imap_sicherheit` | Auswahl `tls`/`starttls`/`keine` | `tls` | |
| `smtp_host` | Text, Pflicht | – | z. B. smtp.gmail.com |
| `smtp_port` | Zahl | 587 | |
| `smtp_sicherheit` | Auswahl `tls`/`starttls`/`keine` | `starttls` | |
| `benutzer` | Text | = `adresse` | Anmeldename für IMAP und SMTP |
| `passwort` | Passwort, Pflicht | – | Bei Gmail ein App-Passwort |
| `zertifikat_pruefen` | Schalter | an | Aus nur für lokale Brücken (Proton-Bridge, selbstsigniert). **Nur** bei Host `127.0.0.1`/`localhost` wirksam – sonst ignorieren und warnen. |

`testen`: IMAP-Login + `SELECT INBOX`, SMTP-Verbindung + `EHLO` + Auth (ohne zu senden) →
„IMAP und SMTP erreichbar, angemeldet als …“.

## Tools

| Tool | Hinweis |
|---|---|
| `email_profile` | lesend: Zugang ohne Passwort |
| `email_list_mailboxes`, `email_fetch`, `email_search`, `email_read` | `read_only_hint` |
| `email_send`, `email_reply`, `email_forward`, `email_draft_reply`, `email_flag`, `email_move`, `email_delete` | `destructive_hint` |

- Parameter und Verhalten wie in der Vorlage (Postfach Standard `INBOX`, `uid`, `limit` Standard 20 / höchstens 100,
  `unseen`, Suche nach Absender/Betreff/Text/Zeitraum wie die Vorlage).
- `email_read`: Text statt HTML (HTML in Text umwandeln, wenn es keinen Textteil gibt), `max_zeichen` (Standard 6000)
  mit Kürzungshinweis aus dem Kern; Anhänge als Liste (Name, Typ, Größe) – **kein** Inhalt. PDF-Text der Vorlage
  vorerst weglassen (in `docs/offen.md` vermerken).
- `email_reply`/`email_draft_reply`: Threading wie die Vorlage (`In-Reply-To`, `References`, „Re:“), Zitat optional.
- `email_send`: Gesendete Mail zusätzlich in den Ordner „Gesendet“ legen, wenn die Vorlage das tut (Ordner per
  `SPECIAL-USE \Sent` finden).
- Zeiten als RFC 3339 mit Versatz (wie in quiver-m365).
- Fehlertexte nie mit Passwort.

## Tests

- **Protokollschicht hinter Traits** (`Postfach` für IMAP, `Versand` für SMTP), Tools gegen Attrappen dieser Traits.
- SMTP: zusätzlich gegen `lettre`s Stub-/Datei-Transport (Kopfzeilen, Threading, Empfänger).
- IMAP-Ende-zu-Ende gegen einen echten Server nur als `#[ignore]`-Test, der über Umgebung (`EMAIL_TEST_*`) gesteuert
  wird – dokumentiere, wie man ihn z. B. mit GreenMail im Docker laufen lässt. Kein Test darf ohne diese Umgebung
  ins Netz gehen.
- Ende-zu-Ende des Programms über stdio mit Attrappe (wie Outline): `list_tools`, ohne Zugang → Hinweistext.
- `quiver-email eintrag` liefert die Felder oben.

## Doku

README-Abschnitt quiver-email (Tools, Felder, Gmail/Proton-Hinweise), `docs/architektur.md`.
Prüfbefehl grün.
