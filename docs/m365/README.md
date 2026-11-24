# quiver-m365

> **Status: in Arbeit.** Spezifikation in [SPEC.md](SPEC.md).

**Microsoft 365 für jeden Agenten** – ein MCP-Server für Mail, Kalender, Dateien, Teams,
SharePoint, Planner/To Do, Kontakte/Personen und OneNote über Microsoft Graph. Der User meldet sich **im Browser** an seinem Konto an (inkl. MFA); alle Aufrufe
laufen mit seinen eigenen Rechten. Reines Rust, ein Programm, MCP über stdio.

Die Anmeldung löst der User im [quiver](https://github.com/patimweb-labs/mcp-quiver)-Fenster aus
(Tools → Microsoft 365 → **Anmelden**), nie ein Agent über einen Tool-Aufruf: `quiver-m365` öffnet
im MCP-Betrieb **keinen** Browser. Wer das Programm ohne quiver benutzt, ruft `quiver-m365 anmelden`
selbst auf – es läuft mit jedem MCP-Client.

## Tools

| Tool | Was es tut | Graph |
|---|---|---|
| `profil` | Name, Mail-Adresse, Abteilung | `GET /me` |
| `mails_suchen` | Suche im Postfach; ohne Suchtext die neuesten Mails im Posteingang | `GET /me/messages`, `GET /me/mailFolders/inbox/messages` |
| `mail_lesen` | eine Mail als Text (nicht HTML); standardmäßig nur der neue Teil ohne zitierten Verlauf | `GET /me/messages/{id}` |
| `mail_senden` | Mail senden (**verändernd**) | `POST /me/sendMail` |
| `termine` | Termine eines Zeitraums | `GET /me/calendarView` |
| `termin_anlegen` | Termin anlegen (**verändernd**) | `POST /me/events` |
| `dateien_suchen` | Dateien in OneDrive suchen | `GET /me/drive/root/search(q='…')` |
| `abmelden` | Refresh-Token löschen | – |

Antworten sind knappes JSON als Text. Fehlt die Anmeldung, antwortet jedes Tool (außer `abmelden`)
nicht mit einem Fehler, sondern mit: „Nicht angemeldet – bitte im quiver-Fenster unter Tools →
Microsoft 365 auf „Anmelden“ klicken.“ Der Agent kann das dem User so weitergeben.

### `mail_lesen`: nur der neue Teil, gedeckelt

Eine lange Mailwechsel-Kette kostet Kontext. `mail_lesen` gibt deshalb standardmäßig nur den
**neuen Teil** der Mail zurück (Graph-Feld `uniqueBody`, ohne den zitierten Verlauf):

| Parameter | Standard | Bedeutung |
|---|---|---|
| `id` | – | id der Mail aus `mails_suchen` |
| `verlauf` | `false` | `true` = ganzer Text samt zitiertem Verlauf (Graph-Feld `body`) – nur, wenn die Vorgeschichte gebraucht wird |
| `max_zeichen` | `4000` | höchstens so viele Zeichen Mailtext (höchstens 50 000), gekürzt an einer Wortgrenze |

Wird gekürzt, steht in der Antwort zusätzlich `"gekuerzt": true` und `"laenge_gesamt"` (Zeichen der
vollen Mail); sonst fehlen beide Felder. Mehr als zwei Leerzeilen hintereinander werden zu zwei
zusammengefasst – das ist meist nur Luft.

### Zeiten

Alle Zeiten in Antworten stehen als RFC 3339 **mit Versatz** in der Zone aus `M365_ZEITZONE`,
ohne Sekundenbruchteile: `2026-11-23T08:40:00+01:00`, im Sommer `2026-07-15T08:40:00+02:00`. Das gilt
für `termine` (start/ende), `mails_suchen`/`mail_lesen` (datum), `dateien_suchen` (geaendert) und die
Rückgabe von `termin_anlegen`. Ganztägige Termine tragen nur ein Datum (`2026-11-24`) und
`ganztags: true`. Eingaben (`von`, `bis`, `start`, `ende`) sind Ortszeit in dieser Zone, dürfen jetzt
aber auch einen Versatz tragen (`…+01:00`, `…Z`) – der Zeitpunkt bleibt dabei derselbe.

## Teams

Die Teams-Tools handeln **als der User**: Sie lesen, was er lesen darf, und was sie senden, kommt von
ihm. Delegierte Rechte: `Team.ReadBasic.All`, `Channel.ReadBasic.All`, `ChannelMessage.Read.All`,
`ChannelMessage.Send`, `TeamMember.Read.All`, `Chat.ReadWrite`, `User.ReadBasic.All` (in der
App-Registrierung eingetragen,
Admin-Zustimmung für neoimpulse.de erteilt – siehe [docs/entra-app.md](docs/entra-app.md)).

| Tool | Was es tut | Graph |
|---|---|---|
| `teams_eingang` | Chats mit ungelesenen Nachrichten und Erwähnungen der letzten `tage` (Standard 3) | `GET /me/chats?$expand=lastMessagePreview`, `POST /search/query` |
| `teams_liste` | Teams, in denen der User Mitglied ist | `GET /me/joinedTeams` |
| `teams_kanaele` | Kanäle eines Teams | `GET /teams/{team}/channels` |
| `teams_kanal_lesen` | Beiträge eines Kanals (Standard 20, höchstens 50), je Beitrag die Anzahl der Antworten | `GET …/channels/{kanal}/messages` |
| `teams_thread_lesen` | ein Beitrag und alle Antworten | `GET …/messages/{id}`, `GET …/replies` |
| `teams_kanal_senden` | Beitrag in einen Kanal schreiben (**verändernd**) | `POST …/channels/{kanal}/messages` |
| `teams_kanal_antworten` | auf einen Beitrag antworten (**verändernd**) | `POST …/messages/{id}/replies` |
| `teams_chats` | Chats mit Thema, Art, Mitgliedern und Vorschau | `GET /me/chats?$expand=members` |
| `teams_chat_lesen` | Nachrichten eines Chats | `GET /chats/{chat}/messages` |
| `teams_chat_senden` | Nachricht in einen Chat senden (**verändernd**) | `POST /chats/{chat}/messages` |
| `teams_chat_anlegen` | 1:1- oder Gruppenchat anlegen, optional mit erster Nachricht (**verändernd**) | `POST /chats` |
| `teams_suchen` | alle Teams-Nachrichten durchsuchen | `POST /search/query` (`chatMessage`) |
| `teams_mitglieder` | Mitglieder eines Teams oder Kanals | `GET /teams/{team}/members`, `…/channels/{kanal}/members` |
| `teams_person_finden` | Personen über Name, E-Mail oder UPN finden | `GET /users?$search=…` (`ConsistencyLevel: eventual`) |
| `teams_reagieren` | auf eine Nachricht reagieren oder die Reaktion entfernen (**verändernd**) | `POST …/setReaction`, `…/unsetReaction` |
| `teams_als_gelesen` | Chat als gelesen oder wieder ungelesen markieren | `POST /chats/{chat}/markChatReadForUser`, `…/markChatUnreadForUser` |

- **Namen statt Ids:** `team`, `kanal` und `chat` dürfen Id **oder** Name sein, bei Chats auch das
  Thema oder ein Mitglied. Aufgelöst wird über die Listen. Passen mehrere, nennt das Tool die
  Kandidaten und **tut nichts** – „Nachricht an Anna“ landet nie im falschen Chat.
- **Nachrichten** kommen als Text (HTML wird umgewandelt), je Nachricht höchstens `max_zeichen`
  (Standard 2000); wird gekürzt, steht `"gekuerzt": true` und `"laenge_gesamt"` dabei. Absender, Zeit
  (RFC 3339 mit Versatz) und Anhänge nur als Name. Gelöschte und System-Nachrichten fehlen.
- **Senden** nimmt `text` (Pflicht) und `als_html` (Standard `false`: der Text wird HTML-escaped,
  Zeilenumbrüche werden zu `<br>`). Die Antwort enthält die Id der neuen Nachricht und, falls
  vorhanden, einen Web-Link.
- **Seiten** über `@odata.nextLink`, aber nie mehr als für `anzahl` nötig. **429/503** mit
  `Retry-After` werden bei lesenden Aufrufen höchstens zweimal wiederholt – ein Senden **nie**,
  sonst gäbe es doppelte Nachrichten.
- **Fehlt die Zustimmung** für die Teams-Rechte (`AADSTS65001`, `consent_required` oder Graph 403
  „Missing scope“), antwortet das Tool mit: „Für Teams fehlt die Zustimmung – im quiver-Fenster unter
  Tools → Microsoft 365 erneut auf ‚Anmelden‘ klicken.“ Es öffnet **keinen** Browser und löscht das
  Refresh-Token **nicht**.
- **Eigenheiten von Graph** (aus dem Live-Test, `docs/aufgaben/07-fehler-aus-dem-livetest.md`):
  `$top` ist auf `/me/joinedTeams` und `/teams/{id}/channels` **nicht** erlaubt – die Tools begrenzen
  clientseitig. `markChatReadForUser` braucht den Mandanten des Users im Rumpf (`tid` aus dem Token).
  Ein neuer Chat bindet das eigene Mitglied als `users('<eigene-id>')`, **nicht** als `…/me`.
  `teams_mitglieder` braucht `TeamMember.Read.All`; fehlt es (403), sagt das Tool das im Klartext
  statt den Rohfehler zu zeigen.

## SharePoint, OneDrive, Planner, To Do, Kontakte, Personen und OneNote

Delegierte Rechte (Aufgabe 06, in der App-Registrierung eingetragen, Admin-Zustimmung für
neoimpulse.de erteilt): `Sites.ReadWrite.All`, `Files.ReadWrite.All`, `Tasks.ReadWrite`,
`Group.Read.All`, `Contacts.ReadWrite`, `People.Read`, `Notes.ReadWrite.All` – siehe
[docs/entra-app.md](docs/entra-app.md).

### SharePoint und OneDrive

| Tool | Was es tut | Graph |
|---|---|---|
| `sharepoint_sites` | Sites suchen (Id `host,site,web` und Name) | `GET /sites?search=` |
| `sharepoint_bibliotheken` | Bibliotheken (Drives) einer Site | `GET /sites/{site}/drives` |
| `dateien_liste` | Ordnerinhalt – in OneDrive oder einer Bibliothek, mit Pfad oder Id | `GET /me/drive/root/children`, `…/root:/{pfad}:/children` |
| `datei_lesen` | eine Datei als Text (gekürzt auf `max_zeichen`, Standard 20 000) | `GET …/items/{id}`, `…/items/{id}/content` |
| `datei_hochladen` | Text hochladen, bis 4 MB, einfacher PUT, Konflikt `rename` (**verändernd**) | `PUT …/root:/{pfad}:/content` |
| `ordner_anlegen` | Ordner anlegen (**verändernd**) | `POST …/children` |
| `sharepoint_listen` | Listen einer Site | `GET /sites/{site}/lists` |
| `sharepoint_listeneintraege` | Einträge einer Liste samt Spalten | `GET …/lists/{liste}/items?$expand=fields` |
| `sharepoint_listeneintrag_anlegen` | Eintrag anlegen (**verändernd**) | `POST …/lists/{liste}/items` |
| `sharepoint_listeneintrag_aendern` | Spaltenwerte ändern (**verändernd**) | `PATCH …/items/{id}/fields` |

- **Wo liegt die Datei?** Ohne `site` arbeiten die Datei-Tools im OneDrive des Users (`/me/drive`).
  Mit `site` (Id oder Name) in einer Bibliothek; ohne `bibliothek` in der Dokumente-Bibliothek.
- **Lesen:** Text, Markdown, CSV, JSON und andere Textdateien kommen als Text zurück. Ist der Text
  länger als `max_zeichen`, wird an einer Wortgrenze gekürzt und die Antwort trägt `"gekuerzt": true`
  und `"laenge_gesamt"`. **Office-Dateien** gibt Microsoft Graph höchstens als PDF aus (die Antwort
  sagt mit `als_pdf`, ob das ging) – Text liefert Graph dort nicht. **PDF und Binärdateien** liefern
  nur Metadaten und Link, keinen Inhalt.
- **Hochladen:** `inhalt` ist Text; mehr als 4 MB lehnt das Tool **ohne Anfrage** ab (dafür bräuchte
  es eine Upload-Session). `verhalten` ist `rename` (Standard), `replace` oder `fail`.
- **Seiten** über `@odata.nextLink`, aber nie mehr als für `anzahl` nötig.

### Planner und To Do

| Tool | Was es tut | Graph |
|---|---|---|
| `planner_plaene` | Pläne des Users – insgesamt oder je Gruppe | `GET /me/planner/plans`, `GET /groups/{gruppe}/planner/plans` |
| `planner_aufgaben` | Aufgaben eines Plans (mit Bucket-Namen) oder alle eigenen | `GET /planner/plans/{plan}/tasks`, `…/buckets`, `GET /me/planner/tasks` |
| `planner_aufgabe_anlegen` | Aufgabe anlegen (**verändernd**) | `POST /planner/tasks` |
| `planner_aufgabe_aendern` | Fortschritt, Fälligkeit, Zuweisung – **mit ETag** (**verändernd**) | `PATCH /planner/tasks/{id}` (`If-Match`) |
| `todo_listen` | To-Do-Listen | `GET /me/todo/lists` |
| `todo_aufgaben` | Aufgaben einer Liste, optional nur offene | `GET /me/todo/lists/{liste}/tasks` |
| `todo_aufgabe_anlegen` | Aufgabe anlegen (**verändernd**) | `POST /me/todo/lists/{liste}/tasks` |
| `todo_aufgabe_erledigen` | Aufgabe erledigen oder wieder öffnen (**verändernd**) | `PATCH …/tasks/{id}` |

- **Planner verlangt ein ETag:** Erst wird die Aufgabe gelesen, dann mit `If-Match` geändert. Liefert
  Graph kein ETag, gibt es **keinen** PATCH – das Tool meldet es und ändert nichts.
- **Fälligkeiten:** Planner rechnet in UTC; `2026-11-30` wird zu `2026-11-30T00:00:00Z`, ein Zeitpunkt
  mit Versatz wird umgerechnet. To Do bekommt Ortszeit **und** Zone (`M365_ZEITZONE`).
- **Zuweisen** nimmt Name, E-Mail, UPN oder Id; mehrdeutige Namen ergeben Kandidaten und keine Aktion.

### Kontakte und Personen

| Tool | Was es tut | Graph |
|---|---|---|
| `kontakte_suchen` | Adressbuch des Users durchsuchen | `GET /me/contacts?$search=` |
| `kontakt_anlegen` | Kontakt anlegen (**verändernd**) | `POST /me/contacts` |
| `personen_suchen` | Kollegen mit Abteilung, Telefon und Mail | `GET /me/people?$search=` |

### OneNote

| Tool | Was es tut | Graph |
|---|---|---|
| `onenote_notizbuecher` | Notizbücher | `GET /me/onenote/notebooks` |
| `onenote_abschnitte` | Abschnitte – alle oder eines Notizbuchs | `GET /me/onenote/sections`, `…/notebooks/{id}/sections` |
| `onenote_seiten` | Seiten eines Abschnitts | `GET …/sections/{id}/pages` |
| `onenote_seite_lesen` | Seite als Text (HTML → Text, `max_zeichen`) | `GET …/pages/{id}`, `…/pages/{id}/content` |
| `onenote_seite_anlegen` | Seite anlegen (**verändernd**) | `POST …/sections/{id}/pages` |

Bei `onenote_seite_anlegen` werden Titel und Text **HTML-escaped** eingesetzt: ein `<b>` im Text
bleibt Text und wird nicht zu Markup.

OneNote kennt bei Notizbüchern und Abschnitten **kein** `webUrl` (Graph antwortet mit 400); der Link
steht unter `links.oneNoteWebUrl.href` und wird von dort gelesen. `/me/todo/lists` verträgt weder
`$top` noch `$select` – begrenzt wird clientseitig, geblättert über `@odata.nextLink`; To-Do-Aufgaben
kommen ohne `$select`, `$top` ist dort erlaubt (`docs/aufgaben/07-fehler-aus-dem-livetest.md`).

### Regeln für alle neuen Tools

- **Namen statt Ids.** Site, Bibliothek, Liste, Plan, Bucket, To-Do-Liste, Abschnitt und Notizbuch
  dürfen Id **oder** Name sein. Passen mehrere, nennt das Tool die Kandidaten und **tut nichts**.
- **429/503** mit `Retry-After` werden bei **lesenden** Aufrufen höchstens zweimal wiederholt. Ein
  POST, PATCH oder PUT wird **nie** automatisch wiederholt – sonst entstünden doppelte Einträge.
- **Fehlt die Zustimmung** für die neuen Rechte, antwortet das Tool mit einem Hinweis (kein Fehler):
  „Für SharePoint, Planner, To Do, Kontakte oder OneNote fehlt die Zustimmung – im quiver-Fenster
  unter Tools → Microsoft 365 erneut auf ‚Anmelden‘ klicken.“ Es öffnet **keinen** Browser und
  löscht das Refresh-Token **nicht**. Der Teams-Hinweis bleibt unverändert.

## Anmeldung

Drei Unterbefehle erledigen alles, was zum Anmelden gehört. mcp-quiver ruft sie mit derselben
Umgebung wie den MCP-Prozess auf (siehe [Eintrag für mcp-quiver](#eintrag-für-mcp-quiver)):

| Aufruf | Was passiert | Exit |
|---|---|---|
| `quiver-m365 anmelden` | Browser-Ablauf (Authorization Code + PKCE, wartet **5 min** auf die Rückleitung), danach `GET /me`; stdout: das Konto (`userPrincipalName`) | 0 = angemeldet, 2 = Fehler (stderr) |
| `quiver-m365 status` | **ohne** Browser: Refresh-Token erneuern und `GET /me`; stdout: das Konto | 0 = angemeldet, 1 = kein (gültiges) Refresh-Token, 2 = Netz-/sonstiger Fehler |
| `quiver-m365 abmelden` | Refresh-Token aus der Tresor-Datei löschen | 0 |

Ein von außen gereichtes `M365_ACCESS_TOKEN` gilt nur im MCP-Betrieb; die Unterbefehle verwalten
das eigene Refresh-Token im Tresor.

Kommt ein neuer Scope dazu (wie die Teams-Rechte), verlangt der Token-Endpunkt einmal eine neue
Zustimmung: Der User klickt im quiver-Fenster erneut auf „Anmelden“. Die Tools sagen das, öffnen
aber keinen Browser und löschen das Refresh-Token nicht.

Im MCP-Betrieb (Aufruf ohne Unterbefehl) wird nie ein Browser geöffnet: fehlt ein gültiges
Refresh-Token, antworten die Tools mit dem Hinweis oben. Wer `quiver-m365` ohne quiver benutzt und
das alte Verhalten will (Browser öffnet sich aus einem Tool-Aufruf heraus), setzt
`M365_BROWSER=erlaubt`.

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
| `M365_SCOPES` | `User.Read Mail.ReadWrite Mail.Send Calendars.ReadWrite Files.Read.All Team.ReadBasic.All Channel.ReadBasic.All ChannelMessage.Read.All ChannelMessage.Send TeamMember.Read.All Chat.ReadWrite User.ReadBasic.All Sites.ReadWrite.All Files.ReadWrite.All Tasks.ReadWrite Group.Read.All Contacts.ReadWrite People.Read Notes.ReadWrite.All offline_access` | delegierte Rechte |
| `M365_AUTHORITY` | `https://login.microsoftonline.com` | Anmeldedienst (Tests: Attrappe) |
| `M365_GRAPH_URL` | `https://graph.microsoft.com/v1.0` | Graph (Tests: Attrappe) |
| `M365_ZEITZONE` | `Europe/Berlin` | Zeitzone für Termine und alle Zeiten in Antworten |
| `M365_ACCESS_TOKEN` | – | fertiges Token von außen; dann keine eigene Anmeldung |
| `M365_BROWSER` | `aus` | `erlaubt` = Browser-Ablauf auch aus einem Tool-Aufruf heraus (Standard: nie) |
| `M365_TRESOR` | Datei | `speicher` = Token nur im Arbeitsspeicher (Tests, Entwicklung) |
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
  "anmeldung": {
    "anmelden": ["anmelden"],
    "status": ["status"],
    "abmelden": ["abmelden"]
  },
  "felder": []
}
```

Damit erscheint im quiver-Fenster unter **Tools → Microsoft 365** der Knopf „Anmelden“: quiver
startet `quiver-m365 anmelden` mit derselben Umgebung wie den MCP-Prozess, der Browser geht auf, der
User bestätigt – fertig. „Status“ ruft `quiver-m365 status` auf (ohne Browser), „Abmelden“ löscht das
Refresh-Token. Ein Agent löst die Anmeldung nie aus.

Für einen bestimmten Tenant ein Feld `tenant` (Quelle `server`) anlegen und in der Laufzeit
`"umgebung": { "M365_TENANT": "${tenant}" }` setzen. Die Agenten sehen die Tools dann als
`microsoft-365__mails_suchen` usw.

## Firmenweite Freigabe (Admin, einmal pro Firma)

Beim ersten Anmelden (Knopf im quiver-Fenster oder `quiver-m365 anmelden`) zeigt Microsoft die
Zustimmungsseite „quiver-m365 möchte: …“ → *Akzeptieren*. Erlaubt der Tenant keine Zustimmung durch
User (häufig bei `Mail.Send`), gibt ein Admin **einmal** frei:

```bash
quiver-m365 freigeben --tenant firma.de
```

Der Befehl öffnet die Admin-Zustimmungsseite (Rechte mit Graph-Präfix, ohne `offline_access`) und
wartet auf die Rückleitung nach `http://127.0.0.1:<port>`. Danach sieht kein User der Firma mehr
eine Rückfrage.

## Wie es arbeitet

- **Anmeldung:** Authorization Code + PKCE im Browser, ausgelöst vom quiver-Fenster
  (`quiver-m365 anmelden`), Rückleitung über einen Loopback-Lauscher auf `127.0.0.1` (kein
  `localhost`, das im Browser zuerst `::1` sein kann). Im MCP-Betrieb öffnet `quiver-m365` nie
  selbst einen Browser – nur mit `M365_BROWSER=erlaubt`.
- **Speicher:** Refresh-Token in der Tresor-Datei (Dienst `quiver-m365`), Access-Token nur im
  Arbeitsspeicher. Nie in Dateien, nie in Logs, nie in Tool-Antworten.
- **Erneuern:** abgelaufenes Token per Refresh-Token, 401 von Graph → einmal erneuern und
  wiederholen, 429/503 mit `Retry-After` → einmal nach der genannten Zeit (höchstens 10 s).
- **Rechte:** nur delegierte Graph-Berechtigungen, nie App-Berechtigungen. Microsoft prüft, nicht wir.
- **Daten:** quiver-m365 speichert nichts außer dem Refresh-Token in der Tresor-Datei des Rechners.
  Mails, Termine und Dateien werden je Aufruf abgerufen und nur an den MCP-Client
  zurückgegeben, der den Aufruf gemacht hat. Kein Server dazwischen, keine Telemetrie.
  Das Tool `abmelden` löscht das Refresh-Token wieder.

## Entwicklung

```bash
# alles prüfen (der Befehl vor jedem Commit)
cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo test --workspace
```

Tests laufen nur gegen Attrappen (axum): Microsoft-Identity-Provider und Graph. Nie gegen echte
Microsoft-Server, nie gegen die echte Tresor-Datei, nie mit einem echten Browser.

Aufbau: `crates/anmeldung` (PKCE, Lauscher, Tresor) ← `crates/graph` (Graph-Client, Modelle) ←
`crates/mcp` (rmcp-Server über stdio, Programm `quiver-m365`).

## Lizenz

[MIT](LICENSE)
