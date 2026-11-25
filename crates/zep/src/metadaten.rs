// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Metadaten des ZEP-MCP und seines Authorization Servers (Aufgabe 06).
//!
//! Fest verdrahtet ist nur die Basis-Adresse, die der User einträgt. Alles
//! andere – Authorization-, Token- und Registrierungs-Endpunkt, der Scope und
//! die PKCE-Verfahren – steht in den Metadaten (RFC 9728 und RFC 8414):
//!
//! ```text
//! MCP          https://host/<mandant>/next/mcp
//! Ressource    https://host/.well-known/oauth-protected-resource/<mandant>/next/mcp
//! Server       https://host/.well-known/oauth-authorization-server/<mandant>/next
//! ```

use std::time::Duration;

use serde::Deserialize;
use url::Url;

/// Wie lange auf Metadaten gewartet wird.
pub const ZEITLIMIT: Duration = Duration::from_secs(20);

/// Was an den Metadaten nicht stimmt.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MetadatenFehler {
    #[error("„{0}“ ist keine gültige MCP-Adresse (erwartet: https://host/<mandant>/next/mcp).")]
    UngueltigeAdresse(String),
    #[error("{url} ist nicht erreichbar: {ursache}")]
    NichtErreichbar { url: String, ursache: String },
    #[error("{url} antwortet mit Status {status} statt mit Metadaten.")]
    Status { url: String, status: u16 },
    #[error("Die Metadaten unter {url} sind nicht lesbar: {ursache}")]
    Unlesbar { url: String, ursache: String },
    #[error("Die Metadaten nennen keinen Authorization Server.")]
    KeinServer,
    #[error(
        "Der Authorization Server bietet kein PKCE mit S256 an – ohne das wäre die Anmeldung unsicher."
    )]
    KeinS256,
    #[error("Der Authorization Server nennt keinen Token-Endpunkt.")]
    KeinTokenEndpunkt,
}

/// Der geschützte Bereich (RFC 9728): wer autorisiert hier, und wofür.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct GeschuetzteRessource {
    #[serde(default)]
    pub resource: Option<String>,
    #[serde(default)]
    pub authorization_servers: Vec<String>,
    #[serde(default)]
    pub scopes_supported: Option<Vec<String>>,
}

/// Die Metadaten des Authorization Servers (RFC 8414), soweit wir sie brauchen.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ServerMetadaten {
    #[serde(default)]
    pub issuer: String,
    #[serde(default)]
    pub authorization_endpoint: String,
    #[serde(default)]
    pub token_endpoint: String,
    #[serde(default)]
    pub registration_endpoint: Option<String>,
    #[serde(default)]
    pub scopes_supported: Option<Vec<String>>,
    #[serde(default)]
    pub code_challenge_methods_supported: Option<Vec<String>>,
    #[serde(default)]
    pub grant_types_supported: Option<Vec<String>>,
    #[serde(default)]
    pub token_endpoint_auth_methods_supported: Option<Vec<String>>,
}

/// Was der Ablauf braucht, aus beiden Dokumenten zusammengesetzt.
#[derive(Debug, Clone)]
pub struct Metadaten {
    /// Die MCP-Adresse, wie der User sie eingetragen hat (ohne Schrägstrich).
    pub mcp: Url,
    /// Der Scope, den der Authorize-Aufruf verlangt (`mcp:use`, wenn die
    /// Metadaten nichts anderes sagen).
    pub scope: String,
    /// Die Authorization Server (in der Reihenfolge der Metadaten).
    pub server: ServerMetadaten,
}

impl Metadaten {
    /// Der Authorization-Endpunkt.
    pub fn authorization_endpoint(&self) -> Url {
        Url::parse(&self.server.authorization_endpoint).unwrap_or_else(|_| self.mcp.clone())
    }

    /// Der Token-Endpunkt.
    pub fn token_endpoint(&self) -> Option<Url> {
        Url::parse(&self.server.token_endpoint).ok()
    }

    /// Der Registrierungs-Endpunkt (DCR), falls der Server einen nennt.
    pub fn registrierungs_endpunkt(&self) -> Option<Url> {
        self.server
            .registration_endpoint
            .as_deref()
            .and_then(|text| Url::parse(text).ok())
    }
}

/// Die Adresse der Metadaten des geschützten Bereichs zu einer MCP-Adresse.
pub fn ressourcen_adresse(mcp: &Url) -> Result<Url, MetadatenFehler> {
    let mut ziel = mcp.clone();
    let pfad = mcp.path().trim_end_matches('/').to_string();
    ziel.set_path(&format!("/.well-known/oauth-protected-resource{pfad}"));
    ziel.set_query(None);
    ziel.set_fragment(None);
    Ok(ziel)
}

/// Die Adresse der Metadaten eines Authorization Servers.
pub fn server_adresse(issuer: &Url) -> Result<Url, MetadatenFehler> {
    let mut ziel = issuer.clone();
    let pfad = issuer.path().trim_end_matches('/').to_string();
    ziel.set_path(&format!("/.well-known/oauth-authorization-server{pfad}"));
    ziel.set_query(None);
    ziel.set_fragment(None);
    Ok(ziel)
}

/// Die MCP-Adresse prüfen und normalisieren (kein Schrägstrich am Ende).
pub fn mcp_adresse(roh: &str) -> Result<Url, MetadatenFehler> {
    let roh = roh.trim();
    if roh.is_empty() {
        return Err(MetadatenFehler::UngueltigeAdresse(roh.to_string()));
    }
    let mut url =
        Url::parse(roh).map_err(|_| MetadatenFehler::UngueltigeAdresse(roh.to_string()))?;
    if !matches!(url.scheme(), "https" | "http") || url.host_str().is_none() {
        return Err(MetadatenFehler::UngueltigeAdresse(roh.to_string()));
    }
    let ohne_schraegstrich = url.path().trim_end_matches('/').to_string();
    url.set_path(if ohne_schraegstrich.is_empty() {
        "/"
    } else {
        &ohne_schraegstrich
    });
    url.set_fragment(None);
    Ok(url)
}

/// Beide Dokumente holen und prüfen, dass der Server kann, was wir brauchen.
pub async fn holen(http: &reqwest::Client, mcp: &Url) -> Result<Metadaten, MetadatenFehler> {
    let ressourcen_url = ressourcen_adresse(mcp)?;
    let roh: GeschuetzteRessource = hole(http, &ressourcen_url).await?;
    let Some(erster_server) = roh.authorization_servers.first() else {
        return Err(MetadatenFehler::KeinServer);
    };
    let issuer = Url::parse(erster_server).map_err(|_| MetadatenFehler::Unlesbar {
        url: ressourcen_url.to_string(),
        ursache: format!("„{erster_server}“ ist keine Adresse"),
    })?;

    let server_url = server_adresse(&issuer)?;
    let server: ServerMetadaten = hole(http, &server_url).await?;

    if server.token_endpoint.is_empty() {
        return Err(MetadatenFehler::KeinTokenEndpunkt);
    }
    if let Some(verfahren) = &server.code_challenge_methods_supported {
        if !verfahren.iter().any(|verfahren| verfahren == "S256") {
            return Err(MetadatenFehler::KeinS256);
        }
    }

    let scope = roh
        .scopes_supported
        .clone()
        .or_else(|| server.scopes_supported.clone())
        .filter(|scopes| !scopes.is_empty())
        .map(|scopes| scopes.join(" "))
        .unwrap_or_else(|| "mcp:use".to_string());

    Ok(Metadaten {
        mcp: mcp.clone(),
        scope,
        server,
    })
}

/// Ein JSON-Dokument holen und lesen.
async fn hole<T: serde::de::DeserializeOwned>(
    http: &reqwest::Client,
    url: &Url,
) -> Result<T, MetadatenFehler> {
    let antwort = http
        .get(url.as_str())
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|fehler| MetadatenFehler::NichtErreichbar {
            url: url.to_string(),
            ursache: fehler.without_url().to_string(),
        })?;

    let status = antwort.status();
    if !status.is_success() {
        return Err(MetadatenFehler::Status {
            url: url.to_string(),
            status: status.as_u16(),
        });
    }

    let koerper = antwort
        .text()
        .await
        .map_err(|fehler| MetadatenFehler::Unlesbar {
            url: url.to_string(),
            ursache: fehler.without_url().to_string(),
        })?;
    serde_json::from_str(&koerper).map_err(|fehler| MetadatenFehler::Unlesbar {
        url: url.to_string(),
        ursache: fehler.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MCP: &str = "https://www.zep-online.de/firma/next/mcp";

    #[test]
    fn die_well_known_adressen_folgen_den_rfc() {
        let mcp = mcp_adresse(MCP).unwrap();
        assert_eq!(
            ressourcen_adresse(&mcp).unwrap().as_str(),
            "https://www.zep-online.de/.well-known/oauth-protected-resource/firma/next/mcp"
        );

        let issuer = Url::parse("https://www.zep-online.de/firma/next").unwrap();
        assert_eq!(
            server_adresse(&issuer).unwrap().as_str(),
            "https://www.zep-online.de/.well-known/oauth-authorization-server/firma/next"
        );
    }

    #[test]
    fn der_schraegstrich_am_ende_faellt_weg() {
        let mcp = mcp_adresse(" https://example.test/a/next/mcp/ ").unwrap();
        assert_eq!(mcp.as_str(), "https://example.test/a/next/mcp");
    }

    #[test]
    fn unbrauchbare_adressen_werden_abgewiesen() {
        for roh in ["", "   ", "kein-schema", "ftp://example.test/mcp"] {
            assert!(
                matches!(mcp_adresse(roh), Err(MetadatenFehler::UngueltigeAdresse(_))),
                "{roh}"
            );
        }
    }

    #[test]
    fn ohne_authorization_server_kein_metadatum() {
        let roh: GeschuetzteRessource =
            serde_json::from_str(r#"{ "resource": "https://example.test/mcp" }"#).unwrap();
        assert!(roh.authorization_servers.is_empty());
    }

    #[test]
    fn server_metadaten_werden_gelesen() {
        let server: ServerMetadaten = serde_json::from_str(
            r#"{
              "issuer": "https://example.test/a/next",
              "authorization_endpoint": "https://example.test/a/next/oauth/authorize",
              "token_endpoint": "https://example.test/a/next/oauth/token",
              "registration_endpoint": "https://example.test/a/next/oauth/register",
              "code_challenge_methods_supported": ["S256"],
              "grant_types_supported": ["authorization_code", "refresh_token"]
            }"#,
        )
        .unwrap();
        assert_eq!(
            server.registration_endpoint.as_deref(),
            Some("https://example.test/a/next/oauth/register")
        );
        assert_eq!(
            server.code_challenge_methods_supported.as_deref(),
            Some(["S256".to_string()].as_slice())
        );
    }
}
