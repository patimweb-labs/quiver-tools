// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der MCP-Server: sechs Werkzeuge über stdio (Aufgabe 05).
//!
//! Die Verbindungen kommen aus `QUIVER_ZUGAENGE` (mehrere je Tool). Fehlt die
//! Liste, antwortet jedes Werkzeug mit dem Hinweis auf das quiver-Fenster –
//! **kein** MCP-Fehler. Jedes Werkzeug nimmt `verbindung`; bei genau einer
//! Verbindung darf der Name fehlen.

use std::any::Any;
use std::time::Duration;

use quiver_tools_kern::schutz::{hinweis_gesperrt, ist_lesend, TITEL_SCHREIBEN};
use quiver_tools_kern::{antwort, hinweis_zugang, OhneProfil, Zugang};
use rmcp::handler::server::tool::schema_for_type;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, JsonObject, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool, ToolAnnotations,
};
use rmcp::schemars::JsonSchema;
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;

use crate::anmeldung::{self, Bereit, SshFehler};
use crate::format::{self, Zustand};
use crate::hostschluessel;
use crate::pfade::Pfade;
use crate::schluessel;
use crate::verbindung::{SammelFehler, Sammlung, Verbindung};
use crate::werkzeug::{MEHRFACH_EINZAHL, MEHRFACH_TITEL, TITEL};

/// Zeitlimit eines Befehls, wenn keines angegeben ist.
const ZEITLIMIT_STANDARD: u32 = 60;
/// Größtes erlaubtes Zeitlimit.
const ZEITLIMIT_MAX: u32 = 600;
/// Wie viele Zeichen je Ausgabestrom zurückkommen.
const MAX_ZEICHEN_STANDARD: u32 = 8000;

/// Was ein Werkzeug zurückgibt.
type Ergebnis = Result<CallToolResult, McpError>;

/// Der SSH-Server: die Verbindungen und die Werkzeugliste.
pub struct SshServer {
    /// Der Zugang des Kerns; bei `mehrfach` ohne Felder, aber mit Titel.
    zugang: Zugang,
    pfade: Pfade,
    verbindungen: Result<Sammlung, SammelFehler>,
}

impl SshServer {
    /// Der Normalfall: alles aus der Umgebung (`QUIVER_ZUGAENGE`,
    /// `QUIVER_SSH_DATEN`, `HOME`).
    pub fn neu(zugang: Zugang) -> Self {
        SshServer {
            zugang,
            pfade: Pfade::aus_umgebung(),
            verbindungen: Sammlung::aus_umgebung(),
        }
    }

    /// Feste Teile (Tests und Sonderfälle), ohne Prozessumgebung.
    pub fn mit(zugang: Zugang, pfade: Pfade, verbindungen: Result<Sammlung, SammelFehler>) -> Self {
        SshServer {
            zugang,
            pfade,
            verbindungen,
        }
    }

    /// Alle Werkzeuge, wie `tools/list` sie meldet.
    pub fn werkzeuge(&self) -> Vec<Tool> {
        // Hoch- und Herunterladen gibt es nur mit einem Austauschordner: ohne
        // ihn wäre jedes lokale Verzeichnis erreichbar (Härtung Paket 4).
        let austausch = self
            .verbindungen
            .as_ref()
            .map(|sammlung| {
                sammlung
                    .alle()
                    .iter()
                    .any(|verbindung| verbindung.austauschordner.is_some())
            })
            .unwrap_or(false);
        let mut werkzeuge = vec![
            lese_werkzeug::<StatusParam>(
                "ssh_status",
                "Verbindungen",
                "Listet die im quiver-Fenster eingetragenen SSH-Verbindungen: Name, benutzer@host:port, \
                 ob ein Schlüssel eingerichtet ist (mit Fingerabdruck), ob ein Passwort für sudo \
                 hinterlegt ist und ob der Host-Schlüssel schon bekannt ist. Öffnet keine Verbindung.",
            ),
            lese_werkzeug::<ListenParam>(
                "ssh_list",
                "Verzeichnis lesen",
                "Listet ein entferntes Verzeichnis über SFTP: Rechte, Größe, Name; Ordner mit / am Ende. \
                 Besser als `ls` über ssh_exec, weil das Ergebnis schon sortiert und ohne Rauschen ist.",
            ),
            schreib_werkzeug::<ExecParam>(
                "ssh_exec",
                "Befehl ausführen",
                "Führt einen Befehl auf der entfernten Maschine aus und gibt stdout, stderr und den \
                 Exit-Code zurück. Mit `sudo: true` läuft er als `sudo -S -p '' -- sh -c '<befehl>'`; das \
                 Passwort kommt aus dem quiver-Fenster und wird nur an sudo übergeben, nie auf der \
                 Kommandozeile. Befehle laufen ohne Terminal – Programme, die eine Eingabe erwarten, \
                 warten bis zum Zeitlimit.",
            ),
            schreib_werkzeug::<UploadParam>(
                "ssh_upload",
                "Datei hochladen",
                "Kopiert eine Datei aus dem Austauschordner über SFTP auf die entfernte Maschine. \
                 `lokal` ist relativ zum Austauschordner aus dem quiver-Fenster und verlässt ihn nie; \
                 `entfernt` enthält den Dateinamen, eine vorhandene Datei wird dort überschrieben.",
            ),
            schreib_werkzeug::<DownloadParam>(
                "ssh_download",
                "Datei herunterladen",
                "Kopiert eine Datei über SFTP von der entfernten Maschine in den Austauschordner. \
                 `lokal` ist relativ zum Austauschordner; vorhandene Dateien werden nie überschrieben. \
                 Für Binärdateien besser als `cat` über ssh_exec.",
            ),
            schreib_werkzeug::<EinrichtenParam>(
                "ssh_schluessel_einrichten",
                "Schlüssel einrichten",
                "Richtet die Schlüssel-Anmeldung neu ein: meldet sich mit dem Passwort an, \
                 erzeugt einen ed25519-Schlüssel, trägt ihn in ~/.ssh/authorized_keys ein, prüft eine \
                 neue Verbindung nur mit dem Schlüssel und ersetzt erst dann den alten Schlüssel. \
                 Nötig nach einem Server-Neuaufbau, wenn die normale Anmeldung den Schlüssel ablehnt.",
            ),
        ];
        if !austausch {
            werkzeuge.retain(|werkzeug| {
                !matches!(werkzeug.name.as_ref(), "ssh_upload" | "ssh_download")
            });
        }
        werkzeuge
    }

    /// Ein Werkzeug aufrufen. Auch der Weg, den die Tests nehmen.
    ///
    /// Die Schreibsperre steht **vor** dem Verteiler und gilt je Verbindung:
    /// ohne den Schalter „Schreiben erlauben“ geht kein Befehl an den Host.
    pub async fn aufrufen(&self, name: &str, argumente: Option<JsonObject>) -> Ergebnis {
        let werkzeuge = self.werkzeuge();
        if !ist_lesend(&werkzeuge, name) {
            let gewaehlt = argumente
                .as_ref()
                .and_then(|karte| karte.get("verbindung"))
                .and_then(|wert| wert.as_str());
            if let Ok(verbindung) = self.waehlen(gewaehlt) {
                if !verbindung.schreiben_erlaubt() {
                    return Ok(antwort(format!(
                        "{}\n\n(Verbindung „{}“)",
                        hinweis_gesperrt(name, TITEL, TITEL_SCHREIBEN),
                        verbindung.name
                    )));
                }
            }
        }
        match name {
            "ssh_status" => self.status(argumente),
            "ssh_list" => self.liste(argumente).await,
            "ssh_exec" => self.exec(argumente).await,
            "ssh_upload" => self.hochladen(argumente).await,
            "ssh_download" => self.herunterladen(argumente).await,
            "ssh_schluessel_einrichten" => self.schluessel_einrichten(argumente).await,
            sonst => Err(McpError::invalid_params(
                format!("Unbekanntes Werkzeug „{sonst}“."),
                None,
            )),
        }
    }

    /// Die Liste der Verbindungen oder ein Fehler ohne ihren Inhalt.
    fn sammlung(&self) -> Result<&Sammlung, McpError> {
        self.verbindungen.as_ref().map_err(|fehler| {
            McpError::invalid_params(
                format!("{fehler} – bitte im quiver-Fenster unter Tools → {TITEL} prüfen."),
                None,
            )
        })
    }

    /// Die gewünschte Verbindung; fehlt die Liste, kommt der Hinweis.
    fn waehlen(&self, name: Option<&str>) -> Result<Verbindung, OhneProfil> {
        let sammlung = self.sammlung().map_err(OhneProfil::Fehler)?;
        if sammlung.ist_leer() {
            return Err(OhneProfil::Hinweis(antwort(hinweis_zugang(TITEL))));
        }
        sammlung.waehlen(name).cloned().map_err(|fehler| {
            OhneProfil::Fehler(McpError::invalid_params(fehler.to_string(), None))
        })
    }

    /// Verbindung nach den Regeln der Aufgabe 05 herstellen.
    async fn verbinden(&self, verbindung: &Verbindung) -> Result<Bereit, OhneProfil> {
        anmeldung::bereit(verbindung, &self.pfade)
            .await
            .map_err(als_abbruch)
    }

    // ------------------------------------------------------------- lesend

    fn status(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: StatusParam = argumente(roh)?;
        let sammlung = self.sammlung()?;
        if sammlung.ist_leer() {
            return Ok(antwort(hinweis_zugang(TITEL)));
        }

        let gewaehlt: Vec<&Verbindung> = match p.verbindung.as_deref() {
            Some(name) => vec![sammlung
                .waehlen(Some(name))
                .map_err(|fehler| McpError::invalid_params(fehler.to_string(), None))?],
            None => sammlung.alle().iter().collect(),
        };

        let zeilen = gewaehlt
            .into_iter()
            .map(|verbindung| {
                let zustand = Zustand {
                    schluessel: self.schluessel_zustand(verbindung),
                    host_schluessel: hostschluessel::bekannt(
                        &self.pfade,
                        &verbindung.host,
                        verbindung.port,
                    ),
                    sudo: verbindung.sudo_passwort().is_some(),
                };
                (verbindung.name.clone(), verbindung.ziel(), zustand)
            })
            .collect::<Vec<_>>();

        Ok(antwort(format::status(&zeilen)))
    }

    /// Fingerabdruck des eingerichteten Schlüssels, falls es einen gibt.
    fn schluessel_zustand(&self, verbindung: &Verbindung) -> Option<String> {
        let datei = self.pfade.schluessel(&verbindung.name).ok()?;
        if !datei.exists() {
            return None;
        }
        Some(
            schluessel::laden(&datei)
                .map(|schluessel| schluessel::fingerprint(schluessel.public_key()))
                .unwrap_or_else(|_| "Datei unlesbar".to_string()),
        )
    }

    async fn liste(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: ListenParam = argumente(roh)?;
        let verbindung = match self.waehlen(p.verbindung.as_deref()) {
            Ok(verbindung) => verbindung,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let pfad = p
            .pfad
            .as_deref()
            .map(str::trim)
            .filter(|pfad| !pfad.is_empty())
            .unwrap_or(".")
            .to_string();

        let bereit = match self.verbinden(&verbindung).await {
            Ok(bereit) => bereit,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let eintraege = bereit
            .sitzung
            .liste(&pfad)
            .await
            .map_err(|fehler| fern_fehler(&verbindung.name, fehler))?;
        bereit.sitzung.beenden().await;
        Ok(antwort(format::mit_hinweis(
            format::verzeichnis(&verbindung.name, &pfad, &eintraege),
            bereit.hinweis,
        )))
    }

    // ---------------------------------------------------------- verändernd

    async fn exec(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: ExecParam = argumente(roh)?;
        if p.befehl.trim().is_empty() {
            return Err(McpError::invalid_params(
                "ssh_exec braucht einen `befehl`.".to_string(),
                None,
            ));
        }
        let verbindung = match self.waehlen(p.verbindung.as_deref()) {
            Ok(verbindung) => verbindung,
            Err(ohne) => return ohne.in_ergebnis(),
        };

        let sudo = p.sudo.unwrap_or(false);
        let sudo_passwort = if sudo {
            match verbindung.sudo_passwort() {
                Some(passwort) => Some(passwort),
                None => {
                    return Ok(antwort(format!(
                        "Für {} ist kein Passwort hinterlegt – Befehle mit sudo brauchen eins. Bitte im quiver-Fenster unter Tools → SSH das Feld „Passwort“ ausfüllen.",
                        verbindung.name
                    )))
                }
            }
        } else {
            None
        };

        let zeitlimit = p
            .zeitlimit
            .unwrap_or(ZEITLIMIT_STANDARD)
            .clamp(1, ZEITLIMIT_MAX);
        let max_zeichen = p.max_zeichen.unwrap_or(MAX_ZEICHEN_STANDARD) as usize;

        let bereit = match self.verbinden(&verbindung).await {
            Ok(bereit) => bereit,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let ergebnis = bereit
            .sitzung
            .exec(
                p.befehl.trim(),
                sudo_passwort,
                Duration::from_secs(u64::from(zeitlimit)),
            )
            .await
            .map_err(|fehler| fern_fehler(&verbindung.name, fehler))?;
        bereit.sitzung.beenden().await;

        let text = format::ausfuehrung(&verbindung.name, &ergebnis, sudo, max_zeichen);
        let text = format::mit_hinweis(text, bereit.hinweis);

        if sudo && ergebnis.status != 0 && sudo_abgelehnt(&ergebnis.stderr) {
            return Ok(antwort(format!(
                "Das Passwort für sudo wurde bei {} abgelehnt – bitte im quiver-Fenster unter Tools → SSH das Feld „Passwort“ prüfen.\n\n{text}",
                verbindung.name
            )));
        }
        Ok(antwort(text))
    }

    async fn hochladen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: UploadParam = argumente(roh)?;
        let verbindung = match self.waehlen(p.verbindung.as_deref()) {
            Ok(verbindung) => verbindung,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let lokal = pflicht(&p.lokal, "lokal")?;
        let entfernt = pflicht(&p.entfernt, "entfernt")?;
        let ablage = match verbindung.ablage() {
            Ok(ablage) => ablage,
            Err(fehler) => return Err(McpError::invalid_params(fehler.to_string(), None)),
        };
        let quelle = match ablage.pfad(lokal) {
            Ok(pfad) => pfad,
            Err(fehler) => return Err(McpError::invalid_params(fehler.to_string(), None)),
        };
        if !quelle.is_file() {
            return Err(McpError::invalid_params(
                format!("„{lokal}“ gibt es im Austauschordner nicht."),
                None,
            ));
        }
        entfernt_pruefen(entfernt)?;

        let bereit = match self.verbinden(&verbindung).await {
            Ok(bereit) => bereit,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let bytes = bereit
            .sitzung
            .hochladen(&quelle, entfernt)
            .await
            .map_err(|fehler| fern_fehler(&verbindung.name, fehler))?;
        bereit.sitzung.beenden().await;

        Ok(antwort(format::mit_hinweis(
            format!(
                "{lokal} nach {entfernt} auf {} hochgeladen ({}).",
                verbindung.name,
                format::groesse(bytes)
            ),
            bereit.hinweis,
        )))
    }

    async fn herunterladen(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: DownloadParam = argumente(roh)?;
        let verbindung = match self.waehlen(p.verbindung.as_deref()) {
            Ok(verbindung) => verbindung,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let entfernt = pflicht(&p.entfernt, "entfernt")?;
        let lokal = pflicht(&p.lokal, "lokal")?;
        let ablage = match verbindung.ablage() {
            Ok(ablage) => ablage,
            Err(fehler) => return Err(McpError::invalid_params(fehler.to_string(), None)),
        };
        entfernt_pruefen(entfernt)?;
        // Vor der Verbindung prüfen: ein vorhandenes Ziel bleibt unberührt.
        if let Ok(ziel) = ablage.pfad(lokal) {
            if ziel.exists() {
                return Err(McpError::invalid_params(
                    format!(
                        "„{lokal}“ gibt es im Austauschordner schon – es wird nicht überschrieben."
                    ),
                    None,
                ));
            }
        }

        let bereit = match self.verbinden(&verbindung).await {
            Ok(bereit) => bereit,
            Err(ohne) => return ohne.in_ergebnis(),
        };
        let inhalt = bereit
            .sitzung
            .herunterladen_bytes(entfernt)
            .await
            .map_err(|fehler| fern_fehler(&verbindung.name, fehler))?;
        bereit.sitzung.beenden().await;
        let bytes = inhalt.len() as u64;
        let ziel = ablage
            .schreiben_bytes_neu(lokal, &inhalt)
            .map_err(|fehler| McpError::invalid_params(fehler.to_string(), None))?;
        let lokal = ziel.display().to_string();

        Ok(antwort(format::mit_hinweis(
            format!(
                "{entfernt} von {} nach {lokal} heruntergeladen ({}).",
                verbindung.name,
                format::groesse(bytes)
            ),
            bereit.hinweis,
        )))
    }

    async fn schluessel_einrichten(&self, roh: Option<JsonObject>) -> Ergebnis {
        let p: EinrichtenParam = argumente(roh)?;
        let verbindung = match self.waehlen(p.verbindung.as_deref()) {
            Ok(verbindung) => verbindung,
            Err(ohne) => return ohne.in_ergebnis(),
        };

        let bereit = match anmeldung::schluessel_einrichten(&verbindung, &self.pfade).await {
            Ok(bereit) => bereit,
            Err(fehler) => return als_abbruch(fehler).in_ergebnis(),
        };
        bereit.sitzung.beenden().await;

        let datei = self
            .pfade
            .schluessel(&verbindung.name)
            .map_err(|fehler| McpError::internal_error(fehler.to_string(), None))?;
        let mut text = bereit.hinweis.unwrap_or_default();
        text.push_str(&format!("\nPrivater Schlüssel: {}", datei.display()));
        text.push_str(&format!(
            "\nÖffentlicher Schlüssel: {}",
            crate::pfade::oeffentlich(&datei).display()
        ));
        Ok(antwort(text))
    }
}

// ---------------------------------------------------------------- Helfer

/// Ein Werkzeug, das nichts ändert.
fn lese_werkzeug<P: JsonSchema + Any>(
    name: &'static str,
    titel: &'static str,
    beschreibung: &'static str,
) -> Tool {
    werkzeug::<P>(name, titel, beschreibung, Some(true), Some(false))
}

/// Ein Werkzeug, das etwas ändert (`destructive_hint`, AGENTS.md §8).
fn schreib_werkzeug<P: JsonSchema + Any>(
    name: &'static str,
    titel: &'static str,
    beschreibung: &'static str,
) -> Tool {
    werkzeug::<P>(name, titel, beschreibung, Some(false), Some(true))
}

fn werkzeug<P: JsonSchema + Any>(
    name: &'static str,
    titel: &'static str,
    beschreibung: &'static str,
    nur_lesen: Option<bool>,
    zerstoerend: Option<bool>,
) -> Tool {
    Tool::new(name, beschreibung, schema_for_type::<P>())
        .with_title(titel)
        .with_annotations(ToolAnnotations::from_raw(
            Some(titel.to_string()),
            nur_lesen,
            zerstoerend,
            nur_lesen,
            Some(false),
        ))
}

/// Die Argumente eines Aufrufs als typisierte Parameter.
fn argumente<T: DeserializeOwned>(roh: Option<JsonObject>) -> Result<T, McpError> {
    let wert = Value::Object(roh.unwrap_or_default());
    serde_json::from_value(wert).map_err(|fehler| {
        McpError::invalid_params(format!("Die Parameter passen nicht: {fehler}"), None)
    })
}

/// Ein Pflichttext, der nicht nur aus Leerraum besteht.
fn pflicht<'a>(wert: &'a str, name: &str) -> Result<&'a str, McpError> {
    let wert = wert.trim();
    if wert.is_empty() {
        return Err(McpError::invalid_params(
            format!("`{name}` darf nicht leer sein."),
            None,
        ));
    }
    Ok(wert)
}

/// Der Pfad auf der entfernten Maschine: frei wählbar (dort gelten die Rechte
/// des angemeldeten Benutzers), aber nie leer und ohne Steuerzeichen.
fn entfernt_pruefen(pfad: &str) -> Result<(), McpError> {
    if pfad.trim().is_empty() || pfad.chars().any(char::is_control) {
        return Err(McpError::invalid_params(
            "Bitte den Pfad auf der entfernten Maschine angeben (ohne Steuerzeichen).".to_string(),
            None,
        ));
    }
    Ok(())
}

/// Ein Fehler beim Arbeiten auf der entfernten Maschine.
fn fern_fehler(name: &str, fehler: anyhow::Error) -> McpError {
    McpError::internal_error(format!("{name}: {fehler}"), None)
}

/// Aus einem [`SshFehler`] wird ein Hinweis (fehlendes Passwort) oder ein Fehler.
fn als_abbruch(fehler: SshFehler) -> OhneProfil {
    match fehler {
        SshFehler::PasswortFehlt { .. } => OhneProfil::Hinweis(antwort(fehler.to_string())),
        anderer => OhneProfil::Fehler(McpError::internal_error(anderer.to_string(), None)),
    }
}

/// Sieht die sudo-Ausgabe nach einem falschen Passwort aus?
fn sudo_abgelehnt(stderr: &str) -> bool {
    let klein = stderr.to_ascii_lowercase();
    klein.contains("sorry, try again")
        || klein.contains("incorrect password")
        || klein.contains("a password is required")
        || klein.contains("no password was provided")
}

// ------------------------------------------------------------- Parameter

/// Parameter von `ssh_status`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct StatusParam {
    /// Nur diese Verbindung zeigen. Ohne Angabe: alle.
    #[serde(default)]
    pub verbindung: Option<String>,
}

/// Parameter von `ssh_list`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ListenParam {
    /// Entferntes Verzeichnis, z. B. /var/log oder . . Standard: das Anmeldeverzeichnis.
    #[serde(default)]
    pub pfad: Option<String>,
    /// Verbindung; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
}

/// Parameter von `ssh_exec`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ExecParam {
    /// Verbindung; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Die Befehlszeile, wie sie in `sh -c` läuft.
    pub befehl: String,
    /// Mit sudo ausführen (`sudo -S -p '' -- sh -c '<befehl>'`). Standard false.
    #[serde(default)]
    pub sudo: Option<bool>,
    /// Zeitlimit in Sekunden, 1–600. Standard 60.
    #[serde(default)]
    pub zeitlimit: Option<u32>,
    /// Höchstens so viele Zeichen je stdout/stderr. Standard 8000.
    #[serde(default)]
    pub max_zeichen: Option<u32>,
}

/// Parameter von `ssh_upload`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct UploadParam {
    /// Verbindung; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Datei auf diesem Gerät. Absolute Pfade sind am sichersten.
    pub lokal: String,
    /// Ziel auf der entfernten Maschine, mit Dateinamen.
    pub entfernt: String,
}

/// Parameter von `ssh_download`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct DownloadParam {
    /// Verbindung; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
    /// Datei auf der entfernten Maschine.
    pub entfernt: String,
    /// Ziel auf diesem Gerät, mit Dateinamen.
    pub lokal: String,
}

/// Parameter von `ssh_schluessel_einrichten`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct EinrichtenParam {
    /// Verbindung; bei genau einer darf der Name fehlen.
    #[serde(default)]
    pub verbindung: Option<String>,
}

// -------------------------------------------------------------- MCP-Anker

impl ServerHandler for SshServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(quiver_tools_kern::mcp::server_info(&self.zugang))
            .with_instructions(format!(
                "SSH/SFTP: Befehle ausführen, Verzeichnisse lesen, Dateien übertragen und die \
                 Schlüssel-Anmeldung einrichten. Die {MEHRFACH_TITEL} ({MEHRFACH_EINZAHL}en) legt der \
                 User im quiver-Fenster unter Tools → {TITEL} an; jedes Werkzeug nimmt `verbindung`, \
                 bei genau einer Verbindung darf der Name fehlen. Fehlt die Liste, sagen die \
                 Werkzeuge das als Text. quiver-ssh meldet sich nach der ersten Einrichtung nur noch \
                 mit einem Schlüssel an; das Passwort wird nur für die Einrichtung und für sudo \
                 gebraucht und steht in keiner Antwort."
            ))
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(self.werkzeuge()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        self.aufrufen(request.name.as_ref(), request.arguments)
            .await
            .map(CallToolResponse::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pfade::Pfade;
    use quiver_tools_kern::Zugang;
    use serde_json::json;

    use crate::Ssh;

    /// Ein Server ohne Verbindungen; der `TempDir` muss beim Aufrufer leben.
    fn ohne_verbindungen() -> (SshServer, tempfile::TempDir) {
        let temp = tempfile::tempdir().unwrap();
        let pfade = Pfade::neu(
            temp.path().join("quiver-tools/ssh"),
            Some(temp.path().to_path_buf()),
        );
        let server = SshServer::mit(
            Zugang::mit_umgebung(&Ssh, |_| None),
            pfade,
            Ok(Sammlung::default()),
        );
        (server, temp)
    }

    fn aufruf(server: &SshServer, name: &str, argumente: Value) -> Ergebnis {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let objekt = argumente.as_object().cloned().unwrap_or_default();
        runtime.block_on(server.aufrufen(name, Some(objekt)))
    }

    #[test]
    fn lesende_und_schreibende_werkzeuge_tragen_die_richtigen_hinweise() {
        let (server, _temp) = ohne_verbindungen();
        let werkzeuge = server.werkzeuge();
        // Ohne Austauschordner gibt es Hoch- und Herunterladen nicht.
        assert_eq!(werkzeuge.len(), 4);
        assert!(!werkzeuge.iter().any(|w| w.name == "ssh_upload"));
        assert!(!werkzeuge.iter().any(|w| w.name == "ssh_download"));

        let lesend = ["ssh_status", "ssh_list"];
        for werkzeug in &werkzeuge {
            let hinweise = werkzeug.annotations.as_ref().expect("Hinweise fehlen");
            if lesend.contains(&werkzeug.name.as_ref()) {
                assert_eq!(hinweise.read_only_hint, Some(true), "{}", werkzeug.name);
                assert_eq!(hinweise.destructive_hint, Some(false), "{}", werkzeug.name);
            } else {
                assert_eq!(hinweise.read_only_hint, Some(false), "{}", werkzeug.name);
                assert_eq!(hinweise.destructive_hint, Some(true), "{}", werkzeug.name);
            }
            // Jedes Werkzeug kennt `verbindung`.
            let eigenschaften = &werkzeug.input_schema["properties"];
            assert!(
                eigenschaften.get("verbindung").is_some(),
                "{} kennt kein `verbindung`: {eigenschaften:?}",
                werkzeug.name
            );
        }
    }

    #[test]
    fn mit_austauschordner_gibt_es_hoch_und_herunterladen() {
        let temp = tempfile::tempdir().unwrap();
        let austausch = tempfile::tempdir().unwrap();
        let pfade = Pfade::neu(temp.path().join("quiver-tools/ssh"), None);
        let liste = json!([{
            "name": "web-01",
            "werte": {
                "host": "127.0.0.1",
                "benutzer": "admin",
                "schreiben": "an",
                "austauschordner": austausch.path().to_str().unwrap()
            }
        }]);
        let sammlung =
            Sammlung::mit_umgebung(|name| (name == "QUIVER_ZUGAENGE").then(|| liste.to_string()));
        let server = SshServer::mit(Zugang::mit_umgebung(&Ssh, |_| None), pfade, sammlung);
        let namen: Vec<String> = server
            .werkzeuge()
            .iter()
            .map(|w| w.name.to_string())
            .collect();
        assert!(namen.contains(&"ssh_upload".to_string()), "{namen:?}");
        assert!(namen.contains(&"ssh_download".to_string()), "{namen:?}");
    }

    #[test]
    fn ohne_verbindungen_kommt_der_hinweis_und_kein_fehler() {
        let (server, _temp) = ohne_verbindungen();
        let ergebnis = aufruf(&server, "ssh_exec", json!({ "befehl": "id -un" })).unwrap();
        let text = ergebnis.content[0].as_text().unwrap().text.clone();
        assert!(text.contains("Zugang fehlt"), "{text}");
        assert!(text.contains("Tools → SSH"), "{text}");
    }

    #[test]
    fn status_ohne_verbindungen_ist_ein_hinweis() {
        let (server, _temp) = ohne_verbindungen();
        let ergebnis = aufruf(&server, "ssh_status", json!({})).unwrap();
        let text = ergebnis.content[0].as_text().unwrap().text.clone();
        assert!(text.contains("Zugang fehlt"), "{text}");
    }

    #[test]
    fn leere_pflichtparameter_und_unbekannte_werkzeuge_sind_parameterfehler() {
        let (server, _temp) = ohne_verbindungen();
        let fehler = aufruf(&server, "ssh_exec", json!({ "befehl": "  " })).unwrap_err();
        assert!(fehler.message.contains("befehl"), "{}", fehler.message);

        let fehler = aufruf(&server, "ssh_gibtsnicht", json!({})).unwrap_err();
        assert!(
            fehler.message.contains("ssh_gibtsnicht"),
            "{}",
            fehler.message
        );
    }

    #[test]
    fn falsche_parameter_werden_gemeldet() {
        let (server, _temp) = ohne_verbindungen();
        let fehler = aufruf(&server, "ssh_exec", json!({ "befehl": 42 })).unwrap_err();
        assert!(fehler.message.contains("Parameter"), "{}", fehler.message);
    }

    #[test]
    fn sudo_ausgabe_wird_erkannt() {
        assert!(sudo_abgelehnt("Sorry, try again."));
        assert!(sudo_abgelehnt("sudo: incorrect password attempt"));
        assert!(sudo_abgelehnt("sudo: a password is required"));
        assert!(!sudo_abgelehnt("mkdir: cannot create directory"));
    }
}
