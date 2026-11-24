// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Was beim Reden mit Graph schiefgehen kann (SPEC.md §4: Fehler mit Graph-Code
//! und Text, **ohne** Tokens).

use m365_anmeldung::AnmeldeFehler;

#[derive(Debug, thiserror::Error)]
pub enum GraphFehler {
    /// Die Anmeldung läuft im Browser. Das Tool macht daraus den Hinweistext
    /// aus SPEC.md §2, keinen Fehler.
    #[error("Anmeldung im Browser geöffnet")]
    AnmeldungLaeuft,
    /// Die Anmeldung selbst ist gescheitert (Text ohne Token).
    #[error("Anmeldung fehlgeschlagen: {0}")]
    Anmeldung(#[from] AnmeldeFehler),
    /// Graph hat mit einem Fehlerobjekt geantwortet: `{"error":{"code","message"}}`.
    #[error("Graph-Fehler {status} ({code}): {text}")]
    Graph {
        status: u16,
        code: String,
        text: String,
    },
    /// Netz, Zeitüberschreitung, Verbindung weg.
    #[error("Graph nicht erreichbar: {0}")]
    Netz(String),
    /// Antwort, die nicht zum erwarteten Modell passt.
    #[error("Antwort von Graph unbrauchbar: {0}")]
    Antwort(String),
}

impl GraphFehler {
    /// Fehlt die Zustimmung für einen neuen Scope (Teams)?
    ///
    /// Zwei Wege führen hierher: der Token-Endpunkt sagt `AADSTS65001` /
    /// `consent_required` (dann steht das schon in der Anmeldung), oder Graph
    /// antwortet mit 403 und `Missing scope`. Beides ist kein Fehler im Ablauf,
    /// sondern eine Aufgabe für den User (Aufgabe 05).
    pub fn zustimmung_fehlt(&self) -> bool {
        match self {
            GraphFehler::Anmeldung(AnmeldeFehler::ZustimmungFehlt) => true,
            GraphFehler::Graph { status, code, text } => {
                *status == 403
                    && (code.to_lowercase().contains("missingscope")
                        || text.to_lowercase().contains("missing scope"))
            }
            _ => false,
        }
    }
}
