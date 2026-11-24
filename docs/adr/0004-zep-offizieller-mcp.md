# 0004 – ZEP über den offiziellen MCP: Vermittler statt Nachbau

> Stand: 2026-11-23. Aufgabe: [docs/aufgaben/06-zep.md](../../docs/aufgaben/06-zep.md).

## Kontext

Die Vorlage `pi-zep` hat die ZEP-API nachgebaut: eigene HTTP-Aufrufe, eigene Tool-Namen,
eigene Grenzen. Für ZEP gibt es aber einen **offiziellen MCP** (Streamable HTTP, OAuth 2.0
mit Dynamic Client Registration, `code` + PKCE S256, `refresh_token`). Ein Nachbau würde
hinter jedem ZEP-Update zurückbleiben und die Tool-Semantik doppelt pflegen.

Gleichzeitig hat der User festgelegt: **die Anmeldung läuft mit Benutzer und Passwort** –
für die erste Anmeldung genauso wie für jede spätere. Er trägt beides im quiver-Fenster
ein; danach soll kein Browser mehr nötig sein.

## Entscheidung

- **`quiver-zep` ist ein Vermittler.** Ein lokaler stdio-MCP (`rmcp`) spricht mit dem
  ZEP-MCP über den rmcp-Client (`StreamableHttpClientTransport`, `Authorization: Bearer`)
  und reicht `list_tools`/`call_tool` **1:1** durch. Nur `zep_profile` kommt dazu – das
  lesende Werkzeug, das jedes quiver-Tool hat (SPEC §2).
- **Alles aus den Metadaten, nur die Basis-URL fest.** Aus der eingetragenen MCP-Adresse
  folgen die Adressen nach RFC 9728/8414:
  `/.well-known/oauth-protected-resource<pfad>` und
  `/.well-known/oauth-authorization-server<pfad des Issuers>`. Authorization-, Token- und
  Registrierungs-Endpunkt, der Scope (`mcp:use`) und `code_challenge_methods_supported`
  werden gelesen, nicht geraten; fehlt `S256`, bricht die Anmeldung ab.
- **Die Routine ersetzt den Browser.** Statt einen headless Browser zu steuern, läuft der
  Authorize-Ablauf über HTTP mit eigenem Cookie-Speicher: Login-Formular laden, Felder
  lesen (`action`, versteckte Felder, CSRF), Benutzer und Passwort einsetzen, absenden,
  Zustimmungsseite bestätigen, die Weiterleitung auf `http://127.0.0.1` **abfangen**
  (nicht verfolgen), Code + Verifier tauschen.
- **Ein eigener, kleiner HTML-Formular-Leser** (`crates/zep/src/html.rs`) statt einer
  Parser-Bibliothek: Er kennt Formulare, Eingaben, Auswahllisten und Knöpfe – nicht mehr.
  Er ist unit-getestet und liefert zu jeder Seite eine Kurzfassung aus Formular- und
  Feld**namen**, damit der Ablauf melden kann, was er sieht (und nie einen Wert).
- **Der Browser bleibt Notfall.** `quiver-zep anmelden --browser` öffnet den
  Standardbrowser und lauscht auf `127.0.0.1:<port>`; nur dieser Weg braucht ihn. Kein
  Werkzeugaufruf öffnet je von selbst einen Browser (`KeinBrowser`).
- **Zugangsdaten-Schutz im Ablauf:** Benutzer und Passwort gehen nur an ein Formular,
  dessen Adresse auf **demselben Host** wie der Authorization-Endpunkt liegt. Eine
  Weiterleitung auf einen fremden Host beendet den Ablauf, statt die Zugangsdaten
  dorthin zu tragen.
- **Client-Registrierung einmal je Installation** (DCR): `client_name "quiver-zep"`,
  `redirect_uris ["http://127.0.0.1"]`, `token_endpoint_auth_method "none"`. Die
  `client_id` liegt im Tresor (Schlüssel `client-id:<url>`), nicht in einer Antwort.
- **Tokens:** Refresh-Token im Schlüsselbund (Dienst `quiver-zep`, Schlüssel `<url>`),
  Access-Token nur im Arbeitsspeicher; ein rotiertes Refresh-Token wird sofort gesichert.
  Die Hintergrund-Erneuerung greift bei 80 % der Laufzeit, ohne Werkzeugaufruf – damit
  verfällt das Refresh-Token nicht aus Untätigkeit.
- **Sperrschutz in einer Datei.** Nur ein Fehlschlag, den ZEP selbst gesehen hat
  (Benutzer und Passwort wurden abgeschickt und abgelehnt), sperrt die Routine: danach
  „Routine gesperrt“ bis zur nächsten erfolgreichen Anmeldung über den Knopf. Der
  Zustand überlebt Neustarts, weil er in `<Zustandsordner>/routine-gesperrt.json` steht.
  Ein gesperrter Ablauf wird nie in einer Schleife wiederholt. Fehler davor (Netz,
  Metadaten, unbekannte Seite vor dem Formular) sperren nicht; sie wiederholen sich
  höchstens alle 60 s (Merker im Speicher). Beim Start meldet sich das Werkzeug ohne
  gültiges Token selbst an, statt auf den Knopf zu warten (Aufgabe 11).
- **401 → erneuern, einmal wiederholen.** Lehnt ZEP das Access-Token ab, wird es
  verworfen, über das Refresh-Token erneuert und der Aufruf genau einmal wiederholt.
- **`tools/list_changed` wird weitergegeben.** Der rmcp-Client-Handler meldet die
  Änderung, der Broker holt die Liste neu und schickt selbst `list_changed` an seinen
  Client.

## Warum so

1. **ZEP bleibt die Wahrheit.** Neue Werkzeuge, geänderte Schemas und Annotationen kommen
   ohne Update an, weil nichts nachgebaut ist.
2. **Kein Browser im Alltag.** Der User trägt Benutzer und Passwort im quiver-Fenster ein
   (SPEC §2); die Routine braucht danach weder Seite noch Anzeige – nur HTTP.
3. **Der Ablauf ist sichtbar und abbrechbar.** Weil die Seiten unbekannt sind, meldet die
   Routine jeden Schritt und bricht bei einer unbekannten Seite ab, statt zu raten. Das
   ist die Voraussetzung für `quiver-zep routine-pruefen` beim begleiteten ersten Test.
4. **Sperren sind teuer, Wiederholungen billig.** ZEP sperrt nach fünf Fehlversuchen –
   also ein Versuch, dann eine Datei, dann Ruhe.
5. **Der Kontrakt bleibt derselbe.** Ein Tool, ein Zugang `standard`, `eintrag`,
   `testen`, MCP über stdio – wie bei allen anderen quiver-Tools (SPEC §4).

## Verworfene Alternativen

- **Die ZEP-API nachbauen** (wie `pi-zep`): doppelte Pflege, eigene Tool-Namen,
  hinterher bei jedem ZEP-Update. Genau das soll dieses Tool nicht sein.
- **Headless Browser (Playwright/Chromium) für die Anmeldung:** ein zweiter
  Browser-Stack samt Downloads, `cfg`-Themen und Startzeit – für einen Vorgang, der aus
  zwei Formularen besteht. Der Notfall nutzt den Browser, den das Gerät schon hat.
- **Passwort im Schlüsselbund speichern und selbst an ZEP senden:** das Passwort kommt
  ohnehin als Umgebung aus quiver; es zusätzlich zu speichern, würde nur eine zweite
  Kopie erzeugen.
- **Refresh-Token in einer Datei:** widerspricht AGENTS.md §9 (Geheimes gehört in den
  Schlüsselbund).
- **Scope fest auf `mcp:use` verdrahten:** die Metadaten sagen, was gilt; fest verdrahtet
  bricht die Anmeldung, sobald ZEP den Scope ändert.
- **`ZEP_URL` in `Basis-URL` und `Pfad` zerlegen:** die MCP-Adresse ist eine Adresse; die
  `/.well-known`-Pfade ergeben sich daraus (RFC 9728), nicht aus einer zweiten Eingabe.

## Preis

- **Die ZEP-Seiten kennen wir nicht.** Der Formularleser ist gegen eine Attrappe getestet,
  die wir selbst gebaut haben. Weicht ZEP ab (MFA, Einmalcode, Captcha), endet die
  Routine mit einer Meldung – dann bleibt `anmelden --browser`. Der begleitete erste Test
  ist `quiver-zep routine-pruefen`.
- **Zwei Abhängigkeiten mehr als in der Aufgabe genannt:** `keyring` (Schlüsselbund,
  Dienst `quiver-zep`) und `open` (Standardbrowser für den Notfall). Beide sind für die
  Aufgabe nötig und waren im Vorlagen-Repo `quiver-m365` schon im Einsatz.
- **Annahme zur Rückleitung:** registriert wird `http://127.0.0.1` (ohne Port); beim
  Notfall wird `http://127.0.0.1:<port>` geschickt. Das folgt RFC 8252 §7.3 (beliebiger
  Loopback-Port). Akzeptiert ein Server das nicht, hilft nur eine neue Registrierung je
  Lauf – dann müsste die `client_id` je Port erneut geholt werden.
- **`ZEP_TRESOR=speicher` ist ein Cheat.** Für die Kindprozess-Tests muss der Tresor im
  Speicher liegen, sonst käme ein Test an den echten Schlüsselbund. Der Schalter steht in
  `docs/offen.md` als Ticket.
- **Der HTML-Leser ist kein HTML-Parser.** Entitäten nur die üblichen, kein CSS, keine
  Namensräume, `<select>` nur mit der gewählten Option. Für Anmeldeformulare reicht das;
  für alles andere ist er nicht gedacht.
- **Ein Zugang je Werkzeug.** Zwei ZEP-Mandanten brauchen zwei Bibliothekseinträge (SPEC
  §2) – der Sperrzustand liegt entsprechend je Zugang.
