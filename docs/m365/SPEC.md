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

**Festlegung (geändert 2026-11-26):** Die App-Registrierung (öffentlicher Client) gehört der Firma,
die quiver betreibt. Ihre Client-ID steht **nicht** im Programm: sie kommt über `M365_CLIENT_ID`, in
quiver als Server-Feld `client_id` des Eintrags, vorgegeben in der Verwaltung. Ohne sie startet
`quiver-m365` mit einer klaren Meldung.
Der Tenant wird hier nicht namentlich genannt; die Client-ID genügt.
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
| `M365_CLIENT_ID` | ja | Client-ID der App-Registrierung der Firma (§2a), in quiver vom Server |
| `M365_TENANT` | nein | Tenant-ID oder `organizations` (Standard) |
| `M365_SCOPES` | nein | Standard: siehe §4 |
| `M365_AUTHORITY` | nein | Standard `https://login.microsoftonline.com` (Tests: Attrappe) |
| `M365_GRAPH_URL` | nein | Standard `https://graph.microsoft.com/v1.0` (Tests: Attrappe) |
| `M365_ACCESS_TOKEN` | nein | fertiges Token von außen (siehe §2, später quiver) |
| `M365_TRESOR` | nein | `speicher` = Token nur im Speicher (Tests, Entwicklung) |
| `M365_BROWSER` | nein | `erlaubt` = Tool darf selbst den Browser öffnen (nur ohne quiver) |

In mcp-quiver: Eintrag mit Laufzeit `stdio` → `quiver-m365`, ohne Pflichtfelder. Optional Feld
`tenant` (Quelle `server`, Standard `organizations`) → `M365_TENANT=${tenant}`.

## 4. Tools (MVP und Teams)

Delegierte Berechtigungen (Mail, Kalender, Dateien):
`User.Read Mail.ReadWrite Mail.Send Calendars.ReadWrite Files.Read.All offline_access`.

**Teams (Aufgabe 05, 2026-11-24)** ergänzt delegiert:
`Team.ReadBasic.All Channel.ReadBasic.All ChannelMessage.Read.All ChannelMessage.Send
Chat.ReadWrite User.ReadBasic.All`; **Aufgabe 07 (2026-11-24)** ergänzt `TeamMember.Read.All`
(ohne dieses Recht antwortet Graph auf `GET /teams/{id}/members` mit 403).
*Begründung:* Alle Teams-Tools handeln **als der User**; es gibt bewusst keine App-Berechtigung
(`*.All` mit Application-Rechten), weil sie jedes Postfach und jeden Chat lesen dürfte.
*Preis:* `ChannelMessage.Read.All` und `ChannelMessage.Send` verlangen in vielen Tenants eine
Admin-Zustimmung. Kommt ein Scope später dazu, antwortet der Token-Endpunkt einmal mit
`AADSTS65001`/`consent_required`; das Tool sagt dann, der User solle im quiver-Fenster erneut auf
„Anmelden“ klicken – es öffnet keinen Browser und löscht das Refresh-Token nicht.

**Aufgabe 06 (2026-11-24)** ergänzt delegiert: `Sites.ReadWrite.All`,
`Files.ReadWrite.All`, `Tasks.ReadWrite`, `Group.Read.All`, `Contacts.ReadWrite`,
`People.Read`, `Notes.ReadWrite.All`.
*Begründung:* SharePoint/OneDrive, Planner/To Do, Kontakte/Personen und OneNote laufen alle über
Graph und handeln **als der User**; auch hier keine App-Berechtigung. `Sites.ReadWrite.All` und
`Files.ReadWrite.All` decken Lesen **und** Schreiben ab, weil Dateien und Listen angelegt werden.
*Preis:* Diese Scopes verlangen fast überall eine Admin-Zustimmung; die ist für den Herausgeber-Tenant
erteilt (`docs/entra-app.md`), andere Tenants geben einmal im Browser frei. Fehlt sie, sagt das
Tool es – ohne Browser und ohne das Refresh-Token zu löschen.

### Teams-Tools

| Tool | Annotation | Graph |
|---|---|---|
| `teams_eingang` | lesend | `/me/chats?$expand=lastMessagePreview` (Lesemarke `viewpoint`), Erwähnungen über `/search/query` |
| `teams_liste` | lesend | `GET /me/joinedTeams` |
| `teams_kanaele` | lesend | `GET /teams/{team}/channels` |
| `teams_kanal_lesen` | lesend | `GET …/channels/{kanal}/messages?$top=` (Standard 20, max. 50) |
| `teams_thread_lesen` | lesend | `GET …/messages/{id}` und `…/replies` |
| `teams_kanal_senden` | destruktiv | `POST …/channels/{kanal}/messages` |
| `teams_kanal_antworten` | destruktiv | `POST …/messages/{id}/replies` |
| `teams_chats` | lesend | `GET /me/chats?$expand=members&$top=` |
| `teams_chat_lesen` | lesend | `GET /chats/{chat}/messages?$top=` |
| `teams_chat_senden` | destruktiv | `POST /chats/{chat}/messages` |
| `teams_chat_anlegen` | destruktiv | `POST /chats` |
| `teams_suchen` | lesend | `POST /search/query` (`entityTypes: ["chatMessage"]`) |
| `teams_mitglieder` | lesend | `GET /teams/{team}/members` bzw. `…/channels/{kanal}/members` |
| `teams_person_finden` | lesend | `GET /users?$search="displayName:…" OR "mail:…"` |
| `teams_reagieren` | destruktiv | `POST …/setReaction` bzw. `unsetReaction` |
| `teams_als_gelesen` | weder lesend noch destruktiv | `POST /chats/{chat}/markChatReadForUser` bzw. `markChatUnreadForUser` |

Festlegungen dazu: Namen werden zu Ids aufgelöst (mehrdeutig → Kandidaten, keine Aktion);
Nachrichten werden HTML→Text gewandelt und auf `max_zeichen` (Standard 2000) gekürzt; gelöschte und
System-Nachrichten fehlen; Senden escaped Text und wiederholt **nie** automatisch (429 beim Lesen
schon, höchstens zweimal).

### Tools aus Aufgabe 06

| Tool | Annotation | Graph |
|---|---|---|
| `sharepoint_sites` | lesend | `GET /sites?search=` |
| `sharepoint_bibliotheken` | lesend | `GET /sites/{site}/drives` |
| `dateien_liste` | lesend | `GET /me/drive/root/children`, `…/root:/{pfad}:/children` |
| `datei_lesen` | lesend | `GET …/items/{id}`, `…/items/{id}/content` (Text; Office als PDF) |
| `datei_hochladen` | destruktiv | `PUT …/root:/{pfad}:/content` (≤ 4 MB, `conflictBehavior=rename`) |
| `ordner_anlegen` | destruktiv | `POST …/children` |
| `sharepoint_listen` | lesend | `GET /sites/{site}/lists` |
| `sharepoint_listeneintraege` | lesend | `GET …/lists/{liste}/items?$expand=fields` |
| `sharepoint_listeneintrag_anlegen` | destruktiv | `POST …/lists/{liste}/items` |
| `sharepoint_listeneintrag_aendern` | destruktiv | `PATCH …/items/{id}/fields` |
| `planner_plaene` | lesend | `GET /me/planner/plans`, `GET /groups/{gruppe}/planner/plans` |
| `planner_aufgaben` | lesend | `GET /planner/plans/{plan}/tasks` (+ Buckets), `GET /me/planner/tasks` |
| `planner_aufgabe_anlegen` | destruktiv | `POST /planner/tasks` |
| `planner_aufgabe_aendern` | destruktiv | `PATCH /planner/tasks/{id}` mit `If-Match` (ETag) |
| `todo_listen` | lesend | `GET /me/todo/lists` |
| `todo_aufgaben` | lesend | `GET /me/todo/lists/{liste}/tasks` |
| `todo_aufgabe_anlegen` | destruktiv | `POST /me/todo/lists/{liste}/tasks` |
| `todo_aufgabe_erledigen` | destruktiv | `PATCH …/tasks/{id}` (`status`) |
| `kontakte_suchen` | lesend | `GET /me/contacts?$search=` |
| `kontakt_anlegen` | destruktiv | `POST /me/contacts` |
| `personen_suchen` | lesend | `GET /me/people?$search=` |
| `onenote_notizbuecher` | lesend | `GET /me/onenote/notebooks` |
| `onenote_abschnitte` | lesend | `GET /me/onenote/sections`, `…/notebooks/{id}/sections` |
| `onenote_seiten` | lesend | `GET …/sections/{id}/pages` |
| `onenote_seite_lesen` | lesend | `GET …/pages/{id}`, `…/pages/{id}/content` (HTML → Text) |
| `onenote_seite_anlegen` | destruktiv | `POST …/sections/{id}/pages` (Titel und Text escaped) |

Festlegungen: Namen werden zu Ids aufgelöst (mehrdeutig → Kandidaten, keine Aktion); Text wird auf
`max_zeichen` gekürzt und mit `gekuerzt`/`laenge_gesamt` ausgewiesen; Office-Dateien liefert Graph
nur als PDF (kein Text), PDF und Binäres nur als Metadaten; ein Upload über 4 MB wird **ohne
Anfrage** abgelehnt; Planner ändert nur mit `If-Match`, ohne ETag gar nicht; POST/PATCH/PUT werden
nie automatisch wiederholt, lesende Aufrufe nach 429/503 höchstens zweimal.

**Offen** (siehe `docs/offen.md`): echte Office-/PDF-Textextraktion, Upload-Sessions über 4 MB,
`If-Match` bei Listeneinträgen, Löschen von Planner-Aufgaben, Checklisten in To Do.

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
crates/graph       Graph-Client: GET/POST/PATCH/PUT mit Token, 401 → erneuern, Text und JSON,
                   Modelle der Antworten
crates/mcp         rmcp-Server über stdio, die Tools aus §4; Programm `quiver-m365`
                   (Teams: `teams*.rs`, Aufgabe 06: `dateien*`, `planner*`, `kontakte*`,
                   `onenote*`, dazu `aufloesen.rs` für Namen → Ids)
```

Richtung: `anmeldung ← graph ← mcp`.

## 6. Tests

Ohne echten Tenant: eine **Attrappe** (axum) spielt Identity-Provider (`/authorize`, `/token`)
und Graph (`/me`, `/me/messages`, …). `M365_AUTHORITY`/`M365_GRAPH_URL` zeigen darauf.
Der „Browser“ ist im Test eine Funktion, die die Authorize-URL direkt aufruft.

## 7. Offene Punkte

1. Admin-Freigabe im Herausgeber-Tenant für den ersten echten Test (`quiver-m365 freigeben`).
4. Herausgeber-Verifizierung (Verified Publisher) – macht die Zustimmungsseite vertrauenswürdiger und erlaubt in vielen Tenants die User-Zustimmung.
2. ~~Teams-Chats und Kanäle (`Chat.ReadWrite`, `ChannelMessage.Send`) – nach dem MVP.~~
   → Aufgabe 05: 16 Tools, siehe `docs/offen.md` für den Rest (Dateien in Kanälen, Präsenz,
   Bearbeiten/Löschen von Nachrichten).
3. Übergabe der Anmeldung an den quiver-Client (§2 „Später“).
