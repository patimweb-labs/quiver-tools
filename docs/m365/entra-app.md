# App-Registrierung und Freigabe

## Die eine Registrierung (erledigt)

`quiver-m365` ist **eine** Multi-Tenant-App im Tenant von neoimpulse.de:

| | |
|---|---|
| Client-ID | `9ae8e0db-e05f-4db8-adde-83d70d416eeb` |
| Kontotypen | Accounts in any organizational directory (Multi-Tenant) |
| Plattform | Public client/native, Redirects `http://127.0.0.1` und `http://localhost` (jeder Port) |
| Öffentliche Client-Flows | an |
| Rechte (delegiert, Graph) | `User.Read`, `Mail.ReadWrite`, `Mail.Send`, `Calendars.ReadWrite`, `Files.Read.All`, `offline_access`; für Teams (2026-11-24): `Team.ReadBasic.All`, `Channel.ReadBasic.All`, `ChannelMessage.Read.All`, `ChannelMessage.Send`, `Chat.ReadWrite`, `User.ReadBasic.All`; für SharePoint/OneDrive, Planner/To Do, Kontakte/Personen und OneNote (2026-11-24): `Sites.ReadWrite.All`, `Files.ReadWrite.All`, `Tasks.ReadWrite`, `Group.Read.All`, `Contacts.ReadWrite`, `People.Read`, `Notes.ReadWrite.All` |
| Rechte (delegiert, Azure DevOps `499b84ac-1321-427f-aa17-267ca6975798`) | `user_impersonation` (2026-11-24; derzeit ungenutzt – DevOps läuft über quiver-devops mit PAT) |
| Secret | keins |

Angelegt 2026-11-23 mit der Azure CLI (`az ad app create … --sign-in-audience AzureADMultipleOrgs`
`--public-client-redirect-uris http://localhost --is-fallback-public-client true`, dann
`az ad app permission add`).

**Admin-Zustimmung für neoimpulse.de** erteilt am 2026-11-23, für Teams und DevOps erneut am
2026-11-24 und für die Rechte aus Aufgabe 06 (`Sites.ReadWrite.All`, `Files.ReadWrite.All`,
`Tasks.ReadWrite`, `Group.Read.All`, `Contacts.ReadWrite`, `People.Read`, `Notes.ReadWrite.All`)
am 2026-11-24 (`az ad app permission admin-consent`, alle User des Tenants). Wer vor dem 24.11.
angemeldet war, klickt **einmal** erneut auf „Anmelden“, damit das Refresh-Token die neuen Rechte
trägt. Andere Tenants geben im Browser frei (siehe unten).

## Freigabe – im Browser

- **User:** Beim ersten Anmelden im quiver-Fenster (Tools → Microsoft 365 → „Anmelden“, ruft
  `quiver-m365 anmelden` auf) zeigt Microsoft „quiver-m365 möchte: …“ → *Akzeptieren*. Fertig.
  Ein Tool-Aufruf öffnet **keinen** Browser.
- **Firma (Admin):** Erlaubt der Tenant keine User-Zustimmung, gibt ein Admin **einmal** frei:
  `quiver-m365 freigeben --tenant firma.de` (öffnet die Admin-Zustimmungsseite im Browser).

## Eigene Registrierung (optional)

Wer nicht dem Herausgeber neoimpulse.de vertrauen will, legt dieselbe Registrierung im eigenen Tenant an
und setzt `M365_CLIENT_ID`.
