# App-Registrierung in Entra ID

`quiver-m365` meldet User im Browser an. Dafür braucht es **einmal pro Tenant** eine
App-Registrierung (öffentlicher Client, **kein** Secret).

## Im Azure-Portal

1. **Entra ID → App registrations → New registration**
   - Name: `quiver-m365`
   - Supported account types: *Accounts in this organizational directory only* (ein Tenant)
     oder *… any organizational directory* (mehrere Tenants)
   - Redirect URI: Plattform **Public client/native (mobile & desktop)**, Wert `http://localhost`
2. **Authentication**
   - „Allow public client flows“: **Yes**
   - (Entra akzeptiert bei `http://localhost` jeden Port – der Lauscher nimmt einen freien.)
3. **API permissions → Add → Microsoft Graph → Delegated permissions**
   `User.Read`, `Mail.ReadWrite`, `Mail.Send`, `Calendars.ReadWrite`, `Files.Read.All`, `offline_access`
   - Je nach Tenant-Richtlinie: **Grant admin consent** (sonst sieht jeder User beim ersten Login
     eine Zustimmungsseite – oder darf gar nicht zustimmen).
4. **Overview:** *Application (client) ID* und *Directory (tenant) ID* notieren.

## Mit der Azure CLI (gleiches Ergebnis)

```bash
az ad app create --display-name quiver-m365 \
  --sign-in-audience AzureADMyOrg \
  --public-client-redirect-uris http://localhost \
  --is-fallback-public-client true
# Rechte (IDs der Graph-Berechtigungen) und Admin-Consent: siehe README des Repos
```

## In mcp-quiver eintragen

Eintrag `microsoft-365`, Laufzeit `stdio`, Befehl `quiver-m365`, Felder `client_id` und `tenant`
(Quelle **server**), Umgebung `M365_CLIENT_ID=${client_id}`, `M365_TENANT=${tenant}`.
