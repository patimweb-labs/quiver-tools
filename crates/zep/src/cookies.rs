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
        // Ohne `Path` gilt wie im Browser das Verzeichnis der Anfrage
        // (RFC 6265, 5.1.4) – nicht `/`. Sonst landet ein Sitzungs-Keks von
        // `/a/view/login.php` auch bei `/a/next/…` und verdrängt dort den
        // richtigen.
        let mut pfad = standardpfad(url.path());
        let mut sicher = false;
        let mut geloescht = false;
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
                // `Max-Age` ≤ 0 oder ein `Expires` in der Vergangenheit löscht
                // den Keks – egal, an welcher Stelle der Zeile es steht.
                "max-age" if wert.parse::<i64>().is_ok_and(|sekunden| sekunden <= 0) => {
                    geloescht = true;
                }
                "expires" if abgelaufen(wert) => geloescht = true,
                _ => {}
            }
        }

        self.eintraege
            .retain(|keks| !(keks.name == name && keks.pfad == pfad && keks.domain == domain));
        if geloescht {
            return;
        }
        self.eintraege.push(Keks {
            name: name.to_string(),
            wert,
            domain,
            pfad,
            sicher,
        });
    }

    /// Die Namen der Kekse, die an `url` gingen – ohne Werte.
    pub fn namen_fuer(&self, url: &Url) -> String {
        match self.kopf(url) {
            None => "keine".to_string(),
            Some(kopf) => kopf
                .split("; ")
                .filter_map(|paar| paar.split_once('=').map(|(name, _)| name.to_string()))
                .collect::<Vec<_>>()
                .join(", "),
        }
    }

    /// Namen, Domain und Pfad der gespeicherten Kekse – **ohne Werte** – für
    /// den Bericht der Routine.
    pub fn uebersicht(&self) -> String {
        if self.eintraege.is_empty() {
            return "keine".to_string();
        }
        self.eintraege
            .iter()
            .map(|keks| {
                format!(
                    "{} ({}{})",
                    keks.name,
                    keks.domain.as_deref().unwrap_or("?"),
                    keks.pfad
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Der `Cookie`-Kopf für einen Aufruf auf `url`, falls Kekse passen.
    pub fn kopf(&self, url: &Url) -> Option<String> {
        let host = url.host_str()?.to_ascii_lowercase();
        let pfad = url.path();
        let sicher = url.scheme() == "https";

        let mut passend: Vec<&Keks> = self
            .eintraege
            .iter()
            .filter(|keks| {
                let domain = keks.domain.clone().unwrap_or_else(|| host.clone());
                let host_passt = host == domain || host.ends_with(&format!(".{domain}"));
                host_passt && pfad_passt(pfad, &keks.pfad) && (sicher || !keks.sicher)
            })
            .collect();
        // Wie im Browser: der längere Pfad zuerst (RFC 6265, 5.4). Bei zwei
        // Keksen gleichen Namens nimmt der Dienst meist den ersten.
        passend.sort_by_key(|keks| std::cmp::Reverse(keks.pfad.len()));
        let passend: Vec<String> = passend
            .into_iter()
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

/// Der Standardpfad eines Kekses: das Verzeichnis des Anfragepfads.
fn standardpfad(anfrage: &str) -> String {
    if !anfrage.starts_with('/') {
        return "/".to_string();
    }
    match anfrage.rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(stelle) => anfrage[..stelle].to_string(),
    }
}

/// Passt der Anfragepfad zum Pfad des Kekses (RFC 6265, 5.1.4)?
/// `/zep` passt zu `/zep` und `/zep/x`, nicht aber zu `/zepx`.
fn pfad_passt(anfrage: &str, keks: &str) -> bool {
    if anfrage == keks {
        return true;
    }
    anfrage.starts_with(keks) && (keks.ends_with('/') || anfrage[keks.len()..].starts_with('/'))
}

/// Liegt ein `Expires`-Datum in der Vergangenheit? Unlesbare Daten zählen
/// nicht als abgelaufen.
fn abgelaufen(datum: &str) -> bool {
    httpdate::parse_http_date(datum.trim())
        .or_else(|_| httpdate::parse_http_date(&datum.trim().replace('-', " ")))
        .is_ok_and(|zeit| zeit <= std::time::SystemTime::now())
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
    fn ohne_path_gilt_das_verzeichnis_der_anfrage() {
        let mut kekse = Kekse::default();
        kekse.merken(
            &url("https://www.example.test/a/view/login.php?x=1"),
            "sitzung=neu",
        );
        assert_eq!(
            kekse
                .kopf(&url("https://www.example.test/a/view/start.php"))
                .as_deref(),
            Some("sitzung=neu")
        );
        assert_eq!(
            kekse.kopf(&url("https://www.example.test/a/next/authorize")),
            None
        );
        assert_eq!(kekse.kopf(&url("https://www.example.test/a/viewer")), None);
    }

    #[test]
    fn der_laengere_pfad_geht_zuerst() {
        let mut kekse = Kekse::default();
        kekse.merken(&url("https://www.example.test/"), "s=kurz; Path=/");
        kekse.merken(
            &url("https://www.example.test/"),
            "s=lang; Path=/a/; Domain=.example.test",
        );
        assert_eq!(
            kekse
                .kopf(&url("https://www.example.test/a/next"))
                .as_deref(),
            Some("s=lang; s=kurz")
        );
    }

    #[test]
    fn geloeschte_kekse_werden_nicht_gespeichert() {
        let mut kekse = Kekse::default();
        let ziel = url("https://example.test/a/");
        kekse.merken(&ziel, "s=echt; Path=/a/");
        // PHP-Art: Max-Age vor Path, Wert „deleted“.
        kekse.merken(
            &ziel,
            "s=deleted; expires=Thu, 01-Jan-1970 00:00:01 GMT; Max-Age=0; path=/a/",
        );
        assert_eq!(kekse.anzahl(), 0);
        kekse.merken(&ziel, "t=1; Path=/a/");
        kekse.merken(
            &ziel,
            "t=weg; Expires=Thu, 01 Jan 1970 00:00:01 GMT; Path=/a/",
        );
        assert_eq!(kekse.anzahl(), 0);
        // Ein Datum in der Zukunft bleibt.
        kekse.merken(
            &ziel,
            "u=1; Expires=Fri, 01 Jan 2100 00:00:00 GMT; Path=/a/",
        );
        assert_eq!(kekse.anzahl(), 1);
    }

    #[test]
    fn die_uebersicht_zeigt_keine_werte() {
        let mut kekse = Kekse::default();
        kekse.merken(
            &url("https://www.example.test/"),
            "PHPSESSID=geheim123; path=/a/; domain=.example.test",
        );
        let text = kekse.uebersicht();
        assert!(text.contains("PHPSESSID (example.test/a/)"), "{text}");
        assert!(!text.contains("geheim123"));
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
