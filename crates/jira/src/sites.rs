// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Eine Jira-Site, wie der User sie im quiver-Fenster pflegt (Aufgabe 13).
//!
//! quiver übergibt **alle** Sites als JSON in `QUIVER_ZUGAENGE`
//! (`quiver_domain::Mehrfach`): Name plus Werte der Felder `url`, `email`,
//! `api_token`, `nur` und `schreiben`. Dieses Modul macht daraus geprüfte
//! [`Site`]en und wählt die richtige aus.
//!
//! Zwei Sicherheitsregeln stehen hier:
//!
//! - Die Adresse muss `https://` sein. Nur Loopback (`127.0.0.1`, `localhost`,
//!   `[::1]`) darf `http://` sein – das ist die Attrappe im Test. Geprüft wird
//!   **nur** über [`url::Url`] (Schema, Host, Port), nie über eine eigene
//!   Zerlegung: Adressen mit Benutzer/Passwort (`user@host`), Abfrage oder
//!   Fragment werden abgelehnt.
//! - E-Mail und Token bleiben im Speicher, erscheinen in keiner Antwort und in
//!   keinem `Debug`; dort steht nur, *ob* sie gesetzt sind.

use std::collections::BTreeMap;

use quiver_tools_kern::zugaenge::{self, ZugaengeFehler};
use url::Url;

/// Feldname der Site-Adresse (Pflicht), z. B. `https://firma.atlassian.net`.
pub const URL: &str = "url";
/// Feldname der E-Mail des Atlassian-Kontos (Pflicht, Teil der Basic-Anmeldung).
pub const EMAIL: &str = "email";
/// Feldname des API-Tokens (geheim, Pflicht).
pub const TOKEN: &str = "api_token";
/// Feldname der erlaubten Projektschlüssel (optional, kommagetrennt).
pub const NUR: &str = "nur";
/// Feldname des Schalters „Schreiben erlauben“ (Standard **aus**).
pub const SCHREIBEN: &str = "schreiben";
/// Feldname des Schalters „Unverschlüsselt erlauben“ (Standard **aus**).
pub const UNVERSCHLUESSELT: &str = quiver_tools_kern::net::FELD_UNVERSCHLUESSELT;
/// Feldname des eigenen CA-Zertifikats (Pfad einer Datei).
pub const CA_ZERTIFIKAT: &str = quiver_tools_kern::net::FELD_CA_ZERTIFIKAT;

/// Die Adresse des Jira-REST-API, relativ zur Site.
pub const API: &str = "/rest/api/3";

/// Eine Jira-Site mit allen Werten. `Debug` zeigt weder E-Mail noch Token.
#[derive(Clone, PartialEq, Eq)]
pub struct Site {
    /// Name aus dem quiver-Fenster, z. B. `firma`.
    pub name: String,
    /// Basisadresse ohne Schrägstrich am Ende.
    pub url: String,
    /// E-Mail des Atlassian-Kontos; nur in der Basic-Kopfzeile.
    pub email: String,
    /// API-Token; nur in der Basic-Kopfzeile.
    pub token: String,
    /// Erlaubte Projektschlüssel (Feld `nur`); leer heißt „alle“.
    pub nur: Vec<String>,
    /// Erlaubt die verändernden Werkzeuge. Standard **aus**.
    pub schreiben: bool,
    /// Eigenes CA-Zertifikat (Pfad einer PEM-Datei), falls eingetragen.
    pub ca_zertifikat: Option<String>,
    /// Die geprüfte Adresse – Grundlage für den Vergleich von Schema, Host
    /// und Port (einer Folgeseite).
    adresse: Url,
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
            .field("ca_zertifikat", &self.ca_zertifikat.is_some())
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
        let unverschluesselt = schalter_aus(zugang.wert(UNVERSCHLUESSELT));
        let (url, adresse) = pruefe_url(&name, &url, unverschluesselt)?;

        Ok(Site {
            name,
            url,
            email,
            token,
            nur: erlaubte_projekte(zugang.wert(NUR)),
            schreiben: schalter_aus(zugang.wert(SCHREIBEN)),
            ca_zertifikat: zugang.wert(CA_ZERTIFIKAT).map(str::to_string),
            adresse,
        })
    }

    /// Erlaubt diese Site die verändernden Werkzeuge?
    pub fn schreiben_erlaubt(&self) -> bool {
        self.schreiben
    }

    /// Ist dieser Projektschlüssel erlaubt (Feld `nur`, ohne Groß-/Kleinschreibung)?
    pub fn erlaubt_projekt(&self, projekt: &str) -> bool {
        let projekt = projekt.trim();
        self.nur.is_empty()
            || self
                .nur
                .iter()
                .any(|erlaubt| erlaubt.eq_ignore_ascii_case(projekt))
    }

    /// Die erlaubten Projekte als Text (`alle` oder `ABC, DEF`).
    pub fn nur_text(&self) -> String {
        if self.nur.is_empty() {
            "alle".to_string()
        } else {
            self.nur.join(", ")
        }
    }

    /// Ist das dieselbe Adresse wie diese Site? (Für eine Folgeseite.)
    ///
    /// Verglichen werden Schema, Host und Port – alle drei am geparsten
    /// [`Url`], nicht an einer eigenen Zerlegung.
    pub fn gleicher_host(&self, url: &Url) -> bool {
        self.adresse.scheme() == url.scheme()
            && self.adresse.host() == url.host()
            && self.adresse.port_or_known_default() == url.port_or_known_default()
    }

    /// Die Basis des REST-API, z. B. `https://firma.atlassian.net/rest/api/3`.
    pub fn basis(&self) -> String {
        format!("{}{API}", self.url)
    }

    /// Die Adresse eines Vorgangs im Browser.
    pub fn vorgang_url(&self, key: &str) -> String {
        format!("{}/browse/{}", self.url, key)
    }

    /// `firma · https://firma.atlassian.net · nur ABC · Schreiben aus` – ohne
    /// E-Mail und Token.
    pub fn beschreibung(&self) -> String {
        format!(
            "{} · {} · nur {} · Schreiben {}",
            self.name,
            self.url,
            self.nur_text(),
            if self.schreiben { "an" } else { "aus" }
        ) + if self.url.starts_with("http://") {
            " · ohne Verschlüsselung (http)"
        } else {
            ""
        }
    }
}

/// Die Adresse prüfen: nur `https://`, Loopback darf `http://` sein.
///
/// Liefert die Adresse ohne Schrägstrich am Ende und dasselbe als [`Url`].
/// Alles wird an der geparsten `Url` entschieden – `user@host`, Abfrage und
/// Fragment werden abgelehnt.
fn pruefe_url(name: &str, roh: &str, unverschluesselt: bool) -> Result<(String, Url), SiteFehler> {
    let roh = roh.trim().trim_end_matches('/');
    let fehler = |grund: &'static str| SiteFehler::Url {
        name: name.to_string(),
        url: roh.to_string(),
        grund,
    };
    let url = Url::parse(roh).map_err(|_| fehler("keine gültige Adresse"))?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(fehler("Zugangsdaten gehören nicht in die Adresse"));
    }
    if url.query().is_some() {
        return Err(fehler("eine Basisadresse hat keine Abfrage"));
    }
    if url.fragment().is_some() {
        return Err(fehler("eine Basisadresse hat kein Fragment"));
    }
    if url.host().is_none() {
        return Err(fehler("die Adresse hat keinen Host"));
    }
    // Eine Regel für alle Tools (kern::net): https immer, http nur auf dem
    // Loopback oder mit eingeschaltetem Feld „Unverschlüsselt erlauben“.
    if quiver_tools_kern::net::adresse_erlaubt(roh, unverschluesselt).is_err() {
        return Err(fehler(
            "die Adresse muss mit https:// beginnen (http nur auf 127.0.0.1, localhost und \
             [::1] oder mit dem Feld „Unverschlüsselt erlauben“)",
        ));
    }
    Ok((roh.to_string(), url))
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

/// Das Feld `nur` als Liste: kommagetrennt, ohne Leerraum, ohne Leereinträge.
pub fn erlaubte_projekte(wert: Option<&str>) -> Vec<String> {
    wert.unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|projekt| !projekt.is_empty())
        .map(str::to_string)
        .collect()
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
            if namen.insert(site.name.to_ascii_lowercase(), ()).is_some() {
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
    #[error("Bei der Site „{name}“ taugt die Adresse „{url}“ nicht: {grund}.")]
    Url {
        name: String,
        url: String,
        grund: &'static str,
    },
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
    #[error("Es ist keine Jira-Site eingerichtet.")]
    Keine,
    #[error(
        "Es gibt mehrere Sites ({}) – bitte `verbindung` angeben, z. B. „{}“.",
        .0.join(", "),
        .0.first().cloned().unwrap_or_default()
    )]
    Mehrdeutig(Vec<String>),
    #[error("Die Site „{name}“ gibt es nicht – bekannt sind: {}.", bekannt.join(", "))]
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

    fn site(name: &str, url: &str) -> zugaenge::Zugang {
        zugang(
            name,
            &[
                ("url", url),
                ("email", "patrick@example.test"),
                ("api_token", "token-4711"),
            ],
        )
    }

    #[test]
    fn eine_site_wird_aus_den_feldern_gebaut() {
        let site = Site::aus_zugang(&zugang(
            "firma",
            &[
                ("url", "https://firma.atlassian.net/"),
                ("email", "patrick@example.test"),
                ("api_token", "token-4711"),
                ("nur", " ABC , DEF ,, "),
                ("schreiben", "an"),
            ],
        ))
        .unwrap();
        assert_eq!(site.name, "firma");
        assert_eq!(site.url, "https://firma.atlassian.net");
        assert_eq!(site.nur, vec!["ABC".to_string(), "DEF".to_string()]);
        assert!(site.schreiben_erlaubt());
        assert_eq!(site.basis(), "https://firma.atlassian.net/rest/api/3");
        assert_eq!(
            site.vorgang_url("ABC-1"),
            "https://firma.atlassian.net/browse/ABC-1"
        );
        assert!(site.erlaubt_projekt("abc"));
        assert!(!site.erlaubt_projekt("XYZ"));
        assert_eq!(site.nur_text(), "ABC, DEF");
    }

    #[test]
    fn ohne_nur_sind_alle_projekte_erlaubt() {
        let site = Site::aus_zugang(&site("firma", "https://firma.atlassian.net")).unwrap();
        assert!(site.erlaubt_projekt("egal"));
        assert_eq!(site.nur_text(), "alle");
        assert!(!site.schreiben_erlaubt());
    }

    #[test]
    fn nur_https_ausser_loopback() {
        assert!(Site::aus_zugang(&site("a", "https://firma.atlassian.net")).is_ok());
        assert!(Site::aus_zugang(&site("a", "http://127.0.0.1:8080")).is_ok());
        assert!(Site::aus_zugang(&site("a", "http://localhost:8080")).is_ok());
        assert!(Site::aus_zugang(&site("a", "http://[::1]:8080")).is_ok());
        assert!(Site::aus_zugang(&site("a", "https://[::1]:8443")).is_ok());

        for falsch in [
            "http://firma.atlassian.net",
            "ftp://firma.atlassian.net",
            "firma.atlassian.net",
            "https://user:geheim@firma.atlassian.net",
            "https://firma.atlassian.net/?x=1",
            "https://firma.atlassian.net/#x",
        ] {
            let fehler = Site::aus_zugang(&site("a", falsch)).unwrap_err();
            assert!(
                matches!(fehler, SiteFehler::Url { .. } | SiteFehler::Feld { .. }),
                "{falsch}: {fehler}"
            );
        }
    }

    #[test]
    fn die_adresse_wird_nur_ueber_url_geprueft() {
        // Ein `\` trennt in einer http-Adresse den Host ab: der Host ist
        // `evil.test`, nicht `127.0.0.1`. Eine eigene Zerlegung am `@` würde
        // hier fälschlich Loopback sehen und `http://` erlauben.
        for falsch in [
            "http://evil.test\\@127.0.0.1/",
            "http://127.0.0.1@evil.test/",
            "http://localhost@evil.test/",
            "http://127.0.0.1.evil.test/",
            "https://127.0.0.1@evil.test/",
        ] {
            let fehler = Site::aus_zugang(&site("a", falsch)).unwrap_err();
            assert!(
                matches!(fehler, SiteFehler::Url { .. }),
                "{falsch}: {fehler}"
            );
        }
        // Der Grund ist der Host, nicht das Schema: `127.0.0.1` ist gar nicht
        // der Host dieser Adresse.
        let fehler = Site::aus_zugang(&site("a", "http://evil.test\\@127.0.0.1/"))
            .unwrap_err()
            .to_string();
        assert!(fehler.contains("https://"), "{fehler}");
    }

    #[test]
    fn der_host_vergleich_geht_ueber_url_mit_port_und_schema() {
        let geprueft = Site::aus_zugang(&site("a", "https://firma.atlassian.net")).unwrap();
        let pruefe = |adresse: &str| geprueft.gleicher_host(&Url::parse(adresse).unwrap());
        assert!(pruefe(
            "https://firma.atlassian.net/rest/api/3/project/search"
        ));
        assert!(pruefe("https://FIRMA.atlassian.net:443/rest/api/3"));
        assert!(!pruefe("http://firma.atlassian.net/rest/api/3"));
        assert!(!pruefe("https://firma.atlassian.net:8443/rest/api/3"));
        assert!(!pruefe("https://firma.atlassian.net.evil.test/rest/api/3"));

        let lokal = Site::aus_zugang(&site("a", "http://127.0.0.1:8080")).unwrap();
        assert!(lokal.gleicher_host(&Url::parse("http://127.0.0.1:8080/x").unwrap()));
        assert!(!lokal.gleicher_host(&Url::parse("http://127.0.0.1:8081/x").unwrap()));
        assert!(!lokal.gleicher_host(&Url::parse("https://127.0.0.1:8080/x").unwrap()));
    }

    #[test]
    fn fehlende_pflichtfelder_werden_genannt() {
        let fehler = Site::aus_zugang(&zugang("a", &[("url", "https://x.test")])).unwrap_err();
        assert_eq!(
            fehler,
            SiteFehler::Feld {
                name: "a".into(),
                feld: "email"
            }
        );
        let fehler = Site::aus_zugang(&zugang("", &[])).unwrap_err();
        assert_eq!(fehler, SiteFehler::Name);
    }

    #[test]
    fn debug_zeigt_weder_token_noch_email_adresse() {
        let site = Site::aus_zugang(&site("firma", "https://firma.atlassian.net")).unwrap();
        let text = format!("{site:?}");
        assert!(!text.contains("token-4711"), "{text}");
        assert!(!text.contains("patrick@example.test"), "{text}");
        assert!(text.contains("firma"), "{text}");
        assert!(text.contains("token: true"), "{text}");
    }

    #[test]
    fn mehrere_sites_verlangen_einen_namen() {
        let sammlung = Sammlung::aus_zugaengen(vec![
            site("firma", "https://firma.atlassian.net"),
            site("kunde", "https://kunde.atlassian.net"),
        ])
        .unwrap();
        assert_eq!(sammlung.namen(), vec!["firma", "kunde"]);
        assert!(sammlung.waehlen(Some("FIRMA")).is_ok());
        assert!(matches!(
            sammlung.waehlen(None),
            Err(AuswahlFehler::Mehrdeutig(_))
        ));
        let fehler = sammlung.waehlen(Some("gibtsnicht")).unwrap_err();
        assert!(fehler.to_string().contains("gibtsnicht"));
        assert!(fehler.to_string().contains("firma"));
    }

    #[test]
    fn eine_einzige_site_darf_ohne_namen_gewaehlt_werden() {
        let sammlung =
            Sammlung::aus_zugaengen(vec![site("firma", "https://firma.atlassian.net")]).unwrap();
        assert_eq!(sammlung.waehlen(None).unwrap().name, "firma");
        assert!(!sammlung.ist_leer());
        assert!(Sammlung::default().ist_leer());
        assert!(matches!(
            Sammlung::default().waehlen(None),
            Err(AuswahlFehler::Keine)
        ));
    }

    #[test]
    fn doppelte_namen_und_kaputtes_json_sind_fehler_ohne_inhalt() {
        let fehler = Sammlung::aus_zugaengen(vec![
            site("firma", "https://firma.atlassian.net"),
            site("Firma", "https://andere.atlassian.net"),
        ])
        .unwrap_err();
        assert!(matches!(fehler, SammelFehler::Doppelt(_)));

        let fehler = Sammlung::mit_umgebung(|_| Some("[{\"name\":".to_string())).unwrap_err();
        let text = fehler.to_string();
        assert!(!text.contains("token"), "{text}");
    }

    #[test]
    fn der_schalter_ist_im_zweifel_aus() {
        assert!(!schalter_aus(None));
        assert!(!schalter_aus(Some("")));
        assert!(!schalter_aus(Some("nein")));
        assert!(schalter_aus(Some("an")));
        assert!(schalter_aus(Some(" true ")));
    }

    #[test]
    fn die_beschreibung_nennt_kein_geheimnis() {
        let site = Site::aus_zugang(&site("firma", "https://firma.atlassian.net")).unwrap();
        let text = site.beschreibung();
        assert!(text.contains("firma"), "{text}");
        assert!(text.contains("Schreiben aus"), "{text}");
        assert!(!text.contains("token-4711"), "{text}");
        assert!(!text.contains("patrick@example.test"), "{text}");
    }
}
