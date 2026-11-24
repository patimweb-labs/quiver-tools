# 15 – quiver-zephyr (Zephyr Scale Cloud, REST v2)

Zuerst `12-atlassian-gemeinsam.md` lesen. ADR `0010-zephyr.md`. `mehrfach = ("Zugänge", "Zugang")`.

Felder: `basis_url` (Url, Standard `https://api.zephyrscale.smartbear.com/v2`), `api_token` (Passwort, Pflicht, Hilfe: Jira-Profil → Zephyr API Access Tokens), `projekt` (Text, Standardprojekt), `nur` (Projektschlüssel), `schreiben` (Schalter). Auth: Bearer.

Lesend: `zephyr_status`, `zephyr_testen` (`/projects?maxResults=1`), `zephyr_projekte`, `zephyr_testfaelle`, `zephyr_testfall` (mit Schritten/Skript), `zephyr_testzyklen`, `zephyr_testzyklus` (mit Zählung der Ausführungen je Status), `zephyr_ausfuehrungen`, `zephyr_ausfuehrung`, `zephyr_status_liste` (`/statuses`), `zephyr_ordner`.

Verändernd: `zephyr_testfall_anlegen`, `zephyr_zyklus_testfaelle_hinzufuegen` (je Testfall eine Ausführung), `zephyr_ergebnis_eintragen` (Status, Kommentar).

- **Mehrere Testfälle:** nacheinander; bricht einer ab, sagt die Antwort genau, welche angelegt wurden, welcher scheiterte und welche **nicht versucht** wurden. Nichts wiederholen.
- **Ergebnis eintragen darf keine Felder verlieren:** Ausführung vorher lesen; die Vorlage (`src/service.ts`) zeigt den Aufruf. Lässt sich aus der Vorlage nicht sicher ableiten, ob `PUT` ersetzt oder ergänzt, gelesene Felder mitsenden und die Annahme in der README festhalten.
- Statusname gegen `/statuses` prüfen; unbekannt → Meldung mit den erlaubten Namen, ohne Änderung.
- Zephyr Squad, Data Center, Testschritte bearbeiten: in „Offen“.
