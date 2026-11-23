# App-Registrierung und Freigabe

## Die eine Registrierung (erledigt)

`quiver-m365` ist **eine** Multi-Tenant-App im Tenant von neoimpulse.de:

| | |
|---|---|
| Client-ID | `9ae8e0db-e05f-4db8-adde-83d70d416eeb` |
| Kontotypen | Accounts in any organizational directory (Multi-Tenant) |
| Plattform | Public client/native, Redirect `http://localhost` (jeder Port) |
| Öffentliche Client-Flows | an |
| Rechte (delegiert, Graph) | `User.Read`, `Mail.ReadWrite`, `Mail.Send`, `Calendars.ReadWrite`, `Files.Read.All`, `offline_access` |
| Secret | keins |

Angelegt 2026-11-23 mit der Azure CLI (`az ad app create … --sign-in-audience AzureADMultipleOrgs`
`--public-client-redirect-uris http://localhost --is-fallback-public-client true`, dann
`az ad app permission add`). **Keine** Admin-Zustimmung erteilt – die passiert im Browser.

## Freigabe – im Browser

- **User:** Beim ersten Tool-Aufruf öffnet sich die Anmeldung; danach zeigt Microsoft
  „quiver-m365 möchte: …“ → *Akzeptieren*. Fertig.
- **Firma (Admin):** Erlaubt der Tenant keine User-Zustimmung, gibt ein Admin **einmal** frei:
  `quiver-m365 freigeben --tenant firma.de` (öffnet die Admin-Zustimmungsseite im Browser).

## Eigene Registrierung (optional)

Wer nicht dem Herausgeber neoimpulse.de vertrauen will, legt dieselbe Registrierung im eigenen Tenant an
und setzt `M365_CLIENT_ID`.
