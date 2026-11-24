// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Kontakt- und Personentools (docs/aufgaben/06-graph-erweiterung.md,
//! docs/aufgaben/09-leere-antworten-und-loeschen.md).
//!
//! `kontakte_suchen` liest das Adressbuch des Users (`/me/contacts`),
//! `personen_suchen` das, was Microsoft über Kollegen weiß (`/me/people`:
//! Abteilung, Telefon, Mail). `kontakt_anlegen` schreibt ins Adressbuch und ist
//! deshalb verändernd, `kontakt_loeschen` ebenso – es nennt den Namen dessen, was
//! verschwindet.

use m365_anmeldung::Anmeldung;
use m365_graph::{Kontakt, Person, Sammlung};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::ErrorData;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::kodierung::kodieren;
use crate::kontakte::{kontakt_json, person_json};
use crate::server::{als_json, ergebnis_erweitert, ungueltig_tool, Quiver, ToolFehler};

/// `$select` für Kontakte.
const KONTAKT_FELDER: &str = "id,displayName,givenName,surname,companyName,jobTitle,emailAddresses,businessPhones,mobilePhone";
/// `$select` für Personen.
const PERSON_FELDER: &str =
    "id,displayName,givenName,surname,companyName,department,jobTitle,scoredEmailAddresses,phones";

// ---------------------------------------------------------------------------
// Parameter der Tools
// ---------------------------------------------------------------------------

/// Parameter für `kontakte_suchen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct KontakteSuchenAnfrage {
    /// Name, Firma oder Mail-Adresse, nach der das Adressbuch durchsucht wird.
    pub suche: String,
    /// Wie viele Treffer (Standard 20, höchstens 50).
    #[serde(default)]
    pub anzahl: Option<u32>,
}

/// Parameter für `kontakt_anlegen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct KontaktAnlegenAnfrage {
    /// Anzeigename, z. B. `Bert Beispiel`.
    pub name: String,
    /// Vorname (optional).
    #[serde(default)]
    pub vorname: Option<String>,
    /// Nachname (optional).
    #[serde(default)]
    pub nachname: Option<String>,
    /// Mail-Adressen (optional).
    #[serde(default)]
    pub mails: Option<Vec<String>>,
    /// Geschäftliche Telefonnummern (optional).
    #[serde(default)]
    pub telefone: Option<Vec<String>>,
    /// Mobilnummer (optional).
    #[serde(default)]
    pub mobil: Option<String>,
    /// Firma (optional).
    #[serde(default)]
    pub firma: Option<String>,
    /// Position (optional).
    #[serde(default)]
    pub position: Option<String>,
}

/// Parameter für `personen_suchen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct PersonenSuchenAnfrage {
    /// Name, Abteilung oder Mail-Adresse der gesuchten Kollegen.
    pub suche: String,
    /// Wie viele Treffer (Standard 20, höchstens 50).
    #[serde(default)]
    pub anzahl: Option<u32>,
}

/// Parameter für `kontakt_loeschen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct KontaktLoeschenAnfrage {
    /// Id des Kontakts (aus `kontakte_suchen`).
    pub kontakt: String,
}

fn anzahl_mit(anzahl: Option<u32>, standard: u32) -> u32 {
    anzahl.unwrap_or(standard).clamp(1, 50)
}

/// Leere und nur aus Leerzeichen bestehende Einträge fallen weg.
fn texte(werte: Option<&[String]>) -> Vec<String> {
    werte
        .unwrap_or_default()
        .iter()
        .map(|wert| wert.trim().to_string())
        .filter(|wert| !wert.is_empty())
        .collect()
}

// ---------------------------------------------------------------------------
// Die Tools
// ---------------------------------------------------------------------------

#[tool_router(router = kontakte_router, vis = "pub(crate)")]
impl<A: Anmeldung + 'static> Quiver<A> {
    /// Das Adressbuch des Users durchsuchen.
    #[tool(
        description = "Kontakte im Adressbuch des Users suchen (Name, Firma, Mail).",
        annotations(read_only_hint = true)
    )]
    pub async fn kontakte_suchen(
        &self,
        Parameters(anfrage): Parameters<KontakteSuchenAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis_erweitert(self.kontakte_suchen_lauf(anfrage).await)
    }

    /// Einen Kontakt anlegen.
    #[tool(
        description = "Einen Kontakt im Adressbuch des Users anlegen. Verändert das Adressbuch.",
        annotations(destructive_hint = true)
    )]
    pub async fn kontakt_anlegen(
        &self,
        Parameters(anfrage): Parameters<KontaktAnlegenAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis_erweitert(self.kontakt_anlegen_lauf(anfrage).await)
    }

    /// Kollegen mit Abteilung, Telefon und Mail finden.
    #[tool(
        description = "Personen im Tenant finden – mit Abteilung, Telefon und Mail (Microsoft „Personen“).",
        annotations(read_only_hint = true)
    )]
    pub async fn personen_suchen(
        &self,
        Parameters(anfrage): Parameters<PersonenSuchenAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis_erweitert(self.personen_suchen_lauf(anfrage).await)
    }

    /// Einen Kontakt aus dem Adressbuch löschen.
    #[tool(
        description = "Einen Kontakt aus dem Adressbuch des Users löschen. Verändert das Adressbuch.",
        annotations(destructive_hint = true)
    )]
    pub async fn kontakt_loeschen(
        &self,
        Parameters(anfrage): Parameters<KontaktLoeschenAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis_erweitert(self.kontakt_loeschen_lauf(anfrage).await)
    }
}

// ---------------------------------------------------------------------------
// Umsetzung der Tools
// ---------------------------------------------------------------------------

impl<A: Anmeldung + 'static> Quiver<A> {
    async fn kontakte_suchen_lauf(
        &self,
        anfrage: KontakteSuchenAnfrage,
    ) -> Result<String, ToolFehler> {
        let suche = anfrage.suche.trim();
        if suche.is_empty() {
            return Err(ungueltig_tool("suche fehlt"));
        }
        let anzahl = anzahl_mit(anfrage.anzahl, 20);
        let kontakte: Sammlung<Kontakt> = self
            .graph
            .get(
                "/me/contacts",
                &[
                    ("$search", format!("\"{suche}\"")),
                    ("$select", KONTAKT_FELDER.to_string()),
                    ("$top", anzahl.to_string()),
                ],
            )
            .await?;
        let liste: Vec<Value> = kontakte
            .value
            .iter()
            .take(anzahl as usize)
            .map(kontakt_json)
            .collect();
        als_json(&liste).map_err(ToolFehler::from)
    }

    async fn kontakt_anlegen_lauf(
        &self,
        anfrage: KontaktAnlegenAnfrage,
    ) -> Result<String, ToolFehler> {
        let name = anfrage.name.trim();
        if name.is_empty() {
            return Err(ungueltig_tool("name fehlt"));
        }
        let mut body = json!({"displayName": name});

        let optional = |wert: Option<&str>| {
            wert.map(str::trim)
                .filter(|wert| !wert.is_empty())
                .map(str::to_string)
        };
        for (feld, wert) in [
            ("givenName", optional(anfrage.vorname.as_deref())),
            ("surname", optional(anfrage.nachname.as_deref())),
            ("companyName", optional(anfrage.firma.as_deref())),
            ("jobTitle", optional(anfrage.position.as_deref())),
            ("mobilePhone", optional(anfrage.mobil.as_deref())),
        ] {
            if let Some(wert) = wert {
                body[feld] = json!(wert);
            }
        }

        let mails = texte(anfrage.mails.as_deref());
        if !mails.is_empty() {
            body["emailAddresses"] = json!(mails
                .iter()
                .map(|mail| json!({"address": mail}))
                .collect::<Vec<Value>>());
        }
        let telefone = texte(anfrage.telefone.as_deref());
        if !telefone.is_empty() {
            body["businessPhones"] = json!(telefone);
        }

        let neu: Kontakt = self.graph.post("/me/contacts", &body).await?;
        let mut antwort = kontakt_json(&neu);
        antwort["angelegt"] = json!(true);
        als_json(&antwort).map_err(ToolFehler::from)
    }

    async fn personen_suchen_lauf(
        &self,
        anfrage: PersonenSuchenAnfrage,
    ) -> Result<String, ToolFehler> {
        let suche = anfrage.suche.trim();
        if suche.is_empty() {
            return Err(ungueltig_tool("suche fehlt"));
        }
        let anzahl = anzahl_mit(anfrage.anzahl, 20);
        let personen: Sammlung<Person> = self
            .graph
            .get(
                "/me/people",
                &[
                    ("$search", format!("\"{suche}\"")),
                    ("$select", PERSON_FELDER.to_string()),
                    ("$top", anzahl.to_string()),
                ],
            )
            .await?;
        let liste: Vec<Value> = personen
            .value
            .iter()
            .take(anzahl as usize)
            .map(person_json)
            .collect();
        als_json(&liste).map_err(ToolFehler::from)
    }

    async fn kontakt_loeschen_lauf(
        &self,
        anfrage: KontaktLoeschenAnfrage,
    ) -> Result<String, ToolFehler> {
        let kontakt = anfrage.kontakt.trim();
        if kontakt.is_empty() {
            return Err(ungueltig_tool("kontakt fehlt"));
        }
        let pfad = format!("/me/contacts/{}", kodieren(kontakt));
        // Erst lesen: der Name gehört in die Meldung, ein 404 soll keinen DELETE auslösen.
        let aktuell: Kontakt = self
            .graph
            .get(&pfad, &[("$select", KONTAKT_FELDER.to_string())])
            .await?;
        self.graph.delete(&pfad, &[]).await?;
        als_json(&json!({
            "geloescht": true,
            "kontakt": kontakt,
            "name": aktuell.display_name,
        }))
        .map_err(ToolFehler::from)
    }
}
