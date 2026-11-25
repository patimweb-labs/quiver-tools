// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Eine Azure-DevOps-Organisation, wie der User sie im quiver-Fenster pflegt
//! (Aufgabe 09).
//!
//! quiver übergibt **alle** Organisationen als JSON in `QUIVER_ZUGAENGE`
//! (`quiver_domain::Mehrfach`): Name plus Werte der Felder `url`, `projekt`,
//! `pat`, `schreiben`. Dieses Modul macht daraus geprüfte [`Organisation`]en und
//! wählt die richtige aus. Der PAT bleibt im Speicher und erscheint in keiner
//! Antwort – `Debug` zeigt nur, *ob* einer gesetzt ist.

use std::collections::BTreeMap;

use quiver_tools_kern::zugaenge::{self, ZugaengeFehler};

/// Feldname der Organisationsadresse (Pflicht), z. B. `https://dev.azure.com/beispiel`.
pub const URL: &str = "url";
/// Feldname des Standardprojekts (optional); leer heißt „je Aufruf angeben“.
pub const PROJEKT: &str = "projekt";
/// Feldname des Personal Access Token (geheim, Pflicht).
pub const PAT: &str = "pat";
/// Feldname des Schalters „Schreiben erlauben“ (Standard **aus**).
pub const SCHREIBEN: &str = "schreiben";
/// Feldname des Schalters „Unverschlüsselt erlauben“ (Standard **aus**).
pub const UNVERSCHLUESSELT: &str = quiver_tools_kern::net::FELD_UNVERSCHLUESSELT;
/// Feldname des eigenen CA-Zertifikats (Pfad einer Datei).
pub const CA_ZERTIFIKAT: &str = quiver_tools_kern::net::FELD_CA_ZERTIFIKAT;

/// Eine Organisation mit allen Werten. `Debug` zeigt den PAT nie.
#[derive(Clone, PartialEq, Eq)]
pub struct Organisation {
    /// Name aus dem quiver-Fenster, z. B. `beispiel`.
    pub name: String,
    /// Basisadresse ohne Schrägstrich am Ende.
    pub url: String,
    /// Standardprojekt; `None` heißt, das Projekt kommt mit dem Aufruf.
    pub projekt: Option<String>,
    /// Personal Access Token. Steht nur im Speicher und nur in der Kopfzeile.
    pub pat: String,
    /// Erlaubt die verändernden Werkzeuge. Standard **aus**.
    pub schreiben: bool,
    /// Eigenes CA-Zertifikat (Pfad einer PEM-Datei), falls eingetragen.
    pub ca_zertifikat: Option<String>,
}

impl std::fmt::Debug for Organisation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Organisation")
            .field("name", &self.name)
            .field("url", &self.url)
            .field("projekt", &self.projekt)
            .field("pat", &(!self.pat.is_empty()))
            .field("schreiben", &self.schreiben)
            .field("ca_zertifikat", &self.ca_zertifikat.is_some())
            .finish()
    }
}

impl Organisation {
    /// Eine Organisation aus einem Eintrag der Liste.
    pub fn aus_zugang(zugang: &zugaenge::Zugang) -> Result<Organisation, OrganisationsFehler> {
        let name = zugang.name.trim().to_string();
        if name.is_empty() {
            return Err(OrganisationsFehler::Name);
        }

        let feld = |feld: &'static str| -> Result<String, OrganisationsFehler> {
            zugang
                .wert(feld)
                .map(str::to_string)
                .ok_or_else(|| OrganisationsFehler::Feld {
                    name: name.clone(),
                    feld,
                })
        };

        let url = feld(URL)?;
        let pat = feld(PAT)?;

        let url = url.trim().trim_end_matches('/').to_string();
        // Eine Regel für alle Tools (kern::net): https immer, http nur auf dem
        // Loopback oder mit eingeschaltetem Feld „Unverschlüsselt erlauben“.
        if quiver_tools_kern::net::adresse_erlaubt(
            &url,
            schalter_aus(zugang.wert(UNVERSCHLUESSELT)),
        )
        .is_err()
        {
            return Err(OrganisationsFehler::Url {
                name: name.clone(),
                url,
            });
        }

        Ok(Organisation {
            name,
            url,
            projekt: zugang.wert(PROJEKT).map(str::to_string),
            pat,
            schreiben: schalter_aus(zugang.wert(SCHREIBEN)),
            ca_zertifikat: zugang.wert(CA_ZERTIFIKAT).map(str::to_string),
        })
    }

    /// Erlaubt diese Organisation die verändernden Werkzeuge?
    pub fn schreiben_erlaubt(&self) -> bool {
        self.schreiben
    }

    /// Das Projekt dieses Aufrufs: das genannte, sonst das des Zugangs.
    pub fn projekt(&self, genannt: Option<&str>) -> Option<String> {
        genannt
            .map(str::trim)
            .filter(|projekt| !projekt.is_empty())
            .map(str::to_string)
            .or_else(|| self.projekt.clone())
    }

    /// `beispiel · https://dev.azure.com/beispiel · Standardprojekt PROJEKT
    /// (frei wählbar) · Schreiben aus` – ohne PAT.
    ///
    /// Das Projekt ist ein **Standard**, keine Grenze: jedes Werkzeug darf ein
    /// anderes nennen. Der Text sagt das ausdrücklich (Härtung Paket 4).
    pub fn beschreibung(&self) -> String {
        let projekt = match self.projekt.as_deref() {
            Some(projekt) => format!("Standardprojekt {projekt} (frei wählbar)"),
            None => "Standardprojekt (frei wählbar, je Aufruf)".to_string(),
        };
        format!(
            "{} · {} · {} · Schreiben {}",
            self.name,
            self.url,
            projekt,
            if self.schreiben { "an" } else { "aus" }
        ) + if self.url.starts_with("http://") {
            " · ohne Verschlüsselung (http)"
        } else {
            ""
        }
    }
}

/// Ein Schalter, der im Zweifel **aus** ist (Feld `schreiben`, Aufgabe 09).
///
/// Nur ein ausdrückliches Einschalten (`an`, `ja`, `1`, `true`, `ein`, `on`)
/// gilt; ein fehlendes Feld und jede andere Schreibweise bleiben aus. Für eine
/// Kundenorganisation ist „nichts gesagt“ die sichere Antwort.
pub fn schalter_aus(wert: Option<&str>) -> bool {
    match wert.map(str::trim) {
        None | Some("") => false,
        Some(wert) => matches!(
            wert.to_ascii_lowercase().as_str(),
            "an" | "ja" | "1" | "true" | "ein" | "on"
        ),
    }
}

/// Alle Organisationen, in der Reihenfolge der Liste.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sammlung {
    organisationen: Vec<Organisation>,
}

impl Sammlung {
    /// Aus der echten Umgebung (`QUIVER_ZUGAENGE`).
    pub fn aus_umgebung() -> Result<Sammlung, SammelFehler> {
        Sammlung::aus_zugaengen(zugaenge::zugaenge()?)
    }

    /// Aus einer beliebigen Quelle (Tests).
    pub fn mit_umgebung<F>(lesen: F) -> Result<Sammlung, SammelFehler>
    where
        F: Fn(&str) -> Option<String>,
    {
        Sammlung::aus_zugaengen(zugaenge::mit_umgebung(lesen)?)
    }

    /// Aus der Liste, wie quiver sie übergibt.
    pub fn aus_zugaengen(liste: Vec<zugaenge::Zugang>) -> Result<Sammlung, SammelFehler> {
        let mut organisationen = Vec::with_capacity(liste.len());
        let mut namen = BTreeMap::new();
        for zugang in &liste {
            let organisation = Organisation::aus_zugang(zugang)?;
            if namen.insert(organisation.name.clone(), ()).is_some() {
                return Err(SammelFehler::Doppelt(organisation.name));
            }
            organisationen.push(organisation);
        }
        Ok(Sammlung { organisationen })
    }

    pub fn alle(&self) -> &[Organisation] {
        &self.organisationen
    }

    pub fn ist_leer(&self) -> bool {
        self.organisationen.is_empty()
    }

    pub fn namen(&self) -> Vec<&str> {
        self.organisationen
            .iter()
            .map(|organisation| organisation.name.as_str())
            .collect()
    }

    /// Die gewünschte Organisation.
    ///
    /// Ohne Namen gilt die einzige; bei mehreren muss der Agent wählen.
    pub fn waehlen(&self, name: Option<&str>) -> Result<&Organisation, AuswahlFehler> {
        match name.map(str::trim).filter(|name| !name.is_empty()) {
            Some(name) => self
                .organisationen
                .iter()
                .find(|organisation| organisation.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| AuswahlFehler::Unbekannt {
                    name: name.to_string(),
                    bekannt: self.namen().iter().map(|n| n.to_string()).collect(),
                }),
            None => match self.organisationen.as_slice() {
                [] => Err(AuswahlFehler::Keine),
                [einzige] => Ok(einzige),
                mehrere => Err(AuswahlFehler::Mehrdeutig(
                    mehrere
                        .iter()
                        .map(|organisation| organisation.name.clone())
                        .collect(),
                )),
            },
        }
    }
}

/// Was an den Werten einer Organisation nicht stimmt.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OrganisationsFehler {
    #[error("Eine Organisation im quiver-Fenster hat keinen Namen – bitte einen Namen vergeben.")]
    Name,
    #[error(
        "Bei der Organisation „{name}“ fehlt das Feld „{feld}“ – bitte im quiver-Fenster eintragen."
    )]
    Feld { name: String, feld: &'static str },
    #[error(
        "Bei der Organisation „{name}“ ist die Adresse „{url}“ nicht erlaubt: sie muss mit \
         https:// beginnen. Für eine Adresse mit http:// bitte das Feld „Unverschlüsselt \
         erlauben“ einschalten (das Token geht dann offen über das Netz)."
    )]
    Url { name: String, url: String },
}

/// Warum die Liste nicht gelesen werden konnte.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SammelFehler {
    #[error(transparent)]
    Zugaenge(#[from] ZugaengeFehler),
    #[error(transparent)]
    Organisation(#[from] OrganisationsFehler),
    #[error(
        "Die Organisation „{0}“ gibt es mehrfach – bitte im quiver-Fenster einen Namen ändern."
    )]
    Doppelt(String),
}

/// Warum keine Organisation gewählt werden konnte.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuswahlFehler {
    #[error("Es ist keine Azure-DevOps-Organisation eingerichtet.")]
    Keine,
    #[error(
        "Es gibt mehrere Organisationen ({}) – bitte `verbindung` angeben.",
        .0.join(", ")
    )]
    Mehrdeutig(Vec<String>),
    #[error(
        "Die Organisation „{}“ gibt es nicht. Bekannt sind: {}.",
        .name,
        .bekannt.join(", ")
    )]
    Unbekannt { name: String, bekannt: Vec<String> },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zugang(name: &str, werte: &[(&str, &str)]) -> zugaenge::Zugang {
        zugaenge::Zugang {
            name: name.to_string(),
            werte: werte
                .iter()
                .map(|(feld, wert)| (feld.to_string(), wert.to_string()))
                .collect(),
        }
    }

    fn vollstaendig(name: &str) -> zugaenge::Zugang {
        zugang(
            name,
            &[
                ("url", "https://dev.azure.com/beispiel/"),
                ("projekt", "PROJEKT"),
                ("pat", "pat-4711"),
            ],
        )
    }

    #[test]
    fn eine_organisation_wird_geprueft_und_hat_vorgaben() {
        let organisation = Organisation::aus_zugang(&vollstaendig("beispiel")).unwrap();
        assert_eq!(organisation.name, "beispiel");
        // Der Schrägstrich am Ende fliegt raus.
        assert_eq!(organisation.url, "https://dev.azure.com/beispiel");
        assert_eq!(organisation.projekt.as_deref(), Some("PROJEKT"));
        // Ohne Feld ist das Schreiben aus – der sichere Zustand.
        assert!(!organisation.schreiben);
        assert!(!organisation.schreiben_erlaubt());
        assert_eq!(
            organisation.beschreibung(),
            "beispiel · https://dev.azure.com/beispiel · Standardprojekt PROJEKT (frei wählbar) · Schreiben aus"
        );
        assert_eq!(organisation.projekt(None).as_deref(), Some("PROJEKT"));
        assert_eq!(
            organisation.projekt(Some(" Anderes ")).as_deref(),
            Some("Anderes")
        );
        assert_eq!(
            organisation.projekt(Some("   ")).as_deref(),
            Some("PROJEKT")
        );
    }

    #[test]
    fn ohne_projekt_bleibt_das_projekt_beim_aufruf() {
        let organisation = Organisation::aus_zugang(&zugang(
            "kunde",
            &[
                ("url", "https://tfs.kunde.de/tfs/DefaultCollection"),
                ("pat", "pat-4712"),
            ],
        ))
        .unwrap();
        assert_eq!(organisation.projekt, None);
        assert_eq!(organisation.projekt(None), None);
        assert_eq!(
            organisation.beschreibung(),
            "kunde · https://tfs.kunde.de/tfs/DefaultCollection · Standardprojekt \
             (frei wählbar, je Aufruf) · Schreiben aus"
        );
        assert_eq!(
            organisation.projekt(Some("PROJEKT")).as_deref(),
            Some("PROJEKT")
        );
    }

    #[test]
    fn schreiben_ist_nur_ausdruecklich_an() {
        let mit = |wert: &str| {
            Organisation::aus_zugang(&zugang(
                "org",
                &[
                    ("url", "https://dev.azure.com/org"),
                    ("pat", "p"),
                    ("schreiben", wert),
                ],
            ))
            .unwrap()
            .schreiben
        };

        for an in ["an", "AN", "ja", "1", "true", "ein", "on"] {
            assert!(mit(an), "{an} sollte an sein");
        }
        for aus in ["aus", "nein", "0", "false", "off", "quatsch", "  "] {
            assert!(!mit(aus), "{aus} sollte aus sein");
        }
        assert!(!schalter_aus(None));
        assert!(!schalter_aus(Some("")));
    }

    #[test]
    fn debug_zeigt_keinen_pat() {
        let organisation = Organisation::aus_zugang(&vollstaendig("beispiel")).unwrap();
        let text = format!("{organisation:?}");
        assert!(!text.contains("pat-4711"), "{text}");
        assert!(text.contains("pat: true"), "{text}");
    }

    #[test]
    fn pflichtfelder_und_adresse_werden_geprueft() {
        assert_eq!(
            Organisation::aus_zugang(&zugang("org", &[("url", "https://x")])),
            Err(OrganisationsFehler::Feld {
                name: "org".into(),
                feld: PAT
            })
        );
        assert_eq!(
            Organisation::aus_zugang(&zugang("org", &[("pat", "p")])),
            Err(OrganisationsFehler::Feld {
                name: "org".into(),
                feld: URL
            })
        );
        assert_eq!(
            Organisation::aus_zugang(&zugang(
                "org",
                &[("url", "dev.azure.com/org"), ("pat", "p")]
            )),
            Err(OrganisationsFehler::Url {
                name: "org".into(),
                url: "dev.azure.com/org".into()
            })
        );
        assert_eq!(
            Organisation::aus_zugang(&zugang("  ", &[("url", "https://x"), ("pat", "p")])),
            Err(OrganisationsFehler::Name)
        );
    }

    #[test]
    fn die_auswahl_ist_ohne_namen_eindeutig() {
        let eine = Sammlung::aus_zugaengen(vec![vollstaendig("beispiel")]).unwrap();
        assert_eq!(eine.waehlen(None).unwrap().name, "beispiel");
        assert_eq!(eine.waehlen(Some("beispiel")).unwrap().name, "beispiel");
        assert!(matches!(
            eine.waehlen(Some("gibtsnicht")),
            Err(AuswahlFehler::Unbekannt { .. })
        ));

        let zwei = zwei_organisationen();
        assert_eq!(zwei.alle().len(), 2);
        assert_eq!(zwei.namen(), vec!["beispiel", "kunde"]);
        assert!(matches!(
            zwei.waehlen(None),
            Err(AuswahlFehler::Mehrdeutig(_))
        ));
        assert_eq!(zwei.waehlen(Some("kunde")).unwrap().projekt, None);

        let keine = Sammlung::default();
        assert!(keine.ist_leer());
        assert_eq!(keine.waehlen(None), Err(AuswahlFehler::Keine));
    }

    /// Zwei Organisationen für die Auswahl-Tests.
    fn zwei_organisationen() -> Sammlung {
        Sammlung::aus_zugaengen(vec![
            vollstaendig("beispiel"),
            zugang(
                "kunde",
                &[
                    ("url", "https://tfs.kunde.de/tfs/DefaultCollection"),
                    ("pat", "pat-4712"),
                ],
            ),
        ])
        .unwrap()
    }

    #[test]
    fn doppelte_namen_und_kaputtes_json_werden_gemeldet() {
        let doppelt = Sammlung::aus_zugaengen(vec![vollstaendig("org"), vollstaendig("org")]);
        assert_eq!(doppelt, Err(SammelFehler::Doppelt("org".into())));

        let kaputt = Sammlung::mit_umgebung(|_| Some("[{\"name\":".to_string()));
        assert!(matches!(kaputt, Err(SammelFehler::Zugaenge(_))));

        let ohne_feld = Sammlung::aus_zugaengen(vec![zugang("org", &[])]);
        assert!(matches!(ohne_feld, Err(SammelFehler::Organisation(_))));
    }

    #[test]
    fn ohne_umgebung_ist_die_sammlung_leer() {
        let leer = Sammlung::mit_umgebung(|_| None).unwrap();
        assert!(leer.ist_leer());
    }
}
