# quiver-tools

> **Status: in Arbeit.** Spezifikation in [SPEC.md](SPEC.md).

Eigene MCP-Tools für [mcp-quiver](https://github.com/patimweb-labs/mcp-quiver) in reinem Rust:
**Outline-Wiki**, **E-Mail** (IMAP/SMTP), **ZEP** (über den offiziellen ZEP-MCP), **SSH/SFTP**,
**ABAP** (SAP ADT), **Azure DevOps** und **Excalidraw** (lokal auf Dateien). Die Zugangsdaten trägt
der User **im quiver-Fenster** ein – unter Tools → <Titel>. quiver legt Geheimes in die Tresor-Datei
und setzt beim Start die Umgebung des Tools; Passwörter und API-Keys gehen nie durch einen Agenten.

## Der Zugang

Jedes Tool bringt seinen Bibliothekseintrag mit:

```bash
quiver-outline eintrag
```

Das JSON hat das Format von `quiver_domain::Eintrag`: Name, Titel, Beschreibung, Version, die Felder
des Zugangs und wie das Programm gestartet wird. quiver zeigt daraus unter **Tools → Outline-Wiki**
die Felder an (Adresse und API-Schlüssel). Offene Felder stehen in der Client-Konfiguration, geheime
im Tresor-Datei des Geräts.

Beim Start setzt quiver die Umgebung: `OUTLINE_URL`, `OUTLINE_API_KEY` – also `<NAME>_<FELD>` in
Großbuchstaben. Der Kern liest nur diese Umgebung, kein Profil aus einer Datei, keine Seite im Browser.

- Es gibt **einen** Zugang je Tool (`standard`). Wer zwei Wikis braucht, braucht zwei Einträge.
  **Ausnahme: quiver-ssh, quiver-abap und quiver-devops.** Dort pflegt der User eine Liste
  (`Eintrag.mehrfach`) – Verbindungen, SAP-Systeme bzw. DevOps-Organisationen; quiver übergibt sie
  als JSON in `QUIVER_ZUGAENGE`, die Felder stehen im Eintrag unter `mehrfach`.
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
  `0` means no limit) and says how much it left out. It **never writes a file**; that is
  what `outline_export` is for.
- `outline_export` writes into the working folder from the quiver window (field
  „Arbeitsordner“, a folder chooser). `target_dir` is an **optional relative
  subfolder**; absolute paths and `..` are refused, existing files are never
  overwritten, and symlinks are never followed. File names are derived from the title,
  never from the id alone.
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
      "OUTLINE_API_KEY": "${api_key}",
      "OUTLINE_UNVERSCHLUESSELT_ERLAUBEN": "${unverschluesselt_erlauben}",
      "OUTLINE_CA_ZERTIFIKAT": "${ca_zertifikat}",
      "OUTLINE_ORDNER": "${ordner}",
      "OUTLINE_SCHREIBEN": "${schreiben}"
    }
  },
  "felder": [
    {
      "name": "url",
      "titel": "Basis-URL",
      "beschreibung": "z. B. https://wiki.firma.de oder https://firma.getoutline.com",
      "art": "url",
      "quelle": "client",
      "pflicht": true,
      "standard": null
    },
    {
      "name": "api_key",
      "titel": "API-Token",
      "beschreibung": "In Outline unter Einstellungen → API & Access erstellen, beginnt mit ol_api_",
      "art": "passwort",
      "quelle": "geheim",
      "pflicht": true,
      "standard": null,
      "link": {
        "text": "Token in Outline erstellen",
        "url": "${url}/settings/api-and-access"
      }
    },
    { "name": "unverschluesselt_erlauben", "art": "schalter", "standard": "aus" },
    { "name": "ca_zertifikat", "art": "text", "standard": null },
    { "name": "ordner", "art": "ordner", "standard": null },
    { "name": "schreiben", "art": "schalter", "standard": "aus" }
  ]
}
```

`titel` is the label quiver shows, `beschreibung` is only the help text below the field, and
`art` says how the field is drawn: `text`, `passwort`, `zahl`, `schalter`, `url` or
`auswahl` (`{ "auswahl": { "optionen": [ { "wert", "anzeige" } ] } }`). A field of art
`passwort` is always secret, whatever its `quelle`. Tools with several accounts put the same
fields under `mehrfach.felder`.

`quelle: "geheim"` heißt: quiver legt den Wert in die Tresor-Datei und gibt ihn nur als
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

The password lives in the encrypted vault file and reaches the tool only as `EMAIL_PASSWORT`; it
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
| `url` | yes | MCP address, e.g. `https://www.zep-online.de/firma/next/mcp` |
| `benutzer` | yes | ZEP user name |
| `passwort` | yes | ZEP password; lives in the encrypted vault file and reaches the tool only as `ZEP_PASSWORT` |

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
- **Tokens:** the (rotating) refresh token in the encrypted vault file (`tresor/` in the state folder, key
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
the sudo password. quiver keeps both passwords in the encrypted vault file and passes every
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
Commands and file transfer need the per-connection switch `schreiben` (default off).
`ssh_upload` and `ssh_download` only exist when the connection has an
`austauschordner` (folder chooser); both sides of a transfer are plain relative names
inside it, so neither a local path nor a remote path can leave the exchange area.

| Tool | Kind | What it does |
|---|---|---|
| `ssh_status` | read | The connections: name, user@host:port, key set up (with fingerprint), sudo password set, host key known |
| `ssh_list` | read | A remote directory over SFTP: mode, size, name |
| `ssh_exec` | write | Run a command: stdout, stderr, exit code; `sudo`, `zeitlimit` (60 s, max 600), `max_zeichen` (8000 each) |
| `ssh_upload` | write | Copy a file from the exchange folder to the remote host (`lokal`, relative to the folder → `entfernt`, relative) |
| `ssh_download` | write | Copy a remote file (`entfernt`, relative) into the exchange folder (`lokal`, relative); never overwrites |
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

## quiver-abap (ABAP over SAP ADT)

`quiver-abap` works with ABAP objects over the SAP ADT REST service: it reads objects,
table contents, syntax messages, unit tests, ATC findings, dumps, traces and transports,
and it writes source code, activates, creates objects and maintains text elements.

Setup (once per system, in quiver): **Tools → ABAP → Systeme** → add a system with the
SAP user, the ADT address (e.g. `https://sap-dev.example.com:44300`), the client
(mandant), the logon language (default `DE`), the SAP password and – only for a system
you really want to change – the `schreiben` switch. quiver keeps the password in the
encrypted vault file and passes every system to the tool as JSON in `QUIVER_ZUGAENGE`,
exactly like `quiver-ssh`.

### Multiple systems, one list

The entry uses `mehrfach`, so it has no `felder` and no `umgebung` of its own:

```json
{
  "id": "abap",
  "titel": "ABAP",
  "herkunft": "eigen",
  "laufzeit": { "art": "stdio", "befehl": "/pfad/zu/quiver-abap", "umgebung": {} },
  "felder": [],
  "mehrfach": {
    "titel": "Systeme",
    "einzahl": "System",
    "felder": [
      { "name": "benutzer", "quelle": "client", "pflicht": true },
      { "name": "url", "quelle": "client", "pflicht": true },
      { "name": "mandant", "quelle": "client", "pflicht": true },
      { "name": "sprache", "quelle": "client", "pflicht": false, "standard": "DE" },
      { "name": "passwort", "quelle": "geheim", "pflicht": true },
      { "name": "zertifikat_pruefen", "quelle": "client", "pflicht": false, "standard": "an" },
      { "name": "schreiben", "quelle": "client", "pflicht": false, "standard": "aus" }
    ]
  }
}
```

Every tool takes `system`; with exactly one system the name may be omitted.
`abap_status` lists the systems without a password, `abap_test_connection` logs on and
reports how many ADT areas the system announces.

### TLS, logon and CSRF

- **Company CAs work out of the box.** The client uses rustls with the roots of the
  system certificate store (`rustls-native-certs`). `Serverzertifikat prüfen` stays on;
  only switch it off if your CA really is not in the store.
- **Logon** is HTTP Basic with `sap-client` (mandant) and `sap-language` as query
  parameters, plus a cookie jar per system (`SAP_SESSIONID_*`).
- **CSRF** as ADT wants it: the first call fetches a token with `x-csrf-token: fetch`;
  a `403` with `x-csrf-token: Required` fetches a new token and repeats the call **once**.
  In a writing session the token is refreshed **inside** that session, so the lock stays
  valid.
- **401 is final.** A rejected password is reported as such and never retried – SAP
  locks users after too many failed logons.
- **ADT exceptions** (`exc:exception`) reach the agent as type plus message, never as a
  bare HTTP code. Passwords and cookies appear in no answer, no error text and no log.

### Tools

Nineteen tools are read-only (`read_only_hint`), including syntax check, unit tests and
ATC: they compute, they change nothing. `abap_lock` and `abap_unlock` are neither
read-only nor destructive, the other four writing tools carry `destructive_hint`.

| Tool | What it does |
|---|---|
| `abap_status` | The systems from the quiver window: name, user@host, client, language, certificate check. No connection, no password |
| `abap_test_connection` | Log on and report the ADT areas; without `system` it checks all systems |
| `abap_search` | Search objects by pattern (`query`, `objectType`, `max`); returns name, type, package, description and object URL |
| `abap_object_structure` | Metadata plus includes and the address of the main source (`name` or `objectUrl`) |
| `abap_read` | The main source, shortened via `max_zeichen` (default 20000) with a note |
| `abap_where_used` | Where-used list of an object |
| `abap_object_types` | The object types search can filter on (`CLAS`, `PROG`, `FUGR`, …) |
| `abap_node_contents` | Contents of a package or function group (`parentType` `DEVC/K`, `parentName`; `$TMP` = local objects) |
| `abap_table` | First rows of a DDIC table or view; `filter` is a SQL condition without WHERE; `max_zeilen` default 100, max 1000 |
| `abap_query` | Free ABAP SQL SELECT (data preview); `max_zeilen` default 100, max 1000 |
| `abap_syntax_check` | Syntax check of the saved main source or of a given `source` – nothing is saved |
| `abap_unit_test` | ABAP Unit run; by default only short, harmless tests, more via `dangerous`/`critical`/`medium`/`long` |
| `abap_atc` | ATC run for an object with findings, priority, check and line (`variant`, default `DEFAULT`) |
| `abap_dumps` | Short dumps, optionally filtered by `query` |
| `abap_traces` | Performance traces of a user (default: the logged-on user) |
| `abap_trace_hitlist` | Hot spots and the aggregated statement tree of a trace (`traceId`) |
| `abap_transports` | Transport requests of a user, grouped by target |
| `abap_transport_details` | One request with status, owner, tasks and objects (`transportNumber`) |
| `abap_text_elements` | Text symbols, selection texts or headings (`category` `symbols`/`selections`/`headings`) |
| `abap_lock` | Lock an object and return the lock handle (`accessMode` default `MODIFY`) |
| `abap_unlock` | Release a lock again (`lockHandle`) |
| `abap_write` | Save the full source (`source`, `transport`, optional `lockHandle`) |
| `abap_activate` | Activate an object and show the messages (`preauditRequested` default true) |
| `abap_create_object` | Create `CLAS/OC`, `INTF/OI`, `PROG/P`, `PROG/I`, `FUGR/F`, `TABL/DT`, `DTEL/DE`, `DOMA/DD` or `DDLS/DF` in a package |
| `abap_set_text_elements` | Write text elements (`category`, `elements` with `id`, `text`, `maxLength`) |
| `abap_profile` | The systems of the entry, without passwords (the core's profile tool; a single profile exists only without `mehrfach`) |

Parameter names follow the template (`query`, `objectType`, `objectUrl`, `parentType`,
`traceId`, `transportNumber`, `category`, `lockHandle`, `accessMode`, `preauditRequested`);
`type` is accepted as an alias for `typ`. Only `max_zeichen` and `max_zeilen` are German,
as the task requires.

### Writing: three guards before any request

1. **`schreiben` per system, default off.** If it is off, every writing tool answers
   „Schreiben ist für <system> aus – im quiver-Fenster unter Tools → ABAP → Systeme →
   <system> ‚schreiben‘ einschalten.“ and **no request goes to SAP**. Nothing is written
   to a production system by accident.
2. **Customer namespace only.** Writing, locking and creating only touch objects named
   `Z*`, `Y*` or `/namespace/name`; for `abap_create_object` the package must be `$TMP`
   or a customer package as well. Everything else is rejected without a request.
3. **`source` up to 2 MB**, otherwise an error without a request.

Locks live in **one stateful session per system** inside the process
(`X-sap-adt-sessiontype: stateful` plus `SAP_SESSIONID_*`): a lock handle is only valid
in the session that obtained it. `abap_write` without `lockHandle` locks, writes and
unlocks (always, even when the write fails); with a `lockHandle` that did not come from
this session it answers „Lock-Handle unbekannt – erst abap_lock aufrufen“ without asking
SAP. If the session times out, the session and its lock table are dropped and the answer
says „Sitzung abgelaufen, Locks verloren – erneut abap_lock“ – a write is **never**
retried silently. When the process ends (stdin closed or SIGTERM) all known locks are
released, at most 5 s, errors only to the log.

Decision record: [docs/adr/0005-adt-client-in-rust.md](docs/adr/0005-adt-client-in-rust.md)
and [docs/adr/0006-abap-schreiben.md](docs/adr/0006-abap-schreiben.md).

## quiver-devops (Azure DevOps)

`quiver-devops` works with Azure DevOps over its REST API (version 7.1): it reads work
items, comments, iterations, repositories, branches, pull requests, pipelines and runs,
and it changes work items, comments, links, pull requests, votes and pipeline runs –
but only after the user allowed writing, and only per organisation.

Setup (once per organisation, in quiver): **Tools → DevOps → Organisationen** → add an
organisation with its address (e.g. `https://dev.azure.com/beispiel` or an Azure DevOps
Server like `https://tfs.customer.example/tfs/DefaultCollection`), an optional default
project and a Personal Access Token (**User settings → Personal access tokens**; the
entry carries a link to that page). quiver keeps the PAT in the encrypted vault file and
passes every organisation to the tool as JSON in `QUIVER_ZUGAENGE`.

### Organisations, one list

```json
{
  "id": "devops",
  "titel": "DevOps",
  "herkunft": "eigen",
  "laufzeit": { "art": "stdio", "befehl": "/pfad/zu/quiver-devops", "umgebung": {} },
  "felder": [],
  "mehrfach": {
    "titel": "Organisationen",
    "einzahl": "Organisation",
    "felder": [
      { "name": "url", "quelle": "client", "pflicht": true },
      { "name": "projekt", "quelle": "client", "pflicht": false },
      { "name": "pat", "quelle": "geheim", "pflicht": true, "link": { "text": "PAT erstellen", "url": "${url}/_usersSettings/tokens" } },
      { "name": "schreiben", "quelle": "client", "pflicht": false, "standard": "aus" }
    ]
  }
}
```

Every tool takes `verbindung`; with exactly one organisation the name may be omitted.
`projekt` may be given per call; if neither the access nor the call names a project, the
tool answers with a hint and sends **no** request. `devops_status` lists the
organisations without a PAT, `devops_testen` logs on (`_apis/connectionData`) and tells
who is signed in.

### Token, errors and rate limits

- **Auth** is HTTP Basic with an empty user and the PAT as password:
  `Authorization: Basic base64(":" + PAT)`. That header is the only place the PAT ever
  appears – no answer, error text, log or test output contains it.
- **`api-version`** rides on every request (`7.1`; the comment endpoints use
  `7.1-preview.4` and `_apis/connectionData` uses `7.1-preview.1`, where DevOps still
  requires the preview – plain `7.1` answers HTTP 400, "The resource is under preview").
- **401, or 203 with an HTML login page** (DevOps redirects a rejected token), becomes
  „PAT für <verbindung> ungültig oder abgelaufen – im quiver-Fenster unter Tools → DevOps
  erneuern.“ and is **not** retried.
- **429** is retried at most twice, honouring `Retry-After`, and **only for reading
  calls**. A create or update is never repeated: only the server knows whether it landed.
- Work item fields, descriptions and comments are HTML; the tools turn them into text
  and shorten them via `max_zeichen` (default 4000) with a note on what was dropped.

### Tools

Eighteen tools are read-only (`read_only_hint`), eight change something
(`destructive_hint`) and need the `schreiben` switch.

| Tool | Kind | What it does |
|---|---|---|
| `devops_status` | read | The organisations from the quiver window: name, address, default project, writing on/off. No request, no PAT |
| `devops_testen` | read | Log on and report who is signed in and in which organisation |
| `devops_projekte` | read | Projects with state and visibility |
| `devops_workitems_suchen` | read | WIQL search (`wiql`) or the convenient `text`, `status`, `typ`, `zugewiesen_an_mich`; `top` default 50, max 200. Fields come from `workitemsbatch` |
| `devops_workitem` | read | One work item with description, acceptance criteria and links (`$expand=relations`) |
| `devops_workitem_kommentare` | read | All comments with author and date |
| `devops_workitem_typen` | read | The project's work item types with their states |
| `devops_iterationen` | read | A team's sprints (`team` required), optionally only the current one (`aktuell`) |
| `devops_iteration_workitems` | read | The work items of one iteration |
| `devops_repos` | read | Git repositories with default branch and size |
| `devops_branches` | read | Branches of a repository with short SHA and creator |
| `devops_pull_requests` | read | Pull requests (`status` default `active`), optionally per repo or creator |
| `devops_pull_request` | read | One pull request with reviewers and votes, linked work items and at most 20 commits |
| `devops_pull_request_threads` | read | Comment threads, without the system threads DevOps creates for votes and pushes |
| `devops_pipelines` | read | YAML pipelines with folder |
| `devops_laeufe` | read | The latest runs (`top` default 10), filtered by pipeline and branch |
| `devops_lauf` | read | One run with state, times and the failed steps from the timeline (with log number) |
| `devops_lauf_log` | read | The **end** of a step's log, default the last 200 lines |
| `devops_workitem_anlegen` | write | Create a work item: type, title, description, assignee, area, iteration, parent |
| `devops_workitem_aendern` | write | Change fields as a list `{feld, wert}` or via `status`, `titel`, `zugewiesen` |
| `devops_workitem_kommentieren` | write | Add a comment to a work item |
| `devops_workitem_verknuepfen` | write | Link two work items (`art` `eltern`, `kind`, `verwandt`) or remove the link again |
| `devops_pull_request_anlegen` | write | Create a pull request: source, target, title, description, draft, optional work items (linked as `ArtifactLink`) |
| `devops_pull_request_kommentieren` | write | A new comment thread, or a reply in an existing one (`thread`) |
| `devops_pull_request_abstimmen` | write | Vote `approve`, `approve-with-suggestions`, `wait`, `reject` or `reset` as the signed-in user |
| `devops_pipeline_starten` | write | Queue a pipeline run with branch, variables and template parameters |
| `devops_profile` | read | The organisations of the entry, without PATs |

### Writing: one guard before any request

`schreiben` is **off** by default, per organisation. While it is off, every one of the
eight writing tools answers „Schreiben ist für <verbindung> aus – im quiver-Fenster
einschalten.“ and **no request goes to DevOps**. A customer organisation stays read-only
until someone switches it on in the quiver window.

Decision record: [docs/adr/0007-devops-mit-pat.md](docs/adr/0007-devops-mit-pat.md).

## quiver-m365 (Microsoft 365 über Graph)

Mail, Kalender, Dateien und **Teams** über Microsoft Graph, mit **einer** Anmeldung im
Browser. Die Anmeldung löst der Knopf „Anmelden“ im quiver-Fenster aus (Tools → Microsoft 365),
**nie** ein Agent. Das Refresh-Token liegt in der Tresor-Datei, und quiver-m365 erneuert es
selbst. Crates: `crates/m365-anmeldung` (PKCE, Loopback, Tresor), `crates/m365-graph`
(Graph-Client) und `crates/m365` (Programm `quiver-m365`, MCP-Tools).

Changing something needs a switch per area, all **off** by default:
`M365_SCHREIBEN_MAIL`, `_KALENDER`, `_DATEIEN`, `_TEAMS`, `_PLANNER`, `_KONTAKTE`,
`_ONENOTE` – or the general `M365_SCHREIBEN`. The check sits in front of the MCP
dispatcher, so a reading tool never needs a switch and a writing tool never runs without
one; the hint names the field. Reading tools have no side effects (no “mark as read”, no
saving); `teams_als_gelesen` is a writing tool and stays behind the Teams switch.

Ausführlich: [docs/m365/README.md](docs/m365/README.md), App-Registrierung und Freigabe:
[docs/m365/entra-app.md](docs/m365/entra-app.md), Spezifikation:
[docs/m365/SPEC.md](docs/m365/SPEC.md). Bis 2026-11-24 war quiver-m365 ein eigenes Repo; die
Historie ist hier übernommen.

## quiver-excalidraw (Excalidraw-Dateien)

Das erste der beiden Diagramm-Werkzeuge: es erzeugt, liest und ändert `.excalidraw`-Dateien
in einem **Arbeitsordner** und zeigt dem Agenten das Ergebnis als **PNG**. Reines Rust –
kein Node, kein Netz, kein Browser im Hintergrund. Crates: `crates/diagramm` (Ablage,
Modell, Layout, Zeichnen) und `crates/excalidraw` (Programm `quiver-excalidraw`).
Aufgabe: [docs/aufgaben/17-excalidraw.md](docs/aufgaben/17-excalidraw.md), Entscheidung:
[docs/adr/0011-excalidraw.md](docs/adr/0011-excalidraw.md).

Ausführlich: [crates/excalidraw/README.md](crates/excalidraw/README.md).

### Three fields, no credentials

| Feld | Art | Pflicht | Standard |
|---|---|---|---|
| `ordner` | Text | ja | – (voller Pfad, `~` wird aufgelöst, wird angelegt) |
| `schreiben` | Schalter | nein | **an** |

There is nothing to sign in to. Everything the tool touches lives inside the working
folder: relative paths only, no `..`, no absolute paths, no symlinks, no hidden names, no
overwriting, no deleting. Verändern geht nur mit dem gelesenen `stand` (SHA-256).

### Tools

| Tool | Kind | What it does |
|---|---|---|
| `excalidraw_status` | read | working folder, number of files, `schreiben` on/off |
| `excalidraw_liste` | read | every `.excalidraw` file, also in subfolders, with size and time |
| `excalidraw_lesen` | read | per element: id, type, text, position, bindings, plus `stand`; `roh: true` for the source text |
| `excalidraw_bild` | read | the scene as a **PNG image**, and what could not be drawn |
| `excalidraw_oeffnen` | read | opens `https://excalidraw.com` and names the full path; `nur_link: true` opens nothing |
| `excalidraw_erstellen` | write | a diagram from `modell` (`knoten`, `kanten`, `gruppen`, `richtung`) |
| `excalidraw_aus_quelltext` | write | a finished file, checked strictly |
| `excalidraw_elemente_aendern` | write | per id: `text`, `farbe`, `form`, `x`/`y`/`breite`/`hoehe`, `verbindungen` |
| `excalidraw_elemente_hinzufuegen` | write | `rectangle`, `diamond`, `ellipse`, `text`, `line`, `arrow`, `frame`, `image` |
| `excalidraw_elemente_entfernen` | write | removes elements and resolves both sides of every binding |
| `excalidraw_exportieren` | write | SVG or PNG into the working folder |
| `excalidraw_profile` | read | the entry from the environment |

### What the file must contain

`type "excalidraw"`, `version 2`, `source`, `elements`, `appState`, `files` – and every
element with all **25 required fields** of the format sources. Text in a shape is bound
twice (`containerId` and `boundElements`), an arrow likewise (`startBinding`/`endBinding`
and `boundElements`). `link` only `https`, `http`, `mailto`. Everything the tool does not
understand – images in `files`, `customData`, whole elements of unknown type – is carried
through unchanged. Drawing is done by the crate itself, with the embedded font; nothing is
uploaded, and `excalidraw_oeffnen` only opens the empty page – the file is dragged into
the window by hand.

## Hardening: secure defaults (Paket 4)

Everything below is enforced by code and tests, not by documentation.

### Writing is off

Every tool that changes something has a switch in the quiver window. It is **off** by
default and lives next to the access it belongs to:

| Tool | Switch | Notes |
|---|---|---|
| `outline`, `email`, `zep` | `schreiben` | one access, one switch |
| `ssh` | `schreiben` | per connection; commands, key setup, upload, download |
| `abap`, `devops`, `jira`, `confluence`, `zephyr` | `schreiben` | already per access |
| `microsoft-365` | `M365_SCHREIBEN` plus one per area | mail, calendar, files, Teams, Planner/To Do, contacts, OneNote |

The check sits in front of the dispatcher (for most tools in the core, for Microsoft 365
in the program itself), so it cannot be forgotten by a single tool: anything that is not
annotated `read_only_hint = true` is blocked. The answer is a plain text hint that names
the field to switch on – for example *„outline_create“ ändert etwas. … bitte im
quiver-Fenster … das Feld „Schreiben erlauben“ einschalten.* – and **no request leaves
the machine**. An access that worked before therefore never fails silently after the
update: it says what to switch on. On the first start after the update the program also
writes “Schreiben ist jetzt aus …” to stderr (the log quiver shows).

The ZEP broker does not simply trust the remote annotations: a fixed list of read-only
tool names lives in the program (`zep_projekte`, `zep_zeiten`, `zep_kunden`,
`zep_profile`), everything else needs the switch. A 401 is retried **only** for those
read-only tools – a write is never sent twice.

`<tool>_profile` prints only an explicit allowlist of harmless fields (addresses, host,
port, project, folder, switches). A typo in a field type can no longer print a secret.

### Network

- One shared HTTP client (`quiver_tools_kern::net`): **no redirects**, a 60 s deadline and
  a hard **8 MB** limit per response (read in chunks, abort as an error).
- Addresses: `https` always. `http` only for `127.0.0.1`, `localhost`, `[::1]` – or when
  the user switches on **„Unverschlüsselt erlauben“** (`unverschluesselt_erlauben`,
  default off). Plain `http` is named as a warning in the status text and in the log.
- TLS: the certificate check can only be switched off for loopback. For any other host it
  stays on unless the user points **„CA-Zertifikat“** (`ca_zertifikat`) at a PEM file.
- Graph: `@odata.nextLink` and every absolute address are pinned to the Graph host
  (scheme, host, port); a foreign host is an error and is never fetched.

### Files, vault

- All writing goes through one `Ablage` with a fixed root: relative names only, no `..`,
  no absolute paths, no hidden names, no symlinks on any level, `create_new` (no
  overwriting), 10 MB per file.
- The tool vaults (`m365-anmeldung`, `zep`) never create a new key when the vault data is
  there but `tresor.schluessel` is missing: the call fails, both files stay untouched, and
  the message says to restore the key or sign in again in the quiver window.

### Tests

- A fuzz test (fixed seed, 20 000 generated inputs) feeds every reader: draw.io XML,
  compressed pages, styles, colours, Excalidraw JSON. No input may panic.
- A contract test over all programs checks that every tool is either annotated read-only
  or belongs to a program with a write switch (default off).
- The Azure DevOps and ABAP mocks answer in the *service shape*: REST 7.1 envelopes
  (`count`, `value`, `url`, `_links`, `rev`) and real ADT XML with **relative** URIs and
  version links. A tool that expects absolute URLs fails here.

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
Anmelde- und Zustimmungsseite, Token-Endpunkt und einem Mini-MCP-Server hinter Bearer,
bei ABAP eine ADT-Attrappe mit CSRF, Cookies, Sperren (nur die Sitzung, die gesperrt
hat, darf schreiben), Quelltext-PUT, Aktivierung und Anlegen, bei DevOps eine
Organisations-Attrappe mit Basic-Anmeldung, zwei Organisationen, 401 und 203 mit
Anmeldeseite sowie 429 nur für lesende Aufrufe –
und setzen die Umgebung als Paare im Speicher;
kein Test spricht mit einem echten Dienst, kein Test braucht eine Tresor-Datei. Der
SMTP-Teil wird zusätzlich gegen `lettre`s Stub-Transport geprüft (Kopfzeilen, Threading,
Empfänger). Die Ende-zu-Ende-Tests starten `quiver-outline`, `quiver-email` bzw.
`quiver-zep` als Kindprozess mit gesetzter und ohne Umgebung. `quiver-zep` läuft dabei mit
`ZEP_TRESOR=speicher`, damit kein Test je an die echte Tresor-Datei kommt.

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
spricht mit einem echten Dienst, kein Test braucht eine Tresor-Datei oder die echte
`~/.ssh` (`HOME` und `QUIVER_SSH_DATEN` zeigen auf temp-Ordner). Der Ende-zu-Ende-Test
startet `quiver-outline` bzw. `quiver-ssh` als Kindprozess mit gesetzter und ohne
Umgebung.

Aufbau, Bausteine und Datenfluss: [docs/architektur.md](docs/architektur.md).
Entscheidungen: [docs/adr/](docs/adr/).

## Lizenz

[MIT](LICENSE)
