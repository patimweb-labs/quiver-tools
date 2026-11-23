// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Attrappe für Microsoft Entra ID (SPEC.md §6): `/authorize`, `/token` und
//! `/adminconsent`. Wird von den Tests als Modul genutzt; nie echte Microsoft-Server.
#![allow(dead_code)]

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Form, Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use m365_anmeldung::{Browser, Konfig, STANDARD_SCOPES};
use serde::Deserialize;
use serde_json::json;

pub const TENANT: &str = "test-tenant";
pub const CLIENT_ID: &str = "test-client";
pub const SCHLUESSEL: &str = "test-tenant/test-client";
/// Der Code, den die Attrappe ausstellt (Tests dürfen ihn kennen).
pub const CODE: &str = "c1";

/// Konfiguration für eine Attrappe auf einer festen Adresse.
pub fn konfig(attrappe: &Attrappe) -> Konfig {
    Konfig {
        client_id: CLIENT_ID.to_string(),
        tenant: TENANT.to_string(),
        scopes: STANDARD_SCOPES.iter().map(|s| s.to_string()).collect(),
        authority: attrappe.adresse.clone(),
        warten: Duration::from_secs(5),
        lauscher_hoechstens: Duration::from_secs(30),
    }
}

/// Der Anmeldedienst als Attrappe.
pub struct Attrappe {
    zustand: Arc<Zustand>,
    /// z. B. `http://127.0.0.1:52341`
    pub adresse: String,
    bedienung: tokio::task::JoinHandle<()>,
}

struct Zustand {
    /// `code_challenge` aus dem letzten `/authorize` (PKCE S256).
    challenge: Mutex<Option<String>>,
    methode: Mutex<Option<String>>,
    autorisierungen: AtomicUsize,
    token_aufrufe: AtomicUsize,
    refresh_aufrufe: AtomicUsize,
    ausgestellt: AtomicUsize,
    gueltigkeit: AtomicU64,
    widerrufen: Mutex<HashSet<String>>,
    /// Wenn gesetzt, antwortet `/token` immer mit `invalid_request`.
    fehler: AtomicBool,
    /// Die letzte Anfrage an `/adminconsent` (nur die Felder, die der Test prüft).
    freigabe: Mutex<Option<FreigabeAnfrage>>,
    freigabe_abgelehnt: AtomicBool,
}

impl Zustand {
    fn neu() -> Self {
        Self {
            challenge: Mutex::new(None),
            methode: Mutex::new(None),
            autorisierungen: AtomicUsize::new(0),
            token_aufrufe: AtomicUsize::new(0),
            refresh_aufrufe: AtomicUsize::new(0),
            ausgestellt: AtomicUsize::new(0),
            gueltigkeit: AtomicU64::new(3600),
            widerrufen: Mutex::new(HashSet::new()),
            fehler: AtomicBool::new(false),
            freigabe: Mutex::new(None),
            freigabe_abgelehnt: AtomicBool::new(false),
        }
    }

    fn fach<T>(fach: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
        fach.lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner())
    }

    /// Neues Paar ausstellen; Zähler läuft hoch (Rotation sichtbar).
    fn ausstellen(&self) -> serde_json::Value {
        let nummer = self.ausgestellt.fetch_add(1, Ordering::SeqCst) + 1;
        let access = format!("at-{nummer}");
        let refresh = format!("rt-{nummer}");
        json!({
            "token_type": "Bearer",
            "access_token": access,
            "refresh_token": refresh,
            "expires_in": self.gueltigkeit.load(Ordering::SeqCst),
            "scope": "User.Read Mail.ReadWrite Mail.Send Calendars.ReadWrite Files.Read.All offline_access",
        })
    }
}

impl Attrappe {
    pub async fn starten() -> Self {
        let zustand = Arc::new(Zustand::neu());
        let app = Router::new()
            .route("/{tenant}/oauth2/v2.0/authorize", get(autorisieren))
            .route("/{tenant}/oauth2/v2.0/token", post(token))
            .route("/{tenant}/v2.0/adminconsent", get(adminconsent))
            .with_state(Arc::clone(&zustand));

        let lauscher = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("Attrappe bindet");
        let port = lauscher.local_addr().expect("Attrappe hat Adresse").port();
        let bedienung = tokio::spawn(async move {
            let _ = axum::serve(lauscher, app).await;
        });

        Self {
            zustand,
            adresse: format!("http://127.0.0.1:{port}"),
            bedienung,
        }
    }

    pub fn autorisierungen(&self) -> usize {
        self.zustand.autorisierungen.load(Ordering::SeqCst)
    }

    pub fn token_aufrufe(&self) -> usize {
        self.zustand.token_aufrufe.load(Ordering::SeqCst)
    }

    pub fn refresh_aufrufe(&self) -> usize {
        self.zustand.refresh_aufrufe.load(Ordering::SeqCst)
    }

    /// Gültigkeitsdauer, die `/token` künftig meldet (Sekunden).
    pub fn setze_gueltigkeit(&self, sekunden: u64) {
        self.zustand.gueltigkeit.store(sekunden, Ordering::SeqCst);
    }

    /// Markiert ein Refresh-Token als widerrufen → `/token` antwortet `invalid_grant`.
    pub fn widerrufen(&self, refresh: &str) {
        Zustand::fach(&self.zustand.widerrufen).insert(refresh.to_string());
    }

    /// `/token` antwortet ab jetzt immer mit einem Fehler.
    pub fn fehler_im_token_endpunkt(&self) {
        self.zustand.fehler.store(true, Ordering::SeqCst);
    }

    /// Die letzte Anfrage an `/adminconsent`, falls eine kam.
    pub fn freigabe_anfrage(&self) -> Option<FreigabeAnfrage> {
        Zustand::fach(&self.zustand.freigabe).clone()
    }

    /// `/adminconsent` leitet mit `?error=access_denied` zurück (Admin lehnt ab).
    pub fn freigabe_ablehnen(&self) {
        self.zustand
            .freigabe_abgelehnt
            .store(true, Ordering::SeqCst);
    }
}

impl Drop for Attrappe {
    fn drop(&mut self) {
        self.bedienung.abort();
    }
}

#[derive(Debug, Deserialize)]
pub struct Autorisierung {
    client_id: Option<String>,
    redirect_uri: Option<String>,
    state: Option<String>,
    code_challenge: Option<String>,
    code_challenge_method: Option<String>,
    response_type: Option<String>,
    prompt: Option<String>,
}

async fn autorisieren(
    State(zustand): State<Arc<Zustand>>,
    Path(tenant): Path<String>,
    Query(anfrage): Query<Autorisierung>,
) -> Response {
    zustand.autorisierungen.fetch_add(1, Ordering::SeqCst);

    if tenant != TENANT {
        return (StatusCode::NOT_FOUND, "unbekannter Mandant").into_response();
    }
    if anfrage.client_id.as_deref() != Some(CLIENT_ID) {
        return (StatusCode::BAD_REQUEST, "client_id falsch").into_response();
    }
    if anfrage.response_type.as_deref() != Some("code") {
        return (StatusCode::BAD_REQUEST, "response_type fehlt").into_response();
    }
    if anfrage.code_challenge_method.as_deref() != Some("S256") {
        return (
            StatusCode::BAD_REQUEST,
            "code_challenge_method ist nicht S256",
        )
            .into_response();
    }
    if anfrage.prompt.as_deref() != Some("select_account") {
        return (StatusCode::BAD_REQUEST, "prompt fehlt").into_response();
    }
    let (Some(challenge), Some(state), Some(umleitung)) = (
        anfrage.code_challenge.clone(),
        anfrage.state.clone(),
        anfrage.redirect_uri.clone(),
    ) else {
        return (
            StatusCode::BAD_REQUEST,
            "code_challenge, state oder redirect_uri fehlt",
        )
            .into_response();
    };

    *Zustand::fach(&zustand.challenge) = Some(challenge);
    *Zustand::fach(&zustand.methode) = anfrage.code_challenge_method;

    let ziel = format!("{umleitung}?code={CODE}&state={state}");
    Redirect::temporary(&ziel).into_response()
}

#[derive(Debug, Deserialize)]
pub struct TokenAnfrage {
    grant_type: String,
    code: Option<String>,
    code_verifier: Option<String>,
    refresh_token: Option<String>,
    client_id: Option<String>,
    redirect_uri: Option<String>,
}

fn token_fehler(code: &str, text: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({"error": code, "error_description": text})),
    )
        .into_response()
}

async fn token(
    State(zustand): State<Arc<Zustand>>,
    Path(tenant): Path<String>,
    Form(anfrage): Form<TokenAnfrage>,
) -> Response {
    zustand.token_aufrufe.fetch_add(1, Ordering::SeqCst);

    if tenant != TENANT {
        return (StatusCode::NOT_FOUND, "unbekannter Mandant").into_response();
    }
    if anfrage.client_id.as_deref() != Some(CLIENT_ID) {
        return token_fehler("invalid_client", "client_id falsch");
    }
    if zustand.fehler.load(Ordering::SeqCst) {
        return token_fehler("invalid_request", "Der Code ist ungültig.");
    }

    match anfrage.grant_type.as_str() {
        "authorization_code" => {
            let (Some(code), Some(verifier), Some(umleitung)) = (
                anfrage.code.as_deref(),
                anfrage.code_verifier.as_deref(),
                anfrage.redirect_uri.as_deref(),
            ) else {
                return token_fehler("invalid_request", "code oder code_verifier fehlt");
            };
            if code != CODE {
                return token_fehler("invalid_grant", "Code unbekannt");
            }
            if !umleitung.starts_with("http://127.0.0.1:") {
                return token_fehler("invalid_request", "redirect_uri unerwartet");
            }
            let erwartet = Zustand::fach(&zustand.challenge).clone();
            let Some(erwartet) = erwartet else {
                return token_fehler("invalid_request", "keine code_challenge gemerkt");
            };
            if base64_url(&sha256(verifier.as_bytes())) != erwartet {
                return token_fehler("invalid_grant", "code_verifier passt nicht");
            }
            Json(zustand.ausstellen()).into_response()
        }
        "refresh_token" => {
            zustand.refresh_aufrufe.fetch_add(1, Ordering::SeqCst);
            let Some(refresh) = anfrage.refresh_token.as_deref() else {
                return token_fehler("invalid_request", "refresh_token fehlt");
            };
            if Zustand::fach(&zustand.widerrufen).contains(refresh) {
                return token_fehler("invalid_grant", "Refresh-Token widerrufen");
            }
            Json(zustand.ausstellen()).into_response()
        }
        anderes => token_fehler("unsupported_grant_type", anderes),
    }
}

// ---------------------------------------------------------------------------
// Admin-Freigabe (SPEC.md §2a): `quiver-m365 freigeben` öffnet
// `<authority>/<tenant>/v2.0/adminconsent`; die Attrappe leitet auf den
// Loopback zurück, den der Befehl selbst geöffnet hat.
// ---------------------------------------------------------------------------

/// Was `quiver-m365 freigeben` an `/adminconsent` schickt.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct FreigabeAnfrage {
    pub client_id: Option<String>,
    pub scope: Option<String>,
    pub redirect_uri: Option<String>,
    pub state: Option<String>,
}

async fn adminconsent(
    State(zustand): State<Arc<Zustand>>,
    Path(tenant): Path<String>,
    Query(anfrage): Query<FreigabeAnfrage>,
) -> Response {
    if tenant != TENANT {
        return (StatusCode::NOT_FOUND, "unbekannter Mandant").into_response();
    }
    if anfrage.client_id.as_deref() != Some(CLIENT_ID) {
        return (StatusCode::BAD_REQUEST, "client_id falsch").into_response();
    }
    let Some(umleitung) = anfrage.redirect_uri.clone() else {
        return (StatusCode::BAD_REQUEST, "redirect_uri fehlt").into_response();
    };
    if !umleitung.starts_with("http://127.0.0.1:") {
        return (StatusCode::BAD_REQUEST, "redirect_uri unerwartet").into_response();
    }
    let state = anfrage.state.clone().unwrap_or_default();
    *Zustand::fach(&zustand.freigabe) = Some(anfrage);

    let ziel = if zustand.freigabe_abgelehnt.load(Ordering::SeqCst) {
        format!(
            "{umleitung}?error=access_denied&error_description=Admin+hat+abgelehnt&state={state}"
        )
    } else {
        format!("{umleitung}?admin_consent=True&tenant={tenant}&state={state}")
    };
    Redirect::temporary(&ziel).into_response()
}

// ---------------------------------------------------------------------------
// Test-Browser: ruft die Authorize-URL auf und folgt der Weiterleitung zum
// Lauscher (ersetzt `localhost` durch `127.0.0.1`, damit der Test nicht von der
// Namensauflösung abhängt).
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct AttrappenBrowser {
    inner: Arc<BrowserInner>,
}

struct BrowserInner {
    http: reqwest::Client,
    aufrufe: AtomicUsize,
    verzoegerung: Duration,
    state_faelschen: Option<String>,
    stumm: bool,
}

impl AttrappenBrowser {
    /// Folgt der Weiterleitung sofort.
    pub fn neu() -> Self {
        Self::bauen(Duration::ZERO, None, false)
    }

    /// Kommt erst nach `verzoegerung` zurück.
    pub fn traege(verzoegerung: Duration) -> Self {
        Self::bauen(verzoegerung, None, false)
    }

    /// Schickt einen falschen `state` zurück.
    pub fn mit_state(state: &str) -> Self {
        Self::bauen(Duration::ZERO, Some(state.to_string()), false)
    }

    /// Ruft gar nichts auf („Browser kommt nie zurück“).
    pub fn stumm() -> Self {
        Self::bauen(Duration::ZERO, None, true)
    }

    fn bauen(verzoegerung: Duration, state_faelschen: Option<String>, stumm: bool) -> Self {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("Test-Client baut");
        Self {
            inner: Arc::new(BrowserInner {
                http,
                aufrufe: AtomicUsize::new(0),
                verzoegerung,
                state_faelschen,
                stumm,
            }),
        }
    }

    /// Wie oft der Browser geöffnet wurde.
    pub fn aufrufe(&self) -> usize {
        self.inner.aufrufe.load(Ordering::SeqCst)
    }
}

impl Browser for AttrappenBrowser {
    fn oeffnen(&self, url: &str) -> anyhow::Result<()> {
        self.inner.aufrufe.fetch_add(1, Ordering::SeqCst);
        if self.inner.stumm {
            return Ok(());
        }
        let inner = Arc::clone(&self.inner);
        let url = url.to_string();
        tokio::spawn(async move {
            let _ = inner.rufen(&url).await;
        });
        Ok(())
    }
}

impl BrowserInner {
    /// Authorize-URL abrufen, der Weiterleitung folgen, beim Lauscher landen.
    async fn rufen(&self, url: &str) -> anyhow::Result<()> {
        if !self.verzoegerung.is_zero() {
            tokio::time::sleep(self.verzoegerung).await;
        }
        let antwort = self.http.get(url).send().await?;
        let Some(ort) = antwort.headers().get(header::LOCATION) else {
            return Err(anyhow::anyhow!(
                "Attrappe hat nicht weitergeleitet ({})",
                antwort.status()
            ));
        };
        let ort = String::from_utf8_lossy(ort.as_bytes()).to_string();
        let mut ziel = url::Url::parse(&ort)?;

        if let Some(falsch) = &self.state_faelschen {
            let paare: Vec<(String, String)> = ziel
                .query_pairs()
                .map(|(name, wert)| {
                    let wert = if name == "state" {
                        falsch.clone()
                    } else {
                        wert.to_string()
                    };
                    (name.to_string(), wert)
                })
                .collect();
            let mut abfrage = String::new();
            for (name, wert) in paare {
                if !abfrage.is_empty() {
                    abfrage.push('&');
                }
                abfrage.push_str(&name);
                abfrage.push('=');
                abfrage.push_str(&wert);
            }
            ziel.set_query(Some(&abfrage));
        }

        ziel.set_host(Some("127.0.0.1"))?;
        let _ = self.http.get(ziel).send().await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// SHA-256 + base64url für die PKCE-Prüfung. Bewusst klein und ohne neue
// Abhängigkeiten (die Aufgabe erlaubt keine weiteren Crates).
// ---------------------------------------------------------------------------

#[allow(clippy::needless_range_loop)]
pub fn sha256(daten: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let mut nachricht = daten.to_vec();
    let bitlaenge = (daten.len() as u64).wrapping_mul(8);
    nachricht.push(0x80);
    while nachricht.len() % 64 != 56 {
        nachricht.push(0);
    }
    nachricht.extend_from_slice(&bitlaenge.to_be_bytes());

    for block in nachricht.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[4 * i],
                block[4 * i + 1],
                block[4 * i + 2],
                block[4 * i + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let (mut a, mut b, mut c, mut d) = (h[0], h[1], h[2], h[3]);
        let (mut e, mut f, mut g, mut hh) = (h[4], h[5], h[6], h[7]);

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut raus = [0u8; 32];
    for i in 0..8 {
        raus[4 * i..4 * i + 4].copy_from_slice(&h[i].to_be_bytes());
    }
    raus
}

/// base64url ohne Auffüllzeichen (RFC 7636 §A).
pub fn base64_url(daten: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut raus = String::with_capacity(daten.len() * 4 / 3 + 4);
    for stueck in daten.chunks(3) {
        let b0 = stueck[0] as u32;
        let b1 = stueck.get(1).copied().unwrap_or(0) as u32;
        let b2 = stueck.get(2).copied().unwrap_or(0) as u32;
        let breite = (b0 << 16) | (b1 << 8) | b2;
        raus.push(ALPHABET[((breite >> 18) & 63) as usize] as char);
        raus.push(ALPHABET[((breite >> 12) & 63) as usize] as char);
        if stueck.len() > 1 {
            raus.push(ALPHABET[((breite >> 6) & 63) as usize] as char);
        }
        if stueck.len() > 2 {
            raus.push(ALPHABET[(breite & 63) as usize] as char);
        }
    }
    raus
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(daten: &[u8]) -> String {
        daten.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn sha256_kennt_die_bekannten_vektoren() {
        assert_eq!(
            hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        // Über eine Blockgrenze hinaus (55/56/64/130 Bytes).
        for laenge in [55usize, 56, 64, 130] {
            let daten = vec![b'a'; laenge];
            assert_eq!(sha256(&daten).len(), 32);
        }
    }

    #[test]
    fn base64_url_ohne_auffuellung() {
        assert_eq!(base64_url(b""), "");
        assert_eq!(base64_url(b"f"), "Zg");
        assert_eq!(base64_url(b"fo"), "Zm8");
        assert_eq!(base64_url(b"foo"), "Zm9v");
        assert_eq!(
            base64_url(&sha256(b"abc")),
            "ungWv48Bz-pBQUDeXa4iI7ADYaOWF3qctBD_YfIAFa0"
        );
    }
}
