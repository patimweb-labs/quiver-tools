// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die MCP-Tools aus SPEC.md §4 und docs/aufgaben/05-teams.md.
//!
//! Regeln: Antworten sind knappes JSON als Text; `AnmeldungLaeuft` ist **kein**
//! Fehler, sondern der Hinweistext aus SPEC.md §2; fehlt die Anmeldung ganz, sagt
//! das Tool dem Agenten, er solle den User auf den quiver-Knopf schicken
//! (Aufgabe 03); fehlt die Zustimmung für einen neuen Scope (Teams), sagt es
//! dasselbe (Aufgabe 05); Fehler von Graph werden zu MCP-Fehlern mit Graph-Code
//! und Text – ohne Token.

use m365_anmeldung::{AnmeldeFehler, Anmeldung};
use m365_graph::{
    Adresse, Datei, Empfaenger, GraphClient, GraphFehler, Nachricht, Profil, Sammlung, Termin,
    Zeitpunkt,
};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::ErrorData;
use rmcp::{tool, tool_handler, tool_router, ServerHandler};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::kodierung::kodieren;
use crate::zeit::Zeitzone;

/// Der eine Satz aus SPEC.md §2, wenn die Anmeldung im Browser läuft.
pub const ANMELDUNG_HINWEIS: &str =
    "Anmeldung im Browser geöffnet – bitte anmelden und den Aufruf danach wiederholen.";

/// Der Satz, wenn kein gültiges Refresh-Token da ist und kein Browser aufgeht
/// (Aufgabe 03). **Kein** MCP-Fehler, damit der Agent es dem User sagen kann.
pub const NICHT_ANGEMELDET_HINWEIS: &str =
    "Nicht angemeldet – bitte im quiver-Fenster unter Tools → Microsoft 365 auf „Anmelden“ klicken.";

/// Der Satz, wenn die Zustimmung für die Teams-Rechte fehlt (Aufgabe 05).
/// **Kein** MCP-Fehler, kein Browser, das Refresh-Token bleibt.
pub const ZUSTIMMUNG_HINWEIS: &str =
    "Für Teams fehlt die Zustimmung – im quiver-Fenster unter Tools → Microsoft 365 erneut auf „Anmelden“ klicken.";

/// Derselbe Fall für die Rechte aus Aufgabe 06 (SharePoint, Planner, To Do,
/// Kontakte, Personen, OneNote). Der Teams-Satz bleibt unverändert, damit
/// bestehende Antworten gleich bleiben.
pub const ZUSTIMMUNG_ERWEITERT_HINWEIS: &str =
    "Für SharePoint, Planner, To Do, Kontakte oder OneNote fehlt die Zustimmung – im quiver-Fenster unter Tools → Microsoft 365 erneut auf „Anmelden“ klicken.";

/// Kopfzeile für `mail_lesen`: Text statt HTML (SPEC.md §4).
const PREFER_TEXT: &str = "outlook.body-content-type=\"text\"";
/// Wie viele Mails/Treffer ein Tool höchstens liefert.
const HOECHSTENS_ANZAHL: u32 = 50;
/// Standard-Anzahl, wenn `anzahl` fehlt.
const STANDARD_ANZAHL: u32 = 10;
/// So viel Mailtext kommt standardmäßig höchstens zurück (Aufgabe 04).
const STANDARD_MAX_ZEICHEN: u32 = 4000;
/// Mehr Text gibt `mail_lesen` nie heraus – ein Agent soll nicht im Verlauf versinken.
const HOECHSTENS_MAX_ZEICHEN: u32 = 50_000;
/// Mehr als zwei Leerzeilen hintereinander werden zu zwei (Aufgabe 04).
const HOECHSTENS_LEERZEILEN: usize = 2;

const MAIL_KURZ: &str = "id,subject,from,receivedDateTime,bodyPreview";
/// `mail_lesen` ohne `verlauf`: nur der neue Teil der Mail (Aufgabe 04).
const MAIL_NEU: &str =
    "id,subject,from,toRecipients,ccRecipients,receivedDateTime,uniqueBody,webLink";
/// `mail_lesen` mit `verlauf`: der ganze Text samt zitiertem Verlauf.
const MAIL_VOLL: &str = "id,subject,from,toRecipients,ccRecipients,receivedDateTime,body,webLink";
const TERMIN_FELDER: &str =
    "id,subject,start,end,location,organizer,attendees,isAllDay,bodyPreview,webLink";
const DATEI_FELDER: &str = "id,name,webUrl,lastModifiedDateTime,size,parentReference";

// ---------------------------------------------------------------------------
// Parameter der Tools (Namen und Beschreibungen deutsch, SPEC.md §4)
// ---------------------------------------------------------------------------

/// Parameter für `mails_suchen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct MailsSuchenAnfrage {
    /// Freitext, nach dem Graph sucht (Betreff, Absender, Text). Leer: die
    /// neuesten Mails im Posteingang.
    #[serde(default)]
    pub suche: Option<String>,
    /// Wie viele Mails (Standard 10, höchstens 50).
    #[serde(default)]
    pub anzahl: Option<u32>,
}

/// Parameter für `mail_lesen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct MailLesenAnfrage {
    /// Die id der Mail (aus `mails_suchen`).
    pub id: String,
    /// Den **ganzen Verlauf** lesen, also auch die zitierten älteren Mails?
    ///
    /// Standard `false`: nur der neue Teil der Mail (`uniqueBody`) – das ist
    /// fast immer, was gebraucht wird. `true` nur, wenn der User ausdrücklich
    /// die Vorgeschichte einer langen Mailwechsel-Kette sehen will; die Antwort
    /// wird dadurch um ein Vielfaches länger.
    #[serde(default)]
    pub verlauf: Option<bool>,
    /// Höchstens so viele Zeichen Mailtext (Standard 4000, höchstens 50 000).
    ///
    /// Längerer Text wird an einer Wortgrenze gekürzt; die Antwort hat dann
    /// zusätzlich `gekuerzt: true` und `laenge_gesamt` (Zeichen der vollen Mail).
    #[serde(default)]
    pub max_zeichen: Option<u32>,
}

/// Parameter für `mail_senden`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct MailSendenAnfrage {
    /// Empfänger, je eine Mail-Adresse.
    pub an: Vec<String>,
    /// Empfänger in Kopie (optional).
    #[serde(default)]
    pub cc: Option<Vec<String>>,
    /// Betreff.
    pub betreff: String,
    /// Der Text der Mail (reiner Text).
    pub text: String,
}

/// Parameter für `termine`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TermineAnfrage {
    /// Beginn des Zeitraums, ISO 8601 in der Zone `M365_ZEITZONE`, z. B.
    /// `2026-11-27T10:00`; ein Versatz ist auch erlaubt (`2026-11-27T10:00:00+01:00`, `…Z`).
    pub von: String,
    /// Ende des Zeitraums, ISO 8601 in der Zone `M365_ZEITZONE`, z. B. `2026-11-28T18:00`
    /// (auch mit Versatz).
    pub bis: String,
}

/// Parameter für `termin_anlegen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TerminAnlegenAnfrage {
    /// Betreff des Termins.
    pub betreff: String,
    /// Beginn, ISO 8601 in der Zone `M365_ZEITZONE`, z. B. `2026-11-27T10:00`; ein
    /// Versatz ist auch erlaubt (`2026-11-27T10:00:00+02:00`, `…Z`).
    pub start: String,
    /// Ende, ISO 8601 in der Zone `M365_ZEITZONE`, z. B. `2026-11-27T11:00`
    /// (auch mit Versatz).
    pub ende: String,
    /// Ort (optional).
    #[serde(default)]
    pub ort: Option<String>,
    /// Teilnehmer, je eine Mail-Adresse (optional).
    #[serde(default)]
    pub teilnehmer: Option<Vec<String>>,
}

/// Parameter für `dateien_suchen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DateienSuchenAnfrage {
    /// Wort oder Satzteil, nach dem die Namen der Dateien durchsucht werden.
    pub suche: String,
    /// Wie viele Treffer (Standard 10, höchstens 50).
    #[serde(default)]
    pub anzahl: Option<u32>,
}

// ---------------------------------------------------------------------------
// Der Server
// ---------------------------------------------------------------------------

/// Der MCP-Server mit den Tools aus SPEC.md §4 und Aufgabe 05.
pub struct Quiver<A: Anmeldung> {
    pub(crate) graph: GraphClient<A>,
    /// Zeitzone für Termine und alle Zeiten in Antworten (`M365_ZEITZONE`,
    /// Standard `Europe/Berlin`).
    pub(crate) zeitzone: Zeitzone,
    tool_router: ToolRouter<Self>,
}

impl<A: Anmeldung + 'static> Quiver<A> {
    /// Baut den Server aus Graph-Client und Zeitzone.
    pub fn neu(graph: GraphClient<A>, zeitzone: impl Into<String>) -> Self {
        // Mail/Kalender/Dateien (SPEC.md §4), Teams (Aufgabe 05) und die
        // Erweiterung (Aufgabe 06) sind getrennte Router, damit die Dateien
        // nicht ins Unermessliche wachsen.
        let mut tool_router = Self::tool_router();
        tool_router.merge(Self::teams_router());
        tool_router.merge(Self::dateien_router());
        tool_router.merge(Self::planner_router());
        tool_router.merge(Self::kontakte_router());
        tool_router.merge(Self::onenote_router());
        Self {
            graph,
            zeitzone: Zeitzone::neu(zeitzone),
            tool_router,
        }
    }

    /// Die Zeitzone, mit der Termine gelesen und geschrieben werden.
    pub fn zeitzone(&self) -> &str {
        self.zeitzone.name()
    }
}

#[tool_router]
impl<A: Anmeldung + 'static> Quiver<A> {
    /// Wer ist angemeldet: Name, Mail-Adresse, Abteilung.
    #[tool(
        description = "Wer ist angemeldet: Name, Mail-Adresse, Abteilung.",
        annotations(read_only_hint = true)
    )]
    pub async fn profil(&self) -> Result<String, ErrorData> {
        let abfrage = self
            .graph
            .get::<Profil>(
                "/me",
                &[(
                    "$select",
                    "displayName,mail,userPrincipalName,department".to_string(),
                )],
            )
            .await;
        aus_ergebnis(abfrage, |profil| {
            als_json(&json!({
                "name": profil.display_name,
                "mail": profil.mail,
                "abteilung": profil.department,
            }))
        })
    }

    /// Mails suchen oder die neuesten im Posteingang lesen.
    #[tool(
        description = "Mails suchen (Betreff, Absender, Text) oder die neuesten im Posteingang lesen.",
        annotations(read_only_hint = true)
    )]
    pub async fn mails_suchen(
        &self,
        Parameters(anfrage): Parameters<MailsSuchenAnfrage>,
    ) -> Result<String, ErrorData> {
        let anzahl = anzahl_begrenzen(anfrage.anzahl);
        let suche = anfrage
            .suche
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty());

        let mut query: Vec<(&str, String)> = vec![
            ("$select", MAIL_KURZ.to_string()),
            ("$top", anzahl.to_string()),
        ];
        // Ohne Suchtext: die neuesten Mails im Posteingang (dafür der Ordnerpfad).
        let pfad = match &suche {
            Some(text) => {
                query.push(("$search", format!("\"{text}\"")));
                "/me/messages"
            }
            None => {
                query.push(("$orderby", "receivedDateTime desc".to_string()));
                "/me/mailFolders/inbox/messages"
            }
        };

        let abfrage = self.graph.get::<Sammlung<Nachricht>>(pfad, &query).await;
        aus_ergebnis(abfrage, |liste| {
            als_json(
                &liste
                    .value
                    .iter()
                    .map(|mail| mail_kurz(mail, &self.zeitzone))
                    .collect::<Vec<Value>>(),
            )
        })
    }

    /// Eine Mail lesen: standardmäßig nur der neue Teil, ohne zitierten Verlauf.
    #[tool(
        description = "Eine Mail lesen: Betreff, Absender, Empfänger, Datum, Text. Standardmäßig nur der neue Teil der Mail (ohne zitierten Verlauf) und höchstens 4000 Zeichen.",
        annotations(read_only_hint = true)
    )]
    pub async fn mail_lesen(
        &self,
        Parameters(anfrage): Parameters<MailLesenAnfrage>,
    ) -> Result<String, ErrorData> {
        let id = anfrage.id.trim();
        if id.is_empty() {
            return Err(ErrorData::invalid_params("id fehlt", None));
        }
        let verlauf = anfrage.verlauf.unwrap_or(false);
        let max_zeichen = max_zeichen_begrenzen(anfrage.max_zeichen);
        let felder = if verlauf { MAIL_VOLL } else { MAIL_NEU };

        let abfrage = self
            .graph
            .get_mit_kopf::<Nachricht>(
                &format!("/me/messages/{}", kodieren(id)),
                &[("$select", felder.to_string())],
                &[("Prefer", PREFER_TEXT)],
            )
            .await;

        aus_ergebnis(abfrage, |mail| {
            // Ohne `verlauf` steht `body` gar nicht in `$select`: der neue Teil
            // der Mail ist `uniqueBody`.
            let roh = if verlauf { mail.body } else { mail.unique_body };
            let (text, laenge_gesamt) =
                mail_text(roh.and_then(|koerper| koerper.content), max_zeichen);

            let mut antwort = json!({
                "id": mail.id,
                "betreff": mail.subject,
                "von": mail.from.as_ref().map(person),
                "an": adressen(mail.to_recipients.as_deref()),
                "cc": adressen(mail.cc_recipients.as_deref()),
                "datum": zeit_in_zone(mail.received_date_time.as_deref(), &self.zeitzone),
                "text": text,
                "link": mail.web_link,
            });
            if let Some(gesamt) = laenge_gesamt {
                antwort["gekuerzt"] = json!(true);
                antwort["laenge_gesamt"] = json!(gesamt);
            }
            als_json(&antwort)
        })
    }

    /// Eine Mail senden (verändernd).
    #[tool(
        description = "Eine Mail senden. Verändert das Postfach des Users.",
        annotations(destructive_hint = true)
    )]
    pub async fn mail_senden(
        &self,
        Parameters(anfrage): Parameters<MailSendenAnfrage>,
    ) -> Result<String, ErrorData> {
        let an = empfaenger_json(&anfrage.an);
        if an.is_empty() {
            return Err(ErrorData::invalid_params(
                "an braucht mindestens eine Adresse",
                None,
            ));
        }
        let cc = empfaenger_json(anfrage.cc.as_deref().unwrap_or_default());

        let mut nachricht = json!({
            "subject": anfrage.betreff,
            "body": {"contentType": "Text", "content": anfrage.text},
            "toRecipients": an,
        });
        if !cc.is_empty() {
            nachricht["ccRecipients"] = json!(cc);
        }

        let abfrage = self
            .graph
            .post::<Value>(
                "/me/sendMail",
                &json!({"message": nachricht, "saveToSentItems": true}),
            )
            .await;
        aus_ergebnis(abfrage, |_| als_json(&json!({"gesendet": true})))
    }

    /// Termine in einem Zeitraum lesen.
    #[tool(
        description = "Termine in einem Zeitraum lesen (ISO 8601, z. B. 2026-11-27T10:00).",
        annotations(read_only_hint = true)
    )]
    pub async fn termine(
        &self,
        Parameters(anfrage): Parameters<TermineAnfrage>,
    ) -> Result<String, ErrorData> {
        let von = self.zeitzone.eingabe(&anfrage.von).map_err(ungueltig)?;
        let bis = self.zeitzone.eingabe(&anfrage.bis).map_err(ungueltig)?;

        let prefer = format!("outlook.timezone=\"{}\"", self.zeitzone.name());
        let query = vec![
            ("startDateTime", von),
            ("endDateTime", bis),
            ("$select", TERMIN_FELDER.to_string()),
            ("$orderby", "start/dateTime".to_string()),
            ("$top", HOECHSTENS_ANZAHL.to_string()),
        ];

        let abfrage = self
            .graph
            .get_mit_kopf::<Sammlung<Termin>>("/me/calendarView", &query, &[("Prefer", &prefer)])
            .await;
        aus_ergebnis(abfrage, |liste| {
            als_json(
                &liste
                    .value
                    .iter()
                    .map(|termin| termin_kurz(termin, &self.zeitzone))
                    .collect::<Vec<Value>>(),
            )
        })
    }

    /// Einen Termin anlegen (verändernd).
    #[tool(
        description = "Einen Termin anlegen. Verändert den Kalender des Users.",
        annotations(destructive_hint = true)
    )]
    pub async fn termin_anlegen(
        &self,
        Parameters(anfrage): Parameters<TerminAnlegenAnfrage>,
    ) -> Result<String, ErrorData> {
        let start = self.zeitzone.eingabe(&anfrage.start).map_err(ungueltig)?;
        let ende = self.zeitzone.eingabe(&anfrage.ende).map_err(ungueltig)?;

        let mut termin = json!({
            "subject": anfrage.betreff,
            "start": {"dateTime": start, "timeZone": self.zeitzone.name()},
            "end": {"dateTime": ende, "timeZone": self.zeitzone.name()},
        });

        if let Some(ort) = anfrage.ort.map(|ort| ort.trim().to_string()) {
            if !ort.is_empty() {
                termin["location"] = json!({"displayName": ort});
            }
        }

        let teilnehmer: Vec<Value> = empfaenger_json(
            anfrage.teilnehmer.as_deref().unwrap_or_default(),
        )
        .into_iter()
        .map(|empfaenger| json!({"emailAddress": empfaenger["emailAddress"], "type": "required"}))
        .collect();
        if !teilnehmer.is_empty() {
            termin["attendees"] = json!(teilnehmer);
        }

        let abfrage = self.graph.post::<Termin>("/me/events", &termin).await;
        aus_ergebnis(abfrage, |termin| {
            als_json(&json!({
                "id": termin.id,
                "betreff": termin.subject,
                "start": zeit_text(termin.start.as_ref(), &self.zeitzone, false),
                "ende": zeit_text(termin.end.as_ref(), &self.zeitzone, false),
                "link": termin.web_link,
            }))
        })
    }

    /// Dateien in OneDrive suchen.
    #[tool(
        description = "Dateien in OneDrive suchen: Name, Pfad, Link, geändert.",
        annotations(read_only_hint = true)
    )]
    pub async fn dateien_suchen(
        &self,
        Parameters(anfrage): Parameters<DateienSuchenAnfrage>,
    ) -> Result<String, ErrorData> {
        let suche = anfrage.suche.trim();
        if suche.is_empty() {
            return Err(ErrorData::invalid_params("suche fehlt", None));
        }
        let anzahl = anzahl_begrenzen(anfrage.anzahl);

        let pfad = format!("/me/drive/root/search(q='{}')", kodieren(suche));
        let query = vec![
            ("$select", DATEI_FELDER.to_string()),
            ("$top", anzahl.to_string()),
        ];

        let abfrage = self.graph.get::<Sammlung<Datei>>(&pfad, &query).await;
        aus_ergebnis(abfrage, |liste| {
            als_json(
                &liste
                    .value
                    .iter()
                    .map(|datei| datei_kurz(datei, &self.zeitzone))
                    .collect::<Vec<Value>>(),
            )
        })
    }

    /// Abmelden: das Refresh-Token löschen.
    ///
    /// Verändert nichts bei Graph, aber sehr wohl den Zustand dieses Rechners:
    /// deshalb nicht „nur lesend“ (Aufgabe 03).
    #[tool(
        description = "Abmelden: löscht das gespeicherte Refresh-Token dieses Users.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    pub async fn abmelden(&self) -> Result<String, ErrorData> {
        self.graph
            .anmeldung()
            .abmelden()
            .map_err(|fehler| ErrorData::internal_error(format!("Abmelden: {fehler}"), None))?;
        als_json(&json!({"abgemeldet": true}))
    }
}

#[tool_handler(router = self.tool_router, name = "quiver-m365")]
impl<A: Anmeldung + 'static> ServerHandler for Quiver<A> {}

// ---------------------------------------------------------------------------
// Reine Hilfen: Antwortform und Grenzen
// ---------------------------------------------------------------------------

/// Was im Inneren eines Teams-Tools schiefgehen kann: ein Hinweis für den User
/// (wird Text, kein Fehler) oder ein echter Fehler.
#[derive(Debug)]
pub(crate) enum ToolFehler {
    Hinweis(String),
    Fehler(ErrorData),
}

impl From<GraphFehler> for ToolFehler {
    fn from(fehler: GraphFehler) -> Self {
        match fehler {
            GraphFehler::AnmeldungLaeuft => ToolFehler::Hinweis(ANMELDUNG_HINWEIS.to_string()),
            GraphFehler::Anmeldung(AnmeldeFehler::NichtAngemeldet) => {
                ToolFehler::Hinweis(NICHT_ANGEMELDET_HINWEIS.to_string())
            }
            GraphFehler::Anmeldung(AnmeldeFehler::ZustimmungFehlt) => {
                ToolFehler::Hinweis(ZUSTIMMUNG_HINWEIS.to_string())
            }
            GraphFehler::Graph { status: 403, .. } if fehler.zustimmung_fehlt() => {
                ToolFehler::Hinweis(ZUSTIMMUNG_HINWEIS.to_string())
            }
            anderes => ToolFehler::Fehler(mcp_fehler(anderes)),
        }
    }
}

impl From<ErrorData> for ToolFehler {
    fn from(fehler: ErrorData) -> Self {
        ToolFehler::Fehler(fehler)
    }
}

impl ToolFehler {
    /// Der Hinweis auf eine fehlende Zustimmung nennt bei den neuen Rechten
    /// nicht mehr nur Teams (Aufgabe 06); alles andere bleibt gleich.
    pub(crate) fn fuer_erweiterung(self) -> Self {
        match self {
            ToolFehler::Hinweis(text) if text == ZUSTIMMUNG_HINWEIS => {
                ToolFehler::Hinweis(ZUSTIMMUNG_ERWEITERT_HINWEIS.to_string())
            }
            anderes => anderes,
        }
    }
}

/// Wie [`ergebnis`], aber mit dem Hinweis für die neuen Rechte (Aufgabe 06).
pub(crate) fn ergebnis_erweitert(inner: Result<String, ToolFehler>) -> Result<String, ErrorData> {
    ergebnis(inner.map_err(ToolFehler::fuer_erweiterung))
}

/// Ergebnis eines Teams-Tools: Hinweise werden Text, Fehler bleiben Fehler.
pub(crate) fn ergebnis(inner: Result<String, ToolFehler>) -> Result<String, ErrorData> {
    match inner {
        Ok(text) => Ok(text),
        Err(ToolFehler::Hinweis(text)) => Ok(text),
        Err(ToolFehler::Fehler(fehler)) => Err(fehler),
    }
}

/// Ungültiger Parameter als [`ToolFehler`].
pub(crate) fn ungueltig_tool(text: impl Into<String>) -> ToolFehler {
    ToolFehler::Fehler(ErrorData::invalid_params(text.into(), None))
}

/// `AnmeldungLaeuft` wird zum Hinweistext, ebenso eine fehlende Anmeldung oder
/// Zustimmung; jeder andere Fehler wird zum MCP-Fehler.
fn aus_ergebnis<T>(
    ergebnis: Result<T, GraphFehler>,
    weiter: impl FnOnce(T) -> Result<String, ErrorData>,
) -> Result<String, ErrorData> {
    match ergebnis {
        Ok(wert) => weiter(wert),
        Err(fehler) => match ToolFehler::from(fehler) {
            ToolFehler::Hinweis(text) => Ok(text),
            ToolFehler::Fehler(fehler) => Err(fehler),
        },
    }
}

/// Fehler ohne Token: `GraphFehler` trägt nur Code und Text.
fn mcp_fehler(fehler: GraphFehler) -> ErrorData {
    ErrorData::internal_error(format!("{fehler}"), None)
}

fn ungueltig(text: String) -> ErrorData {
    ErrorData::invalid_params(text, None)
}

pub(crate) fn als_json<T: serde::Serialize>(wert: &T) -> Result<String, ErrorData> {
    serde_json::to_string(wert)
        .map_err(|fehler| ErrorData::internal_error(format!("JSON: {fehler}"), None))
}

/// `anzahl`: Standard 10, immer zwischen 1 und 50 (SPEC.md §4).
pub fn anzahl_begrenzen(anzahl: Option<u32>) -> u32 {
    anzahl
        .unwrap_or(STANDARD_ANZAHL)
        .clamp(1, HOECHSTENS_ANZAHL)
}

/// `max_zeichen`: Standard 4000, immer zwischen 1 und 50 000 (Aufgabe 04).
pub fn max_zeichen_begrenzen(max_zeichen: Option<u32>) -> usize {
    max_zeichen
        .unwrap_or(STANDARD_MAX_ZEICHEN)
        .clamp(1, HOECHSTENS_MAX_ZEICHEN) as usize
}

/// Der Mailtext für die Antwort: Leerzeilen zusammengefasst, auf `max_zeichen`
/// gekürzt. Der zweite Wert ist die Länge des vollen Textes – nur gesetzt, wenn
/// wirklich gekürzt wurde (dann gehören `gekuerzt` und `laenge_gesamt` dazu).
fn mail_text(roh: Option<String>, max_zeichen: usize) -> (Option<String>, Option<usize>) {
    let Some(text) = roh else {
        return (None, None);
    };
    let text = leerzeilen_begrenzen(&text);
    let gesamt = text.chars().count();
    if gesamt <= max_zeichen {
        return (Some(text), None);
    }
    (Some(an_wortgrenze(&text, max_zeichen)), Some(gesamt))
}

/// Mehr als zwei Leerzeilen hintereinander werden zu zwei – der Rest ist Luft,
/// die nur Kontext kostet (Aufgabe 04).
fn leerzeilen_begrenzen(text: &str) -> String {
    let mut raus = String::with_capacity(text.len());
    let mut leere = 0usize;
    for (nummer, zeile) in text.lines().enumerate() {
        if zeile.trim().is_empty() {
            leere += 1;
            if leere > HOECHSTENS_LEERZEILEN {
                continue;
            }
        } else {
            leere = 0;
        }
        if nummer > 0 {
            raus.push('\n');
        }
        if !zeile.trim().is_empty() {
            raus.push_str(zeile.trim_end());
        }
    }
    raus
}

/// Auf höchstens `max` Zeichen kürzen, möglichst hinter einem ganzen Wort.
/// Findet sich im Stück kein Leerzeichen (ein sehr langes Wort), wird hart
/// geschnitten – lieber ein halbes Wort als gar kein Text.
fn an_wortgrenze(text: &str, max: usize) -> String {
    let stueck: String = text.chars().take(max).collect();
    let stelle = match stueck.rfind(char::is_whitespace) {
        Some(stelle) if stelle > 0 => stelle,
        _ => stueck.len(),
    };
    stueck[..stelle].trim_end().to_string()
}

/// Ein Zeitpunkt aus einer Graph-Antwort als RFC 3339 mit Versatz; `nur_datum`
/// für ganztägige Termine (`2026-11-24`).
fn zeit_text(zeit: Option<&Zeitpunkt>, zone: &Zeitzone, nur_datum: bool) -> Option<String> {
    let zeit = zeit?;
    let roh = zeit.date_time.as_deref()?;
    let quelle = zeit.time_zone.as_deref();
    if nur_datum {
        zone.datum(roh, quelle)
    } else {
        zone.zeitpunkt(roh, quelle)
    }
}

/// Ein Zeitpunkt ohne eigene Zone (Mails, Dateien: Graph liefert UTC).
fn zeit_in_zone(roh: Option<&str>, zone: &Zeitzone) -> Option<String> {
    roh.and_then(|wert| zone.zeitpunkt(wert, None))
}

/// „Name \<adresse\>“, sonst was da ist.
fn person(empfaenger: &Empfaenger) -> String {
    match empfaenger.email_address.as_ref() {
        Some(adresse) => adresse_text(adresse),
        None => String::new(),
    }
}

fn adresse_text(adresse: &Adresse) -> String {
    match (adresse.name.as_deref(), adresse.address.as_deref()) {
        (Some(name), Some(mail)) => format!("{name} <{mail}>"),
        (Some(name), None) => name.to_string(),
        (None, Some(mail)) => mail.to_string(),
        (None, None) => String::new(),
    }
}

fn adressen(empfaenger: Option<&[Empfaenger]>) -> Vec<String> {
    empfaenger
        .unwrap_or_default()
        .iter()
        .map(person)
        .filter(|text| !text.is_empty())
        .collect()
}

/// Mail-Adressen als Graph-Empfänger, leere Einträge fallen weg.
fn empfaenger_json(adressen: &[String]) -> Vec<Value> {
    adressen
        .iter()
        .map(|adresse| adresse.trim())
        .filter(|adresse| !adresse.is_empty())
        .map(|adresse| json!({"emailAddress": {"address": adresse}}))
        .collect()
}

fn mail_kurz(mail: &Nachricht, zone: &Zeitzone) -> Value {
    json!({
        "id": mail.id,
        "betreff": mail.subject,
        "von": mail.from.as_ref().map(person),
        "datum": zeit_in_zone(mail.received_date_time.as_deref(), zone),
        "vorschau": mail.body_preview,
    })
}

fn termin_kurz(termin: &Termin, zone: &Zeitzone) -> Value {
    let ganztags = termin.is_all_day.unwrap_or(false);
    json!({
        "id": termin.id,
        "betreff": termin.subject,
        "start": zeit_text(termin.start.as_ref(), zone, ganztags),
        "ende": zeit_text(termin.end.as_ref(), zone, ganztags),
        "ort": termin
            .location
            .as_ref()
            .and_then(|ort| ort.display_name.clone()),
        "teilnehmer": termin
            .attendees
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter_map(|teilnehmer| teilnehmer.email_address.as_ref().map(adresse_text))
            .filter(|text| !text.is_empty())
            .collect::<Vec<String>>(),
        "ganztags": ganztags,
        "link": termin.web_link,
    })
}

fn datei_kurz(datei: &Datei, zone: &Zeitzone) -> Value {
    json!({
        "name": datei.name,
        "pfad": datei
            .parent_reference
            .as_ref()
            .and_then(|eltern| eltern.path.clone()),
        "link": datei.web_url,
        "geaendert": zeit_in_zone(datei.last_modified_date_time.as_deref(), zone),
        "groesse": datei.size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use m365_graph::{Koerper, Ort};

    fn mitglied(name: Option<&str>, adresse: Option<&str>) -> Empfaenger {
        Empfaenger {
            email_address: Some(Adresse {
                name: name.map(str::to_string),
                address: adresse.map(str::to_string),
            }),
        }
    }

    #[test]
    fn anzahl_wird_begrenzt() {
        assert_eq!(anzahl_begrenzen(None), 10);
        assert_eq!(anzahl_begrenzen(Some(0)), 1);
        assert_eq!(anzahl_begrenzen(Some(7)), 7);
        assert_eq!(anzahl_begrenzen(Some(500)), 50);
    }

    #[test]
    fn max_zeichen_wird_begrenzt() {
        assert_eq!(max_zeichen_begrenzen(None), 4000);
        assert_eq!(max_zeichen_begrenzen(Some(0)), 1);
        assert_eq!(max_zeichen_begrenzen(Some(20)), 20);
        assert_eq!(max_zeichen_begrenzen(Some(999_999)), 50_000);
    }

    #[test]
    fn kurzer_text_bleibt_ganz() {
        let (text, gesamt) = mail_text(Some("Kurz.".to_string()), 4000);
        assert_eq!(text.as_deref(), Some("Kurz."));
        assert_eq!(gesamt, None, "nichts gekürzt, keine Zusatzfelder");
        assert_eq!(mail_text(None, 4000), (None, None));
    }

    #[test]
    fn langer_text_wird_an_der_wortgrenze_gekuerzt() {
        let lang = "eins zwei drei vier fünf sechs";
        let (text, gesamt) = mail_text(Some(lang.to_string()), 16);
        // 16 Zeichen sind „eins zwei drei v“ – zurück bis zum Wortende.
        assert_eq!(text.as_deref(), Some("eins zwei drei"));
        assert_eq!(gesamt, Some(lang.chars().count()));
    }

    #[test]
    fn ein_einziges_langes_wort_wird_hart_geschnitten() {
        let lang = "a".repeat(100);
        let (text, gesamt) = mail_text(Some(lang), 10);
        assert_eq!(text.as_deref(), Some("a".repeat(10).as_str()));
        assert_eq!(gesamt, Some(100));
    }

    #[test]
    fn mehr_als_zwei_leerzeilen_werden_zu_zwei() {
        let roh = "Zeile 1\n\n\n\n\nZeile 2\n  \n\t\n\n\nZeile 3";
        assert_eq!(
            leerzeilen_begrenzen(roh),
            "Zeile 1\n\n\nZeile 2\n\n\nZeile 3"
        );
        // Zwei Leerzeilen bleiben zwei.
        assert_eq!(
            leerzeilen_begrenzen("a\n\n\nb"),
            "a\n\n\nb",
            "zwei Leerzeilen sind erlaubt"
        );
        assert_eq!(leerzeilen_begrenzen("a\r\nb"), "a\nb");
    }

    #[test]
    fn person_und_adressen_bleiben_knapp() {
        assert_eq!(
            person(&mitglied(Some("Anna"), Some("anna@firma.de"))),
            "Anna <anna@firma.de>"
        );
        assert_eq!(
            person(&mitglied(None, Some("anna@firma.de"))),
            "anna@firma.de"
        );
        assert_eq!(person(&mitglied(Some("Anna"), None)), "Anna");
        assert_eq!(person(&Empfaenger::default()), "");

        let liste = vec![
            mitglied(Some("Anna"), Some("anna@firma.de")),
            Empfaenger::default(),
        ];
        assert_eq!(adressen(Some(&liste)), vec!["Anna <anna@firma.de>"]);
        assert!(adressen(None).is_empty());
    }

    #[test]
    fn empfaenger_werden_zu_graph_objekten() {
        let werte = empfaenger_json(&[
            " anna@firma.de ".to_string(),
            String::new(),
            "bert@firma.de".to_string(),
        ]);
        assert_eq!(
            werte,
            vec![
                json!({"emailAddress": {"address": "anna@firma.de"}}),
                json!({"emailAddress": {"address": "bert@firma.de"}}),
            ]
        );
    }

    #[test]
    fn mail_und_termin_werden_knapp_ausgegeben() {
        let zone = Zeitzone::neu("Europe/Berlin");
        let mail = Nachricht {
            id: "m1".to_string(),
            subject: Some("Lage".to_string()),
            from: Some(mitglied(Some("Anna"), Some("anna@firma.de"))),
            received_date_time: Some("2026-11-27T09:00:00Z".to_string()),
            body_preview: Some("Kurz".to_string()),
            body: Some(Koerper {
                content_type: Some("text".to_string()),
                content: Some("Lang".to_string()),
            }),
            ..Nachricht::default()
        };
        let wert = mail_kurz(&mail, &zone);
        assert_eq!(wert["id"], "m1");
        assert_eq!(wert["betreff"], "Lage");
        assert_eq!(wert["von"], "Anna <anna@firma.de>");
        assert_eq!(wert["datum"], "2026-11-27T10:00:00+01:00");
        // Der Volltext steht nur in `mail_lesen`, nicht in der Liste.
        assert!(wert.get("text").is_none());

        let termin = Termin {
            subject: Some("Jour fixe".to_string()),
            location: Some(Ort {
                display_name: Some("Raum 1".to_string()),
            }),
            ..Termin::default()
        };
        let wert = termin_kurz(&termin, &zone);
        assert_eq!(wert["betreff"], "Jour fixe");
        assert_eq!(wert["ort"], "Raum 1");
        assert_eq!(wert["teilnehmer"], json!([]));
        assert_eq!(wert["ganztags"], false);
    }

    #[test]
    fn ganztags_termin_hat_nur_ein_datum() {
        let zone = Zeitzone::neu("Europe/Berlin");
        let termin = Termin {
            subject: Some("Urlaub".to_string()),
            start: Some(Zeitpunkt {
                date_time: Some("2026-11-24T00:00:00.0000000".to_string()),
                time_zone: Some("Europe/Berlin".to_string()),
            }),
            end: Some(Zeitpunkt {
                date_time: Some("2026-11-25T00:00:00.0000000".to_string()),
                time_zone: Some("Europe/Berlin".to_string()),
            }),
            is_all_day: Some(true),
            ..Termin::default()
        };
        let wert = termin_kurz(&termin, &zone);
        assert_eq!(wert["ganztags"], true);
        assert_eq!(wert["start"], "2026-11-24");
        assert_eq!(wert["ende"], "2026-11-25");
    }

    #[test]
    fn fehlende_anmeldung_ist_ein_hinweis_und_kein_fehler() {
        let ergebnis: Result<(), GraphFehler> =
            Err(GraphFehler::Anmeldung(AnmeldeFehler::NichtAngemeldet));
        let text = aus_ergebnis(ergebnis, |_| Ok(String::new())).expect("kein Fehler");
        assert_eq!(text, NICHT_ANGEMELDET_HINWEIS);
        assert!(text.contains("quiver-Fenster"), "{text}");
        assert!(text.contains("Anmelden"), "{text}");
    }

    #[test]
    fn fehler_tragen_keinen_token() {
        let fehler = mcp_fehler(GraphFehler::Graph {
            status: 400,
            code: "ErrorInvalidRecipients".to_string(),
            text: "Die Empfänger sind ungültig.".to_string(),
        });
        let text = format!("{fehler}");
        assert!(text.contains("ErrorInvalidRecipients"));
        assert!(text.contains("Die Empfänger sind ungültig."));
    }
}
