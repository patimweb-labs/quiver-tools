// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Daten der lesenden ADT-Aufrufe (Aufgabe 07).
//!
//! Jede Funktion hier ist rein: XML-Text (oder Quelltext) hinein, Daten
//! heraus. Die Pfade und Feldnamen stammen aus `abap-adt-api`
//! (`build/api/*.js`), die Bedeutung aus dessen TypeScript-Typen. Was ADT
//! nicht liefert, bleibt leer – kein Feld wird geraten.

use crate::adt::fehler::AdtFehler;
use crate::adt::xml::Knoten;

// ------------------------------------------------------------------ Typen

/// Ein Treffer der Objektsuche.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Treffer {
    pub name: String,
    pub typ: String,
    pub paket: String,
    pub beschreibung: String,
    pub uri: String,
}

/// Ein Include eines Objekts (Klasse: `main`, `testclasses`, `definitions` …).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Include {
    /// `class:includeType`, z. B. `main`.
    pub art: String,
    /// Adresse des Quelltexts, falls das Include eine nennt.
    pub url: String,
}

/// Ein ABAP-Objekt: Kennwerte, Includes und die Adresse des Quelltexts.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Objekt {
    pub name: String,
    pub typ: String,
    pub beschreibung: String,
    pub verantwortlich: String,
    pub sprache: String,
    pub objekt_url: String,
    /// Wo der Hauptquelltext liegt (`/source/main` bzw. das Haupt-Include).
    pub quelltext_url: String,
    pub includes: Vec<Include>,
}

/// Ein Knoten des Objektbaums (Paketinhalt).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Baumknoten {
    pub name: String,
    pub typ: String,
    pub beschreibung: String,
    pub uri: String,
}

/// Eine durchsuchbare Objektart.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Objektart {
    pub name: String,
    pub typ: String,
    pub beschreibung: String,
    pub verwendet_fuer: Vec<String>,
}

/// Eine Stelle, die ein Objekt verwendet.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Verwendung {
    pub name: String,
    pub typ: String,
    pub beschreibung: String,
}

/// Das Ergebnis einer Tabellen- oder SQL-Abfrage.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Abfrage {
    pub spalten: Vec<String>,
    pub zeilen: Vec<Vec<String>>,
}

/// Eine Meldung des Syntax-Checks.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Syntaxmeldung {
    /// `E`, `W`, `I` wie im ADT-Bericht.
    pub schwere: String,
    pub zeile: Option<u32>,
    pub text: String,
}

/// Eine Testklasse mit ihren Methoden.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Testklasse {
    pub name: String,
    pub risiko: String,
    pub methoden: Vec<Testmethode>,
}

/// Eine Testmethode und was sie gemeldet hat.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Testmethode {
    pub name: String,
    /// `bestanden`, `fehlgeschlagen` oder `warnung`.
    pub ergebnis: String,
    pub meldungen: Vec<String>,
}

/// Ein Befund des ABAP Test Cockpits.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AtcBefund {
    pub objekt: String,
    pub typ: String,
    pub prio: u32,
    pub pruefung: String,
    pub nachricht: String,
    pub zeile: Option<u32>,
}

/// Ein Kurzspeicherauszug (Dump).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Dump {
    pub id: String,
    pub art: String,
    pub text: String,
    pub autor: String,
}

/// Eine aufgezeichnete Performance-Aufzeichnung.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Aufzeichnung {
    pub id: String,
    pub titel: String,
    pub objekt: String,
    pub zustand: String,
    pub laufzeit: f64,
}

/// Ein Eintrag der Trefferliste (Hot Spots) einer Aufzeichnung.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Hotspot {
    pub beschreibung: String,
    pub aufrufe: u64,
    pub brutto: f64,
    pub anteil: f64,
}

/// Eine Aussage (Statement) einer Aufzeichnung.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Aussage {
    pub beschreibung: String,
    pub aufrufe: u64,
    pub brutto: f64,
}

/// Ein Transportauftrag aus der Übersicht.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Transport {
    /// `Workbench` oder `Customizing`.
    pub art: String,
    pub ziel: String,
    pub nummer: String,
    pub status: String,
    pub beschreibung: String,
    pub inhaber: String,
}

/// Ein Objekt in einem Auftrag.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Auftragsobjekt {
    pub name: String,
    pub typ: String,
}

/// Eine Aufgabe eines Auftrags.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Aufgabe {
    pub nummer: String,
    pub beschreibung: String,
    pub objekte: Vec<Auftragsobjekt>,
}

/// Ein Auftrag mit Aufgaben und Objekten.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Transportdetail {
    pub nummer: String,
    pub beschreibung: String,
    pub status: String,
    pub inhaber: String,
    pub objekte: Vec<Auftragsobjekt>,
    pub aufgaben: Vec<Aufgabe>,
}

/// Ein Textelement (Symbol, Selectionstext, Überschrift).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Textelement {
    pub id: String,
    pub text: String,
    pub max_laenge: Option<usize>,
}

/// Ein Sperrvermerk auf einem Objekt (Aufgabe 08).
///
/// `POST <objekt-url>?_action=LOCK` antwortet mit einem `asx`-Dokument, dessen
/// `DATA` den Handle, den Transportauftrag und die Ortsmarke trägt
/// (`abap-adt-api/build/api/objectcontents.js`, `lock`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Sperre {
    /// Der Handle, den jedes Schreiben in dieser Sitzung mitgeben muss.
    pub handle: String,
    /// Der Transportauftrag (`CORRNR`), leer bei lokalen Objekten.
    pub auftrag: String,
    /// `X` heißt: lokales Objekt (`$TMP`), kein Transport nötig.
    pub lokal: bool,
}

/// Eine Meldung der Aktivierung.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Aktivierungsmeldung {
    /// `E` (Fehler), `W` (Warnung), `I` (Information), `S` (Erfolg), `A`/`X` (Abbruch).
    pub art: String,
    pub zeile: Option<u32>,
    pub text: String,
}

/// Das Ergebnis einer Aktivierung (`/sap/bc/adt/activation`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Aktivierung {
    /// `false`, sobald eine Meldung ein Fehler ist oder Objekte inaktiv blieben.
    pub erfolg: bool,
    pub meldungen: Vec<Aktivierungsmeldung>,
    /// Wie viele Objekte nach der Aktivierung inaktiv blieben.
    pub inaktiv: usize,
}

// -------------------------------------------------------------- Zugriff

/// `text` in einen Baum lesen.
fn baum(xml: &str) -> Result<Knoten, AdtFehler> {
    Knoten::parse(xml)
}

/// Ein Kindelement mit Text, falls vorhanden.
fn text(knoten: &Knoten, name: &str) -> String {
    knoten
        .kind_text(name)
        .map(str::trim)
        .filter(|wert| !wert.is_empty())
        .unwrap_or_default()
        .to_string()
}

/// Ein Attribut als Text, falls vorhanden.
fn attr(knoten: &Knoten, name: &str) -> String {
    knoten.attr_gefuellt(name).unwrap_or_default().to_string()
}

/// Eine Zahl aus einem Attribut oder Text.
fn zahl(wert: Option<&str>) -> f64 {
    wert.map(str::trim)
        .and_then(|wert| wert.replace(',', ".").parse::<f64>().ok())
        .unwrap_or(0.0)
}

// ---------------------------------------------------------------- Suche

/// Treffer der Objektsuche (`/repository/informationsystem/search`).
pub fn treffer(xml: &str) -> Result<Vec<Treffer>, AdtFehler> {
    let baum = baum(xml)?;
    Ok(baum
        .alle_unter(&["objectReferences"], "objectReference")
        .into_iter()
        .map(|knoten| {
            let mut name = attr(knoten, "name");
            let mut beschreibung = attr(knoten, "description");
            // Ältere Systeme liefern `ZREPORT (PROGRAM)` als Namen.
            if let Some((vorne, hinten)) = zerlege_name(&name) {
                name = vorne;
                if beschreibung.is_empty() {
                    beschreibung = hinten;
                }
            }
            Treffer {
                name,
                typ: attr(knoten, "type"),
                paket: attr(knoten, "packageName"),
                beschreibung,
                uri: attr(knoten, "uri"),
            }
        })
        .collect())
}

/// `ZREPORT (PROGRAM)` → (`ZREPORT`, `PROGRAM`).
fn zerlege_name(name: &str) -> Option<(String, String)> {
    let (vorne, hinten) = name.split_once(" (")?;
    let hinten = hinten.strip_suffix(')')?;
    if vorne.is_empty() || hinten.is_empty() {
        return None;
    }
    Some((vorne.trim().to_string(), hinten.trim().to_string()))
}

// ------------------------------------------------------- Objektstruktur

/// Struktur eines Objekts (`GET <objekt-url>`).
pub fn objekt(xml: &str, objekt_url: &str) -> Result<Objekt, AdtFehler> {
    let baum = baum(xml)?;
    let wurzel = baum.kinder.first().ok_or_else(|| AdtFehler::Antwort {
        text: format!("Zu {objekt_url} kam eine leere Antwort."),
    })?;

    if attr(wurzel, "type").is_empty() && attr(wurzel, "name").is_empty() {
        return Err(AdtFehler::Antwort {
            text: format!("Die Antwort zu {objekt_url} ist keine Objektstruktur."),
        });
    }

    let includes: Vec<Include> = wurzel
        .kinder_mit("include")
        .map(|include| {
            let url = include_quelle(include)
                .map(|quelle| folge_url(objekt_url, quelle))
                .unwrap_or_default();
            Include {
                art: attr(include, "includeType"),
                url,
            }
        })
        .collect();

    let quelltext_url = quelltext_url(wurzel, objekt_url);

    Ok(Objekt {
        name: attr(wurzel, "name"),
        typ: attr(wurzel, "type"),
        beschreibung: attr(wurzel, "description"),
        verantwortlich: attr(wurzel, "responsible"),
        sprache: attr(wurzel, "language"),
        objekt_url: objekt_url.to_string(),
        quelltext_url,
        includes,
    })
}

/// Wo der Hauptquelltext liegt – dieselben Regeln wie `ADTClient.mainInclude`.
fn quelltext_url(wurzel: &Knoten, objekt_url: &str) -> String {
    // Pakete haben keinen Quelltext.
    if attr(wurzel, "type").starts_with("DEVC") {
        return objekt_url.to_string();
    }
    // Klassen: das Include `main` mit einem Klartext-Link.
    if let Some(main) = wurzel
        .kinder_mit("include")
        .find(|include| attr(include, "includeType") == "main")
    {
        if let Some(quelle) = include_quelle(main) {
            return folge_url(objekt_url, quelle);
        }
    }
    // Programme und andere: `abapsource:sourceUri` oder ein Klartext-Link.
    if let Some(quelle) = wurzel.attr_gefuellt("sourceUri") {
        return folge_url(objekt_url, quelle);
    }
    if let Some(href) = wurzel
        .kinder_mit("link")
        .find(|link| link.attr("type") == Some("text/plain"))
        .and_then(|link| link.attr_gefuellt("href"))
    {
        return folge_url(objekt_url, href);
    }
    format!("{}/source/main", objekt_url.trim_end_matches('/'))
}

/// Wo der Quelltext eines Includes liegt: `sourceUri`, sonst der Klartext-Link.
///
/// Ein Include trägt auch Links **ohne** Typ, z. B. auf seine Versionen; die
/// sind kein Quelltext.
fn include_quelle(include: &Knoten) -> Option<&str> {
    include.attr_gefuellt("sourceUri").or_else(|| {
        include
            .kinder_mit("link")
            .find(|link| link.attr("type") == Some("text/plain"))
            .and_then(|link| link.attr_gefuellt("href"))
    })
}

/// Eine relative Adresse an der Objektadresse ausrichten, wie ADT sie meint:
/// `source/main` liegt **unter** dem Objekt, `./t000/source/main` neben ihm.
pub fn folge_url(objekt_url: &str, href: &str) -> String {
    let href = href.trim();
    if href.starts_with("http://") || href.starts_with("https://") || href.starts_with('/') {
        return href.to_string();
    }
    let basis = objekt_url.trim_end_matches('/');
    match href.strip_prefix("./") {
        Some(rest) => match basis.rfind('/') {
            Some(stelle) => format!("{}/{}", &basis[..stelle], rest),
            None => rest.to_string(),
        },
        None => format!("{basis}/{href}"),
    }
}

// ------------------------------------------------------------ Verwendung

/// Wo-eingesetzt-Liste (`/repository/informationsystem/usageReferences`).
pub fn verwendungen(xml: &str) -> Result<Vec<Verwendung>, AdtFehler> {
    let baum = baum(xml)?;
    Ok(baum
        .alle("referencedObject")
        .into_iter()
        .map(|knoten| {
            let objekt = knoten.kind("adtObject").unwrap_or(knoten);
            Verwendung {
                name: {
                    let name = attr(objekt, "name");
                    if name.is_empty() {
                        attr(knoten, "uri")
                    } else {
                        name
                    }
                },
                typ: attr(objekt, "type"),
                beschreibung: attr(objekt, "description"),
            }
        })
        .collect())
}

// ----------------------------------------------------------- Objektarten

/// Die durchsuchbaren Objektarten (`/repository/informationsystem/objecttypes`).
pub fn objektarten(xml: &str) -> Result<Vec<Objektart>, AdtFehler> {
    let baum = baum(xml)?;
    Ok(baum
        .alle("namedItem")
        .into_iter()
        .filter_map(|knoten| {
            let name = text(knoten, "name");
            if name.is_empty() {
                return None;
            }
            let daten = text(knoten, "data");
            let mut typ = String::new();
            let mut verwendet_fuer = Vec::new();
            for teil in daten.split(';') {
                let (schluessel, wert) = teil.split_once(':').unwrap_or((teil, ""));
                match schluessel.trim() {
                    "type" => typ = wert.trim().to_string(),
                    "usedBy" => {
                        verwendet_fuer = wert
                            .split(',')
                            .map(str::trim)
                            .filter(|wert| !wert.is_empty())
                            .map(str::to_string)
                            .collect()
                    }
                    _ => {}
                }
            }
            Some(Objektart {
                name,
                typ,
                beschreibung: text(knoten, "description"),
                verwendet_fuer,
            })
        })
        .collect())
}

// -------------------------------------------------------------- Knoten

/// Der Inhalt eines Pakets oder einer Funktionsgruppe (`/repository/nodestructure`).
pub fn baumknoten(xml: &str) -> Result<Vec<Baumknoten>, AdtFehler> {
    let baum = baum(xml)?;
    Ok(baum
        .alle("SEU_ADT_REPOSITORY_OBJ_NODE")
        .into_iter()
        .map(|knoten| Baumknoten {
            name: text(knoten, "OBJECT_NAME"),
            typ: text(knoten, "OBJECT_TYPE"),
            beschreibung: text(knoten, "DESCRIPTION"),
            uri: text(knoten, "OBJECT_URI"),
        })
        .collect())
}

// -------------------------------------------------------------- Abfrage

/// Das Ergebnis einer Datenvorschau oder freien SQL-Abfrage.
pub fn abfrage(xml: &str) -> Result<Abfrage, AdtFehler> {
    let baum = baum(xml)?;
    let spalten_knoten: Vec<&Knoten> = baum.alle("columns");

    let mut spalten = Vec::with_capacity(spalten_knoten.len());
    let mut werte: Vec<Vec<String>> = Vec::with_capacity(spalten_knoten.len());
    for spalte in &spalten_knoten {
        spalten.push(
            spalte
                .kind("metadata")
                .map(|meta| attr(meta, "name"))
                .unwrap_or_default(),
        );
        werte.push(
            spalte
                .alle("data")
                .into_iter()
                .map(|data| data.text.trim().to_string())
                .collect(),
        );
    }

    let zeilen_anzahl = werte.iter().map(Vec::len).max().unwrap_or(0);
    let zeilen = (0..zeilen_anzahl)
        .map(|index| {
            werte
                .iter()
                .map(|spalte| spalte.get(index).cloned().unwrap_or_default())
                .collect()
        })
        .collect();

    Ok(Abfrage { spalten, zeilen })
}

// --------------------------------------------------------- Syntax-Check

/// Die Meldungen eines Syntax-Checks (`/checkruns`).
pub fn syntaxmeldungen(xml: &str) -> Result<Vec<Syntaxmeldung>, AdtFehler> {
    let baum = baum(xml)?;
    Ok(baum
        .alle("checkMessage")
        .into_iter()
        .map(|meldung| Syntaxmeldung {
            schwere: attr(meldung, "type"),
            zeile: zeile_aus_uri(meldung.attr("uri").unwrap_or_default()),
            text: attr(meldung, "shortText"),
        })
        .collect())
}

/// `…#start=12,3` → `12`.
fn zeile_aus_uri(uri: &str) -> Option<u32> {
    let (_, rest) = uri.split_once("start=")?;
    rest.split([',', ';']).next()?.trim().parse().ok()
}

// ----------------------------------------------------------- Unit-Tests

/// Die Testklassen eines ABAP-Unit-Laufs (`/abapunit/testruns`).
pub fn testklassen(xml: &str) -> Result<Vec<Testklasse>, AdtFehler> {
    let baum = baum(xml)?;
    Ok(baum
        .alle("testClass")
        .into_iter()
        .map(|klasse| Testklasse {
            name: attr(klasse, "name"),
            risiko: attr(klasse, "riskLevel"),
            methoden: klasse
                .alle("testMethod")
                .into_iter()
                .map(testmethode)
                .collect(),
        })
        .collect())
}

/// Eine Testmethode samt ihren Warnungen.
fn testmethode(methode: &Knoten) -> Testmethode {
    let mut meldungen = Vec::new();
    let mut ergebnis = "bestanden".to_string();
    for alert in methode.alle("alert") {
        let schwere = attr(alert, "severity").to_ascii_lowercase();
        if schwere == "critical" || schwere == "fatal" {
            ergebnis = "fehlgeschlagen".to_string();
        } else if ergebnis == "bestanden" {
            ergebnis = "warnung".to_string();
        }
        let titel = {
            let als_attribut = attr(alert, "title");
            if als_attribut.is_empty() {
                text(alert, "title")
            } else {
                als_attribut
            }
        };
        if !titel.is_empty() {
            meldungen.push(titel);
        }
        for detail in alert.alle("detail") {
            let inhalt = attr(detail, "text");
            if !inhalt.is_empty() {
                meldungen.push(inhalt);
            }
        }
    }
    Testmethode {
        name: attr(methode, "name"),
        ergebnis,
        meldungen,
    }
}

// ------------------------------------------------------------------ ATC

/// Die Kennung eines gestarteten ATC-Laufs (`/atc/runs`).
pub fn atc_lauf(xml: &str) -> Result<String, AdtFehler> {
    let baum = baum(xml)?;
    let id = baum
        .alle("worklistId")
        .first()
        .map(|knoten| knoten.text.trim().to_string())
        .filter(|id| !id.is_empty())
        .ok_or_else(|| AdtFehler::Antwort {
            text: "Der ATC-Lauf hat keine Kennung zurückgegeben.".to_string(),
        })?;
    Ok(id)
}

/// Die Befunde einer ATC-Prüfliste (`/atc/worklists/<id>`).
pub fn atc_befunde(xml: &str) -> Result<Vec<AtcBefund>, AdtFehler> {
    let baum = baum(xml)?;
    let mut befunde = Vec::new();
    for objekt in baum.alle("object") {
        let name = attr(objekt, "name");
        let typ = attr(objekt, "type");
        for finding in objekt.alle("finding") {
            befunde.push(AtcBefund {
                objekt: name.clone(),
                typ: typ.clone(),
                prio: attr(finding, "priority").parse().unwrap_or(0),
                pruefung: attr(finding, "checkTitle"),
                nachricht: attr(finding, "messageTitle"),
                zeile: zeile_aus_uri(finding.attr("location").unwrap_or_default()),
            });
        }
    }
    Ok(befunde)
}

// ---------------------------------------------------------------- Dumps

/// Die Kurzspeicherauszüge (`/runtime/dumps`).
pub fn dumps(xml: &str) -> Result<Vec<Dump>, AdtFehler> {
    let baum = baum(xml)?;
    Ok(baum
        .alle("entry")
        .into_iter()
        .map(|eintrag| Dump {
            id: text(eintrag, "id"),
            art: eintrag
                .kind("summary")
                .map(|summary| attr(summary, "type"))
                .unwrap_or_default(),
            text: text(eintrag, "summary"),
            autor: eintrag
                .kind("author")
                .map(|autor| text(autor, "name"))
                .unwrap_or_default(),
        })
        .collect())
}

// --------------------------------------------------------------- Traces

/// Die Aufzeichnungen eines Benutzers (`/runtime/traces/abaptraces`).
pub fn aufzeichnungen(xml: &str) -> Result<Vec<Aufzeichnung>, AdtFehler> {
    let baum = baum(xml)?;
    Ok(baum
        .alle("entry")
        .into_iter()
        .map(|eintrag| {
            let erweitert = eintrag.kind("extendedData");
            let wert = |name: &str| erweitert.map(|daten| text(daten, name)).unwrap_or_default();
            Aufzeichnung {
                id: text(eintrag, "id"),
                titel: text(eintrag, "title"),
                objekt: wert("objectName"),
                zustand: erweitert
                    .and_then(|daten| daten.kind("state"))
                    .map(|zustand| attr(zustand, "text"))
                    .unwrap_or_default(),
                laufzeit: zahl(Some(&wert("runtime"))),
            }
        })
        .collect())
}

/// Die Trefferliste einer Aufzeichnung (`…/hitlist`).
pub fn hotspots(xml: &str) -> Result<Vec<Hotspot>, AdtFehler> {
    let baum = baum(xml)?;
    Ok(baum
        .alle("entry")
        .into_iter()
        .map(|eintrag| {
            let brutto = eintrag.kind("grossTime");
            Hotspot {
                beschreibung: attr(eintrag, "description"),
                aufrufe: attr(eintrag, "hitCount").parse().unwrap_or(0),
                brutto: brutto.map(|zeit| zahl(zeit.attr("time"))).unwrap_or(0.0),
                anteil: brutto
                    .map(|zeit| zahl(zeit.attr("percentage")))
                    .unwrap_or(0.0),
            }
        })
        .collect())
}

/// Die Aussagen einer Aufzeichnung (`…/statements`).
pub fn aussagen(xml: &str) -> Result<Vec<Aussage>, AdtFehler> {
    let baum = baum(xml)?;
    Ok(baum
        .alle("statement")
        .into_iter()
        .map(|aussage| Aussage {
            beschreibung: attr(aussage, "description"),
            aufrufe: attr(aussage, "hitCount").parse().unwrap_or(0),
            brutto: aussage
                .kind("grossTime")
                .map(|zeit| zahl(zeit.attr("time")))
                .unwrap_or(0.0),
        })
        .collect())
}

// ----------------------------------------------------------- Transporte

/// Die Aufträge eines Benutzers (`/cts/transportrequests`).
pub fn transporte(xml: &str) -> Result<Vec<Transport>, AdtFehler> {
    let baum = baum(xml)?;
    let mut alle = Vec::new();
    for (zweig, art) in [("workbench", "Workbench"), ("customizing", "Customizing")] {
        for ziel in baum.alle_unter(&["root", zweig], "target") {
            let zielname = attr(ziel, "name");
            let zieldesc = attr(ziel, "desc");
            for auftrag in ziel
                .kinder_mit("modifiable")
                .chain(ziel.kinder_mit("released"))
                .flat_map(|gruppe| gruppe.kinder_mit("request"))
            {
                alle.push(Transport {
                    art: art.to_string(),
                    ziel: if zieldesc.is_empty() {
                        zielname.clone()
                    } else {
                        format!("{zielname} ({zieldesc})")
                    },
                    nummer: attr(auftrag, "number"),
                    status: attr(auftrag, "status"),
                    beschreibung: attr(auftrag, "desc"),
                    inhaber: attr(auftrag, "owner"),
                });
            }
        }
    }
    Ok(alle)
}

/// Ein Auftrag mit Aufgaben und Objekten (`/cts/transportrequests/<nr>`).
pub fn transportdetails(xml: &str) -> Result<Transportdetail, AdtFehler> {
    let baum = baum(xml)?;
    let auftrag = baum
        .alle_unter(&["root"], "request")
        .into_iter()
        .next()
        .ok_or_else(|| AdtFehler::Antwort {
            text: "Die Antwort enthielt keinen Transportauftrag.".to_string(),
        })?;

    let objekte = |knoten: &Knoten| -> Vec<Auftragsobjekt> {
        knoten
            .kinder_mit("abap_object")
            .map(|objekt| Auftragsobjekt {
                name: attr(objekt, "name"),
                typ: attr(objekt, "type"),
            })
            .collect()
    };

    Ok(Transportdetail {
        nummer: attr(auftrag, "number"),
        beschreibung: attr(auftrag, "desc"),
        status: attr(auftrag, "status"),
        inhaber: attr(auftrag, "owner"),
        objekte: objekte(auftrag),
        aufgaben: auftrag
            .kinder_mit("task")
            .map(|aufgabe| Aufgabe {
                nummer: attr(aufgabe, "number"),
                beschreibung: attr(aufgabe, "desc"),
                objekte: objekte(aufgabe),
            })
            .collect(),
    })
}

// -------------------------------------------------------- Textelemente

/// Textelemente aus dem Klartext des Endpunkts (`<url>/source/<kategorie>`).
///
/// Das Format ist zeilenweise: `@MaxLength:30` gilt für die nächste Zeile,
/// `ID=Text` ist ein Element.
pub fn textelemente(text: &str) -> Vec<Textelement> {
    let mut elemente = Vec::new();
    let mut max_laenge = None;
    for zeile in text.lines() {
        let zeile = zeile.trim();
        if let Some(rest) = zeile.strip_prefix("@MaxLength:") {
            max_laenge = rest.trim().parse::<usize>().ok();
        } else if let Some((id, inhalt)) = zeile.split_once('=') {
            let id = id.trim();
            if id.is_empty() {
                continue;
            }
            elemente.push(Textelement {
                id: id.to_string(),
                text: inhalt.to_string(),
                max_laenge: max_laenge.take(),
            });
        }
    }
    elemente
}

/// Die Adresse der Textelemente eines Objekts (`ADTClient.textElementsUrl`).
pub fn textelemente_url(typ: &str, name: &str) -> String {
    let klein = name.to_lowercase();
    let kodiert = if klein.contains('/') {
        klein.replace('/', "%2F")
    } else {
        klein
    };
    let art = if typ.to_uppercase().starts_with("CLAS") {
        "classes"
    } else if typ.to_uppercase().starts_with("FUGR") {
        "functiongroups"
    } else {
        "programs"
    };
    format!("/sap/bc/adt/textelements/{art}/{kodiert}")
}

// --------------------------------------------------------- Sperren (Teil 2)

/// Ein Sperrvermerk aus der Antwort von `?_action=LOCK`.
pub fn sperre(xml: &str) -> Result<Sperre, AdtFehler> {
    let baum = baum(xml)?;
    let daten = baum
        .alle("DATA")
        .into_iter()
        .next()
        .ok_or_else(|| AdtFehler::Antwort {
            text: "Die Sperre kam ohne Handle zurück.".to_string(),
        })?;
    let handle = text(daten, "LOCK_HANDLE");
    if handle.is_empty() {
        return Err(AdtFehler::Antwort {
            text: "Die Sperre kam ohne LOCK_HANDLE zurück.".to_string(),
        });
    }
    Ok(Sperre {
        handle,
        auftrag: text(daten, "CORRNR"),
        lokal: matches!(text(daten, "IS_LOCAL").as_str(), "X" | "x" | "true"),
    })
}

// ----------------------------------------------------- Aktivieren (Teil 2)

/// Das Ergebnis einer Aktivierung.
///
/// Meldungen stehen als `chkl:msg` mit `type`, `line` und einem `shortText`
/// (darunter `txt`) in der Antwort; inaktive Objekte als `ioc:entry` unter
/// `ioc:inactiveObjects` (`abap-adt-api/build/api/activate.js`).
pub fn aktivierung(xml: &str) -> Result<Aktivierung, AdtFehler> {
    let baum = baum(xml)?;
    let meldungen: Vec<Aktivierungsmeldung> = baum
        .alle("msg")
        .into_iter()
        .map(|meldung| {
            let text = {
                let als_attribut = attr(meldung, "shortText");
                if als_attribut.is_empty() {
                    meldung
                        .alle("txt")
                        .first()
                        .map(|txt| txt.text.trim().to_string())
                        .unwrap_or_default()
                } else {
                    als_attribut
                }
            };
            Aktivierungsmeldung {
                art: attr(meldung, "type"),
                zeile: attr(meldung, "line").parse().ok(),
                text,
            }
        })
        .collect();
    let inaktiv = baum
        .alle("inactiveObjects")
        .into_iter()
        .map(|liste| liste.kinder_mit("entry").count())
        .sum();
    let fehler = meldungen.iter().any(|meldung| {
        let art = meldung.art.to_ascii_uppercase();
        art.starts_with('E') || art.starts_with('A') || art.starts_with('X')
    });
    Ok(Aktivierung {
        erfolg: !fehler && inaktiv == 0,
        meldungen,
        inaktiv,
    })
}

/// Die Titel der Bereiche aus der Core-Discovery (`/core/discovery`).
pub fn discovery_titel(xml: &str) -> Result<Vec<String>, AdtFehler> {
    let baum = baum(xml)?;
    Ok(baum
        .alle("workspace")
        .into_iter()
        .map(|bereich| text(bereich, "title"))
        .filter(|titel| !titel.is_empty())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUCHE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
    <adtcore:objectReferences xmlns:adtcore="http://www.sap.com/adt/core">
      <adtcore:objectReference adtcore:uri="/sap/bc/adt/oo/classes/zcl_demo" adtcore:type="CLAS/OC"
        adtcore:name="ZCL_DEMO" adtcore:packageName="$TMP" adtcore:description="Demo-Klasse"/>
      <adtcore:objectReference adtcore:uri="/sap/bc/adt/programs/programs/zreport" adtcore:type="PROG/P"
        adtcore:name="zreport (PROGRAM)" adtcore:packageName="$TMP"/>
    </adtcore:objectReferences>"#;

    const KLASSE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
    <class:abapClass xmlns:class="http://www.sap.com/adt/oo/classes"
      xmlns:adtcore="http://www.sap.com/adt/core" xmlns:atom="http://www.w3.org/2005/Atom"
      adtcore:name="ZCL_DEMO" adtcore:type="CLAS/OC" adtcore:description="Demo-Klasse"
      adtcore:responsible="DEVELOPER" adtcore:language="DE">
      <atom:link href="/sap/bc/adt/oo/classes/zcl_demo" rel="self" type="text/html"/>
      <class:include class:includeType="definitions">
        <atom:link href="/sap/bc/adt/oo/classes/zcl_demo/includes/definitions" rel="self" type="text/plain"/>
      </class:include>
      <class:include class:includeType="main">
        <atom:link href="/sap/bc/adt/oo/classes/zcl_demo/includes/implementations" rel="self" type="text/plain"/>
      </class:include>
    </class:abapClass>"#;

    const PROGRAMM: &str = r#"<?xml version="1.0" encoding="utf-8"?>
    <program:abapProgram xmlns:program="http://www.sap.com/adt/programs/programs"
      xmlns:adtcore="http://www.sap.com/adt/core" xmlns:atom="http://www.w3.org/2005/Atom"
      adtcore:name="ZREPORT" adtcore:type="PROG/P" adtcore:description="Report"
      adtcore:responsible="DEVELOPER" adtcore:language="DE">
      <atom:link href="/sap/bc/adt/programs/programs/zreport" rel="self" type="text/html"/>
    </program:abapProgram>"#;

    #[test]
    fn die_suche_liefert_treffer_und_trennt_alte_namen() {
        let treffer = treffer(SUCHE).unwrap();
        assert_eq!(treffer.len(), 2);
        assert_eq!(treffer[0].name, "ZCL_DEMO");
        assert_eq!(treffer[0].typ, "CLAS/OC");
        assert_eq!(treffer[0].paket, "$TMP");
        assert_eq!(treffer[0].beschreibung, "Demo-Klasse");
        // `zreport (PROGRAM)` wird getrennt.
        assert_eq!(treffer[1].name, "zreport");
        assert_eq!(treffer[1].beschreibung, "PROGRAM");
    }

    #[test]
    fn eine_klasse_kennt_ihre_includes_und_den_hauptquelltext() {
        let gelesen = objekt(KLASSE, "/sap/bc/adt/oo/classes/zcl_demo").unwrap();
        assert_eq!(gelesen.name, "ZCL_DEMO");
        assert_eq!(gelesen.typ, "CLAS/OC");
        assert_eq!(gelesen.beschreibung, "Demo-Klasse");
        assert_eq!(gelesen.verantwortlich, "DEVELOPER");
        assert_eq!(gelesen.sprache, "DE");
        assert_eq!(gelesen.includes.len(), 2);
        assert_eq!(gelesen.includes[1].art, "main");
        assert_eq!(
            gelesen.quelltext_url,
            "/sap/bc/adt/oo/classes/zcl_demo/includes/implementations"
        );
    }

    /// So antwortet ein echtes System (gekürzt): relative Adressen, dazu ein
    /// Link ohne Typ auf die Versionen.
    #[test]
    fn echte_relative_adressen_fuehren_zum_quelltext() {
        let klasse = r#"<class:abapClass xmlns:class="c" xmlns:adtcore="a"
            xmlns:atom="http://www.w3.org/2005/Atom" xmlns:abapsource="s"
            adtcore:name="CL_X" adtcore:type="CLAS/OC" abapsource:sourceUri="source/main">
          <class:include class:includeType="main" abapsource:sourceUri="source/main">
            <atom:link href="includes/main/versions" rel="http://www.sap.com/adt/relations/versions"/>
            <atom:link href="source/main" rel="http://www.sap.com/adt/relations/source" type="text/plain"/>
            <atom:link href="source/main" rel="http://www.sap.com/adt/relations/source" type="text/html"/>
          </class:include>
        </class:abapClass>"#;
        let gelesen = objekt(klasse, "/sap/bc/adt/oo/classes/cl_x").unwrap();
        assert_eq!(
            gelesen.quelltext_url,
            "/sap/bc/adt/oo/classes/cl_x/source/main"
        );
        assert_eq!(
            gelesen.includes[0].url,
            "/sap/bc/adt/oo/classes/cl_x/source/main"
        );
        // Nur ein Link ohne Typ: der ist kein Quelltext.
        let ohne = klasse
            .replace(r#" abapsource:sourceUri="source/main""#, "")
            .replace(r#" type="text/plain""#, "");
        assert_eq!(
            objekt(&ohne, "/sap/bc/adt/oo/classes/cl_x")
                .unwrap()
                .quelltext_url,
            "/sap/bc/adt/oo/classes/cl_x/source/main"
        );
        let tabelle = r#"<blue:blueSource xmlns:blue="b" xmlns:adtcore="a" xmlns:abapsource="s"
            adtcore:name="T000" adtcore:type="TABL/DT" abapsource:sourceUri="./t000/source/main"/>"#;
        assert_eq!(
            objekt(tabelle, "/sap/bc/adt/ddic/tables/t000")
                .unwrap()
                .quelltext_url,
            "/sap/bc/adt/ddic/tables/t000/source/main"
        );
    }

    #[test]
    fn ein_programm_bekommt_den_standardpfad_und_ein_paket_keinen() {
        let gelesen = objekt(PROGRAMM, "/sap/bc/adt/programs/programs/zreport").unwrap();
        assert_eq!(
            gelesen.quelltext_url,
            "/sap/bc/adt/programs/programs/zreport/source/main"
        );
        assert!(gelesen.includes.is_empty());

        let paket = r#"<paket:abapPackage xmlns:paket="x" xmlns:adtcore="y"
            adtcore:name="$TMP" adtcore:type="DEVC/K"/>"#;
        let gelesen = objekt(paket, "/sap/bc/adt/packages/%24tmp").unwrap();
        assert_eq!(gelesen.quelltext_url, "/sap/bc/adt/packages/%24tmp");
    }

    #[test]
    fn relative_adressen_werden_ausgerichtet() {
        assert_eq!(folge_url("/a/b/c", "d/e"), "/a/b/c/d/e");
        assert_eq!(folge_url("/a/b/c/", "d/e"), "/a/b/c/d/e");
        assert_eq!(folge_url("/a/b/c", "./c/d"), "/a/b/c/d");
        assert_eq!(folge_url("/a/b/c", "/x/y"), "/x/y");
        assert_eq!(
            folge_url("/a/b/c", "https://h/x"),
            "https://h/x".to_string()
        );
    }

    #[test]
    fn verwendungen_lesen_name_und_typ() {
        let xml = r#"<usageReferences:usageReferenceResult xmlns:usageReferences="u"
            xmlns:adtcore="a">
          <usageReferences:referencedObjects>
            <usageReferences:referencedObject uri="/sap/bc/adt/programs/programs/zreport/source/main#start=10,4">
              <usageReferences:adtObject adtcore:name="ZREPORT" adtcore:type="PROG/P"
                adtcore:description="Report"/>
            </usageReferences:referencedObject>
          </usageReferences:referencedObjects>
        </usageReferences:usageReferenceResult>"#;
        let verwendungen = verwendungen(xml).unwrap();
        assert_eq!(verwendungen.len(), 1);
        assert_eq!(verwendungen[0].name, "ZREPORT");
        assert_eq!(verwendungen[0].typ, "PROG/P");
        assert_eq!(verwendungen[0].beschreibung, "Report");
    }

    #[test]
    fn objektarten_lesen_typ_und_verwendung() {
        let xml = r#"<nameditem:namedItemList xmlns:nameditem="n">
          <nameditem:namedItem>
            <nameditem:name>CLAS</nameditem:name>
            <nameditem:description>Klasse</nameditem:description>
            <nameditem:data>type:CLAS/OC;usedBy:search,whereUsed</nameditem:data>
          </nameditem:namedItem>
        </nameditem:namedItemList>"#;
        let arten = objektarten(xml).unwrap();
        assert_eq!(arten.len(), 1);
        assert_eq!(arten[0].name, "CLAS");
        assert_eq!(arten[0].typ, "CLAS/OC");
        assert_eq!(arten[0].beschreibung, "Klasse");
        assert_eq!(arten[0].verwendet_fuer, vec!["search", "whereUsed"]);
    }

    #[test]
    fn der_baum_liest_die_knoten() {
        let xml = r#"<asx:abap xmlns:asx="a"><asx:values><DATA>
          <TREE_CONTENT>
            <SEU_ADT_REPOSITORY_OBJ_NODE>
              <OBJECT_TYPE>CLAS/OC</OBJECT_TYPE>
              <OBJECT_NAME>ZCL_DEMO</OBJECT_NAME>
              <OBJECT_URI>/sap/bc/adt/oo/classes/zcl_demo</OBJECT_URI>
              <DESCRIPTION>Demo</DESCRIPTION>
            </SEU_ADT_REPOSITORY_OBJ_NODE>
          </TREE_CONTENT>
        </DATA></asx:values></asx:abap>"#;
        let knoten = baumknoten(xml).unwrap();
        assert_eq!(knoten.len(), 1);
        assert_eq!(knoten[0].name, "ZCL_DEMO");
        assert_eq!(knoten[0].typ, "CLAS/OC");
        assert_eq!(knoten[0].uri, "/sap/bc/adt/oo/classes/zcl_demo");
        assert_eq!(knoten[0].beschreibung, "Demo");
    }

    #[test]
    fn eine_abfrage_wird_zu_spalten_und_zeilen() {
        let xml = r#"<dataPreview:tableData xmlns:dataPreview="d">
          <dataPreview:columns>
            <dataPreview:metadata dataPreview:name="CARRID" dataPreview:type="C"/>
            <dataPreview:dataSet>
              <dataPreview:data>AA</dataPreview:data>
              <dataPreview:data>LH</dataPreview:data>
            </dataPreview:dataSet>
          </dataPreview:columns>
          <dataPreview:columns>
            <dataPreview:metadata dataPreview:name="CONNID" dataPreview:type="N"/>
            <dataPreview:dataSet>
              <dataPreview:data>17</dataPreview:data>
              <dataPreview:data>0400</dataPreview:data>
            </dataPreview:dataSet>
          </dataPreview:columns>
        </dataPreview:tableData>"#;
        let abfrage = abfrage(xml).unwrap();
        assert_eq!(abfrage.spalten, vec!["CARRID", "CONNID"]);
        assert_eq!(abfrage.zeilen.len(), 2);
        assert_eq!(abfrage.zeilen[0], vec!["AA", "17"]);
        assert_eq!(abfrage.zeilen[1], vec!["LH", "0400"]);
    }

    #[test]
    fn der_syntaxcheck_liest_schwere_zeile_und_text() {
        let xml = r#"<chkrun:checkRunReports xmlns:chkrun="c">
          <chkrun:checkReport>
            <chkrun:checkMessageList>
              <chkrun:checkMessage chkrun:uri="/sap/bc/adt/programs/programs/zreport/source/main#start=12,3"
                chkrun:type="W" chkrun:shortText="Variable &quot;lv_x&quot; wird nicht verwendet."/>
            </chkrun:checkMessageList>
          </chkrun:checkReport>
        </chkrun:checkRunReports>"#;
        let meldungen = syntaxmeldungen(xml).unwrap();
        assert_eq!(meldungen.len(), 1);
        assert_eq!(meldungen[0].schwere, "W");
        assert_eq!(meldungen[0].zeile, Some(12));
        assert_eq!(meldungen[0].text, "Variable \"lv_x\" wird nicht verwendet.");
    }

    #[test]
    fn unit_tests_lesen_klassen_methoden_und_alerten() {
        let xml = r#"<aunit:runResult xmlns:aunit="a" xmlns:adtcore="c">
          <program adtcore:name="ZCL_DEMO">
            <testClasses>
              <testClass adtcore:name="LTCL_DEMO" riskLevel="harmless">
                <testMethods>
                  <testMethod adtcore:name="TEST_GUT"/>
                  <testMethod adtcore:name="TEST_SCHLECHT">
                    <alerts>
                      <alert kind="failedAssertion" severity="critical" title="Assertion fehlgeschlagen">
                        <details><detail text="Erwartet 1, war 2"/></details>
                      </alert>
                    </alerts>
                  </testMethod>
                </testMethods>
              </testClass>
            </testClasses>
          </program>
        </aunit:runResult>"#;
        let klassen = testklassen(xml).unwrap();
        assert_eq!(klassen.len(), 1);
        assert_eq!(klassen[0].name, "LTCL_DEMO");
        assert_eq!(klassen[0].risiko, "harmless");
        assert_eq!(klassen[0].methoden.len(), 2);
        assert_eq!(klassen[0].methoden[0].ergebnis, "bestanden");
        assert_eq!(klassen[0].methoden[1].ergebnis, "fehlgeschlagen");
        assert_eq!(klassen[0].methoden[1].meldungen.len(), 2);
    }

    #[test]
    fn atc_liest_lauf_und_befunde() {
        let lauf = r#"<worklistRun><worklistId>ABC123</worklistId>
          <worklistTimestamp>2026-01-02T03:04:05Z</worklistTimestamp></worklistRun>"#;
        assert_eq!(atc_lauf(lauf).unwrap(), "ABC123");

        let liste = r#"<worklist timestamp="2026-01-02T03:04:05Z">
          <objects>
            <object name="ZCL_DEMO" type="CLAS/OC" uri="/sap/bc/adt/oo/classes/zcl_demo">
              <findings>
                <finding priority="1" checkId="CL_CI_TEST" checkTitle="Sicherheit"
                  messageId="001" messageTitle="Fehlender Autoritätscheck"
                  location="/sap/bc/adt/oo/classes/zcl_demo/includes/implementations#start=42,3"/>
              </findings>
            </object>
          </objects>
        </worklist>"#;
        let befunde = atc_befunde(liste).unwrap();
        assert_eq!(befunde.len(), 1);
        assert_eq!(befunde[0].objekt, "ZCL_DEMO");
        assert_eq!(befunde[0].prio, 1);
        assert_eq!(befunde[0].pruefung, "Sicherheit");
        assert_eq!(befunde[0].nachricht, "Fehlender Autoritätscheck");
        assert_eq!(befunde[0].zeile, Some(42));
    }

    #[test]
    fn dumps_lesen_id_art_text_und_autor() {
        let xml = r#"<feed xmlns="http://www.w3.org/2005/Atom">
          <entry>
            <id>20260101-120000-1</id>
            <author><name>DEVELOPER</name></author>
            <summary type="text">MESSAGE_TYPE_UNKNOWN</summary>
          </entry>
        </feed>"#;
        let dumps = dumps(xml).unwrap();
        assert_eq!(dumps.len(), 1);
        assert_eq!(dumps[0].id, "20260101-120000-1");
        assert_eq!(dumps[0].art, "text");
        assert_eq!(dumps[0].text, "MESSAGE_TYPE_UNKNOWN");
        assert_eq!(dumps[0].autor, "DEVELOPER");
    }

    #[test]
    fn traces_lesen_objekt_zustand_und_laufzeit() {
        let xml = r#"<feed xmlns="http://www.w3.org/2005/Atom">
          <entry>
            <id>/sap/bc/adt/runtime/traces/abaptraces/20260101/0001</id>
            <title>Trace 1</title>
            <extendedData>
              <objectName>ZREPORT</objectName>
              <state value="A" text="Aktiv"/>
              <runtime>12.5</runtime>
            </extendedData>
          </entry>
        </feed>"#;
        let traces = aufzeichnungen(xml).unwrap();
        assert_eq!(traces.len(), 1);
        assert_eq!(traces[0].objekt, "ZREPORT");
        assert_eq!(traces[0].zustand, "Aktiv");
        assert_eq!(traces[0].laufzeit, 12.5);
        assert_eq!(traces[0].titel, "Trace 1");
    }

    #[test]
    fn trefferliste_und_aussagen_lesen_zeiten() {
        let hitlist = r#"<hitlist>
          <entry description="LOOP" hitCount="7">
            <grossTime time="42.5" percentage="61.2"/>
          </entry>
        </hitlist>"#;
        let hotspots = hotspots(hitlist).unwrap();
        assert_eq!(hotspots.len(), 1);
        assert_eq!(hotspots[0].beschreibung, "LOOP");
        assert_eq!(hotspots[0].aufrufe, 7);
        assert_eq!(hotspots[0].brutto, 42.5);
        assert_eq!(hotspots[0].anteil, 61.2);

        let statements = r#"<statements count="2">
          <statement description="CALL METHOD" hitCount="3">
            <grossTime time="1.5"/>
          </statement>
        </statements>"#;
        let aussagen = aussagen(statements).unwrap();
        assert_eq!(aussagen.len(), 1);
        assert_eq!(aussagen[0].beschreibung, "CALL METHOD");
        assert_eq!(aussagen[0].aufrufe, 3);
        assert_eq!(aussagen[0].brutto, 1.5);
    }

    #[test]
    fn transporte_lesen_ziel_und_auftrag() {
        let xml = r#"<tm:root xmlns:tm="t">
          <tm:workbench>
            <tm:target tm:name="A4H" tm:desc="Entwicklung">
              <tm:modifiable>
                <tm:request tm:number="DEVK900123" tm:status="D" tm:desc="Demo" tm:owner="DEVELOPER"/>
              </tm:modifiable>
              <tm:released>
                <tm:request tm:number="DEVK900124" tm:status="R" tm:desc="Fertig" tm:owner="DEVELOPER"/>
              </tm:released>
            </tm:target>
          </tm:workbench>
        </tm:root>"#;
        let transporte = transporte(xml).unwrap();
        assert_eq!(transporte.len(), 2);
        assert_eq!(transporte[0].art, "Workbench");
        assert_eq!(transporte[0].ziel, "A4H (Entwicklung)");
        assert_eq!(transporte[0].nummer, "DEVK900123");
        assert_eq!(transporte[0].status, "D");
        assert_eq!(transporte[1].nummer, "DEVK900124");
    }

    #[test]
    fn ein_transport_kennt_aufgaben_und_objekte() {
        let xml = r#"<tm:root xmlns:tm="t">
          <tm:request tm:number="DEVK900123" tm:owner="DEVELOPER" tm:desc="Demo" tm:status="D">
            <tm:abap_object tm:name="ZCL_DEMO" tm:type="CLAS/OC"/>
            <tm:task tm:number="DEVK900123" tm:desc="Aufgabe 1">
              <tm:abap_object tm:name="ZCL_DEMO" tm:type="CLAS/OC"/>
            </tm:task>
          </tm:request>
        </tm:root>"#;
        let detail = transportdetails(xml).unwrap();
        assert_eq!(detail.nummer, "DEVK900123");
        assert_eq!(detail.inhaber, "DEVELOPER");
        assert_eq!(detail.objekte.len(), 1);
        assert_eq!(detail.aufgaben.len(), 1);
        assert_eq!(detail.aufgaben[0].nummer, "DEVK900123");
        assert_eq!(detail.aufgaben[0].objekte[0].name, "ZCL_DEMO");
    }

    #[test]
    fn textelemente_lesen_max_laenge_und_texte() {
        let text = "@MaxLength:30\n001=Anzahl\n002=Summe\n\n";
        let elemente = textelemente(text);
        assert_eq!(elemente.len(), 2);
        assert_eq!(elemente[0].id, "001");
        assert_eq!(elemente[0].text, "Anzahl");
        assert_eq!(elemente[0].max_laenge, Some(30));
        assert_eq!(elemente[1].max_laenge, None);
    }

    #[test]
    fn die_textelement_adresse_folgt_dem_objekttyp() {
        assert_eq!(
            textelemente_url("CLAS/OC", "ZCL_DEMO"),
            "/sap/bc/adt/textelements/classes/zcl_demo"
        );
        assert_eq!(
            textelemente_url("FUGR/F", "ZFG"),
            "/sap/bc/adt/textelements/functiongroups/zfg"
        );
        assert_eq!(
            textelemente_url("PROG/P", "ZREPORT"),
            "/sap/bc/adt/textelements/programs/zreport"
        );
        assert_eq!(
            textelemente_url("PROG/P", "/NAMESPACE/REPORT"),
            "/sap/bc/adt/textelements/programs/%2Fnamespace%2Freport"
        );
    }

    #[test]
    fn discovery_liest_die_titel() {
        let xml = r#"<app:service xmlns:app="a" xmlns:atom="b">
          <app:workspace><atom:title>Objekte</atom:title></app:workspace>
          <app:workspace><atom:title>Daten</atom:title></app:workspace>
        </app:service>"#;
        assert_eq!(discovery_titel(xml).unwrap(), vec!["Objekte", "Daten"]);
    }

    #[test]
    fn eine_leere_struktur_ist_ein_fehler() {
        assert!(matches!(
            objekt("<x/>", "/sap/bc/adt/x"),
            Err(AdtFehler::Antwort { .. })
        ));
        assert!(matches!(
            transportdetails("<tm:root xmlns:tm=\"t\"/>"),
            Err(AdtFehler::Antwort { .. })
        ));
    }

    #[test]
    fn eine_sperre_traegt_handle_auftrag_und_ort() {
        let lokal = r#"<?xml version="1.0" encoding="utf-8"?>
        <asx:abap xmlns:asx="http://www.sap.com/abapxml" version="1.0">
          <asx:values>
            <DATA>
              <LOCK_HANDLE>LH4711</LOCK_HANDLE>
              <IS_LOCAL>X</IS_LOCAL>
            </DATA>
          </asx:values>
        </asx:abap>"#;
        let eigene = sperre(lokal).unwrap();
        assert_eq!(eigene.handle, "LH4711");
        assert!(eigene.lokal);
        assert_eq!(eigene.auftrag, "");

        // Ein Objekt mit Transport nennt den Auftrag statt der Ortsmarke.
        let mit_auftrag = r#"<asx:abap xmlns:asx="a"><asx:values><DATA>
          <LOCK_HANDLE>LH0815</LOCK_HANDLE><CORRNR>DEVK900123</CORRNR>
        </DATA></asx:values></asx:abap>"#;
        let mit_transport = sperre(mit_auftrag).unwrap();
        assert_eq!(mit_transport.handle, "LH0815");
        assert_eq!(mit_transport.auftrag, "DEVK900123");
        assert!(!mit_transport.lokal);

        // Ohne Handle ist die Antwort wertlos.
        let ohne = r#"<asx:abap xmlns:asx="a"><asx:values><DATA/></asx:values></asx:abap>"#;
        assert!(matches!(sperre(ohne), Err(AdtFehler::Antwort { .. })));
        assert!(matches!(
            sperre("<asx:abap/>"),
            Err(AdtFehler::Antwort { .. })
        ));
    }

    #[test]
    fn eine_aktivierung_liest_meldungen_und_inaktive_objekte() {
        let mit_fehler = r#"<?xml version="1.0" encoding="utf-8"?>
        <chkl:checkMessages xmlns:chkl="http://www.sap.com/adt/checkrun">
          <chkl:messages>
            <chkl:msg type="W" line="1" id="W01">
              <chkl:shortText><chkl:txt>Der Parameter wird nie gelesen.</chkl:txt></chkl:shortText>
            </chkl:msg>
            <chkl:msg type="E" line="3" id="E01">
              <chkl:shortText><chkl:txt>WRITE ist hier nicht erlaubt.</chkl:txt></chkl:shortText>
            </chkl:msg>
          </chkl:messages>
        </chkl:checkMessages>"#;
        let ergebnis = aktivierung(mit_fehler).unwrap();
        assert!(!ergebnis.erfolg);
        assert_eq!(ergebnis.meldungen.len(), 2);
        assert_eq!(ergebnis.meldungen[1].art, "E");
        assert_eq!(ergebnis.meldungen[1].zeile, Some(3));
        assert_eq!(ergebnis.meldungen[1].text, "WRITE ist hier nicht erlaubt.");
        assert_eq!(ergebnis.inaktiv, 0);

        // Eine Warnung allein ist Erfolg; ein leeres Dokument auch.
        let nur_warnung = mit_fehler.replace("type=\"E\"", "type=\"W\"");
        assert!(aktivierung(&nur_warnung).unwrap().erfolg);
        assert!(aktivierung("").unwrap().erfolg);

        // Bleibt ein Objekt inaktiv, gilt die Aktivierung als nicht erfolgreich.
        let inaktiv = r#"<ioc:inactiveObjects xmlns:ioc="i">
          <ioc:entry><ioc:object ioc:name="ZCL_DEMO"/></ioc:entry>
        </ioc:inactiveObjects>"#;
        let ergebnis = aktivierung(inaktiv).unwrap();
        assert!(!ergebnis.erfolg);
        assert_eq!(ergebnis.inaktiv, 1);
    }
}
