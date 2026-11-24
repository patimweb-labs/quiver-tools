// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der einzige Weg zu Dateien.
//!
//! Ein [`Ablage`] hat **einen** Arbeitsordner (Feld `ordner` im quiver-Fenster,
//! absolut, `~` wird aufgelöst, wird angelegt). Alles darin ist relativ; ein
//! Pfad verlässt den Ordner nie. Die Regeln stammen aus Aufgabe 16:
//!
//! - **Abgelehnt** werden absolute Pfade, `..`, NUL, Backslash, `:`,
//!   Windows-Gerätenamen (`CON`, `NUL`, `COM1`, …), versteckte Namen (`.…`) und
//!   symbolische Links auf **jeder** Ebene – auch die Datei selbst.
//! - **Nur normale Dateien**, höchstens [`MAX_BYTES`] (10 MB) je Lese- oder
//!   Schreibvorgang.
//! - **Anlegen** nie über Vorhandenes (`create_new`).
//! - **Ändern** nur mit dem [`Stand`], den der Agent gelesen hat (SHA-256 des
//!   Inhalts). Stimmt er nicht, wird nichts geschrieben. Geschrieben wird
//!   atomar (temporär + umbenennen), mit genau einer Sicherung `<name>.bak` und
//!   unter einer Dateisperre, die auch über Prozesse gilt.
//! - **Löschen gibt es nicht.**
//!
//! ```no_run
//! use quiver_diagramm::ablage::Ablage;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let ablage = Ablage::neu("~/Diagramme", "excalidraw")?;
//! let inhalt = ablage.lesen("entwurf.excalidraw")?;
//! let neu = ablage.aendern("entwurf.excalidraw", &inhalt.text, &inhalt.stand)?;
//! println!("neuer Stand: {neu}");
//! # Ok(())
//! # }
//! ```

use fd_lock::RwLock;
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

/// Höchstgröße einer Datei, die gelesen oder geschrieben wird: 10 MB.
pub const MAX_BYTES: u64 = 10 * 1024 * 1024;

/// Name der Sperrdatei im Arbeitsordner. Versteckt, damit [`Ablage::liste`] sie
/// nicht als Diagramm zeigt.
const SPERRDATEI: &str = ".sperre";

/// So tief darf [`Ablage::liste`] in Unterordner steigen.
const MAX_TIEFE: usize = 16;

/// Zähler für temporäre Namen (prozessweit eindeutig).
static ZAEHLER: AtomicU64 = AtomicU64::new(0);

/// So lange sucht [`Ablage::suche`] höchstens.
pub const SUCHE_FRIST: std::time::Duration = std::time::Duration::from_secs(2);
/// So viele Treffer nennt [`Ablage::suche`] höchstens.
pub const SUCHE_MAX: usize = 2000;
/// Hinweis, wenn die Suche vorzeitig endete.
pub const SUCHE_HINWEIS: &str = "Die Suche wurde abgebrochen, die Liste ist unvollständig – bitte im quiver-Fenster einen kleineren Arbeitsordner wählen.";
/// Ordner voller Fremddateien, in denen keine Diagramme gesucht werden.
const UEBERGANGEN: &[&str] = &["node_modules", "target", "__pycache__", "venv", "Library"];

/// Hängt [`SUCHE_HINWEIS`] an, wenn die Suche nicht vollständig war.
pub fn mit_hinweis(text: String, suche: &Suche) -> String {
    if suche.vollstaendig {
        text
    } else {
        format!("{text}\n{SUCHE_HINWEIS}")
    }
}

/// Ergebnis von [`Ablage::suche`].
#[derive(Debug, Clone)]
pub struct Suche {
    /// Gefundene Dateien, nach Pfad sortiert.
    pub eintraege: Vec<Eintrag>,
    /// `false`: Frist oder Höchstzahl erreicht.
    pub vollstaendig: bool,
}

/// Ein Arbeitsordner mit genau einer erlaubten Dateiendung.
///
/// Für Excalidraw ist das `.excalidraw`, für draw.io `.drawio`. Ausgaben
/// (`.svg`, `.png`) schreibt [`Ablage::ausgabe_schreiben`].
#[derive(Debug)]
pub struct Ablage {
    wurzel: PathBuf,
    endung: String,
}

/// Der SHA-256 des Dateiinhalts, den ein Agent gelesen hat.
///
/// Nur mit dem aktuellen Stand ändert [`Ablage::aendern`] eine Datei; so kann
/// kein Aufruf die Arbeit eines anderen überschreiben.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Stand(String);

impl Stand {
    /// Stand aus 64 Hexzeichen (Groß- oder Kleinschreibung) bauen.
    pub fn aus_hex(text: &str) -> Option<Stand> {
        let text = text.trim();
        if text.len() == 64 && text.chars().all(|z| z.is_ascii_hexdigit()) {
            Some(Stand(text.to_ascii_lowercase()))
        } else {
            None
        }
    }

    /// Der Stand als Hexzeichenkette.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Stand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Eine gelesene Datei samt Stand und Randdaten.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inhalt {
    /// Der Inhalt als Text (Diagrammdateien sind immer UTF-8).
    pub text: String,
    /// SHA-256 dieses Inhalts; gehört in jeden späteren Änderungsaufruf.
    pub stand: Stand,
    /// Größe in Bytes.
    pub groesse: u64,
    /// Änderungszeit, sofern das Dateisystem eine liefert.
    pub geaendert: Option<SystemTime>,
}

/// Ein Eintrag aus [`Ablage::liste`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Eintrag {
    /// Pfad relativ zum Arbeitsordner, mit `/` als Trenner.
    pub pfad: String,
    /// Größe in Bytes.
    pub groesse: u64,
    /// Änderungszeit, sofern das Dateisystem eine liefert.
    pub geaendert: Option<SystemTime>,
}

impl Ablage {
    /// Öffnet (und erzeugt) den Arbeitsordner.
    ///
    /// `ordner` kommt aus dem Feld `ordner`: Pflicht, absolut, `~` und `~/…`
    /// werden auf das Benutzerverzeichnis aufgelöst. `endung` ist die Endung der
    /// Diagrammdateien ohne Punkt (`excalidraw`, `drawio`).
    pub fn neu(ordner: &str, endung: &str) -> Result<Ablage, AblageFehler> {
        let ordner = ordner.trim();
        if ordner.is_empty() {
            return Err(AblageFehler::OrdnerFehlt);
        }
        let endung = endung.trim().trim_start_matches('.');
        if endung.is_empty() || !endung.chars().all(|z| z.is_ascii_alphanumeric()) {
            return Err(AblageFehler::EndungUngueltig(endung.to_string()));
        }
        let pfad = aufgeloest(ordner);
        if !pfad.is_absolute() {
            return Err(AblageFehler::OrdnerNichtAbsolut(pfad.display().to_string()));
        }
        fs::create_dir_all(&pfad).map_err(|fehler| AblageFehler::OrdnerAnlegen {
            pfad: pfad.display().to_string(),
            grund: fehler.to_string(),
        })?;
        let wurzel = fs::canonicalize(&pfad).map_err(|fehler| AblageFehler::OrdnerAnlegen {
            pfad: pfad.display().to_string(),
            grund: fehler.to_string(),
        })?;
        Ok(Ablage {
            wurzel,
            endung: endung.to_ascii_lowercase(),
        })
    }

    /// Der Arbeitsordner, so wie er auf der Platte heißt (aufgelöst).
    pub fn wurzel(&self) -> &Path {
        &self.wurzel
    }

    /// Die erlaubte Endung der Diagrammdateien, ohne Punkt.
    pub fn endung(&self) -> &str {
        &self.endung
    }

    /// Prüft einen relativen Pfad und gibt den vollen Pfad zurück.
    ///
    /// Die Datei muss nicht existieren; Existierendes wird aber auf Symlinks
    /// geprüft.
    pub fn pfad(&self, relativ: &str) -> Result<PathBuf, AblageFehler> {
        let teile = self.pruefe_relativ(relativ, &self.endung)?;
        let voll = teile.iter().fold(self.wurzel.clone(), |p, t| p.join(t));
        self.pruefe_symlinks(&voll)?;
        Ok(voll)
    }

    /// Prüft einen relativen Pfad für eine Ausgabe (`.svg` oder `.png`).
    pub fn ausgabe_pfad(&self, relativ: &str) -> Result<PathBuf, AblageFehler> {
        let mut teile = self.pruefe_relativ(relativ, "")?;
        let name = teile.last().cloned().unwrap_or_default();
        let klein = name.to_ascii_lowercase();
        if !(klein.ends_with(".svg") || klein.ends_with(".png")) {
            return Err(AblageFehler::FalscheEndung {
                pfad: relativ.to_string(),
                endung: "svg oder png".to_string(),
            });
        }
        teile.pop();
        let voll = teile.iter().fold(self.wurzel.clone(), |p, t| p.join(t));
        self.pruefe_symlinks(&voll.join(&name))?;
        Ok(voll.join(name))
    }

    /// Liest eine Datei; höchstens [`MAX_BYTES`], nur UTF-8, nur normale Dateien.
    pub fn lesen(&self, relativ: &str) -> Result<Inhalt, AblageFehler> {
        let voll = self.pfad(relativ)?;
        self.lese_datei(relativ, &voll)
    }

    /// Liest nur den Stand einer Datei.
    pub fn stand(&self, relativ: &str) -> Result<Stand, AblageFehler> {
        Ok(self.lesen(relativ)?.stand)
    }

    /// Legt eine Datei neu an; vorhandene Dateien werden nie überschrieben.
    pub fn anlegen(&self, relativ: &str, text: &str) -> Result<Stand, AblageFehler> {
        let voll = self.pfad(relativ)?;
        self.pruefe_groesse(text.len() as u64)?;
        let stand = stand_von(text.as_bytes());
        self.mit_sperre(|| {
            self.ordner_bereit(&voll, relativ)?;
            self.pruefe_symlinks(&voll)?;
            let mut datei = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&voll)
                .map_err(|fehler| match fehler.kind() {
                    std::io::ErrorKind::AlreadyExists => {
                        AblageFehler::ExistiertBereits(relativ.to_string())
                    }
                    _ => datei_fehler(relativ, &fehler),
                })?;
            datei
                .write_all(text.as_bytes())
                .map_err(|fehler| datei_fehler(relativ, &fehler))?;
            datei
                .sync_all()
                .map_err(|fehler| datei_fehler(relativ, &fehler))?;
            Ok(stand)
        })
    }

    /// Ändert eine Datei, wenn ihr Inhalt noch dem gelesenen [`Stand`] entspricht.
    ///
    /// Ablauf unter der Sperre: Stand prüfen → Sicherung `<name>.bak` schreiben →
    /// neuen Inhalt temporär schreiben → umbenennen. Bei falschem Stand wird
    /// **nichts** geschrieben.
    pub fn aendern(&self, relativ: &str, text: &str, stand: &Stand) -> Result<Stand, AblageFehler> {
        let voll = self.pfad(relativ)?;
        self.pruefe_groesse(text.len() as u64)?;
        let neu = stand_von(text.as_bytes());
        self.mit_sperre(|| {
            let alt = self.lese_datei(relativ, &voll)?;
            if alt.stand != *stand {
                return Err(AblageFehler::InzwischenGeaendert(relativ.to_string()));
            }
            self.ordner_bereit(&voll, relativ)?;
            self.pruefe_symlinks(&voll)?;
            let sicherung = sicherungspfad(&voll);
            atomar_schreiben(&sicherung, alt.text.as_bytes()).map_err(|fehler| {
                AblageFehler::Sicherung {
                    pfad: relativ.to_string(),
                    grund: fehler.to_string(),
                }
            })?;
            atomar_schreiben(&voll, text.as_bytes())
                .map_err(|fehler| datei_fehler(relativ, &fehler))?;
            Ok(neu)
        })
    }

    /// Schreibt eine Ausgabe (SVG oder PNG); vorhandene Dateien bleiben unberührt.
    pub fn ausgabe_schreiben(&self, relativ: &str, bytes: &[u8]) -> Result<(), AblageFehler> {
        let voll = self.ausgabe_pfad(relativ)?;
        self.pruefe_groesse(bytes.len() as u64)?;
        self.mit_sperre(|| {
            self.ordner_bereit(&voll, relativ)?;
            self.pruefe_symlinks(&voll)?;
            let mut datei = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&voll)
                .map_err(|fehler| match fehler.kind() {
                    std::io::ErrorKind::AlreadyExists => {
                        AblageFehler::ExistiertBereits(relativ.to_string())
                    }
                    _ => datei_fehler(relativ, &fehler),
                })?;
            datei
                .write_all(bytes)
                .map_err(|fehler| datei_fehler(relativ, &fehler))?;
            datei
                .sync_all()
                .map_err(|fehler| datei_fehler(relativ, &fehler))
        })
    }

    /// Alle Diagrammdateien im Arbeitsordner, Unterordner eingeschlossen.
    ///
    /// Ein großer Arbeitsordner (etwa „Dokumente“) darf kein Werkzeug minutenlang
    /// aufhalten: gesucht wird von oben nach unten, höchstens
    /// [`SUCHE_FRIST`] lang und [`SUCHE_MAX`] Treffer. [`Ablage::suche`] sagt, ob
    /// etwas fehlt.
    pub fn liste(&self) -> Result<Vec<Eintrag>, AblageFehler> {
        Ok(self.suche()?.eintraege)
    }

    /// Wie [`Ablage::liste`], mit der Auskunft, ob die Suche vollständig war.
    pub fn suche(&self) -> Result<Suche, AblageFehler> {
        self.suche_mit(SUCHE_FRIST)
    }

    fn suche_mit(&self, frist: std::time::Duration) -> Result<Suche, AblageFehler> {
        let beginn = std::time::Instant::now();
        let endung = format!(".{}", self.endung);
        let mut eintraege = Vec::new();
        let mut vollstaendig = true;
        let mut offen = std::collections::VecDeque::new();
        offen.push_back((self.wurzel.clone(), String::new(), 0usize));
        'suche: while let Some((ordner, vorsatz, tiefe)) = offen.pop_front() {
            let verzeichnis = match fs::read_dir(&ordner) {
                Ok(verzeichnis) => verzeichnis,
                // Der Arbeitsordner selbst muss lesbar sein; ein gesperrter
                // Unterordner wird übergangen.
                Err(fehler) if tiefe == 0 => {
                    return Err(AblageFehler::OrdnerLesen {
                        pfad: ordner.display().to_string(),
                        grund: fehler.to_string(),
                    })
                }
                Err(_) => continue,
            };
            for eintrag in verzeichnis.flatten() {
                if beginn.elapsed() > frist || eintraege.len() >= SUCHE_MAX {
                    vollstaendig = false;
                    break 'suche;
                }
                let name = eintrag.file_name().to_string_lossy().to_string();
                let Ok(art) = eintrag.file_type() else {
                    continue;
                };
                if name.starts_with('.') || art.is_symlink() {
                    continue;
                }
                let relativ = if vorsatz.is_empty() {
                    name.clone()
                } else {
                    format!("{vorsatz}/{name}")
                };
                if art.is_dir() {
                    if tiefe < MAX_TIEFE && !UEBERGANGEN.contains(&name.as_str()) {
                        offen.push_back((eintrag.path(), relativ, tiefe + 1));
                    }
                } else if art.is_file() && name.to_ascii_lowercase().ends_with(&endung) {
                    let daten = eintrag.metadata().ok();
                    eintraege.push(Eintrag {
                        pfad: relativ,
                        groesse: daten.as_ref().map(|d| d.len()).unwrap_or(0),
                        geaendert: daten.and_then(|d| d.modified().ok()),
                    });
                }
            }
        }
        eintraege.sort_by(|a, b| a.pfad.cmp(&b.pfad));
        Ok(Suche {
            eintraege,
            vollstaendig,
        })
    }

    /// Prüft die Regeln für einen relativen Pfad und zerlegt ihn in Segmente.
    fn pruefe_relativ(&self, relativ: &str, endung: &str) -> Result<Vec<String>, AblageFehler> {
        let relativ = relativ.trim();
        if relativ.is_empty() {
            return Err(AblageFehler::PfadLeer);
        }
        if relativ.contains('\0') {
            return Err(AblageFehler::Verboten {
                pfad: relativ.to_string(),
                was: "NUL".to_string(),
            });
        }
        if relativ.contains('\\') {
            return Err(AblageFehler::Verboten {
                pfad: relativ.to_string(),
                was: "Backslash (nutze / als Trenner)".to_string(),
            });
        }
        if relativ.contains(':') {
            return Err(AblageFehler::Verboten {
                pfad: relativ.to_string(),
                was: ":".to_string(),
            });
        }
        if relativer_pfad_ist_absolut(relativ) {
            return Err(AblageFehler::AbsoluterPfad(relativ.to_string()));
        }

        let mut teile = Vec::new();
        for teil in relativ.split('/') {
            if teil.is_empty() || teil == "." {
                return Err(AblageFehler::Verboten {
                    pfad: relativ.to_string(),
                    was: format!("das Segment „{teil}“"),
                });
            }
            if teil == ".." {
                return Err(AblageFehler::PfadAusbruch(relativ.to_string()));
            }
            if teil.starts_with('.') {
                return Err(AblageFehler::Versteckt(teil.to_string()));
            }
            if ist_geraetename(teil) {
                return Err(AblageFehler::Geraetename(teil.to_string()));
            }
            if teil.chars().count() > 255 {
                return Err(AblageFehler::Verboten {
                    pfad: relativ.to_string(),
                    was: "ein Name über 255 Zeichen".to_string(),
                });
            }
            if teil.chars().any(char::is_control) {
                return Err(AblageFehler::Verboten {
                    pfad: relativ.to_string(),
                    was: "ein Steuerzeichen".to_string(),
                });
            }
            teile.push(teil.to_string());
        }

        if teile.len() > MAX_TIEFE + 1 {
            return Err(AblageFehler::Verboten {
                pfad: relativ.to_string(),
                was: format!("mehr als {MAX_TIEFE} Unterordner"),
            });
        }

        let name = teile.last().expect("mindestens ein Segment");
        if !endung.is_empty()
            && !name
                .to_ascii_lowercase()
                .ends_with(&format!(".{}", endung.to_ascii_lowercase()))
        {
            return Err(AblageFehler::FalscheEndung {
                pfad: relativ.to_string(),
                endung: endung.to_string(),
            });
        }
        Ok(teile)
    }

    /// Prüft jede existierende Ebene auf symbolische Links und normale Ordner.
    fn pruefe_symlinks(&self, voll: &Path) -> Result<(), AblageFehler> {
        let relativ = voll
            .strip_prefix(&self.wurzel)
            .map_err(|_| AblageFehler::AbsoluterPfad(voll.display().to_string()))?;
        let mut lauf = self.wurzel.clone();
        let mut teile: Vec<String> = relativ
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect();
        let letzte = teile.pop();
        for teil in teile {
            lauf.push(&teil);
            match fs::symlink_metadata(&lauf) {
                Ok(daten) if daten.file_type().is_symlink() => {
                    return Err(AblageFehler::Symlink(lauf.display().to_string()));
                }
                Ok(daten) if !daten.is_dir() => {
                    return Err(AblageFehler::KeineDatei(lauf.display().to_string()));
                }
                Ok(_) => {}
                Err(fehler) if fehler.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(fehler) => return Err(datei_fehler(&lauf.display().to_string(), &fehler)),
            }
        }
        if let Some(teil) = letzte {
            lauf.push(teil);
            match fs::symlink_metadata(&lauf) {
                Ok(daten) if daten.file_type().is_symlink() => {
                    return Err(AblageFehler::Symlink(lauf.display().to_string()));
                }
                Ok(daten) if !daten.is_file() => {
                    return Err(AblageFehler::KeineDatei(lauf.display().to_string()));
                }
                Ok(_) => {}
                Err(fehler) if fehler.kind() == std::io::ErrorKind::NotFound => {}
                Err(fehler) => return Err(datei_fehler(&lauf.display().to_string(), &fehler)),
            }
        }
        Ok(())
    }

    /// Jede Ebene eines **Ordners** darf kein Symlink sein und muss, wenn es
    /// sie schon gibt, ein Ordner sein.
    fn pruefe_ordner(&self, ordner: &Path) -> Result<(), AblageFehler> {
        let relativ = ordner
            .strip_prefix(&self.wurzel)
            .map_err(|_| AblageFehler::AbsoluterPfad(ordner.display().to_string()))?;
        let mut lauf = self.wurzel.clone();
        for teil in relativ.components() {
            lauf.push(teil);
            match fs::symlink_metadata(&lauf) {
                Ok(daten) if daten.file_type().is_symlink() => {
                    return Err(AblageFehler::Symlink(lauf.display().to_string()));
                }
                Ok(daten) if !daten.is_dir() => {
                    return Err(AblageFehler::KeineDatei(lauf.display().to_string()));
                }
                Ok(_) => {}
                Err(fehler) if fehler.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(fehler) => return Err(datei_fehler(&lauf.display().to_string(), &fehler)),
            }
        }
        Ok(())
    }

    /// Sorgt dafür, dass der Elternordner existiert und **innerhalb** der Wurzel
    /// liegt – auch nach dem Anlegen (kein Symlink dazwischen).
    fn ordner_bereit(&self, voll: &Path, relativ: &str) -> Result<(), AblageFehler> {
        let eltern = voll.parent().ok_or(AblageFehler::PfadLeer)?;
        // Zuerst prüfen, dann anlegen: sonst könnte `create_dir_all` einem
        // Symlink folgen und Ordner außerhalb des Arbeitsordners erzeugen.
        self.pruefe_ordner(eltern)?;
        fs::create_dir_all(eltern).map_err(|fehler| AblageFehler::OrdnerAnlegen {
            pfad: relativer_ordner(relativ),
            grund: fehler.to_string(),
        })?;
        let echt = fs::canonicalize(eltern).map_err(|fehler| AblageFehler::OrdnerAnlegen {
            pfad: relativer_ordner(relativ),
            grund: fehler.to_string(),
        })?;
        let erwartet = match relativ.rfind('/') {
            Some(pos) => self.wurzel.join(&relativ[..pos]),
            None => self.wurzel.clone(),
        };
        if echt != erwartet {
            return Err(AblageFehler::Symlink(relativer_ordner(relativ)));
        }
        Ok(())
    }

    fn lese_datei(&self, relativ: &str, voll: &Path) -> Result<Inhalt, AblageFehler> {
        let daten = fs::symlink_metadata(voll).map_err(|fehler| match fehler.kind() {
            std::io::ErrorKind::NotFound => AblageFehler::NichtGefunden(relativ.to_string()),
            _ => datei_fehler(relativ, &fehler),
        })?;
        if daten.file_type().is_symlink() {
            return Err(AblageFehler::Symlink(relativ.to_string()));
        }
        if !daten.is_file() {
            return Err(AblageFehler::KeineDatei(relativ.to_string()));
        }
        self.pruefe_groesse(daten.len())?;
        let mut datei = File::open(voll).map_err(|fehler| datei_fehler(relativ, &fehler))?;
        // Nach dem Öffnen erneut prüfen: in der Zwischenzeit könnte der Name
        // gegen einen Symlink getauscht worden sein.
        if let Ok(jetzt) = fs::symlink_metadata(voll) {
            if jetzt.file_type().is_symlink() {
                return Err(AblageFehler::Symlink(relativ.to_string()));
            }
            if jetzt.len() != daten.len() {
                return Err(AblageFehler::InzwischenGeaendert(relativ.to_string()));
            }
        }
        let mut roh = Vec::new();
        (&mut datei)
            .take(MAX_BYTES + 1)
            .read_to_end(&mut roh)
            .map_err(|fehler| datei_fehler(relativ, &fehler))?;
        self.pruefe_groesse(roh.len() as u64)?;
        let text =
            String::from_utf8(roh).map_err(|_| AblageFehler::KeinText(relativ.to_string()))?;
        Ok(Inhalt {
            stand: stand_von(text.as_bytes()),
            text,
            groesse: daten.len(),
            geaendert: daten.modified().ok(),
        })
    }

    fn pruefe_groesse(&self, groesse: u64) -> Result<(), AblageFehler> {
        if groesse > MAX_BYTES {
            return Err(AblageFehler::ZuGross {
                groesse,
                max: MAX_BYTES,
            });
        }
        Ok(())
    }

    /// Führt `arbeit` unter der Dateisperre des Arbeitsordners aus.
    fn mit_sperre<T>(
        &self,
        arbeit: impl FnOnce() -> Result<T, AblageFehler>,
    ) -> Result<T, AblageFehler> {
        let pfad = self.wurzel.join(SPERRDATEI);
        let datei = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&pfad)
            .map_err(|fehler| AblageFehler::Sperre(fehler.to_string()))?;
        let mut sperre = RwLock::new(datei);
        let _wache = sperre
            .write()
            .map_err(|fehler| AblageFehler::Sperre(fehler.to_string()))?;
        arbeit()
    }
}

/// Fehler der Ablage. Jede Meldung ist für einen Agenten gedacht: sie sagt, was
/// falsch ist und was er stattdessen tun soll. Keine Meldung enthält
/// Zugangsdaten.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AblageFehler {
    /// Das Feld `ordner` ist leer.
    #[error("Arbeitsordner fehlt – bitte im quiver-Fenster das Feld „ordner“ ausfüllen")]
    OrdnerFehlt,
    /// Der Arbeitsordner ist relativ.
    #[error("Arbeitsordner „{0}“ ist nicht absolut – bitte einen vollen Pfad angeben")]
    OrdnerNichtAbsolut(String),
    /// Der Arbeitsordner ließ sich nicht anlegen oder öffnen.
    #[error("Arbeitsordner „{pfad}“ ließ sich nicht anlegen: {grund}")]
    OrdnerAnlegen {
        /// Betroffener Pfad.
        pfad: String,
        /// Ursache aus dem Dateisystem.
        grund: String,
    },
    /// Der Ordner ließ sich nicht lesen.
    #[error("Ordner „{pfad}“ ließ sich nicht lesen: {grund}")]
    OrdnerLesen {
        /// Betroffener Pfad.
        pfad: String,
        /// Ursache aus dem Dateisystem.
        grund: String,
    },
    /// Die Endung im Aufruf war unbrauchbar.
    #[error("Endung „{0}“ ist ungültig – erwartet wird z. B. „excalidraw“")]
    EndungUngueltig(String),
    /// Der Pfad war leer.
    #[error("Pfad fehlt – bitte einen Pfad im Arbeitsordner angeben")]
    PfadLeer,
    /// Der Pfad war absolut.
    #[error("Pfad „{0}“ ist absolut – erlaubt sind nur Pfade im Arbeitsordner")]
    AbsoluterPfad(String),
    /// Der Pfad wollte den Arbeitsordner verlassen.
    #[error("Pfad „{0}“ verlässt den Arbeitsordner – „..“ ist nicht erlaubt")]
    PfadAusbruch(String),
    /// Ein Segment war aus einem anderen Grund verboten.
    #[error("Pfad „{pfad}“ enthält {was} – das ist nicht erlaubt")]
    Verboten {
        /// Betroffener Pfad.
        pfad: String,
        /// Was genau verboten ist.
        was: String,
    },
    /// Ein Name war ein Windows-Gerätename.
    #[error("Name „{0}“ ist ein Windows-Gerätename – bitte anders benennen")]
    Geraetename(String),
    /// Ein Name war versteckt.
    #[error("Name „{0}“ ist versteckt – Namen mit „.“ am Anfang sind nicht erlaubt")]
    Versteckt(String),
    /// Die Endung passte nicht.
    #[error("Datei „{pfad}“ braucht die Endung „.{endung}“")]
    FalscheEndung {
        /// Betroffener Pfad.
        pfad: String,
        /// Erwartete Endung.
        endung: String,
    },
    /// Eine Ebene war ein symbolischer Link.
    #[error("„{0}“ ist ein symbolischer Link – symbolische Links sind auf jeder Ebene verboten")]
    Symlink(String),
    /// Es war keine normale Datei.
    #[error("„{0}“ ist keine normale Datei")]
    KeineDatei(String),
    /// Die Datei gibt es nicht.
    #[error("„{0}“ gibt es nicht – bitte mit der Liste prüfen")]
    NichtGefunden(String),
    /// Die Datei existiert schon; Anlegen überschreibt nie.
    #[error("„{0}“ existiert bereits – anlegen überschreibt nie; zum Ändern lesen und mit „stand“ schreiben")]
    ExistiertBereits(String),
    /// Der Inhalt war kein UTF-8.
    #[error("„{0}“ ist kein Text (UTF-8) – als Diagrammdatei nicht verwendbar")]
    KeinText(String),
    /// Die Datei war zu groß.
    #[error("Datei ist {groesse} Bytes groß – erlaubt sind höchstens {max} Bytes (10 MB)")]
    ZuGross {
        /// Tatsächliche Größe.
        groesse: u64,
        /// Erlaubte Größe.
        max: u64,
    },
    /// Der Stand stimmte nicht.
    #[error("„{0}“ wurde inzwischen geändert – bitte neu lesen, dann erneut schreiben")]
    InzwischenGeaendert(String),
    /// Die Sicherung ließ sich nicht schreiben.
    #[error("Sicherung für „{pfad}“ ließ sich nicht schreiben: {grund}")]
    Sicherung {
        /// Betroffener Pfad.
        pfad: String,
        /// Ursache aus dem Dateisystem.
        grund: String,
    },
    /// Die Dateisperre ließ sich nicht nehmen.
    #[error("Dateisperre im Arbeitsordner ließ sich nicht nehmen: {0}")]
    Sperre(String),
    /// Sonstiger Dateifehler.
    #[error("Datei „{pfad}“: {grund}")]
    Datei {
        /// Betroffener Pfad.
        pfad: String,
        /// Ursache aus dem Dateisystem.
        grund: String,
    },
}

fn datei_fehler(pfad: &str, fehler: &std::io::Error) -> AblageFehler {
    AblageFehler::Datei {
        pfad: pfad.to_string(),
        grund: fehler.to_string(),
    }
}

fn relativer_ordner(relativ: &str) -> String {
    match relativ.rfind('/') {
        Some(pos) => relativ[..pos].to_string(),
        None => ".".to_string(),
    }
}

fn stand_von(bytes: &[u8]) -> Stand {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Stand(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use fmt::Write as _;
        let _ = write!(text, "{byte:02x}");
    }
    text
}

fn sicherungspfad(voll: &Path) -> PathBuf {
    let name = voll
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    voll.with_file_name(format!("{name}.bak"))
}

fn temporaer(voll: &Path) -> PathBuf {
    let nummer = ZAEHLER.fetch_add(1, Ordering::Relaxed);
    let name = voll
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    voll.with_file_name(format!(
        ".quiver-tmp-{}-{nummer}-{name}",
        std::process::id()
    ))
}

/// Schreibt `bytes` über eine temporäre Datei und benennt sie um.
fn atomar_schreiben(voll: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let temp = temporaer(voll);
    let ergebnis = (|| {
        let mut datei = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        datei.write_all(bytes)?;
        datei.sync_all()?;
        drop(datei);
        // Ersetzt auch unter Windows (Rust nutzt MOVEFILE_REPLACE_EXISTING). Das
        // Ziel nie vorher löschen: scheitert das Umbenennen, wäre es weg.
        fs::rename(&temp, voll)
    })();
    if ergebnis.is_err() {
        let _ = fs::remove_file(&temp);
    }
    ergebnis
}

fn aufgeloest(ordner: &str) -> PathBuf {
    if ordner == "~" {
        if let Some(zuhause) = zuhause() {
            return zuhause;
        }
    }
    if let Some(rest) = ordner.strip_prefix("~/") {
        if let Some(zuhause) = zuhause() {
            return zuhause.join(rest);
        }
    }
    PathBuf::from(ordner)
}

fn zuhause() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}

fn relativer_pfad_ist_absolut(relativ: &str) -> bool {
    if relativ.starts_with('/') {
        return true;
    }
    // Windows-Laufwerk (C:…) und UNC (\\server) – der Backslash ist ohnehin
    // verboten, die Prüfung ist die zweite Wand.
    let mut zeichen = relativ.chars();
    matches!(
        (zeichen.next(), zeichen.next()),
        (Some(buchstabe), Some(':')) if buchstabe.is_ascii_alphabetic()
    )
}

fn ist_geraetename(teil: &str) -> bool {
    let stamm = teil.split('.').next().unwrap_or("").to_ascii_uppercase();
    if matches!(stamm.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return true;
    }
    // COM1…COM9 und LPT1…LPT9 (auch mit Endung, z. B. „COM1.txt“).
    ["COM", "LPT"].iter().any(|vorsatz| {
        stamm.strip_prefix(vorsatz).is_some_and(|rest| {
            rest.len() == 1 && rest.starts_with(|z: char| ('1'..='9').contains(&z))
        })
    })
}

#[cfg(test)]
mod suche_tests {
    use super::*;

    /// Frist und Höchstzahl begrenzen die Suche, und das steht dabei.
    #[test]
    fn die_suche_endet_mit_der_frist_und_sagt_es() {
        let ordner = tempfile::tempdir().unwrap();
        for nummer in 0..50 {
            std::fs::create_dir_all(ordner.path().join(format!("o{nummer}"))).unwrap();
            std::fs::write(ordner.path().join(format!("o{nummer}/a.drawio")), "x").unwrap();
        }
        std::fs::create_dir_all(ordner.path().join("node_modules/x")).unwrap();
        std::fs::write(ordner.path().join("node_modules/x/b.drawio"), "x").unwrap();
        let ablage = Ablage::neu(ordner.path().to_str().unwrap(), "drawio").unwrap();
        let ganz = ablage.suche().unwrap();
        assert!(ganz.vollstaendig);
        assert_eq!(ganz.eintraege.len(), 50);
        assert_eq!(mit_hinweis("x".into(), &ganz), "x");
        let knapp = ablage.suche_mit(std::time::Duration::ZERO).unwrap();
        assert!(!knapp.vollstaendig);
        assert!(mit_hinweis("x".into(), &knapp).contains("unvollständig"));
    }
}
