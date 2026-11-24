// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Geschichtetes Layout: aus Beschreibung werden Koordinaten.
//!
//! Der Ablauf ist der klassische: Kanten, die einen Kreis schließen, werden
//! erkannt und für die Rangvergabe ignoriert; dann bekommt jeder Knoten einen
//! Rang (Schicht), innerhalb der Schicht wird nach Baryzentrum sortiert, und
//! zum Schluss werden die Schichten gesetzt. Weil jede Schicht eine eigene
//! Reihe ist und die Knoten darin mit festem Abstand stehen, **überlappen sich
//! Knoten nie** – unabhängig davon, wie der Graph aussieht.
//!
//! - **Textmaß** kommt aus der eingebetteten Schrift
//!   ([`crate::zeichnen::schrift`]); die Knotengröße folgt dem Text, nicht
//!   umgekehrt.
//! - **Gleiche Eingabe → gleiche Ausgabe.** Es gibt keinen Zufall und keine
//!   Hash-Reihenfolge; Gleichstände entscheidet der Index im Modell.
//! - **Gruppen** bekommen einen Rahmen um ihre Knoten (Rand und Kopfzeile).
//! - **Kanten** laufen im Zwischenraum der Schichten; Kanten, die nicht zur
//!   nächsten Schicht führen (Rückkanten, Sprünge, gleiche Schicht), laufen
//!   außen herum und treffen keinen Knoten.

use crate::modell::{Diagramm, Farbe, Form, KanteArt, ModellFehler, Richtung};
use crate::zeichnen::schrift;
use std::cmp::Ordering;
use std::collections::VecDeque;

/// Schriftgröße der Knoten- und Kantentexte.
pub const SCHRIFTGROESSE: f32 = 14.0;

/// Höhe einer Textzeile.
pub const ZEILENHOEHE: f32 = SCHRIFTGROESSE * schrift::ZEILENFAKTOR;

/// Innenabstand im Knoten.
pub const INNEN: f32 = 16.0;

/// Abstand zwischen zwei Knoten derselben Schicht.
pub const ABSTAND_GLEICH: f32 = 40.0;

/// Abstand zwischen zwei Schichten.
pub const ABSTAND_SCHICHT: f32 = 80.0;

/// Rand um das ganze Diagramm.
pub const RAND: f32 = 32.0;

/// Rand zwischen Gruppenrahmen und Knoten.
pub const GRUPPEN_RAND: f32 = 18.0;

/// Höhe der Kopfzeile eines Gruppenrahmens.
pub const GRUPPEN_KOPF: f32 = 30.0;

pub use crate::zeichnen::Punkt;

/// Ein Knoten an seinem Platz.
#[derive(Debug, Clone, PartialEq)]
pub struct KnotenPlatz {
    /// Id aus dem Modell.
    pub id: String,
    /// Text aus dem Modell.
    pub text: String,
    /// Form aus dem Modell.
    pub form: Form,
    /// Farbe aus dem Modell.
    pub farbe: Option<Farbe>,
    /// Gruppe aus dem Modell.
    pub gruppe: Option<String>,
    /// Linke Kante.
    pub x: f32,
    /// Oberkante.
    pub y: f32,
    /// Breite.
    pub breite: f32,
    /// Höhe.
    pub hoehe: f32,
    /// Umgebrochene Textzeilen, wie sie gezeichnet werden.
    pub zeilen: Vec<String>,
    /// Schriftgröße des Textes.
    pub schriftgroesse: f32,
}

/// Beschriftung einer Kante.
#[derive(Debug, Clone, PartialEq)]
pub struct Kantenbeschriftung {
    /// Der Text aus dem Modell.
    pub text: String,
    /// Umgebrochene Zeilen.
    pub zeilen: Vec<String>,
    /// Linke obere Ecke der Textbox.
    pub position: Punkt,
    /// Breite der Textbox (für die Ausrichtung).
    pub breite: f32,
}

/// Eine Kante als Linienzug.
#[derive(Debug, Clone, PartialEq)]
pub struct KantenLauf {
    /// Id des Ausgangsknotens.
    pub von: String,
    /// Id des Zielknotens.
    pub nach: String,
    /// Art aus dem Modell (Pfeil, Linie, gestrichelt).
    pub art: KanteArt,
    /// Die Stützpunkte in Zeichenreihenfolge (mindestens zwei).
    pub punkte: Vec<Punkt>,
    /// Beschriftung, falls die Kante eine hat.
    pub beschriftung: Option<Kantenbeschriftung>,
}

/// Der Rahmen einer Gruppe.
#[derive(Debug, Clone, PartialEq)]
pub struct GruppenRahmen {
    /// Id aus dem Modell.
    pub id: String,
    /// Titel aus dem Modell.
    pub titel: String,
    /// Linke Kante.
    pub x: f32,
    /// Oberkante (einschließlich Kopfzeile).
    pub y: f32,
    /// Breite.
    pub breite: f32,
    /// Höhe.
    pub hoehe: f32,
}

/// Das fertige Layout: alles, was [`crate::zeichnen`] zum Malen braucht.
#[derive(Debug, Clone, PartialEq)]
pub struct Zeichnung {
    /// Breite der Zeichenfläche.
    pub breite: f32,
    /// Höhe der Zeichenfläche.
    pub hoehe: f32,
    /// Knoten mit Position und Größe.
    pub knoten: Vec<KnotenPlatz>,
    /// Kanten als Linienzüge.
    pub kanten: Vec<KantenLauf>,
    /// Gruppenrahmen (hinter den Knoten zu zeichnen).
    pub gruppen: Vec<GruppenRahmen>,
}

/// Berechnet das Layout für ein Modell.
///
/// Prüft zuerst das Modell ([`Diagramm::pruefen`]) und gibt dessen Meldungen
/// unverändert weiter: ein ungültiges Modell wird nicht halb gezeichnet.
pub fn layout(diagramm: &Diagramm) -> Result<Zeichnung, ModellFehler> {
    diagramm.pruefen()?;
    let anzahl = diagramm.knoten.len();
    if anzahl == 0 {
        return Ok(Zeichnung {
            breite: 2.0 * RAND,
            hoehe: 2.0 * RAND,
            knoten: Vec::new(),
            kanten: Vec::new(),
            gruppen: Vec::new(),
        });
    }

    let masse: Vec<Masse> = diagramm
        .knoten
        .iter()
        .map(|knoten| masse(&knoten.text, knoten.form))
        .collect();

    let kanten: Vec<(usize, usize)> = diagramm
        .kanten
        .iter()
        .map(|kante| {
            (
                diagramm
                    .knoten_index(&kante.von)
                    .expect("geprüft: von existiert"),
                diagramm
                    .knoten_index(&kante.nach)
                    .expect("geprüft: nach existiert"),
            )
        })
        .collect();

    let rueck = rueckkanten(anzahl, &kanten);
    let raenge = raenge(anzahl, &kanten, &rueck);
    let mut schichten = schichten(anzahl, &raenge);
    let nachbarn = nachbarn(anzahl, &kanten);
    ordnen(&mut schichten, &raenge, &nachbarn);

    let (mut plaetze, raster) = setzen(diagramm, &masse, &schichten);
    let gruppen = rahmen(diagramm, &plaetze);

    let aussen = aussenkante(&plaetze, &gruppen);
    let lage = Routenlage {
        diagramm,
        plaetze: &plaetze,
        raster: &raster,
        raenge: &raenge,
        kanten: &kanten,
        rueck: &rueck,
        aussen,
        unten: (gruppen
            .iter()
            .map(|g| g.y + g.hoehe)
            .fold(f32::NEG_INFINITY, f32::max))
        .max(
            plaetze
                .iter()
                .map(|p| p.y + p.hoehe)
                .fold(f32::NEG_INFINITY, f32::max),
        ) + ABSTAND_GLEICH * 0.7,
    };
    let kantenlaeufe = kanten_routen(&lage);

    // Alles zusammen in den sichtbaren Bereich schieben.
    let mut min_x = 0.0_f32;
    let mut min_y = 0.0_f32;
    let mut max_x = 0.0_f32;
    let mut max_y = 0.0_f32;
    let mut gesehen = false;
    let mut merke = |x: f32, y: f32| {
        if !gesehen {
            gesehen = true;
            (min_x, min_y, max_x, max_y) = (x, y, x, y);
        } else {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    };
    for knoten in &plaetze {
        merke(knoten.x, knoten.y);
        merke(knoten.x + knoten.breite, knoten.y + knoten.hoehe);
    }
    for rahmen in &gruppen {
        merke(rahmen.x, rahmen.y);
        merke(rahmen.x + rahmen.breite, rahmen.y + rahmen.hoehe);
    }
    for lauf in &kantenlaeufe {
        for punkt in &lauf.punkte {
            merke(punkt.x, punkt.y);
        }
        if let Some(beschriftung) = &lauf.beschriftung {
            merke(beschriftung.position.x, beschriftung.position.y);
            merke(
                beschriftung.position.x + beschriftung.breite,
                beschriftung.position.y + beschriftung.zeilen.len() as f32 * ZEILENHOEHE,
            );
        }
    }

    let versatz_x = RAND - min_x;
    let versatz_y = RAND - min_y;
    for knoten in &mut plaetze {
        knoten.x += versatz_x;
        knoten.y += versatz_y;
    }
    let mut gruppen = gruppen;
    for rahmen in &mut gruppen {
        rahmen.x += versatz_x;
        rahmen.y += versatz_y;
    }
    let mut kantenlaeufe = kantenlaeufe;
    for lauf in &mut kantenlaeufe {
        for punkt in &mut lauf.punkte {
            punkt.x += versatz_x;
            punkt.y += versatz_y;
        }
        if let Some(beschriftung) = &mut lauf.beschriftung {
            beschriftung.position.x += versatz_x;
            beschriftung.position.y += versatz_y;
        }
    }

    Ok(Zeichnung {
        breite: max_x - min_x + 2.0 * RAND,
        hoehe: max_y - min_y + 2.0 * RAND,
        knoten: plaetze,
        kanten: kantenlaeufe,
        gruppen,
    })
}

#[derive(Debug, Clone)]
struct Masse {
    breite: f32,
    hoehe: f32,
    zeilen: Vec<String>,
}

/// Knotengröße aus Text und Form.
fn masse(text: &str, form: Form) -> Masse {
    let innen_breite = 220.0;
    let zeilen = schrift::umbrechen(text, SCHRIFTGROESSE, innen_breite);
    let text_breite = zeilen
        .iter()
        .map(|zeile| schrift::breite(zeile, SCHRIFTGROESSE))
        .fold(0.0_f32, f32::max);
    let text_hoehe = zeilen.len() as f32 * ZEILENHOEHE;
    let mut breite = (text_breite + 2.0 * INNEN).max(120.0);
    let mut hoehe = (text_hoehe + 2.0 * INNEN).max(56.0);
    match form {
        Form::Rechteck => {}
        // Im Rechteck, das einer Raute einbeschrieben ist, hat der Text nur
        // halb so viel Platz; darum doppelt so groß.
        Form::Raute => {
            breite *= 2.1;
            hoehe *= 2.0;
        }
        // Ellipse: einbeschriebenes Rechteck ist 1/√2 so groß.
        Form::Ellipse => {
            breite *= 1.5;
            hoehe *= 1.45;
        }
        Form::Zylinder => {
            breite *= 1.05;
            hoehe += 24.0;
        }
        Form::Notiz => {
            breite *= 1.1;
            hoehe += 12.0;
        }
    }
    Masse {
        breite: aufrunden(breite),
        hoehe: aufrunden(hoehe),
        zeilen,
    }
}

fn aufrunden(wert: f32) -> f32 {
    ((wert / 2.0).ceil() * 2.0).max(4.0)
}

/// Markiert Kanten, die im Tiefensuchbaum zurück auf den Stapel zeigen (und
/// damit einen Kreis schließen). Nur für die Rangvergabe; gezeichnet werden sie
/// trotzdem.
fn rueckkanten(anzahl: usize, kanten: &[(usize, usize)]) -> Vec<bool> {
    let mut ausgehend: Vec<Vec<(usize, usize)>> = vec![Vec::new(); anzahl];
    for (index, (von, nach)) in kanten.iter().enumerate() {
        ausgehend[*von].push((*nach, index));
    }
    let mut zustand = vec![0_u8; anzahl];
    let mut rueck = vec![false; kanten.len()];
    for start in 0..anzahl {
        if zustand[start] != 0 {
            continue;
        }
        zustand[start] = 1;
        let mut stapel: Vec<(usize, usize)> = vec![(start, 0)];
        while let Some((knoten, position)) = stapel.last_mut() {
            if let Some(&(ziel, index)) = ausgehend[*knoten].get(*position) {
                *position += 1;
                match zustand[ziel] {
                    0 => {
                        zustand[ziel] = 1;
                        stapel.push((ziel, 0));
                    }
                    1 => rueck[index] = true,
                    _ => {}
                }
            } else {
                zustand[*knoten] = 2;
                stapel.pop();
            }
        }
    }
    rueck
}

/// Rang (Schicht) je Knoten: längster Weg im kreisfreien Restgraphen.
fn raenge(anzahl: usize, kanten: &[(usize, usize)], rueck: &[bool]) -> Vec<usize> {
    let mut ausgehend: Vec<Vec<usize>> = vec![Vec::new(); anzahl];
    let mut eingehend = vec![0_usize; anzahl];
    for (index, (von, nach)) in kanten.iter().enumerate() {
        if rueck[index] {
            continue;
        }
        ausgehend[*von].push(*nach);
        eingehend[*nach] += 1;
    }
    let mut rang = vec![0_usize; anzahl];
    let mut bereit: VecDeque<usize> = (0..anzahl).filter(|i| eingehend[*i] == 0).collect();
    while let Some(von) = bereit.pop_front() {
        for nach in &ausgehend[von] {
            rang[*nach] = rang[*nach].max(rang[von] + 1);
            eingehend[*nach] -= 1;
            if eingehend[*nach] == 0 {
                bereit.push_back(*nach);
            }
        }
    }
    rang
}

fn schichten(anzahl: usize, raenge: &[usize]) -> Vec<Vec<usize>> {
    let hoechster = raenge.iter().copied().max().unwrap_or(0);
    let mut schichten = vec![Vec::new(); hoechster + 1];
    for knoten in 0..anzahl {
        schichten[raenge[knoten]].push(knoten);
    }
    schichten
}

fn nachbarn(anzahl: usize, kanten: &[(usize, usize)]) -> Vec<Vec<usize>> {
    let mut nachbarn: Vec<Vec<usize>> = vec![Vec::new(); anzahl];
    for (von, nach) in kanten {
        if !nachbarn[*von].contains(nach) {
            nachbarn[*von].push(*nach);
        }
        if !nachbarn[*nach].contains(von) {
            nachbarn[*nach].push(*von);
        }
    }
    nachbarn
}

/// Sortiert die Schichten nach Baryzentrum, abwechselnd vorwärts und rückwärts.
fn ordnen(schichten: &mut [Vec<usize>], raenge: &[usize], nachbarn: &[Vec<usize>]) {
    for runde in 0..8 {
        let richtung_vorwaerts = runde % 2 == 0;
        let reihen: Vec<usize> = if richtung_vorwaerts {
            (0..schichten.len()).collect()
        } else {
            (0..schichten.len()).rev().collect()
        };
        for reihe in reihen {
            let lage = lagen(schichten);
            let mut schluessel: Vec<(f32, usize)> = schichten[reihe]
                .iter()
                .map(|knoten| (baryzentrum(*knoten, &lage, raenge, nachbarn), *knoten))
                .collect();
            schluessel.sort_by(|a, b| {
                a.0.partial_cmp(&b.0)
                    .unwrap_or(Ordering::Equal)
                    .then_with(|| a.1.cmp(&b.1))
            });
            schichten[reihe] = schluessel.into_iter().map(|(_, knoten)| knoten).collect();
        }
    }
}

fn lagen(schichten: &[Vec<usize>]) -> Vec<f32> {
    let mut lage = vec![0.0_f32; schichten.iter().map(Vec::len).sum()];
    for schicht in schichten {
        for (position, knoten) in schicht.iter().enumerate() {
            if *knoten < lage.len() {
                lage[*knoten] = position as f32;
            }
        }
    }
    lage
}

fn baryzentrum(knoten: usize, lage: &[f32], raenge: &[usize], nachbarn: &[Vec<usize>]) -> f32 {
    let eigene = raenge[knoten];
    let fremde: Vec<f32> = nachbarn[knoten]
        .iter()
        .filter(|nachbar| raenge[**nachbar] != eigene)
        .map(|nachbar| lage[*nachbar])
        .collect();
    if fremde.is_empty() {
        lage[knoten]
    } else {
        fremde.iter().sum::<f32>() / fremde.len() as f32
    }
}

/// Lage der Schichten: bei „unten“ je Reihe y und Höhe, bei „rechts“ je Spalte
/// x und Breite. Wird für die Kantenführung in den Zwischenräumen gebraucht.
#[derive(Debug, Clone)]
struct Raster {
    anfang: Vec<f32>,
    dicke: Vec<f32>,
    reihe_von: Vec<usize>,
}

impl Raster {
    /// Zwischenraum hinter einer Schicht (unten: darunter, rechts: rechts davon).
    fn dahinter(&self, reihe: usize) -> f32 {
        self.anfang[reihe] + self.dicke[reihe] + ABSTAND_SCHICHT * 0.4
    }

    /// Zwischenraum vor einer Schicht (unten: darüber, rechts: links davon).
    fn davor(&self, reihe: usize) -> f32 {
        self.anfang[reihe] - ABSTAND_SCHICHT * 0.4
    }
}

/// Setzt die Knoten: je Schicht eine Reihe, innerhalb der Reihe mit festem
/// Abstand, jede Reihe mittig zur breitesten.
fn setzen(
    diagramm: &Diagramm,
    masse: &[Masse],
    schichten: &[Vec<usize>],
) -> (Vec<KnotenPlatz>, Raster) {
    let reihen: Vec<(f32, f32)> = schichten
        .iter()
        .map(|schicht| {
            let breite: f32 = schicht.iter().map(|i| masse[*i].breite).sum::<f32>()
                + ABSTAND_GLEICH * schicht.len().saturating_sub(1) as f32;
            let hoehe = schicht
                .iter()
                .map(|i| masse[*i].hoehe)
                .fold(0.0_f32, f32::max);
            (breite, hoehe)
        })
        .collect();
    let gesamt_breite = reihen.iter().map(|(b, _)| *b).fold(0.0_f32, f32::max);
    let gesamt_hoehe = reihen.iter().map(|(_, h)| *h).fold(0.0_f32, f32::max);
    let rechts = diagramm.richtung == Richtung::Rechts;

    let mut plaetze: Vec<KnotenPlatz> = diagramm
        .knoten
        .iter()
        .enumerate()
        .map(|(index, knoten)| KnotenPlatz {
            id: knoten.id.clone(),
            text: knoten.text.clone(),
            form: knoten.form,
            farbe: knoten.farbe,
            gruppe: knoten.gruppe.clone(),
            x: 0.0,
            y: 0.0,
            breite: masse[index].breite,
            hoehe: masse[index].hoehe,
            zeilen: masse[index].zeilen.clone(),
            schriftgroesse: SCHRIFTGROESSE,
        })
        .collect();

    let mut raster = Raster {
        anfang: Vec::with_capacity(schichten.len()),
        dicke: Vec::with_capacity(schichten.len()),
        reihe_von: vec![0; plaetze.len()],
    };
    let mut lauf = 0.0_f32;
    for (reihe, schicht) in schichten.iter().enumerate() {
        let (reihen_breite, reihen_hoehe) = reihen[reihe];
        raster.anfang.push(lauf);
        if rechts {
            let x = lauf;
            let y_start = (gesamt_hoehe - reihen_hoehe) / 2.0;
            let mut y = y_start;
            for knoten in schicht {
                let platz = &mut plaetze[*knoten];
                platz.x = x;
                platz.y = y;
                raster.reihe_von[*knoten] = reihe;
                y += platz.hoehe + ABSTAND_GLEICH;
            }
            raster.dicke.push(reihen_breite);
            lauf += reihen_breite + ABSTAND_SCHICHT;
        } else {
            let y = lauf;
            let x_start = (gesamt_breite - reihen_breite) / 2.0;
            let mut x = x_start;
            for knoten in schicht {
                let platz = &mut plaetze[*knoten];
                platz.x = x;
                platz.y = y;
                raster.reihe_von[*knoten] = reihe;
                x += platz.breite + ABSTAND_GLEICH;
            }
            raster.dicke.push(reihen_hoehe);
            lauf += reihen_hoehe + ABSTAND_SCHICHT;
        }
    }
    (plaetze, raster)
}

/// Rahmen um die Knoten jeder Gruppe.
fn rahmen(diagramm: &Diagramm, plaetze: &[KnotenPlatz]) -> Vec<GruppenRahmen> {
    let mut rahmen = Vec::new();
    let (rechts, unten) = plaetze.iter().fold((0.0_f32, 0.0_f32), |(r, u), p| {
        (r.max(p.x + p.breite), u.max(p.y + p.hoehe))
    });
    let mut leere = 0_usize;
    for gruppe in &diagramm.gruppen {
        let mitglieder: Vec<&KnotenPlatz> = plaetze
            .iter()
            .filter(|platz| platz.gruppe.as_deref() == Some(gruppe.id.as_str()))
            .collect();
        if mitglieder.is_empty() {
            // Leere Gruppe: kleiner Rahmen unter dem Diagramm, damit sie sichtbar
            // bleibt und nicht verschwindet.
            let breite = 180.0;
            let hoehe = GRUPPEN_KOPF + 24.0;
            rahmen.push(GruppenRahmen {
                id: gruppe.id.clone(),
                titel: gruppe.titel.clone(),
                x: (rechts - breite).max(0.0),
                y: unten + 16.0 + leere as f32 * (hoehe + 12.0),
                breite,
                hoehe,
            });
            leere += 1;
            continue;
        }
        let min_x = mitglieder.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
        let min_y = mitglieder.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
        let max_x = mitglieder
            .iter()
            .map(|p| p.x + p.breite)
            .fold(f32::NEG_INFINITY, f32::max);
        let max_y = mitglieder
            .iter()
            .map(|p| p.y + p.hoehe)
            .fold(f32::NEG_INFINITY, f32::max);
        rahmen.push(GruppenRahmen {
            id: gruppe.id.clone(),
            titel: gruppe.titel.clone(),
            x: min_x - GRUPPEN_RAND,
            y: min_y - GRUPPEN_RAND - GRUPPEN_KOPF,
            breite: (max_x - min_x) + 2.0 * GRUPPEN_RAND,
            hoehe: (max_y - min_y) + 2.0 * GRUPPEN_RAND + GRUPPEN_KOPF,
        });
    }
    rahmen
}

/// Rechte Kante des Inhalts (für den Kanal der äußeren Kanten).
fn aussenkante(plaetze: &[KnotenPlatz], gruppen: &[GruppenRahmen]) -> f32 {
    let knoten = plaetze
        .iter()
        .map(|p| p.x + p.breite)
        .fold(f32::NEG_INFINITY, f32::max);
    gruppen
        .iter()
        .map(|g| g.x + g.breite)
        .fold(knoten, f32::max)
}

/// Baut die Linienzüge der Kanten.
///
/// Zwei Fälle, beide ohne Knoten im Weg:
/// - **nächste Schicht:** waagerecht im Zwischenraum hinter der Quellschicht,
///   dann senkrecht in das Ziel.
/// - **alles andere** (Rückkante, Sprung über Schichten): außen herum – bei
///   „unten“ über den Kanal rechts, bei „rechts“ über den Kanal unten. Ein-
///   und ausgestiegen wird dabei im Zwischenraum der jeweiligen Schicht.
struct Routenlage<'a> {
    diagramm: &'a Diagramm,
    plaetze: &'a [KnotenPlatz],
    raster: &'a Raster,
    raenge: &'a [usize],
    kanten: &'a [(usize, usize)],
    rueck: &'a [bool],
    /// Rechte Kante des Inhalts (Kanal für äußere Kanten bei „unten“).
    aussen: f32,
    /// Untere Kante des Inhalts (Kanal für äußere Kanten bei „rechts“).
    unten: f32,
}

fn kanten_routen(lage: &Routenlage<'_>) -> Vec<KantenLauf> {
    let diagramm = lage.diagramm;
    let plaetze = lage.plaetze;
    let raster = lage.raster;
    let raenge = lage.raenge;
    let kanten = lage.kanten;
    let rueck = lage.rueck;
    let rechts = diagramm.richtung == Richtung::Rechts;
    let kanal = lage.aussen + ABSTAND_GLEICH * 0.7;
    let unten = lage.unten;
    let mut laeufe = Vec::with_capacity(diagramm.kanten.len());

    for (index, kante) in diagramm.kanten.iter().enumerate() {
        let (von, nach) = kanten[index];
        let a = &plaetze[von];
        let b = &plaetze[nach];
        let reihe_a = raster.reihe_von[von];
        let reihe_b = raster.reihe_von[nach];
        let direkt = !rueck[index] && raenge[nach] == raenge[von] + 1;

        let punkte = if direkt && rechts {
            let start = Punkt::neu(a.x + a.breite, a.y + a.hoehe / 2.0);
            let ende = Punkt::neu(b.x, b.y + b.hoehe / 2.0);
            let mitte = raster.dahinter(reihe_a);
            vec![
                start,
                Punkt::neu(mitte, start.y),
                Punkt::neu(mitte, ende.y),
                ende,
            ]
        } else if direkt {
            let start = Punkt::neu(a.x + a.breite / 2.0, a.y + a.hoehe);
            let ende = Punkt::neu(b.x + b.breite / 2.0, b.y);
            let mitte = raster.dahinter(reihe_a);
            vec![
                start,
                Punkt::neu(start.x, mitte),
                Punkt::neu(ende.x, mitte),
                ende,
            ]
        } else if rechts {
            // Außen herum: unter dem Inhalt entlang.
            let start = Punkt::neu(a.x + a.breite, a.y + a.hoehe / 2.0);
            let ende = Punkt::neu(b.x, b.y + b.hoehe / 2.0);
            vec![
                start,
                Punkt::neu(raster.dahinter(reihe_a), start.y),
                Punkt::neu(raster.dahinter(reihe_a), unten),
                Punkt::neu(raster.davor(reihe_b), unten),
                Punkt::neu(raster.davor(reihe_b), ende.y),
                ende,
            ]
        } else {
            // Außen herum: rechts am Inhalt vorbei, in den Zwischenräumen der
            // Reihen ein- und aussteigend.
            let start = Punkt::neu(a.x + a.breite / 2.0, a.y + a.hoehe);
            let ende = Punkt::neu(b.x + b.breite / 2.0, b.y);
            vec![
                start,
                Punkt::neu(start.x, raster.dahinter(reihe_a)),
                Punkt::neu(kanal, raster.dahinter(reihe_a)),
                Punkt::neu(kanal, raster.davor(reihe_b)),
                Punkt::neu(ende.x, raster.davor(reihe_b)),
                ende,
            ]
        };

        let beschriftung = kante
            .text
            .as_ref()
            .filter(|text| !text.trim().is_empty())
            .map(|text| beschriftung(text, &punkte));
        laeufe.push(KantenLauf {
            von: kante.von.clone(),
            nach: kante.nach.clone(),
            art: kante.art,
            punkte,
            beschriftung,
        });
    }
    laeufe
}

/// Setzt die Beschriftung über die Mitte des Linienzugs.
fn beschriftung(text: &str, punkte: &[Punkt]) -> Kantenbeschriftung {
    let zeilen = schrift::umbrechen(text, SCHRIFTGROESSE, 180.0);
    let breite = zeilen
        .iter()
        .map(|zeile| schrift::breite(zeile, SCHRIFTGROESSE))
        .fold(0.0_f32, f32::max)
        + 8.0;
    let mitte = mitte_des_wegs(punkte);
    let hoehe = zeilen.len() as f32 * ZEILENHOEHE;
    Kantenbeschriftung {
        text: text.to_string(),
        zeilen,
        position: Punkt::neu(mitte.x - breite / 2.0, mitte.y - hoehe - 4.0),
        breite,
    }
}

/// Der Punkt in der Mitte der Wegstrecke (nicht der Mitte der Stützpunkte).
fn mitte_des_wegs(punkte: &[Punkt]) -> Punkt {
    if punkte.is_empty() {
        return Punkt::neu(0.0, 0.0);
    }
    let mut laenge = 0.0;
    for paar in punkte.windows(2) {
        laenge += abstand(paar[0], paar[1]);
    }
    let mut rest = laenge / 2.0;
    for paar in punkte.windows(2) {
        let strecke = abstand(paar[0], paar[1]);
        if strecke >= rest && strecke > 0.0 {
            let anteil = rest / strecke;
            return Punkt::neu(
                paar[0].x + (paar[1].x - paar[0].x) * anteil,
                paar[0].y + (paar[1].y - paar[0].y) * anteil,
            );
        }
        rest -= strecke;
    }
    *punkte.last().expect("mindestens ein Punkt")
}

fn abstand(a: Punkt, b: Punkt) -> f32 {
    ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt()
}
