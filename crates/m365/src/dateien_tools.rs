// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! SharePoint- und OneDrive-Tools (docs/aufgaben/06-graph-erweiterung.md,
//! docs/aufgaben/09-leere-antworten-und-loeschen.md).
//!
//! Regeln wie bei Teams: Namen dürfen Ids ersetzen (aufgelöst über die Listen,
//! mehrdeutig → Kandidaten und keine Aktion), Antworten sind knappes JSON, und
//! alles, was anlegt oder ändert, wird **nie** automatisch wiederholt.
//!
//! Dateiinhalt: Textdateien werden gelesen und auf `max_zeichen` gekürzt;
//! Office-Dateien kann Graph nur als PDF ausgeben (kein Text), PDF und Binäres
//! liefern nur Metadaten. Das sagt die Antwort mit `hinweis`.
//!
//! Aufgabe 09: `PATCH` auf Listenfelder antwortet mit **204 und leerem Rumpf** –
//! Erfolg, den neuen Stand holt ein GET. Schreibgeschützte Spalten fliegen vor dem
//! Senden raus, ein 403 auf einer Liste wird zum Hinweis auf die Listenrechte.
//! Löschen geht in den Papierkorb von OneDrive/SharePoint, nie endgültig.

use m365_anmeldung::Anmeldung;
use m365_graph::{GraphFehler, Laufwerk, LaufwerkEintrag, Liste, Listeneintrag, Website};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::ErrorData;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::aufloesen::{ist_graph_id, waehlen, Anzeige};
use crate::dateien::{
    eintrag_json, inhalt_kuerzen, laufwerk_json, leseart, liste_json, listeneintrag_json,
    max_zeichen_begrenzen, mime_typ, site_json, Leseart, HOECHSTENS_EINTRAEGE, HOECHSTENS_UPLOAD,
};
use crate::kodierung::kodieren;
use crate::server::{als_json, ergebnis_erweitert, ungueltig_tool, Quiver, ToolFehler};

/// So viele Sites/Bibliotheken/Listen holt eine Namenssuche höchstens.
const HOECHSTENS_LISTE: usize = 200;
/// `$select` für Laufwerkseinträge.
const EINTRAG_FELDER: &str =
    "id,name,size,webUrl,createdDateTime,lastModifiedDateTime,folder,file,parentReference";

/// Der Satz bei 403 auf einer Liste: die Rechte liegen bei SharePoint, nicht am Code.
pub const KEIN_SCHREIBRECHT_HINWEIS: &str =
    "Kein Schreibrecht auf diese Liste (SharePoint-Rechte der Liste).";

/// Spalten, die SharePoint selbst führt – sie werden **vor** dem Senden
/// herausgefiltert (Aufgabe 09), sonst antwortet Graph mit `accessDenied`.
const NUR_LESEN_FELDER: &[&str] = &["LinkTitle", "ID", "Created", "Modified", "Author", "Editor"];

// ---------------------------------------------------------------------------
// Parameter der Tools
// ---------------------------------------------------------------------------

/// Parameter für `sharepoint_sites`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SharepointSitesAnfrage {
    /// Suchwort für die Sites, z. B. `Projekt` (Graph verlangt ein Suchwort).
    pub suche: String,
    /// Wie viele Sites (Standard 10, höchstens 50).
    #[serde(default)]
    pub anzahl: Option<u32>,
}

/// Parameter für `sharepoint_bibliotheken`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SharepointBibliothekenAnfrage {
    /// Site als Id (`host,site,web`) oder Name, z. B. `Projekt Nord`.
    pub site: String,
}

/// Parameter für `dateien_liste`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DateienListeAnfrage {
    /// Site als Id oder Name. Ohne `site`: das OneDrive des Users.
    #[serde(default)]
    pub site: Option<String>,
    /// Bibliothek (Laufwerk) als Id oder Name. Ohne Angabe die Dokumente-Bibliothek.
    #[serde(default)]
    pub bibliothek: Option<String>,
    /// Ordner als Pfad (`Projekte/Nord`) oder Id. Ohne Angabe die oberste Ebene.
    #[serde(default)]
    pub ordner: Option<String>,
    /// Wie viele Einträge (Standard 20, höchstens 100).
    #[serde(default)]
    pub anzahl: Option<u32>,
}

/// Parameter für `datei_lesen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DateiLesenAnfrage {
    /// Datei als Pfad (`Projekte/Nord/Notiz.md`) oder Id.
    pub datei: String,
    /// Site als Id oder Name. Ohne `site`: das OneDrive des Users.
    #[serde(default)]
    pub site: Option<String>,
    /// Bibliothek als Id oder Name (optional).
    #[serde(default)]
    pub bibliothek: Option<String>,
    /// Höchstens so viele Zeichen Text (Standard 20 000, höchstens 50 000).
    #[serde(default)]
    pub max_zeichen: Option<u32>,
}

/// Parameter für `datei_hochladen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DateiHochladenAnfrage {
    /// Dateiname mit Endung, z. B. `Notiz.md`.
    pub name: String,
    /// Der Inhalt als Text (höchstens 4 MB).
    pub inhalt: String,
    /// Zielordner als Pfad oder Id. Ohne Angabe die oberste Ebene.
    #[serde(default)]
    pub ordner: Option<String>,
    /// Site als Id oder Name. Ohne `site`: das OneDrive des Users.
    #[serde(default)]
    pub site: Option<String>,
    /// Bibliothek als Id oder Name (optional).
    #[serde(default)]
    pub bibliothek: Option<String>,
    /// Was bei einem vorhandenen Namen passiert: `rename` (Standard),
    /// `replace` oder `fail`.
    #[serde(default)]
    pub verhalten: Option<String>,
}

/// Parameter für `ordner_anlegen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct OrdnerAnlegenAnfrage {
    /// Name des neuen Ordners.
    pub name: String,
    /// Übergeordneter Ordner als Pfad oder Id. Ohne Angabe die oberste Ebene.
    #[serde(default)]
    pub ordner: Option<String>,
    /// Site als Id oder Name. Ohne `site`: das OneDrive des Users.
    #[serde(default)]
    pub site: Option<String>,
    /// Bibliothek als Id oder Name (optional).
    #[serde(default)]
    pub bibliothek: Option<String>,
}

/// Parameter für `sharepoint_listen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SharepointListenAnfrage {
    /// Site als Id oder Name.
    pub site: String,
}

/// Parameter für `sharepoint_listeneintraege`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SharepointListeneintraegeAnfrage {
    /// Site als Id oder Name.
    pub site: String,
    /// Liste als Id oder Anzeigename, z. B. `Aufgaben`.
    pub liste: String,
    /// Wie viele Einträge (Standard 20, höchstens 100).
    #[serde(default)]
    pub anzahl: Option<u32>,
    /// Graph-Filter über die Spalten, z. B. `fields/Status eq 'offen'`
    /// (nur zusammen mit `$expand=fields` sinnvoll – das setzt das Tool).
    #[serde(default)]
    pub filter: Option<String>,
}

/// Parameter für `sharepoint_listeneintrag_anlegen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SharepointListeneintragAnlegenAnfrage {
    /// Site als Id oder Name.
    pub site: String,
    /// Liste als Id oder Anzeigename.
    pub liste: String,
    /// Die Spaltenwerte als JSON-Objekt, z. B. `{"Titel": "Erster", "Status": "offen"}`.
    pub felder: Value,
}

/// Parameter für `sharepoint_listeneintrag_aendern`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SharepointListeneintragAendernAnfrage {
    /// Site als Id oder Name.
    pub site: String,
    /// Liste als Id oder Anzeigename.
    pub liste: String,
    /// Id des Eintrags (aus `sharepoint_listeneintraege`).
    pub eintrag: String,
    /// Die zu ändernden Spalten als JSON-Objekt, z. B. `{"Status": "fertig"}`.
    pub felder: Value,
}

/// Parameter für `datei_loeschen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DateiLoeschenAnfrage {
    /// Datei oder Ordner als Pfad (`Projekte/Nord/Notiz.md`) oder Id.
    pub datei: String,
    /// Site als Id oder Name. Ohne `site`: das OneDrive des Users.
    #[serde(default)]
    pub site: Option<String>,
    /// Bibliothek als Id oder Name (optional).
    #[serde(default)]
    pub bibliothek: Option<String>,
}

/// Parameter für `sharepoint_listeneintrag_loeschen`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SharepointListeneintragLoeschenAnfrage {
    /// Site als Id oder Name.
    pub site: String,
    /// Liste als Id oder Anzeigename.
    pub liste: String,
    /// Id des Eintrags (aus `sharepoint_listeneintraege`).
    pub eintrag: String,
}

// ---------------------------------------------------------------------------
// Wo liegen die Dateien?
// ---------------------------------------------------------------------------

/// Das OneDrive des Users oder eine Bibliothek einer Site.
#[derive(Debug, Clone)]
enum Ort {
    OneDrive,
    Bibliothek(Anzeige),
}

impl Ort {
    /// Der Graph-Pfad, unter dem Laufwerkseinträge hängen.
    fn basis(&self) -> String {
        match self {
            Ort::OneDrive => "/me/drive".to_string(),
            Ort::Bibliothek(laufwerk) => format!("/drives/{}", laufwerk.id),
        }
    }

    fn name(&self) -> String {
        match self {
            Ort::OneDrive => "OneDrive".to_string(),
            Ort::Bibliothek(laufwerk) => laufwerk.name.clone(),
        }
    }
}

/// Einen Pfad prozent-kodieren, Schrägstriche bleiben Schrägstriche.
fn pfad_kodiert(pfad: &str) -> String {
    pfad.split('/')
        .map(str::trim)
        .filter(|teil| !teil.is_empty())
        .map(kodieren)
        .collect::<Vec<String>>()
        .join("/")
}

/// Pfad der Kinder eines Ordners (oberste Ebene oder ein Ordner).
fn kinder_pfad(ort: &Ort, ordner: Option<&str>) -> String {
    let basis = ort.basis();
    let ordner = ordner.map(str::trim).unwrap_or_default();
    if ordner.is_empty() {
        return format!("{basis}/root/children");
    }
    if ist_graph_id(ordner) {
        format!("{basis}/items/{}/children", kodieren(ordner))
    } else {
        format!("{basis}/root:/{}:/children", pfad_kodiert(ordner))
    }
}

/// Pfad eines Eintrags (Datei oder Ordner) über Pfad oder Id.
fn eintrag_pfad(ort: &Ort, referenz: &str) -> String {
    let basis = ort.basis();
    if ist_graph_id(referenz) {
        format!("{basis}/items/{}", kodieren(referenz))
    } else {
        format!("{basis}/root:/{}", pfad_kodiert(referenz))
    }
}

/// Pfad für einen Upload/einen neuen Ordner unterhalb von `ordner`.
fn ziel_pfad(ort: &Ort, ordner: Option<&str>, name: &str) -> String {
    let basis = ort.basis();
    let ordner = ordner.map(str::trim).unwrap_or_default();
    if ordner.is_empty() {
        return format!("{basis}/root:/{}", kodieren(name));
    }
    if ist_graph_id(ordner) {
        format!("{basis}/items/{}:/{}", kodieren(ordner), kodieren(name))
    } else {
        format!("{basis}/root:/{}/{}", pfad_kodiert(ordner), kodieren(name))
    }
}

fn anzahl_mit(anzahl: Option<u32>, standard: u32, hoechstens: u32) -> u32 {
    anzahl.unwrap_or(standard).clamp(1, hoechstens)
}

/// `felder` muss ein nicht-leeres JSON-Objekt sein.
fn felder_pruefen(felder: &Value) -> Result<Value, ToolFehler> {
    match felder {
        Value::Object(karte) if !karte.is_empty() => Ok(felder.clone()),
        _ => Err(ungueltig_tool(
            "felder muss ein JSON-Objekt mit mindestens einer Spalte sein, z. B. {\"Titel\": \"Erster\"}",
        )),
    }
}

/// Ist die Spalte eine, die SharePoint selbst führt (`LinkTitle`, `ID`, `Created`,
/// `Modified`, `Author`, `Editor` oder etwas mit `_` am Anfang)?
fn ist_nur_lesen(spalte: &str) -> bool {
    spalte.starts_with('_')
        || NUR_LESEN_FELDER
            .iter()
            .any(|name| name.eq_ignore_ascii_case(spalte))
}

/// Schreibgeschützte Spalten herausnehmen: was zu ändern bleibt, und was
/// ignoriert wurde (für den Hinweis in der Antwort).
fn schreibbare_felder(felder: &Value) -> (Value, Vec<String>) {
    let mut bleibt = serde_json::Map::new();
    let mut ignoriert = Vec::new();
    if let Value::Object(karte) = felder {
        for (spalte, wert) in karte {
            if ist_nur_lesen(spalte) {
                ignoriert.push(spalte.clone());
            } else {
                bleibt.insert(spalte.clone(), wert.clone());
            }
        }
    }
    (Value::Object(bleibt), ignoriert)
}

/// Ein 403 auf einer Liste heißt „die Liste ist schreibgeschützt“ – nicht „kaputt“.
fn liste_fehler(fehler: GraphFehler) -> ToolFehler {
    if fehler.zustimmung_fehlt() {
        return ToolFehler::from(fehler);
    }
    match &fehler {
        GraphFehler::Graph { status: 403, .. } => {
            ToolFehler::Hinweis(KEIN_SCHREIBRECHT_HINWEIS.to_string())
        }
        _ => ToolFehler::from(fehler),
    }
}

/// Das Konfliktverhalten für einen Upload.
fn verhalten_pruefen(roh: Option<&str>) -> Result<&'static str, ToolFehler> {
    match roh.map(str::trim).unwrap_or("rename") {
        "" | "rename" => Ok("rename"),
        "replace" | "ersetzen" => Ok("replace"),
        "fail" => Ok("fail"),
        anderes => Err(ungueltig_tool(format!(
            "verhalten {anderes:?} kenne ich nicht – erlaubt sind rename (Standard), replace und fail"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Die Tools
// ---------------------------------------------------------------------------

#[tool_router(router = dateien_router, vis = "pub(crate)")]
impl<A: Anmeldung + 'static> Quiver<A> {
    /// SharePoint-Sites suchen.
    #[tool(
        description = "SharePoint-Sites suchen (Id und Name). Grundlage für Bibliotheken, Dateien und Listen.",
        annotations(read_only_hint = true)
    )]
    pub async fn sharepoint_sites(
        &self,
        Parameters(anfrage): Parameters<SharepointSitesAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis_erweitert(self.sharepoint_sites_lauf(anfrage).await)
    }

    /// Die Bibliotheken (Drives) einer Site.
    #[tool(
        description = "Bibliotheken (Dokumentordner) einer SharePoint-Site auflisten.",
        annotations(read_only_hint = true)
    )]
    pub async fn sharepoint_bibliotheken(
        &self,
        Parameters(anfrage): Parameters<SharepointBibliothekenAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis_erweitert(self.sharepoint_bibliotheken_lauf(anfrage).await)
    }

    /// Den Inhalt eines Ordners lesen.
    #[tool(
        description = "Ordnerinhalt lesen – in OneDrive oder in einer Bibliothek einer SharePoint-Site, mit Pfad oder Id.",
        annotations(read_only_hint = true)
    )]
    pub async fn dateien_liste(
        &self,
        Parameters(anfrage): Parameters<DateienListeAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis_erweitert(self.dateien_liste_lauf(anfrage).await)
    }

    /// Eine Datei als Text lesen.
    #[tool(
        description = "Eine Datei lesen: Text, Markdown, CSV und JSON als Text (gekürzt auf max_zeichen); Office-Dateien und PDFs nur als Metadaten mit Hinweis.",
        annotations(read_only_hint = true)
    )]
    pub async fn datei_lesen(
        &self,
        Parameters(anfrage): Parameters<DateiLesenAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis_erweitert(self.datei_lesen_lauf(anfrage).await)
    }

    /// Eine Textdatei hochladen.
    #[tool(
        description = "Eine Textdatei hochladen (höchstens 4 MB, einfacher PUT). Bei vorhandenem Namen wird standardmäßig umbenannt. Verändert OneDrive bzw. SharePoint.",
        annotations(destructive_hint = true)
    )]
    pub async fn datei_hochladen(
        &self,
        Parameters(anfrage): Parameters<DateiHochladenAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis_erweitert(self.datei_hochladen_lauf(anfrage).await)
    }

    /// Einen Ordner anlegen.
    #[tool(
        description = "Einen Ordner in OneDrive oder einer SharePoint-Bibliothek anlegen. Verändert den Speicher des Users.",
        annotations(destructive_hint = true)
    )]
    pub async fn ordner_anlegen(
        &self,
        Parameters(anfrage): Parameters<OrdnerAnlegenAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis_erweitert(self.ordner_anlegen_lauf(anfrage).await)
    }

    /// Die Listen einer Site.
    #[tool(
        description = "SharePoint-Listen einer Site auflisten (Id, Name, Vorlage).",
        annotations(read_only_hint = true)
    )]
    pub async fn sharepoint_listen(
        &self,
        Parameters(anfrage): Parameters<SharepointListenAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis_erweitert(self.sharepoint_listen_lauf(anfrage).await)
    }

    /// Einträge einer Liste.
    #[tool(
        description = "Einträge einer SharePoint-Liste lesen, inklusive der Spaltenwerte.",
        annotations(read_only_hint = true)
    )]
    pub async fn sharepoint_listeneintraege(
        &self,
        Parameters(anfrage): Parameters<SharepointListeneintraegeAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis_erweitert(self.sharepoint_listeneintraege_lauf(anfrage).await)
    }

    /// Einen Listeneintrag anlegen.
    #[tool(
        description = "Einen Eintrag in einer SharePoint-Liste anlegen. Verändert die Liste.",
        annotations(destructive_hint = true)
    )]
    pub async fn sharepoint_listeneintrag_anlegen(
        &self,
        Parameters(anfrage): Parameters<SharepointListeneintragAnlegenAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis_erweitert(self.sharepoint_listeneintrag_anlegen_lauf(anfrage).await)
    }

    /// Einen Listeneintrag ändern.
    #[tool(
        description = "Die Spaltenwerte eines SharePoint-Listeneintrags ändern. Schreibgeschützte Spalten (LinkTitle, ID, Created, Modified, Author, Editor, _…) werden vorher herausgefiltert. Verändert die Liste.",
        annotations(destructive_hint = true)
    )]
    pub async fn sharepoint_listeneintrag_aendern(
        &self,
        Parameters(anfrage): Parameters<SharepointListeneintragAendernAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis_erweitert(self.sharepoint_listeneintrag_aendern_lauf(anfrage).await)
    }

    /// Eine Datei oder einen Ordner löschen.
    #[tool(
        description = "Eine Datei oder einen Ordner löschen – in den Papierkorb von OneDrive/SharePoint, nicht endgültig. Verändert den Speicher des Users.",
        annotations(destructive_hint = true)
    )]
    pub async fn datei_loeschen(
        &self,
        Parameters(anfrage): Parameters<DateiLoeschenAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis_erweitert(self.datei_loeschen_lauf(anfrage).await)
    }

    /// Einen Listeneintrag löschen.
    #[tool(
        description = "Einen Eintrag aus einer SharePoint-Liste löschen (in den Papierkorb der Liste). Verändert die Liste.",
        annotations(destructive_hint = true)
    )]
    pub async fn sharepoint_listeneintrag_loeschen(
        &self,
        Parameters(anfrage): Parameters<SharepointListeneintragLoeschenAnfrage>,
    ) -> Result<String, ErrorData> {
        ergebnis_erweitert(self.sharepoint_listeneintrag_loeschen_lauf(anfrage).await)
    }
}

// ---------------------------------------------------------------------------
// Auflösen
// ---------------------------------------------------------------------------

impl<A: Anmeldung + 'static> Quiver<A> {
    async fn site_finden(&self, referenz: &str) -> Result<Anzeige, ToolFehler> {
        let referenz = referenz.trim();
        if referenz.is_empty() {
            return Err(ungueltig_tool("site fehlt"));
        }
        if ist_graph_id(referenz) {
            return Ok(Anzeige::neu(referenz, None));
        }
        let sites = self
            .graph
            .get_seiten::<Website>(
                "/sites",
                &[("search", referenz.to_string())],
                HOECHSTENS_LISTE,
            )
            .await?;
        let kandidaten: Vec<Anzeige> = sites
            .into_iter()
            .map(|site| {
                let name = site.display_name.clone().or_else(|| site.name.clone());
                Anzeige::neu(site.id, name.as_deref())
            })
            .collect();
        waehlen(kandidaten, referenz, "Site")
    }

    async fn laufwerke(&self, site: &Anzeige) -> Result<Vec<Laufwerk>, ToolFehler> {
        Ok(self
            .graph
            .get_seiten::<Laufwerk>(
                &format!("/sites/{}/drives", kodieren(&site.id)),
                &[(
                    "$select",
                    "id,name,description,driveType,webUrl".to_string(),
                )],
                HOECHSTENS_LISTE,
            )
            .await?)
    }

    /// OneDrive oder die passende Bibliothek einer Site.
    async fn ort_finden(
        &self,
        site: Option<&str>,
        bibliothek: Option<&str>,
    ) -> Result<Ort, ToolFehler> {
        let site = site.map(str::trim).filter(|site| !site.is_empty());
        let bibliothek = bibliothek
            .map(str::trim)
            .filter(|bibliothek| !bibliothek.is_empty());
        let Some(site) = site else {
            if bibliothek.is_some() {
                return Err(ungueltig_tool(
                    "bibliothek braucht site – ohne site arbeiten die Datei-Tools im OneDrive des Users",
                ));
            }
            return Ok(Ort::OneDrive);
        };

        let site = self.site_finden(site).await?;
        let laufwerke = self.laufwerke(&site).await?;
        let kandidaten: Vec<Anzeige> = laufwerke
            .iter()
            .map(|laufwerk| Anzeige::neu(laufwerk.id.clone(), laufwerk.name.as_deref()))
            .collect();

        if let Some(bibliothek) = bibliothek {
            return Ok(Ort::Bibliothek(waehlen(
                kandidaten,
                bibliothek,
                "Bibliothek",
            )?));
        }
        if kandidaten.is_empty() {
            return Err(ungueltig_tool(format!(
                "Site {:?} hat keine Bibliothek",
                site.name
            )));
        }
        // Ohne Namen: die Dokumente-Bibliothek, sonst die erste.
        let standard = kandidaten
            .iter()
            .find(|kandidat| {
                let name = kandidat.name.to_lowercase();
                name == "dokumente" || name == "documents" || name == "freigegebene dokumente"
            })
            .cloned()
            .unwrap_or_else(|| kandidaten[0].clone());
        Ok(Ort::Bibliothek(standard))
    }

    async fn liste_finden(&self, site: &Anzeige, referenz: &str) -> Result<Anzeige, ToolFehler> {
        let referenz = referenz.trim();
        if referenz.is_empty() {
            return Err(ungueltig_tool("liste fehlt"));
        }
        if ist_graph_id(referenz) {
            return Ok(Anzeige::neu(referenz, None));
        }
        let listen = self
            .graph
            .get_seiten::<Liste>(
                &format!("/sites/{}/lists", kodieren(&site.id)),
                &[(
                    "$select",
                    "id,name,displayName,description,webUrl,list".to_string(),
                )],
                HOECHSTENS_LISTE,
            )
            .await?;
        let kandidaten: Vec<Anzeige> = listen
            .into_iter()
            .map(|liste| {
                let name = liste.display_name.clone().or_else(|| liste.name.clone());
                Anzeige::neu(liste.id, name.as_deref())
            })
            .collect();
        waehlen(kandidaten, referenz, "Liste")
    }
}

// ---------------------------------------------------------------------------
// Umsetzung der Tools
// ---------------------------------------------------------------------------

impl<A: Anmeldung + 'static> Quiver<A> {
    async fn sharepoint_sites_lauf(
        &self,
        anfrage: SharepointSitesAnfrage,
    ) -> Result<String, ToolFehler> {
        let suche = anfrage.suche.trim();
        if suche.is_empty() {
            return Err(ungueltig_tool(
                "suche fehlt – Graph liefert Sites nur zu einem Suchwort",
            ));
        }
        let anzahl = anzahl_mit(anfrage.anzahl, 10, 50);
        let sites = self
            .graph
            .get_seiten::<Website>(
                "/sites",
                &[("search", suche.to_string()), ("$top", anzahl.to_string())],
                anzahl as usize,
            )
            .await?;
        let liste: Vec<Value> = sites.iter().map(site_json).collect();
        als_json(&liste).map_err(ToolFehler::from)
    }

    async fn sharepoint_bibliotheken_lauf(
        &self,
        anfrage: SharepointBibliothekenAnfrage,
    ) -> Result<String, ToolFehler> {
        let site = self.site_finden(&anfrage.site).await?;
        let laufwerke = self.laufwerke(&site).await?;
        let liste: Vec<Value> = laufwerke.iter().map(laufwerk_json).collect();
        als_json(&json!({"site": site.text(), "bibliotheken": liste})).map_err(ToolFehler::from)
    }

    async fn dateien_liste_lauf(&self, anfrage: DateienListeAnfrage) -> Result<String, ToolFehler> {
        let ort = self
            .ort_finden(anfrage.site.as_deref(), anfrage.bibliothek.as_deref())
            .await?;
        let anzahl = anzahl_mit(anfrage.anzahl, 20, 100);
        let pfad = kinder_pfad(&ort, anfrage.ordner.as_deref());
        let eintraege = self
            .graph
            .get_seiten::<LaufwerkEintrag>(
                &pfad,
                &[
                    ("$select", EINTRAG_FELDER.to_string()),
                    ("$top", anzahl.to_string()),
                ],
                anzahl as usize,
            )
            .await?;
        let liste: Vec<Value> = eintraege
            .iter()
            .map(|eintrag| eintrag_json(eintrag, &self.zeitzone))
            .collect();
        als_json(&liste).map_err(ToolFehler::from)
    }

    async fn datei_lesen_lauf(&self, anfrage: DateiLesenAnfrage) -> Result<String, ToolFehler> {
        let datei = anfrage.datei.trim();
        if datei.is_empty() {
            return Err(ungueltig_tool("datei fehlt"));
        }
        let max_zeichen = max_zeichen_begrenzen(anfrage.max_zeichen);
        let ort = self
            .ort_finden(anfrage.site.as_deref(), anfrage.bibliothek.as_deref())
            .await?;

        let eintrag: LaufwerkEintrag = self
            .graph
            .get(
                &eintrag_pfad(&ort, datei),
                &[("$select", EINTRAG_FELDER.to_string())],
            )
            .await?;
        let name = eintrag.name.clone().unwrap_or_else(|| datei.to_string());

        let mut antwort = eintrag_json(&eintrag, &self.zeitzone);
        antwort["inhaltstyp"] = json!(leseart(&name).bezeichnung());
        if eintrag.ist_ordner() {
            antwort["hinweis"] = json!("Das ist ein Ordner – mit `dateien_liste` hineinschauen.");
            return als_json(&antwort).map_err(ToolFehler::from);
        }

        let inhalts_pfad = format!("{}/items/{}/content", ort.basis(), kodieren(&eintrag.id));
        match leseart(&name) {
            Leseart::Text => {
                let inhalt = self.graph.get_text(&inhalts_pfad, &[]).await?;
                let (text, gekuerzt, gesamt) = inhalt_kuerzen(&inhalt, max_zeichen);
                antwort["text"] = json!(text);
                if let Some(gekuerzt) = gekuerzt {
                    antwort["gekuerzt"] = json!(gekuerzt);
                    antwort["laenge_gesamt"] = json!(gesamt);
                }
            }
            Leseart::Office => {
                // Graph kann Office-Dateien nur nach PDF wandeln – Text gibt es
                // dort nicht. Der eine Aufruf sagt, ob die Umwandlung geht.
                // Fehler von Graph (kein PDF möglich) sind hier kein Abbruch;
                // Anmeldung und Netz dagegen schon.
                let als_pdf = match self
                    .graph
                    .get_text_mit_kopf(&inhalts_pfad, &[("format", "pdf".to_string())], &[])
                    .await
                {
                    Ok(pdf) => pdf.starts_with("%PDF"),
                    Err(GraphFehler::Graph { .. }) => false,
                    Err(anderes) => return Err(anderes.into()),
                };
                antwort["als_pdf"] = json!(als_pdf);
                antwort["hinweis"] = json!(
                    "Office-Datei: Microsoft Graph gibt sie höchstens als PDF aus, nicht als Text. Bitte die Datei öffnen (Link)."
                );
            }
            Leseart::Pdf => {
                antwort["hinweis"] = json!(
                    "PDF-Datei: nur Metadaten und Link, kein Text (Microsoft Graph liefert PDF nur als Binärinhalt)."
                );
            }
            Leseart::Binaer => {
                antwort["hinweis"] = json!(
                    "Binärdatei: nur Metadaten und Link, kein Text. Endung unbekannt oder nicht textuell."
                );
            }
        }
        als_json(&antwort).map_err(ToolFehler::from)
    }

    async fn datei_hochladen_lauf(
        &self,
        anfrage: DateiHochladenAnfrage,
    ) -> Result<String, ToolFehler> {
        // Zuerst die Grenze: über 4 MB gibt es keine Anfrage an Graph.
        if anfrage.inhalt.len() > HOECHSTENS_UPLOAD {
            return Err(ungueltig_tool(format!(
                "inhalt ist {} Bytes groß – einfacher Upload geht bis 4 MB ({} Bytes). Für größere Dateien eine Upload-Session nötig; es wurde nichts hochgeladen.",
                anfrage.inhalt.len(),
                HOECHSTENS_UPLOAD
            )));
        }
        let name = anfrage.name.trim();
        if name.is_empty() {
            return Err(ungueltig_tool("name fehlt"));
        }
        if name.contains('/') || name.contains('\\') {
            return Err(ungueltig_tool(
                "name ist ein Dateiname, kein Pfad – den Ordner bitte über `ordner` nennen",
            ));
        }
        let verhalten = verhalten_pruefen(anfrage.verhalten.as_deref())?;
        let ort = self
            .ort_finden(anfrage.site.as_deref(), anfrage.bibliothek.as_deref())
            .await?;

        let hochgeladen: LaufwerkEintrag = self
            .graph
            .put_text(
                &format!(
                    "{}:/content",
                    ziel_pfad(&ort, anfrage.ordner.as_deref(), name)
                ),
                &[("@microsoft.graph.conflictBehavior", verhalten.to_string())],
                &anfrage.inhalt,
                mime_typ(name),
            )
            .await?;

        let mut antwort = eintrag_json(&hochgeladen, &self.zeitzone);
        antwort["ort"] = json!(ort.name());
        antwort["verhalten"] = json!(verhalten);
        als_json(&antwort).map_err(ToolFehler::from)
    }

    async fn ordner_anlegen_lauf(
        &self,
        anfrage: OrdnerAnlegenAnfrage,
    ) -> Result<String, ToolFehler> {
        let name = anfrage.name.trim();
        if name.is_empty() {
            return Err(ungueltig_tool("name fehlt"));
        }
        let ort = self
            .ort_finden(anfrage.site.as_deref(), anfrage.bibliothek.as_deref())
            .await?;
        let angelegt: LaufwerkEintrag = self
            .graph
            .post(
                &kinder_pfad(&ort, anfrage.ordner.as_deref()),
                &json!({
                    "name": name,
                    "folder": {},
                    "@microsoft.graph.conflictBehavior": "fail",
                }),
            )
            .await?;
        let mut antwort = eintrag_json(&angelegt, &self.zeitzone);
        antwort["ort"] = json!(ort.name());
        als_json(&antwort).map_err(ToolFehler::from)
    }

    async fn sharepoint_listen_lauf(
        &self,
        anfrage: SharepointListenAnfrage,
    ) -> Result<String, ToolFehler> {
        let site = self.site_finden(&anfrage.site).await?;
        let listen = self
            .graph
            .get_seiten::<Liste>(
                &format!("/sites/{}/lists", kodieren(&site.id)),
                &[(
                    "$select",
                    "id,name,displayName,description,webUrl,list".to_string(),
                )],
                HOECHSTENS_LISTE,
            )
            .await?;
        let liste: Vec<Value> = listen.iter().map(liste_json).collect();
        als_json(&json!({"site": site.text(), "listen": liste})).map_err(ToolFehler::from)
    }

    async fn sharepoint_listeneintraege_lauf(
        &self,
        anfrage: SharepointListeneintraegeAnfrage,
    ) -> Result<String, ToolFehler> {
        let site = self.site_finden(&anfrage.site).await?;
        let liste = self.liste_finden(&site, &anfrage.liste).await?;
        let anzahl = anzahl_mit(anfrage.anzahl, 20, HOECHSTENS_EINTRAEGE);

        let mut query: Vec<(&str, String)> = vec![
            ("$expand", "fields".to_string()),
            ("$top", anzahl.to_string()),
        ];
        let filter = anfrage
            .filter
            .as_deref()
            .map(str::trim)
            .filter(|filter| !filter.is_empty());
        if let Some(filter) = filter {
            query.push(("$filter", filter.to_string()));
        }

        let eintraege = self
            .graph
            .get_seiten::<Listeneintrag>(
                &format!(
                    "/sites/{}/lists/{}/items",
                    kodieren(&site.id),
                    kodieren(&liste.id)
                ),
                &query,
                anzahl as usize,
            )
            .await?;
        let werte: Vec<Value> = eintraege
            .iter()
            .map(|eintrag| listeneintrag_json(eintrag, &self.zeitzone))
            .collect();
        als_json(&json!({"liste": liste.text(), "eintraege": werte})).map_err(ToolFehler::from)
    }

    async fn sharepoint_listeneintrag_anlegen_lauf(
        &self,
        anfrage: SharepointListeneintragAnlegenAnfrage,
    ) -> Result<String, ToolFehler> {
        let felder = felder_pruefen(&anfrage.felder)?;
        let site = self.site_finden(&anfrage.site).await?;
        let liste = self.liste_finden(&site, &anfrage.liste).await?;
        let eintrag: Listeneintrag = self
            .graph
            .post(
                &format!(
                    "/sites/{}/lists/{}/items",
                    kodieren(&site.id),
                    kodieren(&liste.id)
                ),
                &json!({"fields": felder}),
            )
            .await?;
        let mut antwort = listeneintrag_json(&eintrag, &self.zeitzone);
        antwort["liste"] = json!(liste.text());
        antwort["angelegt"] = json!(true);
        als_json(&antwort).map_err(ToolFehler::from)
    }

    async fn sharepoint_listeneintrag_aendern_lauf(
        &self,
        anfrage: SharepointListeneintragAendernAnfrage,
    ) -> Result<String, ToolFehler> {
        let (felder, ignoriert) = schreibbare_felder(&felder_pruefen(&anfrage.felder)?);
        if felder.as_object().is_some_and(|karte| karte.is_empty()) {
            return Err(ungueltig_tool(format!(
                "nur schreibgeschützte Spalten genannt ({}) – SharePoint führt sie selbst, es wurde nichts geändert.",
                ignoriert.join(", ")
            )));
        }
        let eintrag = anfrage.eintrag.trim();
        if eintrag.is_empty() {
            return Err(ungueltig_tool("eintrag fehlt"));
        }
        let site = self.site_finden(&anfrage.site).await?;
        let liste = self.liste_finden(&site, &anfrage.liste).await?;
        let felder_pfad = format!(
            "/sites/{}/lists/{}/items/{}/fields",
            kodieren(&site.id),
            kodieren(&liste.id),
            kodieren(eintrag)
        );
        // Graph antwortet mit 204 und leerem Rumpf (Aufgabe 09): Erfolg. Den
        // Stand danach holt der GET auf dieselben Felder.
        self.graph
            .patch_ohne_antwort(&felder_pfad, &felder, &[])
            .await
            .map_err(liste_fehler)?;
        let stand: Value = self
            .graph
            .get(&felder_pfad, &[])
            .await
            .map_err(liste_fehler)?;

        let mut antwort = json!({
            "liste": liste.text(),
            "eintrag": eintrag,
            "geaendert": true,
            "felder": stand,
        });
        if !ignoriert.is_empty() {
            antwort["ignoriert"] = json!(ignoriert);
            antwort["hinweis"] = json!(
                "Schreibgeschützte Spalten wurden vor dem Senden herausgefiltert (SharePoint führt sie selbst)."
            );
        }
        als_json(&antwort).map_err(ToolFehler::from)
    }

    async fn datei_loeschen_lauf(
        &self,
        anfrage: DateiLoeschenAnfrage,
    ) -> Result<String, ToolFehler> {
        let datei = anfrage.datei.trim();
        if datei.is_empty() {
            return Err(ungueltig_tool("datei fehlt"));
        }
        let ort = self
            .ort_finden(anfrage.site.as_deref(), anfrage.bibliothek.as_deref())
            .await?;
        let pfad = eintrag_pfad(&ort, datei);
        // Erst die Metadaten: der Name gehört in die Meldung, und eine falsche Id
        // soll keinen DELETE auslösen.
        let eintrag: LaufwerkEintrag = self
            .graph
            .get(&pfad, &[("$select", EINTRAG_FELDER.to_string())])
            .await?;
        // `DELETE …/items/{id}` legt in den Papierkorb – für Dateien und Ordner.
        let loesch_pfad = format!("{}/items/{}", ort.basis(), kodieren(&eintrag.id));
        self.graph.delete(&loesch_pfad, &[]).await?;
        als_json(&json!({
            "geloescht": true,
            "name": eintrag.name,
            "id": eintrag.id,
            "art": if eintrag.ist_ordner() { "ordner" } else { "datei" },
            "ort": ort.name(),
            "papierkorb": true,
        }))
        .map_err(ToolFehler::from)
    }

    async fn sharepoint_listeneintrag_loeschen_lauf(
        &self,
        anfrage: SharepointListeneintragLoeschenAnfrage,
    ) -> Result<String, ToolFehler> {
        let eintrag = anfrage.eintrag.trim();
        if eintrag.is_empty() {
            return Err(ungueltig_tool("eintrag fehlt"));
        }
        let site = self.site_finden(&anfrage.site).await?;
        let liste = self.liste_finden(&site, &anfrage.liste).await?;
        let basis = format!(
            "/sites/{}/lists/{}/items/{}",
            kodieren(&site.id),
            kodieren(&liste.id),
            kodieren(eintrag)
        );
        // Erst lesen: Titel und Link gehören in die Meldung, und ein 404 soll
        // keinen DELETE auslösen.
        let aktuell: Listeneintrag = self
            .graph
            .get(&basis, &[("$expand", "fields".to_string())])
            .await
            .map_err(liste_fehler)?;
        self.graph.delete(&basis, &[]).await.map_err(liste_fehler)?;
        als_json(&json!({
            "geloescht": true,
            "liste": liste.text(),
            "eintrag": eintrag,
            "titel": listeneintrag_titel(&aktuell),
            "link": aktuell.web_url,
        }))
        .map_err(ToolFehler::from)
    }
}

/// Der Titel eines Listeneintrags: `Title`, `Titel` oder `LinkTitle` aus den
/// Spalten – sonst nichts.
fn listeneintrag_titel(eintrag: &Listeneintrag) -> Option<String> {
    let felder = eintrag.fields.as_ref()?.as_object()?;
    ["Title", "Titel", "LinkTitle"]
        .iter()
        .find_map(|name| felder.get(*name).and_then(Value::as_str))
        .map(str::to_string)
}

/// Die Leseart als Wort für die Antwort.
trait LeseartText {
    fn bezeichnung(self) -> &'static str;
}

impl LeseartText for Leseart {
    fn bezeichnung(self) -> &'static str {
        match self {
            Leseart::Text => "text",
            Leseart::Office => "office",
            Leseart::Pdf => "pdf",
            Leseart::Binaer => "binaer",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pfade_werden_kodiert_und_richtig_gebaut() {
        let one = Ort::OneDrive;
        assert_eq!(one.basis(), "/me/drive");
        assert_eq!(kinder_pfad(&one, None), "/me/drive/root/children");
        assert_eq!(
            kinder_pfad(&one, Some(" Projekt Nord ")),
            "/me/drive/root:/Projekt%20Nord:/children"
        );
        assert_eq!(
            kinder_pfad(&one, Some("01ABCDEFGHIJKLMNOPQRSTUVWXYZ234567")),
            "/me/drive/items/01ABCDEFGHIJKLMNOPQRSTUVWXYZ234567/children"
        );
        assert_eq!(
            eintrag_pfad(&one, "Projekte/Nord/Notiz.md"),
            "/me/drive/root:/Projekte/Nord/Notiz.md"
        );
        assert_eq!(
            ziel_pfad(&one, Some("Projekte"), "Notiz.md"),
            "/me/drive/root:/Projekte/Notiz.md"
        );

        let bib = Ort::Bibliothek(Anzeige::neu("drive-doku", Some("Dokumente")));
        assert_eq!(bib.basis(), "/drives/drive-doku");
        assert_eq!(bib.name(), "Dokumente");
        assert_eq!(
            ziel_pfad(&bib, Some("01ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"), "Notiz.md"),
            "/drives/drive-doku/items/01ABCDEFGHIJKLMNOPQRSTUVWXYZ234567:/Notiz.md"
        );
    }

    #[test]
    fn felder_muessen_ein_objekt_sein() {
        assert!(felder_pruefen(&json!({"Titel": "Erster"})).is_ok());
        assert!(felder_pruefen(&json!({})).is_err());
        assert!(felder_pruefen(&json!("Titel")).is_err());
        assert!(felder_pruefen(&Value::Null).is_err());
    }

    #[test]
    fn verhalten_kennt_drei_werte() {
        assert_eq!(verhalten_pruefen(None).expect("Standard"), "rename");
        assert_eq!(
            verhalten_pruefen(Some(" rename ")).expect("rename"),
            "rename"
        );
        assert_eq!(
            verhalten_pruefen(Some("replace")).expect("replace"),
            "replace"
        );
        assert_eq!(verhalten_pruefen(Some("fail")).expect("fail"), "fail");
        assert!(verhalten_pruefen(Some("überschreiben")).is_err());
    }

    #[test]
    fn anzahl_wird_gedeckelt() {
        assert_eq!(anzahl_mit(None, 20, 100), 20);
        assert_eq!(anzahl_mit(Some(0), 20, 100), 1);
        assert_eq!(anzahl_mit(Some(500), 20, 100), 100);
    }
}
