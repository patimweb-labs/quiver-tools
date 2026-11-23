// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Beispiel-Werkzeug: zeigt den Zugang aus der Umgebung und einen winzigen
//! MCP-Server, ohne echten Dienst.
//!
//! ```bash
//! # Zugang prüfen (die Felder kommen als Umgebung, wie quiver sie setzt)
//! DEMO_URL=https://example.test DEMO_TOKEN=geheim \
//!   cargo run -p quiver-tools-kern --example demo -- testen
//!
//! # Bibliothekseintrag für mcp-quiver
//! cargo run -p quiver-tools-kern --example demo -- eintrag
//!
//! # MCP über stdio, so wie quiver das Tool startet
//! cargo run -p quiver-tools-kern --example demo
//! ```

use quiver_tools_kern::{starten, Feld, FeldArt, Profil, Werkzeug};
use rmcp::handler::server::ServerHandler;
use rmcp::model::{CallToolResult, ServerCapabilities, ServerConfig};
use rmcp::ErrorData as McpError;
use rmcp::{tool, tool_handler, tool_router};

/// Vier Felder: Adresse (offen), API-Schlüssel (geheim), Auswahl, Schalter.
/// Die Umgebung heißt `DEMO_URL`, `DEMO_API_KEY`, `DEMO_UMGEBUNG`, `DEMO_STRENG`.
struct Demo;

impl Werkzeug for Demo {
    fn name(&self) -> &'static str {
        "demo"
    }

    fn titel(&self) -> &'static str {
        "Demo-Werkzeug"
    }

    fn beschreibung(&self) -> &'static str {
        "Zeigt die MCP-Hilfen des Kerns, ohne einen echten Dienst."
    }

    fn felder(&self) -> Vec<Feld> {
        vec![
            Feld {
                name: "url",
                titel: "Adresse",
                art: FeldArt::Url,
                pflicht: true,
                standard: Some("https://example.test"),
                hilfe: "Die Adresse des Dienstes, mit https://.",
                link: None,
            },
            Feld {
                name: "api_key",
                titel: "API-Schlüssel",
                art: FeldArt::Passwort,
                pflicht: true,
                standard: None,
                hilfe: "Steht in den Einstellungen des Dienstes.",
                link: None,
            },
            Feld {
                name: "umgebung",
                titel: "Umgebung",
                art: FeldArt::Auswahl(vec![
                    ("produktion".to_string(), "Produktion".to_string()),
                    ("test".to_string(), "Test".to_string()),
                ]),
                pflicht: false,
                standard: Some("test"),
                hilfe: "Nur eine Bezeichnung – das Beispiel fragt niemanden.",
                link: None,
            },
            Feld {
                name: "streng",
                titel: "Streng prüfen",
                art: FeldArt::Schalter,
                pflicht: false,
                standard: Some("true"),
                hilfe: "Zeigt, wie ein Schalter aussieht.",
                link: None,
            },
        ]
    }

    async fn testen(&self, profil: &Profil) -> anyhow::Result<String> {
        // Kein Netz: das Beispiel prüft nur, ob die Adresse plausibel ist.
        let url = profil.werte.get("url").map(String::as_str).unwrap_or("");
        if !url.starts_with("https://") && !url.starts_with("http://") {
            anyhow::bail!("Die Adresse muss mit http:// oder https:// beginnen.");
        }
        Ok(format!(
            "Adresse {url} ist plausibel, ein Schlüssel liegt vor: {}",
            if profil.geheim.contains_key("api_key") {
                "ja"
            } else {
                "nein"
            }
        ))
    }
}

/// Der MCP-Server des Beispiels: ein einziges lesendes Tool.
#[derive(Clone)]
struct DemoServer {
    zugang: quiver_tools_kern::Zugang,
}

#[tool_router]
impl DemoServer {
    #[tool(
        description = "Zeigt den Zugang des Demo-Werkzeugs (ohne Geheimnisse).",
        annotations(title = "Zugang", read_only_hint = true)
    )]
    fn demo_profile(&self) -> Result<CallToolResult, McpError> {
        quiver_tools_kern::mcp::profile_antwort(&self.zugang)
    }
}

#[tool_handler]
impl ServerHandler for DemoServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(quiver_tools_kern::mcp::server_info(&self.zugang))
            .with_instructions(
                "Demo-Werkzeug: ein Profil-Tool, das zeigt, wie die MCP-Hilfen des Kerns benutzt werden."
                    .to_string(),
            )
    }
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    starten(Demo, |zugang| DemoServer { zugang }).await
}
