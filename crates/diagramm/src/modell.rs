// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Das Modell eines Diagramms: Beschreibung statt Koordinaten.
//!
//! Ein Agent liefert Knoten, Kanten und Gruppen; wo sie liegen, entscheidet
//! [`crate::layout`]. Alles hier ist reine Datenhaltung mit serde: dieselbe
//! Struktur kommt als JSON herein, wie sie später in den Werkzeugen steht.
//!
//! ```json
//! {
//!   "richtung": "unten",
//!   "knoten": [
//!     { "id": "start", "text": "Antrag", "form": "ellipse", "farbe": "blau" },
//!     { "id": "pruefen", "text": "Prüfung", "form": "raute" }
//!   ],
//!   "kanten": [ { "von": "start", "nach": "pruefen", "text": "eingereicht" } ],
//!   "gruppen": [ { "id": "vorgang", "titel": "Vorgang" } ]
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;

/// Höchstzahl Knoten je Diagramm (Aufgabe 16).
pub const MAX_KNOTEN: usize = 300;
/// Höchstzahl Kanten je Diagramm (Aufgabe 16).
pub const MAX_KANTEN: usize = 600;
/// Höchstlänge eines Textes in Zeichen (Knoten, Kante, Gruppe).
pub const MAX_TEXT: usize = 2000;

/// Leserichtung des geschichteten Layouts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Richtung {
    /// Schichten von oben nach unten.
    #[default]
    Unten,
    /// Schichten von links nach rechts.
    Rechts,
}

impl Richtung {
    /// Alle Werte, in der Reihenfolge der Dokumentation.
    pub const ALLE: [Richtung; 2] = [Richtung::Unten, Richtung::Rechts];

    /// Name im Modell-JSON (`unten`, `rechts`).
    pub fn name(self) -> &'static str {
        match self {
            Richtung::Unten => "unten",
            Richtung::Rechts => "rechts",
        }
    }

    /// Umgekehrter Weg: Name aus dem Modell-JSON.
    pub fn aus_name(name: &str) -> Option<Richtung> {
        Richtung::ALLE.into_iter().find(|r| r.name() == name)
    }
}

/// Form eines Knotens. Die Auswahl ist fest; freie Formen gibt es nicht.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Form {
    /// Rechteck (Vorgang, Schritt).
    #[default]
    Rechteck,
    /// Raute (Entscheidung).
    Raute,
    /// Ellipse (Start, Ende).
    Ellipse,
    /// Zylinder (Daten, Speicher).
    Zylinder,
    /// Notiz (Kommentar, Merkzettel).
    Notiz,
}

impl Form {
    /// Alle Werte, in der Reihenfolge der Dokumentation.
    pub const ALLE: [Form; 5] = [
        Form::Rechteck,
        Form::Raute,
        Form::Ellipse,
        Form::Zylinder,
        Form::Notiz,
    ];

    /// Name im Modell-JSON (`rechteck`, `raute`, …).
    pub fn name(self) -> &'static str {
        match self {
            Form::Rechteck => "rechteck",
            Form::Raute => "raute",
            Form::Ellipse => "ellipse",
            Form::Zylinder => "zylinder",
            Form::Notiz => "notiz",
        }
    }

    /// Umgekehrter Weg: Name aus dem Modell-JSON.
    pub fn aus_name(name: &str) -> Option<Form> {
        Form::ALLE.into_iter().find(|f| f.name() == name)
    }
}

/// Art einer Kante.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum KanteArt {
    /// Durchgezogene Linie mit Pfeilspitze am Ziel (Ablauf).
    #[default]
    Pfeil,
    /// Durchgezogene Linie ohne Pfeilspitze (Verbindung).
    Linie,
    /// Gestrichelte Linie mit Pfeilspitze (indirekt, optional).
    Gestrichelt,
}

impl KanteArt {
    /// Alle Werte, in der Reihenfolge der Dokumentation.
    pub const ALLE: [KanteArt; 3] = [KanteArt::Pfeil, KanteArt::Linie, KanteArt::Gestrichelt];

    /// Name im Modell-JSON (`pfeil`, `linie`, `gestrichelt`).
    pub fn name(self) -> &'static str {
        match self {
            KanteArt::Pfeil => "pfeil",
            KanteArt::Linie => "linie",
            KanteArt::Gestrichelt => "gestrichelt",
        }
    }

    /// Hat die Kante eine Pfeilspitze am Ziel?
    pub fn hat_pfeil(self) -> bool {
        matches!(self, KanteArt::Pfeil | KanteArt::Gestrichelt)
    }

    /// Wird die Linie gestrichelt gezeichnet?
    pub fn ist_gestrichelt(self) -> bool {
        matches!(self, KanteArt::Gestrichelt)
    }

    /// Umgekehrter Weg: Name aus dem Modell-JSON.
    pub fn aus_name(name: &str) -> Option<KanteArt> {
        KanteArt::ALLE.into_iter().find(|a| a.name() == name)
    }
}

/// Farbnamen aus fester Liste. Freie Farbwerte gibt es nicht (Aufgabe 16).
///
/// Die konkreten Werte liegen in [`crate::zeichnen::palette`]; hier steht nur
/// der semantische Name, damit Modell, Layout und Werkzeuge dieselbe Sprache
/// sprechen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Farbe {
    /// Blau – Hervorhebung, Standard für Schritte.
    Blau,
    /// Grün – Erfolg, Abschluss.
    Gruen,
    /// Rot – Warnung, Fehler, Abbruch.
    Rot,
    /// Gelb – Hinweis, offene Frage.
    Gelb,
    /// Grau – neutral, Hintergrund, deaktiviert.
    Grau,
    /// Violett – Sonderfall.
    Violett,
    /// Orange – Zwischenschritt.
    Orange,
    /// Türkis – Daten, Speicher.
    Tuerkis,
}

impl Farbe {
    /// Alle Werte, in der Reihenfolge der Dokumentation.
    pub const ALLE: [Farbe; 8] = [
        Farbe::Blau,
        Farbe::Gruen,
        Farbe::Rot,
        Farbe::Gelb,
        Farbe::Grau,
        Farbe::Violett,
        Farbe::Orange,
        Farbe::Tuerkis,
    ];

    /// Name im Modell-JSON (`blau`, `gruen`, …).
    pub fn name(self) -> &'static str {
        match self {
            Farbe::Blau => "blau",
            Farbe::Gruen => "gruen",
            Farbe::Rot => "rot",
            Farbe::Gelb => "gelb",
            Farbe::Grau => "grau",
            Farbe::Violett => "violett",
            Farbe::Orange => "orange",
            Farbe::Tuerkis => "tuerkis",
        }
    }

    /// Umgekehrter Weg: Name aus dem Modell-JSON.
    pub fn aus_name(name: &str) -> Option<Farbe> {
        Farbe::ALLE.into_iter().find(|f| f.name() == name)
    }
}

/// Ein Knoten: Id, Text, Form, optional Farbe und Gruppe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Knoten {
    /// Eindeutige Id im Diagramm; Kanten verweisen darauf.
    pub id: String,
    /// Beschriftung im Knoten (mehrzeilig mit `\n`).
    pub text: String,
    /// Form; fehlt sie, ist es ein Rechteck.
    #[serde(default)]
    pub form: Form,
    /// Füllfarbe aus fester Liste; fehlt sie, ist der Knoten neutral.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub farbe: Option<Farbe>,
    /// Id einer Gruppe aus [`Diagramm::gruppen`]; fehlt sie, liegt der Knoten frei.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gruppe: Option<String>,
}

impl Knoten {
    /// Neuer Knoten in Rechteckform ohne Farbe und Gruppe.
    pub fn neu(id: impl Into<String>, text: impl Into<String>) -> Knoten {
        Knoten {
            id: id.into(),
            text: text.into(),
            form: Form::Rechteck,
            farbe: None,
            gruppe: None,
        }
    }
}

/// Eine Kante zwischen zwei Knoten.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Kante {
    /// Id des Ausgangsknotens.
    pub von: String,
    /// Id des Zielknotens.
    pub nach: String,
    /// Beschriftung an der Kante; fehlt sie, bleibt die Kante nackt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Art der Kante; fehlt sie, ist es ein Pfeil.
    #[serde(default)]
    pub art: KanteArt,
}

impl Kante {
    /// Neue Kante als Pfeil ohne Text.
    pub fn neu(von: impl Into<String>, nach: impl Into<String>) -> Kante {
        Kante {
            von: von.into(),
            nach: nach.into(),
            text: None,
            art: KanteArt::Pfeil,
        }
    }
}

/// Eine Gruppe: Rahmen um die Knoten, die sie nennen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gruppe {
    /// Eindeutige Id; `Knoten::gruppe` verweist darauf.
    pub id: String,
    /// Überschrift des Rahmens.
    pub titel: String,
}

impl Gruppe {
    /// Neue Gruppe mit Titel.
    pub fn neu(id: impl Into<String>, titel: impl Into<String>) -> Gruppe {
        Gruppe {
            id: id.into(),
            titel: titel.into(),
        }
    }
}

/// Ein ganzes Diagramm.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagramm {
    /// Leserichtung; fehlt sie, geht es von oben nach unten.
    #[serde(default)]
    pub richtung: Richtung,
    /// Die Knoten in der Reihenfolge, in der sie angelegt wurden.
    pub knoten: Vec<Knoten>,
    /// Die Kanten; fehlen sie, gibt es keine.
    #[serde(default)]
    pub kanten: Vec<Kante>,
    /// Die Gruppen; fehlen sie, gibt es keine.
    #[serde(default)]
    pub gruppen: Vec<Gruppe>,
}

impl Diagramm {
    /// Ein leeres Diagramm mit der Richtung „unten“.
    pub fn leer() -> Diagramm {
        Diagramm {
            richtung: Richtung::Unten,
            knoten: Vec::new(),
            kanten: Vec::new(),
            gruppen: Vec::new(),
        }
    }

    /// Modell aus JSON lesen (etwa aus einem Werkzeugaufruf).
    ///
    /// Prüft nur die JSON-Form; die inhaltlichen Regeln stehen in [`Diagramm::pruefen`].
    pub fn aus_json(text: &str) -> Result<Diagramm, ModellFehler> {
        serde_json::from_str(text)
            .map_err(|fehler| ModellFehler::neu(format!("Modell ist kein gültiges JSON: {fehler}")))
    }

    /// Modell als JSON schreiben (Gegenstück zu [`Diagramm::aus_json`]).
    pub fn als_json(&self) -> Result<String, ModellFehler> {
        serde_json::to_string(self).map_err(|fehler| {
            ModellFehler::neu(format!("Modell lässt sich nicht schreiben: {fehler}"))
        })
    }

    /// Index des Knotens mit dieser Id.
    pub fn knoten_index(&self, id: &str) -> Option<usize> {
        self.knoten.iter().position(|k| k.id == id)
    }

    /// Gruppe mit dieser Id.
    pub fn gruppe(&self, id: &str) -> Option<&Gruppe> {
        self.gruppen.iter().find(|g| g.id == id)
    }

    /// Prüft das Modell gegen alle Regeln aus Aufgabe 16.
    ///
    /// Gesammelt werden **alle** Verstöße, nicht nur der erste: der Agent soll
    /// seinen Aufruf in einem Zug berichtigen können. Die Meldungen nennen den
    /// Ort (`knoten[2].text`) und was dort falsch ist.
    pub fn pruefen(&self) -> Result<(), ModellFehler> {
        let mut fehler = ModellFehler::leer();

        if self.knoten.len() > MAX_KNOTEN {
            fehler.anhaengen(format!(
                "zu viele Knoten: {} (höchstens {MAX_KNOTEN})",
                self.knoten.len()
            ));
        }
        if self.kanten.len() > MAX_KANTEN {
            fehler.anhaengen(format!(
                "zu viele Kanten: {} (höchstens {MAX_KANTEN})",
                self.kanten.len()
            ));
        }

        for (i, knoten) in self.knoten.iter().enumerate() {
            pruefe_id(&mut fehler, &format!("knoten[{i}].id"), &knoten.id);
            pruefe_text(&mut fehler, &format!("knoten[{i}].text"), &knoten.text);
            if let Some(gruppe) = &knoten.gruppe {
                if self.gruppe(gruppe).is_none() {
                    fehler.anhaengen(format!(
                        "knoten[{i}].gruppe: „{gruppe}“ ist keine Gruppe dieses Diagramms"
                    ));
                }
            }
        }
        for (i, knoten) in self.knoten.iter().enumerate() {
            if let Some(vorher) = self.knoten[..i].iter().position(|k| k.id == knoten.id) {
                if !knoten.id.trim().is_empty() {
                    fehler.anhaengen(format!(
                        "knoten[{i}].id: „{}“ kommt schon in knoten[{vorher}] vor",
                        knoten.id
                    ));
                }
            }
        }

        for (i, gruppe) in self.gruppen.iter().enumerate() {
            pruefe_id(&mut fehler, &format!("gruppen[{i}].id"), &gruppe.id);
            pruefe_text(&mut fehler, &format!("gruppen[{i}].titel"), &gruppe.titel);
        }
        for (i, gruppe) in self.gruppen.iter().enumerate() {
            if let Some(vorher) = self.gruppen[..i].iter().position(|g| g.id == gruppe.id) {
                if !gruppe.id.trim().is_empty() {
                    fehler.anhaengen(format!(
                        "gruppen[{i}].id: „{}“ kommt schon in gruppen[{vorher}] vor",
                        gruppe.id
                    ));
                }
            }
        }

        for (i, kante) in self.kanten.iter().enumerate() {
            if kante.von.trim().is_empty() {
                fehler.anhaengen(format!("kanten[{i}].von: id fehlt"));
            } else if self.knoten_index(&kante.von).is_none() {
                fehler.anhaengen(format!(
                    "kanten[{i}].von: „{}“ ist kein Knoten dieses Diagramms",
                    kante.von
                ));
            }
            if kante.nach.trim().is_empty() {
                fehler.anhaengen(format!("kanten[{i}].nach: id fehlt"));
            } else if self.knoten_index(&kante.nach).is_none() {
                fehler.anhaengen(format!(
                    "kanten[{i}].nach: „{}“ ist kein Knoten dieses Diagramms",
                    kante.nach
                ));
            }
            if kante.von == kante.nach && !kante.von.trim().is_empty() {
                fehler.anhaengen(format!(
                    "kanten[{i}]: „{}“ ist mit sich selbst verbunden",
                    kante.von
                ));
            }
            if let Some(text) = &kante.text {
                pruefe_text(&mut fehler, &format!("kanten[{i}].text"), text);
            }
        }

        fehler.in_ordnung()
    }
}

fn pruefe_id(fehler: &mut ModellFehler, ort: &str, id: &str) {
    if id.trim().is_empty() {
        fehler.anhaengen(format!("{ort}: id fehlt"));
        return;
    }
    if id.trim() != id {
        fehler.anhaengen(format!("{ort}: „{id}“ hat Leerzeichen am Rand"));
    }
    if hat_steuerzeichen(id) {
        fehler.anhaengen(format!("{ort}: „{id}“ enthält Steuerzeichen"));
    }
}

fn pruefe_text(fehler: &mut ModellFehler, ort: &str, text: &str) {
    let laenge = text.chars().count();
    if laenge > MAX_TEXT {
        fehler.anhaengen(format!("{ort}: {laenge} Zeichen (höchstens {MAX_TEXT})"));
    }
    if text
        .chars()
        .any(|z| z.is_control() && z != '\n' && z != '\t')
    {
        fehler.anhaengen(format!(
            "{ort}: enthält Steuerzeichen – erlaubt sind nur Zeilenumbruch und Tabulator"
        ));
    }
}

fn hat_steuerzeichen(text: &str) -> bool {
    text.chars().any(char::is_control)
}

/// Gesammelte Verstöße gegen die Modellregeln.
///
/// `Display` gibt eine Meldung mit allen Punkten aus; ein Werkzeug kann sie
/// unverändert an den Agenten weiterreichen.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModellFehler {
    meldungen: Vec<String>,
}

impl ModellFehler {
    /// Ein Fehler mit genau einer Meldung.
    pub fn neu(meldung: impl Into<String>) -> ModellFehler {
        ModellFehler {
            meldungen: vec![meldung.into()],
        }
    }

    /// Ein Fehler ohne Meldungen; [`ModellFehler::in_ordnung`] macht daraus `Ok`.
    pub fn leer() -> ModellFehler {
        ModellFehler {
            meldungen: Vec::new(),
        }
    }

    /// Eine weitere Meldung anhängen.
    pub fn anhaengen(&mut self, meldung: impl Into<String>) {
        self.meldungen.push(meldung.into());
    }

    /// Die einzelnen Meldungen.
    pub fn meldungen(&self) -> &[String] {
        &self.meldungen
    }

    /// `Ok(())`, wenn keine Meldung gesammelt wurde, sonst `Err(self)`.
    pub fn in_ordnung(self) -> Result<(), ModellFehler> {
        if self.meldungen.is_empty() {
            Ok(())
        } else {
            Err(self)
        }
    }
}

impl fmt::Display for ModellFehler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.meldungen.len() {
            0 => write!(f, "Modell ist in Ordnung"),
            1 => write!(f, "Modell ist nicht gültig: {}", self.meldungen[0]),
            _ => {
                writeln!(f, "Modell ist nicht gültig:")?;
                for (i, meldung) in self.meldungen.iter().enumerate() {
                    if i + 1 == self.meldungen.len() {
                        write!(f, "– {meldung}")?;
                    } else {
                        writeln!(f, "– {meldung}")?;
                    }
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ModellFehler {}

#[cfg(test)]
mod tests {
    use super::*;

    fn guetes() -> Diagramm {
        let mut diagramm = Diagramm::leer();
        diagramm.knoten = vec![
            Knoten::neu("start", "Antrag"),
            Knoten {
                form: Form::Raute,
                farbe: Some(Farbe::Blau),
                gruppe: Some("vorgang".to_string()),
                ..Knoten::neu("pruefung", "Prüfung")
            },
        ];
        diagramm.kanten = vec![Kante {
            text: Some("eingereicht".to_string()),
            art: KanteArt::Gestrichelt,
            ..Kante::neu("start", "pruefung")
        }];
        diagramm.gruppen = vec![Gruppe::neu("vorgang", "Vorgang")];
        diagramm
    }

    #[test]
    fn gueltiges_modell_kommt_durch() {
        guetes().pruefen().expect("Modell ist gültig");
    }

    #[test]
    fn json_rundlauf() {
        let diagramm = guetes();
        let json = diagramm.als_json().expect("schreiben");
        let zurueck = Diagramm::aus_json(&json).expect("lesen");
        assert_eq!(diagramm, zurueck);
        assert!(json.contains("\"form\":\"raute\""));
        assert!(json.contains("\"art\":\"gestrichelt\""));
    }

    #[test]
    fn fehlendes_json_ist_eine_meldung() {
        let fehler = Diagramm::aus_json("{").expect_err("kein JSON");
        assert!(fehler.to_string().contains("kein gültiges JSON"));
    }

    #[test]
    fn doppelte_knoten_ids_werden_gemeldet() {
        let mut diagramm = guetes();
        diagramm.knoten.push(Knoten::neu("start", "nochmal"));
        let fehler = diagramm.pruefen().expect_err("doppelt");
        assert!(
            fehler
                .meldungen()
                .iter()
                .any(|m| m.contains("knoten[2].id") && m.contains("knoten[0]")),
            "{fehler}"
        );
    }

    #[test]
    fn leere_ids_werden_gemeldet() {
        let mut diagramm = Diagramm::leer();
        diagramm.knoten = vec![Knoten::neu("", "ohne Id")];
        let fehler = diagramm.pruefen().expect_err("leer");
        assert!(fehler.meldungen()[0].contains("knoten[0].id: id fehlt"));
    }

    #[test]
    fn unbekannte_kantenenden_werden_gemeldet() {
        let mut diagramm = Diagramm::leer();
        diagramm.knoten = vec![Knoten::neu("a", "A")];
        diagramm.kanten = vec![Kante::neu("a", "gibtsnicht")];
        let fehler = diagramm.pruefen().expect_err("unbekannt");
        assert!(
            fehler
                .meldungen()
                .iter()
                .any(|m| m.contains("kanten[0].nach") && m.contains("gibtsnicht")),
            "{fehler}"
        );
    }

    #[test]
    fn selbstverbindung_und_unbekannte_gruppe_werden_gemeldet() {
        let mut diagramm = Diagramm::leer();
        diagramm.knoten = vec![Knoten {
            gruppe: Some("fehlt".to_string()),
            ..Knoten::neu("a", "A")
        }];
        diagramm.kanten = vec![Kante::neu("a", "a")];
        let fehler = diagramm.pruefen().expect_err("zwei Verstöße");
        assert_eq!(fehler.meldungen().len(), 2, "{fehler}");
        assert!(fehler.to_string().contains("knoten[0].gruppe"));
    }

    #[test]
    fn grenzen_werden_geprueft() {
        let mut diagramm = Diagramm::leer();
        diagramm.knoten = (0..=MAX_KNOTEN)
            .map(|i| Knoten::neu(format!("k{i}"), "x"))
            .collect();
        let fehler = diagramm.pruefen().expect_err("zu viele Knoten");
        assert!(fehler.to_string().contains("zu viele Knoten"));

        let mut diagramm = Diagramm::leer();
        diagramm.knoten = vec![Knoten::neu("a", "ä".repeat(MAX_TEXT + 1))];
        let fehler = diagramm.pruefen().expect_err("zu langer Text");
        assert!(fehler.to_string().contains("höchstens 2000"));

        let mut diagramm = Diagramm::leer();
        diagramm.knoten = vec![Knoten::neu("a", "A"), Knoten::neu("b", "B")];
        diagramm.kanten = (0..=MAX_KANTEN).map(|_| Kante::neu("a", "b")).collect();
        let fehler = diagramm.pruefen().expect_err("zu viele Kanten");
        assert!(fehler.to_string().contains("zu viele Kanten"));
    }

    #[test]
    fn steuerzeichen_und_leerzeichen_am_rand_sind_verboten() {
        let mut diagramm = Diagramm::leer();
        diagramm.knoten = vec![Knoten::neu(" a", "A"), Knoten::neu("b", "Zeile\u{0}Null")];
        let fehler = diagramm.pruefen().expect_err("verboten");
        assert!(
            fehler.to_string().contains("Leerzeichen am Rand"),
            "{fehler}"
        );
        assert!(fehler.to_string().contains("Steuerzeichen"), "{fehler}");
    }

    #[test]
    fn namen_der_werte_sind_stabil() {
        assert_eq!(Form::aus_name("zylinder"), Some(Form::Zylinder));
        assert_eq!(Farbe::aus_name("tuerkis"), Some(Farbe::Tuerkis));
        assert_eq!(KanteArt::aus_name("linie"), Some(KanteArt::Linie));
        assert_eq!(Richtung::aus_name("rechts"), Some(Richtung::Rechts));
        assert_eq!(Form::aus_name("Kreis"), None);
        assert_eq!(Farbe::ALLE.len(), 8);
        assert!(KanteArt::Gestrichelt.hat_pfeil());
        assert!(!KanteArt::Linie.hat_pfeil());
        assert!(KanteArt::Gestrichelt.ist_gestrichelt());
    }
}
