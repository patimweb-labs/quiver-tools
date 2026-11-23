// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Was jedes Tool für MCP braucht (SPEC §2, §6): den Hinweis statt eines
//! Fehlers, wenn der Zugang fehlt, das lesende Tool `<name>_profile` und das
//! Kürzen langer Texte.

use rmcp::model::{
    CallToolResult, ContentBlock, Implementation, JsonObject, Tool, ToolAnnotations,
};
use rmcp::ErrorData as McpError;

use crate::hinweis_zugang;
use crate::profil::{Profil, ProfilFehler};
use crate::umgebung::Zugang;

/// Was ein Tool zurückgibt, wenn kein Zugang bereitsteht.
#[derive(Debug)]
pub enum OhneProfil {
    /// Zugang fehlt: ein Text, **kein** MCP-Fehler (SPEC §2).
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
/// [`ProfilFehler::Keins`] wird zum Hinweis auf das quiver-Fenster – ein Text,
/// **kein** MCP-Fehler (SPEC §2).
pub fn zugang(titel: &str, ergebnis: Result<Profil, ProfilFehler>) -> Result<Profil, OhneProfil> {
    ergebnis.map_err(|_| OhneProfil::Hinweis(antwort(hinweis_zugang(titel))))
}

/// Der Name, unter dem sich das Tool bei `initialize` meldet:
/// `quiver-<werkzeug>` mit der Version des Pakets.
pub fn server_info(zugang: &Zugang) -> Implementation {
    Implementation::new(
        format!("quiver-{}", zugang.werkzeug()),
        env!("CARGO_PKG_VERSION"),
    )
    .with_title(zugang.titel().to_string())
}

/// Name des lesenden Profil-Tools: `<name>_profile`.
pub fn profile_tool_name(zugang: &Zugang) -> String {
    format!("{}_profile", zugang.werkzeug())
}

/// Das lesende Tool `<name>_profile`: es ändert nichts, es zeigt den Zugang.
pub fn profile_tool(zugang: &Zugang) -> Tool {
    Tool::new(
        profile_tool_name(zugang),
        format!(
            "Zeigt den Zugang für {} aus der Umgebung: Name und nicht-geheime Felder. Geheimnisse stehen nie darin.",
            zugang.titel()
        ),
        JsonObject::new(),
    )
    .annotate(ToolAnnotations::from_raw(
        Some(format!("Zugang von {}", zugang.titel())),
        Some(true),
        Some(false),
        Some(true),
        Some(false),
    ))
}

/// Antwort für `<name>_profile`: das eine Profil, ohne Geheimnisse.
///
/// Fehlt der Zugang, kommt der Hinweis auf das quiver-Fenster – wie bei jedem
/// anderen Tool.
pub fn profile_antwort(zugang: &Zugang) -> Result<CallToolResult, McpError> {
    // Nur die nicht-geheimen Werte: die Geheimnisse kommen hier nie an.
    let profil = match zugang.profil() {
        Ok(profil) => profil,
        Err(_) => return Ok(antwort(hinweis_zugang(zugang.titel()))),
    };

    let mut text = format!(
        "Zugang für {} (aus der Umgebung):\n- {}",
        zugang.titel(),
        profil.name
    );
    for (name, wert) in &profil.werte {
        if wert.is_empty() {
            continue;
        }
        text.push_str(&format!("\n  {name}: {wert}"));
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
    fn ohne_zugang_ist_ein_hinweis_und_kein_fehler() {
        let ergebnis = zugang("Outline-Wiki", Err(ProfilFehler::Keins));
        let antwort = ergebnis.unwrap_err().in_ergebnis().unwrap();
        let text = antwort.content[0].as_text().unwrap().text.clone();
        assert!(text.contains("Zugang fehlt"), "{text}");
        assert!(text.contains("Outline-Wiki"), "{text}");
    }
}
