// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Das Werkzeug „Outline-Wiki“ (SPEC §2, §4).
//!
//! [`Outline`] sagt dem Kern, welche Felder der Zugang hat und wie man die
//! Verbindung prüft. Die Adresse wird dabei so nachsichtig gelesen, wie die
//! Vorlage sie liest: ein nackter Host, ein Schrägstrich am Ende, `/api` oder
//! `/mcp` aus der Dokumentation – alles wird zur Wurzel des Arbeitsbereichs.

use quiver_tools_kern::{Feld, FeldArt, Profil, Werkzeug};

use crate::client::Client;
use crate::domain::Info;
use crate::format;

/// Kurzname: Teil der Umgebungsvariablen (`OUTLINE_…`) und der Werkzeugnamen.
pub const NAME: &str = "outline";
/// Anzeigename im quiver-Fenster und in MCP.
pub const TITEL: &str = "Outline-Wiki";
/// Feldname der Adresse im Zugang (Umgebungsvariable `OUTLINE_URL`).
pub const ADRESSE: &str = "url";
/// Feldname des API-Schlüssels im Zugang (`OUTLINE_API_KEY`, geheim).
pub const SCHLUESSEL: &str = "api_key";
/// Feldname des Schalters „Unverschlüsselt erlauben“ (`OUTLINE_UNVERSCHLUESSELT_ERLAUBEN`).
pub const UNVERSCHLUESSELT: &str = quiver_tools_kern::net::FELD_UNVERSCHLUESSELT;
/// Feldname des eigenen CA-Zertifikats (`OUTLINE_CA_ZERTIFIKAT`).
pub const CA_ZERTIFIKAT: &str = quiver_tools_kern::net::FELD_CA_ZERTIFIKAT;
/// Feldname des Arbeitsordners für `outline_export` (Feldart „Ordner“).
pub const ORDNER: &str = "ordner";
/// Feldname des Schalters „Schreiben erlauben“ (Standard **aus**).
pub const SCHREIBEN: &str = quiver_tools_kern::FELD_SCHREIBEN;

/// Das Outline-Werkzeug.
#[derive(Debug, Clone, Copy, Default)]
pub struct Outline;

impl Werkzeug for Outline {
    fn name(&self) -> &'static str {
        NAME
    }

    fn titel(&self) -> &'static str {
        TITEL
    }

    fn beschreibung(&self) -> &'static str {
        "Outline-Wiki lesen, schreiben, sortieren und kommentieren."
    }

    fn felder(&self) -> Vec<Feld> {
        vec![
            Feld {
                name: ADRESSE,
                titel: "Basis-URL",
                art: FeldArt::Url,
                pflicht: true,
                standard: None,
                hilfe: "z. B. https://wiki.firma.de oder https://firma.getoutline.com",
                link: None,
            },
            Feld {
                name: SCHLUESSEL,
                titel: "API-Token",
                art: FeldArt::Passwort,
                pflicht: true,
                standard: None,
                hilfe:
                    "In Outline unter Einstellungen → API & Access erstellen, beginnt mit ol_api_",
                link: Some((
                    "Token in Outline erstellen",
                    "${url}/settings/api-and-access",
                )),
            },
            Feld {
                name: UNVERSCHLUESSELT,
                titel: quiver_tools_kern::net::TITEL_UNVERSCHLUESSELT,
                art: FeldArt::Schalter,
                pflicht: false,
                standard: Some("aus"),
                hilfe: "Nur nötig, wenn das Wiki über http:// (ohne TLS) erreichbar ist. \
                        Ohne diesen Schalter sind nur 127.0.0.1, localhost und [::1] per http erlaubt.",
                link: None,
            },
            Feld {
                name: CA_ZERTIFIKAT,
                titel: quiver_tools_kern::net::TITEL_CA_ZERTIFIKAT,
                art: FeldArt::Text,
                pflicht: false,
                standard: None,
                hilfe: "Pfad einer PEM-Datei, wenn das Wiki ein eigenes CA-Zertifikat nutzt. \
                        Leer heißt: Prüfung gegen den Zertifikatsspeicher des Systems.",
                link: None,
            },
            Feld {
                name: ORDNER,
                titel: "Arbeitsordner",
                art: FeldArt::Ordner,
                pflicht: false,
                standard: None,
                hilfe: "Ordner für outline_export. Dateien entstehen nur darin, nie darüber \
                        hinaus; vorhandene Dateien werden nicht überschrieben.",
                link: None,
            },
            Feld {
                name: SCHREIBEN,
                titel: quiver_tools_kern::TITEL_SCHREIBEN,
                art: FeldArt::Schalter,
                pflicht: false,
                standard: Some("aus"),
                hilfe: "Erlaubt die verändernden Werkzeuge (anlegen, ändern, verschieben, \
                        archivieren, löschen, kommentieren, exportieren). Standard: aus.",
                link: None,
            },
        ]
    }

    async fn testen(&self, profil: &Profil) -> anyhow::Result<String> {
        let client = Client::aus_profil(profil)?;
        let antwort = client
            .aufrufen::<Info>("auth.info", serde_json::json!({}))
            .await?;
        Ok(format::info(&antwort.daten))
    }
}

/// Was an einer eingegebenen Adresse nicht stimmt.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AdressFehler {
    #[error("Die Adresse darf nicht leer sein.")]
    Leer,
    #[error("„{0}“ ist keine gültige URL.")]
    Ungueltig(String),
    #[error("{0}")]
    Unverschluesselt(String),
}

/// Die Wurzel des Arbeitsbereichs, mit derselben Adressprüfung wie im Kern:
/// `https` immer, `http` nur auf dem Loopback oder mit eingeschaltetem Feld
/// „Unverschlüsselt erlauben“.
pub fn wurzel_geprueft(roh: &str, unverschluesselt: bool) -> Result<String, AdressFehler> {
    let wurzel = wurzel(roh)?;
    quiver_tools_kern::net::adresse_erlaubt(&wurzel, unverschluesselt).map_err(|fehler| {
        match fehler {
            quiver_tools_kern::net::AdressFehler::Ungueltig { url }
            | quiver_tools_kern::net::AdressFehler::Unsicher { url } => {
                AdressFehler::Ungueltig(url)
            }
            quiver_tools_kern::net::AdressFehler::Unverschluesselt { .. } => {
                AdressFehler::Unverschluesselt(fehler.to_string())
            }
        }
    })?;
    Ok(wurzel)
}

/// Die Wurzel des Arbeitsbereichs aus dem, was der User einträgt.
///
/// Annahmen der Vorlage, übernommen: fehlt `http(s)://`, wird `https://`
/// vorangestellt; ein Schrägstrich am Ende und ein angehängtes `/api` oder
/// `/mcp` fallen weg. Ergebnis ist immer ohne abschließenden Schrägstrich.
pub fn wurzel(roh: &str) -> Result<String, AdressFehler> {
    let roh = roh.trim();
    if roh.is_empty() {
        return Err(AdressFehler::Leer);
    }

    // „ftp://…“ ist ein Schema, das wir nicht bedienen – daraus darf kein
    // „https://ftp://…“ werden.
    if roh.contains("://") && !hat_schema(roh) {
        return Err(AdressFehler::Ungueltig(roh.to_string()));
    }
    let mit_schema = if hat_schema(roh) {
        roh.to_string()
    } else {
        format!("https://{roh}")
    };
    let ohne_schraegstrich = mit_schema.trim_end_matches('/');
    let ohne_anhang = ohne_endung(ohne_schraegstrich, &["/api", "/mcp"]).trim_end_matches('/');

    let url = url::Url::parse(ohne_anhang).map_err(|_| AdressFehler::Ungueltig(roh.to_string()))?;
    if url.host_str().is_none() || !matches!(url.scheme(), "http" | "https") {
        return Err(AdressFehler::Ungueltig(roh.to_string()));
    }
    Ok(ohne_anhang.to_string())
}

fn hat_schema(text: &str) -> bool {
    let klein = text.to_ascii_lowercase();
    klein.starts_with("http://") || klein.starts_with("https://")
}

/// `text` ohne eine der Endungen (Groß-/Kleinschreibung egal).
fn ohne_endung<'a>(text: &'a str, endungen: &[&str]) -> &'a str {
    for endung in endungen {
        if text.len() >= endung.len()
            && text[text.len() - endung.len()..].eq_ignore_ascii_case(endung)
        {
            return &text[..text.len() - endung.len()];
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nackter_host_bekommt_https() {
        assert_eq!(
            wurzel("firma.getoutline.com").unwrap(),
            "https://firma.getoutline.com"
        );
        assert_eq!(
            wurzel("  firma.getoutline.com  ").unwrap(),
            "https://firma.getoutline.com"
        );
    }

    #[test]
    fn schraegstrich_und_api_und_mcp_fallen_weg() {
        for eingabe in [
            "https://firma.getoutline.com/",
            "https://firma.getoutline.com/api",
            "https://firma.getoutline.com/api/",
            "https://firma.getoutline.com/mcp",
            "https://firma.getoutline.com/MCP/",
            "http://localhost:3000/api",
        ] {
            let erwartet = if eingabe.contains("localhost") {
                "http://localhost:3000"
            } else {
                "https://firma.getoutline.com"
            };
            assert_eq!(wurzel(eingabe).unwrap(), erwartet, "{eingabe}");
        }
    }

    #[test]
    fn leer_und_kaputt_werden_abgewiesen() {
        assert_eq!(wurzel("   "), Err(AdressFehler::Leer));
        assert!(matches!(
            wurzel("ht tp://example.test"),
            Err(AdressFehler::Ungueltig(_))
        ));
        assert!(matches!(
            wurzel("ftp://example.test"),
            Err(AdressFehler::Ungueltig(_))
        ));
    }

    #[test]
    fn werkzeug_nennt_name_titel_beschreibung_und_zwei_felder() {
        let werkzeug = Outline;
        assert_eq!(werkzeug.name(), "outline");
        assert_eq!(werkzeug.titel(), "Outline-Wiki");
        assert!(werkzeug.beschreibung().contains("Outline-Wiki"));
        let felder = werkzeug.felder();
        assert_eq!(felder.len(), 6);
        assert_eq!(felder[0].name, "url");
        assert_eq!(felder[0].art, FeldArt::Url);
        assert!(felder[0].pflicht);
        assert!(felder[0].hilfe.contains("https://firma.getoutline.com"));
        assert_eq!(felder[1].name, "api_key");
        assert_eq!(felder[1].art, FeldArt::Passwort);
        assert!(felder[1].pflicht);
        assert!(felder[1].hilfe.contains("ol_api_"));
        // Die Härtung: Adresse, Zertifikat, Arbeitsordner und Schreibschalter.
        assert_eq!(felder[2].name, UNVERSCHLUESSELT);
        assert_eq!(felder[3].name, CA_ZERTIFIKAT);
        assert_eq!(felder[4].name, ORDNER);
        assert_eq!(felder[4].art, FeldArt::Ordner);
        assert_eq!(felder[5].name, SCHREIBEN);
        // Der Schreibschalter ist im Zweifel aus.
        assert_eq!(felder[5].standard, Some("aus"));
    }

    #[test]
    fn http_ist_ohne_schalter_nur_auf_loopback_erlaubt() {
        assert!(wurzel_geprueft("http://127.0.0.1:3000", false).is_ok());
        assert!(wurzel_geprueft("http://localhost:3000", false).is_ok());
        let fehler = wurzel_geprueft("http://wiki.example.test", false).unwrap_err();
        assert!(
            fehler.to_string().contains("Unverschlüsselt erlauben"),
            "{fehler}"
        );
        assert!(wurzel_geprueft("http://wiki.example.test", true).is_ok());
    }
}
