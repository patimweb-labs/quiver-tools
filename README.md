# quiver-m365

> **Status: in Arbeit.** Spezifikation in [SPEC.md](SPEC.md).

**Microsoft 365 für jeden Agenten** – ein MCP-Server für Mail, Kalender und Dateien über
Microsoft Graph. Der User meldet sich **im Browser** an seinem Konto an (inkl. MFA); alle Aufrufe
laufen mit seinen eigenen Rechten. Reines Rust, ein Programm, MCP über stdio.

Gebaut als Tool für die [mcp-quiver](https://github.com/patimweb-labs/mcp-quiver)-Bibliothek,
läuft aber auch allein mit jedem MCP-Client.

## Tools

| Tool | Was es tut | Graph |
|---|---|---|
| `profil` | Name, Mail-Adresse, Abteilung | `GET /me` |
| `mails_suchen` | Suche im Postfach; ohne Suchtext die neuesten Mails im Posteingang | `GET /me/messages`, `GET /me/mailFolders/inbox/messages` |
| `mail_lesen` | eine Mail als Text (nicht HTML) | `GET /me/messages/{id}` |
| `mail_senden` | Mail senden (**verändernd**) | `POST /me/sendMail` |
| `termine` | Termine eines Zeitraums | `GET /me/calendarView` |
| `termin_anlegen` | Termin anlegen (**verändernd**) | `POST /me/events` |
| `dateien_suchen` | Dateien in OneDrive suchen | `GET /me/drive/root/search(q='…')` |
| `abmelden` | Refresh-Token löschen | – |

Antworten sind knappes JSON als Text. Läuft die Anmeldung noch im Browser, antwortet das Tool
nicht mit einem Fehler, sondern mit: „Anmeldung im Browser geöffnet – bitte anmelden und den
Aufruf danach wiederholen.“

## Installation

Voraussetzung: Rust (Version aus [rust-toolchain.toml](rust-toolchain.toml)).

```bash
cargo build --release
# Programm: target/release/quiver-m365
```

Oder direkt in den eigenen `PATH`:

```bash
cargo install --path crates/mcp
quiver-m365 --version
```

Es gibt **keine** App-Registrierung einzurichten und kein Secret: `quiver-m365` ist eine
eingebaute Multi-Tenant-App (öffentlicher Client). Details:
[docs/entra-app.md](docs/entra-app.md).

## Konfiguration

Alles über Umgebung, alles optional:

| Variable | Standard | Bedeutung |
|---|---|---|
| `M365_CLIENT_ID` | eingebaute Client-ID | eigene App-Registrierung (siehe [docs/entra-app.md](docs/entra-app.md)) |
| `M365_TENANT` | `organizations` | Tenant-ID oder Domain |
| `M365_SCOPES` | `User.Read Mail.ReadWrite Mail.Send Calendars.ReadWrite Files.Read.All offline_access` | delegierte Rechte |
| `M365_AUTHORITY` | `https://login.microsoftonline.com` | Anmeldedienst (Tests: Attrappe) |
| `M365_GRAPH_URL` | `https://graph.microsoft.com/v1.0` | Graph (Tests: Attrappe) |
| `M365_ZEITZONE` | `Europe/Berlin` | Zeitzone für Termine |
| `M365_ACCESS_TOKEN` | – | fertiges Token von außen; dann keine eigene Anmeldung |
| `M365_TRESOR` | Schlüsselbund | `speicher` = Token nur im Arbeitsspeicher (Tests, Entwicklung) |
| `RUST_LOG` | `info` | Logs gehen nach **stderr**, stdout gehört MCP |

## Eintrag für mcp-quiver

In der Admin-Oberfläche unter **Bibliothek → Neu** (oder per `PUT /api/v1/admin/eintraege/microsoft-365`):

```json
{
  "id": "microsoft-365",
  "version": "0.1.0",
  "titel": "Microsoft 365",
  "beschreibung": "Mail, Kalender und Dateien über Microsoft Graph – Anmeldung im Browser.",
  "herkunft": "eigen",
  "laufzeit": { "art": "stdio", "befehl": "quiver-m365" },
  "felder": []
}
```

Für einen bestimmten Tenant ein Feld `tenant` (Quelle `server`) anlegen und in der Laufzeit
`"umgebung": { "M365_TENANT": "${tenant}" }` setzen. Die Agenten sehen die Tools dann als
`microsoft-365__mails_suchen` usw.

## Firmenweite Freigabe (Admin, einmal pro Firma)

Beim ersten Tool-Aufruf öffnet sich die Anmeldung im Browser; Microsoft zeigt
„quiver-m365 möchte: …“ → *Akzeptieren*. Erlaubt der Tenant keine Zustimmung durch User
(häufig bei `Mail.Send`), gibt ein Admin **einmal** frei:

```bash
quiver-m365 freigeben --tenant firma.de
```

Der Befehl öffnet die Admin-Zustimmungsseite (Rechte mit Graph-Präfix, ohne `offline_access`) und
wartet auf die Rückleitung nach `http://127.0.0.1:<port>`. Danach sieht kein User der Firma mehr
eine Rückfrage.

## Wie es arbeitet

- **Anmeldung:** Authorization Code + PKCE im Browser, Rückleitung über einen Loopback-Lauscher auf
  `127.0.0.1` (kein `localhost`, das im Browser zuerst `::1` sein kann).
- **Speicher:** Refresh-Token im Schlüsselbund (Dienst `quiver-m365`), Access-Token nur im
  Arbeitsspeicher. Nie in Dateien, nie in Logs, nie in Tool-Antworten.
- **Erneuern:** abgelaufenes Token per Refresh-Token, 401 von Graph → einmal erneuern und
  wiederholen, 429/503 mit `Retry-After` → einmal nach der genannten Zeit (höchstens 10 s).
- **Rechte:** nur delegierte Graph-Berechtigungen, nie App-Berechtigungen. Microsoft prüft, nicht wir.
- **Daten:** quiver-m365 speichert nichts außer dem Refresh-Token im Schlüsselbund des Rechners.
  Mails, Termine und Dateien werden je Aufruf abgerufen und nur an den MCP-Client
  zurückgegeben, der den Aufruf gemacht hat. Kein Server dazwischen, keine Telemetrie.
  Das Tool `abmelden` löscht das Refresh-Token wieder.

## Entwicklung

```bash
# alles prüfen (der Befehl vor jedem Commit)
cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo test --workspace
```

Tests laufen nur gegen Attrappen (axum): Microsoft-Identity-Provider und Graph. Nie gegen echte
Microsoft-Server, nie gegen den echten Schlüsselbund, nie mit einem echten Browser.

Aufbau: `crates/anmeldung` (PKCE, Lauscher, Tresor) ← `crates/graph` (Graph-Client, Modelle) ←
`crates/mcp` (rmcp-Server über stdio, Programm `quiver-m365`).

## Lizenz

[MIT](LICENSE)
