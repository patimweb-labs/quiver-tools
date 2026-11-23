// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die ZEP-Attrappe (Aufgabe 06, „Tests (nur Attrappen)").
//!
//! Ein kleines axum-Programm auf `127.0.0.1`, das den ZEP-MCP nachbildet:
//!
//! - Metadaten (`/.well-known/oauth-protected-resource/…`,
//!   `/.well-known/oauth-authorization-server/…`)
//! - Client-Registrierung (`/register`, DCR), Authorize mit Login-Formular
//!   (CSRF-Feld, Cookies) und Zustimmungsseite, Token (Code und Refresh, mit
//!   Rotation und `invalid_grant`)
//! - einen Mini-MCP-Server mit zwei Werkzeugen hinter `Authorization: Bearer`,
//!   der auf Wunsch `tools/list_changed` schickt.
//!
//! Kein Test spricht je mit zep-online.de. Passwort und Tokens sind erfunden.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Json, Router};
use quiver_zep::html::Seite;
use quiver_zep::{Browser, ZepKonfig};
use rmcp::handler::server::tool::schema_for_type;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};
use serde::Deserialize;
use serde_json::json;

/// Der Benutzer, den die Attrappe kennt.
pub const BENUTZER: &str = "ada";
/// Das Passwort, das die Attrappe akzeptiert. Kein echtes Geheimnis.
pub const PASSWORT: &str = "zep-test-4711";
/// Ein Wert, der in keiner Meldung auftauchen darf.
pub const GEHEIM_MARKE: &str = "geheim-4711-nie-zeigen";

/// Der Mandant in den Adressen.
pub const MANDANT: &str = "zepneoimpulse";

// -------------------------------------------------------------- Zustand

#[derive(Debug, Clone)]
struct Fluss {
    client_id: String,
    redirect_uri: String,
    state: String,
    challenge: String,
    scope: String,
    csrf: String,
    angemeldet: bool,
}

/// Eine ausgestellte Erlaubnis.
#[derive(Debug, Clone)]
struct Code {
    challenge: String,
    redirect_uri: String,
}

struct Zustand {
    /// Registrierte Client-Ids (DCR).
    clients: Mutex<BTreeSet<String>>,
    registrierungen: AtomicUsize,
    /// Laufende Authorize-Vorgänge.
    fluesse: Mutex<BTreeMap<String, Fluss>>,
    /// Ausgestellte Codes (einmal verwendbar).
    codes: Mutex<BTreeMap<String, Code>>,
    /// Sitzungs-Cookies nach erfolgreicher Anmeldung.
    sitzungen: Mutex<BTreeSet<String>>,
    /// Wie oft das Anmeldeformular mit Zugangsdaten abgeschickt wurde.
    login_posts: AtomicUsize,
    /// Wie oft die Zustimmung bestätigt wurde.
    zustimmungen: AtomicUsize,
    token_aufrufe: AtomicUsize,
    refresh_aufrufe: AtomicUsize,
    ausgestellt: AtomicU64,
    /// Gültigkeit der Access-Tokens in Sekunden.
    gueltigkeit: AtomicU64,
    /// Gültige (nicht widerrufene) Refresh-Tokens.
    refresh_tokens: Mutex<BTreeSet<String>>,
    /// Ausgestellte Access-Tokens (die der MCP akzeptiert).
    access_tokens: Mutex<BTreeSet<String>>,
    /// Nimmt die Attrappe das Passwort an?
    passwort_richtig: AtomicBool,
    /// Zusätzliches Werkzeug nach `zep_umschalten`?
    umgeschaltet: AtomicBool,
    /// Zeigt `/authorize` eine Seite, die die Routine nicht einordnen kann?
    unklare_seite: AtomicBool,
    /// Zeigt das Anmeldeformular auf einen fremden Host (Phishing-Versuch)?
    fremdes_formular: AtomicBool,
}

fn griff<T>(fach: &Mutex<T>) -> MutexGuard<'_, T> {
    fach.lock().unwrap_or_else(|gift| gift.into_inner())
}

impl Zustand {
    fn neu() -> Zustand {
        Zustand {
            clients: Mutex::new(BTreeSet::new()),
            registrierungen: AtomicUsize::new(0),
            fluesse: Mutex::new(BTreeMap::new()),
            codes: Mutex::new(BTreeMap::new()),
            sitzungen: Mutex::new(BTreeSet::new()),
            login_posts: AtomicUsize::new(0),
            zustimmungen: AtomicUsize::new(0),
            token_aufrufe: AtomicUsize::new(0),
            refresh_aufrufe: AtomicUsize::new(0),
            ausgestellt: AtomicU64::new(0),
            gueltigkeit: AtomicU64::new(3600),
            refresh_tokens: Mutex::new(BTreeSet::new()),
            access_tokens: Mutex::new(BTreeSet::new()),
            passwort_richtig: AtomicBool::new(true),
            umgeschaltet: AtomicBool::new(false),
            unklare_seite: AtomicBool::new(false),
            fremdes_formular: AtomicBool::new(false),
        }
    }

    /// Ein neues Paar ausstellen; die Nummer läuft hoch (Rotation sichtbar).
    fn ausstellen(&self) -> serde_json::Value {
        let nummer = self.ausgestellt.fetch_add(1, Ordering::SeqCst) + 1;
        let access = format!("at-{nummer}");
        let refresh = format!("rt-{nummer}");
        griff(&self.access_tokens).insert(access.clone());
        griff(&self.refresh_tokens).insert(refresh.clone());
        json!({
            "token_type": "Bearer",
            "access_token": access,
            "refresh_token": refresh,
            "expires_in": self.gueltigkeit.load(Ordering::SeqCst),
            "scope": "mcp:use",
        })
    }

    fn kennt_access(&self, token: &str) -> bool {
        griff(&self.access_tokens).contains(token)
    }

    fn werkzeugliste(&self) -> Vec<Tool> {
        let mut liste = vec![
            werkzeug("zep_projekte", "Projekte", "Listet die Projekte.", true),
            werkzeug("zep_zeiten", "Zeiten", "Listet gebuchte Zeiten.", true),
            werkzeug(
                "zep_umschalten",
                "Umschalten",
                "Ändert die Werkzeugliste der Attrappe (löst list_changed aus).",
                false,
            ),
        ];
        if self.umgeschaltet.load(Ordering::SeqCst) {
            liste.push(werkzeug(
                "zep_kunden",
                "Kunden",
                "Kommt erst nach dem Umschalten.",
                true,
            ));
        }
        liste
    }
}

fn werkzeug(name: &str, titel: &str, beschreibung: &str, nur_lesen: bool) -> Tool {
    Tool::new(
        name.to_string(),
        beschreibung.to_string(),
        schema_for_type::<LeerParam>(),
    )
    .with_title(titel.to_string())
    .with_annotations(ToolAnnotations::from_raw(
        Some(titel.to_string()),
        Some(nur_lesen),
        Some(!nur_lesen),
        Some(nur_lesen),
        Some(false),
    ))
}

#[derive(Debug, Default, serde::Deserialize, rmcp::schemars::JsonSchema)]
struct LeerParam {}

// -------------------------------------------------------------- Attrappe

/// Eine laufende Attrappe.
pub struct Attrappe {
    /// `http://127.0.0.1:<port>` – ohne Schrägstrich am Ende.
    pub adresse: String,
    zustand: Arc<Zustand>,
    ende: tokio::task::JoinHandle<()>,
}

impl Drop for Attrappe {
    fn drop(&mut self) {
        self.ende.abort();
    }
}

impl Attrappe {
    /// Die Attrappe auf einem freien Port starten.
    pub async fn starte() -> Attrappe {
        let zustand = Arc::new(Zustand::neu());
        let service: StreamableHttpService<Werkzeuge, LocalSessionManager> =
            StreamableHttpService::new(
                {
                    let zustand = Arc::clone(&zustand);
                    move || Ok(Werkzeuge::neu(Arc::clone(&zustand)))
                },
                Default::default(),
                StreamableHttpServerConfig::default(),
            );

        let mcp: Router<Arc<Zustand>> = Router::new()
            .nest_service("/{mandant}/next/mcp", service)
            .layer(axum::middleware::from_fn_with_state(
                Arc::clone(&zustand),
                nur_mit_bearer,
            ));

        let app: Router = Router::new()
            .route(
                "/.well-known/oauth-protected-resource/{mandant}/next/mcp",
                get(geschuetzte_ressource),
            )
            .route(
                "/.well-known/oauth-authorization-server/{mandant}/next",
                get(server_metadaten),
            )
            .route("/{mandant}/next/oauth/register", post(registrieren))
            .route("/{mandant}/next/oauth/authorize", get(autorisieren))
            .route("/{mandant}/next/oauth/anmelden", post(anmelden))
            .route(
                "/{mandant}/next/oauth/zustimmen",
                get(zustimmungsseite).post(zustimmen),
            )
            .route("/{mandant}/next/oauth/token", post(token))
            .merge(mcp)
            .with_state(Arc::clone(&zustand));

        let lauscher =
            tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
                .await
                .expect("Attrappe bindet");
        let adresse = format!("http://{}", lauscher.local_addr().unwrap());
        let ende = tokio::spawn(async move {
            let _ = axum::serve(lauscher, app).await;
        });

        Attrappe {
            adresse,
            zustand,
            ende,
        }
    }

    /// Die MCP-Adresse, wie der User sie im quiver-Fenster einträgt.
    pub fn mcp_adresse(&self) -> String {
        format!("{}/{MANDANT}/next/mcp", self.adresse)
    }

    /// Eine Konfiguration für dieses Werkzeug (eigener Zustandsordner).
    pub fn konfig(&self, zustand: std::path::PathBuf, takt: std::time::Duration) -> ZepKonfig {
        ZepKonfig {
            url: self.mcp_adresse(),
            benutzer: BENUTZER.to_string(),
            passwort: PASSWORT.to_string(),
            zustand,
            takt,
            wartezeit: std::time::Duration::from_secs(10),
        }
    }

    pub fn registrierungen(&self) -> usize {
        self.zustand.registrierungen.load(Ordering::SeqCst)
    }

    pub fn login_posts(&self) -> usize {
        self.zustand.login_posts.load(Ordering::SeqCst)
    }

    pub fn zustimmungen(&self) -> usize {
        self.zustand.zustimmungen.load(Ordering::SeqCst)
    }

    pub fn token_aufrufe(&self) -> usize {
        self.zustand.token_aufrufe.load(Ordering::SeqCst)
    }

    pub fn refresh_aufrufe(&self) -> usize {
        self.zustand.refresh_aufrufe.load(Ordering::SeqCst)
    }

    /// Gültigkeitsdauer der künftigen Access-Tokens (Sekunden).
    pub fn setze_gueltigkeit(&self, sekunden: u64) {
        self.zustand.gueltigkeit.store(sekunden, Ordering::SeqCst);
    }

    /// `/authorize` antwortet mit einer Seite, die zu keinem Schritt passt.
    pub fn unklare_seite_zeigen(&self) {
        self.zustand.unklare_seite.store(true, Ordering::SeqCst);
    }

    /// Das Anmeldeformular zeigt auf einen fremden Host.
    pub fn fremdes_formular_zeigen(&self) {
        self.zustand.fremdes_formular.store(true, Ordering::SeqCst);
    }

    /// Alle ausgestellten Access-Tokens entwerten (der MCP antwortet 401).
    pub fn access_tokens_entwerten(&self) {
        griff(&self.zustand.access_tokens).clear();
    }

    /// Das Passwort wird ab jetzt abgelehnt.
    pub fn setze_passwort_richtig(&self, richtig: bool) {
        self.zustand
            .passwort_richtig
            .store(richtig, Ordering::SeqCst);
    }

    /// Ein Refresh-Token widerrufen → `/token` antwortet `invalid_grant`.
    pub fn widerrufen(&self, refresh: &str) {
        griff(&self.zustand.refresh_tokens).remove(refresh);
    }

    /// Der gültige Access-Token-Zustand (letzter ausgestellter Stand).
    pub fn access_tokens(&self) -> Vec<String> {
        griff(&self.zustand.access_tokens).iter().cloned().collect()
    }
}

// ------------------------------------------------------------- Bearer

async fn nur_mit_bearer(
    State(zustand): State<Arc<Zustand>>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|wert| wert.to_str().ok())
        .and_then(|wert| wert.strip_prefix("Bearer "))
        .map(str::to_string);

    match token {
        Some(token) if zustand.kennt_access(&token) => next.run(request).await,
        _ => (
            StatusCode::UNAUTHORIZED,
            [(
                header::WWW_AUTHENTICATE,
                format!(
                    "Bearer resource_metadata=\"/.well-known/oauth-protected-resource/{MANDANT}/next/mcp\""
                ),
            )],
            "Anmeldung nötig",
        )
            .into_response(),
    }
}

// ---------------------------------------------------------- Mini-MCP

/// Der Mini-MCP-Server der Attrappe: drei Werkzeuge, davon eines, das die
/// Werkzeugliste ändert.
#[derive(Clone)]
struct Werkzeuge {
    zustand: Arc<Zustand>,
}

impl Werkzeuge {
    fn neu(zustand: Arc<Zustand>) -> Werkzeuge {
        Werkzeuge { zustand }
    }
}

impl ServerHandler for Werkzeuge {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_tool_list_changed()
                .build(),
        )
        .with_server_info(Implementation::new("zep-attrappe", "0.0.0").with_title("ZEP-Attrappe"))
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(
            self.zustand.werkzeugliste(),
        ))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let antwort = match request.name.as_ref() {
            "zep_projekte" => "Projekte der Attrappe: Neoimpulse, Quiver.",
            "zep_zeiten" => "Zeiten der Attrappe: 2 Buchungen.",
            "zep_kunden" => "Kunden der Attrappe: niemand.",
            "zep_umschalten" => {
                self.zustand.umgeschaltet.store(true, Ordering::SeqCst);
                if let Err(fehler) = context.peer.notify_tool_list_changed().await {
                    tracing::warn!(%fehler, "list_changed der Attrappe nicht gesendet");
                }
                "Umgeschaltet: die Werkzeugliste hat jetzt zep_kunden."
            }
            sonst => {
                return Err(McpError::invalid_params(
                    format!("Unbekanntes Werkzeug „{sonst}“."),
                    None,
                ));
            }
        };
        Ok(CallToolResult::success(vec![ContentBlock::text(antwort)]).into())
    }
}

// ---------------------------------------------------------- Metadaten

async fn geschuetzte_ressource(
    State(_zustand): State<Arc<Zustand>>,
    kopf: HeaderMap,
    axum::extract::OriginalUri(uri): axum::extract::OriginalUri,
) -> Response {
    let basis = basis(&kopf);
    Json(json!({
        "resource": format!("{basis}{}", uri.path().trim_start_matches("/.well-known/oauth-protected-resource")),
        "authorization_servers": [format!("{basis}/{MANDANT}/next")],
        "scopes_supported": ["mcp:use"],
        "bearer_methods_supported": ["header"],
    }))
    .into_response()
}

async fn server_metadaten(State(_zustand): State<Arc<Zustand>>, kopf: HeaderMap) -> Response {
    let basis = basis(&kopf);
    Json(json!({
        "issuer": format!("{basis}/{MANDANT}/next"),
        "authorization_endpoint": format!("{basis}/{MANDANT}/next/oauth/authorize"),
        "token_endpoint": format!("{basis}/{MANDANT}/next/oauth/token"),
        "registration_endpoint": format!("{basis}/{MANDANT}/next/oauth/register"),
        "scopes_supported": ["mcp:use"],
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
    }))
    .into_response()
}

// ---------------------------------------------------- Registrierung (DCR)

#[derive(Deserialize)]
struct Registrierung {
    #[serde(default)]
    client_name: Option<String>,
    #[serde(default)]
    redirect_uris: Vec<String>,
    #[serde(default)]
    token_endpoint_auth_method: Option<String>,
}

async fn registrieren(
    State(zustand): State<Arc<Zustand>>,
    Json(anfrage): Json<Registrierung>,
) -> Response {
    if anfrage.client_name.as_deref() != Some("quiver-zep") {
        return (StatusCode::BAD_REQUEST, "client_name fehlt").into_response();
    }
    if anfrage.token_endpoint_auth_method.as_deref() != Some("none") {
        return (StatusCode::BAD_REQUEST, "token_endpoint_auth_method").into_response();
    }
    // Die Rückleitung muss ein Loopback sein – mit oder ohne Port.
    if !anfrage
        .redirect_uris
        .iter()
        .any(|uri| uri.starts_with("http://127.0.0.1"))
    {
        return (StatusCode::BAD_REQUEST, "redirect_uris").into_response();
    }

    let nummer = zustand.registrierungen.fetch_add(1, Ordering::SeqCst) + 1;
    let client_id = format!("quiver-zep-{nummer}");
    griff(&zustand.clients).insert(client_id.clone());
    Json(json!({
        "client_id": client_id,
        "client_name": "quiver-zep",
        "redirect_uris": anfrage.redirect_uris,
        "token_endpoint_auth_method": "none",
    }))
    .into_response()
}

// ------------------------------------------------------------ Authorize

#[derive(Debug, Deserialize)]
struct Autorisierung {
    response_type: Option<String>,
    client_id: Option<String>,
    redirect_uri: Option<String>,
    state: Option<String>,
    code_challenge: Option<String>,
    code_challenge_method: Option<String>,
    scope: Option<String>,
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

fn seite_html(titel: &str, inhalt: &str) -> Response {
    Html(format!(
        "<!doctype html><html lang=\"de\"><head><meta charset=\"utf-8\">\
         <title>{titel}</title></head><body><h1>{titel}</h1>{inhalt}</body></html>"
    ))
    .into_response()
}

/// Der Host aus dem `Host`-Kopf.
fn basis(kopf: &HeaderMap) -> String {
    let host = kopf
        .get(header::HOST)
        .and_then(|wert| wert.to_str().ok())
        .unwrap_or("127.0.0.1:0");
    format!("http://{host}")
}

async fn autorisieren(
    State(zustand): State<Arc<Zustand>>,
    kopf: HeaderMap,
    Query(anfrage): Query<Autorisierung>,
) -> Response {
    if anfrage.response_type.as_deref() != Some("code") {
        return (StatusCode::BAD_REQUEST, "response_type").into_response();
    }
    if anfrage.code_challenge_method.as_deref() != Some("S256") {
        return (StatusCode::BAD_REQUEST, "code_challenge_method").into_response();
    }
    let (Some(client_id), Some(redirect_uri), Some(state), Some(challenge)) = (
        anfrage.client_id.clone(),
        anfrage.redirect_uri.clone(),
        anfrage.state.clone(),
        anfrage.code_challenge.clone(),
    ) else {
        return (StatusCode::BAD_REQUEST, "Parameter fehlen").into_response();
    };
    if !griff(&zustand.clients).contains(&client_id) {
        return (StatusCode::BAD_REQUEST, "client_id unbekannt").into_response();
    }

    if zustand.unklare_seite.load(Ordering::SeqCst) {
        return unklare_html();
    }

    let kennung = format!("f{}", griff(&zustand.fluesse).len() + 1);
    let csrf = format!("csrf-{kennung}");
    let fluss = Fluss {
        client_id,
        redirect_uri,
        state,
        challenge,
        scope: anfrage.scope.unwrap_or_else(|| "mcp:use".to_string()),
        csrf: csrf.clone(),
        angemeldet: angemeldet(&zustand, &kopf),
    };
    griff(&zustand.fluesse).insert(kennung.clone(), fluss);

    if griff(&zustand.fluesse)[&kennung].angemeldet {
        return zustimmungs_html(&basis(&kopf), &kennung, &csrf);
    }
    let basis = if zustand.fremdes_formular.load(Ordering::SeqCst) {
        "https://boese.example.test".to_string()
    } else {
        basis(&kopf)
    };
    anmelde_html(&basis, &kennung, &csrf)
}

fn angemeldet(zustand: &Arc<Zustand>, kopf: &HeaderMap) -> bool {
    let kopf = kopf
        .get(header::COOKIE)
        .and_then(|wert| wert.to_str().ok())
        .unwrap_or("");
    kopf.split(';')
        .filter_map(|paar| paar.trim().split_once('='))
        .any(|(name, wert)| name == "sitzung" && griff(&zustand.sitzungen).contains(wert))
}

fn anmelde_html(basis: &str, fluss: &str, csrf: &str) -> Response {
    seite_html(
        "Anmelden",
        &format!(
            "<p>Bitte melden Sie sich an.</p>\
             <form method=\"post\" action=\"{basis}/{MANDANT}/next/oauth/anmelden\">\
             <input type=\"hidden\" name=\"fluss\" value=\"{fluss}\">\
             <input type=\"hidden\" name=\"csrf\" value=\"{csrf}\">\
             <label>Benutzer <input type=\"text\" name=\"benutzer\"></label>\
             <label>Passwort <input type=\"password\" name=\"passwort\"></label>\
             <button type=\"submit\" name=\"weiter\" value=\"1\">Anmelden</button>\
             </form>",
            basis = escape(basis),
            fluss = escape(fluss),
            csrf = escape(csrf),
        ),
    )
}

fn zustimmungs_html(basis: &str, fluss: &str, csrf: &str) -> Response {
    seite_html(
        "Zugriff erlauben",
        &format!(
            "<p>quiver-zep möchte den ZEP-MCP benutzen.</p>\
             <form method=\"post\" action=\"{basis}/{MANDANT}/next/oauth/zustimmen\">\
             <input type=\"hidden\" name=\"fluss\" value=\"{fluss}\">\
             <input type=\"hidden\" name=\"csrf\" value=\"{csrf}\">\
             <button type=\"submit\" name=\"ok\" value=\"1\">OK</button>\
             </form>",
            basis = escape(basis),
            fluss = escape(fluss),
            csrf = escape(csrf),
        ),
    )
}

/// Die Seite, die die Routine nicht einordnen kann (unbekannter Aufbau).
fn unklare_html() -> Response {
    seite_html(
        "Zwischenschritt",
        &format!(
            "<p>Bitte wählen Sie eine Sprache.</p>\
             <form method=\"get\" action=\"/sprache\">\
             <input type=\"text\" name=\"kennwort\" value=\"{GEHEIM_MARKE}\">\
             <input type=\"text\" name=\"sprache\" value=\"de\">\
             </form>"
        ),
    )
}

#[derive(Debug, Deserialize)]
struct Anmeldung {
    #[serde(default)]
    fluss: String,
    #[serde(default)]
    csrf: String,
    #[serde(default)]
    benutzer: String,
    #[serde(default)]
    passwort: String,
}

async fn anmelden(
    State(zustand): State<Arc<Zustand>>,
    kopf: HeaderMap,
    Form(anmeldung): Form<Anmeldung>,
) -> Response {
    zustand.login_posts.fetch_add(1, Ordering::SeqCst);

    let Some(fluss) = griff(&zustand.fluesse).get(&anmeldung.fluss).cloned() else {
        return (StatusCode::BAD_REQUEST, "unbekannter Vorgang").into_response();
    };
    if anmeldung.csrf != fluss.csrf {
        return (StatusCode::BAD_REQUEST, "csrf").into_response();
    }

    let richtig = zustand.passwort_richtig.load(Ordering::SeqCst)
        && anmeldung.benutzer == BENUTZER
        && anmeldung.passwort == PASSWORT;
    if !richtig {
        // Zurück zum Formular – wie ein echter Anmeldedienst.
        let csrf = format!("{}-neu", fluss.csrf);
        if let Some(eintrag) = griff(&zustand.fluesse).get_mut(&anmeldung.fluss) {
            eintrag.csrf = csrf.clone();
        }
        return anmelde_html(&basis(&kopf), &anmeldung.fluss, &csrf);
    }

    let sitzung = format!("sitzung-{}", anmeldung.fluss);
    griff(&zustand.sitzungen).insert(sitzung.clone());
    if let Some(eintrag) = griff(&zustand.fluesse).get_mut(&anmeldung.fluss) {
        eintrag.angemeldet = true;
    }

    let ziel = format!(
        "{}/{MANDANT}/next/oauth/zustimmen?fluss={}",
        basis(&kopf),
        anmeldung.fluss
    );
    let mut antwort = Redirect::to(&ziel).into_response();
    antwort.headers_mut().insert(
        header::SET_COOKIE,
        format!("sitzung={sitzung}; Path=/; HttpOnly")
            .parse()
            .expect("Cookie-Kopf"),
    );
    antwort
}

#[derive(Debug, Deserialize)]
struct Zustimmung {
    #[serde(default)]
    fluss: String,
    #[serde(default)]
    csrf: String,
}

async fn zustimmungsseite(
    State(zustand): State<Arc<Zustand>>,
    kopf: HeaderMap,
    Query(anfrage): Query<Zustimmung>,
) -> Response {
    let Some(fluss) = griff(&zustand.fluesse).get(&anfrage.fluss).cloned() else {
        return (StatusCode::BAD_REQUEST, "unbekannter Vorgang").into_response();
    };
    if !fluss.angemeldet {
        return anmelde_html(&basis(&kopf), &anfrage.fluss, &fluss.csrf);
    }
    zustimmungs_html(&basis(&kopf), &anfrage.fluss, &fluss.csrf)
}

async fn zustimmen(
    State(zustand): State<Arc<Zustand>>,
    Form(anfrage): Form<Zustimmung>,
) -> Response {
    let Some(fluss) = griff(&zustand.fluesse).get(&anfrage.fluss).cloned() else {
        return (StatusCode::BAD_REQUEST, "unbekannter Vorgang").into_response();
    };
    if anfrage.csrf != fluss.csrf {
        return (StatusCode::BAD_REQUEST, "csrf").into_response();
    }
    zustand.zustimmungen.fetch_add(1, Ordering::SeqCst);

    let code = format!("code-{}", griff(&zustand.codes).len() + 1);
    griff(&zustand.codes).insert(
        code.clone(),
        Code {
            challenge: fluss.challenge.clone(),
            redirect_uri: fluss.redirect_uri.clone(),
        },
    );

    let trenner = if fluss.redirect_uri.contains('?') {
        '&'
    } else {
        '?'
    };
    let ziel = format!(
        "{}{trenner}code={code}&state={}",
        fluss.redirect_uri, fluss.state
    );
    Redirect::to(&ziel).into_response()
}

// ---------------------------------------------------------------- Token

#[derive(Debug, Deserialize)]
struct TokenAnfrage {
    grant_type: String,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    code_verifier: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    client_id: Option<String>,
    #[serde(default)]
    redirect_uri: Option<String>,
}

fn token_fehler(code: &str, text: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": code, "error_description": text })),
    )
        .into_response()
}

async fn token(State(zustand): State<Arc<Zustand>>, Form(anfrage): Form<TokenAnfrage>) -> Response {
    zustand.token_aufrufe.fetch_add(1, Ordering::SeqCst);

    let Some(client_id) = anfrage.client_id.as_deref() else {
        return token_fehler("invalid_client", "client_id fehlt");
    };
    if !griff(&zustand.clients).contains(client_id) {
        return token_fehler("invalid_client", "client_id unbekannt");
    }

    match anfrage.grant_type.as_str() {
        "authorization_code" => {
            let (Some(code), Some(verifier), Some(redirect_uri)) = (
                anfrage.code.as_deref(),
                anfrage.code_verifier.as_deref(),
                anfrage.redirect_uri.as_deref(),
            ) else {
                return token_fehler("invalid_request", "code oder code_verifier fehlt");
            };
            let Some(gespeichert) = griff(&zustand.codes).remove(code) else {
                return token_fehler("invalid_grant", "Code unbekannt oder verbraucht");
            };
            if gespeichert.redirect_uri != redirect_uri {
                return token_fehler("invalid_grant", "redirect_uri passt nicht");
            }
            if quiver_zep::pkce::challenge(verifier) != gespeichert.challenge {
                return token_fehler("invalid_grant", "code_verifier passt nicht");
            }
            Json(zustand.ausstellen()).into_response()
        }
        "refresh_token" => {
            zustand.refresh_aufrufe.fetch_add(1, Ordering::SeqCst);
            let Some(refresh) = anfrage.refresh_token.as_deref() else {
                return token_fehler("invalid_request", "refresh_token fehlt");
            };
            // Rotation: jedes Refresh-Token gilt genau einmal.
            if !griff(&zustand.refresh_tokens).remove(refresh) {
                return token_fehler("invalid_grant", "Refresh-Token abgelehnt");
            }
            Json(zustand.ausstellen()).into_response()
        }
        anderes => token_fehler("unsupported_grant_type", anderes),
    }
}

// ------------------------------------------------------- Test-Browser

/// Ein Browser für die Tests: er folgt der Authorize-Adresse wie ein Mensch –
/// Formular ausfüllen, absenden, Zustimmung bestätigen, Weiterleitung folgen.
pub struct AttrappenBrowser {
    benutzer: String,
    passwort: String,
    http: reqwest::Client,
    aufrufe: AtomicUsize,
}

impl AttrappenBrowser {
    pub fn neu(benutzer: &str, passwort: &str) -> AttrappenBrowser {
        AttrappenBrowser {
            benutzer: benutzer.to_string(),
            passwort: passwort.to_string(),
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("Test-Browser baut"),
            aufrufe: AtomicUsize::new(0),
        }
    }

    /// Wie oft der Browser geöffnet wurde (muss bei der Routine 0 bleiben).
    pub fn aufrufe(&self) -> usize {
        self.aufrufe.load(Ordering::SeqCst)
    }
}

impl Browser for AttrappenBrowser {
    fn oeffnen(&self, url: &str) -> anyhow::Result<()> {
        self.aufrufe.fetch_add(1, Ordering::SeqCst);
        let http = self.http.clone();
        let ziel = url.to_string();
        let benutzer = self.benutzer.clone();
        let passwort = self.passwort.clone();
        tokio::spawn(async move {
            if let Err(fehler) = verlauf(&http, &ziel, &benutzer, &passwort).await {
                tracing::warn!("Test-Browser: {fehler}");
            }
        });
        Ok(())
    }
}

async fn verlauf(
    http: &reqwest::Client,
    start: &str,
    benutzer: &str,
    passwort: &str,
) -> anyhow::Result<()> {
    let mut ziel = url::Url::parse(start)?;
    for _ in 0..10 {
        let antwort = http.get(ziel.clone()).send().await?;
        let status = antwort.status();
        if status.is_redirection() {
            let ort = antwort
                .headers()
                .get("location")
                .and_then(|wert| wert.to_str().ok())
                .ok_or_else(|| anyhow::anyhow!("Weiterleitung ohne Ziel"))?;
            let weiter = ziel.join(ort)?;
            if matches!(weiter.host_str(), Some("127.0.0.1") | Some("localhost")) {
                // Der letzte Sprung gehört dem Lauscher des Werkzeugs.
                let _ = http.get(weiter.clone()).send().await;
                return Ok(());
            }
            ziel = weiter;
            continue;
        }
        if !status.is_success() {
            anyhow::bail!("Seite antwortet mit {status}");
        }

        let html = antwort.text().await?;
        let seite = Seite::lesen(&html);
        if let Some(formular) = seite.anmeldeformular() {
            let paare = formular
                .anmeldepaar(benutzer, passwort)
                .map_err(|fehler| anyhow::anyhow!("Anmeldeformular: {fehler}"))?;
            let ziel_url = if formular.action.is_empty() {
                ziel.clone()
            } else {
                ziel.join(&formular.action)?
            };
            let antwort = http.post(ziel_url.clone()).form(&paare).send().await?;
            if let Some(ort) = antwort
                .headers()
                .get("location")
                .and_then(|wert| wert.to_str().ok())
            {
                ziel = ziel_url.join(ort)?;
            }
            continue;
        }
        if let Some(formular) = seite.zustimmungsformular() {
            let ziel_url = if formular.action.is_empty() {
                ziel.clone()
            } else {
                ziel.join(&formular.action)?
            };
            let antwort = http
                .post(ziel_url.clone())
                .form(&formular.rumpf())
                .send()
                .await?;
            if let Some(ort) = antwort
                .headers()
                .get("location")
                .and_then(|wert| wert.to_str().ok())
            {
                ziel = ziel_url.join(ort)?;
            }
            continue;
        }
        anyhow::bail!("Der Test-Browser versteht die Seite nicht.");
    }
    anyhow::bail!("Der Test-Browser hat zu viele Schritte gebraucht.");
}
