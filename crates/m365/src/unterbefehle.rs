// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Unterbefehle `anmelden`, `status`, `abmelden` (Aufgabe 03).
//!
//! Die Anmeldung löst der User im quiver-Fenster aus (Knopf „Anmelden“); mcp-quiver
//! ruft dafür diese drei Unterbefehle von `quiver-m365` auf. Deshalb sind sie hier
//! reine Funktionen, die einen [`Ausgang`] liefern – `main` gibt ihn nur aus und
//! macht daraus den Exit-Code. So sind die Befehle ohne Prozessstart prüfbar.
//!
//! * `anmelden` – Browser-Ablauf (hier **5 min** Wartezeit), danach `GET /me`.
//! * `status`   – **ohne** Browser: Refresh-Token erneuern und `GET /me`.
//! * `abmelden` – Refresh-Token löschen, ohne Netz.
//!
//! Kein Token in einer Ausgabe: stdout trägt das Konto (`userPrincipalName`),
//! stderr den Fehlertext.

use std::time::Duration;

use m365_anmeldung::{
    AnmeldeFehler, Anmeldung, Browser, EntraAnmeldung, KeinBrowser, Konfig, Tresor,
};
use m365_graph::{GraphClient, GraphFehler, Profil};

/// So lange wartet `quiver-m365 anmelden` auf die Rückleitung aus dem Browser
/// (Aufgabe 03: 5 min statt 45 s – der User sitzt davor, nicht ein Agent).
pub const ANMELDEN_WARTEN: Duration = Duration::from_secs(300);

/// Feldauswahl für `GET /me`: nur, was die Ausgabe braucht.
const PROFIL_FELDER: &str = "displayName,mail,userPrincipalName";

/// Was ein Unterbefehl ausgibt und welchen Exit-Code er setzt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ausgang {
    /// Text für stdout (ohne Zeilenende), `None` = nichts.
    pub stdout: Option<String>,
    /// Text für stderr (ohne Zeilenende), `None` = nichts.
    pub stderr: Option<String>,
    /// 0 = in Ordnung, 1 = nicht angemeldet, 2 = Fehler.
    pub code: u8,
}

impl Ausgang {
    /// Etwas gelungen (Konto, Meldung) → Exit 0.
    pub fn erfolg(text: impl Into<String>) -> Self {
        Self {
            stdout: Some(text.into()),
            stderr: None,
            code: 0,
        }
    }

    /// Gelungen ohne Text → Exit 0.
    pub fn erfolg_ohne_text() -> Self {
        Self {
            stdout: None,
            stderr: None,
            code: 0,
        }
    }

    /// Kein (gültiges) Refresh-Token → Exit 1.
    pub fn nicht_angemeldet() -> Self {
        Self {
            stdout: None,
            stderr: None,
            code: 1,
        }
    }

    /// Fehler → Exit 2, Text nach stderr.
    pub fn fehler(text: impl Into<String>) -> Self {
        Self {
            stdout: None,
            stderr: Some(text.into()),
            code: 2,
        }
    }
}

/// Warum kein Konto gelesen werden konnte.
#[derive(Debug, Clone, PartialEq, Eq)]
enum KontoFehler {
    /// Kein gültiges Refresh-Token (oder `invalid_grant`): der User muss sich anmelden.
    NichtAngemeldet,
    /// Netz, Graph, kaputte Antwort – als Text, ohne Token.
    Fehler(String),
}

/// `quiver-m365 anmelden`: Browser-Ablauf, danach das Konto ausgeben.
pub async fn anmelden<T, B>(konfig: Konfig, graph_url: &str, tresor: T, browser: B) -> Ausgang
where
    T: Tresor + 'static,
    B: Browser + 'static,
{
    let konfig = Konfig {
        warten: ANMELDEN_WARTEN,
        ..konfig
    };

    match konto_lesen(konfig, graph_url, tresor, browser, false).await {
        Ok(konto) => Ausgang::erfolg(konto),
        // Ein Fehler beim Anmelden ist ein Fehler, keine Zustandsfrage.
        Err(KontoFehler::NichtAngemeldet) => {
            Ausgang::fehler("Anmeldung nicht abgeschlossen – bitte erneut versuchen")
        }
        Err(KontoFehler::Fehler(text)) => Ausgang::fehler(text),
    }
}

/// `quiver-m365 status`: ohne Browser prüfen, ob das Refresh-Token noch trägt.
pub async fn status<T>(konfig: Konfig, graph_url: &str, tresor: T) -> Ausgang
where
    T: Tresor + 'static,
{
    // Kein Browser und kein Lauscher: `status` öffnet nie etwas.
    match konto_lesen(konfig, graph_url, tresor, KeinBrowser::neu(), true).await {
        Ok(konto) => Ausgang::erfolg(konto),
        Err(KontoFehler::NichtAngemeldet) => Ausgang::nicht_angemeldet(),
        Err(KontoFehler::Fehler(text)) => Ausgang::fehler(text),
    }
}

/// `quiver-m365 abmelden`: Refresh-Token löschen.
pub fn abmelden<T>(konfig: Konfig, tresor: T) -> Ausgang
where
    T: Tresor + 'static,
{
    let anmeldung = match anmeldung_bauen(konfig, tresor, KeinBrowser::neu(), true) {
        Ok(anmeldung) => anmeldung,
        Err(KontoFehler::Fehler(text)) => return Ausgang::fehler(text),
        // Ohne Browser ist `NichtAngemeldet` hier kein Sonderfall: es wird nichts
        // angefordert, nur gelöscht.
        Err(KontoFehler::NichtAngemeldet) => return Ausgang::erfolg_ohne_text(),
    };

    match anmeldung.abmelden() {
        Ok(()) => Ausgang::erfolg("Abgemeldet"),
        Err(fehler) => Ausgang::fehler(format!("Abmelden: {fehler}")),
    }
}

/// Baut die Anmeldung für einen Unterbefehl.
///
/// Bewusst [`EntraAnmeldung::mit_festem_token`] mit `None`: die Unterbefehle
/// verwalten das **eigene** Refresh-Token im Tresor. Ein von außen gereichtes
/// `M365_ACCESS_TOKEN` gilt nur im MCP-Betrieb (SPEC.md §2 „Später“).
fn anmeldung_bauen<T, B>(
    konfig: Konfig,
    tresor: T,
    browser: B,
    ohne_browser: bool,
) -> Result<EntraAnmeldung<T, B>, KontoFehler>
where
    T: Tresor + 'static,
    B: Browser + 'static,
{
    let anmeldung = EntraAnmeldung::mit_festem_token(konfig, tresor, browser, None)
        .map_err(|fehler| KontoFehler::Fehler(format!("Anmeldung baut nicht: {fehler}")))?;
    Ok(if ohne_browser {
        anmeldung.ohne_browser()
    } else {
        anmeldung
    })
}

/// `GET /me` über die Anmeldung; liefert das Konto als eine Zeile.
async fn konto_lesen<T, B>(
    konfig: Konfig,
    graph_url: &str,
    tresor: T,
    browser: B,
    ohne_browser: bool,
) -> Result<String, KontoFehler>
where
    T: Tresor + 'static,
    B: Browser + 'static,
{
    let anmeldung = anmeldung_bauen(konfig, tresor, browser, ohne_browser)?;
    let graph = GraphClient::neu(graph_url, anmeldung)
        .map_err(|fehler| KontoFehler::Fehler(format!("Graph-Client baut nicht: {fehler}")))?;

    let abfrage = graph
        .get::<Profil>("/me", &[("$select", PROFIL_FELDER.to_string())])
        .await;

    match abfrage {
        Ok(profil) => konto_text(&profil)
            .ok_or_else(|| KontoFehler::Fehler("Konto ohne userPrincipalName".to_string())),
        // Kein Token, keins erneuerbar, kein Browser: der User ist nicht angemeldet.
        Err(GraphFehler::Anmeldung(AnmeldeFehler::NichtAngemeldet))
        | Err(GraphFehler::AnmeldungLaeuft) => Err(KontoFehler::NichtAngemeldet),
        Err(fehler) => Err(KontoFehler::Fehler(format!("{fehler}"))),
    }
}

/// Das Konto als eine Zeile: `userPrincipalName`, sonst die Mail-Adresse.
fn konto_text(profil: &Profil) -> Option<String> {
    [
        profil.user_principal_name.as_deref(),
        profil.mail.as_deref(),
    ]
    .into_iter()
    .flatten()
    .map(str::trim)
    .find(|text| !text.is_empty())
    .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profil(upn: Option<&str>, mail: Option<&str>) -> Profil {
        Profil {
            id: Some("u-anna".to_string()),
            display_name: Some("Anna Muster".to_string()),
            mail: mail.map(str::to_string),
            user_principal_name: upn.map(str::to_string),
            department: None,
        }
    }

    #[test]
    fn konto_ist_der_upn_sonst_die_mail() {
        assert_eq!(
            konto_text(&profil(Some("anna@firma.de"), Some("a@firma.de"))).as_deref(),
            Some("anna@firma.de")
        );
        assert_eq!(
            konto_text(&profil(None, Some("a@firma.de"))).as_deref(),
            Some("a@firma.de")
        );
        assert_eq!(konto_text(&profil(Some("  "), Some(" "))), None);
        assert_eq!(konto_text(&profil(None, None)), None);
    }

    #[test]
    fn exit_codes_sind_die_aus_der_aufgabe() {
        assert_eq!(Ausgang::erfolg("anna@firma.de").code, 0);
        assert_eq!(Ausgang::erfolg_ohne_text().code, 0);
        assert_eq!(Ausgang::nicht_angemeldet().code, 1);
        let fehler = Ausgang::fehler("kaputt");
        assert_eq!(fehler.code, 2);
        assert_eq!(fehler.stderr.as_deref(), Some("kaputt"));
        assert!(fehler.stdout.is_none());
    }

    /// Ohne Browser ist der Weg zum Konto zu, sobald kein Refresh-Token da ist.
    #[tokio::test]
    async fn ohne_refresh_token_ist_status_nicht_angemeldet() {
        let ausgang = status(
            test_konfig(),
            "http://127.0.0.1:9/v1.0",
            m365_anmeldung::SpeicherTresor::neu(),
        )
        .await;
        assert_eq!(ausgang.code, 1);
    }

    fn test_konfig() -> Konfig {
        Konfig {
            client_id: "test-client".to_string(),
            tenant: "test-tenant".to_string(),
            scopes: vec!["User.Read".to_string()],
            authority: "http://127.0.0.1:9".to_string(),
            warten: Duration::from_secs(1),
            lauscher_hoechstens: Duration::from_secs(1),
        }
    }
}
