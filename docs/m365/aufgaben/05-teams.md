# 05 – Microsoft Teams (Graph)

Repo quiver-m365. Die Anmeldung (Aufgabe 01/03), die Graph-Tools (02/04) und die Tresor-Datei
stehen. Neu kommt **Microsoft Teams** dazu, mit derselben Anmeldung und mehr Scopes. Teams
handelt **als der User**: Es liest, was er lesen darf, und was es sendet, kommt von ihm.

Vorlage zum Nachlesen, nicht zum Übersetzen: `~/Documents/Workspaces/pi-teams` (`src/tools/*.ts`,
`src/graph/`, `docs/tools.md`, `docs/security.md`). **Nicht** übernommen werden Hörmodus
(`teams_watch`), Konten-, Setup- und Login-Tools (das macht quiver), Präsenz (dafür fehlen
Rechte) und Besprechungen (Kalender gibt es schon).

Azure DevOps ist **nicht** Teil dieses Repos. Es kommt als eigenes Tool in quiver-tools, mit PAT
je Organisation.

## 1. Scopes

`STANDARD_SCOPES` ergänzen, delegiert, Graph: `Team.ReadBasic.All`, `Channel.ReadBasic.All`,
`ChannelMessage.Read.All`, `ChannelMessage.Send`, `Chat.ReadWrite`, `User.ReadBasic.All`.
In der App-Registrierung sind sie eingetragen, und die Admin-Zustimmung für neoimpulse.de ist
erteilt (`docs/entra-app.md`).

Fehlt einem User die Zustimmung für einen neuen Scope, liefert der Token-Endpunkt
`invalid_grant` mit AADSTS65001/`consent_required`, oder Graph antwortet mit 403 und
`Missing scope`. Das Tool meldet dann: „Für Teams fehlt die Zustimmung – im quiver-Fenster
unter Tools → Microsoft 365 erneut auf ‚Anmelden‘ klicken.“ Es öffnet **keinen** Browser und
löscht das Refresh-Token **nicht**.

## 2. Tools (Graph v1.0)

| Tool | Annotation | Graph |
|---|---|---|
| `teams_eingang` | lesend | Chats mit ungelesenen Nachrichten (`/me/chats?$expand=lastMessagePreview`, `viewpoint.lastMessageReadDateTime`) und Erwähnungen des Users in den letzten `tage` (Standard 3) |
| `teams_liste` | lesend | `GET /me/joinedTeams` |
| `teams_kanaele` | lesend | `GET /teams/{team}/channels` |
| `teams_kanal_lesen` | lesend | `GET /teams/{team}/channels/{kanal}/messages?$top=` (Standard 20, max 50), je Beitrag die Anzahl der Antworten |
| `teams_thread_lesen` | lesend | Beitrag und alle Antworten (`…/messages/{id}` und `…/replies`) |
| `teams_kanal_senden` | **destruktiv** | `POST …/channels/{kanal}/messages` |
| `teams_kanal_antworten` | **destruktiv** | `POST …/messages/{id}/replies` |
| `teams_chats` | lesend | `GET /me/chats?$expand=members&$top=` (Thema, Art, Mitglieder, Vorschau der letzten Nachricht) |
| `teams_chat_lesen` | lesend | `GET /chats/{chat}/messages?$top=` |
| `teams_chat_senden` | **destruktiv** | `POST /chats/{chat}/messages` |
| `teams_chat_anlegen` | **destruktiv** | `POST /chats` (1:1 oder Gruppe, Mitglieder per E-Mail oder UPN), optional mit erster Nachricht |
| `teams_suchen` | lesend | `POST /search/query` mit `entityTypes: ["chatMessage"]` |
| `teams_mitglieder` | lesend | `GET /teams/{team}/members` bzw. `…/channels/{kanal}/members` |
| `teams_person_finden` | lesend | `GET /users?$search="displayName:…" OR "mail:…"` (`ConsistencyLevel: eventual`) |
| `teams_reagieren` | **destruktiv** | `POST …/messages/{id}/setReaction` bzw. `unsetReaction` (Chat und Kanal) |
| `teams_als_gelesen` | nicht lesend, nicht destruktiv | `POST /chats/{chat}/markChatReadForUser` bzw. `markChatUnreadForUser` |

Regeln dazu:
- `team`, `kanal` und `chat` dürfen **Id oder Name** sein, bei Chats auch das Thema oder ein
  Mitgliedsname. Aufgelöst wird über die Listen. Bei mehreren Treffern gibt es eine Meldung
  mit den Kandidaten und **keine** Aktion.
- Nachrichten: HTML in Text umwandeln, Länge mit `max_zeichen` begrenzen (Standard 2000 je
  Nachricht) und dabei sagen, was gekürzt wurde. Dazu Absender, Zeit (RFC 3339 mit Zeitzone wie
  Aufgabe 04) und Anhänge nur als Name. Gelöschte und System-Nachrichten weglassen.
- Senden: `text` (Pflicht), `als_html` (Standard false; Text wird HTML-escaped,
  Zeilenumbrüche werden zu `<br>`). Die Antwort ist die Id der neuen Nachricht und ein
  Web-Link, falls vorhanden.
- Paginierung über `@odata.nextLink`, aber höchstens so viele Seiten, wie `top` verlangt.
- 429/503: `Retry-After` beachten, höchstens 2 Wiederholungen, und nur bei **lesenden**
  Aufrufen. Ein Senden wird nie automatisch wiederholt, sonst entstehen doppelte Nachrichten.

## 3. Doku

- README: Abschnitt Teams.
- `SPEC.md`: Scopes.
- `docs/offen.md`: Dateien in Kanälen, Präsenz (braucht `Presence.Read.All`), Nachrichten
  bearbeiten und löschen, mehrere Tenants und Kunden-Teams.

## 4. Tests (nur Attrappen, nie gegen Microsoft)

- Die Graph-Attrappe wird um die Teams-Pfade erweitert.
- Namen → Id auflösen. Mehrdeutig → Kandidaten und **keine** Anfrage zum Senden.
- HTML → Text, kürzen, System-Nachrichten weglassen.
- Senden escaped HTML. 429 beim Senden → **kein** zweiter POST. 429 beim Lesen → Wiederholung
  nach `Retry-After`.
- Fehlende Zustimmung → Hinweistext, kein Browser, das Refresh-Token bleibt.
- Paginierung mit `nextLink`.
- Ende-zu-Ende über stdio: `tools/list` enthält die 16 neuen Tools mit richtigen Annotationen,
  und **jedes** Eingabeschema hat `"type": "object"`.

Prüfbefehl grün: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings &&
cargo test --workspace`. **Nicht committen**, das macht Claude nach der Abnahme.
