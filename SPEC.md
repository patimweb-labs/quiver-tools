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
Rückleitung über Loopback (`http://127.0.0.1:<freier Port>`; nicht `localhost`, das Browser zuerst als `::1` auflösen können).
*Begründung:* Der User meldet sich im gewohnten Browser an (inkl. MFA, SSO, Conditional Access),
kein Client-Secret auf dem Gerät, von Microsoft für Desktop-Apps empfohlen.
*Preis:* Es braucht eine App-Registrierung in Entra ID – **einmal**, nicht pro Firma (siehe §2a).
*Verworfen:* Device Code Flow (Code abtippen, in vielen Tenants per Richtlinie gesperrt);
Client Credentials (App-Rechte statt User-Rechte – widerspricht dem Rechtemodell).

## 2a. Eine Registrierung für alle, Freigabe im Browser

**Festlegung:** `quiver-m365` ist **eine** Multi-Tenant-App (öffentlicher Client), registriert im
Tenant von neoimpulse.de, Client-ID `9ae8e0db-e05f-4db8-adde-83d70d416eeb` – eingebaut als Standard.
*Begründung:* Kein User und keine Firma muss etwas registrieren oder konfigurieren. Beim ersten Login
sieht der User die Zustimmungsseite von Microsoft („quiver-m365 möchte: …“) und gibt frei.
*Preis:* Der Herausgeber auf der Zustimmungsseite ist dieser Tenant. Firmen, die Usern keine eigene
Zustimmung erlauben (häufig bei `Mail.Send`, `Files.Read.All`), brauchen **einmal** eine Admin-Freigabe.
*Verworfen:* Client-IDs von Microsoft-eigenen Apps (z. B. Graph PowerShell) – gibt sich als Microsoft aus,
wird von vielen Tenants blockiert.

- **Admin-Freigabe im Browser:** `quiver-m365 freigeben [--tenant <id|domain>]` öffnet
  `https://login.microsoftonline.com/<tenant>/v2.0/adminconsent?client_id=<id>&scope=<Scopes>&redirect_uri=http://127.0.0.1:<port>`
  und wartet auf die Rückleitung (`admin_consent=True` bzw. `error`). Danach sieht kein User der Firma mehr eine Rückfrage.
- **Eigene Registrierung** bleibt möglich: `M365_CLIENT_ID` überschreibt den Standard.

**Festlegung (2026-11-23, Nutzer):** Die Anmeldung löst **der User im quiver-Fenster** aus
(Knopf „Anmelden“ an der Tool-Karte), **nie ein Agent**. mcp-quiver ruft dafür die Unterbefehle
`quiver-m365 anmelden | status | abmelden` (Exit 0 = angemeldet, 1 = nicht angemeldet, 2 = Fehler;
erste Zeile stdout = Konto). Im MCP-Betrieb öffnet `quiver-m365` nie einen Browser; ohne Token
antwortet jedes Tool mit „Nicht angemeldet – bitte im quiver-Fenster … „Anmelden“ klicken.“
Ohne quiver lässt sich der alte Weg mit `M365_BROWSER=erlaubt` wählen.

Ablauf der Anmeldung (`quiver-m365 anmelden`):

```
1. Klick auf „Anmelden“ im quiver-Fenster → quiver-m365 anmelden
2. quiver-m365 lauscht auf 127.0.0.1:<freier Port>, erzeugt state + PKCE-Verifier
3. öffnet den Browser: https://login.microsoftonline.com/<tenant>/oauth2/v2.0/authorize?...
4. User meldet sich an → Rückleitung http://127.0.0.1:<port>/?code=…&state=…
5. state prüfen, Code + Verifier gegen Token tauschen (…/oauth2/v2.0/token)
6. Refresh-Token in die Tresor-Datei, Access-Token nur im Speicher
7. Browser-Seite: „Angemeldet – Fenster kann geschlossen werden“ (Tokens: keine)
```

- **Warten:** Ein Tool-Aufruf wartet höchstens **45 s** auf die Anmeldung. Ist sie bis dahin nicht
  fertig, antwortet das Tool mit dem Text „Anmeldung im Browser geöffnet – bitte anmelden und den
  Aufruf wiederholen“. Der Lauscher läuft im Hintergrund weiter (max. 5 min); der nächste Aufruf
  nutzt das Ergebnis. *Begründung:* MCP-Clients brechen lange Aufrufe ab.
- **Erneuern:** Access-Token läuft ab → Refresh-Token benutzen. Graph antwortet 401 → einmal
  erneuern und wiederholen. Refresh scheitert (widerrufen, abgelaufen) → neue Anmeldung.
- **Speicher:** Tresor-Datei `<Datenordner>/quiver-m365/tresor`, Schlüssel `<tenant>/<client_id>`.
  Nie in Dateien, nie ins Log, nie in Tool-Antworten.
- **Abmelden:** Tool `abmelden` löscht das Refresh-Token.
- **Später (mcp-quiver SPEC §7):** Der quiver-Client übernimmt die Anmeldung und reicht das Token
  pro Aufruf herein. Dafür liest `quiver-m365` ein Token aus `M365_ACCESS_TOKEN`, wenn gesetzt,
  und meldet sich dann nicht selbst an.

## 3. Konfiguration (Umgebung)

| Variable | Pflicht | Bedeutung |
|---|---|---|
| `M365_CLIENT_ID` | nein | Standard: die eingebaute Client-ID (§2a); eigene Registrierung möglich |
| `M365_TENANT` | nein | Tenant-ID oder `organizations` (Standard) |
| `M365_SCOPES` | nein | Standard: siehe §4 |
| `M365_AUTHORITY` | nein | Standard `https://login.microsoftonline.com` (Tests: Attrappe) |
| `M365_GRAPH_URL` | nein | Standard `https://graph.microsoft.com/v1.0` (Tests: Attrappe) |
| `M365_ACCESS_TOKEN` | nein | fertiges Token von außen (siehe §2, später quiver) |
| `M365_TRESOR` | nein | `speicher` = Token nur im Speicher (Tests, Entwicklung) |
| `M365_BROWSER` | nein | `erlaubt` = Tool darf selbst den Browser öffnen (nur ohne quiver) |

In mcp-quiver: Eintrag mit Laufzeit `stdio` → `quiver-m365`, ohne Pflichtfelder. Optional Feld
`tenant` (Quelle `server`, Standard `organizations`) → `M365_TENANT=${tenant}`.

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
crates/anmeldung   PKCE-Ablauf, Loopback-Lauscher, Erneuern, Tresor (Datei/Speicher)
crates/graph       Graph-Client: GET/POST mit Token, 401 → erneuern, Modelle der Antworten
crates/mcp         rmcp-Server über stdio, die Tools aus §4; Programm `quiver-m365`
```

Richtung: `anmeldung ← graph ← mcp`.

## 6. Tests

Ohne echten Tenant: eine **Attrappe** (axum) spielt Identity-Provider (`/authorize`, `/token`)
und Graph (`/me`, `/me/messages`, …). `M365_AUTHORITY`/`M365_GRAPH_URL` zeigen darauf.
Der „Browser“ ist im Test eine Funktion, die die Authorize-URL direkt aufruft.

## 7. Offene Punkte

1. Admin-Freigabe im Tenant von neoimpulse.de für den ersten echten Test (`quiver-m365 freigeben`).
4. Herausgeber-Verifizierung (Verified Publisher) – macht die Zustimmungsseite vertrauenswürdiger und erlaubt in vielen Tenants die User-Zustimmung.
2. Teams-Chats und Kanäle (`Chat.ReadWrite`, `ChannelMessage.Send`) – nach dem MVP.
3. Übergabe der Anmeldung an den quiver-Client (§2 „Später“).
