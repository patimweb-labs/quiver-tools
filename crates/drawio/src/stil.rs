// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Stil-Zeichenkette einer draw.io-Zelle.
//!
//! draw.io beschreibt das Aussehen in einem Attribut: `rounded=1;whiteSpace=wrap;html=1;`.
//! Die Form steht als erstes Wort ohne `=` (`rhombus`, `ellipse`, `swimlane`) oder
//! als `shape=…`. Alles andere sind Paare.
//!
//! [`Stil`] zerlegt die Kette in ihre Teile und setzt **einzelne** Teile. Was es
//! nicht kennt, bleibt stehen – ein Stil aus einer anderen draw.io-Version
//! überlebt so jede Änderung.

/// Eine Stil-Zeichenkette als Liste ihrer durch `;` getrennten Teile.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Stil {
    teile: Vec<String>,
}

impl Stil {
    /// Zerlegt eine Stil-Zeichenkette. Leere Teile bleiben erhalten, damit
    /// `"a=1;;b=2;"` Zeichen für Zeichen dieselbe Kette ergibt.
    pub fn aus_text(text: &str) -> Stil {
        Stil {
            teile: text.split(';').map(str::to_string).collect(),
        }
    }

    /// Die Kette wieder zusammengesetzt.
    pub fn als_text(&self) -> String {
        self.teile.join(";")
    }

    /// Das Formwort (erster Teil ohne `=`), z. B. `rhombus`.
    pub fn form(&self) -> Option<&str> {
        self.teile
            .iter()
            .map(|teil| teil.trim())
            .find(|teil| !teil.is_empty() && !teil.contains('='))
    }

    /// Ersetzt das Formwort; fehlt es, kommt es an den Anfang.
    pub fn setze_form(&mut self, form: &str) {
        match self
            .teile
            .iter()
            .position(|teil| !teil.trim().is_empty() && !teil.contains('='))
        {
            Some(stelle) => self.teile[stelle] = form.to_string(),
            None => self.teile.insert(0, form.to_string()),
        }
    }

    /// Entfernt jedes Formwort (Teile ohne `=`).
    pub fn entferne_form(&mut self) {
        self.teile
            .retain(|teil| teil.trim().is_empty() || teil.contains('='));
    }

    /// Der Wert eines Paares.
    pub fn wert(&self, schluessel: &str) -> Option<&str> {
        self.teile.iter().find_map(|teil| {
            let (name, wert) = teil.split_once('=')?;
            (name.trim() == schluessel).then_some(wert)
        })
    }

    /// Gibt es dieses Paar?
    pub fn hat(&self, schluessel: &str) -> bool {
        self.wert(schluessel).is_some()
    }

    /// Setzt ein Paar; ein vorhandenes wird an seiner Stelle ersetzt.
    ///
    /// Neue Paare landen vor den abschließenden leeren Teilen, damit
    /// `"a=1;"` zu `"a=1;b=2;"` wird und nicht zu `"a=1;;b=2"`.
    pub fn setze(&mut self, schluessel: &str, wert: &str) {
        let neu = format!("{schluessel}={wert}");
        if let Some(stelle) = self.teile.iter().position(|teil| {
            teil.split_once('=')
                .is_some_and(|(name, _)| name.trim() == schluessel)
        }) {
            self.teile[stelle] = neu;
            return;
        }
        let mut stelle = self.teile.len();
        while stelle > 0 && self.teile[stelle - 1].trim().is_empty() {
            stelle -= 1;
        }
        self.teile.insert(stelle, neu);
    }

    /// Entfernt ein Paar (alle Vorkommen).
    pub fn entferne(&mut self, schluessel: &str) {
        self.teile.retain(|teil| {
            !teil
                .split_once('=')
                .is_some_and(|(name, _)| name.trim() == schluessel)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unbekannte_teile_bleiben_stehen() {
        let mut stil = Stil::aus_text("rounded=1;whiteSpace=wrap;html=1;sketch=1;jiggle=2;");
        stil.setze("fillColor", "#e2efff");
        assert_eq!(
            stil.als_text(),
            "rounded=1;whiteSpace=wrap;html=1;sketch=1;jiggle=2;fillColor=#e2efff;"
        );
        assert_eq!(stil.wert("sketch"), Some("1"));
        stil.entferne("jiggle");
        assert_eq!(
            stil.als_text(),
            "rounded=1;whiteSpace=wrap;html=1;sketch=1;fillColor=#e2efff;"
        );
    }

    #[test]
    fn das_formwort_wird_ersetzt_und_entfernt() {
        let mut stil = Stil::aus_text("rhombus;whiteSpace=wrap;html=1;");
        assert_eq!(stil.form(), Some("rhombus"));
        stil.setze_form("ellipse");
        assert_eq!(stil.als_text(), "ellipse;whiteSpace=wrap;html=1;");
        stil.entferne_form();
        assert_eq!(stil.als_text(), "whiteSpace=wrap;html=1;");
        assert_eq!(stil.form(), None);
        // Ein Formwort an den Anfang, wenn keines da ist.
        let mut ohne = Stil::aus_text("rounded=1;html=1;");
        ohne.setze_form("rhombus");
        assert_eq!(ohne.als_text(), "rhombus;rounded=1;html=1;");
    }

    #[test]
    fn setze_ersetzt_das_erste_vorkommen() {
        let mut stil = Stil::aus_text("fillColor=#fff;fillColor=#000;");
        stil.setze("fillColor", "#123456");
        assert_eq!(stil.als_text(), "fillColor=#123456;fillColor=#000;");
    }

    #[test]
    fn ein_leerer_stil_bleibt_leer() {
        let stil = Stil::aus_text("");
        assert_eq!(stil.als_text(), "");
        assert_eq!(stil.form(), None);
        assert_eq!(stil.wert("x"), None);
    }
}
