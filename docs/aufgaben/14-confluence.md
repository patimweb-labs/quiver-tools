# 14 – quiver-confluence (Confluence Cloud, REST v2, Suche über v1)

Zuerst `12-atlassian-gemeinsam.md` lesen. ADR `0009-confluence.md`. `mehrfach = ("Sites", "Site")`.

Felder wie Jira (`url`, `email`, `api_token` mit demselben Link, `nur` = Bereichsschlüssel, `schreiben`). Basis `${url}/wiki/api/v2`, CQL `${url}/wiki/rest/api/search`.

Lesend: `confluence_status`, `confluence_testen`, `confluence_bereiche`, `confluence_suchen` (CQL, dazu Komfort `text`, `bereich`, korrekt maskiert), `confluence_seite` (per Id oder Bereich + Titel; Storage-Format → Text, `als_storage` für das Original), `confluence_unterseiten`, `confluence_kommentare`, `confluence_anhaenge` (nur Liste), `confluence_labels`.

Verändernd: `confluence_seite_anlegen` (Bereich, Titel, Text, optional Elternseite), `confluence_seite_aendern`, `confluence_kommentieren`, `confluence_label_hinzufuegen`.

- **Ändern verliert nichts:** aktuelle Version lesen, `version.number + 1`, Titel und Status behalten, wenn nicht angegeben. Der Agent gibt die gelesene `version` mit (Pflicht); weicht sie vom Server ab oder kommt 409 → Meldung „Seite wurde inzwischen geändert – neu lesen“, **kein** Überschreiben.
- `text` (Standard) wird HTML-escaped ins Storage-Format gesetzt; `storage` nur ausdrücklich und dann unverändert. Genau eines von beiden.
- `anhaengen: true` hängt an den vorhandenen Inhalt an, statt ihn zu ersetzen.
- Data Center, OAuth, Anhänge hochladen: in „Offen“.
