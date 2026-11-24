// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Eine Confluence-Cloud-Site, wie der User sie im quiver-Fenster pflegt
//! (Aufgabe 14).
//!
//! quiver übergibt **alle** Sites als JSON in `QUIVER_ZUGAENGE`
//! (`quiver_domain::Mehrfach`): Name plus Werte der Felder `url`, `email`,
//! `api_token`, `nur`, `schreiben`. Dieses Modul macht daraus geprüfte
//! [`Site`]en und wählt die richtige aus.
//!
//! Zwei Regeln stehen hier und gelten für alles Weitere:
//!
//! - Die Adresse ist **https**; nur die Attrappe darf auf dem Loopback per
//!   `http` laufen (Aufgabe 12, Sicherheit 1).
//! - E-Mail und Token sind Teil der Basic-Anmeldung und erscheinen in keiner
//!   Antwort – `Debug` zeigt nur, *ob* sie gesetzt sind (Sicherheit 3).

use std::collections::BTreeMap;

use quiver_tools_kern::zugaenge::{self, ZugaengeFehler};
use url::Url;

/// Feldname der Site-Adresse (Pflicht), z. B. `https://firma.atlassian.net`.
pub const URL: &str = "url";
/// Feldname der E-Mail des Atlassian-Kontos (Pflicht).
pub const EMAIL: &str = "email";
/// Feldname des API-Tokens (geheim, Pflicht).
pub const TOKEN: &str = "api_token";
/// Feldname der erlaubten Bereichsschlüssel (optional, Text kommagetrennt).
pub const NUR: &str = "nur";
/// Feldname des Schalters „Schreiben erlauben“ (Standard **aus**).
pub const SCHREIBEN: &str = "schreiben";

/// Eine Site mit allen Werten. `Debug` zeigt weder Token noch E-Mail.
#[derive(Clone, PartialEq, Eq)]
pub struct Site {
    /// Name aus dem quiver-Fenster, z. B. `firma`.
    pub name: String,
    /// Basisadresse ohne Schrägstrich am Ende.
    pub url: String,
    /// E-Mail des Atlassian-Kontos; nur Teil der Basic-Kopfzeile.
    pub email: String,
    /// API-Token; nur im Speicher und nur in der Kopfzeile.
    pub token: String,
    /// Erlaubte Bereichsschlüssel aus `nur`; leer heißt „alle“.
    pub nur: Vec<String>,
    /// Erlaubt die verändernden Werkzeuge. Standard **aus**.
    pub schreiben: bool,
}

impl std::fmt::Debug for Site {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Site")
            .field("name", &self.name)
            .field("url", &self.url)
            .field("email", &(!self.email.is_empty()))
            .field("token", &(!self.token.is_empty()))
            .field("nur", &self.nur)
            .field("schreiben", &self.schreiben)
            .finish()
    }
}

impl Site {
    /// Eine Site aus einem Eintrag der Liste.
    pub fn aus_zugang(zugang: &zugaenge::Zugang) -> Result<Site, SiteFehler> {
        let name = zugang.name.trim().to_string();
        if name.is_empty() {
            return Err(SiteFehler::Name);
        }

        let feld = |feld: &'static str| -> Result<String, SiteFehler> {
            zugang
                .wert(feld)
                .map(str::to_string)
                .ok_or_else(|| SiteFehler::Feld {
                    name: name.clone(),
                    feld,
                })
        };

        let url = feld(URL)?;
        let email = feld(EMAIL)?;
        let token = feld(TOKEN)?;

        let url = url.trim().trim_end_matches('/').to_string();
        if !adresse_erlaubt(&url) {
            return Err(SiteFehler::Url {
                name: name.clone(),
                url,
            });
        }

        Ok(Site {
            name,
            url,
            email,
            token,
            nur: bereiche_aus(zugang.wert(NUR)),
            schreiben: schalter_aus(zugang.wert(SCHREIBEN)),
        })
    }

    /// Erlaubt diese Site die verändernden Werkzeuge?
    pub fn schreiben_erlaubt(&self) -> bool {
        self.schreiben
    }

    /// Ist dieser Bereichsschlüssel für das Schreiben freigegeben?
    ///
    /// Ohne `nur` ist alles erlaubt; sonst zählt der Vergleich ohne
    /// Groß-/Kleinschreibung. Der Schlüssel kommt immer vom Server, nie vom
    /// Agenten (Aufgabe 12, Sicherheit 6).
    pub fn bereich_erlaubt(&self, schluessel: &str) -> bool {
        if self.nur.is_empty() {
            return true;
        }
        let schluessel = schluessel.trim();
        self.nur
            .iter()
            .any(|erlaubt| erlaubt.eq_ignore_ascii_case(schluessel))
    }

    /// `firma · https://firma.atlassian.net · Bereiche alle · Schreiben aus`
    /// – ohne Token und ohne E-Mail.
    pub fn beschreibung(&self) -> String {
        format!(
            "{} · {} · Bereiche {} · Schreiben {}",
            self.name,
            self.url,
            if self.nur.is_empty() {
                "alle".to_string()
            } else {
                self.nur.join(", ")
            },
            if self.schreiben { "an" } else { "aus" }
        )
    }
}

/// Darf die Adresse so eingetragen werden?
///
/// **https** immer; `http` nur für den Loopback der Attrappe
/// (`127.0.0.1`, `localhost`, `[::1]`, auch mit Port). Geprüft wird über
/// [`url::Url`] – nie über eine eigene Zerlegung: eine Adresse mit
/// Benutzerteil, Abfrage oder Fragment ist kein Site-Ziel (Korrekturen 4).
pub fn adresse_erlaubt(url: &str) -> bool {
    let Ok(url) = Url::parse(url) else {
        return false;
    };
    if !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    if url.query().is_some() || url.fragment().is_some() {
        return false;
    }
    // Die Basis ist die Site selbst; ein Pfad würde `.../wiki/wiki/api/v2`
    // ergeben (`/wiki` hängt das Tool selbst an).
    if url.path() != "/" {
        return false;
    }
    match url.scheme() {
        "https" => true,
        "http" => {
            let host = url.host_str().unwrap_or_default();
            let host = host
                .strip_prefix('[')
                .and_then(|host| host.strip_suffix(']'))
                .unwrap_or(host);
            matches!(host, "127.0.0.1" | "localhost" | "::1")
        }
        _ => false,
    }
}

/// Die Bereichsschlüssel aus dem Feld `nur` (kommagetrennt, ohne Leerraum).
pub fn bereiche_aus(wert: Option<&str>) -> Vec<String> {
    wert.map(|wert| {
        wert.split(',')
            .map(str::trim)
            .filter(|teil| !teil.is_empty())
            .map(str::to_string)
            .collect()
    })
    .unwrap_or_default()
}

/// Ein Schalter, der im Zweifel **aus** ist (Feld `schreiben`).
///
/// Nur ein ausdrückliches Einschalten (`an`, `ja`, `1`, `true`, `ein`, `on`)
/// gilt; ein fehlendes Feld und jede andere Schreibweise bleiben aus.
pub fn schalter_aus(wert: Option<&str>) -> bool {
    match wert.map(str::trim) {
        None | Some("") => false,
        Some(wert) => matches!(
            wert.to_ascii_lowercase().as_str(),
            "an" | "ja" | "1" | "true" | "ein" | "on"
        ),
    }
}

/// Alle Sites, in der Reihenfolge der Liste.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sammlung {
    sites: Vec<Site>,
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
        let mut sites = Vec::with_capacity(liste.len());
        let mut namen = BTreeMap::new();
        for zugang in &liste {
            let site = Site::aus_zugang(zugang)?;
            if namen.insert(site.name.clone(), ()).is_some() {
                return Err(SammelFehler::Doppelt(site.name));
            }
            sites.push(site);
        }
        Ok(Sammlung { sites })
    }

    pub fn alle(&self) -> &[Site] {
        &self.sites
    }

    pub fn ist_leer(&self) -> bool {
        self.sites.is_empty()
    }

    pub fn namen(&self) -> Vec<&str> {
        self.sites.iter().map(|site| site.name.as_str()).collect()
    }

    /// Die gewünschte Site.
    ///
    /// Ohne Namen gilt die einzige; bei mehreren muss der Agent wählen.
    pub fn waehlen(&self, name: Option<&str>) -> Result<&Site, AuswahlFehler> {
        match name.map(str::trim).filter(|name| !name.is_empty()) {
            Some(name) => self
                .sites
                .iter()
                .find(|site| site.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| AuswahlFehler::Unbekannt {
                    name: name.to_string(),
                    bekannt: self.namen().iter().map(|n| n.to_string()).collect(),
                }),
            None => match self.sites.as_slice() {
                [] => Err(AuswahlFehler::Keine),
                [einzige] => Ok(einzige),
                mehrere => Err(AuswahlFehler::Mehrdeutig(
                    mehrere.iter().map(|site| site.name.clone()).collect(),
                )),
            },
        }
    }
}

/// Was an den Werten einer Site nicht stimmt.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SiteFehler {
    #[error("Eine Site im quiver-Fenster hat keinen Namen – bitte einen Namen vergeben.")]
    Name,
    #[error("Bei der Site „{name}“ fehlt das Feld „{feld}“ – bitte im quiver-Fenster eintragen.")]
    Feld { name: String, feld: &'static str },
    #[error(
        "Die Adresse „{url}“ von „{name}“ muss mit https:// beginnen – nur 127.0.0.1, localhost \
         und [::1] dürfen für die Attrappe http:// sein."
    )]
    Url { name: String, url: String },
}

/// Warum die Liste nicht gelesen werden konnte.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SammelFehler {
    #[error(transparent)]
    Zugaenge(#[from] ZugaengeFehler),
    #[error(transparent)]
    Site(#[from] SiteFehler),
    #[error("Die Site „{0}“ gibt es mehrfach – bitte im quiver-Fenster einen Namen ändern.")]
    Doppelt(String),
}

/// Warum keine Site gewählt werden konnte.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuswahlFehler {
    #[error("Es ist keine Confluence-Site eingerichtet.")]
    Keine,
    #[error(
        "Es gibt mehrere Sites ({}) – bitte `verbindung` angeben.",
        .0.join(", ")
    )]
    Mehrdeutig(Vec<String>),
    #[error(
        "Die Site „{}“ gibt es nicht. Bekannt sind: {}.",
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
                ("url", "https://firma.atlassian.net/"),
                ("email", "patrick@example.test"),
                ("api_token", "token-4711"),
            ],
        )
    }

    #[test]
    fn eine_site_wird_geprueft_und_hat_vorgaben() {
        let site = Site::aus_zugang(&vollstaendig("firma")).unwrap();
        assert_eq!(site.name, "firma");
        // Der Schrägstrich am Ende fliegt raus.
        assert_eq!(site.url, "https://firma.atlassian.net");
        assert!(site.nur.is_empty());
        // Ohne Feld ist das Schreiben aus – der sichere Zustand.
        assert!(!site.schreiben);
        assert!(!site.schreiben_erlaubt());
        assert!(site.bereich_erlaubt("egal"));
        assert_eq!(
            site.beschreibung(),
            "firma · https://firma.atlassian.net · Bereiche alle · Schreiben aus"
        );
    }

    #[test]
    fn nur_grenzt_die_bereiche_ohne_gross_kleinschreibung_ein() {
        let mut eintrag = vollstaendig("firma");
        eintrag
            .werte
            .insert("nur".to_string(), " ENG , ops ".to_string());
        let site = Site::aus_zugang(&eintrag).unwrap();
        assert_eq!(site.nur, vec!["ENG", "ops"]);
        assert!(site.bereich_erlaubt("eng"));
        assert!(site.bereich_erlaubt("OPS"));
        assert!(!site.bereich_erlaubt("DOC"));
        assert!(!site.bereich_erlaubt(""));
        assert_eq!(
            site.beschreibung(),
            "firma · https://firma.atlassian.net · Bereiche ENG, ops · Schreiben aus"
        );

        // Ein leeres `nur` heißt „alle“.
        let site = Site::aus_zugang(&zugang(
            "firma",
            &[
                ("url", "https://firma.atlassian.net"),
                ("email", "a@b.test"),
                ("api_token", "t"),
                ("nur", " , "),
            ],
        ))
        .unwrap();
        assert!(site.nur.is_empty());
        assert!(site.bereich_erlaubt("beliebig"));
    }

    #[test]
    fn schreiben_ist_nur_ausdruecklich_an() {
        let mit = |wert: &str| {
            Site::aus_zugang(&zugang(
                "site",
                &[
                    ("url", "https://x.atlassian.net"),
                    ("email", "a@b.test"),
                    ("api_token", "t"),
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
    fn debug_zeigt_weder_token_noch_email() {
        let site = Site::aus_zugang(&vollstaendig("firma")).unwrap();
        let text = format!("{site:?}");
        assert!(!text.contains("token-4711"), "{text}");
        assert!(!text.contains("patrick@example.test"), "{text}");
        assert!(text.contains("token: true"), "{text}");
        assert!(text.contains("email: true"), "{text}");
    }

    #[test]
    fn die_adresse_ist_https_ausser_loopback() {
        for erlaubt in [
            "https://firma.atlassian.net",
            "https://x",
            "http://127.0.0.1:8080",
            "http://localhost",
            "http://localhost:1234",
            "http://[::1]:8080",
            "http://[::1]",
        ] {
            assert!(adresse_erlaubt(erlaubt), "{erlaubt} sollte erlaubt sein");
        }
        for verboten in [
            "http://firma.atlassian.net",
            "http://127.0.0.2",
            "http://localhost.evil.test",
            "http://evil.test/127.0.0.1",
            "ftp://x",
            "firma.atlassian.net",
            "",
            // Ein Benutzerteil ist kein Site-Ziel.
            "http://127.0.0.1@evil.test",
            "https://evil.test\\@127.0.0.1/",
            "http://evil.test\\@127.0.0.1/",
            "https://user:pass@firma.atlassian.net",
            // Abfrage und Fragment gehören nicht in die Basisadresse.
            "https://firma.atlassian.net?x=1",
            "https://firma.atlassian.net#x",
            // `/wiki` hängt das Tool selbst an.
            "https://firma.atlassian.net/wiki",
        ] {
            assert!(
                !adresse_erlaubt(verboten),
                "{verboten} sollte verboten sein"
            );
        }
    }

    #[test]
    fn pflichtfelder_und_adresse_werden_geprueft() {
        assert_eq!(
            Site::aus_zugang(&zugang("site", &[("url", "https://x")])),
            Err(SiteFehler::Feld {
                name: "site".into(),
                feld: EMAIL
            })
        );
        assert_eq!(
            Site::aus_zugang(&zugang(
                "site",
                &[("url", "https://x"), ("email", "a@b.test")]
            )),
            Err(SiteFehler::Feld {
                name: "site".into(),
                feld: TOKEN
            })
        );
        assert_eq!(
            Site::aus_zugang(&zugang(
                "site",
                &[("email", "a@b.test"), ("api_token", "t")]
            )),
            Err(SiteFehler::Feld {
                name: "site".into(),
                feld: URL
            })
        );
        assert_eq!(
            Site::aus_zugang(&zugang(
                "site",
                &[
                    ("url", "http://firma.atlassian.net"),
                    ("email", "a@b.test"),
                    ("api_token", "t"),
                ]
            )),
            Err(SiteFehler::Url {
                name: "site".into(),
                url: "http://firma.atlassian.net".into()
            })
        );
        assert_eq!(Site::aus_zugang(&zugang("  ", &[])), Err(SiteFehler::Name));
    }

    #[test]
    fn die_auswahl_ist_ohne_namen_eindeutig() {
        let eine = Sammlung::aus_zugaengen(vec![vollstaendig("firma")]).unwrap();
        assert_eq!(eine.waehlen(None).unwrap().name, "firma");
        assert_eq!(eine.waehlen(Some("FIRMA")).unwrap().name, "firma");
        assert!(matches!(
            eine.waehlen(Some("gibtsnicht")),
            Err(AuswahlFehler::Unbekannt { .. })
        ));

        let zwei = zwei_sites();
        assert_eq!(zwei.alle().len(), 2);
        assert_eq!(zwei.namen(), vec!["firma", "kunde"]);
        assert!(matches!(
            zwei.waehlen(None),
            Err(AuswahlFehler::Mehrdeutig(_))
        ));
        assert_eq!(zwei.waehlen(Some("kunde")).unwrap().name, "kunde");

        let keine = Sammlung::default();
        assert!(keine.ist_leer());
        assert_eq!(keine.waehlen(None), Err(AuswahlFehler::Keine));
    }

    /// Zwei Sites für die Auswahl-Tests.
    fn zwei_sites() -> Sammlung {
        Sammlung::aus_zugaengen(vec![
            vollstaendig("firma"),
            zugang(
                "kunde",
                &[
                    ("url", "https://kunde.atlassian.net"),
                    ("email", "a@b.test"),
                    ("api_token", "token-4712"),
                ],
            ),
        ])
        .unwrap()
    }

    #[test]
    fn doppelte_namen_und_kaputtes_json_werden_gemeldet() {
        let doppelt = Sammlung::aus_zugaengen(vec![vollstaendig("site"), vollstaendig("site")]);
        assert_eq!(doppelt, Err(SammelFehler::Doppelt("site".into())));

        let kaputt = Sammlung::mit_umgebung(|_| Some("[{\"name\":".to_string()));
        assert!(matches!(kaputt, Err(SammelFehler::Zugaenge(_))));

        let ohne_feld = Sammlung::aus_zugaengen(vec![zugang("site", &[])]);
        assert!(matches!(ohne_feld, Err(SammelFehler::Site(_))));
    }

    #[test]
    fn ohne_umgebung_ist_die_sammlung_leer() {
        let leer = Sammlung::mit_umgebung(|_| None).unwrap();
        assert!(leer.ist_leer());
    }
}
