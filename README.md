# quiver-m365

> **Status: in Arbeit.** Spezifikation in [SPEC.md](SPEC.md).

**Microsoft 365 für jeden Agenten** – ein MCP-Server für Mail, Kalender und Dateien über
Microsoft Graph. Der User meldet sich **im Browser** an seinem Konto an (inkl. MFA); alle Aufrufe
laufen mit seinen eigenen Rechten. Reines Rust, ein Programm, MCP über stdio.

Gebaut als Tool für die [mcp-quiver](https://github.com/patimweb-labs/mcp-quiver)-Bibliothek,
läuft aber auch allein mit jedem MCP-Client.

## Tools

`profil` · `mails_suchen` · `mail_lesen` · `mail_senden` · `termine` · `termin_anlegen` ·
`dateien_suchen` · `abmelden`

## Voraussetzung

Eine App-Registrierung in Entra ID (öffentlicher Client, kein Secret):
[docs/entra-app.md](docs/entra-app.md).

## Lizenz

[MIT](LICENSE)
