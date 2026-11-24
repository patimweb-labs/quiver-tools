// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Regeln und Rümpfe der schreibenden Werkzeuge (Aufgabe 08).
//!
//! Dieses Modul ist **rein**: kein Netz, kein Zustand. Es hält die drei
//! Schutzregeln fest, die **vor** jeder Anfrage an SAP entschieden sein müssen,
//! und baut die Rümpfe der schreibenden Aufrufe, abgelesen bei `abap-adt-api`
//! (`api/objectcreator.js`, `api/textelements.js`, `api/activate.js`).
//!
//! | Regel | Funktion |
//! |---|---|
//! | nur `Z*`, `Y*` und `/…/…` | [`im_kundennamensraum`], [`paket_erlaubt`] |
//! | `source` bis 2 MB | [`MAX_QUELLE`] |
//! | nur anlegbare Objektarten | [`anlegbar`] |
//!
//! Aus einem Namen allein lässt sich die Regel noch nicht prüfen – aber aus dem
//! Objektnamen, und den liefert auch eine Objekt-URL ([`name_aus_url`]).

use crate::adt::xml::attribut_wert;

/// So groß darf ein `source` höchstens sein (2 MB).
pub const MAX_QUELLE: usize = 2 * 1024 * 1024;

/// Der Namensraum, in dem ein Objekt liegt, das wir anfassen dürfen.
///
/// Erlaubt sind Kundenobjekte (`Z*`, `Y*`) und Objekte in einem eigenen
/// Namensraum (`/NAMESPACE/NAME`). Alles andere – SAP-Standard, fremde
/// Namensräume – wird ohne Anfrage abgelehnt.
pub fn im_kundennamensraum(name: &str) -> bool {
    let name = name.trim();
    let mut zeichen = name.chars();
    match zeichen.next() {
        Some('Z') | Some('z') | Some('Y') | Some('y') => name.chars().count() > 1,
        Some('/') => {
            let rest = name.trim_start_matches('/');
            match rest.split_once('/') {
                Some((raum, objekt)) => {
                    !raum.trim().is_empty()
                        && !objekt.trim().is_empty()
                        && !objekt.trim().starts_with('/')
                }
                None => false,
            }
        }
        _ => false,
    }
}

/// Ein Paket, in das wir anlegen dürfen: `$TMP` oder ein Kundenpaket.
pub fn paket_erlaubt(paket: &str) -> bool {
    let paket = paket.trim();
    paket.eq_ignore_ascii_case("$TMP") || im_kundennamensraum(paket)
}

/// Der Objektname aus einer Objekt-URL.
///
/// `/sap/bc/adt/oo/classes/zcl_demo` → `zcl_demo`;
/// `/sap/bc/adt/oo/classes/%2Ffoo%2Fbar` → `/foo/bar`. Damit lässt sich die
/// Namensraum-Regel auch dann prüfen, wenn der Agent nur eine URL schickt.
pub fn name_aus_url(url: &str) -> Option<String> {
    let pfad = url.trim().split(['?', '#']).next().unwrap_or_default();
    let letzte = pfad.trim_end_matches('/').rsplit('/').next()?;
    let name = prozent_dekodieren(letzte);
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// `%2F` → `/`, `%20` → Leerzeichen; alles andere bleibt stehen.
fn prozent_dekodieren(wert: &str) -> String {
    let bytes = wert.as_bytes();
    let mut ergebnis = Vec::with_capacity(bytes.len());
    let mut stelle = 0;
    while stelle < bytes.len() {
        if bytes[stelle] == b'%' && stelle + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[stelle + 1..stelle + 3]).unwrap_or("");
            if let Ok(zeichen) = u8::from_str_radix(hex, 16) {
                ergebnis.push(zeichen);
                stelle += 3;
                continue;
            }
        }
        ergebnis.push(bytes[stelle]);
        stelle += 1;
    }
    String::from_utf8_lossy(&ergebnis).to_string()
}

// ------------------------------------------------------- Objekte anlegen

/// Eine Objektart, die `abap_create_object` anlegen kann.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anlegbar {
    /// Die ADT-Kennung, z. B. `CLAS/OC`.
    pub typ: &'static str,
    /// Der Anlegepfad unter `/sap/bc/adt/` (`api/objectcreator.js`).
    pub pfad: &'static str,
    /// Die XML-Wurzel des Rumpfs.
    pub wurzel: &'static str,
    /// Die Namensraum-Deklaration dazu.
    pub namensraum: &'static str,
}

/// Die anlegbaren Objektarten – genau die der Aufgabe.
pub const ANLEGBAR: [Anlegbar; 9] = [
    Anlegbar {
        typ: "CLAS/OC",
        pfad: "oo/classes",
        wurzel: "class:abapClass",
        namensraum: "xmlns:class=\"http://www.sap.com/adt/oo/classes\"",
    },
    Anlegbar {
        typ: "INTF/OI",
        pfad: "oo/interfaces",
        wurzel: "intf:abapInterface",
        namensraum: "xmlns:intf=\"http://www.sap.com/adt/oo/interfaces\"",
    },
    Anlegbar {
        typ: "PROG/P",
        pfad: "programs/programs",
        wurzel: "program:abapProgram",
        namensraum: "xmlns:program=\"http://www.sap.com/adt/programs/programs\"",
    },
    Anlegbar {
        typ: "PROG/I",
        pfad: "programs/includes",
        wurzel: "include:abapInclude",
        namensraum: "xmlns:include=\"http://www.sap.com/adt/programs/includes\"",
    },
    Anlegbar {
        typ: "FUGR/F",
        pfad: "functions/groups",
        wurzel: "group:abapFunctionGroup",
        namensraum: "xmlns:group=\"http://www.sap.com/adt/functions/groups\"",
    },
    Anlegbar {
        typ: "TABL/DT",
        pfad: "ddic/tables",
        wurzel: "blue:blueSource",
        namensraum: "xmlns:blue=\"http://www.sap.com/wbobj/blue\"",
    },
    Anlegbar {
        typ: "DTEL/DE",
        pfad: "ddic/dataelements",
        wurzel: "blue:wbobj",
        namensraum: "xmlns:blue=\"http://www.sap.com/wbobj/dictionary/dtel\"",
    },
    Anlegbar {
        typ: "DOMA/DD",
        pfad: "ddic/domains",
        wurzel: "domain:domain",
        namensraum: "xmlns:domain=\"http://www.sap.com/dictionary/domain\"",
    },
    Anlegbar {
        typ: "DDLS/DF",
        pfad: "ddic/ddl/sources",
        wurzel: "ddl:ddlSource",
        namensraum: "xmlns:ddl=\"http://www.sap.com/adt/ddic/ddlsources\"",
    },
];

/// Die Objektart zu einer Kennung (`CLAS/OC`, auch `clas/oc`).
pub fn anlegbar(typ: &str) -> Option<&'static Anlegbar> {
    let typ = typ.trim();
    ANLEGBAR
        .iter()
        .find(|art| art.typ.eq_ignore_ascii_case(typ))
}

/// Die Kennungen, die [`anlegbar`] kennt – für Fehlermeldungen.
pub fn anlegbare_arten() -> String {
    ANLEGBAR
        .iter()
        .map(|art| art.typ)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Der Anlegepfad einer Objektart.
pub fn anlege_pfad(art: &Anlegbar) -> String {
    format!("/sap/bc/adt/{}", art.pfad)
}

/// Die Adresse des angelegten Objekts, wenn der Server keinen `Location`-Kopf
/// schickt (`objectPath` in `api/objectcreator.js`).
pub fn neue_objekt_url(art: &Anlegbar, name: &str) -> String {
    let klein = name.trim().to_lowercase();
    let kodiert = if klein.contains('/') {
        klein.replace('/', "%2F")
    } else {
        klein
    };
    format!("{}/{}", anlege_pfad(art), kodiert)
}

/// Der Rumpf für `abap_create_object` (`createBodySimple`).
pub fn anlege_rumpf(
    art: &Anlegbar,
    name: &str,
    paket: &str,
    beschreibung: &str,
    sprache: &str,
    verantwortlich: &str,
) -> String {
    let sprache = if sprache.trim().is_empty() {
        "EN"
    } else {
        sprache.trim()
    };
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<{wurzel} {namensraum}
  xmlns:adtcore=\"http://www.sap.com/adt/core\"
  adtcore:description=\"{beschreibung}\"
  adtcore:name=\"{name}\" adtcore:type=\"{typ}\"
  adtcore:language=\"{sprache}\" adtcore:masterLanguage=\"{sprache}\"
  adtcore:responsible=\"{verantwortlich}\">
  <adtcore:packageRef adtcore:name=\"{paket}\"/>
</{wurzel}>",
        wurzel = art.wurzel,
        namensraum = art.namensraum,
        typ = art.typ,
        beschreibung = attribut_wert(beschreibung),
        name = attribut_wert(name),
        paket = attribut_wert(paket),
        verantwortlich = attribut_wert(verantwortlich),
    )
}

// ------------------------------------------------------------ Aktivieren

/// Der Rumpf für die Aktivierung (`api/activate.js`).
///
/// `context` ist der Hauptinclude; SAP aktiviert damit genau das Objekt, dessen
/// Quelltext geschrieben wurde.
pub fn aktivierung_rumpf(objekt_url: &str, name: &str, hauptinclude: &str) -> String {
    let mut uri = attribut_wert(objekt_url);
    if !hauptinclude.trim().is_empty() {
        uri.push_str("?context=");
        uri.push_str(&attribut_wert(hauptinclude.trim()));
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
         <adtcore:objectReferences xmlns:adtcore=\"http://www.sap.com/adt/core\">\
         <adtcore:objectReference adtcore:uri=\"{uri}\" adtcore:name=\"{name}\"/>\
         </adtcore:objectReferences>",
        name = attribut_wert(name)
    )
}

// ----------------------------------------------------------- Textelemente

/// Die drei Arten von Textelementen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kategorie {
    Symbole,
    Auswahl,
    Ueberschriften,
}

impl Kategorie {
    /// Aus dem Parameternamen (`symbols`, `selections`, `headings`).
    pub fn aus_text(wert: &str) -> Option<Kategorie> {
        match wert.trim().to_ascii_lowercase().as_str() {
            "symbols" => Some(Kategorie::Symbole),
            "selections" => Some(Kategorie::Auswahl),
            "headings" => Some(Kategorie::Ueberschriften),
            _ => None,
        }
    }

    /// Der Name, den ADT im Medientyp führt.
    pub fn name(&self) -> &'static str {
        match self {
            Kategorie::Symbole => "symbols",
            Kategorie::Auswahl => "selections",
            Kategorie::Ueberschriften => "headings",
        }
    }

    /// Der `Content-Type`, den ADT für diese Art erwartet.
    pub fn medientyp(&self) -> String {
        format!("application/vnd.sap.adt.textelements.{}.v1", self.name())
    }
}

/// Ein Textelement, wie der Agent es übergibt.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Element {
    /// Der Schlüssel, z. B. `001` oder `LISTHEADER`.
    pub id: String,
    pub text: String,
    /// Höchstlänge (nur Textsymbole).
    pub max_laenge: Option<u32>,
}

/// Die Überschriften, die SAP kennt.
const UEBERSCHRIFTEN: [&str; 5] = [
    "LISTHEADER",
    "COLUMNHEADER_1",
    "COLUMNHEADER_2",
    "COLUMNHEADER_3",
    "COLUMNHEADER_4",
];

/// Der Rumpf für `abap_set_text_elements` (`formatTextElements`).
///
/// Format: `@MaxLength:30` (nur Symbole) bzw. `@DDICReference:…` (nur
/// Selectionstexte) vor der Zeile, dann `ID=Text`, danach – außer bei
/// Überschriften – eine Leerzeile. Geprüft wird vorher, damit kein Aufruf
/// hinausgeht, den SAP ohnehin ablehnen würde.
pub fn textelemente_rumpf(elemente: &[Element], kategorie: Kategorie) -> Result<String, String> {
    pruefen(elemente, kategorie)?;
    let mut zeilen = Vec::new();
    for element in elemente {
        if kategorie == Kategorie::Symbole {
            if let Some(max) = element.max_laenge.filter(|max| *max > 0) {
                zeilen.push(format!("@MaxLength:{max}"));
            }
        }
        zeilen.push(format!(
            "{}={}",
            element.id.trim().to_uppercase(),
            element.text
        ));
        if kategorie != Kategorie::Ueberschriften {
            zeilen.push(String::new());
        }
    }
    Ok(zeilen.join("\n"))
}

/// Prüft die Elemente gegen die Regeln der Vorlage.
fn pruefen(elemente: &[Element], kategorie: Kategorie) -> Result<(), String> {
    if elemente.is_empty() {
        return Err("Es wurde kein Textelement übergeben.".to_string());
    }
    for element in elemente {
        let id = element.id.trim().to_uppercase();
        if id.is_empty() {
            return Err("Ein Textelement hat keinen Schlüssel (`id`).".to_string());
        }
        match kategorie {
            Kategorie::Symbole => {
                if id.chars().count() != 3 {
                    return Err(format!(
                        "Der Symbolschlüssel „{id}“ muss genau drei Zeichen haben."
                    ));
                }
                if id.chars().any(char::is_whitespace) {
                    return Err(format!(
                        "Der Symbolschlüssel „{id}“ darf keine Leerzeichen enthalten."
                    ));
                }
                if let Some(max) = element.max_laenge.filter(|max| *max > 0) {
                    if element.text.chars().count() > max as usize {
                        return Err(format!(
                            "Der Text zu „{id}“ ist länger als maxLength {max}."
                        ));
                    }
                }
            }
            Kategorie::Auswahl => {
                if element.text.chars().count() > 30 {
                    return Err(format!(
                        "Der Selectionstext zu „{id}“ ist länger als 30 Zeichen."
                    ));
                }
            }
            Kategorie::Ueberschriften => {
                if !UEBERSCHRIFTEN.contains(&id.as_str()) {
                    return Err(format!(
                        "Die Überschrift „{id}“ gibt es nicht; erlaubt sind {}.",
                        UEBERSCHRIFTEN.join(", ")
                    ));
                }
                let grenze = if id == "LISTHEADER" { 71 } else { 255 };
                if element.text.chars().count() > grenze {
                    return Err(format!(
                        "Die Überschrift „{id}“ ist länger als {grenze} Zeichen."
                    ));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nur_kundenobjekte_und_namensraeume_gehen() {
        for name in ["ZCL_DEMO", "zcl_demo", "Y_RUN", "Z1", "/FOO/BAR"] {
            assert!(im_kundennamensraum(name), "{name} sollte erlaubt sein");
        }
        for name in [
            "",
            "Z",
            "CL_ABAP_TYPEDESCR",
            "cl_abap_typedescr",
            "MARA",
            "SAPBC",
            "/",
            "//BAR",
            "/FOO/",
            "/FOO",
        ] {
            assert!(!im_kundennamensraum(name), "{name} sollte abgelehnt werden");
        }
    }

    #[test]
    fn tmp_ist_als_paket_erlaubt_sonst_gilt_die_namensraum_regel() {
        assert!(paket_erlaubt("$TMP"));
        assert!(paket_erlaubt(" $tmp "));
        assert!(paket_erlaubt("Z_PAKET"));
        assert!(paket_erlaubt("/FOO/PAKET"));
        assert!(!paket_erlaubt("SAPBC"));
        assert!(!paket_erlaubt(""));
    }

    #[test]
    fn der_name_kommt_auch_aus_der_objekt_url() {
        assert_eq!(
            name_aus_url("/sap/bc/adt/oo/classes/zcl_demo").as_deref(),
            Some("zcl_demo")
        );
        assert_eq!(
            name_aus_url("/sap/bc/adt/oo/classes/zcl_demo?version=active").as_deref(),
            Some("zcl_demo")
        );
        assert_eq!(
            name_aus_url("/sap/bc/adt/oo/classes/zcl_demo/").as_deref(),
            Some("zcl_demo")
        );
        // Namensraumobjekte sind in der Adresse kodiert.
        assert_eq!(
            name_aus_url("/sap/bc/adt/oo/classes/%2Ffoo%2Fbar").as_deref(),
            Some("/foo/bar")
        );
        assert_eq!(name_aus_url(""), None);
        assert_eq!(name_aus_url("/"), None);
        assert_eq!(prozent_dekodieren("a%20b"), "a b");
        assert_eq!(prozent_dekodieren("100%"), "100%");
    }

    #[test]
    fn nur_die_arten_der_aufgabe_lassen_sich_anlegen() {
        for typ in [
            "CLAS/OC", "INTF/OI", "PROG/P", "PROG/I", "FUGR/F", "TABL/DT", "DTEL/DE", "DOMA/DD",
            "DDLS/DF",
        ] {
            assert!(anlegbar(typ).is_some(), "{typ} fehlt");
        }
        assert_eq!(anlegbar("clas/oc").unwrap().typ, "CLAS/OC");
        assert!(anlegbar("DEVC/K").is_none());
        assert!(anlegbar("").is_none());
        assert!(anlegbare_arten().contains("DDLS/DF"));
    }

    #[test]
    fn der_anlegepfad_und_die_adresse_folgen_der_vorlage() {
        let klasse = anlegbar("CLAS/OC").unwrap();
        assert_eq!(anlege_pfad(klasse), "/sap/bc/adt/oo/classes");
        assert_eq!(
            neue_objekt_url(klasse, "ZCL_NEU"),
            "/sap/bc/adt/oo/classes/zcl_neu"
        );
        let programm = anlegbar("PROG/P").unwrap();
        assert_eq!(anlege_pfad(programm), "/sap/bc/adt/programs/programs");
        let cds = anlegbar("DDLS/DF").unwrap();
        assert_eq!(anlege_pfad(cds), "/sap/bc/adt/ddic/ddl/sources");
    }

    #[test]
    fn der_anlege_rumpf_hat_wurzel_namensraum_und_paket() {
        let klasse = anlegbar("CLAS/OC").unwrap();
        let rumpf = anlege_rumpf(
            klasse,
            "ZCL_NEU",
            "Z_PAKET",
            "Neue & schöne Klasse",
            "DE",
            "DEVELOPER",
        );
        assert!(rumpf.contains("<class:abapClass"), "{rumpf}");
        assert!(rumpf.contains("xmlns:class=\"http://www.sap.com/adt/oo/classes\""));
        assert!(rumpf.contains("adtcore:name=\"ZCL_NEU\""));
        assert!(rumpf.contains("adtcore:type=\"CLAS/OC\""));
        assert!(rumpf.contains("adtcore:description=\"Neue &amp; schöne Klasse\""));
        assert!(rumpf.contains("adtcore:language=\"DE\""));
        assert!(rumpf.contains("adtcore:responsible=\"DEVELOPER\""));
        assert!(rumpf.contains("<adtcore:packageRef adtcore:name=\"Z_PAKET\"/>"));
        assert!(rumpf.ends_with("</class:abapClass>"));

        // Ohne Sprache gilt EN.
        let programm = anlegbar("PROG/P").unwrap();
        let rumpf = anlege_rumpf(programm, "ZREPORT", "$TMP", "Report", "", "DEVELOPER");
        assert!(rumpf.contains("adtcore:language=\"EN\""));
        assert!(rumpf.contains("<program:abapProgram"));
    }

    #[test]
    fn der_aktivierungs_rumpf_nennt_objekt_und_hauptinclude() {
        let rumpf = aktivierung_rumpf(
            "/sap/bc/adt/oo/classes/zcl_demo",
            "ZCL_DEMO",
            "/sap/bc/adt/oo/classes/zcl_demo/includes/implementations",
        );
        assert!(
            rumpf.contains(
                "adtcore:uri=\"/sap/bc/adt/oo/classes/zcl_demo?context=\
                 /sap/bc/adt/oo/classes/zcl_demo/includes/implementations\""
            ),
            "{rumpf}"
        );
        assert!(rumpf.contains("adtcore:name=\"ZCL_DEMO\""));
        assert!(rumpf.contains("<adtcore:objectReferences"));
    }

    #[test]
    fn textelemente_haben_das_format_der_vorlage() {
        let symbole = vec![
            Element {
                id: "001".into(),
                text: "Anzahl".into(),
                max_laenge: Some(30),
            },
            Element {
                id: "002".into(),
                text: "Summe".into(),
                max_laenge: None,
            },
        ];
        let rumpf = textelemente_rumpf(&symbole, Kategorie::Symbole).unwrap();
        assert_eq!(rumpf, "@MaxLength:30\n001=Anzahl\n\n002=Summe\n");

        // Überschriften stehen ohne Leerzeile untereinander.
        let ueberschriften = vec![Element {
            id: "listheader".into(),
            text: "Liste".into(),
            max_laenge: None,
        }];
        assert_eq!(
            textelemente_rumpf(&ueberschriften, Kategorie::Ueberschriften).unwrap(),
            "LISTHEADER=Liste"
        );

        let auswahl = vec![Element {
            id: "p_carrid".into(),
            text: "Airline".into(),
            max_laenge: None,
        }];
        assert_eq!(
            textelemente_rumpf(&auswahl, Kategorie::Auswahl).unwrap(),
            "P_CARRID=Airline\n"
        );
    }

    #[test]
    fn falsche_textelemente_werden_vor_der_anfrage_abgelehnt() {
        let element = |id: &str, text: &str, max: Option<u32>| Element {
            id: id.into(),
            text: text.into(),
            max_laenge: max,
        };
        // Symbolschlüssel: genau drei Zeichen, keine Leerzeichen.
        assert!(textelemente_rumpf(&[element("01", "x", None)], Kategorie::Symbole).is_err());
        assert!(textelemente_rumpf(&[element("0 1", "x", None)], Kategorie::Symbole).is_err());
        // maxLength gilt.
        assert!(
            textelemente_rumpf(&[element("001", "zu lang", Some(3))], Kategorie::Symbole).is_err()
        );
        // Selectionstexte höchstens 30 Zeichen.
        assert!(
            textelemente_rumpf(&[element("P_X", &"x".repeat(31), None)], Kategorie::Auswahl)
                .is_err()
        );
        // Nur bekannte Überschriften, LISTHEADER höchstens 71 Zeichen.
        assert!(
            textelemente_rumpf(&[element("TITEL", "x", None)], Kategorie::Ueberschriften).is_err()
        );
        assert!(textelemente_rumpf(
            &[element("LISTHEADER", &"x".repeat(72), None)],
            Kategorie::Ueberschriften
        )
        .is_err());
        // Nichts übergeben ist auch ein Fehler.
        assert!(textelemente_rumpf(&[], Kategorie::Symbole).is_err());
        assert!(textelemente_rumpf(&[element("  ", "x", None)], Kategorie::Symbole).is_err());
    }

    #[test]
    fn die_kategorie_kommt_aus_dem_parameternamen() {
        assert_eq!(Kategorie::aus_text("symbols"), Some(Kategorie::Symbole));
        assert_eq!(
            Kategorie::aus_text(" selections "),
            Some(Kategorie::Auswahl)
        );
        assert_eq!(
            Kategorie::aus_text("HEADINGS"),
            Some(Kategorie::Ueberschriften)
        );
        assert_eq!(Kategorie::aus_text("quatsch"), None);
        assert_eq!(Kategorie::Symbole.name(), "symbols");
        assert_eq!(
            Kategorie::Auswahl.medientyp(),
            "application/vnd.sap.adt.textelements.selections.v1"
        );
    }
}
