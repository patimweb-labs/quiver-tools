# SPEC – quiver-m365

> Stand: Entwurf 0, 2026-11-23. Jede Festlegung mit Begründung und Preis.

## 1. Zweck

Ein MCP-Server für **Microsoft 365 über Microsoft Graph**, als eigenes Tool für die
[mcp-quiver](https://github.com/patimweb-labs/mcp-quiver)-Bibliothek. Agenten (pi, Hermes,
Claude Code, …) lesen und schreiben Mail, Kalender und Dateien **mit den Rechten des
angemeldeten Users** – Microsoft prüft die Rechte, nicht wir.

Läuft auch ohne quiver: ein Programm `quiver-m365`, MCP über stdio.

## 2. Anmeldung: Browser, Authorization Code + PKCE

**Festlegung:** OAuth 2.0 Authorization Code Flow mit PKCE (S256) als *öffentlicher Client*,
Rückleitung über Loopback (`http://localhost:<freier Port>`).
*Begründung:* Der User meldet sich im gewohnten Browser an (inkl. MFA, SSO, Conditional Access),
kein Client-Secret auf dem Gerät, von Microsoft für Desktop-Apps empfohlen.
*Preis:* Es braucht eine App-Registrierung in Entra ID (siehe `docs/entra-app.md`).
*Verworfen:* Device Code Flow (Code abtippen, in vielen Tenants per Richtlinie gesperrt);
Client Credentials (App-Rechte statt User-Rechte – widerspricht dem Rechtemodell).

Ablauf:

```
1. Tool-Aufruf, kein gültiges Token
2. quiver-m365 lauscht auf 127.0.0.1:<freier Port>, erzeugt state + PKCE-Verifier
3. öffnet den Browser: https://login.microsoftonline.com/<tenant>/oauth2/v2.0/authorize?...
4. User meldet sich an → Rückleitung http://localhost:<port>/?code=…&state=…
5. state prüfen, Code + Verifier gegen Token tauschen (…/oauth2/v2.0/token)
6. Refresh-Token in den Schlüsselbund, Access-Token nur im Speicher
7. Browser-Seite: „Angemeldet – Fenster kann geschlossen werden“ (Tokens: keine)
```

- **Warten:** Ein Tool-Aufruf wartet höchstens **45 s** auf die Anmeldung. Ist sie bis dahin nicht
  fertig, antwortet das Tool mit dem Text „Anmeldung im Browser geöffnet – bitte anmelden und den
  Aufruf wiederholen“. Der Lauscher läuft im Hintergrund weiter (max. 5 min); der nächste Aufruf
  nutzt das Ergebnis. *Begründung:* MCP-Clients brechen lange Aufrufe ab.
- **Erneuern:** Access-Token läuft ab → Refresh-Token benutzen. Graph antwortet 401 → einmal
  erneuern und wiederholen. Refresh scheitert (widerrufen, abgelaufen) → neue Anmeldung.
- **Speicher:** Schlüsselbund, Dienst `quiver-m365`, Schlüssel `<tenant>/<client_id>`.
  Nie in Dateien, nie ins Log, nie in Tool-Antworten.
- **Abmelden:** Tool `abmelden` löscht das Refresh-Token.
- **Später (mcp-quiver SPEC §7):** Der quiver-Client übernimmt die Anmeldung und reicht das Token
  pro Aufruf herein. Dafür liest `quiver-m365` ein Token aus `M365_ACCESS_TOKEN`, wenn gesetzt,
  und meldet sich dann nicht selbst an.

## 3. Konfiguration (Umgebung)

| Variable | Pflicht | Bedeutung |
|---|---|---|
| `M365_CLIENT_ID` | ja | Application (client) ID der App-Registrierung |
| `M365_TENANT` | nein | Tenant-ID oder `organizations` (Standard) |
| `M365_SCOPES` | nein | Standard: siehe §4 |
| `M365_AUTHORITY` | nein | Standard `https://login.microsoftonline.com` (Tests: Attrappe) |
| `M365_GRAPH_URL` | nein | Standard `https://graph.microsoft.com/v1.0` (Tests: Attrappe) |
| `M365_ACCESS_TOKEN` | nein | fertiges Token von außen (siehe §2, später quiver) |
| `M365_TRESOR` | nein | `speicher` = Token nur im Speicher (Tests, Entwicklung) |

In mcp-quiver: Eintrag mit Feldern `client_id` und `tenant` (Quelle `server`), Laufzeit
`stdio` → `quiver-m365`, Umgebung `M365_CLIENT_ID=${client_id}`, `M365_TENANT=${tenant}`.

## 4. Tools (MVP)

Delegierte Berechtigungen: `User.Read Mail.ReadWrite Mail.Send Calendars.ReadWrite Files.Read.All offline_access`.

| Tool | Graph | Hinweis |
|---|---|---|
| `profil` | `GET /me` | Name, Mail, Abteilung |
| `mails_suchen` | `GET /me/messages` (`$search`, `$top` ≤ 50, `$select`) | Betreff, Absender, Datum, Vorschau, id |
| `mail_lesen` | `GET /me/messages/{id}` | Text statt HTML (Header `Prefer: outlook.body-content-type="text"`) |
| `mail_senden` | `POST /me/sendMail` | an, cc, betreff, text; **verändernd** (MCP-Annotation `destructiveHint`) |
| `termine` | `GET /me/calendarView` (`startDateTime`, `endDateTime`) | Zeitzone: `Prefer: outlook.timezone="Europe/Berlin"` (Umgebung `M365_ZEITZONE`) |
| `termin_anlegen` | `POST /me/events` | betreff, start, ende, ort, teilnehmer; **verändernd** |
| `dateien_suchen` | `GET /me/drive/root/search(q='…')` | Name, Pfad, Link, geändert |
| `abmelden` | – | Refresh-Token löschen |

Antworten sind knappes JSON (nur die genannten Felder), damit Agenten wenig Token verbrauchen.
Fehler von Graph kommen als MCP-Fehler mit Graph-Code und Text (ohne Tokens).

## 5. Aufbau

```
crates/anmeldung   PKCE-Ablauf, Loopback-Lauscher, Erneuern, Tresor (Schlüsselbund/Speicher)
crates/graph       Graph-Client: GET/POST mit Token, 401 → erneuern, Modelle der Antworten
crates/mcp         rmcp-Server über stdio, die Tools aus §4; Programm `quiver-m365`
```

Richtung: `anmeldung ← graph ← mcp`.

## 6. Tests

Ohne echten Tenant: eine **Attrappe** (axum) spielt Identity-Provider (`/authorize`, `/token`)
und Graph (`/me`, `/me/messages`, …). `M365_AUTHORITY`/`M365_GRAPH_URL` zeigen darauf.
Der „Browser“ ist im Test eine Funktion, die die Authorize-URL direkt aufruft.

## 7. Offene Punkte

1. App-Registrierung in eurem Tenant (wer legt an, Admin-Consent für `Mail.Send` usw.?).
2. Teams-Chats und Kanäle (`Chat.ReadWrite`, `ChannelMessage.Send`) – nach dem MVP.
3. Übergabe der Anmeldung an den quiver-Client (§2 „Später“).
