// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der Bibliothekseintrag für mcp-quiver (SPEC §2).
//!
//! `quiver-<tool> eintrag` gibt ihn als JSON aus. quiver liest daraus, wie das
//! Tool gestartet wird und welche Felder der User im quiver-Fenster füllt; die
//! Umgebung der Laufzeit verweist mit `${feld}` auf diese Felder. Es gibt kein
//! `einrichtung`-Feld mehr: eine Seite im Browser öffnet das Tool nicht.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::umgebung::schluessel;
use crate::{FeldArt, Werkzeug};

/// Wer ein Feld füllt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Quelle {
    /// Der User am Gerät, offen in der Client-Konfiguration.
    Client,
    /// Der User am Gerät, in der Tresor-Datei; verlässt das Gerät nie.
    Geheim,
}

/// Woher ein Eintrag kommt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Herkunft {
    /// Von uns gebaut.
    Eigen,
}

/// Wie ein Eintrag läuft.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "art", rename_all = "kebab-case")]
pub enum Laufzeit {
    /// Ein Prozess, MCP über stdin/stdout.
    Stdio {
        befehl: String,
        /// `<NAME>_<FELD>` → `${feld}`; quiver setzt daraus die Umgebung.
        umgebung: BTreeMap<String, String>,
    },
}

/// Ein Feld, das der User im quiver-Fenster füllt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Feld {
    /// Feldname im Tool, z. B. `api_key`.
    pub name: String,
    /// Anzeige statt des Feldnamens, z. B. „Mandant“.
    pub titel: String,
    /// Nur die Hilfe; der Titel steht eigens in `titel`.
    pub beschreibung: String,
    /// Wie quiver das Feld darstellt (Checkbox, Zahl, Auswahl …).
    pub art: Art,
    pub quelle: Quelle,
    pub pflicht: bool,
    pub standard: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<FeldLink>,
}

/// Die Art eines Feldes, im JSON-Format von `quiver_domain::FeldArt`: snake_case,
/// eine Auswahl als `{ "auswahl": { "optionen": [ { "wert", "anzeige" } ] } }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Art {
    Text,
    Passwort,
    Zahl,
    Schalter,
    Url,
    Ordner,
    Auswahl { optionen: Vec<Wahl> },
}

/// Eine Möglichkeit einer Auswahl: gespeicherter Wert und Anzeige.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Wahl {
    pub wert: String,
    pub anzeige: String,
}

/// Hilfe-Link neben einem Feld, im Format von `quiver_domain::FeldLink`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FeldLink {
    pub text: String,
    pub url: String,
}

/// Ein MCP-Tool in der Bibliothek, im Format von `quiver_domain::Eintrag`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Eintrag {
    pub id: String,
    pub version: String,
    pub titel: String,
    pub beschreibung: String,
    pub herkunft: Herkunft,
    pub laufzeit: Laufzeit,
    pub felder: Vec<Feld>,
    /// Mehrere gleichartige Zugänge (`Werkzeug::mehrfach`), im Format von
    /// `quiver_domain::Mehrfach`. Dann ist `felder` leer und `laufzeit` hat
    /// keine `umgebung`: die Werte kommen als JSON in `QUIVER_ZUGAENGE`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mehrfach: Option<Mehrfach>,
}

/// Eine Liste benannter Zugänge, die der User selbst pflegt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Mehrfach {
    /// Überschrift der Liste, z. B. „Verbindungen“.
    pub titel: String,
    /// Bezeichnung eines Elements, z. B. „Verbindung“.
    pub einzahl: String,
    pub felder: Vec<Feld>,
}

/// Der Eintrag eines Werkzeugs.
///
/// `befehl` ist der Pfad des Programms; ohne Angabe der Pfad des laufenden
/// Programms, sonst `quiver-<name>` (quiver sucht es dann im Pfad).
pub fn eintrag<W: Werkzeug>(werkzeug: &W, befehl: Option<String>) -> Eintrag {
    let name = werkzeug.name();

    // Entweder ein Zugang mit Feldern in der Umgebung – oder eine Liste von
    // Zugängen, deren Felder unter `mehrfach` stehen (SPEC §2, Aufgabe 05).
    let (felder, umgebung, mehrfach) = match werkzeug.mehrfach() {
        Some((titel, einzahl)) => (
            Vec::new(),
            BTreeMap::new(),
            Some(Mehrfach {
                titel: titel.to_string(),
                einzahl: einzahl.to_string(),
                felder: werkzeug.felder().into_iter().map(als_feld).collect(),
            }),
        ),
        None => {
            let mut umgebung = BTreeMap::new();
            let mut felder = Vec::new();
            for feld in werkzeug.felder() {
                umgebung.insert(schluessel(name, feld.name), format!("${{{}}}", feld.name));
                felder.push(als_feld(feld));
            }
            (felder, umgebung, None)
        }
    };

    Eintrag {
        id: name.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        titel: werkzeug.titel().to_string(),
        beschreibung: werkzeug.beschreibung().to_string(),
        herkunft: Herkunft::Eigen,
        laufzeit: Laufzeit::Stdio {
            befehl: befehl.unwrap_or_else(|| programm(name)),
            umgebung,
        },
        felder,
        mehrfach,
    }
}

/// Ein Feld des Werkzeugs als Feld des Bibliothekseintrags.
fn als_feld(feld: crate::Feld) -> Feld {
    // Geheim bleibt geheim, egal welche Art das Feld hat.
    let quelle = if feld.art == FeldArt::Passwort {
        Quelle::Geheim
    } else {
        Quelle::Client
    };
    Feld {
        name: feld.name.to_string(),
        titel: feld.titel.to_string(),
        beschreibung: feld.hilfe.to_string(),
        art: als_art(feld.art),
        quelle,
        pflicht: feld.pflicht,
        standard: feld.standard.map(str::to_string),
        link: feld.link.map(|(text, url)| FeldLink {
            text: text.to_string(),
            url: url.to_string(),
        }),
    }
}

/// Die Feldart des Werkzeugs als Feldart des Bibliothekseintrags.
fn als_art(art: FeldArt) -> Art {
    match art {
        FeldArt::Text => Art::Text,
        FeldArt::Passwort => Art::Passwort,
        FeldArt::Zahl => Art::Zahl,
        FeldArt::Schalter => Art::Schalter,
        FeldArt::Url => Art::Url,
        FeldArt::Ordner => Art::Ordner,
        FeldArt::Auswahl(optionen) => Art::Auswahl {
            optionen: optionen
                .into_iter()
                .map(|(wert, anzeige)| Wahl { wert, anzeige })
                .collect(),
        },
    }
}

/// Der Name, unter dem quiver das Programm auf **jedem** Gerät findet. Nie der
/// Pfad dieses Rechners: ein Eintrag geht an andere Geräte.
fn programm(name: &str) -> String {
    format!("quiver-{name}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Feld as KernFeld, Profil};
    use serde_json::json;

    struct Attrappe;

    impl Werkzeug for Attrappe {
        fn name(&self) -> &'static str {
            "attrappe"
        }

        fn titel(&self) -> &'static str {
            "Attrappen-Tool"
        }

        fn beschreibung(&self) -> &'static str {
            "Attrappe für Tests."
        }

        fn felder(&self) -> Vec<KernFeld> {
            vec![
                KernFeld {
                    name: "url",
                    titel: "Adresse",
                    art: FeldArt::Url,
                    pflicht: true,
                    standard: None,
                    link: None,
                    hilfe: "Mit https://.",
                },
                KernFeld {
                    name: "token",
                    titel: "Token",
                    art: FeldArt::Passwort,
                    pflicht: true,
                    standard: None,
                    hilfe: "Steht in den Einstellungen.",
                    link: None,
                },
                KernFeld {
                    name: "umgebung",
                    titel: "Umgebung",
                    art: FeldArt::Auswahl(vec![("test".to_string(), "Test".to_string())]),
                    pflicht: false,
                    standard: Some("test"),
                    link: None,
                    hilfe: "Nur eine Bezeichnung.",
                },
            ]
        }

        async fn testen(&self, _profil: &Profil) -> anyhow::Result<String> {
            Ok("erreichbar".to_string())
        }
    }

    /// Kopie der betroffenen Typen aus `quiver_domain` (mcp-quiver). Sie prüft,
    /// dass das JSON wirklich deserialisierbar ist – ohne Abhängigkeit auf das
    /// andere Projekt.
    #[allow(dead_code)]
    mod quiver_domain {
        use serde::Deserialize;
        use std::collections::BTreeMap;

        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "kebab-case")]
        pub enum Herkunft {
            Eigen,
            Fremd,
        }

        #[derive(Debug, Deserialize)]
        #[serde(tag = "art", rename_all = "kebab-case")]
        pub enum Laufzeit {
            Stdio {
                befehl: String,
                #[serde(default)]
                argumente: Vec<String>,
                #[serde(default)]
                umgebung: BTreeMap<String, String>,
            },
            Remote {
                url: String,
                #[serde(default)]
                kopfzeilen: BTreeMap<String, String>,
            },
        }

        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "kebab-case")]
        pub enum Quelle {
            Server,
            Client,
            Geheim,
        }

        #[derive(Debug, Default, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum FeldArt {
            #[default]
            Text,
            Passwort,
            Zahl,
            Schalter,
            Url,
            Auswahl {
                optionen: Vec<AuswahlOption>,
            },
        }

        #[derive(Debug, Deserialize)]
        pub struct AuswahlOption {
            pub wert: String,
            pub anzeige: String,
        }

        #[derive(Debug, Deserialize)]
        pub struct Feld {
            pub name: String,
            #[serde(default)]
            pub beschreibung: String,
            #[serde(default)]
            pub art: FeldArt,
            #[serde(default)]
            pub titel: Option<String>,
            pub quelle: Quelle,
            #[serde(default = "wahr")]
            pub pflicht: bool,
            #[serde(default)]
            pub standard: Option<String>,
        }

        fn wahr() -> bool {
            true
        }

        #[derive(Debug, Deserialize)]
        pub struct Eintrag {
            pub id: String,
            pub version: String,
            pub titel: String,
            #[serde(default)]
            pub beschreibung: String,
            pub herkunft: Herkunft,
            pub laufzeit: Laufzeit,
            #[serde(default)]
            pub felder: Vec<Feld>,
            #[serde(default = "wahr")]
            pub aktiv: bool,
            #[serde(default)]
            pub mehrfach: Option<Mehrfach>,
        }

        #[derive(Debug, Deserialize)]
        pub struct Mehrfach {
            pub titel: String,
            pub einzahl: String,
            #[serde(default)]
            pub felder: Vec<Feld>,
        }
    }

    /// Alle Feldarten in einem Werkzeug, damit jede ihr JSON bekommt.
    struct Arten;

    impl Werkzeug for Arten {
        fn name(&self) -> &'static str {
            "arten"
        }

        fn titel(&self) -> &'static str {
            "Arten-Tool"
        }

        fn beschreibung(&self) -> &'static str {
            "Ein Feld je Art."
        }

        fn felder(&self) -> Vec<KernFeld> {
            let feld = |name: &'static str, titel: &'static str, art: FeldArt| KernFeld {
                name,
                titel,
                art,
                pflicht: false,
                standard: None,
                hilfe: "Hilfe",
                link: None,
            };
            vec![
                feld("text", "Text", FeldArt::Text),
                feld("passwort", "Passwort", FeldArt::Passwort),
                feld("zahl", "Zahl", FeldArt::Zahl),
                feld("schalter", "Schalter", FeldArt::Schalter),
                feld("url", "Adresse", FeldArt::Url),
                feld(
                    "auswahl",
                    "Auswahl",
                    FeldArt::Auswahl(vec![
                        ("a".to_string(), "A".to_string()),
                        ("b".to_string(), "B".to_string()),
                    ]),
                ),
            ]
        }

        async fn testen(&self, _profil: &Profil) -> anyhow::Result<String> {
            Ok("erreichbar".to_string())
        }
    }

    /// Ein Werkzeug mit mehreren Zugängen (wie `quiver-ssh`).
    struct MehrfachAttrappe;

    impl Werkzeug for MehrfachAttrappe {
        fn name(&self) -> &'static str {
            "mehrfach"
        }

        fn titel(&self) -> &'static str {
            "Mehrfach-Tool"
        }

        fn beschreibung(&self) -> &'static str {
            "Attrappe mit einer Liste von Zugängen."
        }

        fn felder(&self) -> Vec<KernFeld> {
            vec![
                KernFeld {
                    name: "host",
                    titel: "Hostname",
                    art: FeldArt::Text,
                    pflicht: true,
                    standard: None,
                    hilfe: "Hostname oder IP",
                    link: None,
                },
                KernFeld {
                    name: "passwort",
                    titel: "Passwort",
                    art: FeldArt::Passwort,
                    pflicht: false,
                    standard: None,
                    hilfe: "Nur für die erste Anmeldung.",
                    link: None,
                },
            ]
        }

        fn mehrfach(&self) -> Option<(&'static str, &'static str)> {
            Some(("Verbindungen", "Verbindung"))
        }

        async fn testen(&self, _profil: &Profil) -> anyhow::Result<String> {
            Ok("erreichbar".to_string())
        }
    }

    #[test]
    fn der_eintrag_ist_gueltiges_json_fuer_quiver() {
        let eintrag = eintrag(&Attrappe, Some("/pfad/quiver-attrappe".to_string()));
        let json = serde_json::to_value(&eintrag).unwrap();
        let gelesen: quiver_domain::Eintrag = serde_json::from_value(json.clone()).unwrap();

        assert_eq!(gelesen.id, "attrappe");
        assert_eq!(gelesen.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(gelesen.titel, "Attrappen-Tool");
        assert_eq!(gelesen.beschreibung, "Attrappe für Tests.");
        assert!(matches!(gelesen.herkunft, quiver_domain::Herkunft::Eigen));
        assert!(gelesen.aktiv, "ohne `aktiv` gilt der Eintrag als aktiv");

        match gelesen.laufzeit {
            quiver_domain::Laufzeit::Stdio {
                befehl,
                argumente,
                umgebung,
            } => {
                assert_eq!(befehl, "/pfad/quiver-attrappe");
                assert!(argumente.is_empty());
                assert_eq!(
                    umgebung.get("ATTRAPPE_URL").map(String::as_str),
                    Some("${url}")
                );
                assert_eq!(
                    umgebung.get("ATTRAPPE_TOKEN").map(String::as_str),
                    Some("${token}")
                );
                assert_eq!(
                    umgebung.get("ATTRAPPE_UMGEBUNG").map(String::as_str),
                    Some("${umgebung}")
                );
            }
            sonst => panic!("keine stdio-Laufzeit: {sonst:?}"),
        }

        assert_eq!(gelesen.felder.len(), 3);
        let url = &gelesen.felder[0];
        assert_eq!(url.name, "url");
        // Der Titel steht eigens, die Beschreibung ist nur die Hilfe.
        assert_eq!(url.titel.as_deref(), Some("Adresse"));
        assert_eq!(url.beschreibung, "Mit https://.");
        assert!(matches!(url.art, quiver_domain::FeldArt::Url));
        assert!(matches!(url.quelle, quiver_domain::Quelle::Client));
        assert!(url.pflicht);
        assert_eq!(url.standard, None);

        let token = &gelesen.felder[1];
        assert!(matches!(token.quelle, quiver_domain::Quelle::Geheim));
        assert!(matches!(token.art, quiver_domain::FeldArt::Passwort));

        let umgebung = &gelesen.felder[2];
        assert!(matches!(umgebung.quelle, quiver_domain::Quelle::Client));
        assert!(matches!(
            umgebung.art,
            quiver_domain::FeldArt::Auswahl { .. }
        ));
        assert!(!umgebung.pflicht);
        assert_eq!(umgebung.standard.as_deref(), Some("test"));

        // Kein `einrichtung`-Feld: quiver zeigt keinen „Einrichten“-Knopf.
        assert!(json.get("einrichtung").is_none(), "{json}");
        // Ein Zugang: keine Liste.
        assert!(json.get("mehrfach").is_none(), "{json}");
    }

    /// Je Art die richtige JSON-Ausgabe: snake_case, Auswahl mit `optionen`.
    #[test]
    fn jede_art_hat_ihr_json() {
        let json = serde_json::to_value(eintrag(&Arten, None)).unwrap();
        let felder = json["felder"].as_array().unwrap();
        let arten: Vec<&serde_json::Value> = felder.iter().map(|feld| &feld["art"]).collect();

        assert_eq!(
            arten,
            vec![
                &json!("text"),
                &json!("passwort"),
                &json!("zahl"),
                &json!("schalter"),
                &json!("url"),
                &json!({
                    "auswahl": {
                        "optionen": [
                            { "wert": "a", "anzeige": "A" },
                            { "wert": "b", "anzeige": "B" }
                        ]
                    }
                }),
            ],
            "{json}"
        );

        // Auch über die Attrappe lesbar: jede Art kommt beim Client an.
        let gelesen: quiver_domain::Eintrag = serde_json::from_value(json).unwrap();
        match &gelesen.felder[5].art {
            quiver_domain::FeldArt::Auswahl { optionen } => {
                assert_eq!(optionen.len(), 2);
                assert_eq!(optionen[0].wert, "a");
                assert_eq!(optionen[0].anzeige, "A");
            }
            sonst => panic!("keine Auswahl: {sonst:?}"),
        }
    }

    #[test]
    fn mehrere_zugaenge_stehen_unter_mehrfach() {
        let eintrag = eintrag(&MehrfachAttrappe, Some("/pfad/quiver-mehrfach".to_string()));
        let json = serde_json::to_value(&eintrag).unwrap();
        let gelesen: quiver_domain::Eintrag = serde_json::from_value(json.clone()).unwrap();

        // Der Eintrag selbst hat keine Felder und keine Umgebung.
        assert!(gelesen.felder.is_empty(), "{json}");
        match &gelesen.laufzeit {
            quiver_domain::Laufzeit::Stdio { umgebung, .. } => {
                assert!(umgebung.is_empty(), "{json}")
            }
            sonst => panic!("keine stdio-Laufzeit: {sonst:?}"),
        }

        let mehrfach = gelesen.mehrfach.expect("mehrfach fehlt");
        assert_eq!(mehrfach.titel, "Verbindungen");
        assert_eq!(mehrfach.einzahl, "Verbindung");
        assert_eq!(mehrfach.felder.len(), 2);
        assert_eq!(mehrfach.felder[0].name, "host");
        assert_eq!(mehrfach.felder[0].titel.as_deref(), Some("Hostname"));
        assert_eq!(mehrfach.felder[0].beschreibung, "Hostname oder IP");
        assert!(matches!(
            mehrfach.felder[0].art,
            quiver_domain::FeldArt::Text
        ));
        assert!(matches!(
            mehrfach.felder[0].quelle,
            quiver_domain::Quelle::Client
        ));
        assert!(matches!(
            mehrfach.felder[1].quelle,
            quiver_domain::Quelle::Geheim
        ));
        assert!(!mehrfach.felder[1].pflicht);
    }

    #[test]
    fn ohne_befehl_wird_das_laufende_programm_genannt() {
        let eintrag = eintrag(&Attrappe, None);
        let Laufzeit::Stdio { befehl, .. } = &eintrag.laufzeit;
        assert!(!befehl.is_empty());
    }
}
