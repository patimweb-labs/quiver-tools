// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die MCP-Tools aus SPEC.md §4 – genau die acht der Tabelle.
//!
//! Regeln: Antworten sind knappes JSON als Text; `AnmeldungLaeuft` ist **kein**
//! Fehler, sondern der Hinweistext aus SPEC.md §2; Fehler von Graph werden zu
//! MCP-Fehlern mit Graph-Code und Text – ohne Token.

use m365_anmeldung::Anmeldung;
use m365_graph::{
    Adresse, Datei, Empfaenger, GraphClient, GraphFehler, Nachricht, Profil, Sammlung, Termin,
};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::ErrorData;
use rmcp::{tool, tool_handler, tool_router, ServerHandler};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::kodierung::kodieren;
use crate::zeit::iso_zeit;

/// Der eine Satz aus SPEC.md §2, wenn die Anmeldung im Browser läuft.
pub const ANMELDUNG_HINWEIS: &str =
    "Anmeldung im Browser geöffnet – bitte anmelden und den Aufruf danach wiederholen.";

/// Kopfzeile für `mail_lesen`: Text statt HTML (SPEC.md §4).
const PREFER_TEXT: &str = "outlook.body-content-type=\"text\"";
/// Wie viele Mails/Treffer ein Tool höchstens liefert.
const HOECHSTENS_ANZAHL: u32 = 50;
/// Standard-Anzahl, wenn `anzahl` fehlt.
const STANDARD_ANZAHL: u32 = 10;

const MAIL_KURZ: &str = "id,subject,from,receivedDateTime,bodyPreview";
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
    /// Beginn des Zeitraums, ISO 8601, z. B. `2026-11-27T10:00`.
    pub von: String,
    /// Ende des Zeitraums, ISO 8601, z. B. `2026-11-28T18:00`.
    pub bis: String,
}

/// Parameter für `termin_anlegen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TerminAnlegenAnfrage {
    /// Betreff des Termins.
    pub betreff: String,
    /// Beginn, ISO 8601, z. B. `2026-11-27T10:00`.
    pub start: String,
    /// Ende, ISO 8601, z. B. `2026-11-27T11:00`.
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

/// Der MCP-Server mit den acht Tools.
pub struct Quiver<A: Anmeldung> {
    graph: GraphClient<A>,
    /// Zeitzone für Termine (`M365_ZEITZONE`, Standard `Europe/Berlin`).
    zeitzone: String,
    tool_router: ToolRouter<Self>,
}

impl<A: Anmeldung + 'static> Quiver<A> {
    /// Baut den Server aus Graph-Client und Zeitzone.
    pub fn neu(graph: GraphClient<A>, zeitzone: impl Into<String>) -> Self {
        Self {
            graph,
            zeitzone: zeitzone.into(),
            tool_router: Self::tool_router(),
        }
    }

    /// Die Zeitzone, mit der Termine gelesen und geschrieben werden.
    pub fn zeitzone(&self) -> &str {
        &self.zeitzone
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
            als_json(&liste.value.iter().map(mail_kurz).collect::<Vec<Value>>())
        })
    }

    /// Eine Mail im Volltext lesen.
    #[tool(
        description = "Eine Mail lesen: Betreff, Absender, Empfänger, Datum, Text.",
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

        let abfrage = self
            .graph
            .get_mit_kopf::<Nachricht>(
                &format!("/me/messages/{}", kodieren(id)),
                &[("$select", MAIL_VOLL.to_string())],
                &[("Prefer", PREFER_TEXT)],
            )
            .await;

        aus_ergebnis(abfrage, |mail| {
            als_json(&json!({
                "id": mail.id,
                "betreff": mail.subject,
                "von": mail.from.as_ref().map(person),
                "an": adressen(mail.to_recipients.as_deref()),
                "cc": adressen(mail.cc_recipients.as_deref()),
                "datum": mail.received_date_time,
                "text": mail.body.and_then(|koerper| koerper.content),
                "link": mail.web_link,
            }))
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
        let von = iso_zeit(&anfrage.von).map_err(ungueltig)?;
        let bis = iso_zeit(&anfrage.bis).map_err(ungueltig)?;

        let prefer = format!("outlook.timezone=\"{}\"", self.zeitzone);
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
            als_json(&liste.value.iter().map(termin_kurz).collect::<Vec<Value>>())
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
        let start = iso_zeit(&anfrage.start).map_err(ungueltig)?;
        let ende = iso_zeit(&anfrage.ende).map_err(ungueltig)?;

        let mut termin = json!({
            "subject": anfrage.betreff,
            "start": {"dateTime": start, "timeZone": self.zeitzone},
            "end": {"dateTime": ende, "timeZone": self.zeitzone},
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
                "start": termin.start.and_then(|zeit| zeit.date_time),
                "ende": termin.end.and_then(|zeit| zeit.date_time),
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
            als_json(&liste.value.iter().map(datei_kurz).collect::<Vec<Value>>())
        })
    }

    /// Abmelden: das Refresh-Token löschen.
    #[tool(
        description = "Abmelden: löscht das gespeicherte Refresh-Token dieses Users.",
        annotations(read_only_hint = true)
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

/// `AnmeldungLaeuft` wird zum Hinweistext, jeder andere Fehler zum MCP-Fehler.
fn aus_ergebnis<T>(
    ergebnis: Result<T, GraphFehler>,
    weiter: impl FnOnce(T) -> Result<String, ErrorData>,
) -> Result<String, ErrorData> {
    match ergebnis {
        Ok(wert) => weiter(wert),
        Err(GraphFehler::AnmeldungLaeuft) => Ok(ANMELDUNG_HINWEIS.to_string()),
        Err(fehler) => Err(mcp_fehler(fehler)),
    }
}

/// Fehler ohne Token: `GraphFehler` trägt nur Code und Text.
fn mcp_fehler(fehler: GraphFehler) -> ErrorData {
    ErrorData::internal_error(format!("{fehler}"), None)
}

fn ungueltig(text: String) -> ErrorData {
    ErrorData::invalid_params(text, None)
}

fn als_json<T: serde::Serialize>(wert: &T) -> Result<String, ErrorData> {
    serde_json::to_string(wert)
        .map_err(|fehler| ErrorData::internal_error(format!("JSON: {fehler}"), None))
}

/// `anzahl`: Standard 10, immer zwischen 1 und 50 (SPEC.md §4).
pub fn anzahl_begrenzen(anzahl: Option<u32>) -> u32 {
    anzahl
        .unwrap_or(STANDARD_ANZAHL)
        .clamp(1, HOECHSTENS_ANZAHL)
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

fn mail_kurz(mail: &Nachricht) -> Value {
    json!({
        "id": mail.id,
        "betreff": mail.subject,
        "von": mail.from.as_ref().map(person),
        "datum": mail.received_date_time,
        "vorschau": mail.body_preview,
    })
}

fn termin_kurz(termin: &Termin) -> Value {
    json!({
        "id": termin.id,
        "betreff": termin.subject,
        "start": termin.start.as_ref().and_then(|zeit| zeit.date_time.clone()),
        "ende": termin.end.as_ref().and_then(|zeit| zeit.date_time.clone()),
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
        "ganztags": termin.is_all_day,
        "link": termin.web_link,
    })
}

fn datei_kurz(datei: &Datei) -> Value {
    json!({
        "name": datei.name,
        "pfad": datei
            .parent_reference
            .as_ref()
            .and_then(|eltern| eltern.path.clone()),
        "link": datei.web_url,
        "geaendert": datei.last_modified_date_time,
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
        let wert = mail_kurz(&mail);
        assert_eq!(wert["id"], "m1");
        assert_eq!(wert["betreff"], "Lage");
        assert_eq!(wert["von"], "Anna <anna@firma.de>");
        // Der Volltext steht nur in `mail_lesen`, nicht in der Liste.
        assert!(wert.get("text").is_none());

        let termin = Termin {
            subject: Some("Jour fixe".to_string()),
            location: Some(Ort {
                display_name: Some("Raum 1".to_string()),
            }),
            ..Termin::default()
        };
        let wert = termin_kurz(&termin);
        assert_eq!(wert["betreff"], "Jour fixe");
        assert_eq!(wert["ort"], "Raum 1");
        assert_eq!(wert["teilnehmer"], json!([]));
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
