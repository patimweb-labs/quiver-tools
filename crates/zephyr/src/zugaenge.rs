// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Ein Zephyr-Zugang, wie der User ihn im quiver-Fenster pflegt (Aufgabe 15).
//!
//! quiver übergibt **alle** Zugänge als JSON in `QUIVER_ZUGAENGE`
//! (`quiver_domain::Mehrfach`): Name plus Werte der Felder `basis_url`,
//! `api_token`, `projekt`, `nur` und `schreiben`. Dieses Modul macht daraus
//! geprüfte [`Zugang`]en und wählt den richtigen aus. Das Token bleibt im
//! Speicher und erscheint in keiner Antwort – `Debug` zeigt nur, *ob* eines
//! gesetzt ist.
//!
//! Zwei Regeln aus [12-atlassian-gemeinsam.md](../../docs/aufgaben/12-atlassian-gemeinsam.md)
//! stehen hier, **bevor** irgendein Aufruf entsteht: die Adresse ist `https://`
//! (Ausnahme: Loopback für die Attrappe), und ein Token ist Pflicht.

use std::collections::BTreeMap;

use quiver_tools_kern::zugaenge::{self, ZugaengeFehler};
use url::Url;

/// Feldname der Basisadresse (Standard: Zephyr Scale Cloud).
pub const BASIS_URL: &str = "basis_url";
/// Feldname des API-Tokens (geheim, Pflicht).
pub const TOKEN: &str = "api_token";
/// Feldname des Standardprojekts (optional); leer heißt „je Aufruf angeben“.
pub const PROJEKT: &str = "projekt";
/// Feldname der erlaubten Projektschlüssel, kommagetrennt (optional).
pub const NUR: &str = "nur";
/// Feldname des Schalters „Unverschlüsselt erlauben“ (Standard **aus**).
pub const UNVERSCHLUESSELT: &str = quiver_tools_kern::net::FELD_UNVERSCHLUESSELT;
/// Feldname des eigenen CA-Zertifikats (Pfad einer Datei).
pub const CA_ZERTIFIKAT: &str = quiver_tools_kern::net::FELD_CA_ZERTIFIKAT;
/// Feldname des Schalters „Schreiben erlauben“ (Standard **aus**).
pub const SCHREIBEN: &str = "schreiben";

/// Die Adresse der öffentlichen Zephyr-Scale-Cloud (REST v2).
pub const STANDARD_BASIS_URL: &str = "https://api.zephyrscale.smartbear.com/v2";

/// Ein Zugang mit allen Werten. `Debug` zeigt das Token nie.
#[derive(Clone, PartialEq, Eq)]
pub struct Zugang {
    /// Name aus dem quiver-Fenster, z. B. `kunde`.
    pub name: String,
    /// Basisadresse ohne Schrägstrich am Ende, z. B.
    /// `https://api.zephyrscale.smartbear.com/v2`.
    pub basis_url: String,
    /// API-Token. Steht nur im Speicher und nur in der Kopfzeile.
    pub token: String,
    /// Standardprojekt; `None` heißt, das Projekt kommt mit dem Aufruf.
    pub projekt: Option<String>,
    /// Erlaubte Projektschlüssel; leer heißt „alle“.
    pub nur: Vec<String>,
    /// Erlaubt die verändernden Werkzeuge. Standard **aus**.
    pub schreiben: bool,
    /// Eigenes CA-Zertifikat (Pfad einer PEM-Datei), falls eingetragen.
    pub ca_zertifikat: Option<String>,
}

impl std::fmt::Debug for Zugang {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Zugang")
            .field("name", &self.name)
            .field("basis_url", &self.basis_url)
            .field("projekt", &self.projekt)
            .field("nur", &self.nur)
            .field("token", &(!self.token.is_empty()))
            .field("schreiben", &self.schreiben)
            .finish()
    }
}

impl Zugang {
    /// Ein Zugang aus einem Eintrag der Liste.
    pub fn aus_zugang(zugang: &zugaenge::Zugang) -> Result<Zugang, ZugangsFehler> {
        let name = zugang.name.trim().to_string();
        if name.is_empty() {
            return Err(ZugangsFehler::Name);
        }

        let token = zugang
            .wert(TOKEN)
            .map(str::to_string)
            .ok_or_else(|| ZugangsFehler::Feld {
                name: name.clone(),
                feld: TOKEN,
            })?;

        // Ohne eigene Adresse gilt die öffentliche Cloud (Feld `standard`).
        let roh = zugang.wert(BASIS_URL).unwrap_or(STANDARD_BASIS_URL);
        let basis_url = adresse_pruefen(&name, roh, schalter_aus(zugang.wert(UNVERSCHLUESSELT)))?;

        Ok(Zugang {
            name,
            basis_url,
            token,
            projekt: zugang
                .wert(PROJEKT)
                .map(str::trim)
                .filter(|projekt| !projekt.is_empty())
                .map(str::to_string),
            nur: nur_lesen(zugang.wert(NUR)),
            schreiben: schalter_aus(zugang.wert(SCHREIBEN)),
            ca_zertifikat: zugang.wert(CA_ZERTIFIKAT).map(str::to_string),
        })
    }

    /// Erlaubt dieser Zugang die verändernden Werkzeuge?
    pub fn schreiben_erlaubt(&self) -> bool {
        self.schreiben
    }

    /// Ist das Projekt zum Schreiben freigegeben?
    ///
    /// Ohne `nur` ist alles erlaubt. Der Vergleich ist ohne Groß-/Kleinschreibung,
    /// weil Zephyr Schlüssel in Antworten groß schreibt.
    pub fn projekt_erlaubt(&self, projekt: &str) -> bool {
        self.nur.is_empty()
            || self
                .nur
                .iter()
                .any(|erlaubt| erlaubt.eq_ignore_ascii_case(projekt.trim()))
    }

    /// Das Projekt dieses Aufrufs: das genannte, sonst das des Zugangs.
    pub fn projekt(&self, genannt: Option<&str>) -> Option<String> {
        genannt
            .map(str::trim)
            .filter(|projekt| !projekt.is_empty())
            .map(str::to_string)
            .or_else(|| self.projekt.clone())
    }

    /// `kunde · https://… · Projekt QA · Schreiben aus · Nur: QA, OPS` – ohne Token.
    pub fn beschreibung(&self) -> String {
        let mut text = format!(
            "{} · {} · Projekt {} · Schreiben {}",
            self.name,
            self.basis_url,
            self.projekt.as_deref().unwrap_or("(je Aufruf)"),
            if self.schreiben { "an" } else { "aus" }
        );
        if !self.nur.is_empty() {
            text.push_str(&format!(" · Nur: {}", self.nur.join(", ")));
        }
        if self.basis_url.starts_with("http://") {
            text.push_str(" · ohne Verschlüsselung (http)");
        }
        text
    }
}

/// Eine Adresse prüfen: nur `https://`, Ausnahme Loopback für die Attrappe.
///
/// Geprüft wird über [`url::Url`] – Schema, Host, Port, Benutzer und Abfrage
/// werden von dort gelesen, nicht selbst zerlegt. Abgelehnt werden Adressen mit
/// Benutzer oder Passwort (`user@host`), mit Query oder Fragment und alles, was
/// kein `https://` ist. Nur für die Attrappe ist `http://` auf Loopback erlaubt.
///
/// Liefert die Adresse ohne Schrägstrich am Ende zurück.
pub fn adresse_pruefen(
    name: &str,
    roh: &str,
    unverschluesselt: bool,
) -> Result<String, ZugangsFehler> {
    let roh = roh.trim();
    let fehler = || ZugangsFehler::Url {
        name: name.to_string(),
        url: roh.to_string(),
    };
    let url = Url::parse(roh).map_err(|_| fehler())?;

    // `https://127.0.0.1@evil.test/` sieht wie Loopback aus, ist aber ein
    // fremder Host; `https://evil.test\@127.0.0.1/` zerlegt der Parser anders.
    // Beides ist keine Adresse, die ein Token bekommen darf.
    if !url.username().is_empty() || url.password().is_some() {
        return Err(fehler());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(fehler());
    }

    // Eine Regel für alle Tools (kern::net): https immer, http nur auf dem
    // Loopback oder mit eingeschaltetem Feld „Unverschlüsselt erlauben“.
    if quiver_tools_kern::net::adresse_erlaubt(roh, unverschluesselt).is_err() {
        return Err(fehler());
    }
    Ok(roh.trim_end_matches('/').to_string())
}

/// Der Host einer Adresse, klein geschrieben; `None`, wenn keiner dasteht.
///
/// `https://api.example.test/v2` → `api.example.test`,
/// `http://[::1]:8080/v2` → `[::1]`.
pub fn host(url: &str) -> Option<String> {
    Url::parse(url)
        .ok()?
        .host_str()
        .map(str::to_ascii_lowercase)
}

/// Der Ursprung einer Adresse: Schema, Host und Port. `https` und Port 443
/// gelten als derselbe Ursprung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Herkunft {
    pub schema: String,
    pub host: String,
    pub port: Option<u16>,
}

impl std::fmt::Display for Herkunft {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.port {
            Some(port) => write!(f, "{}://{}:{port}", self.schema, self.host),
            None => write!(f, "{}://{}", self.schema, self.host),
        }
    }
}

/// Der Ursprung einer Adresse; `None`, wenn sie sich nicht zerlegen lässt.
pub fn herkunft(url: &str) -> Option<Herkunft> {
    let url = Url::parse(url).ok()?;
    Some(Herkunft {
        schema: url.scheme().to_ascii_lowercase(),
        host: url.host_str()?.to_ascii_lowercase(),
        port: url.port_or_known_default(),
    })
}

/// Ist der Host die eigene Maschine? (Nur dafür ist `http://` erlaubt.)
pub fn ist_loopback(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "[::1]")
}

/// Die Liste der erlaubten Projektschlüssel aus dem Feld `nur`.
pub fn nur_lesen(wert: Option<&str>) -> Vec<String> {
    wert.unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|teil| !teil.is_empty())
        .map(str::to_string)
        .collect()
}

/// Ein Schalter, der im Zweifel **aus** ist (Feld `schreiben`, Aufgabe 15).
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

/// Alle Zugänge, in der Reihenfolge der Liste.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sammlung {
    zugaenge: Vec<Zugang>,
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
        let mut zugaenge = Vec::with_capacity(liste.len());
        let mut namen = BTreeMap::new();
        for eintrag in &liste {
            let zugang = Zugang::aus_zugang(eintrag)?;
            if namen.insert(zugang.name.clone(), ()).is_some() {
                return Err(SammelFehler::Doppelt(zugang.name));
            }
            zugaenge.push(zugang);
        }
        Ok(Sammlung { zugaenge })
    }

    pub fn alle(&self) -> &[Zugang] {
        &self.zugaenge
    }

    pub fn ist_leer(&self) -> bool {
        self.zugaenge.is_empty()
    }

    pub fn namen(&self) -> Vec<&str> {
        self.zugaenge
            .iter()
            .map(|zugang| zugang.name.as_str())
            .collect()
    }

    /// Den gewünschten Zugang wählen.
    ///
    /// Ohne Namen gilt der einzige; bei mehreren muss der Agent wählen. Der
    /// Name wird ohne Groß-/Kleinschreibung verglichen.
    pub fn waehlen(&self, name: Option<&str>) -> Result<&Zugang, AuswahlFehler> {
        match name.map(str::trim).filter(|name| !name.is_empty()) {
            Some(name) => self
                .zugaenge
                .iter()
                .find(|zugang| zugang.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| AuswahlFehler::Unbekannt {
                    name: name.to_string(),
                    bekannt: self.namen().iter().map(|n| n.to_string()).collect(),
                }),
            None => match self.zugaenge.as_slice() {
                [] => Err(AuswahlFehler::Keine),
                [einzige] => Ok(einzige),
                mehrere => Err(AuswahlFehler::Mehrdeutig(
                    mehrere.iter().map(|zugang| zugang.name.clone()).collect(),
                )),
            },
        }
    }
}

/// Was an den Werten eines Zugangs nicht stimmt.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ZugangsFehler {
    #[error("Ein Zugang im quiver-Fenster hat keinen Namen – bitte einen Namen vergeben.")]
    Name,
    #[error("Beim Zugang „{name}“ fehlt das Feld „{feld}“ – bitte im quiver-Fenster eintragen.")]
    Feld { name: String, feld: &'static str },
    #[error(
        "Beim Zugang „{name}“ ist die Adresse „{url}“ keine Zephyr-Adresse – sie muss mit \
         https:// beginnen (http:// nur für 127.0.0.1, localhost oder [::1]) und darf weder \
         Benutzer noch Passwort, Abfrage (`?`) oder Fragment (`#`) enthalten."
    )]
    Url { name: String, url: String },
}

/// Warum die Liste nicht gelesen werden konnte.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SammelFehler {
    #[error(transparent)]
    Zugaenge(#[from] ZugaengeFehler),
    #[error(transparent)]
    Zugang(#[from] ZugangsFehler),
    #[error("Den Zugang „{0}“ gibt es mehrfach – bitte im quiver-Fenster einen Namen ändern.")]
    Doppelt(String),
}

/// Warum kein Zugang gewählt werden konnte.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuswahlFehler {
    #[error("Es ist kein Zephyr-Zugang eingerichtet.")]
    Keine,
    #[error(
        "Es gibt mehrere Zugänge ({}) – bitte `verbindung` angeben.",
        .0.join(", ")
    )]
    Mehrdeutig(Vec<String>),
    #[error(
        "Den Zugang „{}“ gibt es nicht. Bekannt sind: {}.",
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
                ("basis_url", "https://api.zephyrscale.smartbear.com/v2/"),
                ("projekt", " QA "),
                ("api_token", "token-4711"),
            ],
        )
    }

    #[test]
    fn ein_zugang_wird_geprueft_und_hat_vorgaben() {
        let zugang = Zugang::aus_zugang(&vollstaendig("kunde")).unwrap();
        assert_eq!(zugang.name, "kunde");
        // Der Schrägstrich am Ende fliegt raus, das Projekt wird getrimmt.
        assert_eq!(zugang.basis_url, "https://api.zephyrscale.smartbear.com/v2");
        assert_eq!(zugang.projekt.as_deref(), Some("QA"));
        // Ohne Feld ist das Schreiben aus – der sichere Zustand.
        assert!(!zugang.schreiben);
        assert!(!zugang.schreiben_erlaubt());
        assert!(zugang.nur.is_empty());
        assert_eq!(
            zugang.beschreibung(),
            "kunde · https://api.zephyrscale.smartbear.com/v2 · Projekt QA · Schreiben aus"
        );
        assert_eq!(zugang.projekt(None).as_deref(), Some("QA"));
        assert_eq!(zugang.projekt(Some(" OPS ")).as_deref(), Some("OPS"));
        assert_eq!(zugang.projekt(Some("   ")).as_deref(), Some("QA"));
    }

    #[test]
    fn ohne_adresse_gilt_die_cloud() {
        let zugang = Zugang::aus_zugang(&zugang("kunde", &[("api_token", "token-4711")])).unwrap();
        assert_eq!(zugang.basis_url, STANDARD_BASIS_URL);
        assert_eq!(zugang.projekt, None);
        assert_eq!(
            zugang.beschreibung(),
            "kunde · https://api.zephyrscale.smartbear.com/v2 · Projekt (je Aufruf) · \
             Schreiben aus"
        );
    }

    #[test]
    fn nur_wird_kommagetrennt_gelesen_und_ohne_gross_kleinschreibung_verglichen() {
        let zugang = Zugang::aus_zugang(&zugang(
            "kunde",
            &[("api_token", "t"), ("nur", " QA , ops ,, ")],
        ))
        .unwrap();
        assert_eq!(zugang.nur, vec!["QA", "ops"]);
        assert!(zugang.projekt_erlaubt("qa"));
        assert!(zugang.projekt_erlaubt("OPS"));
        assert!(!zugang.projekt_erlaubt("ALT"));
        assert!(zugang.beschreibung().contains("Nur: QA, ops"));

        // Ohne `nur` ist alles erlaubt.
        let offen = Zugang::aus_zugang(&vollstaendig("offen")).unwrap();
        assert!(offen.projekt_erlaubt("irgendwas"));
    }

    #[test]
    fn schreiben_ist_nur_ausdruecklich_an() {
        let mit = |wert: &str| {
            Zugang::aus_zugang(&zugang("z", &[("api_token", "t"), ("schreiben", wert)]))
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
    }

    #[test]
    fn debug_zeigt_das_token_nie() {
        let zugang = Zugang::aus_zugang(&vollstaendig("kunde")).unwrap();
        let text = format!("{zugang:?}");
        assert!(!text.contains("token-4711"), "{text}");
        assert!(text.contains("token: true"), "{text}");
    }

    #[test]
    fn nur_https_und_loopback_sind_erlaubt() {
        assert_eq!(
            adresse_pruefen("k", "https://api.zephyrscale.smartbear.com/v2/", false).unwrap(),
            "https://api.zephyrscale.smartbear.com/v2"
        );
        // Die Attrappe läuft auf Loopback.
        assert_eq!(
            adresse_pruefen("k", "http://127.0.0.1:8080/v2", false).unwrap(),
            "http://127.0.0.1:8080/v2"
        );
        assert!(adresse_pruefen("k", "http://localhost:8080", false).is_ok());
        assert!(adresse_pruefen("k", "http://[::1]:8080/v2", false).is_ok());

        for falsch in [
            "http://api.zephyrscale.smartbear.com/v2",
            "ftp://127.0.0.1/v2",
            "api.zephyrscale.smartbear.com/v2",
            "https://",
            // Benutzer, Passwort, Abfrage und Fragment sind keine Basisadresse.
            "https://benutzer@api.example.test/v2",
            "https://benutzer:geheim@api.example.test/v2",
            "https://api.example.test/v2?projekt=QA",
            "https://api.example.test/v2#teil",
            // Sieht nach Loopback aus, ist aber ein fremder Host.
            "https://127.0.0.1@evil.test/v2",
            "http://evil.test\\@127.0.0.1/",
        ] {
            assert!(
                adresse_pruefen("k", falsch, false).is_err(),
                "{falsch} sollte abgelehnt werden"
            );
        }
    }

    #[test]
    fn der_host_und_der_ursprung_kommen_aus_url() {
        assert_eq!(
            host("https://api.zephyrscale.smartbear.com/v2").as_deref(),
            Some("api.zephyrscale.smartbear.com")
        );
        assert_eq!(
            host("http://127.0.0.1:8080/v2?x=1").as_deref(),
            Some("127.0.0.1")
        );
        assert_eq!(host("http://[::1]:8080/v2").as_deref(), Some("[::1]"));
        assert_eq!(host("kein-schema"), None);
        assert_eq!(host("mailto:test@example.test"), None);

        // Der Ursprung enthält Schema und Port: eine Folgeseite auf einem
        // anderen Port ist ein anderer Host.
        assert_eq!(
            herkunft("https://api.example.test/v2").unwrap().to_string(),
            "https://api.example.test:443"
        );
        assert_ne!(
            herkunft("http://127.0.0.1:8080/v2"),
            herkunft("http://127.0.0.1:9999/v2")
        );
        assert_ne!(
            herkunft("http://127.0.0.1:8080/v2"),
            herkunft("https://127.0.0.1:8080/v2")
        );
        assert_eq!(
            herkunft("http://127.0.0.1:8080/v2"),
            herkunft("http://127.0.0.1:8080/andere")
        );
        assert_eq!(herkunft("kein-schema"), None);
    }

    #[test]
    fn pflichtfelder_und_namen_werden_geprueft() {
        assert_eq!(
            Zugang::aus_zugang(&zugang("k", &[("basis_url", "https://x")])),
            Err(ZugangsFehler::Feld {
                name: "k".into(),
                feld: TOKEN
            })
        );
        assert_eq!(
            Zugang::aus_zugang(&zugang(
                "k",
                &[("api_token", "t"), ("basis_url", "http://example.test")]
            )),
            Err(ZugangsFehler::Url {
                name: "k".into(),
                url: "http://example.test".into()
            })
        );
        assert_eq!(
            Zugang::aus_zugang(&zugang("  ", &[("api_token", "t")])),
            Err(ZugangsFehler::Name)
        );
    }

    #[test]
    fn die_auswahl_ist_ohne_namen_eindeutig() {
        let eine = Sammlung::aus_zugaengen(vec![vollstaendig("kunde")]).unwrap();
        assert_eq!(eine.waehlen(None).unwrap().name, "kunde");
        assert_eq!(eine.waehlen(Some("KUNDE")).unwrap().name, "kunde");
        assert!(matches!(
            eine.waehlen(Some("gibtsnicht")),
            Err(AuswahlFehler::Unbekannt { .. })
        ));

        let zwei = Sammlung::aus_zugaengen(vec![
            vollstaendig("kunde"),
            zugang("ops", &[("api_token", "t2")]),
        ])
        .unwrap();
        assert_eq!(zwei.alle().len(), 2);
        assert_eq!(zwei.namen(), vec!["kunde", "ops"]);
        assert!(matches!(
            zwei.waehlen(None),
            Err(AuswahlFehler::Mehrdeutig(_))
        ));
        assert_eq!(zwei.waehlen(Some("OPS")).unwrap().projekt, None);

        let keine = Sammlung::default();
        assert!(keine.ist_leer());
        assert_eq!(keine.waehlen(None), Err(AuswahlFehler::Keine));
    }

    #[test]
    fn doppelte_namen_und_kaputtes_json_werden_gemeldet() {
        let doppelt = Sammlung::aus_zugaengen(vec![vollstaendig("z"), vollstaendig("z")]);
        assert_eq!(doppelt, Err(SammelFehler::Doppelt("z".into())));

        let kaputt = Sammlung::mit_umgebung(|_| Some("[{\"name\":".to_string()));
        assert!(matches!(kaputt, Err(SammelFehler::Zugaenge(_))));

        let ohne_feld = Sammlung::aus_zugaengen(vec![zugang("z", &[])]);
        assert!(matches!(ohne_feld, Err(SammelFehler::Zugang(_))));

        let leer = Sammlung::mit_umgebung(|_| None).unwrap();
        assert!(leer.ist_leer());
    }
}
