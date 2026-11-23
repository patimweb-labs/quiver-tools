// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Was jedes Tool für MCP braucht (SPEC §2, §6): den gemeinsamen Parameter
//! `profil`, den Hinweis statt eines Fehlers, wenn noch nichts eingerichtet ist,
//! das lesende Tool `<name>_profile` und das Kürzen langer Texte.

use rmcp::ErrorData as McpError;
// `schemars` heißt so, wie das Derive-Makro es erwartet.
use rmcp::model::{
    CallToolResult, ContentBlock, Implementation, JsonObject, Tool, ToolAnnotations,
};
use rmcp::schemars;
use serde::Deserialize;

use crate::profil::{ProfilFehler, Profile};
use crate::{hinweis_einrichten, Profil};

/// Der gemeinsame Parameter jedes Tools: welches Profil gemeint ist.
///
/// Ohne Angabe gilt das Standardprofil. Tools, die weitere Parameter haben,
/// binden dieses Feld per `#[serde(flatten)]` ein.
#[derive(Debug, Clone, Default, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ProfilWahl {
    /// Name eines Profils; ohne Angabe das Standardprofil.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profil: Option<String>,
}

impl ProfilWahl {
    /// Der gewählte Profilname, falls einer genannt wurde.
    pub fn name(&self) -> Option<&str> {
        self.profil.as_deref()
    }
}

/// Was ein Tool zurückgibt, wenn kein Profil bereitsteht.
#[derive(Debug)]
pub enum OhneProfil {
    /// Noch kein Zugang eingerichtet: ein Text, **kein** MCP-Fehler (SPEC §2).
    Hinweis(CallToolResult),
    /// Ein anderer Fehler, als MCP-Fehler ohne Geheimnisse.
    Fehler(McpError),
}

impl OhneProfil {
    /// Fertige Tool-Antwort.
    pub fn in_ergebnis(self) -> Result<CallToolResult, McpError> {
        match self {
            OhneProfil::Hinweis(antwort) => Ok(antwort),
            OhneProfil::Fehler(fehler) => Err(fehler),
        }
    }
}

/// Kurze Textantwort.
pub fn antwort(text: impl Into<String>) -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::text(text.into())])
}

/// Ein Profil holen oder sagen, was der Agent stattdessen sehen soll.
///
/// `ProfilFehler::Keins` wird zum Einrichtungshinweis (kein Fehler), alles
/// andere zum MCP-Fehler.
pub fn zugang(titel: &str, ergebnis: Result<Profil, ProfilFehler>) -> Result<Profil, OhneProfil> {
    ergebnis.map_err(|fehler| match fehler {
        ProfilFehler::Keins => OhneProfil::Hinweis(antwort(hinweis_einrichten(titel))),
        sonst => OhneProfil::Fehler(fehler_als_mcp(&sonst)),
    })
}

/// Ein Profilfehler als MCP-Fehler. Die Texte der [`ProfilFehler`] nennen keine
/// Geheimnisse – sie beschreiben nur, was fehlt.
pub fn fehler_als_mcp(fehler: &ProfilFehler) -> McpError {
    McpError::internal_error(fehler.to_string(), None)
}

/// Der Name, unter dem sich das Tool bei `initialize` meldet:
/// `quiver-<werkzeug>` mit der Version des Pakets.
pub fn server_info(profile: &Profile) -> Implementation {
    Implementation::new(
        format!("quiver-{}", profile.werkzeug()),
        env!("CARGO_PKG_VERSION"),
    )
    .with_title(profile.titel().to_string())
}

/// Name des lesenden Profil-Tools: `<name>_profile`.
pub fn profile_tool_name(profile: &Profile) -> String {
    format!("{}_profile", profile.werkzeug())
}

/// Das lesende Tool `<name>_profile`: es ändert nichts, es listet nur.
pub fn profile_tool(profile: &Profile) -> Tool {
    Tool::new(
        profile_tool_name(profile),
        format!(
            "Listet die eingerichteten Profile für {} (Name, Beschreibung, Standard, nicht-geheime Felder).",
            profile.titel()
        ),
        JsonObject::new(),
    )
    .annotate(ToolAnnotations::from_raw(
        Some(format!("Profile von {}", profile.titel())),
        Some(true),
        Some(false),
        Some(true),
        Some(false),
    ))
}

/// Antwort für `<name>_profile`: alle Profile, ohne Geheimnisse.
///
/// Ist noch keins eingerichtet, kommt der Einrichtungshinweis – wie bei jedem
/// anderen Tool.
pub fn profile_antwort(profile: &Profile) -> Result<CallToolResult, McpError> {
    let stand = profile.stand().map_err(|fehler| fehler_als_mcp(&fehler))?;
    if stand.profile.is_empty() {
        return Ok(antwort(hinweis_einrichten(profile.titel())));
    }

    // Nur die nicht-geheimen Werte: die Geheimnisse kommen hier nie an.
    let mut text = format!("Profile für {}:", profile.titel());
    for profil in &stand.profile {
        let standard = stand.standard.as_deref() == Some(profil.name.as_str());
        text.push_str(&format!("\n- {}", profil.name));
        if standard {
            text.push_str(" (Standard)");
        }
        if !profil.beschreibung.is_empty() {
            text.push_str(&format!(": {}", profil.beschreibung));
        }
        for (name, wert) in &profil.werte {
            if wert.is_empty() {
                continue;
            }
            text.push_str(&format!("\n  {name}: {wert}"));
        }
    }
    Ok(antwort(text))
}

/// Kürzt `text` auf höchstens `max_zeichen` Zeichen, möglichst an einer
/// Wortgrenze.
///
/// Liefert den gekürzten Text und – wenn gekürzt wurde – wie viele Zeichen
/// wegfielen. Ohne Kürzung ist das Ergebnis unverändert.
pub fn kuerzen(text: &str, max_zeichen: usize) -> (String, Option<usize>) {
    let anzahl = text.chars().count();
    if max_zeichen == 0 {
        return (String::new(), (anzahl > 0).then_some(anzahl));
    }
    if anzahl <= max_zeichen {
        return (text.to_string(), None);
    }

    let roh: String = text.chars().take(max_zeichen).collect();
    // An der letzten Wortgrenze abschneiden, aber nicht mehr als die Hälfte
    // wegwerfen: ein einzelnes sehr langes Wort bleibt so stehen.
    let grenze = roh
        .char_indices()
        .filter(|(_, zeichen)| zeichen.is_whitespace())
        .map(|(stelle, _)| stelle)
        .next_back()
        .filter(|stelle| *stelle >= roh.len() / 2)
        .unwrap_or(roh.len());

    let gekuerzt = roh[..grenze]
        .trim_end_matches(char::is_whitespace)
        .to_string();
    let weggelassen = anzahl - gekuerzt.chars().count();
    (gekuerzt, Some(weggelassen))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kuerzen_laesst_kurzes_stehen() {
        assert_eq!(kuerzen("kurz", 10), ("kurz".to_string(), None));
        assert_eq!(kuerzen("genau", 5), ("genau".to_string(), None));
    }

    #[test]
    fn kuerzen_schneidet_an_der_wortgrenze() {
        let text = "Der Agent bezahlt Tokens für jede Antwort.";
        let (gekuerzt, weggelassen) = kuerzen(text, 20);
        assert_eq!(gekuerzt, "Der Agent bezahlt");
        assert!(weggelassen.is_some());
        assert_eq!(
            gekuerzt.chars().count() + weggelassen.unwrap(),
            text.chars().count()
        );
        assert!(!gekuerzt.ends_with(' '));
    }

    #[test]
    fn kuerzen_bricht_lange_woerter_notfalls_hart() {
        let (gekuerzt, weggelassen) = kuerzen("aaaaaaaaaaaaaaaaaaaa", 5);
        assert_eq!(gekuerzt, "aaaaa");
        assert_eq!(weggelassen, Some(15));
    }

    #[test]
    fn kuerzen_zaehlt_zeichen_nicht_bytes() {
        let text = "Grüße für äöü";
        let (gekuerzt, _) = kuerzen(text, 5);
        assert_eq!(gekuerzt, "Grüße");
    }

    #[test]
    fn kuerzen_mit_null_zeichen() {
        assert_eq!(kuerzen("abc", 0), (String::new(), Some(3)));
        assert_eq!(kuerzen("", 0), (String::new(), None));
    }

    #[test]
    fn ohne_profil_ist_ein_hinweis_und_kein_fehler() {
        let ergebnis = zugang("Outline-Wiki", Err(ProfilFehler::Keins));
        let antwort = ergebnis.unwrap_err().in_ergebnis().unwrap();
        let text = antwort.content[0].as_text().unwrap().text.clone();
        assert!(text.contains("Noch kein Zugang eingerichtet"));
        assert!(text.contains("Outline-Wiki"));
    }

    #[test]
    fn anderer_fehler_wird_zum_mcp_fehler() {
        let ergebnis = zugang(
            "Outline-Wiki",
            Err(ProfilFehler::Unbekannt("gibtsnicht".into())),
        );
        let fehler = ergebnis.unwrap_err().in_ergebnis().unwrap_err();
        assert!(fehler.message.contains("gibtsnicht"));
    }
}
