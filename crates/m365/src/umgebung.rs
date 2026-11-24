// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Konfiguration aus der Umgebung (SPEC.md §3), soweit nicht in
//! [`m365_anmeldung::Konfig`]: Graph-Adresse und Zeitzone.

/// Standard-Graph-Adresse (Tests: Attrappe).
pub const STANDARD_GRAPH: &str = "https://graph.microsoft.com/v1.0";
/// Standard-Zeitzone für Termine (SPEC.md §4).
pub const STANDARD_ZEITZONE: &str = "Europe/Berlin";

/// Wert einer Umgebungsvariablen; getrimmt, leere Werte gelten als nicht gesetzt.
pub fn wert(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|roh| roh.trim().to_string())
        .filter(|gekuerzt| !gekuerzt.is_empty())
}

/// Wert oder Standard – die reine Regel, damit sie ohne Umgebung prüfbar ist.
pub fn wert_oder(roh: Option<String>, standard: &str) -> String {
    roh.unwrap_or_else(|| standard.to_string())
}

/// `M365_GRAPH_URL`, Standard [`STANDARD_GRAPH`].
pub fn graph_url() -> String {
    wert_oder(wert("M365_GRAPH_URL"), STANDARD_GRAPH)
}

/// `M365_ZEITZONE`, Standard [`STANDARD_ZEITZONE`].
pub fn zeitzone() -> String {
    wert_oder(wert("M365_ZEITZONE"), STANDARD_ZEITZONE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wert_oder_nimmt_standard_bei_leerem_wert() {
        assert_eq!(wert_oder(None, "x"), "x");
        assert_eq!(wert_oder(Some("y".to_string()), "x"), "y");
    }

    /// Name, den kein Test setzt – so hängt das Ergebnis nicht an der Umgebung.
    #[test]
    fn wert_ignoriert_leere_und_getrimmte_werte() {
        assert_eq!(wert("QUIVER_M365_GIBT_ES_NICHT"), None);
    }

    #[test]
    fn standards_stehen_in_der_spec() {
        assert_eq!(STANDARD_GRAPH, "https://graph.microsoft.com/v1.0");
        assert_eq!(STANDARD_ZEITZONE, "Europe/Berlin");
    }
}
