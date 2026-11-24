// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Admin-Freigabe im Browser (SPEC.md §2a, Aufgabe 02):
//! `quiver-m365 freigeben [--tenant <id|domain>]` öffnet
//! `<authority>/<tenant>/v2.0/adminconsent?...` und wartet auf die Rückleitung.
//!
//! Der Lauscher bindet an `127.0.0.1` – und die `redirect_uri` zeigt genau
//! dorthin (nicht auf `localhost`, das im Browser zuerst `::1` sein kann).

use std::fmt::Write as _;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use m365_anmeldung::{Browser, Konfig};

use crate::kodierung::{entkodieren, kodieren};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// So lange wartet `freigeben` auf die Rückleitung aus dem Browser.
pub const WARTEN: Duration = Duration::from_secs(300);
/// Pfad der Admin-Zustimmungsseite (ohne Autorität und Tenant).
const PFAD: &str = "v2.0/adminconsent";
/// Rechte, die bei der Admin-Freigabe nichts zu suchen haben.
const OHNE_FREIGABE: &[&str] = &["offline_access"];

/// Ergebnis einer erfolgreichen Freigabe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Freigabe {
    /// Der Tenant, für den freigegeben wurde (aus der Rückleitung).
    pub tenant: String,
}

/// Öffnet die Admin-Zustimmungsseite und wartet auf die Rückleitung.
pub async fn freigeben<B: Browser>(
    konfig: &Konfig,
    tenant: &str,
    browser: &B,
) -> anyhow::Result<Freigabe> {
    let tenant = tenant.trim();
    if tenant.is_empty() || tenant == "organizations" {
        anyhow::bail!(
            "Bitte den Tenant angeben: quiver-m365 freigeben --tenant firma.de \
             (für die Admin-Freigabe braucht es einen bestimmten Tenant)"
        );
    }

    let lauscher = TcpListener::bind(("127.0.0.1", 0)).await?;
    let port = lauscher.local_addr()?.port();
    let state = zufalls_state();
    let url = freigabe_url(konfig, tenant, port, &state);

    browser.oeffnen(&url)?;
    tracing::info!("Browser für die Admin-Freigabe geöffnet (Tenant {tenant})");

    let ende = tokio::time::Instant::now() + WARTEN;
    loop {
        let rest = ende.saturating_duration_since(tokio::time::Instant::now());
        if rest.is_zero() {
            anyhow::bail!("Keine Antwort auf die Freigabe innerhalb von 5 Minuten");
        }
        let (mut strom, _) = match tokio::time::timeout(rest, lauscher.accept()).await {
            Ok(Ok(paar)) => paar,
            Ok(Err(fehler)) => return Err(fehler.into()),
            Err(_) => anyhow::bail!("Keine Antwort auf die Freigabe innerhalb von 5 Minuten"),
        };

        let mut puffer = [0u8; 8192];
        let gelesen =
            match tokio::time::timeout(Duration::from_secs(10), strom.read(&mut puffer)).await {
                Ok(Ok(gelesen)) => gelesen,
                _ => continue,
            };
        let roh = String::from_utf8_lossy(&puffer[..gelesen]).to_string();

        let Some(ziel) = anfrage_ziel(&roh) else {
            antwort(&mut strom, 400, "Ungültige Anfrage.").await;
            continue;
        };
        let felder = abfrage_felder(&ziel);

        // `favicon.ico` und alles andere ohne unsere Felder ignorieren.
        let unser = felder
            .iter()
            .any(|(name, _)| name == "state" || name == "error" || name == "admin_consent");
        if !unser {
            antwort(&mut strom, 404, "Hier gibt es nichts zu sehen.").await;
            continue;
        }

        let wert = |name: &str| {
            felder
                .iter()
                .find(|(feld, _)| feld == name)
                .map(|(_, wert)| wert.clone())
        };

        match wert("state") {
            Some(rueck) if rueck == state => {}
            _ => {
                antwort(&mut strom, 400, "state passt nicht – Freigabe verworfen.").await;
                anyhow::bail!("state passt nicht – Freigabe verworfen");
            }
        }

        if let Some(fehler) = wert("error") {
            let text = wert("error_description").unwrap_or(fehler);
            antwort(
                &mut strom,
                200,
                &format!(
                    "Freigabe nicht erteilt: {}<br>Du kannst dieses Fenster schließen.",
                    escape(&text)
                ),
            )
            .await;
            anyhow::bail!("Freigabe abgelehnt: {text}");
        }

        match wert("admin_consent").as_deref() {
            Some(consent) if consent.eq_ignore_ascii_case("true") => {
                let tenant = wert("tenant").unwrap_or_else(|| tenant.to_string());
                antwort(
                    &mut strom,
                    200,
                    &format!(
                        "Freigegeben für Tenant {} – du kannst dieses Fenster schließen.",
                        escape(&tenant)
                    ),
                )
                .await;
                tracing::info!("Admin-Freigabe erteilt");
                return Ok(Freigabe { tenant });
            }
            _ => {
                antwort(
                    &mut strom,
                    400,
                    "Die Antwort enthält keine Freigabe.<br>Du kannst dieses Fenster schließen.",
                )
                .await;
                anyhow::bail!("Rückleitung ohne admin_consent");
            }
        }
    }
}

/// Die Adresse, die im Browser aufgeht.
pub fn freigabe_url(konfig: &Konfig, tenant: &str, port: u16, state: &str) -> String {
    let authority = konfig.authority.trim_end_matches('/');
    let tenant = tenant.trim_matches('/');
    let scope = freigabe_scopes(&konfig.scopes).join(" ");
    let umleitung = format!("http://127.0.0.1:{port}");
    format!(
        "{authority}/{tenant}/{PFAD}?client_id={}&scope={}&redirect_uri={}&state={}",
        kodieren(&konfig.client_id),
        kodieren(&scope),
        kodieren(&umleitung),
        kodieren(state),
    )
}

/// Die Rechte für die Admin-Freigabe: ohne `offline_access`, mit Graph-Präfix.
pub fn freigabe_scopes(scopes: &[String]) -> Vec<String> {
    scopes
        .iter()
        .filter(|scope| !OHNE_FREIGABE.contains(&scope.as_str()))
        .map(|scope| {
            if scope.starts_with("https://") {
                scope.clone()
            } else {
                format!("https://graph.microsoft.com/{scope}")
            }
        })
        .collect()
}

/// `state` (CSRF-Schutz): 64 Hexzeichen, ohne eigenes Zufalls-Crate.
fn zufalls_state() -> String {
    use std::hash::{BuildHasher, Hasher};

    let zeit = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|dauer| dauer.as_nanos() as u64)
        .unwrap_or_default();
    let mut text = String::with_capacity(64);
    for runde in 0..4u64 {
        // `RandomState` bringt Zufallsschlüssel mit; Zeit, PID und Runde mischen.
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u64(zeit);
        hasher.write_u32(std::process::id());
        hasher.write_u64(runde);
        let _ = write!(text, "{:016x}", hasher.finish());
    }
    text
}

/// Ziel (`/pfad?abfrage`) aus der ersten Zeile einer HTTP-Anfrage.
fn anfrage_ziel(anfrage: &str) -> Option<String> {
    let erste_zeile = anfrage.lines().next()?;
    let mut teile = erste_zeile.split_whitespace();
    let methode = teile.next()?;
    if methode != "GET" && methode != "HEAD" {
        return None;
    }
    let ziel = teile.next()?;
    if !ziel.starts_with('/') {
        return None;
    }
    Some(ziel.to_string())
}

/// `?a=b&c=d` in Paare zerlegen, Prozent-Kodierung aufgelöst.
fn abfrage_felder(ziel: &str) -> Vec<(String, String)> {
    let Some((_, abfrage)) = ziel.split_once('?') else {
        return Vec::new();
    };
    abfrage
        .split('&')
        .filter(|teil| !teil.is_empty())
        .map(|teil| {
            let (name, wert) = teil.split_once('=').unwrap_or((teil, ""));
            (entkodieren(name), entkodieren(wert))
        })
        .collect()
}

fn escape(text: &str) -> String {
    let mut raus = String::with_capacity(text.len());
    for zeichen in text.chars() {
        match zeichen {
            '&' => raus.push_str("&amp;"),
            '<' => raus.push_str("&lt;"),
            '>' => raus.push_str("&gt;"),
            '"' => raus.push_str("&quot;"),
            '\'' => raus.push_str("&#39;"),
            sonst => raus.push(sonst),
        }
    }
    raus
}

/// Die eine Seite im Browser – ohne Token, ohne Query.
async fn antwort(strom: &mut tokio::net::TcpStream, status: u16, meldung: &str) {
    let grund = match status {
        200 => "OK",
        400 => "Bad Request",
        _ => "Not Found",
    };
    let seite = format!(
        "<!doctype html><html lang=\"de\"><head><meta charset=\"utf-8\">\
         <title>quiver-m365 – Freigabe</title></head><body><p>{meldung}</p></body></html>"
    );
    let kopf = format!(
        "HTTP/1.1 {status} {grund}\r\n\
         Content-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\r\n",
        seite.len()
    );
    let _ = strom.write_all(kopf.as_bytes()).await;
    let _ = strom.write_all(seite.as_bytes()).await;
    let _ = strom.flush().await;
    let _ = strom.shutdown().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn konfig() -> Konfig {
        Konfig {
            client_id: "abc-123".to_string(),
            tenant: "organizations".to_string(),
            scopes: vec![
                "User.Read".to_string(),
                "Mail.Send".to_string(),
                "offline_access".to_string(),
            ],
            authority: "https://login.microsoftonline.com/".to_string(),
            warten: Duration::from_secs(5),
            lauscher_hoechstens: Duration::from_secs(30),
        }
    }

    #[test]
    fn url_hat_die_form_aus_der_spec() {
        let url = freigabe_url(&konfig(), "firma.de", 4242, "abc");
        assert!(url.starts_with("https://login.microsoftonline.com/firma.de/v2.0/adminconsent?"));
        assert!(url.contains("client_id=abc-123"));
        assert!(url.contains("state=abc"));
        assert!(
            url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A4242"),
            "Rückleitung auf 127.0.0.1: {url}"
        );
        assert!(
            url.contains("scope=https%3A%2F%2Fgraph.microsoft.com%2FUser.Read%20"),
            "Rechte mit Graph-Präfix: {url}"
        );
        assert!(
            !url.contains("offline_access"),
            "ohne offline_access: {url}"
        );
    }

    #[test]
    fn scopes_bekommen_das_praefix() {
        let scopes = freigabe_scopes(&[
            "User.Read".to_string(),
            "https://graph.microsoft.com/Mail.Send".to_string(),
            "offline_access".to_string(),
        ]);
        assert_eq!(
            scopes,
            vec![
                "https://graph.microsoft.com/User.Read".to_string(),
                "https://graph.microsoft.com/Mail.Send".to_string(),
            ]
        );
    }

    #[test]
    fn anfrage_ziel_wird_gelesen() {
        let roh = "GET /?admin_consent=True&tenant=t&state=s HTTP/1.1\r\nHost: x\r\n\r\n";
        assert_eq!(
            anfrage_ziel(roh).as_deref(),
            Some("/?admin_consent=True&tenant=t&state=s")
        );
        assert!(anfrage_ziel("POST / HTTP/1.1").is_none());
        assert!(anfrage_ziel("").is_none());
    }

    #[test]
    fn abfrage_wird_zerlegt() {
        let felder = abfrage_felder("/?a=1&b=zwei%20Worte&c");
        assert_eq!(
            felder,
            vec![
                ("a".to_string(), "1".to_string()),
                ("b".to_string(), "zwei Worte".to_string()),
                ("c".to_string(), String::new()),
            ]
        );
        assert!(abfrage_felder("/ohne").is_empty());
    }

    #[test]
    fn state_ist_lang_und_zufaellig() {
        let a = zufalls_state();
        let b = zufalls_state();
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|zeichen| zeichen.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn html_wird_maskiert() {
        assert_eq!(escape("<b>&"), "&lt;b&gt;&amp;");
    }
}
