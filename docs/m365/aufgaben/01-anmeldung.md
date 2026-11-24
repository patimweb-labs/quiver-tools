# 01 – Anmeldung im Browser (Authorization Code + PKCE)

Crate: `crates/anmeldung` (`m365-anmeldung`). Die öffentliche Schnittstelle steht in
`src/lib.rs` – **umsetzen, nicht ändern** (fehlt etwas: aufhören und in der Antwort sagen).
Ablauf und Regeln: `SPEC.md` §2.

## Abhängigkeiten (nur diese)

`oauth2 = "5"` (mit `reqwest`-Feature, ohne Standard-TLS → rustls), `reqwest` (workspace),
`tokio` (workspace), `axum = "0.8"` (Lauscher), `open = "5"`, `keyring = "4"`,
`rand = "0.9"`, `serde`, `serde_json`, `tracing`, `url = "2"`.
Dev: `axum` (Attrappe), `tokio` mit `test-util` wenn nötig.

## Umsetzen

1. `Konfig::aus_umgebung() -> anyhow::Result<Konfig>` (SPEC §3; `M365_CLIENT_ID` Pflicht).
2. `Schluesselbund` (keyring, Dienst `quiver-m365`) und `SpeicherTresor` (Mutex<HashMap>) für `Tresor`;
   `OeffneBrowser` (`open::that`) für `Browser`.
3. `EntraAnmeldung<T: Tresor, B: Browser>` implementiert `Anmeldung`:
   - Access-Token + Ablaufzeit im Speicher; 60 s vor Ablauf als abgelaufen behandeln.
   - Refresh-Token aus dem Tresor (Schlüssel `<tenant>/<client_id>`) → Token-Endpunkt
     `grant_type=refresh_token`. Neues Refresh-Token (Rotation!) sofort speichern.
     Antwort `invalid_grant` → Refresh-Token löschen, weiter mit Browser.
   - Browser-Ablauf: Lauscher auf `127.0.0.1:0`, `redirect_uri = http://localhost:<port>`,
     PKCE S256, zufälliger `state`, Authorize-URL mit `prompt=select_account`,
     `Browser::oeffnen`. Die Rückleitung `GET /?code&state` (oder `?error&error_description`)
     beantwortet eine kleine HTML-Seite („Angemeldet – du kannst dieses Fenster schließen.“ bzw.
     Fehlertext; **ohne** Code oder Token), danach schließt der Lauscher.
   - **Nur ein** Anmeldevorgang gleichzeitig: laufen schon einer, warten weitere Aufrufe auf denselben.
   - Wartet höchstens `Konfig::warten`, dann `Ok(Token::WartetAufBrowser)`; der Vorgang läuft im
     Hintergrund weiter bis `lauscher_hoechstens`, sein Ergebnis bedient den nächsten Aufruf.
   - `state` falsch → `StateFalsch`, Vorgang verworfen.
   - `M365_ACCESS_TOKEN` gesetzt → immer `Bereit(dieses)`, kein Browser, kein Tresor.
4. Logs mit `tracing`: Schritte ja („Browser geöffnet“, „Token erneuert“), Werte nie.

## Attrappe für Tests (`tests/attrappe.rs`, als Modul von den Tests genutzt)

axum-Server mit:
- `GET /<tenant>/oauth2/v2.0/authorize` → prüft `client_id`, `code_challenge_method=S256`,
  merkt `code_challenge`, leitet (302) an `redirect_uri?code=c1&state=<state>` weiter.
- `POST /<tenant>/oauth2/v2.0/token` → `authorization_code`: prüft `code_verifier` gegen die
  gemerkte Challenge; `refresh_token`: gibt neues Paar (Rotation) oder `invalid_grant`, wenn das
  Refresh-Token als widerrufen markiert ist. Antwort mit `expires_in`.
Test-„Browser“: ruft die Authorize-URL mit `reqwest` auf (folgt der Weiterleitung zum Lauscher).

## Abnahme (Tests)

1. Erste Anmeldung über Browser → `Bereit`, Refresh-Token liegt im `SpeicherTresor`.
2. Zweiter Aufruf → aus dem Speicher, Attrappe sieht keinen weiteren Token-Aufruf.
3. Abgelaufen (kurze `expires_in`) → Refresh, rotiertes Refresh-Token gespeichert.
4. `invalid_grant` → Browser-Ablauf erneut.
5. Falscher `state` → `StateFalsch`.
6. Browser, der nie zurückkommt + `warten` = 200 ms → `WartetAufBrowser`; kommt er später, bedient
   der nächste Aufruf das Ergebnis ohne zweiten Browser.
7. Zwei gleichzeitige Aufrufe → nur **ein** Browser-Aufruf.
8. `M365_ACCESS_TOKEN` → `Bereit` ohne Netz.
9. Kein Token-Wert in irgendeiner Fehlermeldung (Test mit `format!("{fehler}")`).

Prüfbefehl grün.
