# 0003 – IMAP mit `async-imap`, SMTP mit `lettre`

> Stand: 2026-11-23. Aufgabe: [docs/aufgaben/04-email.md](../../docs/aufgaben/04-email.md).

## Kontext

`quiver-email` braucht IMAP und SMTP in **einem** Programm, das der Kern bereits als
async-Server über stdio startet (`rmcp`, `tokio`). Die Aufgabe empfiehlt `async-imap` mit
`tokio` und `tokio-rustls`, `lettre` mit rustls und `mail-parser`; OpenSSL/native-tls ist
ausgeschlossen, weil die Tools auf jedem Gerät ohne Systembibliotheken bauen sollen
(SPEC §1, AGENTS.md §2.10). Die Protokollschicht muss hinter Traits liegen, damit Tests
ohne Mailserver auskommen.

## Entscheidung

- **IMAP:** `async-imap` 0.12 mit `runtime-tokio`, TLS über `tokio-rustls` und
  `webpki-roots`. STARTTLS wird von Hand gemacht: Klartext-Verbindung, `STARTTLS`
  (über `run_command_and_check_ok`), `into_inner()`, TLS-Handshake, neuer `Client` –
  so beschreibt es die Dokumentation von `async-imap`.
- **SMTP:** `lettre` 0.11 ohne Default-Features, mit `builder`, `smtp-transport`,
  `tokio1-rustls-tls`. `Tls::Wrapper` für `tls`, `Tls::Required` für `starttls`,
  `Tls::None` für `keine`. `zertifikat_pruefen = aus` wird zu
  `dangerous_accept_invalid_certs`/`-hostnames` – aber nur, wenn der Host lokal ist
  (`Konto::prueft_zertifikat`).
- **Lesen:** `mail-parser` 1.0; `text_bodies()` wandelt HTML-only-Nachrichten in Text um,
  `attachments()` liefert Name, Typ und Größe ohne Inhalt.
- **Eine zusätzliche Abhängigkeit:** `futures-util` (`TryStreamExt`). `async-imap`
  liefert seine Ergebnisse als `impl Stream`; ohne das Trait lässt sich der Strom nicht
  einsammeln. Es ist bereits eine transitive Abhängigkeit von `async-imap`, bringt also
  keinen neuen Code in den Baum.
- **Zusätzlich explizit:** `rustls` (0.23, `ring`), weil die IMAP-Seite ihre
  `ClientConfig` selbst baut (Wurzelzertifikate bzw. Prüfung aus) und dafür die
  `CryptoProvider`-API braucht.

## Warum so

1. **Ein Laufzeitmodell.** `async-imap` und `lettre` sprechen beide `tokio`; der
   MCP-Server ist ohnehin async. Kein `spawn_blocking`, keine Thread-Brücke.
2. **Nur rustls.** Beide Wege nutzen `ring` und die Webpki-Wurzeln; es gibt kein
   `cfg(target_os)`, kein OpenSSL und keine System-TLS-Bibliothek.
3. **Die Naht bleibt testbar.** `Postfach` und `Versand` sind Trait-Objekte mit
   `Pin<Box<dyn Future>>`; die Attrappen in den Tests brauchen weder Port noch Server.
4. **`lettre` bringt den Stub-Transport mit.** Damit lassen sich Kopfzeilen, Threading
   und Umschlag (CC/BCC) prüfen, ohne zu senden – genau die Zusage aus der Aufgabe.

## Verworfene Alternativen

- **`imap` (synchron, 3.x):** blockierend; jeder Aufruf bräuchte `spawn_blocking`, und
  die TLS-Anbindung zieht je nach Feature native-tls mit. Die Vorlage ist async, der
  Kern ist async – die synchrone Crate wäre ein Bruch.
- **`imap-codec`:** nur der Codec, kein Client (kein Verbindungsaufbau, kein Login, kein
  STARTTLS). Zu viel Protokoll selbst gebaut.
- **`rustls-native-certs` statt `webpki-roots`:** würde den System-Speicher achten
  (Firmen-CAs), bringt aber plattformabhängige Leseorte mit. `webpki-roots` ist
  deterministisch und reicht für Gmail, self-hosted Server und die lokale Bridge; ein
  Wechsel ist eine Zeile in `postfach.rs` und steht in `docs/offen.md`.

## Preis

- **STARTTLS ist eigener Code.** Der Weg „Klartext, dann Upgrade“ ist nur über den
  ignorierten Ende-zu-Ende-Test (`tests/imap_echt.rs`) gegen einen echten Server
  prüfbar; die Attrappen decken ihn nicht ab.
- **Webpki-Wurzeln statt System-Speicher.** Ein Server mit Firmen-CA scheitert, bis
  `rustls-native-certs` nachgezogen wird.
- **Eine Abhängigkeit mehr als in der Aufgabe genannt** (`futures-util`), begründet
  oben; sie ist bereits im Baum.
- **Verbindung je Aufruf.** Jedes Werkzeug meldet sich neu an (wie die Vorlage). Das
  kostet eine Anmeldung, hält aber Zustand und Zeitablauf aus dem Programm.
