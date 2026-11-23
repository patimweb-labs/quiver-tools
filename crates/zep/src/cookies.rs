// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Ein kleiner Cookie-Speicher für den Anmelde-Ablauf.
//!
//! Der Ablauf (Login-Formular, Zustimmungsseite, Weiterleitung) hängt an
//! Sitzungs-Cookies. Weil kein Browser im Spiel ist, merkt sich dieser Speicher
//! `Set-Cookie` und schickt die passenden `Cookie`-Kopfzeilen zurück. Mehr kann
//! er nicht – und mehr braucht er nicht.

use url::Url;

/// Ein gespeicherter Keks.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Keks {
    name: String,
    wert: String,
    /// Ohne `.Domain` gilt der Host der Antwort.
    domain: Option<String>,
    pfad: String,
    /// `true` für `Secure`: nur über https senden.
    sicher: bool,
}

/// Alle Kekse eines Anmelde-Ablaufs.
#[derive(Debug, Clone, Default)]
pub struct Kekse {
    eintraege: Vec<Keks>,
}

impl Kekse {
    /// Einen `Set-Cookie`-Kopf von einer Antwort auf `url` merken.
    ///
    /// Leere oder unlesbare Zeilen werden stillschweigend übergangen: ein
    /// Anmeldedienst, der etwas Schräges schickt, soll den Ablauf nicht mit
    /// einem Parserfehler beenden.
    pub fn merken(&mut self, url: &Url, set_cookie: &str) {
        let mut teile = set_cookie.split(';');
        let Some(paar) = teile.next() else {
            return;
        };
        let Some((name, wert)) = paar.split_once('=') else {
            return;
        };
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let wert = wert.trim().to_string();

        // Ohne `Domain` gilt der Host, der die Antwort geschickt hat – ein Keks
        // darf nie zu einem anderen Host wandern.
        let mut domain = url.host_str().map(|host| host.to_ascii_lowercase());
        let mut pfad = "/".to_string();
        let mut sicher = false;
        for anweisung in teile {
            let (schluessel, wert) = match anweisung.split_once('=') {
                Some((schluessel, wert)) => (schluessel.trim().to_ascii_lowercase(), wert.trim()),
                None => (anweisung.trim().to_ascii_lowercase(), ""),
            };
            match schluessel.as_str() {
                "domain" => {
                    let ohne_punkt = wert.trim_start_matches('.').to_ascii_lowercase();
                    if !ohne_punkt.is_empty() {
                        domain = Some(ohne_punkt);
                    }
                }
                "path" => {
                    if wert.starts_with('/') {
                        pfad = wert.to_string();
                    }
                }
                "secure" => sicher = true,
                // `Max-Age=0` und `Expires` in der Vergangenheit löschen den
                // Keks. Für einen Ablauf von wenigen Sekunden genügt: alles
                // andere ignorieren, ein `Max-Age=0` entfernt den Eintrag.
                "max-age" if wert.trim() == "0" => {
                    self.eintraege
                        .retain(|keks| keks.name != name || keks.pfad != pfad);
                }
                _ => {}
            }
        }

        self.eintraege
            .retain(|keks| !(keks.name == name && keks.pfad == pfad && keks.domain == domain));
        self.eintraege.push(Keks {
            name: name.to_string(),
            wert,
            domain,
            pfad,
            sicher,
        });
    }

    /// Der `Cookie`-Kopf für einen Aufruf auf `url`, falls Kekse passen.
    pub fn kopf(&self, url: &Url) -> Option<String> {
        let host = url.host_str()?.to_ascii_lowercase();
        let pfad = url.path();
        let sicher = url.scheme() == "https";

        let passend: Vec<String> = self
            .eintraege
            .iter()
            .filter(|keks| {
                let domain = keks.domain.clone().unwrap_or_else(|| host.clone());
                let host_passt = host == domain || host.ends_with(&format!(".{domain}"));
                let pfad_passt = pfad.starts_with(&keks.pfad);
                host_passt && pfad_passt && (sicher || !keks.sicher)
            })
            .map(|keks| format!("{}={}", keks.name, keks.wert))
            .collect();

        if passend.is_empty() {
            None
        } else {
            Some(passend.join("; "))
        }
    }

    /// Wie viele Kekse gespeichert sind (nur für Meldungen und Tests).
    pub fn anzahl(&self) -> usize {
        self.eintraege.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(text: &str) -> Url {
        Url::parse(text).unwrap()
    }

    #[test]
    fn kekse_gehen_nur_an_ihren_host_und_pfad() {
        let mut kekse = Kekse::default();
        let anmelden = url("https://zep.example.test/zep/next/oauth/authorize");
        kekse.merken(&anmelden, "sitzung=abc123; Path=/zep; HttpOnly; Secure");
        kekse.merken(&anmelden, "weit=1; Path=/");

        assert_eq!(kekse.anzahl(), 2);
        assert_eq!(
            kekse
                .kopf(&url("https://zep.example.test/zep/next/oauth/token"))
                .as_deref(),
            Some("sitzung=abc123; weit=1")
        );
        // Anderer Pfad: nur der Keks mit `/`.
        assert_eq!(
            kekse
                .kopf(&url("https://zep.example.test/anders"))
                .as_deref(),
            Some("weit=1")
        );
        // Anderer Host: nichts.
        assert_eq!(kekse.kopf(&url("https://boese.example.test/zep")), None);
    }

    #[test]
    fn domain_gilt_auch_fuer_untergeordnete_hosts() {
        let mut kekse = Kekse::default();
        kekse.merken(
            &url("https://www.example.test/"),
            "a=b; Domain=.example.test; Path=/",
        );
        assert_eq!(
            kekse.kopf(&url("https://zep.example.test/x")).as_deref(),
            Some("a=b")
        );
    }

    #[test]
    fn secure_geht_nur_ueber_https() {
        let mut kekse = Kekse::default();
        kekse.merken(&url("https://example.test/"), "s=1; Secure; Path=/");
        assert_eq!(kekse.kopf(&url("http://example.test/")), None);
        assert_eq!(
            kekse.kopf(&url("https://example.test/")).as_deref(),
            Some("s=1")
        );
    }

    #[test]
    fn derselbe_name_ersetzt_den_alten_wert() {
        let mut kekse = Kekse::default();
        let ziel = url("https://example.test/");
        kekse.merken(&ziel, "a=alt; Path=/");
        kekse.merken(&ziel, "a=neu; Path=/");
        assert_eq!(kekse.anzahl(), 1);
        assert_eq!(kekse.kopf(&ziel).as_deref(), Some("a=neu"));
    }

    #[test]
    fn kaputte_zeilen_werden_uebergangen() {
        let mut kekse = Kekse::default();
        let ziel = url("https://example.test/");
        kekse.merken(&ziel, "ohne-gleichheitszeichen");
        kekse.merken(&ziel, "=wert");
        kekse.merken(&ziel, "");
        assert_eq!(kekse.anzahl(), 0);
    }
}
