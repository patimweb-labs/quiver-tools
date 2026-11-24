// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der Weg zum ZEP-MCP (Aufgabe 06, „Durchreichen").
//!
//! `quiver-zep` baut den ZEP-MCP nicht nach: es ist ein **Vermittler**. Dieser
//! Modul ist der rmcp-Client (Streamable HTTP mit `Authorization: Bearer`), der
//! die Werkzeugliste holt und Aufrufe 1:1 weiterreicht – Namen, Schemas und
//! Annotationen bleiben, wie ZEP sie schickt.
//!
//! Zwei Aufgaben kommen dazu:
//!
//! - **401 → erneuern, einmal wiederholen.** Lehnt ZEP das Access-Token ab,
//!   wird es verworfen, über das Refresh-Token erneuert und der Aufruf genau
//!   einmal wiederholt.
//! - **`tools/list_changed`** von ZEP wird über [`ZepClient`] gemeldet, damit
//!   der eigene Server die Liste neu holt und seinerseits `list_changed` sendet.

use std::sync::Arc;
use std::time::Duration;

use rmcp::model::{
    CallToolRequestParams, CallToolResult, ClientCapabilities, ClientConfig, Implementation, Tool,
};
use rmcp::service::{NotificationContext, Peer, RoleClient, RunningService};
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::{ClientHandler, ServiceError, ServiceExt};
use tokio::sync::{Mutex as AsyncMutex, Notify};

use crate::anmeldung::{AnmeldeFehler, Anmeldung};

/// Wie lange das Schließen einer alten Verbindung höchstens dauert.
const ABSCHIED: Duration = Duration::from_secs(3);

/// Was beim Vermitteln schiefgehen kann.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VermittlerFehler {
    /// Die Anmeldung fehlt oder die Routine ist gesperrt.
    #[error("{0}")]
    Anmeldung(#[from] AnmeldeFehler),
    /// Der ZEP-MCP ist nicht erreichbar oder antwortet nicht wie erwartet.
    #[error("Der ZEP-MCP ist nicht erreichbar: {0}")]
    NichtErreichbar(String),
    /// ZEP hat den Aufruf mit einem Protokollfehler beantwortet.
    #[error("Der ZEP-MCP meldet einen Fehler: {0}")]
    Protokoll(String),
}

impl VermittlerFehler {
    /// Braucht der Agent eine Anmeldung im quiver-Fenster?
    pub fn braucht_anmeldung(&self) -> bool {
        matches!(
            self,
            VermittlerFehler::Anmeldung(
                AnmeldeFehler::NichtAngemeldet
                    | AnmeldeFehler::Gesperrt(_)
                    | AnmeldeFehler::Voruebergehend(_)
            )
        )
    }
}

/// Ein laufender Client zum ZEP-MCP.
struct Dienst {
    service: RunningService<RoleClient, ZepClient>,
}

/// Der rmcp-Client-Handler: er meldet nur, dass ZEP seine Werkzeugliste
/// geändert hat.
#[derive(Clone)]
pub struct ZepClient {
    aenderung: Arc<Notify>,
}

impl ZepClient {
    /// Ein Handler, der Änderungen an `aenderung` meldet.
    pub fn neu(aenderung: Arc<Notify>) -> ZepClient {
        ZepClient { aenderung }
    }
}

impl ClientHandler for ZepClient {
    fn get_info(&self) -> ClientConfig {
        ClientConfig::new(
            ClientCapabilities::builder().build(),
            Implementation::new("quiver-zep", env!("CARGO_PKG_VERSION"))
                .with_title("quiver-zep (Vermittler)"),
        )
    }

    async fn on_tool_list_changed(&self, _context: NotificationContext<RoleClient>) {
        tracing::info!("ZEP hat seine Werkzeugliste geändert");
        self.aenderung.notify_one();
    }
}

/// Der Vermittler zu einem ZEP-MCP.
#[derive(Clone)]
pub struct Vermittler {
    inner: Arc<Inner>,
}

struct Inner {
    anmeldung: Arc<Anmeldung>,
    mcp: String,
    aenderung: Arc<Notify>,
    dienst: AsyncMutex<Option<Dienst>>,
}

impl std::fmt::Debug for Vermittler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vermittler")
            .field("mcp", &self.inner.mcp)
            .field("benutzer", &self.inner.anmeldung.benutzer())
            .finish_non_exhaustive()
    }
}

impl Vermittler {
    /// Ein Vermittler über einer Anmeldung.
    pub fn neu(anmeldung: Arc<Anmeldung>) -> Vermittler {
        let mcp = anmeldung.url().to_string();
        Vermittler {
            inner: Arc::new(Inner {
                anmeldung,
                mcp,
                aenderung: Arc::new(Notify::new()),
                dienst: AsyncMutex::new(None),
            }),
        }
    }

    /// Das Signal „ZEP hat die Werkzeugliste geändert“.
    pub fn aenderungen(&self) -> Arc<Notify> {
        Arc::clone(&self.inner.aenderung)
    }

    /// Die Werkzeugliste von ZEP – 1:1, wie ZEP sie meldet.
    pub async fn werkzeuge(&self) -> Result<Vec<Tool>, VermittlerFehler> {
        self.mit_wiederholung(|peer| async move { peer.list_all_tools().await })
            .await
    }

    /// Ein Werkzeug bei ZEP aufrufen – Namen, Argumente und Antwort unverändert.
    pub async fn aufrufen(
        &self,
        params: CallToolRequestParams,
    ) -> Result<CallToolResult, VermittlerFehler> {
        let name = params.name.to_string();
        let ergebnis = self
            .mit_wiederholung(move |peer| {
                let params = params.clone();
                async move { peer.call_tool(params).await }
            })
            .await;
        if let Err(fehler) = &ergebnis {
            // Der zweite Versuch ist gescheitert: derselbe Aufruf noch einmal
            // wäre nur eine Schleife. Der Agent bekommt den Fehler zu sehen.
            tracing::debug!(werkzeug = %name, fehler = %fehler, "Aufruf bei ZEP gescheitert");
        }
        ergebnis
    }

    /// Den Dienst schließen (nach einem 401 oder beim Beenden).
    pub async fn schliessen(&self) {
        let dienst = self.inner.dienst.lock().await.take();
        if let Some(dienst) = dienst {
            let _ = tokio::time::timeout(ABSCHIED, dienst.service.cancel()).await;
        }
    }

    // ------------------------------------------------------------ innen

    /// Einen Aufruf mit einem Dienst versuchen; bei einem Anmeldefehler das
    /// Token erneuern und **einmal** wiederholen.
    async fn mit_wiederholung<T, F, Fut>(&self, aufruf: F) -> Result<T, VermittlerFehler>
    where
        F: Fn(Peer<RoleClient>) -> Fut,
        Fut: std::future::Future<Output = Result<T, ServiceError>>,
    {
        let peer = self.peer().await?;
        match aufruf(peer).await {
            Ok(wert) => Ok(wert),
            Err(fehler) if ist_anmeldefehler(&fehler) => {
                tracing::info!("ZEP lehnt das Access-Token ab – erneuern und einmal wiederholen");
                self.inner.anmeldung.verwerfen();
                self.schliessen().await;
                // Ein neues Access-Token (Refresh-Token oder Routine).
                self.inner.anmeldung.access_token().await?;
                let peer = self.peer().await?;
                aufruf(peer).await.map_err(protokollfehler)
            }
            Err(fehler) => Err(protokollfehler(fehler)),
        }
    }

    /// Der Peer eines laufenden Dienstes – vorhanden oder neu gebaut.
    ///
    /// Bauen und Merken passieren unter **einer** Sperre: sonst würden zwei
    /// gleichzeitige Aufrufe zwei Verbindungen bauen und die erste fiele dem
    /// zweiten Aufruf unter den Füßen weg („Transport closed“).
    async fn peer(&self) -> Result<Peer<RoleClient>, VermittlerFehler> {
        let mut fach = self.inner.dienst.lock().await;
        if let Some(dienst) = fach.as_ref() {
            return Ok(dienst.service.peer().clone());
        }
        let dienst = self.dienst_bauen().await?;
        let peer = dienst.service.peer().clone();
        *fach = Some(dienst);
        tracing::info!(mcp = %self.inner.mcp, "Verbindung zum ZEP-MCP steht");
        Ok(peer)
    }

    /// Einen Dienst bauen: Token holen, Transport mit `Bearer`, Handschlag.
    async fn dienst_bauen(&self) -> Result<Dienst, VermittlerFehler> {
        let token = self.inner.anmeldung.access_token().await?;
        let config = StreamableHttpClientTransportConfig::with_uri(self.inner.mcp.clone())
            .auth_header(token);
        let transport = StreamableHttpClientTransport::from_config(config);
        let handler = ZepClient::neu(Arc::clone(&self.inner.aenderung));

        let service = handler.serve(transport).await.map_err(|fehler| {
            if fehler.auth_challenge().is_some() {
                VermittlerFehler::Anmeldung(AnmeldeFehler::NichtAngemeldet)
            } else {
                VermittlerFehler::NichtErreichbar(fehler.to_string())
            }
        })?;
        Ok(Dienst { service })
    }
}

/// Hat ZEP den Aufruf wegen der Anmeldung abgelehnt (401/403)?
fn ist_anmeldefehler(fehler: &ServiceError) -> bool {
    fehler.auth_challenge().is_some()
}

/// Einen Protokollfehler als Text ohne Geheimnisse.
fn protokollfehler(fehler: ServiceError) -> VermittlerFehler {
    VermittlerFehler::Protokoll(fehler.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ein_protokollfehler_ist_kein_anmeldefehler() {
        let fehler = VermittlerFehler::Protokoll("irgendwas".to_string());
        assert!(!fehler.braucht_anmeldung());
        assert!(VermittlerFehler::Anmeldung(AnmeldeFehler::NichtAngemeldet).braucht_anmeldung());
        assert!(
            VermittlerFehler::Anmeldung(AnmeldeFehler::Gesperrt("gesperrt".to_string()))
                .braucht_anmeldung()
        );
        assert!(!VermittlerFehler::NichtErreichbar("weg".to_string()).braucht_anmeldung());
    }

    #[tokio::test]
    async fn ohne_zugang_kommt_der_anmeldehinweis() {
        let ordner = tempfile::tempdir().unwrap();
        let anmeldung = Arc::new(Anmeldung::neu(
            crate::werkzeug::ZepKonfig {
                url: "https://zep.example.test/a/next/mcp".to_string(),
                benutzer: "ada".to_string(),
                passwort: "streng-geheim-4711".to_string(),
                zustand: ordner.path().to_path_buf(),
                takt: Duration::from_millis(50),
                wartezeit: Duration::from_secs(2),
                pause: Duration::from_millis(50),
            },
            Arc::new(crate::tresor::SpeicherTresor::neu()),
            Arc::new(crate::browser::KeinBrowser),
        ));
        // Kein erreichbarer Server: der Fehler darf keinen Wert nennen.
        let vermittler = Vermittler::neu(anmeldung);
        let fehler = vermittler.werkzeuge().await.unwrap_err();
        assert!(!fehler.to_string().contains("streng-geheim-4711"));
        assert!(format!("{vermittler:?}").contains("ada"));
    }
}
