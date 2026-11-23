// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Beispiel-Werkzeug: zeigt die Einrichtungsseite und einen winzigen
//! MCP-Server, ohne echten Dienst.
//!
//! ```bash
//! QUIVER_TOOLS_TRESOR=speicher \
//!   cargo run -p quiver-tools-kern --example demo -- einrichten
//! ```
//!
//! Der Tresor im Speicher ist Absicht: das Beispiel soll ohne Schlüsselbund
//! laufen. Für den MCP-Weg ohne Argument starten.
//!
//! `QUIVER_TOOLS_KEIN_BROWSER=1` öffnet **keinen** Browser, sondern gibt die
//! Adresse der Seite auf stdout aus und wartet, bis „Fertig“ gedrückt wird. So
//! entstehen die Bilder in `docs/bilder` (`docs/bilder/erzeugen.mjs`) und so
//! sieht man die Seite auf einem Rechner ohne Fenster an.

use std::sync::Arc;
use std::time::Duration;

use quiver_tools_kern::{
    einrichten_mit_profilen, starten, Browser, Feld, FeldArt, Merker, Profil, Profile, Werkzeug,
};
use rmcp::handler::server::ServerHandler;
use rmcp::model::{CallToolResult, ServerCapabilities, ServerConfig};
use rmcp::ErrorData as McpError;
use rmcp::{tool, tool_handler, tool_router};

/// Vier Felder: Adresse (offen), API-Schlüssel (geheim), Auswahl, Schalter.
struct Demo;

impl Werkzeug for Demo {
    fn name(&self) -> &'static str {
        "demo"
    }

    fn titel(&self) -> &'static str {
        "Demo-Werkzeug"
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
            },
            Feld {
                name: "api_key",
                titel: "API-Schlüssel",
                art: FeldArt::Passwort,
                pflicht: true,
                standard: None,
                hilfe: "Steht in den Einstellungen des Dienstes.",
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
            },
            Feld {
                name: "streng",
                titel: "Streng prüfen",
                art: FeldArt::Schalter,
                pflicht: false,
                standard: Some("true"),
                hilfe: "Zeigt, wie ein Schalter auf der Seite aussieht.",
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
    profile: quiver_tools_kern::Profile,
}

#[tool_router]
impl DemoServer {
    #[tool(
        description = "Listet die eingerichteten Profile des Demo-Werkzeugs (ohne Geheimnisse).",
        annotations(title = "Profile", read_only_hint = true)
    )]
    fn demo_profile(&self) -> Result<CallToolResult, McpError> {
        quiver_tools_kern::mcp::profile_antwort(&self.profile)
    }
}

#[tool_handler]
impl ServerHandler for DemoServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(quiver_tools_kern::mcp::server_info(&self.profile))
            .with_instructions(
            "Demo-Werkzeug: ein Profil-Tool, das zeigt, wie die MCP-Hilfen des Kerns benutzt werden."
                .to_string(),
        )
    }
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    if std::env::var_os("QUIVER_TOOLS_KEIN_BROWSER").is_some() {
        return seite_ohne_browser().await;
    }
    starten(Demo, |profile| DemoServer { profile }).await
}

/// Dieselbe Seite, aber ohne Browser: die Adresse geht auf stdout.
async fn seite_ohne_browser() -> std::process::ExitCode {
    let profile = Profile::laden(&Demo, quiver_tools_kern::tresor::aus_umgebung());
    let merker = Arc::new(Merker::neu());
    let seite = tokio::spawn(einrichten_mit_profilen(
        Demo,
        profile,
        merker.clone() as Arc<dyn Browser>,
        Duration::from_secs(10 * 60),
    ));
    println!("{}", merker.adresse().await);
    match seite.await {
        Ok(ende) => ende.exit_code(),
        Err(_) => std::process::ExitCode::FAILURE,
    }
}
