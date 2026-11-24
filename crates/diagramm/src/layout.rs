// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Geschichtetes Layout: aus Beschreibung werden Koordinaten.
//!
//! Der Ablauf ist der klassische: Kanten, die einen Kreis schließen, werden
//! erkannt und für die Rangvergabe ignoriert; dann bekommt jeder Knoten einen
//! Rang (Schicht), innerhalb der Schicht wird nach Baryzentrum sortiert, und
//! zum Schluss werden die Schichten gesetzt.
//!
//! Drei Zusagen prägen die Umsetzung:
//!
//! - **Gruppen sind Bänder.** Jede Gruppe bekommt einen Streifen quer zur
//!   Leserichtung, der in jeder Schicht reserviert bleibt. Knoten, die nicht zur
//!   Gruppe gehören, liegen deshalb nie in ihrem Rahmen, und zwei Rahmen
//!   überlappen sich nie.
//! - **Kanten laufen in den Zwischenräumen.** Jede Kante verlässt ihren Knoten
//!   an einer eigenen Stelle, läuft zwischen den Schichten und steigt dort in
//!   den Zielknoten. Ist ein fremder Rahmen im Weg, sucht der Router einen Weg
//!   außen herum.
//! - **Beschriftungen stehen auf ihrer Kante.** Jede Kantenbeschriftung sitzt
//!   mittig auf einem Stück *ihrer* Kante, weicht fremden Knoten, fremden Linien
//!   und anderen Beschriftungen aus und bekommt beim Zeichnen einen deckenden
//!   Hintergrund.
//!
//! Gerechnet wird durchgehend in einem Raster aus **Fluss** (in Leserichtung)
//! und **Quere** (quer dazu); erst am Ende werden daraus x und y. Dadurch gilt
//! derselbe Code für „unten“ und „rechts“.
//!
//! - **Textmaß** kommt aus der eingebetteten Schrift
//!   ([`crate::zeichnen::schrift`]); die Knotengröße folgt dem Text.
//! - **Gleiche Eingabe → gleiche Ausgabe.** Es gibt keinen Zufall und keine
//!   Hash-Reihenfolge; Gleichstände entscheidet der Index im Modell.

use crate::modell::{Diagramm, Farbe, Form, KanteArt, ModellFehler, Richtung};
use crate::zeichnen::schrift;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, VecDeque};

/// Schriftgröße der Knoten- und Kantentexte.
pub const SCHRIFTGROESSE: f32 = 14.0;

/// Höhe einer Textzeile.
pub const ZEILENHOEHE: f32 = SCHRIFTGROESSE * schrift::ZEILENFAKTOR;

/// Innenabstand im Knoten.
pub const INNEN: f32 = 16.0;

/// Abstand zwischen zwei Knoten derselben Schicht.
pub const ABSTAND_GLEICH: f32 = 40.0;

/// Grundabstand zwischen zwei Schichten (ohne den Platz für Gruppenköpfe).
pub const ABSTAND_SCHICHT: f32 = 84.0;

/// Rand um das ganze Diagramm.
pub const RAND: f32 = 32.0;

/// Rand zwischen Gruppenrahmen und Knoten.
pub const GRUPPEN_RAND: f32 = 18.0;

/// Höhe der Kopfzeile eines Gruppenrahmens.
pub const GRUPPEN_KOPF: f32 = 26.0;

/// Abstand zwischen Rahmenkante und Titeltext.
const TITEL_RAND: f32 = 12.0;

/// Sicherheitsabstand zwischen Kantenbahn und Rahmen im Zwischenraum.
const KANAL_RAND: f32 = 14.0;

/// Wie viele Kantenbahnen ein Zwischenraum bekommt.
const BAHNEN: usize = 3;

/// Abstand zweier Bahnen im selben Zwischenraum.
const BAHN_ABSTAND: f32 = 12.0;

/// Abstand des äußeren Kanals vom Inhalt.
const KANAL_ABSTAND: f32 = 40.0;

/// Höchstbreite einer Kantenbeschriftung vor dem Umbruch.
const BESCHRIFTUNG_BREITE: f32 = 180.0;

/// Wie viele Plätze je Umbruchbreite für eine Beschriftung geprüft werden.
const KANDIDATEN: usize = 90;

/// Strafkosten für eine Richtungsänderung im Gitterweg.
const BIEGE: u32 = 110;

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

    let rechts = diagramm.richtung == Richtung::Rechts;
    let masse: Vec<Masse> = diagramm
        .knoten
        .iter()
        .map(|knoten| masse(&knoten.text, knoten.form, rechts))
        .collect();

    let paare: Vec<(usize, usize)> = diagramm
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

    let rueck = rueckkanten(anzahl, &paare);
    let raenge = raenge(anzahl, &paare, &rueck);
    let mut schichten = schichten(anzahl, &raenge);
    let nachbarn = nachbarn(anzahl, &paare);
    let gruppe = gruppe_je_knoten(diagramm);
    ordnen(&mut schichten, &raenge, &nachbarn, &gruppe);

    let grund = grundriss(diagramm, &masse, &schichten, &gruppe, rechts);
    let wege = routen(&grund, &paare, &rueck, &gruppe);
    let stellen = beschriftungen(diagramm, &wege, &grund, &gruppe, rechts);

    // Alles zusammen in den sichtbaren Bereich schieben.
    let mut min_f = f32::INFINITY;
    let mut min_q = f32::INFINITY;
    let mut max_f = f32::NEG_INFINITY;
    let mut max_q = f32::NEG_INFINITY;
    let mut merke = |f: f32, q: f32| {
        min_f = min_f.min(f);
        min_q = min_q.min(q);
        max_f = max_f.max(f);
        max_q = max_q.max(q);
    };
    for kasten in &grund.knoten {
        merke(kasten.f, kasten.q);
        merke(kasten.ende_f(), kasten.ende_q());
    }
    for kasten in grund.rahmen.iter().flatten() {
        merke(kasten.f, kasten.q);
        merke(kasten.ende_f(), kasten.ende_q());
    }
    for weg in &wege {
        for punkt in weg {
            merke(punkt.0, punkt.1);
        }
    }
    for stelle in stellen.iter().flatten() {
        merke(stelle.kasten.f, stelle.kasten.q);
        merke(stelle.kasten.ende_f(), stelle.kasten.ende_q());
    }

    let versatz_f = RAND - min_f;
    let versatz_q = RAND - min_q;
    let achsen = Achsen { rechts };
    let nach = |f: f32, q: f32| (f + versatz_f, q + versatz_q);

    let knoten = diagramm
        .knoten
        .iter()
        .enumerate()
        .map(|(index, knoten)| {
            let (f, q) = nach(grund.knoten[index].f, grund.knoten[index].q);
            let mass = grund.knoten[index];
            KnotenPlatz {
                id: knoten.id.clone(),
                text: knoten.text.clone(),
                form: knoten.form,
                farbe: knoten.farbe,
                gruppe: knoten.gruppe.clone(),
                x: achsen.punkt(f, q).x,
                y: achsen.punkt(f, q).y,
                breite: if rechts { mass.lf } else { mass.lq },
                hoehe: if rechts { mass.lq } else { mass.lf },
                zeilen: masse[index].zeilen.clone(),
                schriftgroesse: SCHRIFTGROESSE,
            }
        })
        .collect();

    let kanten = diagramm
        .kanten
        .iter()
        .enumerate()
        .map(|(index, kante)| KantenLauf {
            von: kante.von.clone(),
            nach: kante.nach.clone(),
            art: kante.art,
            punkte: wege[index]
                .iter()
                .map(|(f, q)| {
                    let (f, q) = nach(*f, *q);
                    achsen.punkt(f, q)
                })
                .collect(),
            beschriftung: stellen[index].as_ref().map(|stelle| {
                let (f, q) = nach(stelle.kasten.f, stelle.kasten.q);
                Kantenbeschriftung {
                    text: kante.text.clone().unwrap_or_default(),
                    zeilen: stelle.zeilen.clone(),
                    position: achsen.punkt(f, q),
                    breite: if rechts {
                        stelle.kasten.lf
                    } else {
                        stelle.kasten.lq
                    },
                }
            }),
        })
        .collect();

    let gruppen = diagramm
        .gruppen
        .iter()
        .enumerate()
        .filter_map(|(index, gruppe)| {
            let kasten = grund.rahmen[index]?;
            let (f, q) = nach(kasten.f, kasten.q);
            let ecke = achsen.punkt(f, q);
            Some(GruppenRahmen {
                id: gruppe.id.clone(),
                titel: gruppe.titel.clone(),
                x: ecke.x,
                y: ecke.y,
                breite: if rechts { kasten.lf } else { kasten.lq },
                hoehe: if rechts { kasten.lq } else { kasten.lf },
            })
        })
        .collect();

    let (breite, hoehe) = if rechts {
        (max_f - min_f, max_q - min_q)
    } else {
        (max_q - min_q, max_f - min_f)
    };

    Ok(Zeichnung {
        breite: breite + 2.0 * RAND,
        hoehe: hoehe + 2.0 * RAND,
        knoten,
        kanten,
        gruppen,
    })
}

// ------------------------------------------------------------ Rasterkoordinaten

/// Übersetzt Rasterkoordinaten (Fluss, Quere) in Bildkoordinaten.
#[derive(Debug, Clone, Copy)]
struct Achsen {
    rechts: bool,
}

impl Achsen {
    fn punkt(self, f: f32, q: f32) -> Punkt {
        if self.rechts {
            Punkt::neu(f, q)
        } else {
            Punkt::neu(q, f)
        }
    }
}

/// Rechteck im Raster: `f` läuft in Leserichtung, `q` quer dazu.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Kasten {
    f: f32,
    q: f32,
    lf: f32,
    lq: f32,
}

impl Kasten {
    fn neu(f: f32, q: f32, lf: f32, lq: f32) -> Kasten {
        Kasten { f, q, lf, lq }
    }

    fn ende_f(self) -> f32 {
        self.f + self.lf
    }

    fn ende_q(self) -> f32 {
        self.q + self.lq
    }

    fn mitte_f(self) -> f32 {
        self.f + self.lf / 2.0
    }

    fn mitte_q(self) -> f32 {
        self.q + self.lq / 2.0
    }

    /// Liegt `q` im Inneren der Querausdehnung?
    fn umfasst_q(self, q: f32) -> bool {
        q > self.q + 0.01 && q < self.ende_q() - 0.01
    }

    /// Liegt `f` im Inneren der Flussausdehnung?
    fn umfasst_f(self, f: f32) -> bool {
        f > self.f + 0.01 && f < self.ende_f() - 0.01
    }

    /// Überlappt das Intervall `[a, b]` die Flussausdehnung?
    fn deckt_f(self, a: f32, b: f32) -> bool {
        let lo = a.min(b);
        let hi = a.max(b);
        self.f < hi - 0.01 && self.ende_f() > lo + 0.01
    }

    /// Überlappt das Intervall `[a, b]` die Querausdehnung?
    fn deckt_q(self, a: f32, b: f32) -> bool {
        let lo = a.min(b);
        let hi = a.max(b);
        self.q < hi - 0.01 && self.ende_q() > lo + 0.01
    }

    fn schneidet(self, andere: Kasten) -> bool {
        self.f < andere.ende_f() - 0.01
            && andere.f < self.ende_f() - 0.01
            && self.q < andere.ende_q() - 0.01
            && andere.q < self.ende_q() - 0.01
    }
}

/// Trifft die achsenparallele Strecke `a`–`b` den Kasten (Inneres)?
fn strecke_trifft(kasten: Kasten, a: (f32, f32), b: (f32, f32)) -> bool {
    let (f_lo, f_hi) = (a.0.min(b.0), a.0.max(b.0));
    let (q_lo, q_hi) = (a.1.min(b.1), a.1.max(b.1));
    if (f_hi - f_lo).abs() < 0.01 {
        // Querlauf: fester Flusswert, Querintervall
        q_hi.min(kasten.ende_q()) - q_lo.max(kasten.q) > 0.01 && kasten.umfasst_f(f_lo)
    } else if (q_hi - q_lo).abs() < 0.01 {
        // Flusslauf: fester Querwert, Flussintervall
        f_hi.min(kasten.ende_f()) - f_lo.max(kasten.f) > 0.01 && kasten.umfasst_q(q_lo)
    } else {
        f_hi.min(kasten.ende_f()) - f_lo.max(kasten.f) > 0.01
            && q_hi.min(kasten.ende_q()) - q_lo.max(kasten.q) > 0.01
    }
}

// ------------------------------------------------------------------- Textmaß

#[derive(Debug, Clone)]
struct Masse {
    /// Ausdehnung quer zur Leserichtung.
    quer: f32,
    /// Ausdehnung in Leserichtung.
    laengs: f32,
    zeilen: Vec<String>,
}

/// Knotengröße aus Text und Form.
fn masse(text: &str, form: Form, rechts: bool) -> Masse {
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
    let (quer, laengs) = if rechts {
        (hoehe, breite)
    } else {
        (breite, hoehe)
    };
    Masse {
        quer: aufrunden(quer),
        laengs: aufrunden(laengs),
        zeilen,
    }
}

fn aufrunden(wert: f32) -> f32 {
    ((wert / 2.0).ceil() * 2.0).max(4.0)
}

// ------------------------------------------------------------------ Rangordnung

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

/// Gruppe je Knoten: nur Gruppen mit mindestens einem Mitglied zählen.
fn gruppe_je_knoten(diagramm: &Diagramm) -> Vec<Option<usize>> {
    let mut mitglied = vec![false; diagramm.gruppen.len()];
    let roh: Vec<Option<usize>> = diagramm
        .knoten
        .iter()
        .map(|knoten| {
            knoten
                .gruppe
                .as_ref()
                .and_then(|id| diagramm.gruppen.iter().position(|g| &g.id == id))
        })
        .collect();
    for index in roh.iter().flatten() {
        mitglied[*index] = true;
    }
    roh.into_iter()
        .map(|index| index.filter(|i| mitglied[*i]))
        .collect()
}

/// Ein Posten der Querachse: eine ganze Gruppe oder ein freier Knoten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Gruppe(usize),
    Frei(usize),
}

fn slot_id(slot: Slot, gruppen_anzahl: usize) -> usize {
    match slot {
        Slot::Gruppe(g) => g,
        Slot::Frei(n) => gruppen_anzahl + n,
    }
}

/// Die Posten einer Schicht in ihrer Reihenfolge; Gruppenmitglieder stehen
/// zusammen und werden zu einem Posten.
fn posten_der_schicht(schicht: &[usize], gruppe: &[Option<usize>]) -> Vec<(Slot, Vec<usize>)> {
    let mut posten: Vec<(Slot, Vec<usize>)> = Vec::new();
    for knoten in schicht {
        let slot = match gruppe[*knoten] {
            Some(g) => Slot::Gruppe(g),
            None => Slot::Frei(*knoten),
        };
        match posten.last_mut() {
            Some((letzter, liste)) if *letzter == slot => liste.push(*knoten),
            _ => posten.push((slot, vec![*knoten])),
        }
    }
    posten
}

/// Sortiert die Schichten nach Baryzentrum, abwechselnd vorwärts und rückwärts.
/// Gruppenmitglieder bleiben dabei zusammen: ein Posten zählt mit dem Mittel
/// seiner Mitglieder.
fn ordnen(
    schichten: &mut [Vec<usize>],
    raenge: &[usize],
    nachbarn: &[Vec<usize>],
    gruppe: &[Option<usize>],
) {
    for runde in 0..8 {
        let vorwaerts = runde % 2 == 0;
        let reihen: Vec<usize> = if vorwaerts {
            (0..schichten.len()).collect()
        } else {
            (0..schichten.len()).rev().collect()
        };
        for reihe in reihen {
            let lage = lagen(schichten);
            let mut gruppen: Vec<(usize, Vec<(f32, usize)>)> = Vec::new();
            let mut frei: Vec<(f32, usize)> = Vec::new();
            for knoten in &schichten[reihe] {
                let wert = baryzentrum(*knoten, &lage, raenge, nachbarn);
                match gruppe[*knoten] {
                    Some(g) => match gruppen.iter_mut().find(|(id, _)| *id == g) {
                        Some((_, liste)) => liste.push((wert, *knoten)),
                        None => gruppen.push((g, vec![(wert, *knoten)])),
                    },
                    None => frei.push((wert, *knoten)),
                }
            }
            let mut posten: Vec<(f32, usize, Vec<usize>)> = Vec::new();
            for (_, mut liste) in gruppen {
                liste.sort_by(|a, b| {
                    a.0.partial_cmp(&b.0)
                        .unwrap_or(Ordering::Equal)
                        .then_with(|| a.1.cmp(&b.1))
                });
                let mittel = liste.iter().map(|(wert, _)| *wert).sum::<f32>() / liste.len() as f32;
                let kleinster = liste.iter().map(|(_, k)| *k).min().unwrap_or(0);
                posten.push((
                    mittel,
                    kleinster,
                    liste.into_iter().map(|(_, k)| k).collect(),
                ));
            }
            for (wert, knoten) in frei {
                posten.push((wert, knoten, vec![knoten]));
            }
            posten.sort_by(|a, b| {
                a.0.partial_cmp(&b.0)
                    .unwrap_or(Ordering::Equal)
                    .then_with(|| a.1.cmp(&b.1))
            });
            schichten[reihe] = posten.into_iter().flat_map(|(_, _, liste)| liste).collect();
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

/// Eine Reihenfolge der Posten, die in allen Schichten gilt.
///
/// Maß ist die mittlere Position des Postens in seinen Schichten (nicht die
/// normierte, sonst mischen sich Schichten mit verschieden vielen Knoten).
fn slot_folge(
    schichten: &[Vec<usize>],
    gruppe: &[Option<usize>],
    gruppen_anzahl: usize,
    anzahl: usize,
) -> Vec<Slot> {
    let gesamt = gruppen_anzahl + anzahl;
    let mut summe = vec![0.0_f32; gesamt];
    let mut zaehler = vec![0_usize; gesamt];
    let mut kleinster = vec![usize::MAX; gesamt];
    for schicht in schichten {
        let posten = posten_der_schicht(schicht, gruppe);
        for (position, (slot, mitglieder)) in posten.iter().enumerate() {
            let id = slot_id(*slot, gruppen_anzahl);
            summe[id] += position as f32;
            zaehler[id] += 1;
            for knoten in mitglieder {
                kleinster[id] = kleinster[id].min(*knoten);
            }
        }
    }
    let mut folge: Vec<Slot> = Vec::new();
    for (g, anzahl) in zaehler.iter().enumerate().take(gruppen_anzahl) {
        if *anzahl > 0 {
            folge.push(Slot::Gruppe(g));
        }
    }
    for n in 0..anzahl {
        let id = slot_id(Slot::Frei(n), gruppen_anzahl);
        if zaehler[id] > 0 {
            folge.push(Slot::Frei(n));
        }
    }
    folge.sort_by(|a, b| {
        let ia = slot_id(*a, gruppen_anzahl);
        let ib = slot_id(*b, gruppen_anzahl);
        let wa = summe[ia] / zaehler[ia].max(1) as f32;
        let wb = summe[ib] / zaehler[ib].max(1) as f32;
        wa.partial_cmp(&wb)
            .unwrap_or(Ordering::Equal)
            .then_with(|| kleinster[ia].cmp(&kleinster[ib]))
    });
    folge
}

/// Ein Platz auf der Querachse: eine Spur für Gruppenbänder oder ein Bereich
/// für freie Knoten.
#[derive(Debug, Clone, Copy)]
struct Querposten {
    /// `Some(spur)` für Gruppenbänder, `None` für einen Bereich freier Knoten.
    spur: Option<usize>,
    /// Nummer des Bereichs (nur bei `spur == None` sinnvoll).
    region: usize,
    start: f32,
    groesse: f32,
}

// -------------------------------------------------------------------- Grundriss

/// Die gesamte Geometrie in Rasterkoordinaten.
struct Grundriss {
    /// Kasten je Knoten (in Modellreihenfolge).
    knoten: Vec<Kasten>,
    /// Form je Knoten (bestimmt, wo eine Kante den Knoten trifft).
    formen: Vec<Form>,
    /// Schicht je Knoten.
    reihe_von: Vec<usize>,
    /// Zwischenraum vor/hinter jeder Schicht: (Anfang, Ende) für Kantenbahnen.
    zwischen: Vec<(f32, f32)>,
    /// Rahmen je Gruppe (Modellreihenfolge), auch für Gruppen ohne Mitglieder.
    rahmen: Vec<Option<Kasten>>,
    /// Titelfelder der Rahmen (Hindernisse für Beschriftungen).
    titel: Vec<Kasten>,
    /// Äußere Kante in Querrichtung (Kanal für Kanten).
    quer: f32,
    /// Innere Kante in Querrichtung.
    quer_min: f32,
    /// Anzahl der Schichten.
    schichten: usize,
}

fn grundriss(
    diagramm: &Diagramm,
    masse: &[Masse],
    schichten: &[Vec<usize>],
    gruppe: &[Option<usize>],
    rechts: bool,
) -> Grundriss {
    let anzahl = masse.len();
    let gruppen_anzahl = diagramm.gruppen.len();
    let folge = slot_folge(schichten, gruppe, gruppen_anzahl, anzahl);

    let mut rang = vec![usize::MAX; gruppen_anzahl + anzahl];
    for (index, slot) in folge.iter().enumerate() {
        rang[slot_id(*slot, gruppen_anzahl)] = index;
    }

    let posten_je_schicht: Vec<Vec<(Slot, Vec<usize>)>> = schichten
        .iter()
        .map(|schicht| {
            let mut posten = posten_der_schicht(schicht, gruppe);
            posten.sort_by_key(|(slot, _)| rang[slot_id(*slot, gruppen_anzahl)]);
            posten
        })
        .collect();

    // Erste und letzte Schicht jeder Gruppe.
    let mut reihen = vec![(usize::MAX, 0_usize); gruppen_anzahl];
    for (reihe, posten) in posten_je_schicht.iter().enumerate() {
        for (slot, _) in posten {
            if let Slot::Gruppe(g) = slot {
                reihen[*g] = (reihen[*g].0.min(reihe), reihen[*g].1.max(reihe));
            }
        }
    }
    let mut beginnt_bei = vec![false; schichten.len()];
    let mut endet_bei = vec![false; schichten.len()];
    for (erste, letzte) in &reihen {
        if *erste != usize::MAX {
            beginnt_bei[*erste] = true;
            endet_bei[*letzte] = true;
        }
    }

    // Fluss: Schichtdicken und Zwischenräume, mit Platz für Gruppenköpfe.
    let schicht_lf: Vec<f32> = schichten
        .iter()
        .map(|schicht| {
            schicht
                .iter()
                .map(|k| masse[*k].laengs)
                .fold(0.0_f32, f32::max)
        })
        .collect();
    let aussen = 2.0 * KANAL_RAND + (BAHNEN as f32) * BAHN_ABSTAND;
    let mut schicht_f = vec![0.0_f32; schichten.len()];
    let mut zwischen: Vec<(f32, f32)> = Vec::with_capacity(schichten.len() + 1);
    zwischen.push((-aussen, -KANAL_RAND));
    for i in 1..schichten.len() {
        let ende_vorher = schicht_f[i - 1] + schicht_lf[i - 1];
        let schwanz = if endet_bei[i - 1] { GRUPPEN_RAND } else { 0.0 };
        let mut kopf = if beginnt_bei[i] { GRUPPEN_RAND } else { 0.0 };
        if beginnt_bei[i] && !rechts {
            kopf += GRUPPEN_KOPF;
        }
        let start = ende_vorher + ABSTAND_SCHICHT + schwanz + kopf;
        schicht_f[i] = start;
        zwischen.push((
            ende_vorher + schwanz + KANAL_RAND,
            start - kopf - KANAL_RAND,
        ));
    }
    let letzte = schichten.len() - 1;
    zwischen.push((
        schicht_f[letzte] + schicht_lf[letzte] + KANAL_RAND,
        schicht_f[letzte] + schicht_lf[letzte] + aussen,
    ));

    let mut knoten = vec![Kasten::neu(0.0, 0.0, 0.0, 0.0); anzahl];
    let mut reihe_von = vec![0_usize; anzahl];
    for (reihe, schicht) in schichten.iter().enumerate() {
        for k in schicht {
            knoten[*k].f = schicht_f[reihe] + (schicht_lf[reihe] - masse[*k].laengs) / 2.0;
            knoten[*k].lf = masse[*k].laengs;
            reihe_von[*k] = reihe;
        }
    }

    // Querbedarf der Gruppen: Mitglieder nebeneinander, dazu Rand und Kopf.
    let mut quer_gruppe = vec![0.0_f32; gruppen_anzahl];
    for posten in &posten_je_schicht {
        for (slot, mitglieder) in posten {
            if let Slot::Gruppe(g) = slot {
                let summe: f32 = mitglieder.iter().map(|k| masse[*k].quer).sum();
                let platz = summe + ABSTAND_GLEICH * mitglieder.len().saturating_sub(1) as f32;
                quer_gruppe[*g] = quer_gruppe[*g].max(platz);
            }
        }
    }
    let mut band_quer = vec![0.0_f32; gruppen_anzahl];
    for g in 0..gruppen_anzahl {
        let titel_breite =
            schrift::breite(&diagramm.gruppen[g].titel, SCHRIFTGROESSE) + 2.0 * TITEL_RAND;
        let mut breite = quer_gruppe[g] + 2.0 * GRUPPEN_RAND;
        if rechts {
            breite += GRUPPEN_KOPF;
        }
        band_quer[g] = aufrunden(breite.max(titel_breite));
    }

    // Spuren: Bänder, deren Flussbereiche sich nicht überlappen, teilen sich
    // einen Streifen. So bleibt das Diagramm kurz, ohne dass sich zwei Rahmen
    // überlagern können.
    let kopf_fluss = if rechts { 0.0 } else { GRUPPEN_KOPF };
    let mut band_fluss = vec![(0.0_f32, 0.0_f32); gruppen_anzahl];
    for g in 0..gruppen_anzahl {
        let (erste, letzte_reihe) = reihen[g];
        if erste == usize::MAX {
            continue;
        }
        band_fluss[g] = (
            schicht_f[erste] - GRUPPEN_RAND - kopf_fluss,
            schicht_f[letzte_reihe] + schicht_lf[letzte_reihe] + GRUPPEN_RAND,
        );
    }
    let mut spur = vec![usize::MAX; gruppen_anzahl];
    let mut spur_groesse: Vec<f32> = Vec::new();
    let mut spur_bereiche: Vec<Vec<(f32, f32)>> = Vec::new();
    for slot in &folge {
        let Slot::Gruppe(g) = *slot else {
            continue;
        };
        let mut ziel = None;
        if let Some(letzte_spur) = spur_groesse.len().checked_sub(1) {
            let frei = spur_bereiche[letzte_spur]
                .iter()
                .all(|(a, b)| band_fluss[g].1 <= *a + 0.01 || band_fluss[g].0 >= *b - 0.01);
            if frei {
                ziel = Some(letzte_spur);
            }
        }
        match ziel {
            Some(index) => {
                spur[g] = index;
                spur_bereiche[index].push(band_fluss[g]);
                spur_groesse[index] = spur_groesse[index].max(band_quer[g]);
            }
            None => {
                spur[g] = spur_groesse.len();
                spur_bereiche.push(vec![band_fluss[g]]);
                spur_groesse.push(band_quer[g]);
            }
        }
    }

    // Bereich je freiem Knoten: hinter der Spur des Bandes davor.
    let mut region_von = vec![0_usize; anzahl];
    let mut letzte_spur: Option<usize> = None;
    for slot in &folge {
        match slot {
            Slot::Gruppe(g) => letzte_spur = Some(spur[*g]),
            Slot::Frei(k) => region_von[*k] = letzte_spur.map(|s| s + 1).unwrap_or(0),
        }
    }
    let regionen = spur_groesse.len() + 1;

    // Querbedarf der Bereiche: die freien Knoten einer Schicht nebeneinander.
    let mut quer_region = vec![0.0_f32; regionen];
    for posten in &posten_je_schicht {
        let mut i = 0;
        while i < posten.len() {
            let Slot::Frei(knoten_index) = posten[i].0 else {
                i += 1;
                continue;
            };
            let region = region_von[knoten_index];
            let mut summe = 0.0;
            let mut n = 0_usize;
            while i < posten.len() {
                match posten[i].0 {
                    Slot::Frei(k) if region_von[k] == region => {
                        summe += masse[k].quer;
                        n += 1;
                        i += 1;
                    }
                    _ => break,
                }
            }
            let platz = summe + ABSTAND_GLEICH * n.saturating_sub(1) as f32;
            quer_region[region] = quer_region[region].max(platz);
        }
    }

    // Reihenfolge der Querposten: Bereiche und Spuren abwechselnd.
    let mut quer_folge: Vec<Querposten> = Vec::new();
    quer_folge.push(Querposten {
        spur: None,
        region: 0,
        start: 0.0,
        groesse: quer_region[0],
    });
    for (index, groesse) in spur_groesse.iter().enumerate() {
        let region = index + 1;
        quer_folge.push(Querposten {
            spur: Some(index),
            region,
            start: 0.0,
            groesse: *groesse,
        });
        quer_folge.push(Querposten {
            spur: None,
            region,
            start: 0.0,
            groesse: quer_region[region],
        });
    }
    let mut quer_folge: Vec<Querposten> = quer_folge
        .into_iter()
        .filter(|posten| posten.groesse > 0.5)
        .collect();
    let mut lauf = 0.0_f32;
    for (index, posten) in quer_folge.iter_mut().enumerate() {
        if index > 0 {
            lauf += ABSTAND_GLEICH;
        }
        posten.start = lauf;
        lauf += posten.groesse;
    }
    let quer_gesamt = lauf;

    let mut spur_start = vec![0.0_f32; spur_groesse.len()];
    let mut region_start = vec![0.0_f32; regionen];
    for posten in &quer_folge {
        match posten.spur {
            Some(index) => spur_start[index] = posten.start,
            None => region_start[posten.region] = posten.start,
        }
    }
    let mut band_start = vec![0.0_f32; gruppen_anzahl];
    for g in 0..gruppen_anzahl {
        if spur[g] == usize::MAX {
            continue;
        }
        band_start[g] = spur_start[spur[g]] + (spur_groesse[spur[g]] - band_quer[g]) / 2.0;
    }

    // Knoten quer setzen.
    for posten in &posten_je_schicht {
        let mut i = 0;
        while i < posten.len() {
            match posten[i].0 {
                Slot::Gruppe(g) => {
                    let mitglieder = &posten[i].1;
                    let kopf = if rechts { GRUPPEN_KOPF } else { 0.0 };
                    let innen_start = band_start[g] + GRUPPEN_RAND + kopf;
                    let innen = band_quer[g] - 2.0 * GRUPPEN_RAND - kopf;
                    let summe: f32 = mitglieder.iter().map(|k| masse[*k].quer).sum();
                    let platz = summe + ABSTAND_GLEICH * mitglieder.len().saturating_sub(1) as f32;
                    let mut q = innen_start + ((innen - platz) / 2.0).max(0.0);
                    for k in mitglieder {
                        knoten[*k].q = q;
                        knoten[*k].lq = masse[*k].quer;
                        q += masse[*k].quer + ABSTAND_GLEICH;
                    }
                    i += 1;
                }
                Slot::Frei(knoten_index) => {
                    let region = region_von[knoten_index];
                    let mut kette: Vec<usize> = Vec::new();
                    while i < posten.len() {
                        match posten[i].0 {
                            Slot::Frei(k) if region_von[k] == region => {
                                kette.push(k);
                                i += 1;
                            }
                            _ => break,
                        }
                    }
                    let summe: f32 = kette.iter().map(|k| masse[*k].quer).sum();
                    let platz = summe + ABSTAND_GLEICH * kette.len().saturating_sub(1) as f32;
                    let mut q = region_start[region] + (quer_region[region] - platz) / 2.0;
                    for k in kette {
                        knoten[k].q = q;
                        knoten[k].lq = masse[k].quer;
                        q += masse[k].quer + ABSTAND_GLEICH;
                    }
                }
            }
        }
    }

    // Rahmen.
    let mut rahmen = vec![None; gruppen_anzahl];
    let mut titel = Vec::new();
    for g in 0..gruppen_anzahl {
        if reihen[g].0 == usize::MAX {
            continue;
        }
        let (f_start, f_ende) = band_fluss[g];
        let kasten = Kasten::neu(f_start, band_start[g], f_ende - f_start, band_quer[g]);
        rahmen[g] = Some(kasten);
        titel.push(titel_kasten(kasten, &diagramm.gruppen[g].titel, rechts));
    }

    // Leere Gruppen: unter dem Inhalt, damit sie sichtbar bleiben.
    let inhalt_ende = schicht_f[letzte] + schicht_lf[letzte];
    let mut q = 0.0_f32;
    for g in 0..gruppen_anzahl {
        if reihen[g].0 != usize::MAX {
            continue;
        }
        let lf = if rechts {
            2.0 * GRUPPEN_RAND
        } else {
            GRUPPEN_KOPF + GRUPPEN_RAND
        };
        let lq = (schrift::breite(&diagramm.gruppen[g].titel, SCHRIFTGROESSE) + 2.0 * TITEL_RAND)
            .max(180.0);
        let kasten = Kasten::neu(inhalt_ende + aussen + 20.0, q, lf, lq);
        rahmen[g] = Some(kasten);
        titel.push(titel_kasten(kasten, &diagramm.gruppen[g].titel, rechts));
        q += lq + ABSTAND_GLEICH;
    }

    let mut quer = quer_gesamt;
    let mut quer_min = 0.0_f32;
    for kasten in knoten.iter().chain(rahmen.iter().flatten()) {
        quer = quer.max(kasten.ende_q());
        quer_min = quer_min.min(kasten.q);
    }

    Grundriss {
        knoten,
        formen: diagramm.knoten.iter().map(|knoten| knoten.form).collect(),
        reihe_von,
        zwischen,
        rahmen,
        titel,
        quer,
        quer_min,
        schichten: schichten.len(),
    }
}

/// Feld, das der Titel eines Rahmens einnimmt.
fn titel_kasten(rahmen: Kasten, titel: &str, rechts: bool) -> Kasten {
    let breite = schrift::breite(titel, SCHRIFTGROESSE);
    let hoehe = ZEILENHOEHE;
    if rechts {
        Kasten::neu(rahmen.f + TITEL_RAND, rahmen.q + 4.0, breite, hoehe)
    } else {
        Kasten::neu(rahmen.f + 4.0, rahmen.q + TITEL_RAND, hoehe, breite)
    }
}

// ------------------------------------------------------------------- Kantenwege

impl Grundriss {
    /// Flusslage einer Bahn im Zwischenraum `zwischenraum`.
    fn bahn(&self, zwischenraum: usize, index: usize) -> f32 {
        let (lo, hi) = self.zwischen[zwischenraum];
        let schritt = (hi - lo) / BAHNEN as f32;
        lo + schritt * (index.min(BAHNEN - 1) as f32 + 0.5)
    }
}

/// Gruppe der beiden Enden einer Kante.
fn ausnahmen(gruppe: &[Option<usize>], von: usize, nach: usize) -> Vec<usize> {
    let mut liste = Vec::new();
    for index in [gruppe[von], gruppe[nach]].into_iter().flatten() {
        if !liste.contains(&index) {
            liste.push(index);
        }
    }
    liste
}

/// Ist der Querlauf auf dieser Flusslage frei?
fn quer_offen(hindernisse: &[Kasten], f: f32, q_a: f32, q_b: f32) -> bool {
    hindernisse
        .iter()
        .all(|kasten| !(kasten.umfasst_f(f) && kasten.deckt_q(q_a, q_b)))
}

/// Ist der Flusslauf auf dieser Querlage frei?
fn fluss_offen(hindernisse: &[Kasten], f_a: f32, f_b: f32, q: f32) -> bool {
    hindernisse
        .iter()
        .all(|kasten| !(kasten.umfasst_q(q) && kasten.deckt_f(f_a, f_b)))
}

/// Baut die Linienzüge aller Kanten.
fn routen(
    grund: &Grundriss,
    paare: &[(usize, usize)],
    rueck: &[bool],
    gruppe: &[Option<usize>],
) -> Vec<Vec<(f32, f32)>> {
    let anzahl = grund.knoten.len();
    let mut ausgehend: Vec<Vec<usize>> = vec![Vec::new(); anzahl];
    let mut eingehend: Vec<Vec<usize>> = vec![Vec::new(); anzahl];
    for (index, (von, nach)) in paare.iter().enumerate() {
        ausgehend[*von].push(index);
        eingehend[*nach].push(index);
    }

    let mut austritt = vec![0.0_f32; paare.len()];
    let mut eintritt = vec![0.0_f32; paare.len()];
    for knoten in 0..anzahl {
        let kasten = grund.knoten[knoten];
        let mut liste = ausgehend[knoten].clone();
        liste.sort_by(|a, b| {
            let wa = grund.knoten[paare[*a].1].mitte_q();
            let wb = grund.knoten[paare[*b].1].mitte_q();
            wa.partial_cmp(&wb)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.cmp(b))
        });
        for (position, index) in liste.iter().enumerate() {
            austritt[*index] = port(kasten, position, liste.len());
        }
        let mut liste = eingehend[knoten].clone();
        liste.sort_by(|a, b| {
            let wa = grund.knoten[paare[*a].0].mitte_q();
            let wb = grund.knoten[paare[*b].0].mitte_q();
            wa.partial_cmp(&wb)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.cmp(b))
        });
        for (position, index) in liste.iter().enumerate() {
            eintritt[*index] = port(kasten, position, liste.len());
        }
    }

    // Bahnen: Kanten, deren Querlauf sich im selben Zwischenraum überschneidet,
    // bekommen verschiedene Höhen. So liegen parallele Kanten nebeneinander.
    let mut kanal = vec![0.0_f32; paare.len()];
    let mut lange = 0_usize;
    // (lo, hi, Kante, Austritt?)
    let mut belegung: Vec<Vec<(f32, f32, usize, bool)>> = vec![Vec::new(); grund.schichten + 1];
    for (index, (von, nach)) in paare.iter().enumerate() {
        let von_reihe = grund.reihe_von[*von];
        let nach_reihe = grund.reihe_von[*nach];
        let aus = (von_reihe + 1).min(grund.schichten);
        let ein = nach_reihe;
        if nach_reihe == von_reihe + 1 && !rueck[index] {
            let lo = austritt[index].min(eintritt[index]);
            let hi = austritt[index].max(eintritt[index]);
            belegung[aus].push((lo, hi, index, true));
        } else {
            kanal[index] = grund.quer + KANAL_ABSTAND + lange as f32 * BAHN_ABSTAND;
            lange += 1;
            let c1 = austritt[index];
            let c2 = eintritt[index];
            belegung[aus].push((c1.min(kanal[index]), c1.max(kanal[index]), index, true));
            belegung[ein].push((c2.min(kanal[index]), c2.max(kanal[index]), index, false));
        }
    }
    let mut bahn_ab = vec![0_usize; paare.len()];
    let mut bahn_an = vec![0_usize; paare.len()];
    for liste in belegung.iter_mut() {
        liste.sort_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.2.cmp(&b.2))
                .then_with(|| a.3.cmp(&b.3))
        });
        let mut belegt: Vec<Vec<(f32, f32)>> = vec![Vec::new(); BAHNEN];
        let mut gesetzt = vec![false; liste.len()];
        // Erst mit Abstand verteilen (dann sieht man zwei Läufe nicht als einen
        // durchgehenden Strich), dann – wenn die Bahnen knapp werden – ohne.
        for trennung in [ABSTAND_GLEICH, 0.5] {
            for (position, (lo, hi, index, austritt)) in liste.iter().enumerate() {
                if gesetzt[position] {
                    continue;
                }
                for (nummer, strecken) in belegt.iter_mut().enumerate() {
                    let frei = strecken
                        .iter()
                        .all(|(l, h)| *hi + trennung <= *l + 0.01 || *h + trennung <= *lo + 0.01);
                    if frei {
                        strecken.push((*lo, *hi));
                        if *austritt {
                            bahn_ab[*index] = nummer;
                        } else {
                            bahn_an[*index] = nummer;
                        }
                        gesetzt[position] = true;
                        break;
                    }
                }
            }
        }
    }

    let mut wege = Vec::with_capacity(paare.len());
    for (index, (von, nach)) in paare.iter().enumerate() {
        let von_reihe = grund.reihe_von[*von];
        let nach_reihe = grund.reihe_von[*nach];
        let a = grund.knoten[*von];
        let b = grund.knoten[*nach];
        let ab = grund.bahn((von_reihe + 1).min(grund.schichten), bahn_ab[index]);
        let an = grund.bahn(nach_reihe, bahn_an[index]);
        let c1 = austritt[index];
        let c2 = eintritt[index];
        let abflug = randpunkt(a, grund.formen[*von], c1, true);
        let ankunft = randpunkt(b, grund.formen[*nach], c2, false);
        let mut punkte = vec![(abflug, c1), (ab, c1)];
        if nach_reihe == von_reihe + 1 && !rueck[index] {
            punkte.push((ab, c2));
        } else {
            punkte.push((ab, kanal[index]));
            punkte.push((an, kanal[index]));
            punkte.push((an, c2));
        }
        punkte.push((ankunft, c2));
        let mut punkte = aufraeumen(punkte);

        let ausnahme = ausnahmen(gruppe, *von, *nach);
        if !wege_frei(grund, &punkte, &ausnahme) {
            if let Some(ersatz) = gitterweg(grund, &ausnahme, (ab, c1), (an, c2)) {
                punkte = ersatz;
            }
        }
        wege.push(punkte);
    }
    wege
}

/// Flusslage, an der eine Kante den Knoten verlässt (`hinaus`) oder betritt.
///
/// Ein Rechteck wird an seiner Kante getroffen, eine Raute und eine Ellipse an
/// ihrer schrägen bzw. runden Fläche – sonst hinge der Pfeil in der Luft.
fn randpunkt(kasten: Kasten, form: Form, q: f32, hinaus: bool) -> f32 {
    let halb = (kasten.lq / 2.0).max(0.01);
    let versatz = (q - kasten.mitte_q()).abs().min(halb);
    let anteil = match form {
        Form::Raute => 1.0 - versatz / halb,
        Form::Ellipse => (1.0 - (versatz / halb).powi(2)).max(0.0).sqrt(),
        _ => 1.0,
    };
    if hinaus {
        kasten.mitte_f() + kasten.lf / 2.0 * anteil
    } else {
        kasten.mitte_f() - kasten.lf / 2.0 * anteil
    }
}

/// Austritts- bzw. Eintrittsstelle an einem Knoten.
fn port(kasten: Kasten, position: usize, anzahl: usize) -> f32 {
    if anzahl <= 1 {
        kasten.mitte_q()
    } else {
        kasten.q + kasten.lq * (position as f32 + 1.0) / (anzahl as f32 + 1.0)
    }
}

/// Prüft einen Weg gegen alle fremden Rahmen.
fn wege_frei(grund: &Grundriss, punkte: &[(f32, f32)], ausnahme: &[usize]) -> bool {
    for (index, rahmen) in grund.rahmen.iter().enumerate() {
        if ausnahme.contains(&index) {
            continue;
        }
        if let Some(rahmen) = rahmen {
            for paar in punkte.windows(2) {
                if strecke_trifft(*rahmen, paar[0], paar[1]) {
                    return false;
                }
            }
        }
    }
    true
}

/// Entfernt doppelte und überflüssige Stützpunkte.
fn aufraeumen(punkte: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    let mut aus: Vec<(f32, f32)> = Vec::with_capacity(punkte.len());
    for punkt in punkte {
        match aus.last() {
            Some(letzter)
                if (letzter.0 - punkt.0).abs() < 0.01 && (letzter.1 - punkt.1).abs() < 0.01 =>
            {
                continue
            }
            _ => aus.push(punkt),
        }
    }
    let mut ergebnis: Vec<(f32, f32)> = Vec::with_capacity(aus.len());
    for punkt in aus {
        if ergebnis.len() >= 2 {
            let a = ergebnis[ergebnis.len() - 2];
            let b = ergebnis[ergebnis.len() - 1];
            let gleiche_f = (a.0 - b.0).abs() < 0.01 && (b.0 - punkt.0).abs() < 0.01;
            let gleiche_q = (a.1 - b.1).abs() < 0.01 && (b.1 - punkt.1).abs() < 0.01;
            let vorwaerts = if gleiche_f {
                (b.1 - a.1) * (punkt.1 - b.1) >= 0.0
            } else {
                (b.0 - a.0) * (punkt.0 - b.0) >= 0.0
            };
            if (gleiche_f || gleiche_q) && vorwaerts {
                ergebnis.pop();
            }
        }
        ergebnis.push(punkt);
    }
    ergebnis
}

/// Sucht im Raster einen Weg um fremde Rahmen und Knoten herum.
///
/// Das Gitter besteht aus den Kantenbahnen der Zwischenräume (Fluss) und den
/// Rändern von Bändern, Knoten und Beschriftungen (Quere). Ein Dijkstra mit
/// Strafkosten für Richtungswechsel findet den kürzesten Weg; er wird nur
/// gebraucht, wenn der einfache Weg durch einen fremden Rahmen liefe.
fn gitterweg(
    grund: &Grundriss,
    ausnahme: &[usize],
    start: (f32, f32),
    ziel: (f32, f32),
) -> Option<Vec<(f32, f32)>> {
    // Flusslinien: alle Bahnen aller Zwischenräume.
    let mut fs: Vec<f32> = Vec::new();
    for zwischenraum in 0..=grund.schichten {
        for index in 0..BAHNEN {
            fs.push(grund.bahn(zwischenraum, index));
        }
    }
    fs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    fs.dedup_by(|a, b| (*a - *b).abs() < 0.5);

    // Querlinien: Bänder, Knoten und die äußeren Kanäle.
    let mut qs: Vec<f32> = vec![start.1, ziel.1, grund.quer_min - KANAL_ABSTAND];
    qs.push(grund.quer + KANAL_ABSTAND);
    for kasten in &grund.knoten {
        qs.push(kasten.mitte_q());
        qs.push(kasten.q - 6.0);
        qs.push(kasten.ende_q() + 6.0);
    }
    for rahmen in grund.rahmen.iter().flatten() {
        qs.push(rahmen.q - 6.0);
        qs.push(rahmen.ende_q() + 6.0);
    }
    let innen = start.1.min(ziel.1) - 4.0 * ABSTAND_GLEICH;
    let aussen = start.1.max(ziel.1) + 4.0 * ABSTAND_GLEICH;
    qs.retain(|q| {
        (*q >= innen && *q <= aussen) || (*q - start.1).abs() < 0.01 || (*q - ziel.1).abs() < 0.01
    });
    qs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    qs.dedup_by(|a, b| (*a - *b).abs() < 0.5);

    // Nur Hindernisse, die im Fenster liegen, müssen geprüft werden.
    let (q_lo, q_hi) = (qs[0], qs[qs.len() - 1]);
    let mut hindernisse: Vec<Kasten> = Vec::new();
    for kasten in &grund.knoten {
        if kasten.deckt_q(q_lo, q_hi) {
            hindernisse.push(*kasten);
        }
    }
    for (index, rahmen) in grund.rahmen.iter().enumerate() {
        if ausnahme.contains(&index) {
            continue;
        }
        if let Some(rahmen) = rahmen {
            if rahmen.deckt_q(q_lo, q_hi) {
                hindernisse.push(*rahmen);
            }
        }
    }
    let finde_f = |f: f32| -> Option<usize> { fs.iter().position(|wert| (wert - f).abs() < 0.5) };
    let finde_q = |q: f32| -> Option<usize> { qs.iter().position(|wert| (wert - q).abs() < 0.5) };
    let (f_start, q_start) = (finde_f(start.0)?, finde_q(start.1)?);
    let (f_ziel, q_ziel) = (finde_f(ziel.0)?, finde_q(ziel.1)?);

    let nf = fs.len();
    let nq = qs.len();
    if nf * nq * 4 > 60_000 {
        return None;
    }
    let zustand = |fi: usize, qi: usize, richtung: usize| (fi * nq + qi) * 4 + richtung;
    let mut kosten = vec![u32::MAX; nf * nq * 4];
    let mut vor = vec![u32::MAX; nf * nq * 4];
    let mut haufen: BinaryHeap<std::cmp::Reverse<(u32, u32)>> = BinaryHeap::new();
    for richtung in 0..4 {
        let s = zustand(f_start, q_start, richtung);
        kosten[s] = 0;
        haufen.push(std::cmp::Reverse((0, s as u32)));
    }

    let schritt = |wert: f32| (wert * 4.0).round() as u32;
    while let Some(std::cmp::Reverse((kosten_bisher, s))) = haufen.pop() {
        let s = s as usize;
        if kosten_bisher > kosten[s] {
            continue;
        }
        let richtung = s % 4;
        let ort = s / 4;
        let fi = ort / nq;
        let qi = ort % nq;
        let mut nachbarn: Vec<(usize, usize, usize, bool)> = Vec::new();
        if fi + 1 < nf {
            nachbarn.push((fi + 1, qi, 0, false));
        }
        if fi > 0 {
            nachbarn.push((fi - 1, qi, 1, false));
        }
        if qi + 1 < nq {
            nachbarn.push((fi, qi + 1, 2, true));
        }
        if qi > 0 {
            nachbarn.push((fi, qi - 1, 3, true));
        }
        for (nf_i, nq_i, neue_richtung, quer) in nachbarn {
            let frei = if quer {
                quer_offen(&hindernisse, fs[fi], qs[qi], qs[nq_i])
            } else {
                fluss_offen(&hindernisse, fs[fi], fs[nf_i], qs[qi])
            };
            if !frei {
                continue;
            }
            let laenge = if quer {
                (qs[nq_i] - qs[qi]).abs()
            } else {
                (fs[nf_i] - fs[fi]).abs()
            };
            let mut neu = kosten_bisher + schritt(laenge);
            if richtung != neue_richtung {
                neu += BIEGE;
            }
            let zi = zustand(nf_i, nq_i, neue_richtung);
            if neu < kosten[zi] {
                kosten[zi] = neu;
                vor[zi] = s as u32;
                haufen.push(std::cmp::Reverse((neu, zi as u32)));
            }
        }
    }

    let mut bestes: Option<(u32, usize)> = None;
    for richtung in 0..4 {
        let z = zustand(f_ziel, q_ziel, richtung);
        if kosten[z] != u32::MAX && bestes.map(|(k, _)| kosten[z] < k).unwrap_or(true) {
            bestes = Some((kosten[z], z));
        }
    }
    let (_, mut aktuell) = bestes?;
    let mut punkte: Vec<(f32, f32)> = Vec::new();
    loop {
        let ort = aktuell / 4;
        punkte.push((fs[ort / nq], qs[ort % nq]));
        match vor[aktuell] {
            u32::MAX => break,
            vorher => aktuell = vorher as usize,
        }
    }
    punkte.reverse();
    let mut weg = vec![(start.0, start.1)];
    weg.extend(punkte);
    weg.push((ziel.0, ziel.1));
    Some(aufraeumen(weg))
}

// --------------------------------------------------------------- Beschriftungen

/// Eine platzierte Kantenbeschriftung samt ihrem Feld.
struct Stelle {
    zeilen: Vec<String>,
    kasten: Kasten,
}

/// Setzt alle Kantenbeschriftungen mittig auf ein Stück ihrer eigenen Kante.
fn beschriftungen(
    diagramm: &Diagramm,
    wege: &[Vec<(f32, f32)>],
    grund: &Grundriss,
    gruppe: &[Option<usize>],
    rechts: bool,
) -> Vec<Option<Stelle>> {
    let mut stellen: Vec<Option<Stelle>> = Vec::with_capacity(diagramm.kanten.len());
    let mut belegt: Vec<Kasten> = Vec::new();
    let weg_kasten: Vec<Kasten> = wege.iter().map(|weg| umkasten(weg)).collect();
    for (index, kante) in diagramm.kanten.iter().enumerate() {
        let text = kante
            .text
            .as_ref()
            .map(|text| text.trim())
            .filter(|text| !text.is_empty());
        let Some(text) = text else {
            stellen.push(None);
            continue;
        };
        let ausnahme = ausnahmen(
            gruppe,
            index_von(diagramm, &kante.von),
            index_von(diagramm, &kante.nach),
        );
        let mut beste: Option<(usize, Stelle)> = None;
        for breite_max in [BESCHRIFTUNG_BREITE, 130.0, 90.0, 60.0, 42.0] {
            let zeilen = schrift::umbrechen(text, SCHRIFTGROESSE, breite_max);
            let breite = zeilen
                .iter()
                .map(|zeile| schrift::breite(zeile, SCHRIFTGROESSE))
                .fold(0.0_f32, f32::max)
                + 10.0;
            let hoehe = zeilen.len() as f32 * ZEILENHOEHE + 6.0;
            let (lf, lq) = if rechts {
                (breite, hoehe)
            } else {
                (hoehe, breite)
            };
            for (f, q) in kandidaten(&wege[index], lf, lq) {
                let kasten = Kasten::neu(f - lf / 2.0, q - lq / 2.0, lf, lq);
                let konflikte = konflikte(
                    grund,
                    &belegt,
                    wege,
                    &weg_kasten,
                    index,
                    ausnahme.as_slice(),
                    kasten,
                );
                if beste.as_ref().map(|(k, _)| konflikte < *k).unwrap_or(true) {
                    beste = Some((
                        konflikte,
                        Stelle {
                            zeilen: zeilen.clone(),
                            kasten,
                        },
                    ));
                }
                if konflikte == 0 {
                    break;
                }
            }
            if beste.as_ref().map(|(k, _)| *k == 0).unwrap_or(false) {
                break;
            }
        }
        match beste {
            Some((_, stelle)) => {
                belegt.push(stelle.kasten);
                stellen.push(Some(stelle));
            }
            None => stellen.push(None),
        }
    }
    stellen
}

fn index_von(diagramm: &Diagramm, id: &str) -> usize {
    diagramm.knoten_index(id).unwrap_or(0)
}

/// Mögliche Plätze für eine Beschriftung: zuerst mittig auf einem Stück des
/// Weges (nahe der Wegemitte zuerst), dann seitlich daneben.
fn kandidaten(weg: &[(f32, f32)], lf: f32, lq: f32) -> Vec<(f32, f32)> {
    let gesamt: f32 = weg.windows(2).map(|paar| abstand(paar[0], paar[1])).sum();
    let mut mittig: Vec<(f32, f32, f32)> = Vec::new();
    let mut seitlich: Vec<(f32, f32, f32)> = Vec::new();
    const ANTEILE: [f32; 7] = [0.5, 0.35, 0.65, 0.22, 0.78, 0.12, 0.88];
    let mut gelaufen = 0.0_f32;
    for paar in weg.windows(2) {
        let laenge = abstand(paar[0], paar[1]);
        let quer = (paar[1].0 - paar[0].0).abs() < 0.01;
        let versatz_f = if quer { lf / 2.0 + 1.0 } else { 0.0 };
        let versatz_q = if quer { 0.0 } else { lq / 2.0 + 1.0 };
        for anteil in ANTEILE {
            let ort = (gelaufen + laenge * anteil) / gesamt.max(0.01);
            let guete = (ort - 0.5).abs();
            let f = paar[0].0 + (paar[1].0 - paar[0].0) * anteil;
            let q = paar[0].1 + (paar[1].1 - paar[0].1) * anteil;
            mittig.push((guete, f, q));
            if versatz_f > 0.0 {
                seitlich.push((guete, f - versatz_f, q));
                seitlich.push((guete, f + versatz_f, q));
            } else {
                seitlich.push((guete, f, q - versatz_q));
                seitlich.push((guete, f, q + versatz_q));
            }
        }
        gelaufen += laenge;
    }
    mittig.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));
    seitlich.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));
    mittig
        .into_iter()
        .chain(seitlich)
        .take(KANDIDATEN)
        .map(|(_, f, q)| (f, q))
        .collect()
}

/// Umschließender Kasten eines Weges (auch für einen einzelnen Punkt).
fn umkasten(weg: &[(f32, f32)]) -> Kasten {
    let mut min_f = f32::INFINITY;
    let mut min_q = f32::INFINITY;
    let mut max_f = f32::NEG_INFINITY;
    let mut max_q = f32::NEG_INFINITY;
    for (f, q) in weg {
        min_f = min_f.min(*f);
        min_q = min_q.min(*q);
        max_f = max_f.max(*f);
        max_q = max_q.max(*q);
    }
    Kasten::neu(min_f, min_q, max_f - min_f, max_q - min_q)
}

/// Berührt der (auch punktförmige) Kasten das Feld?
fn kasten_trifft(umschliessend: Kasten, feld: Kasten) -> bool {
    umschliessend.deckt_f(feld.f, feld.ende_f()) && umschliessend.deckt_q(feld.q, feld.ende_q())
}

/// Zählt Zusammenstöße eines Beschriftungsfelds (0 heißt frei).
fn konflikte(
    grund: &Grundriss,
    belegt: &[Kasten],
    wege: &[Vec<(f32, f32)>],
    weg_kasten: &[Kasten],
    eigene: usize,
    ausnahme: &[usize],
    kasten: Kasten,
) -> usize {
    let mut anzahl = 0;
    for knoten in &grund.knoten {
        if knoten.schneidet(kasten) {
            anzahl += 3;
        }
    }
    for anderer in belegt {
        if anderer.schneidet(kasten) {
            anzahl += 3;
        }
    }
    for titel in &grund.titel {
        if titel.schneidet(kasten) {
            anzahl += 2;
        }
    }
    for (index, rahmen) in grund.rahmen.iter().enumerate() {
        if ausnahme.contains(&index) {
            continue;
        }
        if let Some(rahmen) = rahmen {
            if rahmen.schneidet(kasten) {
                anzahl += 1;
            }
        }
    }
    for (index, weg) in wege.iter().enumerate() {
        if index == eigene || !kasten_trifft(weg_kasten[index], kasten) {
            continue;
        }
        if weg
            .windows(2)
            .any(|paar| strecke_trifft(kasten, paar[0], paar[1]))
        {
            anzahl += 2;
        }
    }
    anzahl
}

fn abstand(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linie() -> Diagramm {
        let mut diagramm = Diagramm::leer();
        diagramm.knoten = vec![
            crate::modell::Knoten::neu("a", "A"),
            crate::modell::Knoten::neu("b", "B"),
        ];
        diagramm.kanten = vec![crate::modell::Kante {
            text: Some("weiter".to_string()),
            ..crate::modell::Kante::neu("a", "b")
        }];
        diagramm
    }

    #[test]
    fn raster_kennt_achsen() {
        let achsen = Achsen { rechts: false };
        assert_eq!(achsen.punkt(1.0, 2.0), Punkt::neu(2.0, 1.0));
        let achsen = Achsen { rechts: true };
        assert_eq!(achsen.punkt(1.0, 2.0), Punkt::neu(1.0, 2.0));
    }

    #[test]
    fn strecken_treffen_kasten_nur_im_inneren() {
        let kasten = Kasten::neu(10.0, 10.0, 20.0, 20.0);
        // Querlauf mitten durch
        assert!(strecke_trifft(kasten, (20.0, 0.0), (20.0, 100.0)));
        // Flusslauf mitten durch
        assert!(strecke_trifft(kasten, (0.0, 20.0), (100.0, 20.0)));
        // Am Rand entlang ist kein Treffer
        assert!(!strecke_trifft(kasten, (10.0, 0.0), (10.0, 100.0)));
        // Daneben
        assert!(!strecke_trifft(kasten, (20.0, 40.0), (20.0, 100.0)));
        // Nur berühren
        assert!(!strecke_trifft(kasten, (20.0, 30.0), (20.0, 100.0)));
    }

    #[test]
    fn port_verteilt_die_kanten() {
        let kasten = Kasten::neu(0.0, 0.0, 40.0, 60.0);
        assert!((port(kasten, 0, 1) - 30.0).abs() < 0.01);
        assert!((port(kasten, 0, 2) - 20.0).abs() < 0.01);
        assert!((port(kasten, 1, 2) - 40.0).abs() < 0.01);
    }

    #[test]
    fn aufraeumen_entfernt_ueberfluessige_punkte() {
        let punkte = aufraeumen(vec![
            (0.0, 0.0),
            (0.0, 10.0),
            (0.0, 20.0),
            (5.0, 20.0),
            (5.0, 20.0),
        ]);
        assert_eq!(punkte, vec![(0.0, 0.0), (0.0, 20.0), (5.0, 20.0)]);
    }

    #[test]
    fn einfache_linie_hat_einen_weg_und_eine_beschriftung() {
        let diagramm = linie();
        let zeichnung = layout(&diagramm).expect("Layout");
        assert_eq!(zeichnung.kanten.len(), 1);
        let kante = &zeichnung.kanten[0];
        assert!(kante.punkte.len() >= 2);
        let beschriftung = kante.beschriftung.as_ref().expect("Beschriftung");
        assert_eq!(beschriftung.text, "weiter");
        assert!(!beschriftung.zeilen.is_empty());
        assert!(beschriftung.breite > 0.0);
    }

    #[test]
    fn eine_kante_startet_am_knotenrand() {
        let diagramm = linie();
        let zeichnung = layout(&diagramm).expect("Layout");
        let a = &zeichnung.knoten[0];
        let erster = zeichnung.kanten[0].punkte[0];
        assert!((erster.y - (a.y + a.hoehe)).abs() < 0.01);
        let letzter = *zeichnung.kanten[0].punkte.last().expect("Ende");
        let b = &zeichnung.knoten[1];
        assert!((letzter.y - b.y).abs() < 0.01);
    }

    #[test]
    fn raute_und_ellipse_werden_an_der_form_getroffen() {
        let kasten = Kasten::neu(0.0, 0.0, 40.0, 100.0);
        // Rechteck: die Kante
        assert!((randpunkt(kasten, Form::Rechteck, 80.0, true) - 40.0).abs() < 0.01);
        // Raute: in der Mitte die Spitze, am Rand die Ecke
        assert!((randpunkt(kasten, Form::Raute, 50.0, true) - 40.0).abs() < 0.01);
        assert!((randpunkt(kasten, Form::Raute, 100.0, true) - 20.0).abs() < 0.01);
        // Ellipse liegt zwischen beidem
        let ellipse = randpunkt(kasten, Form::Ellipse, 75.0, true);
        assert!(ellipse > 20.0 && ellipse < 40.0);
    }
}
