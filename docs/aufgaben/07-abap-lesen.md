# 07 – quiver-abap, Teil 1: ADT-Client und lesende Tools

Crate: `crates/abap`, Programm `quiver-abap`. Vorlage: `~/Documents/Workspaces/pi-abap-fs` (`src/tools/`,
`src/client.ts`, `docs/architecture.md`, `README.md`). Das ADT-Protokoll (HTTP-Pfade, Header, XML) steht im
MIT-lizenzierten `abap-adt-api` unter `~/Documents/Workspaces/pi-abap-fs/node_modules/abap-adt-api/build/`
(`AdtHTTP.js`, `api/*.js`) – **dort die Anfragen ablesen**, in Rust neu bauen (Semantik, kein Übersetzen).

**Nur lesen.** Schreibende Tools (lock/unlock/write/activate/create_object/set_text_elements) kommen in Teil 2.

## Systeme: Liste im quiver-Fenster (`Werkzeug::mehrfach`)

Wie quiver-ssh (`mehrfach = ("Systeme", "System")`, `QUIVER_ZUGAENGE`). Felder in dieser Reihenfolge:

| Feld | Art | Hilfe |
|---|---|---|
| `benutzer` | Text, Pflicht | SAP-Benutzer |
| `url` | Url, Pflicht | z. B. `https://sap-dev.firma.de:44300` |
| `mandant` | Text, Pflicht | z. B. `100` |
| `sprache` | Text | Standard `DE` |
| `passwort` | Passwort, Pflicht | SAP-Passwort |
| `zertifikat_pruefen` | Schalter | Standard an. Firmen-CAs werden über den System-Zertifikatsspeicher erkannt (`rustls-native-certs`); nur abschalten, wenn das nicht reicht. |

Jedes Tool nimmt `system` (Name); bei genau einem System optional. `abap_status` (lesend) listet die Systeme ohne
Passwort. `abap_test_connection` (lesend) meldet sich an und gibt Systeminfo zurück.

## ADT-Client (`crates/abap/src/adt/`)

- HTTP mit `reqwest` (rustls + native-certs), Cookie-Speicher je System (`SAP_SESSIONID_*`), Basic-Auth,
  `sap-client`/`sap-language` als Query, **CSRF**: `x-csrf-token: fetch` holen, bei 403 `Required` neu holen und
  einmal wiederholen. Stateless reicht für Teil 1 (Teil 2 braucht `X-sap-adt-sessiontype: stateful`).
- XML mit `quick-xml` (+ `serde`), nur die Felder, die die Tools brauchen.
- Fehler: ADT-Ausnahme (`exc:exception` → Typ, Nachricht) als MCP-Fehler, **nie** Passwort/Cookies im Text.
- 401 → klare Meldung „Anmeldung an <system> abgelehnt“ (kein Wiederholen – SAP sperrt Benutzer nach Fehlversuchen).

## Tools (alle `read_only_hint`)

`abap_status`, `abap_test_connection`, `abap_search`, `abap_object_structure`, `abap_read`, `abap_where_used`,
`abap_object_types`, `abap_node_contents`, `abap_table`, `abap_query`, `abap_syntax_check`, `abap_unit_test`,
`abap_atc`, `abap_dumps`, `abap_traces`, `abap_trace_hitlist`, `abap_transports`, `abap_transport_details`,
`abap_text_elements` – Parameter und Ausgaben wie die Vorlage, knapp formatiert.
- `abap_read`: Quelltext mit `max_zeichen` (Standard 20 000) und Kürzungshinweis.
- `abap_query`/`abap_table`: `max_zeilen` (Standard 100, höchstens 1000).
- `abap_syntax_check`, `abap_unit_test`, `abap_atc` verändern nichts im System – trotzdem lesend markieren.

## Tests (nur Attrappe)

axum-Attrappe eines ADT-Servers: CSRF-Fetch (inkl. 403 → neu holen), Cookies, Basic-Auth, je Tool die nötige
Antwort (XML-Beispiele aus `abap-adt-api` bzw. dessen Tests nachbauen). Fälle: je Tool ein Erfolg, CSRF-Erneuerung,
401 ohne Wiederholung, ADT-Ausnahme → Fehlertext, Systemwahl, kein Zugang → Hinweistext, Ende-zu-Ende über stdio,
`quiver-abap eintrag` mit `mehrfach`.

## Doku

README-Abschnitt quiver-abap (Teil 1), `docs/architektur.md`, `docs/adr/0005-adt-client-in-rust.md`
(warum eigener Client, Grenzen). Prüfbefehl grün.
