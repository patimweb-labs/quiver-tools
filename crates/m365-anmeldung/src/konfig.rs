// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Konfiguration aus der Umgebung (SPEC.md §3).

use std::time::Duration;

use crate::{Konfig, STANDARD_SCOPES};

/// Standard-Mandant, wenn `M365_TENANT` fehlt.
pub const STANDARD_TENANT: &str = "organizations";
/// Die Meldung, wenn die Anwendungs-ID fehlt.
pub const OHNE_ID: &str = "Für Microsoft 365 fehlt die Anwendungs-ID (M365_CLIENT_ID). Sie kommt \
                           von deiner Firma – bitte in der Verwaltung beim Eintrag „Microsoft 365“ \
                           vorgeben lassen.";
/// Standard-Autorität, wenn `M365_AUTHORITY` fehlt.
pub const STANDARD_AUTHORITY: &str = "https://login.microsoftonline.com";
/// Standard: so lange wartet ein Tool-Aufruf auf die Anmeldung im Browser.
pub const STANDARD_WARTEN: Duration = Duration::from_secs(45);
/// Standard: so lange bleibt der Lauscher im Hintergrund höchstens offen.
pub const STANDARD_LAUSCHER: Duration = Duration::from_secs(300);

impl Konfig {
    /// Liest die Konfiguration aus der Umgebung. `M365_CLIENT_ID` ist **Pflicht**:
    /// die Anwendungs-ID der App-Registrierung kommt von der Firma (in quiver
    /// über die Verwaltung), nicht aus dem Programm (SPEC.md §2a).
    ///
    /// Getrimmte, leere Werte gelten als nicht gesetzt.
    pub fn aus_umgebung() -> anyhow::Result<Konfig> {
        let konfig = Self::aus_umgebung_ohne_id();
        if konfig.client_id.is_empty() {
            anyhow::bail!(OHNE_ID);
        }
        Ok(konfig)
    }

    /// Wie [`Konfig::aus_umgebung`], aber eine fehlende Anwendungs-ID ist kein
    /// Fehler: der MCP-Dienst startet trotzdem, und jeder Aufruf, der eine
    /// Anmeldung braucht, antwortet mit [`OHNE_ID`]. So zeigt die Karte im
    /// quiver-Fenster den Grund statt „startet nicht“.
    pub fn aus_umgebung_ohne_id() -> Konfig {
        let client_id = wert("M365_CLIENT_ID").unwrap_or_default();
        Konfig {
            client_id,
            tenant: wert("M365_TENANT").unwrap_or_else(|| STANDARD_TENANT.to_string()),
            scopes: scopes(wert("M365_SCOPES")),
            authority: wert("M365_AUTHORITY")
                .unwrap_or_else(|| STANDARD_AUTHORITY.to_string())
                .trim_end_matches('/')
                .to_string(),
            warten: STANDARD_WARTEN,
            lauscher_hoechstens: STANDARD_LAUSCHER,
        }
    }
}

fn wert(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|roh| roh.trim().to_string())
        .filter(|gekuerzt| !gekuerzt.is_empty())
}

/// `Mail.Read Mail.Send` oder `Mail.Read,Mail.Send` → Liste; leer → Standard aus SPEC §4.
fn scopes(roh: Option<String>) -> Vec<String> {
    match roh {
        Some(roh) => {
            let liste: Vec<String> = roh
                .split([' ', ',', '\t', '\n'])
                .map(str::trim)
                .filter(|teil| !teil.is_empty())
                .map(|teil| teil.to_string())
                .collect();
            if liste.is_empty() {
                standard_scopes()
            } else {
                liste
            }
        }
        None => standard_scopes(),
    }
}

fn standard_scopes() -> Vec<String> {
    STANDARD_SCOPES.iter().map(|s| s.to_string()).collect()
}
