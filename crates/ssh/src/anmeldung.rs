// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der Ablauf jeder Verbindung (Aufgabe 05, „Pflicht: nach einer
//! Passwort-Anmeldung immer Schlüssel“).
//!
//! 1. **Schlüssel vorhanden** → nur Schlüssel-Anmeldung. Schlägt sie fehl, ist
//!    das ein Fehler; es gibt keinen stillen Rückfall aufs Passwort.
//! 2. **Kein Schlüssel, aber Anmelde-Passwort** → Passwort-Anmeldung, Schlüssel
//!    erzeugen, öffentlichen Teil in `authorized_keys` eintragen, neue
//!    Verbindung **nur mit Schlüssel** prüfen, erst dann den Schlüssel
//!    übernehmen. Scheitert ein Schritt, wird abgebrochen und aufgeräumt.
//! 3. **Weder noch** → Hinweis, das Passwort im quiver-Fenster zu hinterlegen.
//!
//! `ssh_schluessel_einrichten` erzwingt Schritt 2 – etwa nach einem neu
//! aufgesetzten Server; der alte Schlüssel wird erst nach erfolgreicher Prüfung
//! ersetzt.

use std::time::Duration;

use crate::client::{self, Anmeldung, ClientFehler, HostSchluesselGeaendert, Sitzung};
use crate::pfade::{self, PfadFehler, Pfade};
use crate::schluessel::{self, SchluesselFehler};
use crate::verbindung::Verbindung;

/// Wie lange der Aufbau einer Verbindung höchstens dauern darf.
pub const VERBINDUNGSGEDULD: Duration = Duration::from_secs(30);
/// Zeitlimit für die Schritte der Schlüssel-Einrichtung.
const EINBAU_GEDULD: Duration = Duration::from_secs(30);

/// Was ein Werkzeug braucht: die Sitzung und was der User wissen soll.
pub struct Bereit {
    pub sitzung: Sitzung,
    /// Einmaliger Hinweis (Schlüssel eingerichtet, Host-Schlüssel gemerkt).
    pub hinweis: Option<String>,
}

/// Warum eine Verbindung nicht zustande kommt. Enthält nie ein Geheimnis.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SshFehler {
    #[error(
        "Für {name} fehlt das Anmelde-Passwort für die erste Anmeldung – bitte im quiver-Fenster unter Tools → SSH eintragen."
    )]
    PasswortFehlt { name: String },
    #[error(
        "Schlüssel-Anmeldung an {name} fehlgeschlagen. Mit `ssh_schluessel_einrichten` neu einrichten (braucht das Anmelde-Passwort)."
    )]
    SchluesselAbgelehnt { name: String },
    #[error(
        "Passwort-Anmeldung an {name} fehlgeschlagen – bitte das Anmelde-Passwort im quiver-Fenster prüfen."
    )]
    PasswortAbgelehnt { name: String },
    #[error(
        "Der Server {adresse} hat die Schlüssel-Anmeldung abgelehnt (evtl. `PubkeyAuthentication no` in der sshd-Konfiguration). Die Schlüsseldateien wurden wieder entfernt; {name} braucht weiterhin das Anmelde-Passwort."
    )]
    EinbauAbgelehnt { name: String, adresse: String },
    #[error(transparent)]
    HostSchluessel(#[from] HostSchluesselGeaendert),
    #[error(transparent)]
    Schluessel(#[from] SchluesselFehler),
    #[error(transparent)]
    Pfad(#[from] PfadFehler),
    /// Zwischenschritt: der Server hat die Schlüssel-Anmeldung abgelehnt. Die
    /// Aufrufer machen daraus eine passende Meldung (erste Einrichtung oder
    /// vorhandener Schlüssel).
    #[error("Die Anmeldung mit dem Schlüssel an {name} wurde abgelehnt.")]
    Abgelehnt { name: String },
    #[error("Verbindung zu {name} ({adresse}) fehlgeschlagen: {grund}")]
    Verbindung {
        name: String,
        adresse: String,
        grund: String,
    },
    #[error("Die Schlüssel-Einrichtung auf {name} ist gescheitert: {grund}")]
    Einbau { name: String, grund: String },
    #[error("{name}: {grund}")]
    Fern { name: String, grund: String },
}

/// Wie eine abgelehnte Anmeldung gemeint ist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Art {
    Schluessel,
    Passwort,
}

/// Verbindung nach den Regeln oben herstellen.
pub async fn bereit(verbindung: &Verbindung, pfade: &Pfade) -> Result<Bereit, SshFehler> {
    let datei = pfade.schluessel(&verbindung.name)?;
    if datei.exists() {
        let sitzung = match oeffnen(verbindung, pfade, Anmeldung::Schluessel(&datei)).await {
            Ok(sitzung) => sitzung,
            Err(SshFehler::Abgelehnt { .. }) => {
                return Err(SshFehler::SchluesselAbgelehnt {
                    name: verbindung.name.clone(),
                })
            }
            Err(anderer) => return Err(anderer),
        };
        return Ok(Bereit {
            hinweis: host_hinweis(&sitzung),
            sitzung,
        });
    }

    if verbindung.anmelde_passwort.is_some() {
        return einrichten(verbindung, pfade, false).await;
    }

    Err(SshFehler::PasswortFehlt {
        name: verbindung.name.clone(),
    })
}

/// `ssh_schluessel_einrichten`: Schritt 2 erzwingen, alten Schlüssel ersetzen.
pub async fn schluessel_einrichten(
    verbindung: &Verbindung,
    pfade: &Pfade,
) -> Result<Bereit, SshFehler> {
    if verbindung.anmelde_passwort.is_none() {
        return Err(SshFehler::PasswortFehlt {
            name: verbindung.name.clone(),
        });
    }
    einrichten(verbindung, pfade, true).await
}

/// Schlüssel einrichten (Schritt 2) und mit ihm verbinden.
///
/// `ersetzen` heißt: auch einen vorhandenen Schlüssel neu aufsetzen. Der alte
/// bleibt liegen, bis der neue nachweislich funktioniert.
async fn einrichten(
    verbindung: &Verbindung,
    pfade: &Pfade,
    ersetzen: bool,
) -> Result<Bereit, SshFehler> {
    let passwort =
        verbindung
            .anmelde_passwort
            .as_deref()
            .ok_or_else(|| SshFehler::PasswortFehlt {
                name: verbindung.name.clone(),
            })?;
    let ziel = pfade.schluessel(&verbindung.name)?;
    let vorlaeufig = pfade.vorlaeufig(&verbindung.name)?;
    let ersetzt_wurde = ziel.exists();

    // Reste eines früheren, abgebrochenen Versuchs zuerst weg.
    let _ = std::fs::remove_file(&vorlaeufig);
    let _ = std::fs::remove_file(pfade::oeffentlich(&vorlaeufig));

    let paar = schluessel::erzeugen(&vorlaeufig, &format!("quiver-ssh {}", verbindung.name))?;

    let ergebnis = async {
        // Erste Anmeldung: ausdrücklich per Passwort, nicht per Schlüssel.
        let sitzung = oeffnen(verbindung, pfade, Anmeldung::Passwort(passwort)).await?;
        // Der TOFU-Hinweis entsteht hier, nicht bei der Prüf-Verbindung.
        let host_hinweis = host_hinweis(&sitzung);
        einbauen(verbindung, &sitzung, &paar.zeile).await?;
        sitzung.beenden().await;

        // Beweis: eine neue Verbindung, die nur den Schlüssel kennt.
        match oeffnen(verbindung, pfade, Anmeldung::Schluessel(&paar.privat)).await {
            Ok(sitzung) => Ok((sitzung, host_hinweis)),
            Err(SshFehler::Abgelehnt { .. }) | Err(SshFehler::Verbindung { .. }) => {
                Err(SshFehler::EinbauAbgelehnt {
                    name: verbindung.name.clone(),
                    adresse: verbindung.adresse(),
                })
            }
            Err(anderer) => Err(anderer),
        }
    }
    .await;

    match ergebnis {
        Ok((sitzung, host_hinweis)) => {
            let uebernommen = match schluessel::uebernehmen(&paar, &ziel) {
                Ok(uebernommen) => uebernommen,
                Err(fehler) => {
                    schluessel::entfernen(&paar);
                    return Err(fehler.into());
                }
            };
            let mut hinweis = host_hinweis.unwrap_or_default();
            if ersetzen && ersetzt_wurde {
                hinweis.push_str("Der bisherige Schlüssel wurde ersetzt. ");
            }
            hinweis.push_str(&format!(
                "Schlüssel für {} eingerichtet ({}). Das Anmelde-Passwort wird nicht mehr gebraucht und kann im quiver-Fenster gelöscht werden.",
                verbindung.name, uebernommen.fingerprint
            ));
            Ok(Bereit {
                sitzung,
                hinweis: Some(hinweis),
            })
        }
        Err(fehler) => {
            schluessel::entfernen(&paar);
            Err(fehler)
        }
    }
}

/// Der Schritt „öffentlichen Schlüssel eintragen“ (doppelt vermeiden).
async fn einbauen(
    verbindung: &Verbindung,
    sitzung: &Sitzung,
    zeile: &str,
) -> Result<(), SshFehler> {
    // `~` löst die entfernte Shell auf; Rechte setzt derselbe Aufruf, weil
    // sshd ein gruppen-schreibbares `authorized_keys` stillschweigend ignoriert.
    let vorbereiten = sitzung
        .exec(
            "umask 077 && mkdir -p \"$HOME/.ssh\" && touch \"$HOME/.ssh/authorized_keys\" \
             && chmod 700 \"$HOME/.ssh\" && chmod 600 \"$HOME/.ssh/authorized_keys\" \
             && printf '%s' \"$HOME/.ssh/authorized_keys\"",
            None,
            EINBAU_GEDULD,
        )
        .await
        .map_err(|fehler| einbau_fehler(verbindung, fehler))?;
    if vorbereiten.status != 0 {
        return Err(SshFehler::Einbau {
            name: verbindung.name.clone(),
            grund: kurz(&vorbereiten.stderr, &format!("exit {}", vorbereiten.status)),
        });
    }

    let datei = vorbereiten.stdout.trim().to_string();
    if datei.is_empty() {
        return Err(SshFehler::Einbau {
            name: verbindung.name.clone(),
            grund: "der entfernte Pfad von ~/.ssh/authorized_keys ist leer".to_string(),
        });
    }

    let vorhanden = sitzung
        .exec(
            &format!("cat {}", client::quoten(&datei)),
            None,
            EINBAU_GEDULD,
        )
        .await
        .map_err(|fehler| einbau_fehler(verbindung, fehler))?;
    if vorhanden.status != 0 {
        return Err(SshFehler::Einbau {
            name: verbindung.name.clone(),
            grund: kurz(
                &vorhanden.stderr,
                &format!("authorized_keys nicht lesbar (exit {})", vorhanden.status),
            ),
        });
    }
    if vorhanden
        .stdout
        .lines()
        .any(|vorhandene_zeile| schluessel::gleiches_material(vorhandene_zeile, zeile))
    {
        // Schon da: nichts anhängen, sonst wächst die Datei bei jedem Aufruf.
        return Ok(());
    }

    let anhaengen = sitzung
        .exec(
            &format!(
                "printf '%s\\n' {} >> {}",
                client::quoten(zeile),
                client::quoten(&datei)
            ),
            None,
            EINBAU_GEDULD,
        )
        .await
        .map_err(|fehler| einbau_fehler(verbindung, fehler))?;
    if anhaengen.status != 0 {
        return Err(SshFehler::Einbau {
            name: verbindung.name.clone(),
            grund: kurz(
                &anhaengen.stderr,
                &format!(
                    "authorized_keys nicht schreibbar (exit {})",
                    anhaengen.status
                ),
            ),
        });
    }
    Ok(())
}

fn einbau_fehler(verbindung: &Verbindung, fehler: anyhow::Error) -> SshFehler {
    SshFehler::Einbau {
        name: verbindung.name.clone(),
        grund: fehler.to_string(),
    }
}

/// Eine kurze Meldung aus der Ausgabe des entfernten Befehls, nie ein
/// Geheimnis (die Ausgabe stammt von `mkdir`/`cat`/`printf`).
fn kurz(stderr: &str, ersatz: &str) -> String {
    let text = stderr.trim();
    if text.is_empty() {
        ersatz.to_string()
    } else {
        text.lines().next().unwrap_or(ersatz).to_string()
    }
}

/// Der TOFU-Hinweis, wenn der Host-Schlüssel gerade gemerkt wurde.
fn host_hinweis(sitzung: &Sitzung) -> Option<String> {
    sitzung.neu_gemerkt.as_ref().map(|fingerprint| {
        format!("Host-Schlüssel zum ersten Mal gesehen und gemerkt ({fingerprint}).")
    })
}

/// Öffnet eine Sitzung und übersetzt die Fehler in Meldungen ohne Geheimnis.
async fn oeffnen(
    verbindung: &Verbindung,
    pfade: &Pfade,
    anmeldung: Anmeldung<'_>,
) -> Result<Sitzung, SshFehler> {
    let art = match anmeldung {
        Anmeldung::Schluessel(_) => Art::Schluessel,
        Anmeldung::Passwort(_) => Art::Passwort,
    };
    match tokio::time::timeout(
        VERBINDUNGSGEDULD,
        client::verbinden(verbindung, pfade, anmeldung),
    )
    .await
    {
        Ok(Ok(sitzung)) => Ok(sitzung),
        Ok(Err(ClientFehler::HostSchluessel(geaendert))) => Err(geaendert.into()),
        Ok(Err(ClientFehler::Abgelehnt)) => Err(abgelehnt(verbindung, art)),
        Ok(Err(ClientFehler::Schluessel(grund))) => Err(SshFehler::Verbindung {
            name: verbindung.name.clone(),
            adresse: verbindung.adresse(),
            grund,
        }),
        Ok(Err(ClientFehler::Verbindung { adresse, grund })) => Err(SshFehler::Verbindung {
            name: verbindung.name.clone(),
            adresse,
            grund,
        }),
        Err(_) => Err(SshFehler::Verbindung {
            name: verbindung.name.clone(),
            adresse: verbindung.adresse(),
            grund: format!(
                "keine Antwort innerhalb von {} s",
                VERBINDUNGSGEDULD.as_secs()
            ),
        }),
    }
}

fn abgelehnt(verbindung: &Verbindung, art: Art) -> SshFehler {
    match art {
        Art::Schluessel => SshFehler::Abgelehnt {
            name: verbindung.name.clone(),
        },
        Art::Passwort => SshFehler::PasswortAbgelehnt {
            name: verbindung.name.clone(),
        },
    }
}
