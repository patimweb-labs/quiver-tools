# quiver-tools

> **Status: in Arbeit.** Spezifikation in [SPEC.md](SPEC.md).

Eigene MCP-Tools für [mcp-quiver](https://github.com/patimweb-labs/mcp-quiver) in reinem Rust:
**Outline-Wiki**, **E-Mail** (IMAP/SMTP), **ZEP** (über den offiziellen ZEP-MCP), **SSH/SFTP** und
**ABAP** (SAP ADT). Die Zugangsdaten trägt
**Outline-Wiki**, **SSH/SFTP**, **E-Mail** (IMAP/SMTP) und **ABAP** (SAP ADT). Die Zugangsdaten trägt
der User **im quiver-Fenster** ein – unter Tools → <Titel>. quiver legt Geheimes in den Schlüsselbund
und setzt beim Start die Umgebung des Tools; Passwörter und API-Keys gehen nie durch einen Agenten.

## Der Zugang

Jedes Tool bringt seinen Bibliothekseintrag mit:

```bash
quiver-outline eintrag
```

Das JSON hat das Format von `quiver_domain::Eintrag`: Name, Titel, Beschreibung, Version, die Felder
des Zugangs und wie das Programm gestartet wird. quiver zeigt daraus unter **Tools → Outline-Wiki**
die Felder an (Adresse und API-Schlüssel). Offene Felder stehen in der Client-Konfiguration, geheime
im Schlüsselbund des Geräts.

Beim Start setzt quiver die Umgebung: `OUTLINE_URL`, `OUTLINE_API_KEY` – also `<NAME>_<FELD>` in
Großbuchstaben. Der Kern liest nur diese Umgebung, kein Profil aus einer Datei, keine Seite im Browser.

- Es gibt **einen** Zugang je Tool (`standard`). Wer zwei Wikis braucht, braucht zwei Einträge.
  **Ausnahme: quiver-ssh.** Dort pflegt der User eine Liste von Verbindungen (`Eintrag.mehrfach`);
  quiver übergibt sie als JSON in `QUIVER_ZUGAENGE`, die Felder stehen im Eintrag unter `mehrfach`.
- Fehlt ein Pflichtfeld, antwortet jedes Werkzeug mit dem Hinweis
  „Zugang fehlt – bitte im quiver-Fenster unter Tools → <Titel> die Angaben eintragen.“ – als Text,
  nicht als Fehler.
- `quiver-<tool> testen` prüft den Zugang von Hand (Exit 0 = erreichbar, 1 = fehlt oder Fehler).
- Ein `profil`-Parameter gibt es nicht mehr.

## quiver-outline (Outline wiki)

`quiver-outline` is the first of the four tools: it reads, searches and edits an
[Outline](https://www.getoutline.com/) wiki – cloud or self-hosted – without the API
key ever passing through a model.

Setup (once):

1. In Outline: **Settings → API & Apps** → create an API key (it starts with `ol_api_`).
2. In quiver: **Tools → Outline-Wiki** → enter the workspace address and the key.
3. Check it: `quiver-outline testen` (or from a shell with the same environment).

The address is forgiving: a bare host, a trailing slash, a pasted `/api` base or the
`/mcp` endpoint from Outline's MCP docs are all normalised to the workspace root.

### Tools

Every tool uses the one access described above. Without it each tool answers with the
hint instead of an error.

| Tool | Kind | What it does |
|---|---|---|
| `outline_collections` | read | List collections with their ids |
| `outline_structure` | read | Nested document tree of one collection |
| `outline_search` | read | Full-text search with collection, status and date filters |
| `outline_list` | read | List documents: recent, by collection, by parent, or your own drafts |
| `outline_read` | read | Read a document's markdown body, optionally with child documents or saved to a file |
| `outline_comments` | read | Comment threads of a document, nested by reply |
| `outline_profile` | read | The access that is set, without secrets |
| `outline_create` | write | Create a document in a collection or under a parent document |
| `outline_update` | write | Replace, append to or prepend to a body; rename; publish a draft |
| `outline_move` | write | Move a document to another collection or parent |
| `outline_archive` | write | Archive, restore or unpublish a document |
| `outline_delete` | write | Move a document to the trash, or delete it permanently |
| `outline_comment` | write | Post a comment or a reply |
| `outline_export` | write | Write a document (optionally with children) or a whole collection to local `.md` files |

A few details the agent relies on:

- `outline_read` truncates the body after `max_zeichen` characters (default 8000,
  `0` means no limit) and says how much it left out.
- `outline_export` always needs a `target_dir`; file names are derived from the title,
  never from the id alone, and cannot leave that directory.
- Outline errors become MCP errors with status and text – never with the API key.

### Entry for mcp-quiver

The tool is an own binary (no npm/pypi package), so `quiver-outline eintrag` prints the
entry. The address and the key are **fields**, not fixed values; `${feld}` in the
environment refers to them:

```json
{
  "id": "outline",
  "version": "0.1.0",
  "titel": "Outline-Wiki",
  "beschreibung": "Outline-Wiki lesen, schreiben, sortieren und kommentieren.",
  "herkunft": "eigen",
  "laufzeit": {
    "art": "stdio",
    "befehl": "/pfad/zu/quiver-outline",
    "umgebung": {
      "OUTLINE_URL": "${url}",
      "OUTLINE_API_KEY": "${api_key}"
    }
  },
  "felder": [
    {
      "name": "url",
      "beschreibung": "Adresse – Adresse deines Outline, z. B. https://firma.getoutline.com",
      "quelle": "client",
      "pflicht": true,
      "standard": null
    },
    {
      "name": "api_key",
      "beschreibung": "API-Schlüssel – Einstellungen → API & Apps, beginnt mit ol_api_",
      "quelle": "geheim",
      "pflicht": true,
      "standard": null
    }
  ]
}
```

`quelle: "geheim"` heißt: quiver legt den Wert in den Schlüsselbund und gibt ihn nur als
Umgebungsvariable an das Tool. Ein `einrichtung`-Feld gibt es nicht – das Tool öffnet
keine Seite im Browser.

## quiver-email (E-Mail über IMAP/SMTP)

`quiver-email` is the second tool: it reads, searches, sends, replies to and sorts mail
in any IMAP/SMTP mailbox – Gmail with an app password, a self-hosted server, or a local
Proton Bridge. Microsoft 365 / Exchange Online accounts stay in **quiver-m365** (Graph,
no IMAP/SMTP basics), so the two tools never overlap.

Setup (once), in quiver under **Tools → E-Mail**:

| Field | Required | Default | Notes |
|---|---|---|---|
| `adresse` | yes | – | Sender address, e.g. `you@gmail.com` |
| `name` | no | – | Display name of the sender |
| `imap_host` | yes | – | e.g. `imap.gmail.com`; Proton Bridge: `127.0.0.1` |
| `imap_port` | no | `993` | `143` for STARTTLS, Proton Bridge: `1143` |
| `imap_sicherheit` | no | `tls` | `tls`, `starttls` or `keine` |
| `smtp_host` | yes | – | e.g. `smtp.gmail.com` |
| `smtp_port` | no | `587` | `465` for TLS, Proton Bridge: `1025` |
| `smtp_sicherheit` | no | `starttls` | `tls`, `starttls` or `keine` |
| `benutzer` | no | = `adresse` | Login name for IMAP and SMTP |
| `passwort` | yes | – | For Gmail an **app password** |
| `zertifikat_pruefen` | no | `an` | `aus` only for a local bridge (`127.0.0.1`, `localhost`); ignored (with a warning) for any other host |

The password lives in the keychain and reaches the tool only as `EMAIL_PASSWORT`; it
appears in no answer, no log line and no error text.

```bash
quiver-email eintrag      # library entry with these fields
quiver-email testen       # IMAP login + SELECT INBOX, SMTP connect + EHLO + auth
```

`testen` prints “IMAP und SMTP erreichbar, angemeldet als …” and exits 0, or explains
what is missing and exits 1.

### Tools

All tools use the one access above; without it they answer with the hint instead of an
error.

| Tool | Kind | What it does |
|---|---|---|
| `email_profile` | read | The access that is set, without the password |
| `email_list_mailboxes` | read | Folders with their IMAP attributes (`\Sent`, `\Noselect`) |
| `email_fetch` | read | Latest messages of a mailbox: UID, sender, subject, date, flags |
| `email_search` | read | Search by sender, subject, body, date range (`YYYY-MM-DD`) or unread |
| `email_read` | read | Headers, attachment list (name, type, size) and text by UID |
| `email_send` | write | Send text with CC/BCC; copy into the Sent folder |
| `email_reply` | write | Reply with `In-Reply-To`/`References` and `Re:`, optional quoting and reply-all |
| `email_forward` | write | Forward with the original headers inline and `Fwd:` |
| `email_draft_reply` | write | Store a reply draft (`\Draft`, `X-Unsent`) without sending |
| `email_flag` | write | Set or remove IMAP flags (`Seen`, `Flagged`, `Answered`, …) |
| `email_move` | write | Move a message to another folder |
| `email_delete` | write | Delete a message; only with UIDPLUS does the server remove exactly that one |

Behaviour the agent can rely on:

- `mailbox` defaults to `INBOX`; `limit` defaults to 20 and is capped at 100.
- `email_read` returns text, never HTML: an HTML-only message is converted to text.
  It truncates after `max_zeichen` characters (default 6000, `0` means no limit) and
  says how much it left out. Attachments are listed, never downloaded.
- Threading is real: `email_reply` and `email_draft_reply` set `In-Reply-To` and the
  full `References` chain, and never stack `Re: Re: Re:`.
- After `email_send`, `email_reply` and `email_forward` a copy goes into the folder the
  server marks `\Sent` (RFC 6154). Gmail files sent mail itself, so no second copy is
  created there.
- Error texts never contain the password. A failed Sent-folder copy does not turn a
  successful send into an error; the answer says what happened.

### Gmail and Proton Bridge

**Gmail:** IMAP and SMTP need an [app password](https://myaccount.google.com/apppasswords)
(2-step verification must be on) – the account password is rejected. Hosts:
`imap.gmail.com:993` (TLS) and `smtp.gmail.com:587` (STARTTLS).

**Proton Bridge:** the bridge listens locally only. Use host `127.0.0.1`,
IMAP `1143`, SMTP `1025`, and switch `zertifikat_pruefen` off – the bridge presents a
self-signed certificate. The tool honours that switch **only** for `127.0.0.1` and
`localhost`; for any other host certificate checking stays on.

## quiver-zep (ZEP über den offiziellen ZEP-MCP)

`quiver-zep` is the third tool, and the only one that is a **broker**: it does not
rebuild ZEP. It is a local stdio MCP that talks to the official ZEP MCP (Streamable
HTTP, OAuth) and passes its tools through unchanged – names, schemas and annotations.
Its own job is only to **stay logged in**.

Setup (once), in quiver under **Tools → ZEP**:

| Field | Required | Notes |
|---|---|---|
| `url` | yes | MCP address, e.g. `https://www.zep-online.de/zepneoimpulse/next/mcp` |
| `benutzer` | yes | ZEP user name |
| `passwort` | yes | ZEP password; lives in the keychain and reaches the tool only as `ZEP_PASSWORT` |

```bash
quiver-zep eintrag           # library entry with these fields
quiver-zep testen            # signs in and names the tools ZEP offers
quiver-zep anmelden          # the normal sign-in: user name + password, no browser
quiver-zep anmelden --browser  # emergency only: browser with loopback 127.0.0.1
quiver-zep status            # is a session there? (exit 1 if not)
quiver-zep abmelden          # delete the refresh token, lift the lock
quiver-zep routine-pruefen   # run the routine once and report every step
```

What it does on its own:

- **Client registration (DCR)** once per installation: `client_name "quiver-zep"`,
  `redirect_uris ["http://127.0.0.1"]`, `token_endpoint_auth_method "none"`. Everything
  else – authorization, token and registration endpoints, the scope (`mcp:use`), PKCE
  S256 – comes from the `/.well-known` metadata. Only the base address is hard-coded.
- **Tokens:** the (rotating) refresh token in the keychain (service `quiver-zep`, key
  `<url>`), the access token in memory only. A rotated refresh token is stored at once.
- **Background renewal:** while the process runs, the access token is renewed at 80 % of
  its lifetime, even when no tool is called – so the refresh token never expires from
  idleness.
- **Routine without a browser:** the authorize flow runs over HTTP with its own cookie
  jar – load the login form, read its fields (action, hidden fields, CSRF), send user
  name and password, confirm the consent page, intercept the redirect to
  `127.0.0.1` (not followed), exchange code + PKCE verifier. The pages are unknown, so
  a small HTML form reader (`src/html.rs`) reads them and the routine reports step by
  step what it sees – form and field **names**, never values. A page it cannot classify
  aborts with that message.
- **Credentials stay on the authorization host.** User name and password are only sent to
  a form whose address is on the same host as the authorization endpoint; a redirect to a
  foreign host aborts the routine instead of carrying the password there.
- **Lockout protection:** ZEP locks an account after five failed attempts. Every
  failure of the routine is **one** attempt; after it the routine is locked (state in a
  file, so it survives restarts) until the next successful sign-in via the button.
  It never retries in a loop.
- **Pass-through:** `list_tools`/`call_tool` go 1:1 to ZEP; a 401 renews the token and
  repeats the call once. The tool list is fetched at startup, and when ZEP sends
  `tools/list_changed` the broker refetches it and sends `list_changed` itself.
  `zep_profile` (the usual read-only tool of every quiver tool) is added.

If nothing works, every tool answers “ZEP-Anmeldung nötig – bitte im quiver-Fenster
unter Tools → ZEP auf „Anmelden“ klicken.” as text, not as an MCP error; `status` exits 1.

Decision record: [docs/adr/0004-zep-offizieller-mcp.md](docs/adr/0004-zep-offizieller-mcp.md).
## quiver-ssh (SSH und SFTP)

`quiver-ssh` runs commands on remote hosts and moves files over SFTP – without a
password sitting on disk after the first login and without the sudo password ever
leaving the machine in a command line, a log or a tool answer.

Setup (once per connection, in quiver): **Tools → SSH → Verbindungen** → add a
connection with host, port (default 22), user, the login password and – separately –
the sudo password. quiver keeps both passwords in the keychain and passes every
connection to the tool as JSON in `QUIVER_ZUGAENGE`.

### The key rule

A password is only used for the **first** login. On that login the tool

1. connects with the password,
2. creates an ed25519 key under `<daten>/quiver-tools/ssh/schluessel/<name>_ed25519`
   (0600, plus `.pub`),
3. appends the public key to `~/.ssh/authorized_keys` of the remote user (700/600,
   no duplicate, comment `quiver-ssh <name>`),
4. opens a **new connection that only knows the key**, and
5. only then keeps the key. If any step fails, the key files are removed, nothing is
   replaced and the call is aborted with a clear message (for example
   `PubkeyAuthentication no`).

From then on the connection uses the key only. If the server no longer knows it, the
tool says so and points at `ssh_schluessel_einrichten`; it never silently falls back
to the password. The login password can be deleted in the quiver window after the
first success.

### Host keys

Host keys are checked against `<daten>/quiver-tools/ssh/known_hosts` and
`~/.ssh/known_hosts` (hashed entries included). The first contact is trusted on first
use: the key is recorded and the answer names its fingerprint. A **changed** host key
is a hard error that names the old and new fingerprints plus the file and line to
delete – never an automatic overwrite.

### sudo

`ssh_exec` with `sudo: true` runs `sudo -S -p '' -- sh -c '<befehl>'` and writes the
sudo password to stdin. The password appears in no command line, no output, no error
text and no log. A wrong password is reported as such (from sudo's own output); a
missing one is a hint to fill it in in the quiver window.

### Tools

Every tool takes `verbindung`; with exactly one connection the name may be omitted.

| Tool | Kind | What it does |
|---|---|---|
| `ssh_status` | read | The connections: name, user@host:port, key set up (with fingerprint), sudo password set, host key known |
| `ssh_list` | read | A remote directory over SFTP: mode, size, name |
| `ssh_exec` | write | Run a command: stdout, stderr, exit code; `sudo`, `zeitlimit` (60 s, max 600), `max_zeichen` (8000 each) |
| `ssh_upload` | write | Copy a local file to the remote host (`lokal` → `entfernt`) |
| `ssh_download` | write | Copy a remote file to this machine (`entfernt` → `lokal`) |
| `ssh_schluessel_einrichten` | write | Force the key setup again (e.g. after a rebuilt server); replaces the old key only after a key-only login succeeded |

### Entry for mcp-quiver

SSH is the first tool with **several** accesses, so its entry uses `mehrfach` instead
of `felder` and has no environment of its own – the client passes the list in
`QUIVER_ZUGAENGE`:

```json
{
  "id": "ssh",
  "titel": "SSH",
  "herkunft": "eigen",
  "laufzeit": { "art": "stdio", "befehl": "/pfad/zu/quiver-ssh", "umgebung": {} },
  "felder": [],
  "mehrfach": {
    "titel": "Verbindungen",
    "einzahl": "Verbindung",
    "felder": [
      { "name": "host", "quelle": "client", "pflicht": true },
      { "name": "port", "quelle": "client", "pflicht": false, "standard": "22" },
      { "name": "benutzer", "quelle": "client", "pflicht": true },
      { "name": "anmelde_passwort", "quelle": "geheim", "pflicht": false },
      { "name": "sudo_passwort", "quelle": "geheim", "pflicht": false }
    ]
  }
}
```

Files live under `<daten>/quiver-tools/ssh/` (`<daten>` = `QUIVER_SSH_DATEN`, for tests
and special cases, otherwise the platform's data directory):

```
schluessel/web-01_ed25519      (+ .pub)   0600 / 0700
known_hosts                              0600
```

## Entwickeln

```bash
# alles prüfen (der Befehl vor jedem Commit)
cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo test --workspace

# Bibliothekseintrag des Beispiel-Werkzeugs
cargo run -p quiver-tools-kern --example demo -- eintrag

# Zugang prüfen – die Felder kommen als Umgebung, so wie quiver sie setzt
DEMO_URL=https://example.test DEMO_TOKEN=geheim \
  cargo run -p quiver-tools-kern --example demo -- testen

# MCP über stdio, so wie quiver das Tool startet
cargo run -p quiver-tools-kern --example demo
```

Tests laufen nur gegen Attrappen – bei Outline ein axum-Server auf `127.0.0.1`, bei
E-Mail die Traits `Postfach`/`Versand`, bei ZEP eine axum-Attrappe mit Metadaten, DCR,
Anmelde- und Zustimmungsseite, Token-Endpunkt und einem Mini-MCP-Server hinter Bearer –
und setzen die Umgebung als Paare im Speicher;
kein Test spricht mit einem echten Dienst, kein Test braucht einen Schlüsselbund. Der
SMTP-Teil wird zusätzlich gegen `lettre`s Stub-Transport geprüft (Kopfzeilen, Threading,
Empfänger). Die Ende-zu-Ende-Tests starten `quiver-outline`, `quiver-email` bzw.
`quiver-zep` als Kindprozess mit gesetzter und ohne Umgebung. `quiver-zep` läuft dabei mit
`ZEP_TRESOR=speicher`, damit kein Test je an den echten Schlüsselbund kommt.

Ein Test spricht auf Anforderung mit einem echten IMAP-Server: `tests/imap_echt.rs` ist
`#[ignore]` und wird über `EMAIL_TEST_*` gesteuert. Beispiel mit GreenMail im Docker
(siehe Kopf der Datei für alle Variablen):

```bash
docker run --rm -d --name greenmail \
  -e GREENMAIL_OPTS="-Dgreenmail.setup.test.all -Dgreenmail.hostname=0.0.0.0" \
  -p 3025:3025 -p 3143:3143 -p 3990:3990 -p 3993:3993 greenmail/standalone:latest

EMAIL_TEST_IMAP_HOST=127.0.0.1 EMAIL_TEST_IMAP_PORT=3993 \
EMAIL_TEST_IMAP_SICHERHEIT=tls EMAIL_TEST_BENUTZER=test@example.com \
EMAIL_TEST_PASSWORT=geheim EMAIL_TEST_ZERTIFIKAT_PRUEFEN=aus \
  cargo test -p quiver-email --test imap_echt -- --ignored
```
Tests laufen nur gegen Attrappen (axum auf `127.0.0.1`, für SSH ein echter
`russh`-Server im Testprozess) und setzen die Umgebung als Paare im Speicher; kein Test
spricht mit einem echten Dienst, kein Test braucht einen Schlüsselbund oder die echte
`~/.ssh` (`HOME` und `QUIVER_SSH_DATEN` zeigen auf temp-Ordner). Der Ende-zu-Ende-Test
startet `quiver-outline` bzw. `quiver-ssh` als Kindprozess mit gesetzter und ohne
Umgebung.

Aufbau, Bausteine und Datenfluss: [docs/architektur.md](docs/architektur.md).
Entscheidungen: [docs/adr/](docs/adr/).

## Lizenz

[MIT](LICENSE)
