// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der Browser-Ablauf (SPEC.md §2): Loopback-Lauscher, PKCE (S256), `state`,
//! Code-Tausch. Hier steht keine Politik – nur der eine Vorgang.

use std::borrow::Cow;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Query, State};
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use oauth2::basic::{BasicClient, BasicTokenResponse};
use oauth2::{
    AuthType, AuthUrl, AuthorizationCode, ClientId, CsrfToken, EndpointNotSet, EndpointSet,
    PkceCodeChallenge, RedirectUrl, RequestTokenError, Scope, TokenResponse, TokenUrl,
};
use rand::RngCore;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::time::Instant;

use crate::{Browser, Konfig};

/// Fertig konfigurierter Client: Autorisierung und Token sind gesetzt.
pub(crate) type OauthClient =
    BasicClient<EndpointSet, EndpointNotSet, EndpointNotSet, EndpointNotSet, EndpointSet>;

/// Was ein Anmeldevorgang liefert. Enthält Geheimnisse → `Debug` ist von Hand.
#[derive(Clone)]
pub(crate) struct Tokenpaar {
    pub(crate) access: String,
    pub(crate) refresh: Option<String>,
    pub(crate) ablauf: Instant,
}

impl Tokenpaar {
    /// Ein Token gilt 60 s vor seinem Ablauf schon als abgelaufen (SPEC.md §2).
    pub(crate) const VORLAUF: Duration = Duration::from_secs(60);

    pub(crate) fn gilt_noch(&self) -> bool {
        Self::gueltig_bis(self.ablauf, Instant::now())
    }

    /// Reine Regel, damit sie ohne Uhr prüfbar ist.
    pub(crate) fn gueltig_bis(ablauf: Instant, jetzt: Instant) -> bool {
        jetzt + Self::VORLAUF < ablauf
    }
}

impl std::fmt::Debug for Tokenpaar {
    /// Nie Token-Werte, auch nicht in einer Panik oder einem Log.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tokenpaar")
            .field("access", &"<geheim>")
            .field(
                "refresh",
                &if self.refresh.is_some() {
                    "<geheim>"
                } else {
                    "<keins>"
                },
            )
            .field("ablauf", &self.ablauf)
            .finish()
    }
}

/// Warum ein Vorgang nicht zu einem Token geführt hat. Nur Text, nie Werte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FlussFehler {
    /// `state` aus der Rückleitung passt nicht zu dem, den wir erzeugt haben.
    StateFalsch,
    /// Der Anmeldedienst hat abgelehnt (Text des Dienstes).
    Abgelehnt(String),
    /// Der Lauscher war vor der Rückleitung fertig (`lauscher_hoechstens`).
    LauscherAbgelaufen,
    /// Das Refresh-Token ist widerrufen oder abgelaufen (`invalid_grant`).
    RefreshUngueltig,
    /// Alles andere, als Text.
    Sonstiges(String),
}

pub(crate) type Ergebnis = Result<Tokenpaar, FlussFehler>;

/// Baut den OAuth-Client: öffentlicher Client (kein Secret), `client_id` im Body,
/// damit Microsoft zufrieden ist.
pub(crate) fn client_bauen(konfig: &Konfig) -> anyhow::Result<OauthClient> {
    let basis = konfig.authority.trim_end_matches('/');
    let tenant = konfig.tenant.trim_matches('/');

    Ok(BasicClient::new(ClientId::new(konfig.client_id.clone()))
        .set_auth_type(AuthType::RequestBody)
        .set_auth_uri(AuthUrl::new(format!(
            "{basis}/{tenant}/oauth2/v2.0/authorize"
        ))?)
        .set_token_uri(TokenUrl::new(format!(
            "{basis}/{tenant}/oauth2/v2.0/token"
        ))?))
}

/// Überträgt eine Token-Antwort in ein [`Tokenpaar`].
pub(crate) fn aus_antwort(antwort: &BasicTokenResponse) -> anyhow::Result<Tokenpaar> {
    let access = antwort.access_token().secret().to_string();
    let refresh = antwort
        .refresh_token()
        .map(|refresh| refresh.secret().to_string());
    let gueltig = antwort
        .expires_in()
        .unwrap_or_else(|| Duration::from_secs(3600));

    Ok(Tokenpaar {
        access,
        refresh,
        ablauf: Instant::now() + gueltig,
    })
}

/// Der ganze Browser-Ablauf: Lauscher auf, URL öffnen, Rückleitung abwarten,
/// `state` prüfen, Code gegen Token tauschen.
pub(crate) async fn browser_anmeldung<B: Browser>(
    client: &OauthClient,
    http: &reqwest::Client,
    konfig: &Konfig,
    browser: &B,
) -> Ergebnis {
    let lauscher = TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|fehler| FlussFehler::Sonstiges(format!("Lauscher startet nicht: {fehler}")))?;
    let port = lauscher
        .local_addr()
        .map_err(|fehler| FlussFehler::Sonstiges(format!("Lauscher ohne Adresse: {fehler}")))?
        .port();
    let umleitung = RedirectUrl::new(format!("http://localhost:{port}"))
        .map_err(|fehler| FlussFehler::Sonstiges(format!("Umleitung ungültig: {fehler}")))?;

    let state = zufalls_state();
    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();

    let (auth_url, _csrf) = client
        .authorize_url(|| CsrfToken::new(state.clone()))
        .add_scopes(konfig.scopes.iter().cloned().map(Scope::new))
        .add_extra_param("prompt", "select_account")
        .set_pkce_challenge(challenge)
        .set_redirect_uri(Cow::Owned(umleitung.clone()))
        .url();

    let (melden, empfangen) = oneshot::channel::<Rueckmeldung>();
    let kanal: Kanal = Arc::new(Mutex::new(Some(melden)));
    let app = Router::new()
        .route("/", get(rueckmeldung))
        .with_state(kanal);

    let (halt_senden, halt_empfangen) = oneshot::channel::<()>();
    let mut bedienung = tokio::spawn(async move {
        let _ = axum::serve(lauscher, app)
            .with_graceful_shutdown(async move {
                let _ = halt_empfangen.await;
            })
            .await;
    });

    browser.oeffnen(auth_url.as_str()).map_err(|fehler| {
        FlussFehler::Sonstiges(format!("Browser ließ sich nicht öffnen: {fehler}"))
    })?;
    tracing::info!("Browser geöffnet");

    let eingang = tokio::time::timeout(konfig.lauscher_hoechstens, empfangen).await;
    // Lauscher schließen; die letzte Antwort darf noch rausgehen.
    let _ = halt_senden.send(());
    match tokio::time::timeout(Duration::from_secs(3), &mut bedienung).await {
        Ok(_) => {}
        Err(_) => bedienung.abort(),
    }

    let felder = match eingang {
        Ok(Ok(felder)) => felder,
        Ok(Err(_)) => {
            return Err(FlussFehler::Sonstiges(
                "Lauscher ohne Antwort beendet".to_string(),
            ))
        }
        Err(_) => return Err(FlussFehler::LauscherAbgelaufen),
    };

    // `state` zuerst: passt er nicht, ist der Vorgang verworfen.
    match felder.state.as_deref() {
        Some(rueck) if gleich_konstant(rueck.as_bytes(), state.as_bytes()) => {}
        _ => return Err(FlussFehler::StateFalsch),
    }

    if let Some(fehler) = felder.error {
        let text = felder.error_description.unwrap_or(fehler);
        return Err(FlussFehler::Abgelehnt(text));
    }

    let Some(code) = felder.code else {
        return Err(FlussFehler::Sonstiges("Rückleitung ohne Code".to_string()));
    };

    let antwort = client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(verifier)
        .set_redirect_uri(Cow::Owned(umleitung))
        .request_async(http)
        .await;

    match antwort {
        Ok(antwort) => aus_antwort(&antwort).map_err(|fehler| {
            FlussFehler::Sonstiges(format!("Token-Antwort unbrauchbar: {fehler}"))
        }),
        Err(fehler) => Err(match fehler {
            // Der Text des Dienstes enthält keinen Token.
            RequestTokenError::ServerResponse(_) => FlussFehler::Abgelehnt(format!("{fehler}")),
            _ => FlussFehler::Sonstiges(format!("{fehler}")),
        }),
    }
}

type Kanal = Arc<Mutex<Option<oneshot::Sender<Rueckmeldung>>>>;

/// Was der Anmeldedienst zurückleitet (RFC 6749 §4.1.2).
#[derive(Debug, Default, serde::Deserialize)]
pub(crate) struct Rueckmeldung {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

/// Die eine Seite, die der User im Browser sieht – ohne Code, ohne Token.
async fn rueckmeldung(
    State(kanal): State<Kanal>,
    Query(felder): Query<Rueckmeldung>,
) -> Html<String> {
    let meldung = match (&felder.error, felder.code.is_some()) {
        (Some(_), _) => format!(
            "<p>Anmeldung fehlgeschlagen: {}</p><p>Du kannst dieses Fenster schließen.</p>",
            escape(felder.error_description.as_deref().unwrap_or("ohne Angabe"))
        ),
        (None, true) => "<p>Angemeldet – du kannst dieses Fenster schließen.</p>".to_string(),
        (None, false) => "<p>Keine Antwort vom Anmeldedienst.</p>".to_string(),
    };

    let sender = kanal.lock().ok().and_then(|mut fach| fach.take());
    if let Some(sender) = sender {
        let _ = sender.send(felder);
    }

    Html(format!(
        "<!doctype html><html lang=\"de\"><head><meta charset=\"utf-8\">\
         <title>quiver-m365 – Anmeldung</title></head><body>{meldung}</body></html>"
    ))
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

/// Vergleich ohne Zeitkanal: gleiche Länge und kein Byte-Unterschied.
fn gleich_konstant(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut unterschied = 0u8;
    for (x, y) in a.iter().zip(b) {
        unterschied |= x ^ y;
    }
    unterschied == 0
}

/// `state` (CSRF-Schutz): 32 zufällige Bytes als Hex.
fn zufalls_state() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(text, "{byte:02x}");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn token_gilt_bis_vorlauf() {
        let jetzt = Instant::now();
        let ablauf = jetzt + Duration::from_secs(3600);
        assert!(Tokenpaar::gueltig_bis(ablauf, jetzt));
        // Kurz vor Ablauf: schon abgelaufen (60 s Vorlauf).
        assert!(!Tokenpaar::gueltig_bis(
            ablauf,
            ablauf - Duration::from_secs(30)
        ));
        assert!(!Tokenpaar::gueltig_bis(ablauf, ablauf - Tokenpaar::VORLAUF));
        assert!(Tokenpaar::gueltig_bis(
            ablauf,
            ablauf - Tokenpaar::VORLAUF - Duration::from_secs(1)
        ));
    }

    #[test]
    fn state_ist_zufaellig_und_lang() {
        let a = zufalls_state();
        let b = zufalls_state();
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|zeichen| zeichen.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn vergleich_ohne_zeitkanal() {
        assert!(gleich_konstant(b"abc", b"abc"));
        assert!(!gleich_konstant(b"abc", b"abd"));
        assert!(!gleich_konstant(b"abc", b"ab"));
    }

    #[test]
    fn debug_zeigt_keine_token_werte() {
        let paar = Tokenpaar {
            access: "geheimer-access".to_string(),
            refresh: Some("geheimer-refresh".to_string()),
            ablauf: Instant::now(),
        };
        let text = format!("{paar:?}");
        assert!(!text.contains("geheimer-access"));
        assert!(!text.contains("geheimer-refresh"));
    }
}
