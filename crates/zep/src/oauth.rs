// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die OAuth-Aufrufe des Werkzeugs (Aufgabe 06): Client-Registrierung (DCR),
//! Authorize-Adresse, Code-Tausch und Erneuerung per Refresh-Token.
//!
//! Alle Endpunkte kommen aus [`crate::metadaten`]; hier steht kein fester
//! Anbieter. Fehlertexte nennen höchstens `error` und `error_description` des
//! Servers – nie ein Token, nie ein Passwort.

use std::time::Duration;

use serde::Deserialize;
use url::Url;

use crate::pkce::Pkce;

/// Der Name, unter dem sich das Werkzeug registriert (DCR).
pub const CLIENT_NAME: &str = "quiver-zep";

/// Die Rückleitung ohne Port: so wird sie registriert (Aufgabe 06 – Port egal).
pub const UMLEITUNG: &str = "http://127.0.0.1";

/// Wie lange ein Token-Aufruf höchstens dauert.
pub const ZEITLIMIT: Duration = Duration::from_secs(20);

/// Die Rückleitung für einen bestimmten Port (Notfall über den Browser).
pub fn umleitung_mit_port(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

/// Was der Token-Endpunkt ausstellt. Enthält Geheimnisse → `Debug` ist von Hand.
#[derive(Clone, PartialEq, Eq)]
pub struct TokenAntwort {
    /// Das Access-Token; bleibt im Arbeitsspeicher.
    pub access: String,
    /// Das (neue) Refresh-Token; rotiert bei jedem Aufruf oft mit.
    pub refresh: Option<String>,
    /// Wie lange das Access-Token gilt.
    pub gueltig: Duration,
}

impl std::fmt::Debug for TokenAntwort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenAntwort")
            .field("access", &"<geheim>")
            .field(
                "refresh",
                &if self.refresh.is_some() {
                    "<geheim>"
                } else {
                    "<keins>"
                },
            )
            .field("gueltig", &self.gueltig)
            .finish()
    }
}

/// Was beim Aufruf eines OAuth-Endpunkts schiefgehen kann.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OauthFehler {
    /// Der Server hat den Vorgang abgelehnt (RFC 6749 §5.2).
    #[error("Der Authorization Server lehnt ab: {code}{hinweis}")]
    Abgelehnt { code: String, hinweis: String },
    #[error("{url} antwortet nicht: {ursache}")]
    NichtErreichbar { url: String, ursache: String },
    #[error("{url} antwortet mit Status {status}.")]
    Status { url: String, status: u16 },
    #[error("Die Antwort von {url} ist nicht lesbar: {ursache}")]
    Unlesbar { url: String, ursache: String },
    #[error("Der Authorization Server nennt keinen Registrierungs-Endpunkt (DCR).")]
    KeinRegistrierungsEndpunkt,
    #[error("Die Registrierung hat keine client_id geliefert.")]
    KeineClientId,
    #[error("Es gibt keinen Token-Endpunkt.")]
    KeinTokenEndpunkt,
    #[error("Die Adresse „{0}“ taugt nicht als Rückleitung.")]
    UngueltigeUmleitung(String),
}

impl OauthFehler {
    /// `invalid_grant`: Refresh-Token abgelehnt oder abgelaufen – neu anmelden.
    pub fn ist_invalid_grant(&self) -> bool {
        matches!(self, OauthFehler::Abgelehnt { code, .. } if code == "invalid_grant")
    }
}

/// Die Registrierung (RFC 7591): öffentlicher Client ohne Secret, Rückleitung
/// `http://127.0.0.1` – der Port darf beim Aufruf ein anderer sein.
#[derive(Debug, serde::Serialize)]
struct Registrierung<'a> {
    client_name: &'a str,
    redirect_uris: Vec<&'a str>,
    grant_types: Vec<&'a str>,
    response_types: Vec<&'a str>,
    token_endpoint_auth_method: &'a str,
}

#[derive(Debug, Deserialize)]
struct RegistrierungsAntwort {
    client_id: String,
}

/// Den Client einmal registrieren und die `client_id` zurückgeben.
pub async fn registrieren(http: &reqwest::Client, endpunkt: &Url) -> Result<String, OauthFehler> {
    let rumpf = Registrierung {
        client_name: CLIENT_NAME,
        redirect_uris: vec![UMLEITUNG],
        grant_types: vec!["authorization_code", "refresh_token"],
        response_types: vec!["code"],
        token_endpoint_auth_method: "none",
    };

    let antwort = http
        .post(endpunkt.as_str())
        .header("Accept", "application/json")
        .json(&rumpf)
        .send()
        .await
        .map_err(|fehler| OauthFehler::NichtErreichbar {
            url: endpunkt.to_string(),
            ursache: fehler.without_url().to_string(),
        })?;

    let status = antwort.status();
    let koerper = antwort.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(statusfehler(endpunkt, status.as_u16(), &koerper));
    }
    let gelesen: RegistrierungsAntwort =
        serde_json::from_str(&koerper).map_err(|fehler| OauthFehler::Unlesbar {
            url: endpunkt.to_string(),
            ursache: fehler.to_string(),
        })?;
    if gelesen.client_id.trim().is_empty() {
        return Err(OauthFehler::KeineClientId);
    }
    Ok(gelesen.client_id)
}

/// Die Adresse, die der User im Browser (oder die Routine ohne Browser) aufruft.
pub fn authorize_url(
    endpunkt: &Url,
    client_id: &str,
    umleitung: &str,
    state: &str,
    pkce: &Pkce,
    scope: &str,
) -> Result<Url, OauthFehler> {
    let mut ziel = endpunkt.clone();
    ziel.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", umleitung)
        .append_pair("state", state)
        .append_pair("code_challenge", pkce.challenge())
        .append_pair("code_challenge_method", "S256")
        .append_pair("scope", scope);
    Ok(ziel)
}

/// Der Rumpf einer Token-Anfrage.
#[derive(Debug, Deserialize)]
struct TokenRoh {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
}

impl TokenRoh {
    fn in_antwort(self) -> TokenAntwort {
        TokenAntwort {
            access: self.access_token,
            refresh: self.refresh_token,
            gueltig: Duration::from_secs(self.expires_in.unwrap_or(3600).max(1)),
        }
    }
}

/// Den Code gegen Token tauschen (`grant_type=authorization_code`).
pub async fn code_tauschen(
    http: &reqwest::Client,
    endpunkt: &Url,
    client_id: &str,
    code: &str,
    verifier: &str,
    umleitung: &str,
) -> Result<TokenAntwort, OauthFehler> {
    let formular = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", umleitung),
        ("client_id", client_id),
        ("code_verifier", verifier),
    ];
    token_anfragen(http, endpunkt, &formular).await
}

/// Ein neues Access-Token per Refresh-Token holen (`grant_type=refresh_token`).
pub async fn erneuern(
    http: &reqwest::Client,
    endpunkt: &Url,
    client_id: &str,
    refresh: &str,
) -> Result<TokenAntwort, OauthFehler> {
    let formular = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh),
        ("client_id", client_id),
    ];
    token_anfragen(http, endpunkt, &formular).await
}

async fn token_anfragen(
    http: &reqwest::Client,
    endpunkt: &Url,
    formular: &[(&str, &str)],
) -> Result<TokenAntwort, OauthFehler> {
    let antwort = http
        .post(endpunkt.as_str())
        .header("Accept", "application/json")
        .form(&formular)
        .send()
        .await
        .map_err(|fehler| OauthFehler::NichtErreichbar {
            url: endpunkt.to_string(),
            ursache: fehler.without_url().to_string(),
        })?;

    let status = antwort.status();
    let koerper = antwort
        .text()
        .await
        .map_err(|fehler| OauthFehler::Unlesbar {
            url: endpunkt.to_string(),
            ursache: fehler.without_url().to_string(),
        })?;

    if !status.is_success() {
        return Err(statusfehler(endpunkt, status.as_u16(), &koerper));
    }

    let roh: TokenRoh = serde_json::from_str(&koerper).map_err(|fehler| OauthFehler::Unlesbar {
        url: endpunkt.to_string(),
        ursache: fehler.to_string(),
    })?;
    Ok(roh.in_antwort())
}

/// Ein Fehlerstatus: `error`/`error_description` des Servers, sonst der Status.
fn statusfehler(endpunkt: &Url, status: u16, koerper: &str) -> OauthFehler {
    #[derive(Deserialize)]
    struct Fehler {
        #[serde(default)]
        error: Option<String>,
        #[serde(default)]
        error_description: Option<String>,
    }

    if let Ok(fehler) = serde_json::from_str::<Fehler>(koerper) {
        if let Some(code) = fehler.error.filter(|code| !code.trim().is_empty()) {
            let hinweis = fehler
                .error_description
                .filter(|text| !text.trim().is_empty())
                .map(|text| format!(" ({text})"))
                .unwrap_or_default();
            return OauthFehler::Abgelehnt { code, hinweis };
        }
    }
    OauthFehler::Status {
        url: endpunkt.to_string(),
        status,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn die_authorize_adresse_traegt_alles_was_pkce_braucht() {
        let endpunkt = Url::parse("https://zep.example.test/a/next/oauth/authorize").unwrap();
        let pkce = Pkce::neu();
        let url = authorize_url(
            &endpunkt,
            "quiver-zep-1",
            UMLEITUNG,
            "state-4711",
            &pkce,
            "mcp:use",
        )
        .unwrap();

        let paare: std::collections::BTreeMap<String, String> =
            url.query_pairs().into_owned().collect();
        assert_eq!(paare["response_type"], "code");
        assert_eq!(paare["client_id"], "quiver-zep-1");
        assert_eq!(paare["redirect_uri"], "http://127.0.0.1");
        assert_eq!(paare["state"], "state-4711");
        assert_eq!(paare["code_challenge"], pkce.challenge());
        assert_eq!(paare["code_challenge_method"], "S256");
        assert_eq!(paare["scope"], "mcp:use");
    }

    #[test]
    fn die_umleitung_mit_port_bleibt_am_loopback() {
        assert_eq!(umleitung_mit_port(4711), "http://127.0.0.1:4711");
        assert!(UMLEITUNG.starts_with("http://127.0.0.1"));
    }

    #[test]
    fn nur_invalid_grant_verlangt_eine_neue_anmeldung() {
        let grant = OauthFehler::Abgelehnt {
            code: "invalid_grant".to_string(),
            hinweis: " (abgelaufen)".to_string(),
        };
        assert!(grant.ist_invalid_grant());
        assert!(grant.to_string().contains("invalid_grant"));

        let anderer = OauthFehler::Abgelehnt {
            code: "invalid_client".to_string(),
            hinweis: String::new(),
        };
        assert!(!anderer.ist_invalid_grant());
        assert!(!OauthFehler::KeineClientId.ist_invalid_grant());
    }

    #[test]
    fn ein_fehlerstatus_wird_zu_einer_abgelehnten_antwort() {
        let endpunkt = Url::parse("https://zep.example.test/token").unwrap();
        let fehler = statusfehler(
            &endpunkt,
            400,
            r#"{"error":"invalid_grant","error_description":"Token abgelaufen"}"#,
        );
        assert_eq!(
            fehler,
            OauthFehler::Abgelehnt {
                code: "invalid_grant".to_string(),
                hinweis: " (Token abgelaufen)".to_string()
            }
        );
        assert_eq!(
            statusfehler(&endpunkt, 500, "<html>kaputt</html>"),
            OauthFehler::Status {
                url: "https://zep.example.test/token".to_string(),
                status: 500
            }
        );
    }

    #[test]
    fn debug_zeigt_keine_tokens() {
        let antwort = TokenAntwort {
            access: "at-nur-im-test".to_string(),
            refresh: Some("rt-nur-im-test".to_string()),
            gueltig: Duration::from_secs(60),
        };
        let text = format!("{antwort:?}");
        assert!(!text.contains("at-nur-im-test"));
        assert!(!text.contains("rt-nur-im-test"));
        assert!(text.contains("geheim"));
    }
}
