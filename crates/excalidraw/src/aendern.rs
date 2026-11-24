// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Ändern, Verbinden, Lösen und Entfernen – mit beiden Seiten der Bindung.
//!
//! Die Regel aus Aufgabe 17: **Text in Formen über `containerId` +
//! `boundElements` (beidseitig), Pfeile über `startBinding`/`endBinding` **und**
//! `boundElements`.** Entfernen löst die Gegenseite mit auf, Verschieben zieht
//! gebundene Texte und Pfeile mit. Jede Änderung erhöht `version` und
//! `versionNonce` des betroffenen Elements.

use quiver_diagramm::modell::{Farbe, Form, KanteArt};

use crate::bauen;
use crate::datei::{Binding, Datei, Element, Ende};
use crate::farben;
use crate::fehler::Fehler;

/// Setzt den Text eines Textes oder den Text in einer Form.
///
/// Ist die Form leer, entsteht ein neuer, beidseitig gebundener Text.
pub fn text_setzen(datei: &mut Datei, id: &str, inhalt: &str) -> Result<String, Fehler> {
    let element = datei
        .element(id)
        .cloned()
        .ok_or_else(|| Fehler::ElementFehlt(id.to_string()))?;

    if element.ist("text") {
        setze_text(datei, id, inhalt)?;
        return Ok(format!("Text von „{id}“ geändert"));
    }

    if let Some(text_id) = element.gebundene("text").first().cloned() {
        setze_text(datei, &text_id, inhalt)?;
        return Ok(format!("Text „{text_id}“ in „{id}“ geändert"));
    }

    // Noch kein Text: einen anlegen und beidseitig binden.
    let farbe = farben::farbe_aus_hex(&element.background_color);
    let mut text = bauen::text_im(inhalt, &element, farbe, "center");
    text.id = datei.freie_id(&format!("{id}-text"));
    let text_id = datei.hinzufuegen(text)?;
    if let Some(ziel) = datei.element_mut(id) {
        ziel.bindung_eintragen(Binding::text(text_id.clone()));
        ziel.beruehren();
    }
    Ok(format!("Text „{text_id}“ in „{id}“ angelegt"))
}

/// Setzt die Füllfarbe eines Elements (bei Text die Schriftfarbe).
pub fn farbe_setzen(datei: &mut Datei, id: &str, farbe: Farbe) -> Result<(), Fehler> {
    let toene = farben::toene(farbe);
    let element = datei
        .element_mut(id)
        .ok_or_else(|| Fehler::ElementFehlt(id.to_string()))?;
    if element.ist("text") || element.ist("arrow") || element.ist("line") {
        element.stroke_color = toene.text.to_string();
    } else {
        element.background_color = toene.fuellung.to_string();
        element.stroke_color = toene.rand.to_string();
    }
    element.beruehren();
    Ok(())
}

/// Setzt die Form eines Elements (Rechteck, Raute, Ellipse).
pub fn form_setzen(datei: &mut Datei, id: &str, form: Form) -> Result<(), Fehler> {
    let element = datei
        .element_mut(id)
        .ok_or_else(|| Fehler::ElementFehlt(id.to_string()))?;
    if !element.ist_form() {
        return Err(Fehler::Ungueltig(format!(
            "„{id}“ ist ein {} – eine Form lässt sich nur an rectangle, diamond und ellipse ändern",
            element.art
        )));
    }
    element.art = farben::art_der_form(form).to_string();
    element.roundness = Some(if element.ist("rectangle") {
        serde_json::json!({ "type": 3 })
    } else {
        serde_json::json!({ "type": 2 })
    });
    element.beruehren();
    Ok(())
}

/// Verschiebt (und optional vergrößert) ein Element.
///
/// Gebundene Texte wandern mit; Pfeile, die daran hängen, bekommen ihren
/// Endpunkt neu auf dem Rand des Elements.
pub fn verschieben(
    datei: &mut Datei,
    id: &str,
    x: Option<f64>,
    y: Option<f64>,
    breite: Option<f64>,
    hoehe: Option<f64>,
) -> Result<(), Fehler> {
    let alt = datei
        .element(id)
        .cloned()
        .ok_or_else(|| Fehler::ElementFehlt(id.to_string()))?;
    let neu_x = x.unwrap_or(alt.x);
    let neu_y = y.unwrap_or(alt.y);
    let neu_breite = breite.unwrap_or(alt.width).max(1.0);
    let neu_hoehe = hoehe.unwrap_or(alt.height).max(1.0);
    let dx = neu_x - alt.x;
    let dy = neu_y - alt.y;
    let skaliert = (neu_breite - alt.width).abs() > f64::EPSILON
        || (neu_hoehe - alt.height).abs() > f64::EPSILON;

    let element = datei
        .element_mut(id)
        .ok_or_else(|| Fehler::ElementFehlt(id.to_string()))?;
    element.x = neu_x;
    element.y = neu_y;
    element.width = neu_breite;
    element.height = neu_hoehe;
    element.beruehren();

    // Texte wandern mit; nach dem Skalieren werden sie neu ausgerichtet.
    for text_id in alt.gebundene("text") {
        if skaliert {
            ausrichten(datei, &text_id);
        } else if let Some(text) = datei.element_mut(&text_id) {
            text.x += dx;
            text.y += dy;
            text.beruehren();
        }
    }

    // Pfeile: der Endpunkt am verschobenen Element wird neu gesetzt.
    let neu = datei
        .element(id)
        .cloned()
        .ok_or_else(|| Fehler::ElementFehlt(id.to_string()))?;
    for pfeil_id in alt.gebundene("arrow") {
        let Some(pfeil) = datei.element(&pfeil_id).cloned() else {
            continue;
        };
        let mut punkte = pfeil.punkte();
        if punkte.len() < 2 {
            continue;
        }
        let mut geaendert = false;
        // Hängt der Pfeil an beiden Enden, läuft er danach gerade: die alten
        // Knicke gehörten zur alten Lage.
        let anderes = match (pfeil.start(), pfeil.schluss()) {
            (Some(start), Some(schluss)) if start == id && schluss != id => Some(schluss),
            (Some(start), Some(schluss)) if schluss == id && start != id => Some(start),
            _ => None,
        }
        .and_then(|anderes| datei.element(anderes))
        .cloned();
        if let Some(anderes) = anderes {
            let hier = bauen::randpunkt(&neu, anderes.mitte());
            let dort = bauen::randpunkt(&anderes, neu.mitte());
            punkte = if pfeil.start() == Some(id) {
                vec![hier, dort]
            } else {
                vec![dort, hier]
            };
            geaendert = true;
        } else if pfeil.start() == Some(id) {
            punkte[0] = bauen::randpunkt(&neu, punkte[1]);
            geaendert = true;
        }
        if !geaendert && pfeil.schluss() == Some(id) {
            let letzter = punkte.len() - 1;
            punkte[letzter] = bauen::randpunkt(&neu, punkte[letzter - 1]);
            geaendert = true;
        }
        if geaendert {
            if let Some(ziel) = datei.element_mut(&pfeil_id) {
                ziel.punkte_setzen(&punkte);
                ziel.beruehren();
            }
            // Die Beschriftung bleibt auf ihrem Pfeil.
            for text_id in pfeil.gebundene("text") {
                ausrichten(datei, &text_id);
            }
        }
    }
    Ok(())
}

/// Verbindet zwei Elemente mit einem Pfeil oder einer Linie.
///
/// Gibt es die Verbindung schon, wird nur ihre Art geändert.
pub fn verbinden(
    datei: &mut Datei,
    von: &str,
    nach: &str,
    art: KanteArt,
    id: Option<&str>,
) -> Result<String, Fehler> {
    if von == nach {
        return Err(Fehler::Ungueltig(format!(
            "„{von}“ kann nicht mit sich selbst verbunden werden"
        )));
    }
    let von_element = datei
        .element(von)
        .cloned()
        .ok_or_else(|| Fehler::ElementFehlt(von.to_string()))?;
    let nach_element = datei
        .element(nach)
        .cloned()
        .ok_or_else(|| Fehler::ElementFehlt(nach.to_string()))?;

    let vorhanden = datei
        .elements
        .iter()
        .find(|element| {
            element.ist_linie()
                && element.start() == Some(von)
                && element.schluss() == Some(nach)
                && !element.is_deleted
        })
        .map(|element| element.id.clone());
    if let Some(pfeil_id) = vorhanden {
        if let Some(pfeil) = datei.element_mut(&pfeil_id) {
            bauen::stil_setzen(pfeil, art);
            pfeil.beruehren();
        }
        return Ok(format!(
            "Verbindung „{pfeil_id}“ zwischen „{von}“ und „{nach}“ aktualisiert"
        ));
    }

    let mut pfeil = bauen::pfeil_zwischen(&von_element, &nach_element, art);
    pfeil.id = id
        .map(str::to_string)
        .unwrap_or_else(|| datei.freie_id("pfeil"));
    let pfeil_id = datei.hinzufuegen(pfeil)?;
    for ende in [von, nach] {
        if let Some(ziel) = datei.element_mut(ende) {
            ziel.bindung_eintragen(Binding::pfeil(pfeil_id.clone()));
            ziel.beruehren();
        }
    }
    Ok(format!(
        "Verbindung „{pfeil_id}“ von „{von}“ nach „{nach}“ angelegt"
    ))
}

/// Entfernt die Verbindung zwischen zwei Elementen (den Pfeil selbst).
pub fn loesen(datei: &mut Datei, von: &str, nach: &str) -> Result<usize, Fehler> {
    for id in [von, nach] {
        if datei.element(id).is_none() {
            return Err(Fehler::ElementFehlt(id.to_string()));
        }
    }
    let treffer: Vec<String> = datei
        .elements
        .iter()
        .filter(|element| {
            element.ist_linie()
                && ((element.start() == Some(von) && element.schluss() == Some(nach))
                    || (element.start() == Some(nach) && element.schluss() == Some(von)))
        })
        .map(|element| element.id.clone())
        .collect();
    for id in &treffer {
        entfernen(datei, id)?;
    }
    Ok(treffer.len())
}

/// Entfernt ein Element und löst dabei jede Bindung in beide Richtungen auf.
///
/// Gibt zurück, wie viele Gegenstellen aufgelöst wurden.
pub fn entfernen(datei: &mut Datei, id: &str) -> Result<usize, Fehler> {
    let position = datei
        .position(id)
        .ok_or_else(|| Fehler::ElementFehlt(id.to_string()))?;
    let element: Element = datei.elements[position].clone();
    let mut geloest = 0;

    // Was an diesem Element hing, wird frei.
    let anhaengsel: Vec<Binding> = element.bound_elements.iter().flatten().cloned().collect();
    for bindung in anhaengsel {
        match bindung.art.as_str() {
            "text" => {
                if let Some(text) = datei.element_mut(&bindung.id) {
                    if text.container() == Some(id) {
                        text.container_id = None;
                        text.beruehren();
                        geloest += 1;
                    }
                }
            }
            "arrow" => {
                if let Some(pfeil) = datei.element_mut(&bindung.id) {
                    let mut getroffen = false;
                    if pfeil.start() == Some(id) {
                        pfeil.bindung_setzen(Ende::Start, None);
                        getroffen = true;
                    }
                    if pfeil.schluss() == Some(id) {
                        pfeil.bindung_setzen(Ende::Schluss, None);
                        getroffen = true;
                    }
                    if getroffen {
                        pfeil.beruehren();
                        geloest += 1;
                    }
                }
            }
            _ => {}
        }
    }

    // Die Gegenseite: der Behälter dieses Textes …
    let container = element.container().map(str::to_string);
    if let Some(container) = container {
        if let Some(ziel) = datei.element_mut(&container) {
            if ziel.bindung_entfernen("text", id) {
                ziel.beruehren();
                geloest += 1;
            }
        }
    }
    // … und die Enden dieses Pfeils.
    let enden = [
        element.start().map(str::to_string),
        element.schluss().map(str::to_string),
    ];
    for ende in enden.into_iter().flatten() {
        if let Some(ziel) = datei.element_mut(&ende) {
            if ziel.bindung_entfernen("arrow", id) {
                ziel.beruehren();
                geloest += 1;
            }
        }
    }

    // Ein Rahmen lässt seine Mitglieder frei.
    if element.ist("frame") {
        let mitglieder: Vec<String> = datei
            .elements
            .iter()
            .filter(|eintrag| eintrag.frame_id.as_deref() == Some(id))
            .map(|eintrag| eintrag.id.clone())
            .collect();
        for mitglied in mitglieder {
            if let Some(ziel) = datei.element_mut(&mitglied) {
                ziel.frame_id = None;
                ziel.beruehren();
            }
        }
    }

    datei.elements.remove(position);
    Ok(geloest)
}

/// Setzt einen Text mittig in seinen Behälter (ohne ihn neu umzubrechen).
pub fn ausrichten(datei: &mut Datei, text_id: &str) {
    let Some(text) = datei.element(text_id) else {
        return;
    };
    let (breite, hoehe) = (text.width, text.height);
    let Some(container) = text.container().and_then(|id| datei.element(id)) else {
        return;
    };
    let mitte = container.mitte();
    let (x, y) = (mitte.0 - breite / 2.0, mitte.1 - hoehe / 2.0);
    if let Some(text) = datei.element_mut(text_id) {
        text.x = x;
        text.y = y;
        text.beruehren();
    }
}

/// Schreibt `inhalt` in einen vorhandenen Text und misst ihn neu.
fn setze_text(datei: &mut Datei, text_id: &str, inhalt: &str) -> Result<(), Fehler> {
    let text = datei
        .element(text_id)
        .cloned()
        .ok_or_else(|| Fehler::ElementFehlt(text_id.to_string()))?;
    let groesse = text.font_size.unwrap_or(bauen::TEXTGROESSE);
    let behaelter = text.container().and_then(|id| datei.element(id)).cloned();
    let breite = match &behaelter {
        Some(behaelter) => (behaelter.width - 2.0 * bauen::TEXT_INNEN).max(24.0),
        None => text.width.max(24.0),
    };

    let zeilen = farben::umbrechen(inhalt, groesse, breite);
    let gemessen = zeilen
        .iter()
        .map(|zeile| farben::textbreite(zeile, groesse))
        .fold(0.0_f64, f64::max);
    let hoehe = zeilen.len() as f64 * groesse * bauen::ZEILENFAKTOR;

    let element = datei
        .element_mut(text_id)
        .ok_or_else(|| Fehler::ElementFehlt(text_id.to_string()))?;
    element.text = Some(zeilen.join("\n"));
    element.original_text = Some(inhalt.to_string());
    element.width = gemessen.max(1.0);
    element.height = hoehe.max(1.0);
    if let Some(behaelter) = &behaelter {
        let mitte = behaelter.mitte();
        element.x = mitte.0 - element.width / 2.0;
        element.y = mitte.1 - element.height / 2.0;
    }
    element.beruehren();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bauen::Vorlage;
    use crate::pruefen;
    use quiver_diagramm::modell::Diagramm;

    /// Zwei Formen, verbunden mit einem Pfeil.
    fn zwei_formen() -> Datei {
        let mut datei = Datei::neu();
        bauen::hinzufuegen(
            &mut datei,
            &Vorlage {
                art: "rectangle".to_string(),
                id: Some("a".to_string()),
                text: Some("Anfang".to_string()),
                x: Some(0.0),
                y: Some(0.0),
                breite: Some(100.0),
                hoehe: Some(50.0),
                ..Vorlage::default()
            },
        )
        .unwrap();
        bauen::hinzufuegen(
            &mut datei,
            &Vorlage {
                art: "ellipse".to_string(),
                id: Some("b".to_string()),
                x: Some(300.0),
                y: Some(0.0),
                breite: Some(100.0),
                hoehe: Some(50.0),
                ..Vorlage::default()
            },
        )
        .unwrap();
        verbinden(&mut datei, "a", "b", KanteArt::Pfeil, None).unwrap();
        datei
    }

    fn gueltig(datei: &Datei) {
        let roh = serde_json::to_value(datei).unwrap();
        pruefen::pruefen(&roh).expect("Datei bleibt gültig");
    }

    #[test]
    fn ein_text_in_einer_form_haengt_beidseitig() {
        let mut datei = zwei_formen();
        let meldung = text_setzen(&mut datei, "a", "Neuer Text").unwrap();
        assert!(meldung.contains("geändert"), "{meldung}");
        let text_id = datei.element("a").unwrap().gebundene("text")[0].clone();
        // Die Form ist 100 px breit: der Text wird darin umbrochen.
        assert_eq!(
            datei.element(&text_id).unwrap().original_text.as_deref(),
            Some("Neuer Text")
        );
        assert!(datei
            .element(&text_id)
            .unwrap()
            .text
            .as_deref()
            .unwrap()
            .contains("Neuer"));
        assert_eq!(datei.element(&text_id).unwrap().container(), Some("a"));
        gueltig(&datei);
    }

    #[test]
    fn eine_leere_form_bekommt_einen_text() {
        let mut datei = zwei_formen();
        let meldung = text_setzen(&mut datei, "b", "Neu").unwrap();
        assert!(meldung.contains("angelegt"), "{meldung}");
        assert_eq!(datei.element("b").unwrap().gebundene("text").len(), 1);
        gueltig(&datei);
    }

    #[test]
    fn ein_freier_text_wird_umgebrochen() {
        let mut datei = Datei::neu();
        datei
            .hinzufuegen(bauen::freier_text("kurz", 0.0, 0.0, 100.0, None))
            .unwrap();
        let id = datei.elements[0].id.clone();
        text_setzen(&mut datei, &id, "ein deutlich längerer Text als vorher").unwrap();
        let text = datei.element(&id).unwrap();
        assert!(text.text.as_deref().unwrap().contains('\n'));
        assert!(text.height > 20.0);
        gueltig(&datei);
    }

    #[test]
    fn verschieben_zieht_text_und_pfeil_mit() {
        let mut datei = zwei_formen();
        let text_vorher = datei.element("a").unwrap().gebundene("text")[0].clone();
        let y_vorher = datei.element(&text_vorher).unwrap().y;
        let pfeil_vorher = datei.element("a").unwrap().gebundene("arrow")[0].clone();
        let start_vorher = datei.element(&pfeil_vorher).unwrap().punkte()[0];

        verschieben(&mut datei, "a", Some(0.0), Some(200.0), None, None).unwrap();

        let a = datei.element("a").unwrap();
        assert_eq!(a.y, 200.0);
        // Der Text ist mitgewandert.
        let text = datei.element(&text_vorher).unwrap();
        assert!((text.y - (y_vorher + 200.0)).abs() < 0.001);
        // Der Pfeil beginnt jetzt am neuen Rand.
        let start_nachher = datei.element(&pfeil_vorher).unwrap().punkte()[0];
        assert_ne!(start_vorher, start_nachher);
        // Der Anfang liegt jetzt auf der neuen Oberkante von „a“.
        assert!((start_nachher.1 - 200.0).abs() < 0.001, "{start_nachher:?}");
        assert!(
            (0.0..=100.0).contains(&start_nachher.0),
            "{start_nachher:?}"
        );
        gueltig(&datei);
    }

    #[test]
    fn vergroessern_richtet_den_text_neu_aus() {
        let mut datei = zwei_formen();
        let text_id = datei.element("a").unwrap().gebundene("text")[0].clone();
        verschieben(&mut datei, "a", None, None, Some(200.0), Some(100.0)).unwrap();
        let a = datei.element("a").unwrap();
        let text = datei.element(&text_id).unwrap();
        assert!((text.x - (a.x + (a.width - text.width) / 2.0)).abs() < 0.001);
        gueltig(&datei);
    }

    #[test]
    fn entfernen_loest_beide_seiten() {
        let mut datei = zwei_formen();
        let text_id = datei.element("a").unwrap().gebundene("text")[0].clone();
        let pfeil_id = datei.element("a").unwrap().gebundene("arrow")[0].clone();

        let geloest = entfernen(&mut datei, "a").unwrap();
        assert_eq!(geloest, 2, "Text und Pfeil");
        assert!(datei.element("a").is_none());
        // Der Text lebt weiter, aber ohne Behälter.
        let text = datei.element(&text_id).unwrap();
        assert_eq!(text.container(), None);
        // Der Pfeil hat nur noch ein Ende.
        let pfeil = datei.element(&pfeil_id).unwrap();
        assert_eq!(pfeil.start(), None);
        assert_eq!(pfeil.schluss(), Some("b"));
        assert!(!datei.element("b").unwrap().gebundene("arrow").is_empty());
        gueltig(&datei);
    }

    #[test]
    fn entfernen_eines_pfeils_loest_seine_enden() {
        let mut datei = zwei_formen();
        let pfeil_id = datei.element("a").unwrap().gebundene("arrow")[0].clone();
        entfernen(&mut datei, &pfeil_id).unwrap();
        assert!(datei.element("a").unwrap().gebundene("arrow").is_empty());
        assert!(datei.element("b").unwrap().gebundene("arrow").is_empty());
        gueltig(&datei);
    }

    #[test]
    fn verbinden_ist_wiederholbar_und_loesen_entfernt_den_pfeil() {
        let mut datei = zwei_formen();
        let anzahl = datei.elements.len();
        let meldung = verbinden(&mut datei, "a", "b", KanteArt::Gestrichelt, None).unwrap();
        assert!(meldung.contains("aktualisiert"), "{meldung}");
        assert_eq!(datei.elements.len(), anzahl, "kein zweiter Pfeil");

        let geloest = loesen(&mut datei, "b", "a").unwrap();
        assert_eq!(geloest, 1);
        assert!(datei.element("a").unwrap().gebundene("arrow").is_empty());
        gueltig(&datei);
    }

    #[test]
    fn ein_element_mit_sich_selbst_wird_abgelehnt() {
        let mut datei = zwei_formen();
        let fehler = verbinden(&mut datei, "a", "a", KanteArt::Pfeil, None).unwrap_err();
        assert!(fehler.to_string().contains("sich selbst"), "{fehler}");
    }

    #[test]
    fn unbekannte_ids_werden_gemeldet() {
        let mut datei = zwei_formen();
        assert!(matches!(
            text_setzen(&mut datei, "gibtsnicht", "x"),
            Err(Fehler::ElementFehlt(id)) if id == "gibtsnicht"
        ));
        assert!(matches!(
            entfernen(&mut datei, "gibtsnicht"),
            Err(Fehler::ElementFehlt(_))
        ));
    }

    #[test]
    fn farbe_und_form_werden_gesetzt() {
        let mut datei = zwei_formen();
        farbe_setzen(&mut datei, "a", Farbe::Gruen).unwrap();
        let a = datei.element("a").unwrap();
        assert_eq!(a.background_color, farben::toene(Farbe::Gruen).fuellung);
        form_setzen(&mut datei, "a", Form::Raute).unwrap();
        assert_eq!(datei.element("a").unwrap().art, "diamond");
        gueltig(&datei);
        // Der Text in der Form bleibt gebunden.
        assert!(!datei.element("a").unwrap().gebundene("text").is_empty());
    }

    #[test]
    fn ein_rahmen_laesst_seine_mitglieder_frei() {
        let mut datei = Datei::neu();
        let mut rahmen = Element::neu("frame", 0.0, 0.0, 200.0, 200.0);
        rahmen.id = "rahmen".to_string();
        datei.hinzufuegen(rahmen).unwrap();
        let mut form = Element::neu("rectangle", 10.0, 10.0, 50.0, 50.0);
        form.id = "form".to_string();
        form.frame_id = Some("rahmen".to_string());
        datei.hinzufuegen(form).unwrap();

        entfernen(&mut datei, "rahmen").unwrap();
        assert_eq!(datei.element("form").unwrap().frame_id, None);
        gueltig(&datei);
    }

    #[test]
    fn ein_modell_laesst_sich_als_ganzes_aendern() {
        let mut datei = crate::bauen::aus_diagramm(&Diagramm::leer()).unwrap();
        text_setzen(&mut datei, "gibtsnicht", "x").unwrap_err();
    }
}
