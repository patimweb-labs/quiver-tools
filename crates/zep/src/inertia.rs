// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Zustimmungsseite von ZEP (Laravel Passport mit Inertia).
//!
//! ZEP baut die Seite „Zugriff erlauben“ per JavaScript: im HTML steht kein
//! `<form>`, sondern ein `<script data-page type="application/json">` mit den
//! Daten der Seite. Der Knopf „OK“ schickt – wie das klassische Passport-
//! Formular – `_token`, `state`, `client_id` und `auth_token` per POST an
//! `approveUrl`. Dieses Modul liest genau diese Angaben heraus.

use crate::html::zerlegen;

/// Was die Zustimmung braucht.
#[derive(Clone, PartialEq, Eq)]
pub struct Zustimmung {
    /// Wohin „OK“ geht (wie in der Seite, noch nicht aufgelöst).
    pub approve_url: String,
    /// Der Client, um den es geht – muss unserer sein.
    pub client_id: String,
    /// Der Name des Clients, wie ZEP ihn anzeigt.
    pub client_name: String,
    /// Die erbetenen Scopes (Namen).
    pub scopes: Vec<String>,
    state: String,
    auth_token: String,
    csrf_token: String,
}

impl std::fmt::Debug for Zustimmung {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Zustimmung")
            .field("approve_url", &self.approve_url)
            .field("client_id", &self.client_id)
            .field("client_name", &self.client_name)
            .field("scopes", &self.scopes)
            .finish_non_exhaustive()
    }
}

impl Zustimmung {
    /// Der Rumpf für „OK“ – wie das Passport-Formular.
    pub fn rumpf(&self) -> Vec<(String, String)> {
        vec![
            ("_token".to_string(), self.csrf_token.clone()),
            ("state".to_string(), self.state.clone()),
            ("client_id".to_string(), self.client_id.clone()),
            ("auth_token".to_string(), self.auth_token.clone()),
        ]
    }
}

/// Die Zustimmung aus einer Inertia-Seite lesen, falls es eine ist.
pub fn zustimmung(html: &str) -> Option<Zustimmung> {
    let seite = seitendaten(html)?;
    let props = seite.get("props")?;
    let text = |name: &str| {
        props
            .get(name)
            .and_then(|wert| wert.as_str())
            .map(str::to_string)
    };
    let approve_url = text("approveUrl")?;
    let auth_token = text("authToken")?;
    let state = text("state")?;
    let client_id = text("clientId")?;
    let csrf_token = text("csrf_token")?;
    if [&approve_url, &auth_token, &state, &client_id, &csrf_token]
        .iter()
        .any(|wert| wert.trim().is_empty())
    {
        return None;
    }
    let scopes = props
        .get("scopes")
        .and_then(|liste| liste.as_array())
        .map(|liste| {
            liste
                .iter()
                .filter_map(|scope| {
                    scope.as_str().map(str::to_string).or_else(|| {
                        scope
                            .get("id")
                            .and_then(|id| id.as_str())
                            .map(str::to_string)
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Some(Zustimmung {
        approve_url,
        client_id,
        client_name: text("clientName").unwrap_or_default(),
        scopes,
        state,
        auth_token,
        csrf_token,
    })
}

/// Die Seitendaten: `data-page` als Attribut (`<div id="app" data-page='…'>`)
/// oder als Inhalt eines `<script data-page type="application/json">`.
fn seitendaten(html: &str) -> Option<serde_json::Value> {
    let mut rest = html;
    while let Some(anfang) = rest.find('<') {
        rest = &rest[anfang..];
        let ende = tag_ende(rest)?;
        let roh = &rest[1..ende];
        rest = &rest[ende + 1..];
        if roh.starts_with('/') || roh.starts_with('!') {
            continue;
        }
        let (name, attribute) = zerlegen(roh);
        let Some((_, wert)) = attribute.iter().find(|(attr, _)| attr == "data-page") else {
            continue;
        };
        if !wert.trim().is_empty() {
            if let Ok(json) = serde_json::from_str(wert) {
                return Some(json);
            }
        }
        if name == "script" {
            let schluss = rest.to_ascii_lowercase().find("</script")?;
            return serde_json::from_str(rest[..schluss].trim()).ok();
        }
    }
    None
}

/// Wo endet ein Tag? Ein `>` in Anführungszeichen zählt nicht.
fn tag_ende(text: &str) -> Option<usize> {
    let mut quote: Option<u8> = None;
    for (stelle, byte) in text.bytes().enumerate().skip(1) {
        match (quote, byte) {
            (None, b'"' | b'\'') => quote = Some(byte),
            (Some(offen), _) if offen == byte => quote = None,
            (None, b'>') => return Some(stelle),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seite(json: &str) -> String {
        format!(
            "<html><body><script data-page=\"app\" type=\"application/json\">{json}</script>\
             <div id=\"app\"></div></body></html>"
        )
    }

    const DATEN: &str = r#"{"component":"OAuth/Authorize","props":{
        "approveUrl":"https://www.zep-online.de/m/next/oauth/authorize",
        "denyUrl":"https://www.zep-online.de/m/next/oauth/authorize",
        "authToken":"auth-geheim","state":"state-geheim","clientId":"c-1",
        "clientName":"quiver-zep","csrf_token":"csrf-geheim","scopes":["mcp:use"]}}"#;

    #[test]
    fn die_zustimmung_wird_aus_dem_skript_gelesen() {
        let zustimmung = zustimmung(&seite(DATEN)).expect("Zustimmung");
        assert_eq!(
            zustimmung.approve_url,
            "https://www.zep-online.de/m/next/oauth/authorize"
        );
        assert_eq!(zustimmung.client_id, "c-1");
        assert_eq!(zustimmung.client_name, "quiver-zep");
        assert_eq!(zustimmung.scopes, vec!["mcp:use".to_string()]);
        let rumpf = zustimmung.rumpf();
        let namen: Vec<&str> = rumpf.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(namen, ["_token", "state", "client_id", "auth_token"]);
        assert!(rumpf.contains(&("auth_token".to_string(), "auth-geheim".to_string())));
        // Debug zeigt keine Geheimnisse.
        let text = format!("{zustimmung:?}");
        assert!(!text.contains("geheim"), "{text}");
    }

    #[test]
    fn data_page_als_attribut_geht_auch() {
        let html = format!(
            "<div id=\"app\" data-page='{}'></div>",
            DATEN.replace('\n', " ")
        );
        assert!(zustimmung(&html).is_some());
    }

    #[test]
    fn ohne_approve_url_ist_es_keine_zustimmung() {
        let ohne = DATEN.replace("approveUrl", "andereUrl");
        assert!(zustimmung(&seite(&ohne)).is_none());
        assert!(zustimmung("<html><body><form></form></body></html>").is_none());
        assert!(zustimmung(&seite("kein json")).is_none());
    }
}
