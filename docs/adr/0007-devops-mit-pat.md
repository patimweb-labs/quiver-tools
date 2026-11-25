# 0007 – DevOps mit PAT: mehrere Organisationen, Schreiben mit Standard aus

> Stand: 2026-11-24. Aufgabe: [docs/aufgaben/09-devops.md](../../docs/aufgaben/09-devops.md).
> Baut auf [0002-zugang-ueber-quiver-felder.md](0002-zugang-ueber-quiver-felder.md) und
> [0006-abap-schreiben.md](0006-abap-schreiben.md) auf.

## Kontext

`quiver-devops` soll Azure DevOps bedienen: Arbeitsitems, Kommentare, Iterationen, Repos,
Branches, Pull Requests, Pipelines und Läufe. Die Vorlage `pi-azure-devops` (TypeScript)
tut das über `azure-devops-node-api`; für ein eigenes Rust-Programm gibt es kein SDK, die
REST-API 7.1 ist aber vollständig dokumentiert.

Drei Fragen standen am Anfang:

1. **Wie meldet sich das Tool an?** Azure DevOps kennt zwei Wege: Entra ID
   (OAuth2/Device-Code, delegierte Rechte) und Personal Access Tokens (PAT, Basic-Auth).
2. **Wie viele Zugänge?** Ein Nutzer arbeitet regelmäßig in der eigenen Organisation
   **und** in der eines Kunden – oft in einem fremden Tenant oder auf einem lokalen
   Azure DevOps Server.
3. **Darf ein Agent schreiben?** Anlegen, Kommentieren, Abstimmen und Pipelines starten
   sind echte Änderungen an einem System, in dem andere Menschen arbeiten.

## Entscheidung

### 1. PAT statt Entra ID

Jede Organisation ist ein Zugang mit `url`, optionalem `projekt`, `pat` und dem Schalter
`schreiben`. Die Anmeldung ist HTTP Basic mit leerem Benutzer und dem PAT als Passwort
(`Authorization: Basic base64(":" + PAT)`) – genau das, was die Vorlage über
`getPersonalAccessTokenHandler` tut.

**Warum nicht Entra ID:**

- **Kundenorganisationen liegen in fremden Tenants.** Eine App-Registrierung im eigenen
  Tenant hilft dort nicht; jede Kundenorganisation bräuchte eine eigene Zustimmung, oft
  durch einen Administrator des Kunden. Ein PAT stellt der Nutzer selbst aus – in
  Sekunden, mit genau den Rechten, die er braucht.
- **Azure DevOps Server kennt kein Entra.** On-Premises-Installationen
  (`https://tfs.kunde.de/tfs/DefaultCollection`) authentifizieren gegen AD bzw. lokale
  Konten. Basic mit PAT ist dort der einzige Weg, der ohne Zusatzsoftware funktioniert.
- **Kein Browser, kein Rückkanal, keine Rotation.** Ein Device-Code-Ablauf bräuchte ein
  Fenster, ein Konto und ein Token mit Ablauf; der PAT liegt ohnehin in der Tresor-Datei
  von quiver und wird vom Nutzer erneuert. Das Tool muss nichts davon wissen.
- **Kein SDK, kein OAuth-Zoo.** Basic + `api-version` + JSON-Patch sind drei Regeln; ein
  OAuth-Client mit Refresh und Scopes wäre ein Vielfaches an Code und an Fehlerquellen.

Der Preis: ein PAT ist ein langlebiges Geheimnis mit den Rechten seines Besitzers. Es
liegt in der Tresor-Datei, erscheint nur in der Kopfzeile und in keiner Antwort; erneuern
muss es der Mensch.

### 2. Eine Liste von Organisationen (`mehrfach`), Projekt optional

Wie `quiver-ssh` und `quiver-abap` pflegt der User eine Liste: `mehrfach = ("Organisationen",
"Organisation")`, quiver übergibt sie als JSON in `QUIVER_ZUGAENGE`. Jedes Werkzeug nimmt
`verbindung`; bei genau einer Organisation darf der Name fehlen.

Das Projekt steht **zweimal** zur Verfügung: fest am Zugang (wer nur ein Projekt braucht)
oder je Aufruf. Fehlt beides, antwortet das Werkzeug mit einem Hinweis und **ohne**
Anfrage – es gibt keinen geratenen Standard.

### 3. Schreiben nur mit ausdrücklichem Schalter, Standard **aus**

Das vierte Feld ist `schreiben` (Schalter, Standard `aus`). Ist es aus, antwortet jedes
der acht verändernden Werkzeuge mit „Schreiben ist für <verbindung> aus – im
quiver-Fenster einschalten.“ und schickt **keine** Anfrage.

**Warum Standard aus:** Eine Kundenorganisation ist fast immer ein Produktivsystem.
Nichts zu sagen muss die sichere Antwort sein – nicht die bequeme. Der Preis ist ein
Klick im quiver-Fenster, bevor der Agent das erste Mal kommentiert.

### 4. Grenzen, die im Client liegen

- `api-version=7.1` an jeder Anfrage; die Kommentar-Endpunkte `7.1-preview.4` und
  `_apis/connectionData` `7.1-preview.1`, weil DevOps sie noch als Vorschau führt
  (mit `7.1` antwortet der Dienst mit HTTP 400, „The resource is under preview“).
- **401 und 203 mit HTML** (DevOps leitet bei ungültigem PAT zur Anmeldung um) sind
  endgültig und werden nicht wiederholt.
- **429** wird höchstens zweimal und **nur bei lesenden** Aufrufen wiederholt
  (`Retry-After`, höchstens 10 s). Ein Anlegen oder Ändern wird nie wiederholt: ob es
  ankam, weiß nur der Server.
- Texte aus HTML werden zu Text und über `max_zeichen` gekürzt (Standard 4000); ein Log
  zeigt das Ende (Standard 200 Zeilen).

## Preis

- **Ein PAT je Organisation, vom Menschen gepflegt.** Läuft er ab, antwortet das Tool mit
  dem Hinweis auf das quiver-Fenster; es erneuert nichts selbst.
- **Rechte des PAT sind die Rechte des Tools.** Wer einen PAT mit Schreibrechten
  hinterlegt, gibt sie dem Agenten – der Schalter `schreiben` ist die zweite Bremse,
  keine Rechtetrennung. Wer wirklich trennen will, gibt einen Lese-PAT.
- **Kein Entra, kein Single Sign-on, kein Conditional Access.** In Organisationen, die
  PATs verbieten (Richtlinie `Allow creation of PATs = off`), funktioniert das Tool nicht;
  dann bleibt nur ein anderer Weg (und eine neue Aufgabe).
- **`ersteller` filtert lokal.** DevOps kennt in der Abfrage nur eine Ersteller-ID,
  keinen Anzeigenamen; bei großen Projekten kostet der Filter einen zusätzlichen Blick
  auf die ohnehin geholte Seite.
- **`devops_pull_request_abstimmen` braucht zwei Anfragen.** Die Stimme gilt für den
  angemeldeten Benutzer; seine ID kommt aus `_apis/connectionData`.

## Grenzen, die bewusst bleiben

Nicht übernommen und in [offen.md](../offen.md) vermerkt: Testpläne, Kapazität,
Board-Spalten, Branch-Policies, Auto-complete, Läufe abbrechen und wiederholen sowie
Artefakte. Die Vorlage hat sie; Aufgabe 09 nennt 26 Werkzeuge.
