// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Ein kleiner HTML-Formular-Leser (Aufgabe 06, Anmelde-Routine).
//!
//! Den Aufbau der ZEP-Anmeldeseiten kennen wir nicht. Statt ihn zu raten, liest
//! dieses Modul, was auf einer Seite steht: Formulare mit `action` und `method`,
//! ihre Eingabefelder (Name, Wert, Art), Auswahllisten und Knöpfe. Die
//! Ablaufsteuerung entscheidet dann, was sie damit tut – und meldet nur **Namen**,
//! nie Werte.
//!
//! Der Leser ist absichtlich kein vollständiger HTML-Parser: keine Entitäten
//! außer den üblichen, keine Namensräume, kein CSS. Er kennt nur, was für ein
//! Formular nötig ist, und überspringt alles andere.

/// Ein Eingabefeld eines Formulars.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feld {
    /// `name` des Feldes; leere Namen werden nicht aufgenommen.
    pub name: String,
    /// Vorbelegter Wert (`value` oder die gewählte Option).
    pub wert: String,
    /// `type` in Kleinschreibung, z. B. `text`, `password`, `hidden`.
    pub art: String,
}

/// Ein Knopf eines Formulars.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Knopf {
    /// `name` des Knopfes, falls gesetzt (`<input type="submit" name="ok">`).
    pub name: Option<String>,
    /// Wert, den ein Klick mitschickt.
    pub wert: String,
    /// Sichtbarer Text (bei `<button>` der Inhalt, sonst der Wert).
    pub text: String,
    /// `type` in Kleinschreibung; `submit`, wenn nichts anderes dasteht.
    pub art: String,
}

/// Ein Formular einer Seite.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Formular {
    /// `action` wie im HTML; leer heißt „diese Adresse“.
    pub action: String,
    /// `method` in Kleinschreibung; `get`, wenn nichts anderes dasteht.
    pub methode: String,
    pub felder: Vec<Feld>,
    pub knoepfe: Vec<Knopf>,
}

/// Was auf einer Seite an Formularen steht.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Seite {
    pub formulare: Vec<Formular>,
}

impl Seite {
    /// Die Formulare einer Seite lesen.
    pub fn lesen(html: &str) -> Seite {
        let mut formulare: Vec<Formular> = Vec::new();
        let mut aktuell: Option<Formular> = None;
        // Eine offene Auswahlliste: Name, erster Wert, schon gewählt?
        let mut auswahl: Option<(String, Option<String>, bool)> = None;

        let mut stelle = 0usize;
        while let Some(treffer) = html[stelle..].find('<') {
            let anfang = stelle + treffer;
            let rest = &html[anfang..];

            // Kommentare und Anweisungen überspringen.
            if rest.starts_with("<!--") {
                match rest.find("-->") {
                    Some(ende) => stelle = anfang + ende + 3,
                    None => break,
                }
                continue;
            }
            if rest.starts_with("<!") || rest.starts_with("<?") {
                match rest.find('>') {
                    Some(ende) => stelle = anfang + ende + 1,
                    None => break,
                }
                continue;
            }
            let Some(relativ) = rest.find('>') else {
                break;
            };
            let ende = anfang + relativ;
            let roh = &html[anfang + 1..ende];
            stelle = ende + 1;

            let (name, attribute) = zerlegen(roh);
            // Skripte und Stile enthalten Text, der wie HTML aussieht –
            // überspringen, damit kein `<form>` aus einem String auftaucht.
            if name == "script" || name == "style" {
                let (_, nach_innen) = innentext(html, stelle, &name);
                match html[nach_innen..].find('>') {
                    Some(relativ) => stelle = nach_innen + relativ + 1,
                    None => break,
                }
                continue;
            }
            match name.as_str() {
                "form" => {
                    if let Some(alt) = aktuell.take() {
                        formulare.push(alt);
                    }
                    let methode = wert(&attribute, "method")
                        .filter(|text| !text.is_empty())
                        .unwrap_or_else(|| "get".to_string())
                        .to_ascii_lowercase();
                    aktuell = Some(Formular {
                        action: wert(&attribute, "action").unwrap_or_default(),
                        methode,
                        felder: Vec::new(),
                        knoepfe: Vec::new(),
                    });
                }
                "/form" => {
                    if let Some(fertig) = aktuell.take() {
                        formulare.push(fertig);
                    }
                }
                "input" => {
                    let art = wert(&attribute, "type")
                        .filter(|text| !text.is_empty())
                        .unwrap_or_else(|| "text".to_string())
                        .to_ascii_lowercase();
                    let feldname = wert(&attribute, "name").unwrap_or_default();
                    let feldwert = wert(&attribute, "value").unwrap_or_default();
                    let art_des_knopfes =
                        matches!(art.as_str(), "submit" | "button" | "reset" | "image");
                    if let Some(formular) = aktuell.as_mut() {
                        if art_des_knopfes {
                            formular.knoepfe.push(Knopf {
                                name: (!feldname.is_empty()).then(|| feldname.clone()),
                                text: if feldwert.is_empty() {
                                    art.clone()
                                } else {
                                    feldwert.clone()
                                },
                                wert: feldwert,
                                art,
                            });
                        } else if !feldname.is_empty() && !matches!(art.as_str(), "file" | "reset")
                        {
                            formular.felder.push(Feld {
                                name: feldname,
                                wert: feldwert,
                                art,
                            });
                        }
                    }
                }
                "textarea" => {
                    let feldname = wert(&attribute, "name").unwrap_or_default();
                    if let Some(formular) = aktuell.as_mut() {
                        if !feldname.is_empty() {
                            formular.felder.push(Feld {
                                name: feldname,
                                wert: String::new(),
                                art: "textarea".to_string(),
                            });
                        }
                    }
                }
                "select" => {
                    let feldname = wert(&attribute, "name").unwrap_or_default();
                    if !feldname.is_empty() {
                        auswahl = Some((feldname, None, false));
                    }
                }
                "option" => {
                    if let Some((_, erster, gewaehlt)) = auswahl.as_mut() {
                        let wert_der_option = wert(&attribute, "value").unwrap_or_default();
                        if erster.is_none() {
                            *erster = Some(wert_der_option.clone());
                        }
                        if attribute.iter().any(|(name, _)| name == "selected") && !*gewaehlt {
                            *gewaehlt = true;
                            *erster = Some(wert_der_option);
                        }
                    }
                }
                "/select" => {
                    if let Some((feldname, wert_der_auswahl, _)) = auswahl.take() {
                        if let Some(formular) = aktuell.as_mut() {
                            formular.felder.push(Feld {
                                name: feldname,
                                wert: wert_der_auswahl.unwrap_or_default(),
                                art: "select".to_string(),
                            });
                        }
                    }
                }
                "button" => {
                    let art = wert(&attribute, "type")
                        .filter(|text| !text.is_empty())
                        .unwrap_or_else(|| "submit".to_string())
                        .to_ascii_lowercase();
                    let knopfname = wert(&attribute, "name");
                    let knopfwert = wert(&attribute, "value").unwrap_or_default();
                    let (inhalt, weiter) = innentext(html, stelle, "button");
                    stelle = weiter;
                    if let Some(formular) = aktuell.as_mut() {
                        formular.knoepfe.push(Knopf {
                            name: knopfname,
                            text: if inhalt.is_empty() {
                                if knopfwert.is_empty() {
                                    "button".to_string()
                                } else {
                                    knopfwert.clone()
                                }
                            } else {
                                inhalt
                            },
                            wert: knopfwert,
                            art,
                        });
                    }
                }
                _ => {}
            }
        }
        if let Some(fertig) = aktuell.take() {
            formulare.push(fertig);
        }

        Seite { formulare }
    }

    /// Eine kurze Beschreibung der Seite – nur Namen, nie Werte.
    ///
    /// Beispiel: `2 Formular(e): login (post → /session; Felder: benutzer,
    /// passwort, csrf; Knöpfe: Anmelden)`.
    pub fn kurzfassung(&self) -> String {
        if self.formulare.is_empty() {
            return "kein Formular".to_string();
        }
        let teile: Vec<String> = self
            .formulare
            .iter()
            .enumerate()
            .map(|(nummer, formular)| {
                let felder = feldnamen(&formular.felder);
                let knoepfe = knopftexte(&formular.knoepfe);
                format!(
                    "#{} {} → {}; Felder: {}; Knöpfe: {}",
                    nummer + 1,
                    formular.methode,
                    if formular.action.is_empty() {
                        "(diese Adresse)"
                    } else {
                        formular.action.as_str()
                    },
                    if felder.is_empty() {
                        "keine".to_string()
                    } else {
                        felder.join(", ")
                    },
                    if knoepfe.is_empty() {
                        "keine".to_string()
                    } else {
                        knoepfe.join(", ")
                    }
                )
            })
            .collect();
        format!(
            "{} Formular(e): {}",
            self.formulare.len(),
            teile.join(" | ")
        )
    }

    /// Das erste Formular mit einem Passwortfeld – das Anmeldeformular.
    pub fn anmeldeformular(&self) -> Option<&Formular> {
        self.formulare
            .iter()
            .find(|formular| formular.passwortfeld().is_some())
    }

    /// Das erste Formular ohne Passwortfeld, das einen Absende-Knopf hat –
    /// die Zustimmungsseite.
    pub fn zustimmungsformular(&self) -> Option<&Formular> {
        self.formulare
            .iter()
            .find(|formular| formular.passwortfeld().is_none() && formular.absendeknopf().is_some())
    }

    /// Alle Feldnamen der Seite, ohne Werte (für Meldungen).
    pub fn feldnamen(&self) -> Vec<String> {
        let mut namen: Vec<String> = Vec::new();
        for formular in &self.formulare {
            for feld in &formular.felder {
                if !namen.contains(&feld.name) {
                    namen.push(feld.name.clone());
                }
            }
        }
        namen
    }
}

impl Formular {
    /// Das Passwortfeld, falls es eines gibt.
    pub fn passwortfeld(&self) -> Option<&Feld> {
        self.felder.iter().find(|feld| feld.art == "password")
    }

    /// Das Feld für den Benutzernamen: das erste sichtbare Textfeld, das nicht
    /// das Passwort ist.
    pub fn benutzerfeld(&self) -> Option<&Feld> {
        self.felder.iter().find(|feld| {
            !matches!(
                feld.art.as_str(),
                "password" | "hidden" | "checkbox" | "radio" | "submit" | "button"
            )
        })
    }

    /// Der Knopf, der das Formular absendet.
    pub fn absendeknopf(&self) -> Option<&Knopf> {
        self.knoepfe
            .iter()
            .find(|knopf| matches!(knopf.art.as_str(), "submit" | "image" | ""))
    }

    /// Die Paare, die beim Absenden geschickt werden: alle Felder mit ihrem
    /// Wert, dazu Name und Wert des Absende-Knopfes.
    pub fn rumpf(&self) -> Vec<(String, String)> {
        let mut paare: Vec<(String, String)> = self
            .felder
            .iter()
            .map(|feld| (feld.name.clone(), feld.wert.clone()))
            .collect();
        if let Some(knopf) = self.absendeknopf() {
            if let Some(name) = &knopf.name {
                paare.push((name.clone(), knopf.wert.clone()));
            }
        }
        paare
    }

    /// Die Paare für die Anmeldung: der Rumpf, aber mit Benutzer und Passwort
    /// an den Feldern, die dafür gedacht sind.
    ///
    /// `Err` nennt, was fehlt – nie einen Wert.
    pub fn anmeldepaar(
        &self,
        benutzer: &str,
        passwort: &str,
    ) -> Result<Vec<(String, String)>, String> {
        let Some(passwortfeld) = self.passwortfeld() else {
            return Err("Das Formular hat kein Passwortfeld.".to_string());
        };
        let Some(benutzerfeld) = self.benutzerfeld() else {
            return Err("Das Formular hat kein Feld für den Benutzernamen.".to_string());
        };
        let mut paare = self.rumpf();
        for (name, wert) in paare.iter_mut() {
            if *name == benutzerfeld.name {
                *wert = benutzer.to_string();
            } else if *name == passwortfeld.name {
                *wert = passwort.to_string();
            }
        }
        Ok(paare)
    }
}

/// Die Namen aller Felder einer Liste.
fn feldnamen(felder: &[Feld]) -> Vec<String> {
    felder.iter().map(|feld| feld.name.clone()).collect()
}

/// Die Texte aller Knöpfe einer Liste.
fn knopftexte(knoepfe: &[Knopf]) -> Vec<String> {
    knoepfe.iter().map(|knopf| knopf.text.clone()).collect()
}

/// Der Wert eines Attributs (Namen sind schon klein geschrieben).
fn wert(attribute: &[(String, String)], name: &str) -> Option<String> {
    attribute
        .iter()
        .find(|(schluessel, _)| schluessel == name)
        .map(|(_, wert)| wert.clone())
}

/// Zerlegt den Inhalt zwischen `<` und `>` in Name und Attribute.
fn zerlegen(roh: &str) -> (String, Vec<(String, String)>) {
    let roh = roh.trim();
    // `</form>` wird zu `/form`: der Schrägstrich gehört zum Namen.
    let (schliessend, roh) = match roh.strip_prefix('/') {
        Some(rest) => (true, rest.trim_start()),
        None => (false, roh),
    };
    let name_ende = roh
        .find(|zeichen: char| zeichen.is_whitespace() || zeichen == '/')
        .unwrap_or(roh.len());
    let name = if schliessend {
        format!("/{}", roh[..name_ende].to_ascii_lowercase())
    } else {
        roh[..name_ende].to_ascii_lowercase()
    };
    let rest = &roh[name_ende..];
    let bytes = rest.as_bytes();
    let mut position = 0usize;
    let mut attribute: Vec<(String, String)> = Vec::new();

    while position < rest.len() {
        while position < rest.len()
            && (bytes[position].is_ascii_whitespace() || bytes[position] == b'/')
        {
            position += 1;
        }
        if position >= rest.len() {
            break;
        }
        let name_anfang = position;
        while position < rest.len()
            && !bytes[position].is_ascii_whitespace()
            && bytes[position] != b'='
            && bytes[position] != b'/'
        {
            position += 1;
        }
        let attribut = rest[name_anfang..position].to_ascii_lowercase();
        if attribut.is_empty() {
            position += 1;
            continue;
        }
        while position < rest.len() && bytes[position].is_ascii_whitespace() {
            position += 1;
        }
        if position < rest.len() && bytes[position] == b'=' {
            position += 1;
            while position < rest.len() && bytes[position].is_ascii_whitespace() {
                position += 1;
            }
            if position < rest.len() && (bytes[position] == b'"' || bytes[position] == b'\'') {
                let anfuehrung = bytes[position];
                position += 1;
                let wert_anfang = position;
                while position < rest.len() && bytes[position] != anfuehrung {
                    position += 1;
                }
                let wert = entschluesseln(&rest[wert_anfang..position]);
                if position < rest.len() {
                    position += 1;
                }
                attribute.push((attribut, wert));
            } else {
                let wert_anfang = position;
                while position < rest.len() && !bytes[position].is_ascii_whitespace() {
                    position += 1;
                }
                let wert = entschluesseln(&rest[wert_anfang..position]);
                attribute.push((attribut, wert));
            }
        } else {
            attribute.push((attribut, String::new()));
        }
    }

    (name, attribute)
}

/// Der sichtbare Text eines Elements ab `stelle`, und wo es danach weitergeht.
fn innentext(html: &str, stelle: usize, name: &str) -> (String, usize) {
    let ende_etikett = format!("</{name}");
    let rest = &html[stelle..];
    let klein = rest.to_ascii_lowercase();
    let (roh, weiter) = match klein.find(&ende_etikett) {
        Some(pos) => (&rest[..pos], stelle + pos),
        None => (rest, html.len()),
    };
    (nur_text(roh), weiter)
}

/// Der Text ohne Etiketten, mit einfachen Leerzeichen.
fn nur_text(roh: &str) -> String {
    let mut text = String::with_capacity(roh.len());
    let mut im_etikett = false;
    for zeichen in roh.chars() {
        match zeichen {
            '<' => im_etikett = true,
            '>' => im_etikett = false,
            sonst if !im_etikett => text.push(sonst),
            _ => {}
        }
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Die üblichen Entitäten auflösen (mehr braucht ein Formular nicht).
fn entschluesseln(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut raus = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find('&') {
        raus.push_str(&rest[..pos]);
        let nach = &rest[pos..];
        let ende = nach.find(';').filter(|ende| *ende <= 10);
        let Some(ende) = ende else {
            raus.push('&');
            rest = &nach[1..];
            continue;
        };
        let name = &nach[1..ende];
        let ersatz = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            "nbsp" => Some(' '),
            _ => None,
        };
        match ersatz {
            Some(zeichen) => {
                raus.push(zeichen);
                rest = &nach[ende + 1..];
            }
            None => {
                raus.push('&');
                rest = &nach[1..];
            }
        }
    }
    raus.push_str(rest);
    raus
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Eine Anmeldeseite, wie sie ein Identity-Provider bauen könnte.
    fn anmeldeseite() -> &'static str {
        r#"<!doctype html>
<html lang="de"><head><title>Anmelden</title></head>
<body>
  <!-- ein Kommentar mit <form action="/falle"> stört nicht -->
  <form method="POST" action="/zep/next/login?lang=de">
    <input type="hidden" name="csrf_token" value="abc&amp;123">
    <input type="text" name="benutzer" placeholder="Benutzername">
    <input type="password" name="passwort">
    <input type="checkbox" name="merken" value="1">
    <select name="mandant">
      <option value="a">A</option>
      <option value="b" selected>B</option>
    </select>
    <button type="submit" name="weiter" value="1">Anmelden</button>
  </form>
  <form action="/suche" method="get"><input name="q"><input type="submit" value="Suchen"></form>
</body></html>"#
    }

    #[test]
    fn formulare_mit_feldern_und_knoepfen() {
        let seite = Seite::lesen(anmeldeseite());
        assert_eq!(seite.formulare.len(), 2);

        let anmelden = &seite.formulare[0];
        assert_eq!(anmelden.methode, "post");
        assert_eq!(anmelden.action, "/zep/next/login?lang=de");
        let namen: Vec<&str> = anmelden
            .felder
            .iter()
            .map(|feld| feld.name.as_str())
            .collect();
        assert_eq!(
            namen,
            vec!["csrf_token", "benutzer", "passwort", "merken", "mandant"]
        );
        assert_eq!(anmelden.felder[0].wert, "abc&123");
        assert_eq!(anmelden.felder[4].wert, "b", "die gewählte Option gilt");
        assert_eq!(anmelden.knoepfe.len(), 1);
        assert_eq!(anmelden.knoepfe[0].name.as_deref(), Some("weiter"));
        assert_eq!(anmelden.knoepfe[0].text, "Anmelden");

        let suche = &seite.formulare[1];
        assert_eq!(suche.methode, "get");
        assert_eq!(suche.felder[0].name, "q");
        assert_eq!(suche.knoepfe[0].art, "submit");
    }

    #[test]
    fn die_rolle_des_formulars_ergibt_sich_aus_den_feldern() {
        let seite = Seite::lesen(anmeldeseite());
        let anmelden = seite.anmeldeformular().expect("Anmeldeformular fehlt");
        assert_eq!(anmelden.passwortfeld().unwrap().name, "passwort");
        assert_eq!(anmelden.benutzerfeld().unwrap().name, "benutzer");
        // Das zweite Formular hat kein Passwortfeld.
        assert_eq!(seite.zustimmungsformular().unwrap().action, "/suche");
    }

    #[test]
    fn der_rumpf_nennt_alle_felder_und_den_knopf() {
        let seite = Seite::lesen(anmeldeseite());
        let anmelden = &seite.formulare[0];
        let paare = anmelden.anmeldepaar("ada", "streng-geheim").unwrap();
        assert_eq!(
            paare,
            vec![
                ("csrf_token".to_string(), "abc&123".to_string()),
                ("benutzer".to_string(), "ada".to_string()),
                ("passwort".to_string(), "streng-geheim".to_string()),
                ("merken".to_string(), "1".to_string()),
                ("mandant".to_string(), "b".to_string()),
                ("weiter".to_string(), "1".to_string()),
            ]
        );
    }

    #[test]
    fn ohne_passwortfeld_gibt_es_keine_anmeldung() {
        let seite = Seite::lesen(anmeldeseite());
        let fehler = seite.formulare[1].anmeldepaar("ada", "geheim").unwrap_err();
        assert!(fehler.contains("Passwortfeld"), "{fehler}");
    }

    #[test]
    fn eine_zustimmungsseite_hat_nur_knoepfe_und_versteckte_felder() {
        let html = r#"<html><body><h1>Zugriff erlauben?</h1>
            <form action="/oauth/authorize" method="post">
              <input type="hidden" name="client_id" value="quiver-zep-1">
              <input type="hidden" name="consent" value="ja">
              <button type="submit">OK</button>
              <button type="button">Ablehnen</button>
            </form></body></html>"#;
        let seite = Seite::lesen(html);
        assert_eq!(seite.formulare.len(), 1);
        assert!(seite.anmeldeformular().is_none());
        let zustimmung = seite.zustimmungsformular().expect("keine Zustimmung");
        assert_eq!(zustimmung.knoepfe.len(), 2);
        assert_eq!(
            zustimmung.rumpf(),
            vec![
                ("client_id".to_string(), "quiver-zep-1".to_string()),
                ("consent".to_string(), "ja".to_string()),
            ]
        );
        assert!(seite.kurzfassung().contains("client_id"));
        assert!(!seite.kurzfassung().contains("quiver-zep-1"), "keine Werte");
    }

    #[test]
    fn eine_seite_ohne_formular_wird_erkannt() {
        let seite = Seite::lesen("<html><body>Etwas ist schiefgelaufen.</body></html>");
        assert!(seite.formulare.is_empty());
        assert_eq!(seite.kurzfassung(), "kein Formular");
        assert!(seite.anmeldeformular().is_none());
        assert!(seite.zustimmungsformular().is_none());
        assert!(seite.feldnamen().is_empty());
    }

    #[test]
    fn ein_nicht_geschlossenes_formular_zaehlt_trotzdem() {
        let seite = Seite::lesen("<form action='/x'><input name='a' value='1'>");
        assert_eq!(seite.formulare.len(), 1);
        assert_eq!(seite.formulare[0].action, "/x");
        assert_eq!(seite.formulare[0].felder[0].name, "a");
    }

    #[test]
    fn knoepfe_mit_inhalt_und_ohne_namen() {
        let seite = Seite::lesen(
            "<form><button type='submit'>  Weiter
            <b>jetzt</b> </button><input type='submit' value='Abbrechen'></form>",
        );
        let knoepfe = &seite.formulare[0].knoepfe;
        assert_eq!(knoepfe.len(), 2);
        assert_eq!(knoepfe[0].text, "Weiter jetzt");
        assert_eq!(knoepfe[0].name, None);
        assert_eq!(knoepfe[1].text, "Abbrechen");
        assert!(seite.zustimmungsformular().is_some());
    }

    #[test]
    fn unbekannte_etiketten_stoeren_nicht() {
        let seite = Seite::lesen(
            "<div><script>var a = \"<form>\";</script><p>Text</p>\
             <form action='/a'><label>Benutzer</label><input name='u'></form></div>",
        );
        assert_eq!(seite.formulare.len(), 1);
        assert_eq!(seite.formulare[0].felder.len(), 1);
    }

    #[test]
    fn feldnamen_der_seite_sind_eindeutig() {
        let seite = Seite::lesen(
            "<form><input name='a'><input name='b'></form><form><input name='a'></form>",
        );
        assert_eq!(seite.feldnamen(), vec!["a", "b"]);
    }

    #[test]
    fn entschluesseln_kennt_die_ueblichen_entitaeten() {
        assert_eq!(
            entschluesseln("a&amp;b&lt;c&gt;d&quot;e&#39;f"),
            "a&b<c>d\"e'f"
        );
        assert_eq!(entschluesseln("ohne"), "ohne");
        assert_eq!(entschluesseln("a&unbekannt;b"), "a&unbekannt;b");
        assert_eq!(entschluesseln("&"), "&");
    }
}
