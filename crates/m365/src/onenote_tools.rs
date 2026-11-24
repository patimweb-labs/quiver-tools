// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! OneNote-Tools (docs/aufgaben/06-graph-erweiterung.md).
//!
//! Gelesen wird mit `html_zu_text` (dieselbe Umwandlung wie bei Teams),
//! geschrieben als HTML mit escaptem Titel und Text. Namen (Notizbuch,
//! Abschnitt) dürfen Ids ersetzen; mehrdeutig → Kandidaten und keine Aktion.

use m365_anmeldung::Anmeldung;
use m365_graph::{Abschnitt, Notizbuch, Seite};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::ErrorData;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::aufloesen::{ist_graph_id, waehlen, Anzeige};
use crate::kodierung::kodieren;
use crate::onenote::{
    abschnitt_json, max_zeichen_begrenzen, notizbuch_json, seite_als_html, seite_als_text,
    seite_json,
};
use crate::server::{als_json, ergebnis_erweitert, ungueltig_tool, Quiver, ToolFehler};

/// So viele Notizbücher/Abschnitte/Seiten holt eine Suche höchstens.
const HOECHSTENS_LISTE: usize = 200;

/// `$select` für Notizbücher und Abschnitte.
///
/// **Kein `webUrl`**: Microsoft Graph kennt dieses Feld bei OneNote-Notizbüchern
/// und -Abschnitten nicht (`Could not find a property named 'webUrl'`,
/// Aufgabe 07). Der Link kommt aus `links.oneNoteWebUrl.href`.
const NOTEBOOK_FELDER: &str = "id,displayName,createdDateTime,lastModifiedDateTime";

/// `$select` für Seiten (Aufgabe 08: auch Seiten kennen kein `webUrl`).
const PAGE_FELDER: &str = "id,title,createdDateTime,lastModifiedDateTime";

// ---------------------------------------------------------------------------
// Parameter der Tools
// ---------------------------------------------------------------------------

/// Parameter für `onenote_abschnitte`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct OneNoteAbschnitteAnfrage {
    /// Nur die Abschnitte dieses Notizbuchs (Id oder Name).
    #[serde(default)]
    pub notizbuch: Option<String>,
}

/// Parameter für `onenote_seiten`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct OneNoteSeitenAnfrage {
    /// Abschnitt als Id oder Name (aus `onenote_abschnitte`).
    pub abschnitt: String,
    /// Wie viele Seiten (Standard 20, höchstens 100).
    #[serde(default)]
    pub anzahl: Option<u32>,
}

/// Parameter für `onenote_seite_lesen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct OneNoteSeiteLesenAnfrage {
    /// Id der Seite (aus `onenote_seiten`).
    pub seite: String,
    /// Höchstens so viele Zeichen Text (Standard 20 000, höchstens 50 000).
    #[serde(default)]
    pub max_zeichen: Option<u32>,
}

/// Parameter für `onenote_seite_anlegen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct OneNoteSeiteAnlegenAnfrage {
    /// Abschnitt als Id oder Name, in dem die Seite landet.
    pub abschnitt: String,
    /// Titel der neuen Seite.
    pub titel: String,
    /// Der Text der Seite (Pflicht). Er wird HTML-escaped eingesetzt.
    pub text: String,
}

fn anzahl_mit(anzahl: Option<u32>, standard: u32) -> u32 {
    anzahl.unwrap_or(standard).clamp(1, 100)
}

// ---------------------------------------------------------------------------
// Die Tools
// ---------------------------------------------------------------------------

#[tool_router(router = onenote_router, vis = "pub(crate)")]
impl<A: Anmeldung + 'static> Quiver<A> {
    /// Die Notizbücher des Users.
    #[tool(
        description = "OneNote-Notizbücher auflisten (Id und Name).",
        annotations(read_only_hint = true)
    )]
    pub async fn onenote_notizbuecher(&self) -> Result<String, ErrorData> {
        ergebnis_erweitert(self.onenote_notizbuecher_lauf().await)
    }

    /// Die Abschnitte – insgesamt oder eines Notizbuchs.
    #[tool(
        description = "OneNote-Abschnitte auflisten – alle oder die eines Notizbuchs.",
        annotations(read_only_hint = true)
    )]
    pub async fn onenote_abschnitte(
        &self,
        Parameters(anfrage): Parameters<OneNoteAbschnitteAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis_erweitert(self.onenote_abschnitte_lauf(anfrage).await)
    }

    /// Die Seiten eines Abschnitts.
    #[tool(
        description = "OneNote-Seiten eines Abschnitts auflisten (Titel, Link, geändert).",
        annotations(read_only_hint = true)
    )]
    pub async fn onenote_seiten(
        &self,
        Parameters(anfrage): Parameters<OneNoteSeitenAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis_erweitert(self.onenote_seiten_lauf(anfrage).await)
    }

    /// Eine Seite als Text lesen.
    #[tool(
        description = "Eine OneNote-Seite als Text lesen (HTML wird umgewandelt, Länge mit max_zeichen begrenzt).",
        annotations(read_only_hint = true)
    )]
    pub async fn onenote_seite_lesen(
        &self,
        Parameters(anfrage): Parameters<OneNoteSeiteLesenAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis_erweitert(self.onenote_seite_lesen_lauf(anfrage).await)
    }

    /// Eine Seite anlegen.
    #[tool(
        description = "Eine OneNote-Seite in einem Abschnitt anlegen. Verändert das Notizbuch.",
        annotations(destructive_hint = true)
    )]
    pub async fn onenote_seite_anlegen(
        &self,
        Parameters(anfrage): Parameters<OneNoteSeiteAnlegenAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis_erweitert(self.onenote_seite_anlegen_lauf(anfrage).await)
    }
}

// ---------------------------------------------------------------------------
// Auflösen
// ---------------------------------------------------------------------------

impl<A: Anmeldung + 'static> Quiver<A> {
    async fn notizbuch_finden(&self, referenz: &str) -> Result<Anzeige, ToolFehler> {
        let referenz = referenz.trim();
        if referenz.is_empty() {
            return Err(ungueltig_tool("notizbuch fehlt"));
        }
        if ist_graph_id(referenz) {
            return Ok(Anzeige::neu(referenz, None));
        }
        let notizbuecher = self
            .graph
            .get_seiten::<Notizbuch>(
                "/me/onenote/notebooks",
                &[("$select", NOTEBOOK_FELDER.to_string())],
                HOECHSTENS_LISTE,
            )
            .await?;
        let kandidaten: Vec<Anzeige> = notizbuecher
            .into_iter()
            .map(|notizbuch| Anzeige::neu(notizbuch.id, notizbuch.display_name.as_deref()))
            .collect();
        waehlen(kandidaten, referenz, "Notizbuch")
    }

    async fn abschnitt_finden(&self, referenz: &str) -> Result<Anzeige, ToolFehler> {
        let referenz = referenz.trim();
        if referenz.is_empty() {
            return Err(ungueltig_tool("abschnitt fehlt"));
        }
        if ist_graph_id(referenz) {
            return Ok(Anzeige::neu(referenz, None));
        }
        let abschnitte = self
            .graph
            .get_seiten::<Abschnitt>(
                "/me/onenote/sections",
                &[("$select", NOTEBOOK_FELDER.to_string())],
                HOECHSTENS_LISTE,
            )
            .await?;
        let kandidaten: Vec<Anzeige> = abschnitte
            .into_iter()
            .map(|abschnitt| Anzeige::neu(abschnitt.id, abschnitt.display_name.as_deref()))
            .collect();
        waehlen(kandidaten, referenz, "Abschnitt")
    }
}

// ---------------------------------------------------------------------------
// Umsetzung der Tools
// ---------------------------------------------------------------------------

impl<A: Anmeldung + 'static> Quiver<A> {
    async fn onenote_notizbuecher_lauf(&self) -> Result<String, ToolFehler> {
        let notizbuecher = self
            .graph
            .get_seiten::<Notizbuch>(
                "/me/onenote/notebooks",
                &[("$select", NOTEBOOK_FELDER.to_string())],
                HOECHSTENS_LISTE,
            )
            .await?;
        let liste: Vec<Value> = notizbuecher.iter().map(notizbuch_json).collect();
        als_json(&liste).map_err(ToolFehler::from)
    }

    async fn onenote_abschnitte_lauf(
        &self,
        anfrage: OneNoteAbschnitteAnfrage,
    ) -> Result<String, ToolFehler> {
        let (pfad, wo) = match anfrage
            .notizbuch
            .as_deref()
            .map(str::trim)
            .filter(|notizbuch| !notizbuch.is_empty())
        {
            Some(referenz) => {
                let notizbuch = self.notizbuch_finden(referenz).await?;
                (
                    format!("/me/onenote/notebooks/{}/sections", kodieren(&notizbuch.id)),
                    Some(notizbuch.text()),
                )
            }
            None => ("/me/onenote/sections".to_string(), None),
        };
        let abschnitte = self
            .graph
            .get_seiten::<Abschnitt>(
                &pfad,
                &[("$select", NOTEBOOK_FELDER.to_string())],
                HOECHSTENS_LISTE,
            )
            .await?;
        let liste: Vec<Value> = abschnitte.iter().map(abschnitt_json).collect();
        let mut antwort = serde_json::json!({"abschnitte": liste});
        if let Some(wo) = wo {
            antwort["notizbuch"] = serde_json::json!(wo);
        }
        als_json(&antwort).map_err(ToolFehler::from)
    }

    async fn onenote_seiten_lauf(
        &self,
        anfrage: OneNoteSeitenAnfrage,
    ) -> Result<String, ToolFehler> {
        let abschnitt = self.abschnitt_finden(&anfrage.abschnitt).await?;
        let anzahl = anzahl_mit(anfrage.anzahl, 20);
        let seiten = self
            .graph
            .get_seiten::<Seite>(
                &format!("/me/onenote/sections/{}/pages", kodieren(&abschnitt.id)),
                &[
                    ("$select", PAGE_FELDER.to_string()),
                    ("$top", anzahl.to_string()),
                ],
                anzahl as usize,
            )
            .await?;
        let liste: Vec<Value> = seiten.iter().map(seite_json).collect();
        als_json(&serde_json::json!({"abschnitt": abschnitt.text(), "seiten": liste}))
            .map_err(ToolFehler::from)
    }

    async fn onenote_seite_lesen_lauf(
        &self,
        anfrage: OneNoteSeiteLesenAnfrage,
    ) -> Result<String, ToolFehler> {
        let seite = anfrage.seite.trim();
        if seite.is_empty() {
            return Err(ungueltig_tool("seite fehlt"));
        }
        let max_zeichen = max_zeichen_begrenzen(anfrage.max_zeichen);

        let kopf: Seite = self
            .graph
            .get(
                &format!("/me/onenote/pages/{}", kodieren(seite)),
                &[("$select", PAGE_FELDER.to_string())],
            )
            .await?;
        let html = self
            .graph
            .get_text(
                &format!("/me/onenote/pages/{}/content", kodieren(seite)),
                &[],
            )
            .await?;
        let (text, gekuerzt, gesamt) = seite_als_text(&html, max_zeichen);

        let mut antwort = seite_json(&kopf);
        antwort["text"] = serde_json::json!(text);
        if let Some(gekuerzt) = gekuerzt {
            antwort["gekuerzt"] = serde_json::json!(gekuerzt);
            antwort["laenge_gesamt"] = serde_json::json!(gesamt);
        }
        als_json(&antwort).map_err(ToolFehler::from)
    }

    async fn onenote_seite_anlegen_lauf(
        &self,
        anfrage: OneNoteSeiteAnlegenAnfrage,
    ) -> Result<String, ToolFehler> {
        let titel = anfrage.titel.trim();
        if titel.is_empty() {
            return Err(ungueltig_tool("titel fehlt"));
        }
        if anfrage.text.trim().is_empty() {
            return Err(ungueltig_tool("text fehlt"));
        }
        let abschnitt = self.abschnitt_finden(&anfrage.abschnitt).await?;
        let html = seite_als_html(titel, &anfrage.text);

        let neu: Seite = self
            .graph
            .post_text(
                &format!("/me/onenote/sections/{}/pages", kodieren(&abschnitt.id)),
                &html,
                "text/html",
            )
            .await?;
        let mut antwort = seite_json(&neu);
        antwort["abschnitt"] = serde_json::json!(abschnitt.text());
        antwort["angelegt"] = serde_json::json!(true);
        als_json(&antwort).map_err(ToolFehler::from)
    }
}
