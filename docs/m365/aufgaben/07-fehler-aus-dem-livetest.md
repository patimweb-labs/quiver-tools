# 07 – Fehler aus dem Live-Test gegen Microsoft Graph (2026-11-24)

Der erste Test gegen ein echtes Microsoft 365 hat Fehler gezeigt, die unsere Graph-Attrappe
durchgelassen hat. Zu beheben ist **beides**: der Code und die Attrappe. Die Attrappe soll die
Regeln von Graph durchsetzen, damit solche Fehler künftig schon im Test auffallen.

| Tool | Graph-Antwort | Korrektur |
|---|---|---|
| `teams_liste` | `Query option 'Top' is not allowed` | kein `$top` auf `/me/joinedTeams`, stattdessen clientseitig begrenzen |
| `teams_kanaele` | dito | kein `$top` auf `/teams/{id}/channels` |
| `onenote_notizbuecher`, `onenote_abschnitte` | `Could not find a property named 'webUrl'` | `webUrl` raus aus `$select`; der Link steht unter `links.oneNoteWebUrl.href` |
| `teams_als_gelesen` | `TenantId for the user must be specified` | Rumpf `{"user":{"id":…,"tenantId":…}}`, tenantId aus `/me` oder aus dem Token (`tid`) |
| `teams_chat_anlegen` | `'user@odata.bind' field is missing` | eigenes Mitglied als `https://graph.microsoft.com/v1.0/users('<eigene-id>')` binden, **nicht** `…/me`; Rolle `owner` |
| `teams_mitglieder` | 403, braucht `TeamMember.Read.All` | Scope in `STANDARD_SCOPES` und in `docs/m365/entra-app.md` ergänzen; bei 403 Hinweistext statt Rohfehler |
| `todo_listen` | 400 `invalidRequest` | `/me/todo/lists` **ohne** `$top`/`$select`, Paginierung über `@odata.nextLink`; prüfe dasselbe für `todo_aufgaben` (`$top` erlaubt, `$select` nicht verwenden) |

## Attrappe

Die Attrappe antwortet für die genannten Endpunkte genauso wie Graph:
- 400 bei `$top` auf joinedTeams und channels
- 400 bei `webUrl` in `$select` für notebooks und sections
- 400 bei markChatReadForUser ohne tenantId
- 400 bei chats ohne `user@odata.bind` je Mitglied bzw. mit `/me`
- 400 bei `$top`/`$select` auf todo/lists

Zu jedem Fehler kommt ein Test, der mit dem alten Code rot wäre.

## Sonst

- Bestehendes **nicht** ändern, nur diese Fehler beheben.
- Prüfbefehl grün: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`.
- Nicht committen.
