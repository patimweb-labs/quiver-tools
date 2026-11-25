# 06 – quiver-zep (offizieller ZEP-MCP mit dauerhafter Anmeldung)

Crate: `crates/zep`, Programm `quiver-zep`. **Kein Nachbau** von pi-zep: `quiver-zep` ist ein lokaler stdio-MCP,
der den **offiziellen ZEP-MCP** (Streamable HTTP, OAuth) anspricht und dessen Tools 1:1 durchreicht.
Seine eigene Aufgabe ist nur: **angemeldet bleiben.**

## Bekannt (öffentliche Metadaten, 2026-11-23)

- MCP: `https://www.zep-online.de/<mandant>/next/mcp` → 401 mit `resource_metadata`
- Protected Resource: `/.well-known/oauth-protected-resource/<mandant>/next/mcp` → `authorization_servers`, Scope `mcp:use`
- Authorization Server: `/.well-known/oauth-authorization-server/<mandant>/next`:
  `authorization_endpoint …/oauth/authorize`, `token_endpoint …/oauth/token`, `registration_endpoint …/oauth/register`
  (Dynamic Client Registration), `code` + **PKCE S256**, `authorization_code` + **`refresh_token`**.
Alles aus den Metadaten lesen, **nichts** fest verdrahten außer der Basis-URL.

## Felder (Umgebung `ZEP_<FELD>`)

| Feld | Art | Hilfe |
|---|---|---|
| `url` | Url, Pflicht | MCP-Adresse, z. B. `https://www.zep-online.de/firma/next/mcp` |
| `benutzer` | Text, **Pflicht** | ZEP-Benutzername |
| `passwort` | Passwort, **Pflicht** | ZEP-Passwort |

**Festlegung des Nutzers: Die Anmeldung läuft per Benutzer und Passwort** – für die **erste** Anmeldung genauso
wie für jede spätere. Der User trägt Benutzer und Passwort im quiver-Fenster ein, danach braucht es keinen Browser.
Zusätzlich `anmeldung` im Eintrag (`quiver-zep anmelden|status|abmelden`): `anmelden` führt **die Routine**
(Benutzer/Passwort) aus; nur wenn sie an einer unbekannten Seite scheitert (z. B. MFA), öffnet `anmelden --browser`
den Browser (PKCE, Loopback `127.0.0.1`) als Notfall.

## Anmeldung und Aufrechterhaltung

1. **Client-Registrierung** (DCR) einmal je Installation: `client_name "quiver-zep"`, `redirect_uris
   ["http://127.0.0.1"]` (Port egal, sonst je Lauf neu registrieren vermeiden), `token_endpoint_auth_method "none"`.
   Ergebnis (`client_id`) speichern.
2. **Tokens:** Refresh-Token im Schlüsselbund (Dienst `quiver-zep`, Schlüssel `<url>`), Access-Token im Speicher.
   Rotierte Refresh-Tokens sofort speichern.
3. **Hintergrund-Erneuerung:** Solange der Prozess läuft, das Access-Token **vor** Ablauf (z. B. bei 80 % der
   Laufzeit) erneuern, auch ohne Tool-Aufrufe. Ziel: das Refresh-Token verfällt nie aus Untätigkeit.
4. **Routine (Anmeldung mit Benutzer/Passwort)** – beim **ersten Start ohne Refresh-Token** und immer, wenn das
   Refresh-Token abgelehnt wird (`invalid_grant`): Authorize-Ablauf per HTTP mit Cookie-Speicher ohne Browser –
   Login-Formular laden, Felder aus dem HTML lesen (Formular-`action`, versteckte Felder, CSRF), Benutzer/Passwort
   senden, Zustimmungsseite („OK“) bestätigen, Weiterleitung auf `127.0.0.1` abfangen (nicht folgen), Code + PKCE
   tauschen. **Den Aufbau der Seiten kennen wir noch nicht:** baue einen kleinen, gut getesteten HTML-Formular-
   Leser (Formulare, Eingaben, Knöpfe) und eine Ablaufsteuerung, die Schritt für Schritt meldet, was sie sieht
   (Formular-Namen/Feldnamen, **nie Werte**). Unklare Seite → abbrechen mit dieser Meldung.
5. **Sperrschutz:** ZEP sperrt nach 5 Fehlversuchen. Je Fehlschlag der Routine **ein** Versuch, dann Zustand
   „Routine gesperrt“ bis zur nächsten erfolgreichen Anmeldung über den Knopf (Zustand in einer Datei merken,
   auch über Neustarts). Nie in einer Schleife wiederholen.
6. Schlägt alles fehl → jedes Tool antwortet „ZEP-Anmeldung nötig – bitte im quiver-Fenster unter Tools → ZEP auf
   „Anmelden“ klicken.“; `status` liefert Exit 1.
7. Unterbefehl `quiver-zep routine-pruefen`: führt die Routine **einmal** aus und meldet jeden Schritt (für den
   begleiteten ersten Test mit dem echten Konto; ohne Werte, ohne Tokens).

## Durchreichen

rmcp-Client (Streamable HTTP mit `Authorization: Bearer`) zum ZEP-MCP; `list_tools`/`call_tool` 1:1 (Namen,
Schemas, Annotationen unverändert). 401 → erneuern, einmal wiederholen. Tool-Liste beim Start holen und bei
`tools/list_changed` von ZEP aktualisieren und selbst `list_changed` senden.

## Tests (nur Attrappen)

axum-Attrappe für Metadaten, DCR, authorize (Login-Formular mit CSRF-Feld + Zustimmungsseite, Cookies),
token (code/refresh, Rotation, `invalid_grant`), und einen Mini-MCP-Server mit zwei Tools hinter Bearer.
Fälle: **Erstanmeldung über die Routine ohne Browser** (Browser-Attrappe darf nicht aufgerufen werden); Notfall über `anmelden --browser`; Hintergrund-Erneuerung (kurze Laufzeiten); `invalid_grant` → Routine →
neues Token, Tools gehen weiter; falsches Passwort → genau ein Versuch, danach gesperrt (kein zweiter Login-POST);
Seite mit unbekanntem Aufbau → Abbruch mit Feldnamen ohne Werte; Durchreichen inkl. `list_changed`.

## Doku

README-Abschnitt quiver-zep, `docs/adr/0004-zep-offizieller-mcp.md` (warum Vermittler statt Nachbau, Routine,
Sperrschutz). Prüfbefehl grün.
