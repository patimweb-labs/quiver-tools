// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der eine Weg ins Netz (SPEC §2, Härtung Paket 4).
//!
//! Alle Programme benutzen denselben Client und dieselben Prüfungen; ein Tool
//! kann sie nicht „vergessen“:
//!
//! - **Keine Weiterleitung** ([`client`]): Antworten mit `3xx` werden zu einem
//!   Fehler, nie verfolgt. Zugangsdaten gehen damit nur an den eingetragenen
//!   Host.
//! - **Frist** je Aufruf (Standard [`ZEITLIMIT`]).
//! - **Grenze beim Lesen** ([`text_begrenzt`], [`MAX_BYTES`]): eine Antwort
//!   über 8 MB bricht ab, statt den Speicher zu füllen.
//! - **Adressen** ([`adresse_erlaubt`]): immer `https`. `http` nur auf dem
//!   Loopback **oder** wenn der User im quiver-Fenster das Feld
//!   „Unverschlüsselt erlauben“ (`unverschluesselt_erlauben`) einschaltet –
//!   Standard **aus**, mit Warnung in Status und Log ([`adresse_warnung`]).
//! - **Zertifikat** ([`zertifikat`]): abschaltbar nur auf dem Loopback oder
//!   mit einem eigenen CA-Zertifikat (Datei). Sonst bleibt die Prüfung an, und
//!   die Warnung nennt das Feld, das zu setzen ist.
//!
//! Die Adressen werden ausschließlich über [`url::Url`] beurteilt, nie über
//! eine eigene Zerlegung: `user@host`, Abfrage und Fragment sind keine
//! Werkzeugziele.

use std::path::PathBuf;
use std::time::Duration;

use reqwest::{Client, ClientBuilder, Response};
use url::{Host, Url};

/// Höchstgröße einer Antwort, die ein Werkzeug annimmt: 8 MB.
///
/// Mehr passt nicht sinnvoll in eine Antwort an ein Modell, und ein fehlerhafter
/// Endpunkt soll den Prozess nicht volllaufen lassen.
pub const MAX_BYTES: u64 = 8 * 1024 * 1024;

/// Wie lange ein einzelner Aufruf höchstens dauern darf.
pub const ZEITLIMIT: Duration = Duration::from_secs(60);

/// Feldname des Schalters „Unverschlüsselt erlauben“ im quiver-Fenster.
pub const FELD_UNVERSCHLUESSELT: &str = "unverschluesselt_erlauben";
/// Beschriftung dieses Schalters.
pub const TITEL_UNVERSCHLUESSELT: &str = "Unverschlüsselt erlauben";

/// Feldname des eigenen CA-Zertifikats (Pfad einer Datei).
pub const FELD_CA_ZERTIFIKAT: &str = "ca_zertifikat";
/// Beschriftung dieses Feldes.
pub const TITEL_CA_ZERTIFIKAT: &str = "CA-Zertifikat";

/// Was an einer Adresse nicht stimmt. Der Text nennt immer das Feld, das
/// weiterhilft – ein vorhandener Zugang fällt nicht still aus.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AdressFehler {
    #[error(
        "Die Adresse „{url}“ muss mit https:// beginnen. Ohne Verschlüsselung sind nur \
         127.0.0.1, localhost und [::1] erlaubt; für einen anderen Host bitte im \
         quiver-Fenster das Feld „{TITEL_UNVERSCHLUESSELT}“ einschalten (Standard: aus)."
    )]
    Unverschluesselt { url: String },
    #[error("Die Adresse „{url}“ ist keine gültige URL.")]
    Ungueltig { url: String },
    #[error(
        "Die Adresse „{url}“ enthält einen Benutzerteil, eine Abfrage oder ein Fragment – \
         das gehört nicht in eine Werkzeugadresse."
    )]
    Unsicher { url: String },
}

/// Was beim Lesen einer Antwort schiefgeht.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NetFehler {
    #[error("Die Antwort ist größer als {max_mb} MB und wird nicht gelesen.")]
    ZuGross { max_mb: u64 },
    #[error("Die Antwort ließ sich nicht lesen: {0}")]
    Lesen(String),
    #[error("Das CA-Zertifikat „{pfad}“ ließ sich nicht lesen: {grund}")]
    Ca { pfad: String, grund: String },
    #[error("Der HTTP-Client ließ sich nicht bauen: {0}")]
    Client(String),
}

/// Der gemeinsame HTTP-Client: keine Weiterleitung, feste Frist, eigener
/// User-Agent.
///
/// `anwendung` ist der Name im `User-Agent`, z. B. `quiver-outline/0.1.0`.
pub fn client(anwendung: &str) -> Result<Client, NetFehler> {
    bauen(anwendung)
        .build()
        .map_err(|fehler| NetFehler::Client(fehler.to_string()))
}

/// Der Client, bevor Zertifikat und Ähnliches gesetzt sind (Tests, Sonderfälle).
pub fn bauen(anwendung: &str) -> ClientBuilder {
    Client::builder()
        .timeout(ZEITLIMIT)
        .user_agent(anwendung.to_string())
        // Keine Weiterleitung: die Kopfzeile geht nur an den eingetragenen
        // Host, ein `3xx` wird gemeldet statt verfolgt.
        .redirect(reqwest::redirect::Policy::none())
}

/// Ist der Host die eigene Maschine? (Loopback der Attrappe im Test.)
pub fn ist_loopback_host(host: &str) -> bool {
    let host = host.trim().trim_start_matches('[').trim_end_matches(']');
    match host.parse::<std::net::IpAddr>() {
        Ok(ip) => ip.is_loopback(),
        Err(_) => host.eq_ignore_ascii_case("localhost"),
    }
}

fn host_ist_loopback(host: &Host<&str>) -> bool {
    match host {
        Host::Domain(domain) => domain.eq_ignore_ascii_case("localhost"),
        Host::Ipv4(adresse) => adresse.is_loopback(),
        Host::Ipv6(adresse) => adresse.is_loopback(),
    }
}

/// Darf diese Adresse als Werkzeugziel eingetragen werden?
///
/// **https** immer; `http` nur auf dem Loopback oder wenn
/// `unverschluesselt_erlauben` gesetzt ist. Ein Benutzerteil, eine Abfrage oder
/// ein Fragment sind nie erlaubt.
pub fn adresse_erlaubt(roh: &str, unverschluesselt_erlauben: bool) -> Result<Url, AdressFehler> {
    let roh = roh.trim();
    let url = Url::parse(roh).map_err(|_| AdressFehler::Ungueltig {
        url: roh.to_string(),
    })?;
    let unsicher = || AdressFehler::Unsicher {
        url: roh.to_string(),
    };
    if !url.username().is_empty() || url.password().is_some() {
        return Err(unsicher());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(unsicher());
    }
    let Some(host) = url.host() else {
        return Err(AdressFehler::Ungueltig {
            url: roh.to_string(),
        });
    };
    match url.scheme() {
        "https" => Ok(url),
        "http" => {
            if host_ist_loopback(&host) || unverschluesselt_erlauben {
                // Steht so im Log und im Status: ohne TLS ist es eine Warnung.
                adresse_warnung(&url);
                Ok(url)
            } else {
                Err(AdressFehler::Unverschluesselt {
                    url: roh.to_string(),
                })
            }
        }
        _ => Err(AdressFehler::Ungueltig {
            url: roh.to_string(),
        }),
    }
}

/// Der Hinweis zu einer unverschlüsselten Adresse – für den Status und das Log.
///
/// `None` heißt: die Adresse ist verschlüsselt und es gibt nichts zu sagen.
pub fn adresse_warnung(url: &Url) -> Option<String> {
    if url.scheme() != "http" {
        return None;
    }
    let text = format!(
        "Achtung: {} wird ohne Verschlüsselung angesprochen (Feld „{TITEL_UNVERSCHLUESSELT}“ ist an).",
        url.host_str().unwrap_or("der Host")
    );
    tracing::warn!("{text}");
    eprintln!("Hinweis: {text}");
    Some(text)
}

/// Wie das Serverzertifikat geprüft wird.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Zertifikat {
    /// Reguläre Prüfung gegen den Zertifikatsspeicher des Systems.
    Pruefen,
    /// Keine Prüfung – nur auf dem Loopback zulässig (Attrappe).
    AusLoopback,
    /// Prüfung gegen dieses eigene CA-Zertifikat (Datei, PEM).
    EigenesCa(PathBuf),
}

impl Zertifikat {
    /// Nimmt der Client jedes Zertifikat an?
    pub fn ungeprueft(&self) -> bool {
        matches!(self, Zertifikat::AusLoopback)
    }

    /// Setzt die Zertifikatsregel am Client.
    pub fn anwenden(&self, bauen: ClientBuilder) -> Result<ClientBuilder, NetFehler> {
        match self {
            Zertifikat::Pruefen => Ok(bauen),
            Zertifikat::AusLoopback => Ok(bauen.danger_accept_invalid_certs(true)),
            Zertifikat::EigenesCa(pfad) => {
                let pem = std::fs::read(pfad).map_err(|fehler| NetFehler::Ca {
                    pfad: pfad.display().to_string(),
                    grund: fehler.to_string(),
                })?;
                let zertifikat =
                    reqwest::Certificate::from_pem(&pem).map_err(|fehler| NetFehler::Ca {
                        pfad: pfad.display().to_string(),
                        grund: fehler.to_string(),
                    })?;
                Ok(bauen.add_root_certificate(zertifikat))
            }
        }
    }
}

/// Die Zertifikatsregel aus den Feldern des Zugangs.
///
/// Abschalten gilt **nur** auf dem Loopback; für jeden anderen Host bleibt die
/// Prüfung an, solange kein eigenes CA-Zertifikat eingetragen ist. Der zweite
/// Wert ist der Hinweis für Status und Log.
pub fn zertifikat(
    pruefen: bool,
    host: &str,
    ca_datei: Option<&str>,
) -> (Zertifikat, Option<String>) {
    // Ein eigenes CA-Zertifikat gilt immer; es ergänzt die Prüfung.
    let ca = ca_datei.map(str::trim).filter(|pfad| !pfad.is_empty());
    if let Some(pfad) = ca {
        return (Zertifikat::EigenesCa(PathBuf::from(pfad)), None);
    }
    if pruefen {
        return (Zertifikat::Pruefen, None);
    }
    if ist_loopback_host(host) {
        return (Zertifikat::AusLoopback, None);
    }
    let text = format!(
        "Die Zertifikatsprüfung bleibt an: sie lässt sich nur für 127.0.0.1, localhost und [::1] \
         abschalten. Für {host} bitte im quiver-Fenster das Feld „{TITEL_CA_ZERTIFIKAT}“ mit einer \
         Zertifikatsdatei füllen oder die Prüfung eingeschaltet lassen."
    );
    // Einmal je Prozess: das Log soll nicht bei jeder Verbindung wachsen.
    static EINMAL: std::sync::Once = std::sync::Once::new();
    EINMAL.call_once(|| {
        tracing::warn!("{text}");
        eprintln!("Hinweis: {text}");
    });
    (Zertifikat::Pruefen, Some(text))
}

/// Eine Antwort lesen – höchstens [`MAX_BYTES`], sonst Abbruch als Fehler.
///
/// Der Rumpf wird stückweise gelesen; eine `Content-Length` über der Grenze
/// erspart schon das Lesen. Ungültiges UTF-8 wird ersetzt, nicht abgelehnt.
pub async fn text_begrenzt(mut antwort: Response) -> Result<String, NetFehler> {
    let max_mb = MAX_BYTES / (1024 * 1024);
    if let Some(laenge) = antwort.content_length() {
        if laenge > MAX_BYTES {
            return Err(NetFehler::ZuGross { max_mb });
        }
    }
    let mut gesamt: Vec<u8> = Vec::new();
    loop {
        let stueck = antwort
            .chunk()
            .await
            .map_err(|fehler| NetFehler::Lesen(fehler.to_string()))?;
        let Some(stueck) = stueck else {
            break;
        };
        if gesamt.len() as u64 + stueck.len() as u64 > MAX_BYTES {
            return Err(NetFehler::ZuGross { max_mb });
        }
        gesamt.extend_from_slice(&stueck);
    }
    Ok(String::from_utf8_lossy(&gesamt).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_ist_immer_erlaubt() {
        assert!(adresse_erlaubt("https://firma.example.test", false).is_ok());
        assert!(adresse_erlaubt("https://firma.example.test:8443/pfad", false).is_ok());
        assert!(adresse_erlaubt("https://[::1]:8443", false).is_ok());
    }

    #[test]
    fn http_nur_loopback_oder_mit_schalter() {
        for loopback in [
            "http://127.0.0.1:8080",
            "http://localhost",
            "http://[::1]:1234",
        ] {
            assert!(
                adresse_erlaubt(loopback, false).is_ok(),
                "{loopback} sollte erlaubt sein"
            );
        }

        // Ein vorhandener Zugang darf nicht still ausfallen: der Fehler nennt
        // das Feld, das einzuschalten ist.
        let fehler = adresse_erlaubt("http://sap.example.test:8000", false).unwrap_err();
        let text = fehler.to_string();
        assert!(text.contains("https://"), "{text}");
        assert!(text.contains(TITEL_UNVERSCHLUESSELT), "{text}");
        assert!(text.contains(FELD_UNVERSCHLUESSELT) || text.contains(TITEL_UNVERSCHLUESSELT));

        assert!(adresse_erlaubt("http://sap.example.test:8000", true).is_ok());
    }

    #[test]
    fn benutzerteil_abfrage_und_fragment_sind_verboten() {
        for falsch in [
            "https://user:geheim@firma.example.test",
            "https://firma.example.test/?x=1",
            "https://firma.example.test/#x",
            "ftp://firma.example.test",
            "firma.example.test",
            "",
        ] {
            assert!(
                adresse_erlaubt(falsch, true).is_err(),
                "{falsch} sollte verboten sein"
            );
        }
    }

    #[test]
    fn die_warnung_nennt_nur_http() {
        let http = Url::parse("http://127.0.0.1:8080").unwrap();
        let text = adresse_warnung(&http).expect("Warnung");
        assert!(text.contains(TITEL_UNVERSCHLUESSELT), "{text}");
        let https = Url::parse("https://firma.example.test").unwrap();
        assert_eq!(adresse_warnung(&https), None);
    }

    #[test]
    fn zertifikat_abschalten_gilt_nur_loopback_oder_mit_ca() {
        assert_eq!(
            zertifikat(true, "sap.example.test", None).0,
            Zertifikat::Pruefen
        );
        assert_eq!(
            zertifikat(false, "127.0.0.1", None).0,
            Zertifikat::AusLoopback
        );
        assert_eq!(
            zertifikat(false, "localhost", None).0,
            Zertifikat::AusLoopback
        );

        let (regel, warnung) = zertifikat(false, "sap.example.test", None);
        assert_eq!(regel, Zertifikat::Pruefen, "sonst bliebe die Prüfung aus");
        let text = warnung.expect("Warnung");
        assert!(text.contains(TITEL_CA_ZERTIFIKAT), "{text}");

        let (regel, warnung) = zertifikat(false, "sap.example.test", Some("/tmp/ca.pem"));
        assert_eq!(regel, Zertifikat::EigenesCa(PathBuf::from("/tmp/ca.pem")));
        assert_eq!(warnung, None);
    }

    #[test]
    fn ein_fehler_nennt_kein_geheimnis() {
        let fehler = AdressFehler::Ungueltig {
            url: "ht tp://example.test".to_string(),
        };
        assert!(fehler.to_string().contains("ht tp://example.test"));
    }
}
