// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Einrichtungsseite als HTML (SPEC §4).
//!
//! Reine Funktionen: [`Sicht`] hinein, HTML heraus. Hier wird nichts geladen und
//! nichts gerechnet, nur dargestellt – und alles, was aus Daten kommt, wird
//! maskiert. Ein Geheimnis kommt in dieser Datei nicht vor: die [`Sicht`]
//! transportiert nur nicht-geheime Werte.

use std::collections::{BTreeMap, BTreeSet};

use html_escape::encode_double_quoted_attribute;

use std::borrow::Cow;

/// Maskiert `&`, `<`, `>` und `"` – damit ist das Ergebnis sowohl als Textinhalt
/// als auch in einem doppelt gequoteten Attribut richtig. Adressen bleiben
/// dabei lesbar (kein `&#x2F;`).
fn encode_text(text: &str) -> Cow<'_, str> {
    encode_double_quoted_attribute(text)
}

use crate::{Feld, FeldArt};

/// `assets/tokens.css` – die Kopie aus mcp-quiver, nicht von Hand ändern.
pub const TOKENS: &str = include_str!("../../../../assets/tokens.css");

/// Das eigene Aussehen der Seite.
pub const STIL: &str = include_str!("stil.css");

/// Die Schriften (IBM Plex Sans), ausgeliefert unter `schrift/<name>`.
pub const SCHRIFTEN: [(&str, &[u8]); 2] = [
    (
        "PlexSans-Regular.ttf",
        include_bytes!("../../../../assets/PlexSans-Regular.ttf"),
    ),
    (
        "PlexSans-SemiBold.ttf",
        include_bytes!("../../../../assets/PlexSans-SemiBold.ttf"),
    ),
];

/// Eine Meldung über dem Inhalt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meldung {
    pub ton: Ton,
    pub text: String,
}

impl Meldung {
    pub fn gut(text: impl Into<String>) -> Self {
        Meldung {
            ton: Ton::Gut,
            text: text.into(),
        }
    }

    pub fn fehler(text: impl Into<String>) -> Self {
        Meldung {
            ton: Ton::Fehler,
            text: text.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ton {
    Gut,
    Fehler,
}

/// Ein Profil in der Liste – ohne Geheimnisse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfilZeile {
    pub name: String,
    pub beschreibung: String,
    /// Nicht-geheime Werte, wie sie in der Zeile stehen.
    pub werte: Vec<(String, String)>,
    pub standard: bool,
}

/// Das Formular: anlegen oder bearbeiten. Geheimnisse stehen hier nie drin;
/// `gesetzt` sagt nur, **dass** eins hinterlegt ist.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Formular {
    pub neu: bool,
    pub name: String,
    pub beschreibung: String,
    pub werte: BTreeMap<String, String>,
    pub gesetzt: BTreeSet<String>,
    pub fehler: Option<String>,
}

/// Alles, was die Seite zeigt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sicht {
    /// Anzeigename des Werkzeugs, z. B. „Outline-Wiki“.
    pub titel: String,
    /// Pfadpräfix mit Einmal-Token, mit abschließendem `/`.
    pub basis: String,
    pub felder: Vec<Feld>,
    pub profile: Vec<ProfilZeile>,
    pub standard: Option<String>,
    pub meldung: Option<Meldung>,
    pub formular: Option<Formular>,
}

/// Wie der Kopf die Theme-Auswahl beschriftet (SPEC §4, AGENTS.md §2.7).
const THEMEN: [(&str, &str); 3] = [("dark", "Dunkel"), ("light", "Hell"), ("mono", "Mono")];

/// Baut die ganze Seite.
pub fn seite(sicht: &Sicht) -> String {
    let titel = format!("{} · Einrichtung", sicht.titel);
    let mut html = String::with_capacity(8 * 1024);

    html.push_str("<!doctype html>\n<html lang=\"de\" data-theme=\"dark\">\n<head>\n");
    html.push_str("<meta charset=\"utf-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    html.push_str("<meta name=\"referrer\" content=\"no-referrer\">\n");
    html.push_str(&format!("<title>{}</title>\n", encode_text(&titel)));
    html.push_str(&format!(
        "<link rel=\"stylesheet\" href=\"{}stil.css\">\n",
        encode_double_quoted_attribute(&sicht.basis)
    ));
    html.push_str("</head>\n<body>\n");

    html.push_str(&kopf(sicht));
    html.push_str("<main class=\"inhalt\">\n<div class=\"stapel\">\n");
    if let Some(meldung) = &sicht.meldung {
        html.push_str(&format!(
            "<div class=\"meldung meldung--{}\" role=\"status\"><p class=\"meldung__text\">{}</p></div>\n",
            match meldung.ton {
                Ton::Gut => "gut",
                Ton::Fehler => "fehler",
            },
            encode_text(&meldung.text)
        ));
    }
    html.push_str(&format!(
        "<div class=\"seitenkopf\">\n<h1 class=\"seitenkopf__titel\">Zugänge</h1>\n<p class=\"seitenkopf__unter\">Profile für {}, gespeichert auf diesem Rechner. Passwörter und Tokens liegen im Schlüsselbund und erscheinen hier nie.</p>\n</div>\n",
        encode_text(&sicht.titel)
    ));
    html.push_str(&liste(sicht));
    if let Some(werte) = &sicht.formular {
        html.push_str(&formular(sicht, werte));
    }
    html.push_str(&fuss(sicht));
    html.push_str("</div>\n</main>\n");
    html.push_str(&skript());
    html.push_str("</body>\n</html>\n");
    html
}

/// Kopf mit Wortmarke und Segmentauswahl für das Farbschema.
fn kopf(sicht: &Sicht) -> String {
    let mut html = String::from("<header class=\"kopf\">\n");
    html.push_str(&format!(
        "<span class=\"marke\">{}</span>\n",
        encode_text(&format!("{} · Einrichtung", sicht.titel))
    ));
    html.push_str(
        "<div class=\"kopf__rechts\">\n<div class=\"segment\" role=\"radiogroup\" aria-label=\"Farbschema\">\n",
    );
    for (wert, name) in THEMEN {
        html.push_str(&format!(
            "<button class=\"segment__knopf{aktiv}\" type=\"button\" role=\"radio\" aria-checked=\"{checked}\" data-theme-set=\"{wert}\">{name}</button>\n",
            aktiv = if wert == "dark" { " is-aktiv" } else { "" },
            checked = if wert == "dark" { "true" } else { "false" },
            wert = wert,
            name = name
        ));
    }
    html.push_str("</div>\n</div>\n</header>\n");
    html
}

/// Die Liste der Profile mit Aktionen.
fn liste(sicht: &Sicht) -> String {
    let mut html = String::from("<section class=\"karte\">\n<header class=\"karte__kopf\">\n");
    html.push_str("<h2 class=\"karte__titel\">Profile</h2>\n");
    html.push_str(&format!(
        "<p class=\"karte__unter\">{}</p>\n</header>\n",
        if sicht.profile.is_empty() {
            "Noch keins – leg unten das erste an.".to_string()
        } else {
            format!("{} eingerichtet.", anzahl(sicht.profile.len()))
        }
    ));

    if sicht.profile.is_empty() {
        html.push_str("</section>\n");
        return html;
    }

    html.push_str("<ul class=\"zeilen\">\n");
    for zeile in &sicht.profile {
        html.push_str("<li class=\"zeile\">\n<div class=\"zeile__text\">\n");
        html.push_str("<span class=\"zeile__name\">");
        html.push_str(&encode_text(&zeile.name));
        if zeile.standard || sicht.standard.as_deref() == Some(zeile.name.as_str()) {
            html.push_str(" <span class=\"abzeichen\">Standard</span>");
        }
        html.push_str("</span>\n");
        let mut unter: Vec<String> = Vec::new();
        if !zeile.beschreibung.is_empty() {
            unter.push(zeile.beschreibung.clone());
        }
        for (name, wert) in &zeile.werte {
            if wert.is_empty() {
                continue;
            }
            unter.push(format!("{name}: {wert}"));
        }
        if !unter.is_empty() {
            html.push_str(&format!(
                "<span class=\"zeile__unter\">{}</span>\n",
                encode_text(&unter.join(" · "))
            ));
        }
        html.push_str("</div>\n<div class=\"zeile__knoepfe\">\n");

        // Bearbeiten ist ein Link, alles andere ändert etwas: nur per POST.
        html.push_str(&format!(
            "<a class=\"knopf\" href=\"{}\">Bearbeiten</a>\n",
            encode_double_quoted_attribute(&format!(
                "{}bearbeiten?name={}",
                sicht.basis,
                encode_text(&zeile.name)
            ))
        ));
        html.push_str(&aktion(
            sicht,
            "testen",
            &zeile.name,
            "Testen",
            "knopf",
            None,
        ));
        if sicht.standard.as_deref() != Some(zeile.name.as_str()) {
            html.push_str(&aktion(
                sicht,
                "standard",
                &zeile.name,
                "Als Standard",
                "knopf knopf--still",
                None,
            ));
        }
        html.push_str(&aktion(
            sicht,
            "loeschen",
            &zeile.name,
            "Löschen",
            "knopf knopf--still",
            Some("Profil wirklich löschen? Das Passwort wird dabei vergessen."),
        ));
        html.push_str("</div>\n</li>\n");
    }
    html.push_str("</ul>\n</section>\n");
    html
}

/// Ein kleiner POST-Knopf mit verstecktem Profilnamen.
fn aktion(
    sicht: &Sicht,
    pfad: &str,
    name: &str,
    beschriftung: &str,
    klasse: &str,
    rueckfrage: Option<&str>,
) -> String {
    format!(
        "<form method=\"post\" action=\"{ziel}\"{rueckfrage}><input type=\"hidden\" name=\"name\" value=\"{name}\"><button class=\"{klasse}\" type=\"submit\">{beschriftung}</button></form>\n",
        ziel = encode_double_quoted_attribute(&format!("{}{}", sicht.basis, pfad)),
        rueckfrage = rueckfrage
            .map(|text| format!(
                " onsubmit=\"return confirm('{}')\"",
                text.replace('\'', "\\'")
            ))
            .unwrap_or_default(),
        name = encode_double_quoted_attribute(name),
        klasse = klasse,
        beschriftung = encode_text(beschriftung),
    )
}

/// Das Formular aus dem Feldschema (SPEC §4).
fn formular(sicht: &Sicht, formular: &Formular) -> String {
    let mut html = String::from("<section class=\"karte\">\n<header class=\"karte__kopf\">\n");
    html.push_str(&format!(
        "<h2 class=\"karte__titel\">{}</h2>\n<p class=\"karte__unter\">{}</p>\n</header>\n",
        if formular.neu {
            "Neues Profil"
        } else {
            "Profil bearbeiten"
        },
        "Pflichtfelder sind mit * gekennzeichnet."
    ));
    if let Some(fehler) = &formular.fehler {
        html.push_str(&format!(
            "<div class=\"meldung meldung--fehler\" role=\"alert\"><p class=\"meldung__text\">{}</p></div>\n",
            encode_text(fehler)
        ));
    }

    html.push_str(&format!(
        "<form class=\"formular\" method=\"post\" action=\"{}\">\n",
        encode_double_quoted_attribute(&format!("{}speichern", sicht.basis))
    ));

    // Name: beim Bearbeiten fest, beim Anlegen frei.
    if formular.neu {
        html.push_str(&feld_rahmen(
            "feld-name",
            "Name *",
            &format!(
                "<input class=\"eingabe\" id=\"feld-name\" name=\"name\" type=\"text\" value=\"{}\" autocomplete=\"off\" autofocus required maxlength=\"40\" pattern=\"[a-z0-9_\\-]+\">",
                encode_double_quoted_attribute(&formular.name)
            ),
            "a–z, 0–9, „-“ und „_“; 1–40 Zeichen.",
        ));
    } else {
        html.push_str(&feld_rahmen(
            "feld-name",
            "Name",
            &format!(
                "<input type=\"hidden\" name=\"name\" value=\"{}\"><span class=\"leise\" id=\"feld-name\">{}</span>",
                encode_double_quoted_attribute(&formular.name),
                encode_text(&formular.name)
            ),
            "Der Name lässt sich nicht ändern.",
        ));
    }

    html.push_str(&feld_rahmen(
        "feld-beschreibung",
        "Beschreibung",
        &format!(
            "<input class=\"eingabe\" id=\"feld-beschreibung\" name=\"beschreibung\" type=\"text\" value=\"{}\" autocomplete=\"off\">",
            encode_double_quoted_attribute(&formular.beschreibung)
        ),
        "Wofür ist dieses Profil da?",
    ));

    for feld in &sicht.felder {
        html.push_str(&feld_rahmen(
            &format!("feld-{}", feld.name),
            &feld_titel(feld),
            &eingabe(feld, formular),
            &hilfe(feld, formular),
        ));
    }

    html.push_str("<div class=\"reihe\">\n");
    html.push_str(&format!(
        "<button class=\"knopf knopf--haupt\" type=\"submit\">{}</button>\n",
        if formular.neu {
            "Profil anlegen"
        } else {
            "Speichern"
        }
    ));
    html.push_str(&format!(
        "<a class=\"knopf knopf--still\" href=\"{}\">Abbrechen</a>\n",
        encode_double_quoted_attribute(&sicht.basis)
    ));
    html.push_str("</div>\n</form>\n</section>\n");
    html
}

fn feld_titel(feld: &Feld) -> String {
    if feld.pflicht {
        format!("{} *", feld.titel)
    } else {
        feld.titel.to_string()
    }
}

/// Das Bedienelement eines Feldes – ohne Geheimnis.
fn eingabe(feld: &Feld, formular: &Formular) -> String {
    let id = format!("feld-{}", feld.name);
    let name = format!("f_{}", feld.name);
    let wert = formular
        .werte
        .get(feld.name)
        .cloned()
        .or_else(|| feld.standard.map(str::to_string))
        .unwrap_or_default();
    let wert_attribut = encode_double_quoted_attribute(&wert);
    let id = encode_double_quoted_attribute(&id);
    let name = encode_double_quoted_attribute(&name);

    match &feld.art {
        FeldArt::Passwort => {
            let gesetzt = formular.gesetzt.contains(feld.name);
            format!(
                "<input class=\"eingabe\" id=\"{id}\" name=\"{name}\" type=\"password\" value=\"\" autocomplete=\"new-password\"{platzhalter}>",
                platzhalter = if gesetzt {
                    " placeholder=\"gesetzt – leer lassen, um es zu behalten\""
                } else {
                    ""
                }
            )
        }
        FeldArt::Zahl => format!(
            "<input class=\"eingabe\" id=\"{id}\" name=\"{name}\" type=\"number\" step=\"any\" value=\"{wert_attribut}\" inputmode=\"numeric\">"
        ),
        FeldArt::Schalter => {
            let an = matches!(wert.as_str(), "true" | "1" | "ja" | "an");
            format!(
                "<input class=\"eingabe eingabe--schalter\" id=\"{id}\" name=\"{name}\" type=\"checkbox\" value=\"true\"{an}>",
                an = if an { " checked" } else { "" }
            )
        }
        FeldArt::Url => format!(
            "<input class=\"eingabe\" id=\"{id}\" name=\"{name}\" type=\"url\" value=\"{wert_attribut}\" autocomplete=\"off\" spellcheck=\"false\">"
        ),
        FeldArt::Auswahl(moeglichkeiten) => {
            let mut html = format!("<select class=\"eingabe\" id=\"{id}\" name=\"{name}\">");
            for (wahl, anzeige) in moeglichkeiten {
                html.push_str(&format!(
                    "<option value=\"{}\"{}>{}</option>",
                    encode_double_quoted_attribute(wahl),
                    if *wahl == wert { " selected" } else { "" },
                    encode_text(anzeige)
                ));
            }
            html.push_str("</select>");
            html
        }
        FeldArt::Text => format!(
            "<input class=\"eingabe\" id=\"{id}\" name=\"{name}\" type=\"text\" value=\"{wert_attribut}\" autocomplete=\"off\">"
        ),
    }
}

/// Der Hilfetext unter einem Feld, ergänzt um „gesetzt“ bei Geheimnissen.
fn hilfe(feld: &Feld, formular: &Formular) -> String {
    let mut teile: Vec<String> = Vec::new();
    if feld.art == FeldArt::Passwort && formular.gesetzt.contains(feld.name) {
        teile.push("Ein Wert ist gespeichert; leer lassen behält ihn.".to_string());
    }
    if !feld.hilfe.is_empty() {
        teile.push(feld.hilfe.to_string());
    }
    teile.join(" ")
}

fn feld_rahmen(id: &str, beschriftung: &str, inhalt: &str, hinweis: &str) -> String {
    let id = encode_double_quoted_attribute(id);
    let mut html = String::from("<div class=\"feld\">\n");
    html.push_str(&format!(
        "<label class=\"feld__label\" for=\"{id}\">{}</label>\n",
        encode_text(beschriftung)
    ));
    html.push_str(&format!("<div class=\"feld__inhalt\">\n{inhalt}\n"));
    if !hinweis.is_empty() {
        html.push_str(&format!(
            "<p class=\"feld__hinweis\">{}</p>\n",
            encode_text(hinweis)
        ));
    }
    html.push_str("</div>\n</div>\n");
    html
}

/// „Speichern“ und „Fertig“ – der Abschluss der Seite.
fn fuss(sicht: &Sicht) -> String {
    let mut html = String::from("<div class=\"fuss\">\n");
    html.push_str(&format!(
        "<form method=\"post\" action=\"{}\"><button class=\"knopf knopf--haupt\" type=\"submit\">Fertig</button></form>\n",
        encode_double_quoted_attribute(&format!("{}fertig", sicht.basis))
    ));
    html.push_str(&format!(
        "<a class=\"knopf knopf--still\" href=\"{}\">Neu laden</a>\n",
        encode_double_quoted_attribute(&sicht.basis)
    ));
    html.push_str("</div>\n");
    html
}

/// Theme-Umschalter (SPEC §4: Standard nach `prefers-color-scheme`).
fn skript() -> String {
    String::from(
        r#"<script>
(() => {
  const wurzel = document.documentElement;
  const speicher = "quiver-thema";

  const setze = (wert, sichern) => {
    wurzel.dataset.theme = wert;
    document.querySelectorAll("[data-theme-set]").forEach((k) => {
      const aktiv = k.dataset.themeSet === wert;
      k.classList.toggle("is-aktiv", aktiv);
      k.setAttribute("aria-checked", String(aktiv));
    });
    if (sichern) localStorage.setItem(speicher, wert);
  };

  const merken = localStorage.getItem(speicher);
  const hell = window.matchMedia && window.matchMedia("(prefers-color-scheme: light)").matches;
  setze(merken || (hell ? "light" : "dark"), false);

  document.querySelectorAll("[data-theme-set]").forEach((k) => {
    k.addEventListener("click", () => setze(k.dataset.themeSet, true));
  });
})();
</script>
"#,
    )
}

/// Die Seite, die nach „Fertig“ stehen bleibt: keine Formulare mehr, alles
/// inline, damit sie auch nach dem Beenden der Seite vollständig ist.
pub fn abschluss(titel: &str, gespeichert: bool) -> String {
    format!(
        "<!doctype html>\n<html lang=\"de\" data-theme=\"dark\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{titel} · Fertig</title>\n<style>\n{tokens}\n{stil}\n</style>\n</head>\n<body>\n<header class=\"kopf\"><span class=\"marke\">{marke}</span></header>\n<main class=\"inhalt\">\n<div class=\"stapel\">\n<div class=\"meldung meldung--gut\" role=\"status\"><p class=\"meldung__text\">{text}</p></div>\n<p class=\"leise\">Dieses Fenster lässt sich schließen.</p>\n</div>\n</main>\n</body>\n</html>\n",
        titel = encode_text(titel),
        tokens = TOKENS,
        stil = STIL,
        marke = encode_text(&format!("{titel} · Einrichtung")),
        text = encode_text(if gespeichert {
            "Fertig – die Zugänge sind gespeichert."
        } else {
            "Fertig – es wurde nichts geändert."
        }),
    )
}

fn anzahl(zahl: usize) -> String {
    if zahl == 1 {
        "1 Profil".to_string()
    } else {
        format!("{zahl} Profile")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FeldArt;

    fn feld(name: &'static str, titel: &'static str, art: FeldArt) -> Feld {
        Feld {
            name,
            titel,
            art,
            pflicht: true,
            standard: None,
            hilfe: "Hilfe",
        }
    }

    fn sicht() -> Sicht {
        Sicht {
            titel: "Outline-Wiki".to_string(),
            basis: "/e/abc123/".to_string(),
            felder: vec![
                feld("url", "Adresse", FeldArt::Url),
                feld("token", "Token", FeldArt::Passwort),
            ],
            profile: vec![ProfilZeile {
                name: "arbeit".to_string(),
                beschreibung: "Mein <Zugang>".to_string(),
                werte: vec![("url".to_string(), "https://example.test".to_string())],
                standard: true,
            }],
            standard: Some("arbeit".to_string()),
            meldung: None,
            formular: None,
        }
    }

    #[test]
    fn html_maskiert_daten() {
        let mut sicht = sicht();
        sicht.profile[0].beschreibung = "<script>alert('x')</script>".to_string();
        sicht.profile[0].name = "arbeit\"onload=\"alert(1)".to_string();
        let html = seite(&sicht);

        assert!(!html.contains("<script>alert"));
        assert!(html.contains("&lt;script&gt;alert"));
        // In Attributen ist auch das Anführungszeichen maskiert.
        assert!(html.contains("&quot;onload=&quot;alert(1)"));
        assert!(!html.contains("\"onload=\""));
    }

    #[test]
    fn formular_zeigt_kein_geheimnis() {
        let mut sicht = sicht();
        let mut formular = Formular {
            neu: false,
            name: "arbeit".to_string(),
            beschreibung: "Mein Zugang".to_string(),
            ..Formular::default()
        };
        formular.gesetzt.insert("token".to_string());
        sicht.formular = Some(formular);
        let html = seite(&sicht);

        assert!(html.contains("type=\"password\""));
        assert!(html.contains("gesetzt – leer lassen"));
        // Ein Passwortfeld hat nie einen Wert.
        assert!(!html.contains("value=\"s3hr"));
    }

    #[test]
    fn auswahl_und_schalter_werden_gerendert() {
        let mut sicht = sicht();
        sicht.felder = vec![
            feld(
                "art",
                "Art",
                FeldArt::Auswahl(vec![
                    ("a".to_string(), "Alpha".to_string()),
                    ("b".to_string(), "Beta".to_string()),
                ]),
            ),
            feld("aktiv", "Aktiv", FeldArt::Schalter),
            feld("tiefe", "Tiefe", FeldArt::Zahl),
        ];
        let mut formular = Formular {
            neu: false,
            name: "arbeit".to_string(),
            ..Formular::default()
        };
        formular.werte.insert("art".to_string(), "b".to_string());
        formular
            .werte
            .insert("aktiv".to_string(), "true".to_string());
        formular.werte.insert("tiefe".to_string(), "3".to_string());
        sicht.formular = Some(formular);
        let html = seite(&sicht);

        assert!(html.contains("<select"));
        assert!(html.contains("<option value=\"b\" selected>Beta</option>"));
        assert!(html.contains("type=\"checkbox\" value=\"true\" checked"));
        assert!(html.contains("type=\"number\""));
    }

    #[test]
    fn kopf_hat_drei_themen_und_die_wortmarke() {
        let html = seite(&sicht());
        assert!(html.contains("Outline-Wiki · Einrichtung"));
        for wert in ["dark", "light", "mono"] {
            assert!(html.contains(&format!("data-theme-set=\"{wert}\"")));
        }
        assert!(html.contains("prefers-color-scheme"));
    }

    #[test]
    fn alle_pfade_tragen_das_token() {
        let html = seite(&sicht());
        assert!(html.contains("href=\"/e/abc123/stil.css\""));
        assert!(html.contains("action=\"/e/abc123/fertig\""));
        assert!(html.contains("action=\"/e/abc123/loeschen\""));
        assert!(!html.contains("action=\"/fertig\""));
    }
}
