// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der MCP-Server über stdio (Aufgabe 06, „Durchreichen").
//!
//! Er bietet genau das an, was der ZEP-MCP anbietet – dieselben Namen,
//! Schemas und Annotationen – und dazu das lesende `zep_profile` des Kerns.
//! Ein Aufruf geht unverändert an ZEP; die Antwort kommt unverändert zurück.
//!
//! Zwei Dinge tut der Server selbst:
//!
//! - **Anmelden bleiben**: er startet die Hintergrund-Erneuerung des Tokens.
//! - **Liste aktuell halten**: meldet ZEP `tools/list_changed`, holt er die
//!   Liste neu und schickt selbst `list_changed` an seinen Client.
//!
//! Ist keine Anmeldung möglich, antwortet jedes Werkzeug mit dem einen Satz aus
//! [`HINWEIS_ANMELDUNG`] – ein Text, **kein** MCP-Fehler.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use quiver_tools_kern::mcp::{profile_antwort, profile_tool};
use quiver_tools_kern::{antwort, ProfilFehler, Zugang};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::{NotificationContext, Peer, RequestContext};
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};

use crate::anmeldung::{Anmeldung, HINWEIS_ANMELDUNG};
use crate::browser::{Browser, KeinBrowser};
use crate::tresor::Tresor;
use crate::vermittler::Vermittler;

/// Was ein Werkzeug zurückgibt.
type Ergebnis = Result<CallToolResult, McpError>;

/// Der ZEP-Server: Zugang, Anmeldung, Vermittler und die gemerkte Werkzeugliste.
pub struct ZepServer {
    geteilt: Arc<Geteilt>,
}

struct Geteilt {
    zugang: Zugang,
    anmeldung: Option<Arc<Anmeldung>>,
    vermittler: Option<Arc<Vermittler>>,
    /// Die zuletzt von ZEP geholte Liste (ohne `zep_profile`).
    werkzeuge: Mutex<Vec<Tool>>,
    /// Wurde die Liste schon einmal geholt?
    geladen: AtomicBool,
    /// Der Client auf der anderen Seite von stdio (für `list_changed`).
    peer: Mutex<Option<Peer<RoleServer>>>,
    /// Läuft die Hintergrundarbeit schon?
    gestartet: AtomicBool,
}

impl ZepServer {
    /// Ein Server über dem Zugang eines Werkzeugs.
    ///
    /// Ohne vollständigen Zugang ist `anmeldung` leer: dann antworten alle
    /// Werkzeuge mit dem Hinweis des Kerns (SPEC §2).
    pub fn neu(zugang: Zugang) -> ZepServer {
        ZepServer::mit_diensten(
            zugang,
            crate::tresor::aus_umgebung(),
            Arc::new(KeinBrowser::neu()),
        )
    }

    /// Wie [`ZepServer::neu`], aber mit eigenem Tresor und Browser (Tests).
    pub fn mit_diensten(
        zugang: Zugang,
        tresor: Arc<dyn Tresor>,
        browser: Arc<dyn Browser>,
    ) -> ZepServer {
        let anmeldung =
            crate::werkzeug::anmeldung_aus_zugang(&zugang, tresor, browser).map(Arc::new);
        let vermittler = anmeldung.clone().map(Vermittler::neu).map(Arc::new);
        ZepServer {
            geteilt: Arc::new(Geteilt {
                zugang,
                anmeldung,
                vermittler,
                werkzeuge: Mutex::new(Vec::new()),
                geladen: AtomicBool::new(false),
                peer: Mutex::new(None),
                gestartet: AtomicBool::new(false),
            }),
        }
    }

    /// Die Anmeldung, falls der Zugang vollständig ist (Tests, Befehle).
    pub fn anmeldung(&self) -> Option<Arc<Anmeldung>> {
        self.geteilt.anmeldung.clone()
    }

    /// Der Vermittler, falls der Zugang vollständig ist.
    pub fn vermittler(&self) -> Option<Arc<Vermittler>> {
        self.geteilt.vermittler.clone()
    }

    /// Die Werkzeugliste, wie `tools/list` sie meldet – `zep_profile` zuletzt.
    pub fn werkzeuge(&self) -> Vec<Tool> {
        let mut liste = self
            .geteilt
            .werkzeuge
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner())
            .clone();
        let profil = profile_tool(&self.geteilt.zugang);
        if !liste.iter().any(|werkzeug| werkzeug.name == profil.name) {
            liste.push(profil);
        }
        liste
    }

    /// Die Liste bei ZEP holen (beim Start und nach `list_changed`).
    pub async fn liste_holen(&self) -> bool {
        let Some(vermittler) = &self.geteilt.vermittler else {
            return false;
        };
        match vermittler.werkzeuge().await {
            Ok(werkzeuge) => {
                *self
                    .geteilt
                    .werkzeuge
                    .lock()
                    .unwrap_or_else(|vergiftet| vergiftet.into_inner()) = werkzeuge;
                self.geteilt.geladen.store(true, Ordering::SeqCst);
                true
            }
            Err(fehler) => {
                tracing::warn!(%fehler, "Werkzeugliste des ZEP-MCP nicht geholt");
                false
            }
        }
    }

    /// Ein Werkzeug aufrufen. Auch der Weg, den die Tests nehmen.
    pub async fn aufrufen(
        &self,
        name: &str,
        argumente: Option<rmcp::model::JsonObject>,
    ) -> Ergebnis {
        if name == quiver_tools_kern::mcp::profile_tool_name(&self.geteilt.zugang) {
            return self.profil();
        }

        // Fehlt der Zugang, sagt das der Kern – ein Text, kein Fehler.
        if self.geteilt.anmeldung.is_none() {
            return Ok(antwort(quiver_tools_kern::hinweis_zugang(
                self.geteilt.zugang.titel(),
            )));
        }

        let Some(vermittler) = &self.geteilt.vermittler else {
            return Ok(antwort(HINWEIS_ANMELDUNG));
        };
        // Die Werkzeugliste muss einmal geholt sein, sonst kennt ZEP den Namen
        // vielleicht gar nicht (erster Aufruf nach dem Start).
        if !self.geteilt.geladen.load(Ordering::SeqCst) {
            self.liste_holen().await;
        }

        let params = CallToolRequestParams::new(name.to_string())
            .with_arguments(argumente.unwrap_or_default());
        match vermittler.aufrufen(params).await {
            Ok(ergebnis) => Ok(ergebnis),
            Err(fehler) if fehler.braucht_anmeldung() => Ok(antwort(HINWEIS_ANMELDUNG)),
            Err(fehler) => Err(McpError::internal_error(fehler.to_string(), None)),
        }
    }

    /// `zep_profile`: der Zugang ohne Geheimnisse.
    fn profil(&self) -> Ergebnis {
        if self.geteilt.zugang.profil() == Err(ProfilFehler::Keins) {
            return Ok(antwort(quiver_tools_kern::hinweis_zugang(
                self.geteilt.zugang.titel(),
            )));
        }
        profile_antwort(&self.geteilt.zugang)
    }

    /// Hintergrundarbeit starten: Token erneuern, Liste nachführen.
    ///
    /// Läuft einmal je Prozess. Der erste Aufruf setzt den Peer; danach darf
    /// `list_changed` an den Client gehen.
    fn starte_hintergrund(&self, peer: Peer<RoleServer>) {
        if self.geteilt.gestartet.swap(true, Ordering::SeqCst) {
            return;
        }
        *self
            .geteilt
            .peer
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner()) = Some(peer);

        if let Some(anmeldung) = &self.geteilt.anmeldung {
            anmeldung.starte_hintergrund();
        }

        let Some(vermittler) = &self.geteilt.vermittler else {
            return;
        };
        let aenderung = vermittler.aenderungen();
        let geteilt = Arc::clone(&self.geteilt);
        tokio::spawn(async move {
            loop {
                aenderung.notified().await;
                let Some(vermittler) = &geteilt.vermittler else {
                    return;
                };
                let Ok(werkzeuge) = vermittler.werkzeuge().await else {
                    continue;
                };
                *geteilt
                    .werkzeuge
                    .lock()
                    .unwrap_or_else(|vergiftet| vergiftet.into_inner()) = werkzeuge;
                geteilt.geladen.store(true, Ordering::SeqCst);
                let peer = geteilt
                    .peer
                    .lock()
                    .unwrap_or_else(|vergiftet| vergiftet.into_inner())
                    .clone();
                if let Some(peer) = peer {
                    if let Err(fehler) = peer.notify_tool_list_changed().await {
                        tracing::warn!(%fehler, "list_changed ließ sich nicht senden");
                    } else {
                        tracing::info!(
                            "Werkzeugliste von ZEP übernommen und list_changed gesendet"
                        );
                    }
                }
            }
        });
    }
}

impl ServerHandler for ZepServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_tool_list_changed()
                .build(),
        )
        .with_server_info(quiver_tools_kern::mcp::server_info(&self.geteilt.zugang))
        .with_instructions(
            "ZEP über den offiziellen ZEP-MCP: Projekte, Zeiten und Berichte. Die Werkzeuge \
             sind die des ZEP-MCP, unverändert durchgereicht. Die Anmeldung hält das Werkzeug \
             selbst: Benutzer und Passwort stehen im quiver-Fenster unter Tools → ZEP. Ist \
             keine Anmeldung möglich, sagt das jedes Werkzeug als Text; dann im quiver-Fenster \
             auf „Anmelden“ klicken."
                .to_string(),
        )
    }

    async fn on_initialized(&self, context: NotificationContext<RoleServer>) {
        self.starte_hintergrund(context.peer);
        // Die Werkzeugliste beim Start holen.
        self.liste_holen().await;
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        if !self.geteilt.geladen.load(Ordering::SeqCst) {
            self.liste_holen().await;
        }
        Ok(ListToolsResult::with_all_items(self.werkzeuge()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        self.aufrufen(request.name.as_ref(), request.arguments)
            .await
            .map(CallToolResponse::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quiver_tools_kern::Zugang;

    fn ohne_zugang() -> Zugang {
        Zugang::mit_umgebung(&crate::werkzeug::Zep, |_| None)
    }

    fn zugang(adresse: &str) -> Zugang {
        Zugang::mit_umgebung(&crate::werkzeug::Zep, |name| match name {
            "ZEP_URL" => Some(adresse.to_string()),
            "ZEP_BENUTZER" => Some("ada".to_string()),
            "ZEP_PASSWORT" => Some("streng-geheim-4711".to_string()),
            _ => None,
        })
    }

    #[test]
    fn ohne_zugang_gibt_es_nur_das_profil_und_den_hinweis() {
        let server = ZepServer::neu(ohne_zugang());
        let werkzeuge = server.werkzeuge();
        assert_eq!(werkzeuge.len(), 1);
        assert_eq!(werkzeuge[0].name, "zep_profile");
        assert!(server.anmeldung().is_none());
        assert!(server.vermittler().is_none());

        let laufzeit = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let ergebnis = laufzeit
            .block_on(server.aufrufen("zep_profile", None))
            .unwrap();
        let text = ergebnis.content[0].as_text().unwrap().text.clone();
        assert!(text.contains("Zugang fehlt"), "{text}");
        assert!(text.contains("ZEP"), "{text}");
    }

    #[test]
    fn die_faehigkeiten_melden_tools_und_list_changed() {
        let server = ZepServer::neu(zugang("https://zep.example.test/a/next/mcp"));
        let config = server.get_info();
        let tools = config.capabilities.tools.expect("keine Werkzeug-Fähigkeit");
        assert_eq!(tools.list_changed, Some(true));
        assert_eq!(config.server_info.name, "quiver-zep");
        assert_eq!(config.server_info.title.as_deref(), Some("ZEP"));
        assert!(config
            .instructions
            .as_deref()
            .unwrap_or_default()
            .contains("quiver-Fenster"));
    }

    #[test]
    fn ein_unbekanntes_werkzeug_ohne_anmeldung_antwortet_mit_dem_hinweis() {
        let server = ZepServer::neu(zugang("https://zep.example.test/a/next/mcp"));
        let laufzeit = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        // Der Host ist nicht erreichbar: statt eines Fehlers kommt der Hinweis.
        let ergebnis = laufzeit
            .block_on(server.aufrufen("zep_projekte", None))
            .unwrap();
        let text = ergebnis.content[0].as_text().unwrap().text.clone();
        assert!(text.contains("ZEP-Anmeldung nötig"), "{text}");
        assert!(text.contains("Tools → ZEP"), "{text}");
        assert!(!text.contains("streng-geheim-4711"));
    }
}
