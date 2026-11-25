# App-Registrierung und Freigabe

## Die eine Registrierung (erledigt)

`quiver-m365` ist **eine** Multi-Tenant-App im Tenant des Herausgebers (die Firma steht hier
absichtlich nicht; die eingebaute Client-ID genügt):

| | |
|---|---|
| Client-ID | `9ae8e0db-e05f-4db8-adde-83d70d416eeb` |
| Kontotypen | Accounts in any organizational directory (Multi-Tenant) |
| Plattform | Public client/native, Redirects `http://127.0.0.1` und `http://localhost` (jeder Port) |
| Öffentliche Client-Flows | **aus** (2026-11-24): keine Gerätecode-Anmeldung; Browser mit PKCE und Loopback braucht sie nicht |
| Rechte (delegiert, Graph) | `User.Read`, `Mail.ReadWrite`, `Mail.Send`, `Calendars.ReadWrite`, `Files.Read.All`, `offline_access`; für Teams (2026-11-24): `Team.ReadBasic.All`, `Channel.ReadBasic.All`, `ChannelMessage.Read.All`, `ChannelMessage.Send`, `TeamMember.Read.All`, `Chat.ReadWrite`, `User.ReadBasic.All`; für SharePoint/OneDrive, Planner/To Do, Kontakte/Personen und OneNote (2026-11-24): `Sites.ReadWrite.All`, `Files.ReadWrite.All`, `Tasks.ReadWrite`, `Group.Read.All`, `Contacts.ReadWrite`, `People.Read`, `Notes.ReadWrite.All` |
| Zuweisung erforderlich (Herausgeber-Tenant) | **an** (2026-11-24): nur zugewiesene User oder Gruppen dürfen sich anmelden; andere sehen AADSTS50105. Freischalten im Entra-Portal: Unternehmensanwendungen → quiver-m365 → Benutzer und Gruppen |
| Azure DevOps | Recht am 2026-11-24 entfernt (quiver-devops nutzt PAT). **Offen:** die Admin-Zustimmung `user_impersonation` im Entra-Portal widerrufen |
| Secret | keins |

Angelegt 2026-11-23 mit der Azure CLI (`az ad app create … --sign-in-audience AzureADMultipleOrgs`
`--public-client-redirect-uris http://localhost --is-fallback-public-client true`, dann
`az ad app permission add`).

**Admin-Zustimmung im Herausgeber-Tenant** erteilt am 2026-11-23, für Teams und DevOps erneut am
2026-11-24 und für die Rechte aus Aufgabe 06 (`Sites.ReadWrite.All`, `Files.ReadWrite.All`,
`Tasks.ReadWrite`, `Group.Read.All`, `Contacts.ReadWrite`, `People.Read`, `Notes.ReadWrite.All`)
am 2026-11-24 (`az ad app permission admin-consent`, alle User des Tenants). Wer vor dem 24.11.
angemeldet war, klickt **einmal** erneut auf „Anmelden“, damit das Refresh-Token die neuen Rechte
trägt. Andere Tenants geben im Browser frei (siehe unten).

`TeamMember.Read.All` kam mit Aufgabe 07 (2026-11-24) dazu: ohne dieses Recht antwortet Graph auf
`GET /teams/{id}/members` mit **403**; `teams_mitglieder` sagt das dann im Klartext statt den
Rohfehler zu zeigen. Auch dafür ist nach dem Nachtrag ein **neues** Anmelden nötig.

## Freigabe – im Browser

- **User:** Beim ersten Anmelden im quiver-Fenster (Tools → Microsoft 365 → „Anmelden“, ruft
  `quiver-m365 anmelden` auf) zeigt Microsoft „quiver-m365 möchte: …“ → *Akzeptieren*. Fertig.
  Ein Tool-Aufruf öffnet **keinen** Browser.
- **Firma (Admin):** Erlaubt der Tenant keine User-Zustimmung, gibt ein Admin **einmal** frei:
  `quiver-m365 freigeben --tenant example.test` (öffnet die Admin-Zustimmungsseite im Browser).

## Für Kollegen und Firmen: mitgelieferte App benutzen

Nichts anzulegen. Einmal freigeben, dann meldet sich jeder im quiver-Fenster an.

1. **Freigeben (Admin, einmal je Tenant):** `quiver-m365 freigeben --tenant example.test` öffnet die Zustimmungsseite. Als Admin anmelden, Rechte prüfen, **Akzeptieren**. Danach gibt es im Tenant die Unternehmensanwendung „quiver-m365“.
2. **Eingrenzen (empfohlen):** Entra Admin Center → **Unternehmensanwendungen → quiver-m365 → Eigenschaften → Zuweisung erforderlich: Ja**, dann unter **Benutzer und Gruppen** zuweisen. Andere sehen `AADSTS50105`.
3. **Anmelden (jeder):** quiver-Fenster → Tools → Microsoft 365 → **Anmelden**.

Kommen mit einer neuen Version Rechte dazu: Schritt 1 wiederholen, und jeder klickt einmal erneut auf **Anmelden**.

## Eigene Registrierung (Schritt für Schritt)

Für alle, die keine fremde App in ihrem Tenant wollen. Nötig ist eine Rolle wie **Anwendungsadministrator**.

### 1. App registrieren

**Entra Admin Center → Identität → Anwendungen → App-Registrierungen → Neue Registrierung**

| Feld | Wert |
|---|---|
| Name | `quiver-m365` |
| Kontotypen | **Nur Konten in diesem Organisationsverzeichnis** |
| Umleitungs-URI | Plattform **Öffentlicher Client/nativ (mobil und Desktop)**, `http://127.0.0.1` |

Unter **Authentifizierung** zusätzlich `http://localhost` eintragen (der Port ist bei Loopback beliebig). **Öffentliche Clientflows zulassen: Nein**, das schaltet nur die Gerätecode-Anmeldung ab. Der Browser-Weg (Autorisierungscode mit PKCE) funktioniert trotzdem. **Kein Geheimnis, kein Zertifikat:** ein Programm auf dem Rechner könnte es nicht geheim halten.

### 2. Rechte

**API-Berechtigungen → Hinzufügen → Microsoft Graph → Delegierte Berechtigungen.** Alles gilt **im Namen des angemeldeten Benutzers**, nie darüber hinaus.

| Bereich | Rechte | Admin nötig |
|---|---|---|
| Grundlage | `User.Read`, `offline_access` | nein |
| Mail | `Mail.ReadWrite`, `Mail.Send` | nein |
| Kalender | `Calendars.ReadWrite` | nein |
| Dateien, SharePoint | `Files.Read.All`, `Files.ReadWrite.All`, `Sites.ReadWrite.All` | nein |
| Teams | `Team.ReadBasic.All`, `Channel.ReadBasic.All`, `ChannelMessage.Send`, `Chat.ReadWrite`, `User.ReadBasic.All` | nein |
| Teams | `ChannelMessage.Read.All`, `TeamMember.Read.All` | **ja** |
| Planner, To Do | `Tasks.ReadWrite`, `Group.Read.All` | `Group.Read.All` **ja** |
| Kontakte, Personen | `Contacts.ReadWrite`, `People.Read` | nein |
| OneNote | `Notes.ReadWrite.All` | nein |

Dann **Administratorzustimmung erteilen**. (Ob Benutzer den übrigen Rechten selbst zustimmen dürfen, entscheidet die Richtlinie im Tenant.)

**Weniger Rechte:** Was nicht gebraucht wird, hier weglassen **und** in `M365_SCOPES` (siehe [README](README.md#konfiguration)). Beide Listen müssen zusammenpassen; die Tools des fehlenden Bereichs melden dann die fehlende Zustimmung.

### 3. Zuweisen

Wie oben: **Zuweisung erforderlich: Ja**, Benutzer oder Gruppen zuweisen (Gruppen brauchen Entra ID P1; verschachtelte zählen nicht).

### 4. In quiver eintragen

In der Verwaltung → **Bibliothek → Microsoft 365** als Umgebung setzen, für alle:

| Variable | Wert |
|---|---|
| `M365_CLIENT_ID` | Anwendungs-ID der eigenen App |
| `M365_TENANT` | Verzeichnis-ID (nicht `organizations`, die App kennt nur diesen Tenant) |

Danach melden sich alle neu an.

### Dasselbe mit der Azure CLI

```bash
az login --tenant <TENANT>
APP=$(az ad app create --display-name quiver-m365 --sign-in-audience AzureADMyOrg \
  --public-client-redirect-uris http://127.0.0.1 http://localhost \
  --is-fallback-public-client false --query appId -o tsv)
az ad sp create --id "$APP"

GRAPH=00000003-0000-0000-c000-000000000000
for R in User.Read offline_access Mail.ReadWrite Mail.Send Calendars.ReadWrite \
         Files.Read.All Files.ReadWrite.All Sites.ReadWrite.All \
         Team.ReadBasic.All Channel.ReadBasic.All ChannelMessage.Read.All ChannelMessage.Send \
         TeamMember.Read.All Chat.ReadWrite User.ReadBasic.All \
         Tasks.ReadWrite Group.Read.All Contacts.ReadWrite People.Read Notes.ReadWrite.All; do
  ID=$(az ad sp show --id $GRAPH --query "oauth2PermissionScopes[?value=='$R'].id | [0]" -o tsv)
  az ad app permission add --id "$APP" --api $GRAPH --api-permissions "${ID}=Scope"
done
sleep 30
az ad app permission admin-consent --id "$APP"
```

## Wenn es klemmt

| Meldung | Ursache und Abhilfe |
|---|---|
| `AADSTS50011` | Umleitungs-URI fehlt: Plattform **Öffentlicher Client**, `http://127.0.0.1` und `http://localhost` |
| `AADSTS50020`, `AADSTS50194` | Eigene App mit einem Tenant, aber `M365_TENANT` steht auf `organizations` |
| `AADSTS50105` | Benutzer nicht zugewiesen |
| `AADSTS65001`, `AADSTS90094` | Zustimmung fehlt: freigeben, dann erneut **Anmelden** |
| `AADSTS7000218` | Die Umleitungs-URI steht unter **Web** statt unter **Öffentlicher Client** |
| `AADSTS700016` | Falsche `M365_CLIENT_ID`, oder die App ist im Tenant noch nicht freigegeben |
| „Für Teams fehlt die Zustimmung“ | Neue Rechte seit der letzten Anmeldung: erneut **Anmelden** |
| 403 bei Team-Mitgliedern | `TeamMember.Read.All` fehlt |
| `AADSTS530…` (bedingter Zugriff) | Der Tenant verlangt z. B. ein verwaltetes Gerät; Ausnahme oder Gerät prüfen |

Die Anmeldung am **quiver-Server** ist davon getrennt und hat eine eigene App:
[mcp-quiver: docs/entra.de.md](https://github.com/patimweb-labs/mcp-quiver/blob/main/docs/entra.de.md).
