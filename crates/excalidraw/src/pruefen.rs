// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Prüfung einer `.excalidraw`-Datei gegen die Quellen des Formats.
//!
//! [`pruefen`] sieht sich das **rohe** JSON an (kein `Default`, kein
//! `restore.ts`): sie verlangt die Pflichtfelder wirklich und prüft, dass
//! Bindungen auf beiden Seiten stehen. Genau das ist die Abnahme für
//! `excalidraw_erstellen`, `excalidraw_aus_quelltext` und die Tests.
//!
//! Warum so streng: Excalidraw lädt auch halbe Dateien, aber ein Agent, der
//! Felder vergisst, bekommt sonst ein Bild, das beim Öffnen anders aussieht als
//! erwartet. Die Meldungen nennen den Ort (`elements[2] (id „a“): …`), damit ein
//! Aufruf reicht, um es zu berichtigen.

use serde_json::{Map, Value};
use std::collections::BTreeMap;

use crate::datei::{Datei, ART, VERSION};
use crate::fehler::Fehler;

/// Die Pflichtfelder jedes Elements, in der Reihenfolge der Quellen
/// (`newElement.ts` und `restore.ts`).
pub const PFLICHTFELDER: &[&str] = &[
    "id",
    "type",
    "x",
    "y",
    "width",
    "height",
    "angle",
    "strokeColor",
    "backgroundColor",
    "fillStyle",
    "strokeWidth",
    "strokeStyle",
    "roughness",
    "opacity",
    "seed",
    "version",
    "versionNonce",
    "index",
    "isDeleted",
    "groupIds",
    "frameId",
    "boundElements",
    "updated",
    "link",
    "locked",
];

/// Die Pflichtteile der Datei selbst.
pub const DATEIFELDER: &[&str] = &["type", "version", "source", "elements", "appState", "files"];

/// Die erlaubten Schemata eines `link`.
pub const LINK_SCHEMATA: &[&str] = &["https://", "http://", "mailto:"];

/// Prüft rohes JSON gegen die Regeln des Formats.
///
/// Gesammelt werden **alle** Verstöße, nicht nur der erste.
pub fn pruefen(roh: &Value) -> Result<(), Vec<String>> {
    let mut meldungen = Vec::new();
    let Some(objekt) = roh.as_object() else {
        return Err(vec!["Die Datei ist kein JSON-Objekt.".to_string()]);
    };

    for feld in DATEIFELDER {
        if objekt.get(*feld).is_none() {
            meldungen.push(format!("„{feld}“ fehlt"));
        }
    }

    match objekt.get("type").and_then(Value::as_str) {
        Some(ART) => {}
        Some(anderes) => meldungen.push(format!("„type“ ist „{anderes}“, erwartet wird „{ART}“")),
        None if objekt.get("type").is_some() => meldungen.push("„type“ ist kein Text".to_string()),
        None => {}
    }
    match objekt.get("version") {
        Some(Value::Number(zahl)) if zahl.as_u64() == Some(u64::from(VERSION)) => {}
        Some(anderes) => {
            meldungen.push(format!("„version“ ist {anderes}, erwartet wird {VERSION}"))
        }
        None => {}
    }
    match objekt.get("source") {
        Some(Value::String(text)) if text.trim().is_empty() => {
            meldungen.push("„source“ ist leer".to_string())
        }
        Some(Value::String(_)) | None => {}
        Some(_) => meldungen.push("„source“ ist kein Text".to_string()),
    }
    for feld in ["appState", "files"] {
        if let Some(wert) = objekt.get(feld) {
            if !wert.is_object() {
                meldungen.push(format!("„{feld}“ ist kein Objekt"));
            }
        }
    }

    let elemente = match objekt.get("elements") {
        Some(Value::Array(elemente)) => elemente,
        Some(_) => {
            meldungen.push("„elements“ ist keine Liste".to_string());
            &[][..]
        }
        None => &[][..],
    };

    let mut ids: BTreeMap<String, usize> = BTreeMap::new();
    for (i, element) in elemente.iter().enumerate() {
        if let Some(id) = element_pruefen(element, i, &mut meldungen) {
            if let Some(vorher) = ids.insert(id.clone(), i) {
                meldungen.push(format!(
                    "elements[{i}]: Id „{id}“ kommt schon in elements[{vorher}] vor"
                ));
            }
        }
    }
    bindungen_pruefen(elemente, &mut meldungen);

    if meldungen.is_empty() {
        Ok(())
    } else {
        Err(meldungen)
    }
}

/// Prüft die Pflichtfelder und Werte eines Elements; gibt seine Id zurück.
fn element_pruefen(element: &Value, i: usize, meldungen: &mut Vec<String>) -> Option<String> {
    let Some(objekt) = element.as_object() else {
        meldungen.push(format!("elements[{i}]: ist kein Objekt"));
        return None;
    };
    let id = objekt
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let ort = if id.is_empty() {
        format!("elements[{i}]")
    } else {
        format!("elements[{i}] (id „{id}“)")
    };

    for feld in PFLICHTFELDER {
        if !objekt.contains_key(*feld) {
            meldungen.push(format!("{ort}: Pflichtfeld „{feld}“ fehlt"));
        }
    }

    if id.trim().is_empty() {
        meldungen.push(format!("{ort}: „id“ ist leer"));
    } else if id.trim() != id {
        meldungen.push(format!("{ort}: „id“ hat Leerzeichen am Rand"));
    }
    match objekt.get("type") {
        Some(Value::String(art)) if art.trim().is_empty() => {
            meldungen.push(format!("{ort}: „type“ ist leer"))
        }
        Some(Value::String(_)) | None => {}
        Some(_) => meldungen.push(format!("{ort}: „type“ ist kein Text")),
    }
    for feld in ["x", "y", "angle", "strokeWidth", "roughness", "opacity"] {
        if let Some(wert) = objekt.get(feld) {
            if !wert.is_number() {
                meldungen.push(format!("{ort}: „{feld}“ ist keine Zahl"));
            }
        }
    }
    for feld in ["width", "height"] {
        match objekt.get(feld) {
            Some(wert) if wert.as_f64().is_some_and(|zahl| zahl < 0.0) => {
                meldungen.push(format!("{ort}: „{feld}“ ist negativ"))
            }
            Some(wert) if !wert.is_number() => {
                meldungen.push(format!("{ort}: „{feld}“ ist keine Zahl"))
            }
            _ => {}
        }
    }
    if let Some(wert) = objekt.get("opacity") {
        if let Some(zahl) = wert.as_f64() {
            if !(0.0..=100.0).contains(&zahl) {
                meldungen.push(format!(
                    "{ort}: „opacity“ ist {zahl}, erlaubt sind 0 bis 100"
                ));
            }
        }
    }
    for feld in ["seed", "version", "versionNonce", "updated"] {
        if let Some(wert) = objekt.get(feld) {
            if !wert.is_number() {
                meldungen.push(format!("{ort}: „{feld}“ ist keine Zahl"));
            }
        }
    }
    for feld in ["strokeColor", "backgroundColor", "fillStyle", "strokeStyle"] {
        match objekt.get(feld) {
            Some(Value::String(_)) | None => {}
            Some(_) => meldungen.push(format!("{ort}: „{feld}“ ist kein Text")),
        }
    }
    for feld in ["isDeleted", "locked"] {
        match objekt.get(feld) {
            Some(Value::Bool(_)) | None => {}
            Some(_) => meldungen.push(format!("{ort}: „{feld}“ ist kein Wahrheitswert")),
        }
    }
    match objekt.get("index") {
        Some(Value::Null) | Some(Value::String(_)) | None => {}
        Some(_) => meldungen.push(format!("{ort}: „index“ ist weder null noch Text")),
    }
    match objekt.get("groupIds") {
        Some(Value::Array(gruppen)) if gruppen.iter().all(Value::is_string) => {}
        Some(Value::Array(_)) => meldungen.push(format!(
            "{ort}: „groupIds“ enthält etwas, das kein Text ist"
        )),
        Some(_) => meldungen.push(format!("{ort}: „groupIds“ ist keine Liste")),
        None => {}
    }
    match objekt.get("frameId") {
        Some(Value::Null) | Some(Value::String(_)) | None => {}
        Some(_) => meldungen.push(format!("{ort}: „frameId“ ist weder null noch Text")),
    }
    match objekt.get("boundElements") {
        Some(Value::Null) | None => {}
        Some(Value::Array(bindungen)) => {
            for bindung in bindungen {
                if bindung.get("id").and_then(Value::as_str).is_none()
                    || bindung.get("type").and_then(Value::as_str).is_none()
                {
                    meldungen.push(format!(
                        "{ort}: ein Eintrag in „boundElements“ hat kein „type“ und „id“"
                    ));
                }
            }
        }
        Some(_) => meldungen.push(format!("{ort}: „boundElements“ ist keine Liste")),
    }
    if let Some(Value::String(link)) = objekt.get("link") {
        if !LINK_SCHEMATA.iter().any(|schema| link.starts_with(schema)) {
            meldungen.push(format!(
                "{ort}: „link“ ist „{link}“ – erlaubt sind nur https, http und mailto"
            ));
        }
    }
    match objekt.get("link") {
        Some(Value::Null) | Some(Value::String(_)) | None => {}
        Some(_) => meldungen.push(format!("{ort}: „link“ ist weder null noch Text")),
    }

    if id.trim().is_empty() {
        None
    } else {
        Some(id)
    }
}

/// Prüft, dass jede Bindung auf beiden Seiten steht und nirgends ins Leere zeigt.
fn bindungen_pruefen(elemente: &[Value], meldungen: &mut Vec<String>) {
    let mut karte: BTreeMap<&str, &Map<String, Value>> = BTreeMap::new();
    for element in elemente {
        let Some(objekt) = element.as_object() else {
            continue;
        };
        if let Some(id) = objekt.get("id").and_then(Value::as_str) {
            karte.insert(id, objekt);
        }
    }

    for (i, element) in elemente.iter().enumerate() {
        let Some(objekt) = element.as_object() else {
            continue;
        };
        let id = objekt.get("id").and_then(Value::as_str).unwrap_or("");
        let ort = if id.is_empty() {
            format!("elements[{i}]")
        } else {
            format!("elements[{i}] (id „{id}“)")
        };

        if let Some(container) = objekt.get("containerId").and_then(Value::as_str) {
            match karte.get(container) {
                None => meldungen.push(format!(
                    "{ort}: „containerId“ nennt „{container}“, das es nicht gibt"
                )),
                Some(ziel) => {
                    if !hat_bindung(ziel, "text", id) {
                        meldungen.push(format!(
                            "{ort}: „containerId“ ist „{container}“, aber dessen „boundElements“ nennt diesen Text nicht"
                        ));
                    }
                }
            }
        }

        if let Some(Value::Array(bindungen)) = objekt.get("boundElements") {
            for bindung in bindungen {
                let art = bindung.get("type").and_then(Value::as_str).unwrap_or("");
                let ziel_id = bindung.get("id").and_then(Value::as_str).unwrap_or("");
                let Some(ziel) = karte.get(ziel_id) else {
                    meldungen.push(format!(
                        "{ort}: „boundElements“ nennt „{ziel_id}“, das es nicht gibt"
                    ));
                    continue;
                };
                match art {
                    "text" => {
                        if ziel.get("containerId").and_then(Value::as_str) != Some(id) {
                            meldungen.push(format!(
                                "{ort}: „boundElements“ nennt den Text „{ziel_id}“, aber dessen „containerId“ ist nicht „{id}“"
                            ));
                        }
                    }
                    "arrow" => {
                        let haengt = bindung_ziel(ziel.get("startBinding")) == Some(id)
                            || bindung_ziel(ziel.get("endBinding")) == Some(id);
                        if !haengt {
                            meldungen.push(format!(
                                "{ort}: „boundElements“ nennt den Pfeil „{ziel_id}“, aber der hängt nicht an „{id}“"
                            ));
                        }
                    }
                    sonst => meldungen.push(format!(
                        "{ort}: „boundElements“ nennt die unbekannte Art „{sonst}“"
                    )),
                }
            }
        }

        for feld in ["startBinding", "endBinding"] {
            let Some(ziel_id) = bindung_ziel(objekt.get(feld)) else {
                continue;
            };
            match karte.get(ziel_id) {
                None => meldungen.push(format!(
                    "{ort}: „{feld}“ nennt „{ziel_id}“, das es nicht gibt"
                )),
                Some(ziel) => {
                    if !hat_bindung(ziel, "arrow", id) {
                        meldungen.push(format!(
                            "{ort}: „{feld}“ ist „{ziel_id}“, aber dessen „boundElements“ nennt diesen Pfeil nicht"
                        ));
                    }
                }
            }
        }

        if let Some(rahmen) = objekt.get("frameId").and_then(Value::as_str) {
            match karte.get(rahmen) {
                None => meldungen.push(format!(
                    "{ort}: „frameId“ nennt „{rahmen}“, das es nicht gibt"
                )),
                Some(ziel) => {
                    if ziel.get("type").and_then(Value::as_str) != Some("frame") {
                        meldungen.push(format!(
                            "{ort}: „frameId“ nennt „{rahmen}“, das kein „frame“ ist"
                        ));
                    }
                }
            }
        }
    }
}

fn hat_bindung(element: &Map<String, Value>, art: &str, id: &str) -> bool {
    element
        .get("boundElements")
        .and_then(Value::as_array)
        .is_some_and(|bindungen| {
            bindungen.iter().any(|bindung| {
                bindung.get("type").and_then(Value::as_str) == Some(art)
                    && bindung.get("id").and_then(Value::as_str) == Some(id)
            })
        })
}

fn bindung_ziel(wert: Option<&Value>) -> Option<&str> {
    wert?.get("elementId")?.as_str()
}

/// Alle Meldungen als ein Text für den Agenten.
pub fn meldungstext(meldungen: &[String]) -> String {
    let mut text = String::from("Die Datei ist keine gültige `.excalidraw`-Datei:");
    for meldung in meldungen {
        text.push_str("\n– ");
        text.push_str(meldung);
    }
    text
}

/// Prüft einen Quelltext streng und liest ihn dann großzügig.
///
/// Der Weg für `excalidraw_aus_quelltext`: was der Agent als „fertig“ abgibt,
/// muss die Pflichtfelder wirklich enthalten.
pub fn pruefen_und_lesen(text: &str) -> Result<Datei, Fehler> {
    let roh: Value =
        serde_json::from_str(text).map_err(|fehler| Fehler::Json(fehler.to_string()))?;
    pruefen(&roh).map_err(|meldungen| Fehler::Ungueltig(meldungstext(&meldungen)))?;
    Datei::aus_text(text)
}

/// Die Ids, die doppelt vorkommen (für den Test der Prüfung selbst).
#[cfg(test)]
fn doppelte_ids(elemente: &[Value]) -> std::collections::HashSet<String> {
    use std::collections::HashSet;
    let mut gesehen = HashSet::new();
    let mut doppelt = HashSet::new();
    for element in elemente {
        if let Some(id) = element.get("id").and_then(Value::as_str) {
            if !gesehen.insert(id.to_string()) {
                doppelt.insert(id.to_string());
            }
        }
    }
    doppelt
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datei::Element;
    use serde_json::json;

    /// Eine kleine, gültige Datei mit Form, Text und Pfeil.
    fn gueltig() -> Value {
        let mut form = Element::neu("rectangle", 10.0, 10.0, 100.0, 50.0);
        form.id = "form".to_string();
        form.bound_elements = Some(vec![crate::datei::Binding::text("text")]);
        let mut text = Element::neu("text", 20.0, 20.0, 40.0, 20.0);
        text.id = "text".to_string();
        text.text = Some("Hallo".to_string());
        text.original_text = Some("Hallo".to_string());
        text.container_id = Some("form".to_string());
        let mut pfeil = Element::neu("arrow", 110.0, 30.0, 40.0, 0.0);
        pfeil.id = "pfeil".to_string();
        pfeil.punkte_setzen(&[(110.0, 30.0), (150.0, 30.0)]);
        pfeil.bindung_setzen(crate::datei::Ende::Start, Some("form"));
        let mut datei = Datei::neu();
        datei.elements = vec![form, text, pfeil];
        if let Some(element) = datei.element_mut("form") {
            element.bindung_eintragen(crate::datei::Binding::pfeil("pfeil"));
        }
        serde_json::to_value(&datei).unwrap()
    }

    #[test]
    fn eine_gueltige_datei_kommt_durch() {
        pruefen(&gueltig()).expect("gültig");
    }

    #[test]
    fn fehlende_pflichtfelder_werden_genannt() {
        let mut roh = gueltig();
        roh["elements"][0]
            .as_object_mut()
            .unwrap()
            .remove("versionNonce");
        let meldungen = pruefen(&roh).expect_err("Pflichtfeld fehlt");
        assert!(
            meldungen
                .iter()
                .any(|m| m.contains("elements[0]") && m.contains("versionNonce")),
            "{meldungen:?}"
        );
    }

    #[test]
    fn ein_einseitiger_text_wird_gemeldet() {
        let mut roh = gueltig();
        roh["elements"][0]["boundElements"] = json!([]);
        let meldungen = pruefen(&roh).expect_err("Gegenseite fehlt");
        assert!(
            meldungen
                .iter()
                .any(|m| m.contains("boundElements") && m.contains("containerId")),
            "{meldungen:?}"
        );
    }

    #[test]
    fn ein_einseitiger_pfeil_wird_gemeldet() {
        let mut roh = gueltig();
        roh["elements"][0]["boundElements"] = json!([]);
        let meldungen = pruefen(&roh).expect_err("Gegenseite fehlt");
        assert!(
            meldungen
                .iter()
                .any(|m| m.contains("startBinding") || m.contains("boundElements")),
            "{meldungen:?}"
        );
    }

    #[test]
    fn eine_bindung_ins_leere_wird_gemeldet() {
        let mut roh = gueltig();
        roh["elements"][2]["startBinding"] =
            json!({ "elementId": "gibtsnicht", "focus": 0, "gap": 4 });
        let meldungen = pruefen(&roh).expect_err("Ziel fehlt");
        assert!(
            meldungen
                .iter()
                .any(|m| m.contains("gibtsnicht") && m.contains("nicht gibt")),
            "{meldungen:?}"
        );
    }

    #[test]
    fn kopf_und_link_werden_geprueft() {
        let mut roh = gueltig();
        roh["type"] = json!("drawio");
        roh["version"] = json!(1);
        roh["elements"][0]["link"] = json!("javascript:alert(1)");
        let meldungen = pruefen(&roh).expect_err("mehrere Verstöße");
        assert!(
            meldungen.iter().any(|m| m.contains("type")),
            "{meldungen:?}"
        );
        assert!(
            meldungen.iter().any(|m| m.contains("version")),
            "{meldungen:?}"
        );
        assert!(
            meldungen.iter().any(|m| m.contains("mailto")),
            "{meldungen:?}"
        );
    }

    #[test]
    fn doppelte_ids_werden_gemeldet() {
        let mut roh = gueltig();
        roh["elements"][1]["id"] = json!("form");
        let meldungen = pruefen(&roh).expect_err("doppelt");
        assert!(
            meldungen.iter().any(|m| m.contains("schon in elements[0]")),
            "{meldungen:?}"
        );
        assert_eq!(doppelte_ids(roh["elements"].as_array().unwrap()).len(), 1);
    }

    #[test]
    fn eine_ausgabe_unserer_datei_ist_gueltig() {
        let datei = Datei::neu();
        let roh = serde_json::to_value(&datei).unwrap();
        pruefen(&roh).expect("auch die leere Datei ist gültig");
    }
}
