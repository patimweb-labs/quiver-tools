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
    /// Der User am Gerät, im Schlüsselbund; verlässt das Gerät nie.
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
    /// `titel – hilfe`.
    pub beschreibung: String,
    pub quelle: Quelle,
    pub pflicht: bool,
    pub standard: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<FeldLink>,
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
}

/// Der Eintrag eines Werkzeugs.
///
/// `befehl` ist der Pfad des Programms; ohne Angabe der Pfad des laufenden
/// Programms, sonst `quiver-<name>` (quiver sucht es dann im Pfad).
pub fn eintrag<W: Werkzeug>(werkzeug: &W, befehl: Option<String>) -> Eintrag {
    let name = werkzeug.name();
    let mut umgebung = BTreeMap::new();
    let mut felder = Vec::new();
    for feld in werkzeug.felder() {
        umgebung.insert(schluessel(name, feld.name), format!("${{{}}}", feld.name));
        felder.push(Feld {
            name: feld.name.to_string(),
            beschreibung: beschreibung(feld.titel, feld.hilfe),
            quelle: if feld.art == FeldArt::Passwort {
                Quelle::Geheim
            } else {
                Quelle::Client
            },
            pflicht: feld.pflicht,
            standard: feld.standard.map(str::to_string),
            link: feld.link.map(|(text, url)| FeldLink {
                text: text.to_string(),
                url: url.to_string(),
            }),
        });
    }

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
    }
}

/// `titel – hilfe`; ohne Hilfe bleibt der Titel allein.
fn beschreibung(titel: &str, hilfe: &str) -> String {
    if hilfe.is_empty() {
        titel.to_string()
    } else {
        format!("{titel} – {hilfe}")
    }
}

/// Das laufende Programm, sonst der Name, unter dem quiver es im Pfad sucht.
fn programm(name: &str) -> String {
    std::env::current_exe()
        .ok()
        .map(|pfad| pfad.display().to_string())
        .unwrap_or_else(|| format!("quiver-{name}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Feld as KernFeld, Profil};

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

        #[derive(Debug, Deserialize)]
        pub struct Feld {
            pub name: String,
            #[serde(default)]
            pub beschreibung: String,
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
        assert_eq!(url.beschreibung, "Adresse – Mit https://.");
        assert!(matches!(url.quelle, quiver_domain::Quelle::Client));
        assert!(url.pflicht);
        assert_eq!(url.standard, None);

        let token = &gelesen.felder[1];
        assert!(matches!(token.quelle, quiver_domain::Quelle::Geheim));

        let umgebung = &gelesen.felder[2];
        assert!(matches!(umgebung.quelle, quiver_domain::Quelle::Client));
        assert!(!umgebung.pflicht);
        assert_eq!(umgebung.standard.as_deref(), Some("test"));

        // Kein `einrichtung`-Feld: quiver zeigt keinen „Einrichten“-Knopf.
        assert!(json.get("einrichtung").is_none(), "{json}");
    }

    #[test]
    fn ohne_befehl_wird_das_laufende_programm_genannt() {
        let eintrag = eintrag(&Attrappe, None);
        let Laufzeit::Stdio { befehl, .. } = &eintrag.laufzeit;
        assert!(!befehl.is_empty());
    }
}
