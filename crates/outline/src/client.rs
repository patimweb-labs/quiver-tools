// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der Weg zu Outline (SPEC §6): jede Methode ist ein POST auf
//! `<wurzel>/api/<methode>` mit JSON-Rumpf und `Authorization: Bearer …`.
//!
//! Dieses Modul ist die **einzige** Stelle mit Netzverkehr. Es übersetzt
//! Antworten in [`crate::domain`], Fehler in [`OutlineFehler`] und sorgt dafür,
//! dass kein Fehlertext den API-Schlüssel enthält.

use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;

use crate::Profil;

/// Wie lange auf Outline gewartet wird, bevor ein Aufruf aufgibt.
pub const ZEITLIMIT: Duration = Duration::from_secs(30);

/// Was beim Aufruf von Outline schiefgehen kann. **Kein** Text nennt den
/// API-Schlüssel: [`Client::aufrufen`] ersetzt ihn, falls ein Server ihn
/// zurückspiegelt.
#[derive(Debug, thiserror::Error)]
pub enum OutlineFehler {
    #[error("Im Profil fehlt die Adresse des Outline.")]
    KeineAdresse,
    #[error("Die Adresse „{0}“ ist keine gültige URL.")]
    UngueltigeAdresse(String),
    #[error("Im Profil fehlt der API-Schlüssel.")]
    KeinSchluessel,
    #[error(
        "Outline hat den API-Schlüssel abgelehnt (401). In Outline unter Einstellungen → API & Apps einen Schlüssel anlegen und die Einrichtung erneut öffnen."
    )]
    NichtAngemeldet,
    #[error("Outline verweigert den Zugriff (403){hinweis}. Der Benutzer des Schlüssels darf diese Sammlung oder Seite vielleicht nicht sehen.")]
    Verboten { hinweis: String },
    #[error("Outline drosselt die Anfragen (429). Kurz warten und erneut versuchen.")]
    Gedrosselt,
    #[error("Outline-Aufruf {methode} ist fehlgeschlagen ({status}){hinweis}.")]
    Status {
        methode: String,
        status: u16,
        hinweis: String,
    },
    #[error("Outline war unter {wurzel} nicht erreichbar: {ursache}")]
    NichtErreichbar { wurzel: String, ursache: String },
    #[error("Outline hat nicht innerhalb von {0} Sekunden geantwortet.")]
    Zeitlimit(u64),
    #[error("Unter {wurzel} antwortet etwas anderes als Outline (HTML statt JSON, Status {status}). Stimmt die Adresse?")]
    KeinOutline { wurzel: String, status: u16 },
    #[error("Outline meldet einen Fehler: {0}")]
    Meldung(String),
    #[error("Die Antwort von Outline war nicht lesbar: {0}")]
    Unlesbar(String),
}

/// Ein Ausschnitt aus einer Liste: wo sie weitergeht und wie lang sie ist.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Seiten {
    #[serde(default)]
    pub offset: Option<usize>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub total: Option<usize>,
}

/// Was Outline auf einen Aufruf antwortet: die Daten und – bei Listen – der
/// Ausschnitt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Antwort<T> {
    pub daten: T,
    pub seiten: Option<Seiten>,
}

/// Der Rahmen jeder Outline-Antwort: `{ ok, data, pagination? }`.
#[derive(Debug, Deserialize)]
struct Rahmen<T> {
    ok: Option<bool>,
    data: Option<T>,
    pagination: Option<Seiten>,
    error: Option<String>,
    message: Option<String>,
}

/// Ein Client für **ein** Profil.
pub struct Client {
    /// Die normalisierte Wurzel, z. B. `https://firma.getoutline.com`.
    wurzel: String,
    /// Der Schlüssel. Er verlässt dieses Modul nur als Kopfzeile.
    schluessel: String,
    http: reqwest::Client,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("wurzel", &self.wurzel)
            .finish_non_exhaustive()
    }
}

impl Client {
    /// Ein Client aus einem gespeicherten Profil.
    pub fn aus_profil(profil: &Profil) -> Result<Client, OutlineFehler> {
        let adresse = profil
            .werte
            .get("url")
            .map(String::as_str)
            .unwrap_or("")
            .trim();
        if adresse.is_empty() {
            return Err(OutlineFehler::KeineAdresse);
        }
        let wurzel = crate::werkzeug::wurzel(adresse)
            .map_err(|_| OutlineFehler::UngueltigeAdresse(adresse.to_string()))?;

        let schluessel = profil
            .geheim
            .get("api_key")
            .map(String::as_str)
            .unwrap_or("")
            .trim();
        if schluessel.is_empty() {
            return Err(OutlineFehler::KeinSchluessel);
        }

        let http = reqwest::Client::builder()
            .timeout(ZEITLIMIT)
            .user_agent(concat!("quiver-outline/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|fehler| OutlineFehler::NichtErreichbar {
                wurzel: wurzel.clone(),
                ursache: fehler.to_string(),
            })?;

        Ok(Client {
            wurzel,
            schluessel: schluessel.to_string(),
            http,
        })
    }

    /// Die Wurzel des Arbeitsbereichs (ohne abschließenden Schrägstrich).
    pub fn wurzel(&self) -> &str {
        &self.wurzel
    }

    /// Eine Outline-Methode aufrufen, z. B. `documents.info`.
    pub async fn aufrufen<T: DeserializeOwned>(
        &self,
        methode: &str,
        rumpf: Value,
    ) -> Result<Antwort<T>, OutlineFehler> {
        let adresse = format!("{}/api/{methode}", self.wurzel);
        let antwort = self
            .http
            .post(&adresse)
            .header("Accept", "application/json")
            .bearer_auth(&self.schluessel)
            .json(&rumpf)
            .send()
            .await
            .map_err(|fehler| self.netzfehler(methode, fehler))?;

        let status = antwort.status();
        let koerper = antwort
            .bytes()
            .await
            .map_err(|fehler| OutlineFehler::NichtErreichbar {
                wurzel: self.wurzel.clone(),
                ursache: self.ohne_schluessel(&fehler.to_string()),
            })?;

        let gelesen: Option<Rahmen<T>> = serde_json::from_slice(&koerper).ok();
        let ist_html = koerper
            .iter()
            .copied()
            .find(|byte| !byte.is_ascii_whitespace())
            == Some(b'<');

        if !status.is_success() {
            if ist_html {
                return Err(OutlineFehler::KeinOutline {
                    wurzel: self.wurzel.clone(),
                    status: status.as_u16(),
                });
            }
            return Err(self.statusfehler(methode, status.as_u16(), gelesen));
        }

        let rahmen = match gelesen {
            Some(rahmen) => rahmen,
            None if ist_html => {
                return Err(OutlineFehler::KeinOutline {
                    wurzel: self.wurzel.clone(),
                    status: status.as_u16(),
                });
            }
            None => {
                return Err(OutlineFehler::Unlesbar(
                    match std::str::from_utf8(&koerper) {
                        Ok(text) => self.ohne_schluessel(&kuerzen_roh(text, 200)),
                        Err(_) => "kein JSON".to_string(),
                    },
                ));
            }
        };

        if rahmen.ok == Some(false) {
            return Err(OutlineFehler::Meldung(
                self.ohne_schluessel(&detail(&rahmen.error, &rahmen.message)),
            ));
        }
        let daten = rahmen.data.ok_or_else(|| {
            OutlineFehler::Meldung(format!("{methode} hat keine Daten geliefert."))
        })?;
        Ok(Antwort {
            daten,
            seiten: rahmen.pagination,
        })
    }

    /// Ein Fehler mit Status: 401/403/429 bekommen einen Satz, der sagt, was zu
    /// tun ist; alles andere nennt Methode, Status und den Text von Outline.
    fn statusfehler<T>(
        &self,
        methode: &str,
        status: u16,
        rahmen: Option<Rahmen<T>>,
    ) -> OutlineFehler {
        // Nur Methode und Status: kein Rumpf, kein Schlüssel (SPEC §3).
        tracing::warn!(methode, status, wurzel = %self.wurzel, "Outline-Aufruf fehlgeschlagen");
        match status {
            401 => OutlineFehler::NichtAngemeldet,
            403 => OutlineFehler::Verboten {
                hinweis: self.hinweis(rahmen.as_ref()),
            },
            429 => OutlineFehler::Gedrosselt,
            sonst => OutlineFehler::Status {
                methode: methode.to_string(),
                status: sonst,
                hinweis: self.hinweis(rahmen.as_ref()),
            },
        }
    }

    fn hinweis<T>(&self, rahmen: Option<&Rahmen<T>>) -> String {
        let text = rahmen
            .map(|rahmen| detail(&rahmen.error, &rahmen.message))
            .unwrap_or_default();
        if text.is_empty() {
            String::new()
        } else {
            format!(": {}", self.ohne_schluessel(&kuerzen_roh(&text, 300)))
        }
    }

    fn netzfehler(&self, methode: &str, fehler: reqwest::Error) -> OutlineFehler {
        if fehler.is_timeout() {
            return OutlineFehler::Zeitlimit(ZEITLIMIT.as_secs());
        }
        let ursache = fehler.without_url().to_string();
        OutlineFehler::NichtErreichbar {
            wurzel: format!("{} (Aufruf {methode})", self.wurzel),
            ursache: self.ohne_schluessel(&ursache),
        }
    }

    /// Sicherheitsnetz (SPEC §3): der Schlüssel steht in keinem Fehlertext,
    /// auch wenn ein Server ihn zurückspiegelt.
    fn ohne_schluessel(&self, text: &str) -> String {
        if self.schluessel.is_empty() {
            text.to_string()
        } else {
            text.replace(&self.schluessel, "[geheim]")
        }
    }
}

/// Der Text, den Outline zu einem Fehler mitschickt (ohne Wertung).
fn detail(fehler: &Option<String>, meldung: &Option<String>) -> String {
    meldung
        .as_deref()
        .filter(|text| !text.trim().is_empty())
        .or(fehler.as_deref())
        .unwrap_or("")
        .trim()
        .to_string()
}

/// Für Meldungen aus fremden Texten: hart kürzen, ohne Rücksicht auf Wörter.
fn kuerzen_roh(text: &str, max: usize) -> String {
    let mut raus: String = text.chars().take(max).collect();
    if text.chars().count() > max {
        raus.push('…');
    }
    raus
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Profil;

    /// Kein echtes Geheimnis: nur ein Wert, der in keinem Text auftauchen darf.
    const GEHEIM: &str = "ol_api_test_4711";

    fn client() -> Client {
        let mut profil = Profil::leer();
        profil
            .werte
            .insert("url".to_string(), "https://example.test".to_string());
        profil
            .geheim
            .insert("api_key".to_string(), GEHEIM.to_string());
        Client::aus_profil(&profil).unwrap()
    }

    #[test]
    fn ohne_adresse_und_ohne_schluessel_kein_client() {
        let mut profil = Profil::leer();
        assert!(matches!(
            Client::aus_profil(&profil),
            Err(OutlineFehler::KeineAdresse)
        ));

        profil
            .werte
            .insert("url".to_string(), "https://example.test".to_string());
        assert!(matches!(
            Client::aus_profil(&profil),
            Err(OutlineFehler::KeinSchluessel)
        ));
    }

    #[test]
    fn ungueltige_adresse_wird_gemeldet_ohne_schluessel() {
        let mut profil = Profil::leer();
        profil
            .werte
            .insert("url".to_string(), "ht tp://example.test".to_string());
        profil
            .geheim
            .insert("api_key".to_string(), GEHEIM.to_string());
        let fehler = Client::aus_profil(&profil).unwrap_err().to_string();
        assert!(fehler.contains("keine gültige URL"), "{fehler}");
        assert!(!fehler.contains(GEHEIM));
    }

    #[test]
    fn die_wurzel_ist_normalisiert() {
        let mut profil = Profil::leer();
        profil
            .werte
            .insert("url".to_string(), "firma.getoutline.com/api/".to_string());
        profil
            .geheim
            .insert("api_key".to_string(), GEHEIM.to_string());
        let client = Client::aus_profil(&profil).unwrap();
        assert_eq!(client.wurzel(), "https://firma.getoutline.com");
    }

    #[test]
    fn debug_zeigt_keinen_schluessel() {
        let text = format!("{:?}", client());
        assert!(text.contains("example.test"));
        assert!(!text.contains(GEHEIM));
    }

    #[test]
    fn fehlertexte_verlieren_den_schluessel() {
        let client = client();
        assert_eq!(
            client.ohne_schluessel(&format!("Kopfzeile war Bearer {GEHEIM}")),
            "Kopfzeile war Bearer [geheim]"
        );
    }

    #[test]
    fn detail_bevorzugt_die_meldung() {
        assert_eq!(
            detail(&Some("code".to_string()), &Some("Text".to_string())),
            "Text"
        );
        assert_eq!(detail(&Some("code".to_string()), &None), "code");
        assert_eq!(detail(&None, &Some("  ".to_string())), "");
    }

    #[test]
    fn kuerzen_roh_setzt_ein_zeichen_ans_ende() {
        assert_eq!(kuerzen_roh("abcdef", 3), "abc…");
        assert_eq!(kuerzen_roh("abc", 3), "abc");
    }
}
