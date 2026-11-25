# 13 – quiver-jira (Jira Cloud, REST v3)

Zuerst `12-atlassian-gemeinsam.md` lesen. ADR `0008-jira.md`. `mehrfach = ("Sites", "Site")`.

Felder: `url` (Url, Pflicht, z. B. `https://firma.atlassian.net`), `email` (Text, Pflicht), `api_token` (Passwort, Pflicht, Link „API-Token erstellen“ → `https://id.atlassian.com/manage-profile/security/api-tokens`), `nur` (Projektschlüssel), `schreiben` (Schalter). Auth: Basic `email:token`.

Lesend: `jira_status`, `jira_testen` (`/myself`), `jira_projekte` (`/project/search`), `jira_suchen` (JQL über `/search/jql` mit `nextPageToken`, dazu Komfort `projekt`, `status`, `zugewiesen_an_mich`, `text`, aus dem JQL mit korrekt maskierten Zeichenketten gebaut wird), `jira_vorgang` (Felder, Beschreibung ADF → Text, Links, Unteraufgaben), `jira_kommentare`, `jira_uebergaenge`, `jira_felder_zum_anlegen` (createmeta je Projekt/Vorgangstyp), `jira_filter_liste`, `jira_filter`, `jira_verknuepfungsarten`, `jira_person_suchen`.

Verändernd: `jira_anlegen` (auch Unteraufgabe über `eltern`), `jira_aendern`, `jira_uebergang`, `jira_kommentieren`, `jira_verknuepfen` (beide Vorgänge gegen `nur` prüfen).

Text → ADF (Absätze, Zeilenumbrüche, Listen, Codeblock) und ADF → Text in eigenem, gut getestetem Modul; unbekannte ADF-Knoten verlieren keinen Text. OAuth (3LO) und Jira Data Center: in „Offen“.
