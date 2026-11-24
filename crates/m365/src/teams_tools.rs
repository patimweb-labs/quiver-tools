// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Teams-Tools aus docs/aufgaben/05-teams.md – Graph v1.0, **als der User**.
//!
//! Regeln, die für jedes Tool gelten:
//!
//! * `team`, `kanal` und `chat` dürfen Id **oder** Name sein (bei Chats auch das
//!   Thema oder ein Mitglied). Aufgelöst wird über die Listen; mehrere Treffer
//!   ergeben eine Meldung mit den Kandidaten und **keine** Aktion.
//! * Gelesen wird über `@odata.nextLink`, aber nie mehr Seiten als nötig.
//! * Gesendet wird genau einmal – kein Wiederholen nach 429 (doppelte Nachricht).
//! * Antworten sind knappes JSON: HTML→Text, Länge begrenzt, Anhänge nur als Name,
//!   gelöschte und System-Nachrichten weg.
//! * Aufgabe 09: `setReaction`, `markChatReadForUser` und `softDelete` antworten mit
//!   **204 und leerem Rumpf** – Erfolg, ohne Modell zu erwarten. Löschen heißt bei
//!   Teams `softDelete`: nur eigene Nachrichten, wiederherstellbar.

use jiff::Timestamp;
use m365_anmeldung::{Anmeldung, Token};
use m365_graph::{
    mandant_aus_token, Chat, GraphFehler, Kanal, Mitglied, Profil, Sammlung, SucheAntwort, Team,
    TeamsNachricht, Treffer, Vorschau,
};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::ErrorData;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::server::{als_json, ergebnis, ungueltig_tool, Quiver, ToolFehler};
use crate::teams::{
    beitrag_json, chat_titel, gesendet_json, html_zu_text, innerhalb, ist_neuer,
    max_zeichen_begrenzen, mitglied_json, nachricht_json, sichtbar, text_kuerzen, treffer_json,
    vorschau_absender,
};

/// So viele Einträge holt eine Namenssuche höchstens (Teams/Channels/Chats).
const HOECHSTENS_LISTE: usize = 200;
/// So viele Antworten eines Threads werden gezählt bzw. geliefert.
const HOECHSTENS_ANTWORTEN: usize = 50;
/// So viele Tage zurück sieht `teams_eingang` standardmäßig.
const STANDARD_TAGE: u32 = 3;
/// So viele Zeichen einer Chat-Vorschau stehen in `teams_chats`.
const VORSCHAU_ZEICHEN: usize = 200;

/// Der Satz, wenn `teams_mitglieder` mit 403 antwortet.
///
/// Ein 403 heißt hier fast immer: das delegierte Recht `TeamMember.Read.All`
/// fehlt (`Authorization_RequestDenied`). Der Rohfehler hilft niemandem, der
/// Satz schon (Aufgabe 07).
pub const MITGLIEDER_HINWEIS: &str =
    "Für `teams_mitglieder` fehlt das Recht `TeamMember.Read.All` – im quiver-Fenster unter Tools → Microsoft 365 erneut auf „Anmelden“ klicken (in vielen Tenants gibt ein Admin das Recht einmal frei).";

// ---------------------------------------------------------------------------
// Parameter der Tools (Namen und Beschreibungen deutsch, wie im Rest des Repos)
// ---------------------------------------------------------------------------

/// Parameter für `teams_eingang`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TeamsEingangAnfrage {
    /// So viele Tage zurück werden Erwähnungen gesucht (Standard 3, höchstens 90).
    #[serde(default)]
    pub tage: Option<u32>,
}

/// Parameter für `teams_kanaele`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TeamsKanaeleAnfrage {
    /// Team als Id oder Name, z. B. `Entwicklung` (aus `teams_liste`).
    pub team: String,
}

/// Parameter für `teams_kanal_lesen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TeamsKanalLesenAnfrage {
    /// Team als Id oder Name.
    pub team: String,
    /// Kanal als Id oder Name, z. B. `Allgemein` (aus `teams_kanaele`).
    pub kanal: String,
    /// Wie viele Beiträge (Standard 20, höchstens 50).
    #[serde(default)]
    pub anzahl: Option<u32>,
    /// Höchstens so viele Zeichen je Beitrag (Standard 2000, höchstens 50 000).
    #[serde(default)]
    pub max_zeichen: Option<u32>,
}

/// Parameter für `teams_thread_lesen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TeamsThreadLesenAnfrage {
    /// Team als Id oder Name.
    pub team: String,
    /// Kanal als Id oder Name.
    pub kanal: String,
    /// Id des Beitrags (aus `teams_kanal_lesen`).
    pub nachricht: String,
    /// Höchstens so viele Zeichen je Nachricht (Standard 2000, höchstens 50 000).
    #[serde(default)]
    pub max_zeichen: Option<u32>,
}

/// Parameter für `teams_kanal_senden`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TeamsKanalSendenAnfrage {
    /// Team als Id oder Name.
    pub team: String,
    /// Kanal als Id oder Name.
    pub kanal: String,
    /// Der Text des Beitrags (Pflicht).
    pub text: String,
    /// `true`: `text` ist bereits HTML. Standard `false`: der Text wird
    /// HTML-escaped und Zeilenumbrüche werden zu `<br>`.
    #[serde(default)]
    pub als_html: Option<bool>,
}

/// Parameter für `teams_kanal_antworten`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TeamsKanalAntwortenAnfrage {
    /// Team als Id oder Name.
    pub team: String,
    /// Kanal als Id oder Name.
    pub kanal: String,
    /// Id des Beitrags, auf den geantwortet wird.
    pub nachricht: String,
    /// Der Text der Antwort (Pflicht).
    pub text: String,
    /// `true`: `text` ist bereits HTML (Standard `false`).
    #[serde(default)]
    pub als_html: Option<bool>,
}

/// Parameter für `teams_chats`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TeamsChatsAnfrage {
    /// Wie viele Chats (Standard 25, höchstens 50).
    #[serde(default)]
    pub anzahl: Option<u32>,
}

/// Parameter für `teams_chat_lesen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TeamsChatLesenAnfrage {
    /// Chat als Id, Thema oder Mitgliedsname (aus `teams_chats`).
    pub chat: String,
    /// Wie viele Nachrichten (Standard 20, höchstens 50).
    #[serde(default)]
    pub anzahl: Option<u32>,
    /// Höchstens so viele Zeichen je Nachricht (Standard 2000, höchstens 50 000).
    #[serde(default)]
    pub max_zeichen: Option<u32>,
}

/// Parameter für `teams_chat_senden`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TeamsChatSendenAnfrage {
    /// Chat als Id, Thema oder Mitgliedsname.
    pub chat: String,
    /// Der Text der Nachricht (Pflicht).
    pub text: String,
    /// `true`: `text` ist bereits HTML (Standard `false`).
    #[serde(default)]
    pub als_html: Option<bool>,
}

/// Parameter für `teams_chat_anlegen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TeamsChatAnlegenAnfrage {
    /// Die anderen Teilnehmer, je E-Mail-Adresse, UPN oder Name.
    pub teilnehmer: Vec<String>,
    /// Thema des Gruppenchats (nur bei mehreren Teilnehmern).
    #[serde(default)]
    pub thema: Option<String>,
    /// Erste Nachricht, die gleich in den neuen Chat geht (optional).
    #[serde(default)]
    pub nachricht: Option<String>,
}

/// Parameter für `teams_suchen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TeamsSuchenAnfrage {
    /// Suchtext (Microsoft Search über alle Teams-Nachrichten des Users).
    pub suche: String,
    /// Wie viele Treffer (Standard 25, höchstens 50).
    #[serde(default)]
    pub anzahl: Option<u32>,
    /// Höchstens so viele Zeichen je Treffertext (Standard 2000).
    #[serde(default)]
    pub max_zeichen: Option<u32>,
}

/// Parameter für `teams_mitglieder`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TeamsMitgliederAnfrage {
    /// Team als Id oder Name.
    pub team: String,
    /// Nur die Mitglieder dieses Kanals (optional, Id oder Name).
    #[serde(default)]
    pub kanal: Option<String>,
}

/// Parameter für `teams_person_finden`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TeamsPersonFindenAnfrage {
    /// Name, E-Mail-Adresse oder UPN der gesuchten Person.
    pub suche: String,
    /// Wie viele Treffer (Standard 10, höchstens 50).
    #[serde(default)]
    pub anzahl: Option<u32>,
}

/// Parameter für `teams_reagieren`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TeamsReagierenAnfrage {
    /// Chat als Id, Thema oder Mitgliedsname – **oder** `team` + `kanal`.
    #[serde(default)]
    pub chat: Option<String>,
    /// Team als Id oder Name (zusammen mit `kanal` statt `chat`).
    #[serde(default)]
    pub team: Option<String>,
    /// Kanal als Id oder Name (zusammen mit `team`).
    #[serde(default)]
    pub kanal: Option<String>,
    /// Id der Nachricht, auf die reagiert wird.
    pub nachricht: String,
    /// Die Reaktion als Wort (`like`, `love`, `laugh`, `surprised`, `sad`,
    /// `angry`) oder direkt als Emoji (👍 ❤️ 😆 😮 😢 😡).
    pub reaktion: String,
    /// `true`: die Reaktion wieder entfernen (Standard `false`).
    #[serde(default)]
    pub entfernen: Option<bool>,
}

/// Parameter für `teams_als_gelesen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TeamsAlsGelesenAnfrage {
    /// Chat als Id, Thema oder Mitgliedsname.
    pub chat: String,
    /// `true` (Standard): als gelesen markieren. `false`: wieder ungelesen.
    #[serde(default)]
    pub gelesen: Option<bool>,
}

/// Parameter für `teams_nachricht_loeschen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TeamsNachrichtLoeschenAnfrage {
    /// Chat als Id, Thema oder Mitgliedsname – **oder** `team` + `kanal`.
    #[serde(default)]
    pub chat: Option<String>,
    /// Team als Id oder Name (zusammen mit `kanal` statt `chat`).
    #[serde(default)]
    pub team: Option<String>,
    /// Kanal als Id oder Name (zusammen mit `team`).
    #[serde(default)]
    pub kanal: Option<String>,
    /// Id der Nachricht, die gelöscht wird.
    pub nachricht: String,
}

// ---------------------------------------------------------------------------
// Namen → Id
// ---------------------------------------------------------------------------

/// Ein aufgelöster Treffer: Id für Graph, Name für die Antwort.
#[derive(Debug, Clone)]
struct Anzeige {
    id: String,
    name: String,
}

impl Anzeige {
    fn neu(id: impl Into<String>, name: Option<&str>) -> Self {
        let id = id.into();
        let name = name
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(&id)
            .to_string();
        Self { id, name }
    }

    fn text(&self) -> String {
        if self.name == self.id {
            self.id.clone()
        } else {
            format!("{} ({})", self.name, self.id)
        }
    }
}

/// Eine Person aus `GET /users`.
#[derive(Debug, Clone)]
struct Person {
    id: String,
    name: String,
    mail: Option<String>,
    upn: Option<String>,
}

impl Person {
    fn text(&self) -> String {
        match (&self.mail, &self.upn) {
            (Some(mail), _) => format!("{} <{mail}> ({})", self.name, self.id),
            (None, Some(upn)) => format!("{} <{upn}> ({})", self.name, self.id),
            _ => format!("{} ({})", self.name, self.id),
        }
    }
}

fn ist_guid(text: &str) -> bool {
    let text = text.trim();
    text.len() == 36
        && text.chars().enumerate().all(|(stelle, zeichen)| {
            if matches!(stelle, 8 | 13 | 18 | 23) {
                zeichen == '-'
            } else {
                zeichen.is_ascii_hexdigit()
            }
        })
}

/// Sieht der Text nach einer Id aus (GUID oder `19:…`/`48:…` von Teams)?
fn ist_id(text: &str) -> bool {
    let text = text.trim();
    ist_guid(text) || text.starts_with("19:") || text.starts_with("48:")
}

fn mehrdeutig(art: &str, referenz: &str, treffer: &[&Anzeige]) -> ToolFehler {
    let liste = treffer
        .iter()
        .take(8)
        .map(|treffer| treffer.text())
        .collect::<Vec<String>>()
        .join(", ");
    ungueltig_tool(format!(
        "{art} {referenz:?} ist mehrdeutig – {} Treffer: {liste}. Bitte die Id nennen; es wurde nichts geändert.",
        treffer.len()
    ))
}

/// Genau ein Treffer über Id, genauer Name, sonst Name-Teil.
fn waehlen(kandidaten: Vec<Anzeige>, referenz: &str, art: &str) -> Result<Anzeige, ToolFehler> {
    let referenz = referenz.trim();
    let klein = referenz.to_lowercase();

    if let Some(treffer) = kandidaten
        .iter()
        .find(|kandidat| kandidat.id.eq_ignore_ascii_case(referenz))
    {
        return Ok(treffer.clone());
    }

    let genau: Vec<&Anzeige> = kandidaten
        .iter()
        .filter(|kandidat| kandidat.name.to_lowercase() == klein)
        .collect();
    match genau.len() {
        1 => return Ok(genau[0].clone()),
        0 => {}
        _ => return Err(mehrdeutig(art, referenz, &genau)),
    }

    let teil: Vec<&Anzeige> = kandidaten
        .iter()
        .filter(|kandidat| kandidat.name.to_lowercase().contains(&klein))
        .collect();
    match teil.len() {
        1 => Ok(teil[0].clone()),
        0 => Err(ungueltig_tool(format!(
            "{art} {referenz:?} nicht gefunden. Bitte die Liste abrufen und die Id nennen."
        ))),
        _ => Err(mehrdeutig(art, referenz, &teil)),
    }
}

fn anzahl_mit(anzahl: Option<u32>, standard: u32, hoechstens: u32) -> u32 {
    anzahl.unwrap_or(standard).clamp(1, hoechstens)
}

// ---------------------------------------------------------------------------
// Die Tools
// ---------------------------------------------------------------------------

#[tool_router(router = teams_router, vis = "pub(crate)")]
impl<A: Anmeldung + 'static> Quiver<A> {
    // -- lesen ------------------------------------------------------------

    /// Was liegen geblieben ist: ungelesene Chats und Erwähnungen.
    #[tool(
        description = "Teams-Eingang: Chats mit ungelesenen Nachrichten und Erwähnungen des Users. Für „was habe ich verpasst“.",
        annotations(read_only_hint = true)
    )]
    pub async fn teams_eingang(
        &self,
        Parameters(anfrage): Parameters<TeamsEingangAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis(self.teams_eingang_lauf(anfrage).await)
    }

    /// Alle Teams, in denen der User Mitglied ist.
    #[tool(
        description = "Teams auflisten, in denen der User Mitglied ist (Id und Name).",
        annotations(read_only_hint = true)
    )]
    pub async fn teams_liste(&self) -> Result<String, ErrorData> {
        ergebnis(self.teams_liste_lauf().await)
    }

    /// Die Kanäle eines Teams.
    #[tool(
        description = "Kanäle eines Teams auflisten (Id und Name).",
        annotations(read_only_hint = true)
    )]
    pub async fn teams_kanaele(
        &self,
        Parameters(anfrage): Parameters<TeamsKanaeleAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis(self.teams_kanaele_lauf(anfrage).await)
    }

    /// Beiträge eines Kanals, je Beitrag die Anzahl der Antworten.
    #[tool(
        description = "Beiträge eines Teams-Kanals lesen (neueste zuerst), je Beitrag die Anzahl der Antworten.",
        annotations(read_only_hint = true)
    )]
    pub async fn teams_kanal_lesen(
        &self,
        Parameters(anfrage): Parameters<TeamsKanalLesenAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis(self.teams_kanal_lesen_lauf(anfrage).await)
    }

    /// Ein Beitrag und alle Antworten.
    #[tool(
        description = "Einen Kanalbeitrag mit allen Antworten lesen.",
        annotations(read_only_hint = true)
    )]
    pub async fn teams_thread_lesen(
        &self,
        Parameters(anfrage): Parameters<TeamsThreadLesenAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis(self.teams_thread_lesen_lauf(anfrage).await)
    }

    /// Chats mit Mitgliedern und Vorschau.
    #[tool(
        description = "Die Chats des Users: Thema, Art, Mitglieder, Vorschau der letzten Nachricht.",
        annotations(read_only_hint = true)
    )]
    pub async fn teams_chats(
        &self,
        Parameters(anfrage): Parameters<TeamsChatsAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis(self.teams_chats_lauf(anfrage).await)
    }

    /// Nachrichten eines Chats.
    #[tool(
        description = "Die Nachrichten eines Chats lesen (neueste zuerst).",
        annotations(read_only_hint = true)
    )]
    pub async fn teams_chat_lesen(
        &self,
        Parameters(anfrage): Parameters<TeamsChatLesenAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis(self.teams_chat_lesen_lauf(anfrage).await)
    }

    /// Teams-Nachrichten durchsuchen.
    #[tool(
        description = "Teams-Nachrichten durchsuchen (Chats und Kanäle, Microsoft Search).",
        annotations(read_only_hint = true)
    )]
    pub async fn teams_suchen(
        &self,
        Parameters(anfrage): Parameters<TeamsSuchenAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis(self.teams_suchen_lauf(anfrage).await)
    }

    /// Mitglieder eines Teams oder Kanals.
    #[tool(
        description = "Mitglieder eines Teams oder – mit `kanal` – eines Kanals auflisten.",
        annotations(read_only_hint = true)
    )]
    pub async fn teams_mitglieder(
        &self,
        Parameters(anfrage): Parameters<TeamsMitgliederAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis(self.teams_mitglieder_lauf(anfrage).await)
    }

    /// Personen im Tenant finden.
    #[tool(
        description = "Personen im Tenant über Name, E-Mail oder UPN finden (für Chats und Erwähnungen).",
        annotations(read_only_hint = true)
    )]
    pub async fn teams_person_finden(
        &self,
        Parameters(anfrage): Parameters<TeamsPersonFindenAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis(self.teams_person_finden_lauf(anfrage).await)
    }

    // -- schreiben (verändernd) -------------------------------------------

    /// Einen Beitrag in einen Kanal schreiben.
    #[tool(
        description = "Einen Beitrag in einen Teams-Kanal schreiben. Erscheint als Nachricht des Users.",
        annotations(destructive_hint = true)
    )]
    pub async fn teams_kanal_senden(
        &self,
        Parameters(anfrage): Parameters<TeamsKanalSendenAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis(self.teams_kanal_senden_lauf(anfrage).await)
    }

    /// Auf einen Kanalbeitrag antworten.
    #[tool(
        description = "Auf einen Kanalbeitrag antworten (im selben Thread). Erscheint als Nachricht des Users.",
        annotations(destructive_hint = true)
    )]
    pub async fn teams_kanal_antworten(
        &self,
        Parameters(anfrage): Parameters<TeamsKanalAntwortenAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis(self.teams_kanal_antworten_lauf(anfrage).await)
    }

    /// Eine Chat-Nachricht senden.
    #[tool(
        description = "Eine Nachricht in einen Chat senden. Erscheint als Nachricht des Users.",
        annotations(destructive_hint = true)
    )]
    pub async fn teams_chat_senden(
        &self,
        Parameters(anfrage): Parameters<TeamsChatSendenAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis(self.teams_chat_senden_lauf(anfrage).await)
    }

    /// Einen Chat anlegen.
    #[tool(
        description = "Einen 1:1- oder Gruppenchat anlegen, optional mit erster Nachricht.",
        annotations(destructive_hint = true)
    )]
    pub async fn teams_chat_anlegen(
        &self,
        Parameters(anfrage): Parameters<TeamsChatAnlegenAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis(self.teams_chat_anlegen_lauf(anfrage).await)
    }

    /// Auf eine Nachricht reagieren.
    #[tool(
        description = "Auf eine Teams-Nachricht reagieren – oder die Reaktion wieder entfernen.",
        annotations(destructive_hint = true)
    )]
    pub async fn teams_reagieren(
        &self,
        Parameters(anfrage): Parameters<TeamsReagierenAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis(self.teams_reagieren_lauf(anfrage).await)
    }

    /// Einen Chat als gelesen (oder ungelesen) markieren.
    #[tool(
        description = "Einen Chat als gelesen – oder wieder ungelesen – markieren (Lesemarke des Users).",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    pub async fn teams_als_gelesen(
        &self,
        Parameters(anfrage): Parameters<TeamsAlsGelesenAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis(self.teams_als_gelesen_lauf(anfrage).await)
    }

    /// Eine eigene Nachricht löschen (softDelete, wiederherstellbar).
    #[tool(
        description = "Eine eigene Teams-Nachricht löschen (softDelete – wiederherstellbar, fremde Nachrichten lehnt Graph ab). Verändert den Chat bzw. Kanal.",
        annotations(destructive_hint = true)
    )]
    pub async fn teams_nachricht_loeschen(
        &self,
        Parameters(anfrage): Parameters<TeamsNachrichtLoeschenAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis(self.teams_nachricht_loeschen_lauf(anfrage).await)
    }
}

// ---------------------------------------------------------------------------
// Auflösen (jede Anfrage an Graph kann hier scheitern → ToolFehler)
// ---------------------------------------------------------------------------

impl<A: Anmeldung + 'static> Quiver<A> {
    async fn team_finden(&self, referenz: &str) -> Result<Anzeige, ToolFehler> {
        let referenz = referenz.trim();
        if referenz.is_empty() {
            return Err(ungueltig_tool("team fehlt"));
        }
        if ist_guid(referenz) {
            return Ok(Anzeige {
                id: referenz.to_string(),
                name: referenz.to_string(),
            });
        }
        let teams = self
            .graph
            .get_seiten::<Team>("/me/joinedTeams", &[], HOECHSTENS_LISTE)
            .await?;
        let kandidaten = teams
            .into_iter()
            .map(|team| Anzeige::neu(team.id, team.display_name.as_deref()))
            .collect();
        waehlen(kandidaten, referenz, "Team")
    }

    async fn kanal_finden(&self, team: &Anzeige, referenz: &str) -> Result<Anzeige, ToolFehler> {
        let referenz = referenz.trim();
        if referenz.is_empty() {
            return Err(ungueltig_tool("kanal fehlt"));
        }
        if ist_id(referenz) {
            return Ok(Anzeige {
                id: referenz.to_string(),
                name: referenz.to_string(),
            });
        }
        let pfad = format!("/teams/{}/channels", team.id);
        let kanaele = self
            .graph
            .get_seiten::<Kanal>(&pfad, &[], HOECHSTENS_LISTE)
            .await?;
        let kandidaten = kanaele
            .into_iter()
            .map(|kanal| Anzeige::neu(kanal.id, kanal.display_name.as_deref()))
            .collect();
        waehlen(kandidaten, referenz, "Kanal")
    }

    async fn chats_laden(&self, anzahl: u32) -> Result<Vec<Chat>, ToolFehler> {
        Ok(self
            .graph
            .get_seiten::<Chat>(
                "/me/chats",
                &[
                    ("$expand", "members,lastMessagePreview".to_string()),
                    ("$top", anzahl.to_string()),
                ],
                anzahl as usize,
            )
            .await?)
    }

    /// Chat über Id, Thema oder Mitglied (Name, Mail, UPN) auflösen.
    async fn chat_finden(&self, referenz: &str) -> Result<Chat, ToolFehler> {
        let referenz = referenz.trim();
        if referenz.is_empty() {
            return Err(ungueltig_tool("chat fehlt"));
        }
        if ist_id(referenz) {
            return Ok(Chat {
                id: referenz.to_string(),
                ..Chat::default()
            });
        }

        let chats = self.chats_laden(50).await?;
        let klein = referenz.to_lowercase();
        let passt = |chat: &Chat| {
            chat.id.eq_ignore_ascii_case(referenz)
                || chat
                    .topic
                    .as_deref()
                    .is_some_and(|thema| thema.to_lowercase() == klein)
                || chat
                    .members
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .any(|mitglied| {
                        [
                            mitglied.display_name.as_deref(),
                            mitglied.email.as_deref(),
                            mitglied.user_id.as_deref(),
                        ]
                        .into_iter()
                        .flatten()
                        .any(|wert| wert.to_lowercase() == klein)
                    })
        };

        let genau: Vec<Chat> = chats.iter().filter(|chat| passt(chat)).cloned().collect();
        match genau.len() {
            1 => return Ok(genau.into_iter().next().expect("ein Chat")),
            0 => {}
            _ => {
                let kandidaten: Vec<Anzeige> = genau
                    .iter()
                    .map(|chat| Anzeige::neu(chat.id.clone(), Some(&chat_titel(chat, None))))
                    .collect();
                return Err(mehrdeutig(
                    "Chat",
                    referenz,
                    kandidaten.iter().collect::<Vec<_>>().as_slice(),
                ));
            }
        }

        // Kein genauer Treffer: Thema oder Mitglied enthält den Text.
        let teil: Vec<&Chat> = chats
            .iter()
            .filter(|chat| {
                chat_titel(chat, None).to_lowercase().contains(&klein)
                    || chat
                        .members
                        .as_deref()
                        .unwrap_or_default()
                        .iter()
                        .filter_map(|mitglied| mitglied.display_name.as_deref())
                        .any(|name| name.to_lowercase().contains(&klein))
            })
            .collect();
        match teil.len() {
            1 => Ok(teil[0].clone()),
            0 => Err(ungueltig_tool(format!(
                "Chat {referenz:?} nicht gefunden. Bitte `teams_chats` abrufen und die Id nennen."
            ))),
            _ => {
                let kandidaten: Vec<Anzeige> = teil
                    .iter()
                    .map(|chat| Anzeige::neu(chat.id.clone(), Some(&chat_titel(chat, None))))
                    .collect();
                Err(mehrdeutig(
                    "Chat",
                    referenz,
                    kandidaten.iter().collect::<Vec<_>>().as_slice(),
                ))
            }
        }
    }

    async fn personen_im_tenant_suchen(
        &self,
        suche: &str,
        anzahl: u32,
    ) -> Result<Vec<Person>, ToolFehler> {
        let abfrage = format!("\"displayName:{suche}\" OR \"mail:{suche}\"");
        let liste: Sammlung<Profil> = self
            .graph
            .get_mit_kopf(
                "/users",
                &[
                    ("$search", abfrage),
                    (
                        "$select",
                        "id,displayName,mail,userPrincipalName".to_string(),
                    ),
                    ("$top", anzahl.to_string()),
                ],
                &[("ConsistencyLevel", "eventual")],
            )
            .await?;
        Ok(liste
            .value
            .into_iter()
            .filter_map(|profil| {
                let id = profil.id?;
                Some(Person {
                    name: profil.display_name.unwrap_or_else(|| id.clone()),
                    mail: profil.mail,
                    upn: profil.user_principal_name,
                    id,
                })
            })
            .collect())
    }

    /// Eine Person eindeutig finden; mehrere Treffer ergeben keine Aktion.
    async fn person_finden(&self, referenz: &str) -> Result<Person, ToolFehler> {
        let referenz = referenz.trim();
        if referenz.is_empty() {
            return Err(ungueltig_tool("Teilnehmer fehlt"));
        }
        let personen = self.personen_im_tenant_suchen(referenz, 10).await?;
        let klein = referenz.to_lowercase();
        let identisch: Vec<&Person> = personen
            .iter()
            .filter(|person| {
                person.id.eq_ignore_ascii_case(referenz)
                    || person
                        .mail
                        .as_deref()
                        .is_some_and(|mail| mail.eq_ignore_ascii_case(referenz))
                    || person
                        .upn
                        .as_deref()
                        .is_some_and(|upn| upn.eq_ignore_ascii_case(referenz))
                    || person.name.to_lowercase() == klein
            })
            .collect();
        match identisch.len() {
            1 => return Ok(identisch[0].clone()),
            0 => {}
            _ => {
                let liste = identisch
                    .iter()
                    .take(8)
                    .map(|person| person.text())
                    .collect::<Vec<String>>()
                    .join(", ");
                return Err(ungueltig_tool(format!(
                    "Person {referenz:?} ist mehrdeutig – {} Treffer: {liste}. Bitte E-Mail oder UPN nennen; es wurde nichts geändert.",
                    identisch.len()
                )));
            }
        }
        // Kein genauer Treffer: der Name enthält den Suchtext.
        let teil: Vec<&Person> = personen
            .iter()
            .filter(|person| person.name.to_lowercase().contains(&klein))
            .collect();
        if teil.len() == 1 {
            return Ok(teil[0].clone());
        }
        let kandidaten: Vec<&Person> = if teil.is_empty() {
            personen.iter().collect()
        } else {
            teil
        };
        match kandidaten.len() {
            0 => Err(ungueltig_tool(format!(
                "Person {referenz:?} nicht gefunden. Bitte `teams_person_finden` benutzen."
            ))),
            _ => {
                let liste = kandidaten
                    .iter()
                    .take(8)
                    .map(|person| person.text())
                    .collect::<Vec<String>>()
                    .join(", ");
                Err(ungueltig_tool(format!(
                    "Person {referenz:?} ist mehrdeutig – {} Treffer: {liste}. Bitte E-Mail oder UPN nennen; es wurde nichts geändert.",
                    kandidaten.len()
                )))
            }
        }
    }

    /// Die eigene Id und der eigene Name (`GET /me`).
    async fn ich(&self) -> Result<(Option<String>, Option<String>), ToolFehler> {
        let profil: Profil = self
            .graph
            .get("/me", &[("$select", "id,displayName".to_string())])
            .await?;
        Ok((profil.id, profil.display_name))
    }

    /// Der Mandant des angemeldeten Users – aus dem `tid`-Anspruch des Tokens.
    ///
    /// Kein zusätzlicher Aufruf an Graph; ein Token ohne `tid` (von Hand
    /// gesetzt) ergibt einen Hinweis statt eines 400 von Graph (Aufgabe 07).
    async fn mandant(&self) -> Result<String, ToolFehler> {
        match self.graph.anmeldung().access_token().await {
            Ok(Token::Bereit(token)) => mandant_aus_token(&token).ok_or_else(|| {
                ungueltig_tool(
                    "Der Mandant fehlt im Access-Token (`tid`). Bitte im quiver-Fenster unter Tools → Microsoft 365 erneut auf „Anmelden“ klicken.",
                )
            }),
            Ok(Token::WartetAufBrowser) => Err(GraphFehler::AnmeldungLaeuft.into()),
            Err(fehler) => Err(GraphFehler::Anmeldung(fehler).into()),
        }
    }

    async fn antwortzahl(&self, pfad: &str) -> Result<usize, ToolFehler> {
        let antworten = self
            .graph
            .get_seiten::<TeamsNachricht>(pfad, &[("$top", "50".to_string())], HOECHSTENS_ANTWORTEN)
            .await?;
        Ok(antworten.len())
    }

    async fn suche_ausfuehren(&self, suche: &str, anzahl: u32) -> Result<Vec<Treffer>, ToolFehler> {
        let anfrage = json!({
            "requests": [{
                "entityTypes": ["chatMessage"],
                "query": {"queryString": suche},
                "from": 0,
                "size": anzahl.min(25),
            }]
        });
        let antwort: SucheAntwort = self.graph.post("/search/query", &anfrage).await?;
        Ok(antwort
            .value
            .into_iter()
            .flat_map(|block| block.hits_containers)
            .flat_map(|behaelter| behaelter.hits)
            .collect())
    }
}

// ---------------------------------------------------------------------------
// Umsetzung der Tools
// ---------------------------------------------------------------------------

impl<A: Anmeldung + 'static> Quiver<A> {
    async fn teams_eingang_lauf(&self, anfrage: TeamsEingangAnfrage) -> Result<String, ToolFehler> {
        let tage = anfrage.tage.unwrap_or(STANDARD_TAGE).clamp(1, 90);
        let (ich_id, ich_name) = self.ich().await?;
        let jetzt = Timestamp::now();

        // `members` mitladen: ein 1:1-Chat hat kein Thema, sein Name sind die
        // Mitglieder.
        let chats = self.chats_laden(50).await?;
        let mut ungelesen: Vec<Value> = Vec::new();
        for chat in &chats {
            let Some(vorschau) = chat.last_message_preview.as_ref() else {
                continue;
            };
            let absender = vorschau_absender(vorschau);
            // Was der User selbst zuletzt schrieb, wartet nicht auf ihn.
            if absender.is_some() && absender == ich_name {
                continue;
            }
            let gelesen = chat
                .viewpoint
                .as_ref()
                .and_then(|sicht| sicht.last_message_read_date_time.as_deref());
            let neu = vorschau.created_date_time.as_deref();
            // Ohne Lesemarke gilt der Chat als ungelesen (neuer Chat).
            if gelesen.is_some() && !ist_neuer(neu, gelesen) {
                continue;
            }
            let text = vorschau
                .body
                .as_ref()
                .and_then(|koerper| koerper.content.clone())
                .map(|inhalt| html_zu_text(&inhalt))
                .unwrap_or_default();
            let (vorschau_text, _) = text_kuerzen(&text, VORSCHAU_ZEICHEN);
            ungelesen.push(json!({
                "chat": chat.id,
                "thema": chat_titel(chat, ich_name.as_deref()),
                "von": absender,
                "zeit": neu.and_then(|zeit| self.zeitzone.zeitpunkt(zeit, None)),
                "vorschau": vorschau_text,
            }));
        }

        let mut erwaehnungen: Vec<Value> = Vec::new();
        if let Some(eigener_name) = ich_name.as_deref() {
            let treffer = self
                .suche_ausfuehren(&format!("\"{eigener_name}\""), 25)
                .await?;
            let eigene_id = ich_id.as_deref();
            erwaehnungen = treffer
                .iter()
                .filter(|treffer| {
                    let Some(resource) = treffer.resource.as_ref() else {
                        return false;
                    };
                    let zeit_ok = innerhalb(resource.created_date_time.as_deref(), tage, jetzt);
                    let mich_erwaehnt = resource
                        .mentions
                        .as_deref()
                        .unwrap_or_default()
                        .iter()
                        .any(|erwaehnung| {
                            let person = erwaehnung
                                .mentioned
                                .as_ref()
                                .and_then(|erwaehnt| erwaehnt.user.as_ref());
                            let id_passt = eigene_id.is_some()
                                && person.and_then(|person| person.id.as_deref()) == eigene_id;
                            let name_passt = person
                                .and_then(|person| person.display_name.as_deref())
                                .is_some_and(|name| name == eigener_name);
                            id_passt || name_passt
                        });
                    zeit_ok && mich_erwaehnt
                })
                .filter_map(|treffer| treffer_json(treffer, &self.zeitzone, 500))
                .collect();
        }

        als_json(&json!({
            "tage": tage,
            "chats": ungelesen,
            "erwaehnungen": erwaehnungen,
        }))
        .map_err(ToolFehler::from)
    }

    async fn teams_liste_lauf(&self) -> Result<String, ToolFehler> {
        // Kein `$top` auf `/me/joinedTeams` (Graph: „Query option 'Top' is not
        // allowed“, Aufgabe 07); begrenzt wird clientseitig über `get_seiten`.
        let teams = self
            .graph
            .get_seiten::<Team>("/me/joinedTeams", &[], HOECHSTENS_LISTE)
            .await?;
        let liste: Vec<Value> = teams
            .iter()
            .map(|team| {
                json!({
                    "id": team.id,
                    "name": team.display_name,
                    "beschreibung": team.description,
                })
            })
            .collect();
        als_json(&liste).map_err(ToolFehler::from)
    }

    async fn teams_kanaele_lauf(&self, anfrage: TeamsKanaeleAnfrage) -> Result<String, ToolFehler> {
        let team = self.team_finden(&anfrage.team).await?;
        let pfad = format!("/teams/{}/channels", team.id);
        // Auch hier kein `$top` (Aufgabe 07).
        let kanaele = self
            .graph
            .get_seiten::<Kanal>(&pfad, &[], HOECHSTENS_LISTE)
            .await?;
        let liste: Vec<Value> = kanaele
            .iter()
            .map(|kanal| {
                json!({
                    "id": kanal.id,
                    "name": kanal.display_name,
                    "beschreibung": kanal.description,
                    "art": kanal.membership_type,
                })
            })
            .collect();
        als_json(&liste).map_err(ToolFehler::from)
    }

    async fn teams_kanal_lesen_lauf(
        &self,
        anfrage: TeamsKanalLesenAnfrage,
    ) -> Result<String, ToolFehler> {
        let team = self.team_finden(&anfrage.team).await?;
        let kanal = self.kanal_finden(&team, &anfrage.kanal).await?;
        let top = anzahl_mit(anfrage.anzahl, 20, 50);
        let max_zeichen = max_zeichen_begrenzen(anfrage.max_zeichen);
        let pfad = format!("/teams/{}/channels/{}/messages", team.id, kanal.id);

        let nachrichten = self
            .graph
            .get_seiten::<TeamsNachricht>(&pfad, &[("$top", top.to_string())], top as usize)
            .await?;

        let mut liste: Vec<Value> = Vec::new();
        for nachricht in &nachrichten {
            if !sichtbar(nachricht) {
                continue;
            }
            let antworten = self
                .antwortzahl(&format!("{pfad}/{}/replies", nachricht.id))
                .await?;
            if let Some(wert) = beitrag_json(nachricht, antworten, &self.zeitzone, max_zeichen) {
                liste.push(wert);
            }
        }
        als_json(&liste).map_err(ToolFehler::from)
    }

    async fn teams_thread_lesen_lauf(
        &self,
        anfrage: TeamsThreadLesenAnfrage,
    ) -> Result<String, ToolFehler> {
        let team = self.team_finden(&anfrage.team).await?;
        let kanal = self.kanal_finden(&team, &anfrage.kanal).await?;
        let nachricht = anfrage.nachricht.trim();
        if nachricht.is_empty() {
            return Err(ungueltig_tool("nachricht fehlt"));
        }
        let max_zeichen = max_zeichen_begrenzen(anfrage.max_zeichen);
        let basis = format!("/teams/{}/channels/{}/messages", team.id, kanal.id);

        let beitrag: TeamsNachricht = self.graph.get(&format!("{basis}/{nachricht}"), &[]).await?;
        let antworten = self
            .graph
            .get_seiten::<TeamsNachricht>(
                &format!("{basis}/{nachricht}/replies"),
                &[("$top", "50".to_string())],
                HOECHSTENS_ANTWORTEN,
            )
            .await?;
        let antworten: Vec<Value> = antworten
            .iter()
            .filter_map(|antwort| nachricht_json(antwort, &self.zeitzone, max_zeichen))
            .collect();

        als_json(&json!({
            "beitrag": nachricht_json(&beitrag, &self.zeitzone, max_zeichen),
            "antworten": antworten,
        }))
        .map_err(ToolFehler::from)
    }

    async fn teams_chats_lauf(&self, anfrage: TeamsChatsAnfrage) -> Result<String, ToolFehler> {
        let anzahl = anzahl_mit(anfrage.anzahl, 25, 50);
        let chats = self.chats_laden(anzahl).await?;
        let liste: Vec<Value> = chats
            .iter()
            .map(|chat| {
                let vorschau: Option<&Vorschau> = chat.last_message_preview.as_ref();
                let text = vorschau
                    .and_then(|vorschau| vorschau.body.as_ref())
                    .and_then(|koerper| koerper.content.clone())
                    .map(|inhalt| html_zu_text(&inhalt))
                    .unwrap_or_default();
                let (vorschau_text, _) = text_kuerzen(&text, VORSCHAU_ZEICHEN);
                json!({
                    "id": chat.id,
                    "thema": chat.topic,
                    "art": chat.chat_type,
                    "mitglieder": chat
                        .members
                        .as_deref()
                        .unwrap_or_default()
                        .iter()
                        .filter_map(|mitglied| mitglied.display_name.clone())
                        .collect::<Vec<String>>(),
                    "vorschau": vorschau_text,
                    "von": vorschau.and_then(vorschau_absender),
                    "zeit": vorschau
                        .and_then(|vorschau| vorschau.created_date_time.as_deref())
                        .and_then(|zeit| self.zeitzone.zeitpunkt(zeit, None)),
                })
            })
            .collect();
        als_json(&liste).map_err(ToolFehler::from)
    }

    async fn teams_chat_lesen_lauf(
        &self,
        anfrage: TeamsChatLesenAnfrage,
    ) -> Result<String, ToolFehler> {
        let chat = self.chat_finden(&anfrage.chat).await?;
        let top = anzahl_mit(anfrage.anzahl, 20, 50);
        let max_zeichen = max_zeichen_begrenzen(anfrage.max_zeichen);
        let nachrichten = self
            .graph
            .get_seiten::<TeamsNachricht>(
                &format!("/chats/{}/messages", chat.id),
                &[("$top", top.to_string())],
                top as usize,
            )
            .await?;
        let liste: Vec<Value> = nachrichten
            .iter()
            .filter_map(|nachricht| nachricht_json(nachricht, &self.zeitzone, max_zeichen))
            .collect();
        als_json(&liste).map_err(ToolFehler::from)
    }

    async fn teams_suchen_lauf(&self, anfrage: TeamsSuchenAnfrage) -> Result<String, ToolFehler> {
        let suche = anfrage.suche.trim();
        if suche.is_empty() {
            return Err(ungueltig_tool("suche fehlt"));
        }
        let anzahl = anzahl_mit(anfrage.anzahl, 25, 50);
        let max_zeichen = max_zeichen_begrenzen(anfrage.max_zeichen);
        let treffer = self.suche_ausfuehren(suche, anzahl).await?;
        let liste: Vec<Value> = treffer
            .iter()
            .filter_map(|treffer| treffer_json(treffer, &self.zeitzone, max_zeichen))
            .take(anzahl as usize)
            .collect();
        als_json(&liste).map_err(ToolFehler::from)
    }

    async fn teams_mitglieder_lauf(
        &self,
        anfrage: TeamsMitgliederAnfrage,
    ) -> Result<String, ToolFehler> {
        let team = self.team_finden(&anfrage.team).await?;
        let (pfad, wo) = match anfrage
            .kanal
            .as_deref()
            .map(str::trim)
            .filter(|k| !k.is_empty())
        {
            Some(kanal) => {
                let kanal = self.kanal_finden(&team, kanal).await?;
                (
                    format!("/teams/{}/channels/{}/members", team.id, kanal.id),
                    format!("{} / {}", team.name, kanal.name),
                )
            }
            None => (format!("/teams/{}/members", team.id), team.name.clone()),
        };
        let mitglieder = self
            .graph
            .get_seiten::<Mitglied>(&pfad, &[("$top", "200".to_string())], HOECHSTENS_LISTE)
            .await
            .map_err(mitglieder_fehler)?;
        let liste: Vec<Value> = mitglieder.iter().map(mitglied_json).collect();
        als_json(&json!({"wo": wo, "mitglieder": liste})).map_err(ToolFehler::from)
    }

    async fn teams_person_finden_lauf(
        &self,
        anfrage: TeamsPersonFindenAnfrage,
    ) -> Result<String, ToolFehler> {
        let suche = anfrage.suche.trim();
        if suche.is_empty() {
            return Err(ungueltig_tool("suche fehlt"));
        }
        let anzahl = anzahl_mit(anfrage.anzahl, 10, 50);
        let personen = self.personen_im_tenant_suchen(suche, anzahl).await?;
        let liste: Vec<Value> = personen
            .iter()
            .map(|person| {
                json!({
                    "id": person.id,
                    "name": person.name,
                    "mail": person.mail,
                    "benutzer": person.upn,
                })
            })
            .collect();
        als_json(&liste).map_err(ToolFehler::from)
    }

    async fn teams_kanal_senden_lauf(
        &self,
        anfrage: TeamsKanalSendenAnfrage,
    ) -> Result<String, ToolFehler> {
        let team = self.team_finden(&anfrage.team).await?;
        let kanal = self.kanal_finden(&team, &anfrage.kanal).await?;
        let body = nachricht_body(&anfrage.text, anfrage.als_html.unwrap_or(false))?;
        let neu: TeamsNachricht = self
            .graph
            .post(
                &format!("/teams/{}/channels/{}/messages", team.id, kanal.id),
                &body,
            )
            .await?;
        als_json(&gesendet_json(&neu)).map_err(ToolFehler::from)
    }

    async fn teams_kanal_antworten_lauf(
        &self,
        anfrage: TeamsKanalAntwortenAnfrage,
    ) -> Result<String, ToolFehler> {
        let team = self.team_finden(&anfrage.team).await?;
        let kanal = self.kanal_finden(&team, &anfrage.kanal).await?;
        let nachricht = anfrage.nachricht.trim();
        if nachricht.is_empty() {
            return Err(ungueltig_tool("nachricht fehlt"));
        }
        let body = nachricht_body(&anfrage.text, anfrage.als_html.unwrap_or(false))?;
        let neu: TeamsNachricht = self
            .graph
            .post(
                &format!(
                    "/teams/{}/channels/{}/messages/{nachricht}/replies",
                    team.id, kanal.id
                ),
                &body,
            )
            .await?;
        als_json(&gesendet_json(&neu)).map_err(ToolFehler::from)
    }

    async fn teams_chat_senden_lauf(
        &self,
        anfrage: TeamsChatSendenAnfrage,
    ) -> Result<String, ToolFehler> {
        let chat = self.chat_finden(&anfrage.chat).await?;
        let body = nachricht_body(&anfrage.text, anfrage.als_html.unwrap_or(false))?;
        let neu: TeamsNachricht = self
            .graph
            .post(&format!("/chats/{}/messages", chat.id), &body)
            .await?;
        als_json(&gesendet_json(&neu)).map_err(ToolFehler::from)
    }

    async fn teams_chat_anlegen_lauf(
        &self,
        anfrage: TeamsChatAnlegenAnfrage,
    ) -> Result<String, ToolFehler> {
        if anfrage.teilnehmer.is_empty() {
            return Err(ungueltig_tool(
                "teilnehmer braucht mindestens eine Person (E-Mail, UPN oder Name)",
            ));
        }
        let mut personen = Vec::with_capacity(anfrage.teilnehmer.len());
        for referenz in &anfrage.teilnehmer {
            personen.push(self.person_finden(referenz).await?);
        }

        // Der User selbst gehört immer dazu (Graph erwartet ihn ausdrücklich) –
        // aber als **eigene Id**, nicht als `/me`: `user@odata.bind` verlangt eine
        // Bindung an die Ressource (Aufgabe 07: „'user@odata.bind' field is
        // missing“ bei `…/me`).
        let (ich_id, _) = self.ich().await?;
        let ich_id = ich_id.ok_or_else(|| {
            ungueltig_tool("Die eigene Benutzer-Id fehlt in der Antwort von Graph (/me).")
        })?;

        // Der eigene User wird nie doppelt gebunden, und ein Mitglied steht nur
        // einmal im Rumpf (Aufgabe 08: „Duplicate chat members“). Bleibt niemand
        // übrig, gibt es keinen Chat mit sich selbst.
        let mut gesehen: Vec<String> = vec![ich_id.to_lowercase()];
        personen.retain(|person| {
            let id = person.id.to_lowercase();
            if gesehen.contains(&id) {
                false
            } else {
                gesehen.push(id);
                true
            }
        });
        if personen.is_empty() {
            return Err(ungueltig_tool(
                "Du kannst keinen Chat nur mit dir selbst anlegen – nenne mindestens eine andere Person.",
            ));
        }

        let art = if personen.len() == 1 {
            "oneOnOne"
        } else {
            "group"
        };
        let mut mitglieder = vec![json!({
            "@odata.type": "#microsoft.graph.aadUserConversationMember",
            "roles": ["owner"],
            "user@odata.bind": format!("https://graph.microsoft.com/v1.0/users('{ich_id}')"),
        })];
        for person in &personen {
            mitglieder.push(json!({
                "@odata.type": "#microsoft.graph.aadUserConversationMember",
                "roles": ["owner"],
                "user@odata.bind": format!("https://graph.microsoft.com/v1.0/users/{}", person.id),
            }));
        }

        let mut body = json!({"chatType": art, "members": mitglieder});
        let thema = anfrage
            .thema
            .as_deref()
            .map(str::trim)
            .filter(|thema| !thema.is_empty());
        if let (Some(thema), "group") = (thema, art) {
            body["topic"] = json!(thema);
        }

        let chat: Chat = self.graph.post("/chats", &body).await?;
        let mut antwort = json!({
            "id": chat.id,
            "art": art,
            "thema": chat.topic,
            "teilnehmer": personen.iter().map(|person| person.name.clone()).collect::<Vec<String>>(),
        });

        if let Some(nachricht) = anfrage
            .nachricht
            .as_deref()
            .map(str::trim)
            .filter(|nachricht| !nachricht.is_empty())
        {
            let body = nachricht_body(nachricht, false)?;
            let neu: TeamsNachricht = self
                .graph
                .post(&format!("/chats/{}/messages", chat.id), &body)
                .await?;
            antwort["nachricht"] = json!(neu.id);
        }

        als_json(&antwort).map_err(ToolFehler::from)
    }

    async fn teams_reagieren_lauf(
        &self,
        anfrage: TeamsReagierenAnfrage,
    ) -> Result<String, ToolFehler> {
        let nachricht = anfrage.nachricht.trim();
        if nachricht.is_empty() {
            return Err(ungueltig_tool("nachricht fehlt"));
        }
        let roh = anfrage.reaktion.trim();
        let reaktion = reaktion_normalisieren(roh).ok_or_else(|| {
            let erlaubt = "erlaubt sind die Wörter like, love, laugh, surprised, sad, angry oder das Emoji (👍 ❤️ 😆 😮 😢 😡)";
            if roh.is_empty() {
                ungueltig_tool(format!("reaktion fehlt – {erlaubt}."))
            } else {
                ungueltig_tool(format!("reaktion {roh:?} ist unbekannt – {erlaubt}."))
            }
        })?;

        let basis = self
            .nachricht_basis(
                anfrage.chat.as_deref(),
                anfrage.team.as_deref(),
                anfrage.kanal.as_deref(),
                nachricht,
            )
            .await?;

        let entfernen = anfrage.entfernen.unwrap_or(false);
        let aktion = if entfernen {
            "unsetReaction"
        } else {
            "setReaction"
        };
        // 204 mit leerem Rumpf: der Aufruf ist erfolgreich (Aufgabe 09).
        self.graph
            .post_ohne_antwort(
                &format!("{basis}/{aktion}"),
                &json!({"reactionType": reaktion}),
            )
            .await?;

        als_json(&json!({
            "reagiert": !entfernen,
            "entfernt": entfernen,
            "reaktion": reaktion,
            "nachricht": nachricht,
        }))
        .map_err(ToolFehler::from)
    }

    async fn teams_nachricht_loeschen_lauf(
        &self,
        anfrage: TeamsNachrichtLoeschenAnfrage,
    ) -> Result<String, ToolFehler> {
        let nachricht = anfrage.nachricht.trim();
        if nachricht.is_empty() {
            return Err(ungueltig_tool("nachricht fehlt"));
        }
        let basis = self
            .nachricht_basis(
                anfrage.chat.as_deref(),
                anfrage.team.as_deref(),
                anfrage.kanal.as_deref(),
                nachricht,
            )
            .await?;
        // `softDelete`: Graph markiert die Nachricht als gelöscht, sie bleibt
        // wiederherstellbar. Fremde Nachrichten lehnt Graph ab (403).
        self.graph
            .post_ohne_antwort(&format!("{basis}/softDelete"), &Value::Null)
            .await?;
        als_json(&json!({
            "geloescht": true,
            "nachricht": nachricht,
            "wiederherstellbar": true,
        }))
        .map_err(ToolFehler::from)
    }

    async fn teams_als_gelesen_lauf(
        &self,
        anfrage: TeamsAlsGelesenAnfrage,
    ) -> Result<String, ToolFehler> {
        let gelesen = anfrage.gelesen.unwrap_or(true);
        let (ich_id, _) = self.ich().await?;
        let ich_id = ich_id.ok_or_else(|| {
            ungueltig_tool("Die eigene Benutzer-Id fehlt in der Antwort von Graph (/me).")
        })?;
        // `markChatReadForUser` verlangt den Mandanten des Users im Rumpf
        // (Aufgabe 07: „TenantId for the user must be specified“). Er steht im
        // `tid`-Anspruch des Access-Tokens.
        let mandant = self.mandant().await?;
        let chat = self.chat_finden(&anfrage.chat).await?;
        let aktion = if gelesen {
            "markChatReadForUser"
        } else {
            "markChatUnreadForUser"
        };
        // 204 mit leerem Rumpf: die Lesemarke ist gesetzt (Aufgabe 09).
        self.graph
            .post_ohne_antwort(
                &format!("/chats/{}/{aktion}", chat.id),
                &json!({"user": {"id": ich_id, "tenantId": mandant}}),
            )
            .await?;

        als_json(&json!({"chat": chat.id, "gelesen": gelesen})).map_err(ToolFehler::from)
    }

    /// Der Graph-Pfad einer Nachricht: über den Chat **oder** über Team und Kanal.
    /// Genau eines von beidem, sonst eine Meldung ohne Anfrage.
    async fn nachricht_basis(
        &self,
        chat: Option<&str>,
        team: Option<&str>,
        kanal: Option<&str>,
        nachricht: &str,
    ) -> Result<String, ToolFehler> {
        match chat.map(str::trim).filter(|chat| !chat.is_empty()) {
            Some(chat) => {
                let chat = self.chat_finden(chat).await?;
                Ok(format!("/chats/{}/messages/{nachricht}", chat.id))
            }
            None => {
                let team = team
                    .map(str::trim)
                    .filter(|team| !team.is_empty())
                    .ok_or_else(|| {
                        ungueltig_tool(
                            "chat fehlt – oder team und kanal nennen (genau eines von beidem)",
                        )
                    })?;
                let kanal = kanal
                    .map(str::trim)
                    .filter(|kanal| !kanal.is_empty())
                    .ok_or_else(|| {
                        ungueltig_tool(
                            "kanal fehlt – zusammen mit team (oder stattdessen chat nennen)",
                        )
                    })?;
                let team = self.team_finden(team).await?;
                let kanal = self.kanal_finden(&team, kanal).await?;
                Ok(format!(
                    "/teams/{}/channels/{}/messages/{nachricht}",
                    team.id, kanal.id
                ))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Reine Hilfen
// ---------------------------------------------------------------------------

/// Ein 403 bei `teams_mitglieder` ist keine Panne, sondern ein fehlendes Recht:
/// statt des Rohfehlers kommt der Satz aus [`MITGLIEDER_HINWEIS`] (Aufgabe 07).
/// Ein 403 mit `Missing scope` bleibt der allgemeine Zustimmungs-Hinweis, und
/// alles andere bleibt, wie es war.
fn mitglieder_fehler(fehler: GraphFehler) -> ToolFehler {
    if fehler.zustimmung_fehlt() {
        return ToolFehler::from(fehler);
    }
    match &fehler {
        GraphFehler::Graph { status: 403, .. } => {
            ToolFehler::Hinweis(MITGLIEDER_HINWEIS.to_string())
        }
        _ => ToolFehler::from(fehler),
    }
}

/// Der Graph-Körper einer neuen Nachricht.
///
/// Teams rendert HTML; reiner Text wird deshalb escaped und bekommt `<br>` für
/// Zeilenumbrüche. `als_html` überspringt das (der Aufrufer weiß, was er tut).
fn nachricht_body(text: &str, als_html: bool) -> Result<Value, ToolFehler> {
    if text.trim().is_empty() {
        return Err(ungueltig_tool("text fehlt"));
    }
    let inhalt = if als_html {
        text.to_string()
    } else {
        text_zu_html(text)
    };
    Ok(json!({"body": {"contentType": "html", "content": inhalt}}))
}

/// Reiner Text → HTML: `&`, `<`, `>`, `"`, `'` escapen, Zeilenumbrüche zu `<br>`.
fn text_zu_html(text: &str) -> String {
    let mut raus = String::with_capacity(text.len() + 16);
    for zeichen in text.replace("\r\n", "\n").chars() {
        match zeichen {
            '&' => raus.push_str("&amp;"),
            '<' => raus.push_str("&lt;"),
            '>' => raus.push_str("&gt;"),
            '"' => raus.push_str("&quot;"),
            '\'' => raus.push_str("&#39;"),
            '\n' => raus.push_str("<br>"),
            anderes => raus.push(anderes),
        }
    }
    raus
}

/// Emoji oder Wort → das **Unicode-Emoji**, das Graph als `reactionType` will
/// (Aufgabe 08: „Unicode 'heart' in the payload is not supported“).
///
/// `None` für alles Unbekannte und für leeren Text – dann geht keine Anfrage
/// an Graph.
fn reaktion_normalisieren(roh: &str) -> Option<String> {
    let roh = roh.trim();
    let wort = match roh {
        "like" => Some("👍"),
        "heart" | "love" | "herz" => Some("❤️"),
        "laugh" | "lachen" => Some("😆"),
        "surprised" | "überrascht" => Some("😮"),
        "sad" | "traurig" => Some("😢"),
        "angry" | "wütend" => Some("😡"),
        _ => None,
    };
    if let Some(emoji) = wort {
        return Some(emoji.to_string());
    }
    // Ein Emoji bleibt, wie es ist.
    if ist_emoji(roh) {
        return Some(roh.to_string());
    }
    None
}

/// Ist der Text ein Emoji? Nur die Bildzeichen- und Emoji-Blöcke zählen.
fn ist_emoji(text: &str) -> bool {
    !text.is_empty()
        && text.chars().all(|zeichen| {
            matches!(
                zeichen as u32,
                0x2190..=0x21FF
                    | 0x2600..=0x27BF
                    | 0x2B00..=0x2BFF
                    | 0x1F000..=0x1FAFF
                    | 0xFE00..=0xFE0F
                    | 0x200D
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reiner_text_wird_escaped_und_bekommt_zeilenumbrueche() {
        assert_eq!(
            text_zu_html("Milch & Brot <b>fett</b>\r\nZeile 2"),
            "Milch &amp; Brot &lt;b&gt;fett&lt;/b&gt;<br>Zeile 2"
        );
        assert_eq!(
            text_zu_html("Er sagte \"nein\" – 'ok'"),
            "Er sagte &quot;nein&quot; – &#39;ok&#39;"
        );
    }

    #[test]
    fn nachricht_body_ist_html_und_lehnt_leeren_text_ab() {
        let body = nachricht_body("Hallo\nWelt", false).expect("Text da");
        assert_eq!(body["body"]["contentType"], "html");
        assert_eq!(body["body"]["content"], "Hallo<br>Welt");

        let roh = nachricht_body("<b>Hallo</b>", true).expect("HTML da");
        assert_eq!(
            roh["body"]["content"], "<b>Hallo</b>",
            "als_html bleibt roh"
        );

        assert!(nachricht_body("   ", false).is_err());
    }

    #[test]
    fn reaktionen_werden_zu_emoji() {
        assert_eq!(reaktion_normalisieren("like"), Some("👍".to_string()));
        assert_eq!(reaktion_normalisieren(" heart "), Some("❤️".to_string()));
        assert_eq!(reaktion_normalisieren("love"), Some("❤️".to_string()));
        assert_eq!(reaktion_normalisieren("laugh"), Some("😆".to_string()));
        assert_eq!(reaktion_normalisieren("angry"), Some("😡".to_string()));
        // Ein Emoji bleibt, wie es ist.
        assert_eq!(reaktion_normalisieren("👍"), Some("👍".to_string()));
        assert_eq!(reaktion_normalisieren("😡"), Some("😡".to_string()));
        // Unbekannt und leer: keine Anfrage, klare Meldung.
        assert_eq!(reaktion_normalisieren("unbekannt"), None);
        assert_eq!(reaktion_normalisieren("   "), None);
    }

    #[test]
    fn ids_werden_erkannt() {
        assert!(ist_id("19:abc@thread.v2"));
        assert!(ist_id("48:notes"));
        assert!(ist_id("12345678-1234-1234-1234-123456789abc"));
        assert!(!ist_id("Entwicklung"));
        assert!(!ist_id("Büro"));
    }

    #[test]
    fn namen_loesen_genau_dann_teilweise_auf() {
        let kandidaten = vec![
            Anzeige::neu("t1", Some("Entwicklung")),
            Anzeige::neu("t2", Some("Vertrieb")),
            Anzeige::neu("t3", Some("Entwicklung Intern")),
        ];
        assert_eq!(
            waehlen(kandidaten.clone(), "t2", "Team").expect("Id").name,
            "Vertrieb"
        );
        assert_eq!(
            waehlen(kandidaten.clone(), "vertrieb", "Team")
                .expect("genau")
                .id,
            "t2"
        );
        assert_eq!(
            waehlen(kandidaten.clone(), "intern", "Team")
                .expect("Teil")
                .id,
            "t3"
        );
        assert_eq!(
            waehlen(kandidaten.clone(), "Entwicklung", "Team")
                .expect("genauer Name gewinnt")
                .id,
            "t1"
        );

        let fehler = waehlen(kandidaten.clone(), "Entwick", "Team").expect_err("mehrdeutig");
        let text = match fehler {
            ToolFehler::Fehler(fehler) => format!("{fehler}"),
            ToolFehler::Hinweis(text) => text,
        };
        assert!(text.contains("mehrdeutig"), "{text}");
        assert!(text.contains("Entwicklung (t1)"), "{text}");
        assert!(text.contains("Entwicklung Intern (t3)"), "{text}");
        assert!(text.contains("nichts geändert"), "{text}");

        assert!(waehlen(kandidaten, "Quatsch", "Team").is_err());
    }
}
