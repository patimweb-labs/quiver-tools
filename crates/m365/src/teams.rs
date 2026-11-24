// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Teams-Nachrichten für die Antworten (docs/aufgaben/05-teams.md):
//!
//! * HTML aus Teams → lesbarer Text (`<p>`, `<br>`, `<li>`, Entities, `<at>`),
//! * Länge je Nachricht begrenzen (`max_zeichen`, Standard 2000) und **sagen**,
//!   dass gekürzt wurde (`gekuerzt`, `laenge_gesamt`),
//! * Absender, Zeit (RFC 3339 mit Versatz wie Aufgabe 04), Anhänge nur als Name,
//! * gelöschte und System-Nachrichten weg (`sichtbar`).
//!
//! Reine Funktionen ohne Netz – damit prüfbar, ohne einen Tenant zu brauchen.

use jiff::Timestamp;
use m365_graph::{Absender, Chat, Mitglied, TeamsNachricht, Treffer, Vorschau};
use serde_json::{json, Value};

use crate::zeit::Zeitzone;

/// Standard-Länge einer Nachricht in der Antwort.
pub const STANDARD_MAX_ZEICHEN: usize = 2000;
/// Mehr Text gibt eine Nachricht nie heraus.
pub const HOECHSTENS_MAX_ZEICHEN: usize = 50_000;
/// Mehr als zwei Leerzeilen hintereinander werden zu zwei.
const HOECHSTENS_LEERZEILEN: usize = 2;

/// `max_zeichen` begrenzen: Standard 2000, immer zwischen 1 und 50 000.
pub fn max_zeichen_begrenzen(max_zeichen: Option<u32>) -> usize {
    max_zeichen
        .unwrap_or(STANDARD_MAX_ZEICHEN as u32)
        .clamp(1, HOECHSTENS_MAX_ZEICHEN as u32) as usize
}

// ---------------------------------------------------------------------------
// HTML → Text
// ---------------------------------------------------------------------------

/// Teams liefert Nachrichten als HTML. Hier wird daraus Text: Block-Elemente
/// werden Zeilen, `<at>` wird zu `@Name`, Entities werden aufgelöst.
pub fn html_zu_text(html: &str) -> String {
    let mut raus = String::with_capacity(html.len());
    let mut rest = html;

    while let Some(stelle) = rest.find('<') {
        raus.push_str(&rest[..stelle]);
        let Some(ende) = rest[stelle..].find('>') else {
            // Kein schließendes `>`: der Rest ist Text.
            raus.push_str(&rest[stelle..]);
            return leerzeilen_begrenzen(&raus);
        };
        let tag = &rest[stelle + 1..stelle + ende];
        // Schließende Tags fügen keine Zeile hinzu – das öffnende Tag hat es
        // schon getan (`</p><p>` ergibt genau einen Zeilenumbruch).
        let schliessend = tag.trim_start().starts_with('/');
        let name = tag
            .trim_start_matches('/')
            .split(|zeichen: char| zeichen.is_whitespace() || zeichen == '/')
            .next()
            .unwrap_or_default()
            .to_lowercase();
        match name.as_str() {
            _ if schliessend => {}
            "br" | "p" | "div" | "li" | "tr" | "blockquote" | "table" | "h1" | "h2" | "h3"
            | "h4" | "h5" | "h6" => raus.push('\n'),
            "td" | "th" => raus.push(' '),
            // `<at id="0">Anna</at>` → „@Anna“.
            "at" => raus.push('@'),
            _ => {}
        }
        rest = &rest[stelle + ende + 1..];
    }
    raus.push_str(rest);

    leerzeilen_begrenzen(&entities_aufloesen(&raus))
}

/// `&amp;`, `&nbsp;`, `&lt;` … auflösen. Unbekanntes bleibt stehen.
fn entities_aufloesen(text: &str) -> String {
    let mut raus = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(stelle) = rest.find('&') {
        raus.push_str(&rest[..stelle]);
        let nach = &rest[stelle..];
        let ende = nach.find(';').filter(|ende| *ende <= 10);
        let Some(ende) = ende else {
            raus.push('&');
            rest = &nach[1..];
            continue;
        };
        let ersetzt = match &nach[1..ende] {
            "amp" => Some("&"),
            "lt" => Some("<"),
            "gt" => Some(">"),
            "quot" => Some("\""),
            "apos" | "#39" => Some("'"),
            "nbsp" => Some(" "),
            "ndash" => Some("–"),
            "mdash" => Some("—"),
            "hellip" => Some("…"),
            _ => None,
        };
        match ersetzt {
            Some(zeichen) => {
                raus.push_str(zeichen);
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

/// Zeilen säubern: höchstens zwei Leerzeilen, keine Leerzeilen am Rand.
fn leerzeilen_begrenzen(text: &str) -> String {
    let mut zeilen: Vec<String> = Vec::new();
    let mut leere = 0usize;
    for zeile in text.lines() {
        if zeile.trim().is_empty() {
            leere += 1;
            if leere > HOECHSTENS_LEERZEILEN || zeilen.is_empty() {
                continue;
            }
        } else {
            leere = 0;
        }
        zeilen.push(zeile.trim_end().to_string());
    }
    while zeilen.last().is_some_and(|zeile| zeile.is_empty()) {
        zeilen.pop();
    }
    zeilen.join("\n")
}

// ---------------------------------------------------------------------------
// Kürzen
// ---------------------------------------------------------------------------

/// Text auf `max` Zeichen kürzen, möglichst hinter einem ganzen Wort.
///
/// Zweiter Wert: die Länge des vollen Textes – nur gesetzt, wenn wirklich
/// gekürzt wurde (dann gehören `gekuerzt` und `laenge_gesamt` in die Antwort).
pub fn text_kuerzen(text: &str, max: usize) -> (String, Option<usize>) {
    let gesamt = text.chars().count();
    if gesamt <= max {
        return (text.to_string(), None);
    }
    let stueck: String = text.chars().take(max).collect();
    let stelle = match stueck.rfind(char::is_whitespace) {
        Some(stelle) if stelle > 0 => stelle,
        _ => stueck.len(),
    };
    (stueck[..stelle].trim_end().to_string(), Some(gesamt))
}

// ---------------------------------------------------------------------------
// Nachrichten als JSON
// ---------------------------------------------------------------------------

/// Gehört die Nachricht in eine Antwort? System-Nachrichten („Anna hat das Team
/// beigetreten“) und gelöschte sagen einem Agenten nichts.
pub fn sichtbar(nachricht: &TeamsNachricht) -> bool {
    if nachricht.deleted_date_time.is_some() {
        return false;
    }
    !matches!(
        nachricht.message_type.as_deref(),
        Some("systemEventMessage")
    )
}

/// „Name“ des Absenders; Anwendungen (Bots) und Unbekanntes inbegriffen.
pub fn absender(nachricht: &TeamsNachricht) -> Option<String> {
    absender_name(nachricht.from.as_ref())
}

/// Absender der letzten Nachricht eines Chats (`lastMessagePreview.from`).
pub fn vorschau_absender(vorschau: &Vorschau) -> Option<String> {
    absender_name(vorschau.from.as_ref())
}

fn absender_name(absender: Option<&Absender>) -> Option<String> {
    let absender = absender?;
    absender
        .user
        .as_ref()
        .and_then(|person| person.display_name.clone())
        .or_else(|| {
            absender
                .application
                .as_ref()
                .and_then(|app| app.display_name.clone())
        })
        .filter(|name| !name.trim().is_empty())
}

/// Nur die Namen der Anhänge – Inhalte gehören nicht in eine Tool-Antwort.
pub fn anhaenge(nachricht: &TeamsNachricht) -> Vec<String> {
    nachricht
        .attachments
        .as_deref()
        .unwrap_or_default()
        .iter()
        .filter_map(|anhang| anhang.name.clone())
        .filter(|name| !name.trim().is_empty())
        .collect()
}

fn text_von(nachricht: &TeamsNachricht) -> String {
    nachricht
        .body
        .as_ref()
        .and_then(|koerper| koerper.content.clone())
        .map(|inhalt| html_zu_text(&inhalt))
        .unwrap_or_default()
}

/// Zeit einer Nachricht als RFC 3339 mit Versatz (Graph liefert UTC mit `Z`).
pub fn zeit_von(nachricht: &TeamsNachricht, zone: &Zeitzone) -> Option<String> {
    nachricht
        .created_date_time
        .as_deref()
        .and_then(|zeit| zone.zeitpunkt(zeit, None))
}

/// Eine Nachricht als knappes JSON; `None` für gelöschte und System-Nachrichten.
pub fn nachricht_json(
    nachricht: &TeamsNachricht,
    zone: &Zeitzone,
    max_zeichen: usize,
) -> Option<Value> {
    if !sichtbar(nachricht) {
        return None;
    }
    let (text, laenge) = text_kuerzen(&text_von(nachricht), max_zeichen);

    let mut wert = json!({
        "id": nachricht.id,
        "von": absender(nachricht),
        "zeit": zeit_von(nachricht, zone),
        "text": text,
    });
    if let Some(laenge) = laenge {
        wert["gekuerzt"] = json!(true);
        wert["laenge_gesamt"] = json!(laenge);
    }
    if let Some(betreff) = nachricht
        .subject
        .as_deref()
        .filter(|b| !b.trim().is_empty())
    {
        wert["betreff"] = json!(betreff);
    }
    let anhaenge = anhaenge(nachricht);
    if !anhaenge.is_empty() {
        wert["anhaenge"] = json!(anhaenge);
    }
    if let Some(link) = nachricht
        .web_url
        .as_deref()
        .filter(|link| !link.trim().is_empty())
    {
        wert["link"] = json!(link);
    }
    Some(wert)
}

/// Eine Nachricht mit der Anzahl ihrer Antworten (Kanalübersicht).
pub fn beitrag_json(
    nachricht: &TeamsNachricht,
    antworten: usize,
    zone: &Zeitzone,
    max_zeichen: usize,
) -> Option<Value> {
    let mut wert = nachricht_json(nachricht, zone, max_zeichen)?;
    wert["antworten"] = json!(antworten);
    Some(wert)
}

/// Antwort auf ein Senden: Id der neuen Nachricht und Web-Link, falls es einen gibt.
pub fn gesendet_json(nachricht: &TeamsNachricht) -> Value {
    let mut wert = json!({"id": nachricht.id, "gesendet": true});
    if let Some(link) = nachricht
        .web_url
        .as_deref()
        .filter(|link| !link.trim().is_empty())
    {
        wert["link"] = json!(link);
    }
    wert
}

// ---------------------------------------------------------------------------
// Chats, Mitglieder, Suche
// ---------------------------------------------------------------------------

/// Wie ein Chat für Menschen heißt: Thema, sonst die Mitglieder ohne mich.
pub fn chat_titel(chat: &Chat, ich: Option<&str>) -> String {
    if let Some(thema) = chat
        .topic
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        return thema.to_string();
    }
    let namen: Vec<&str> = chat
        .members
        .as_deref()
        .unwrap_or_default()
        .iter()
        .filter_map(|mitglied| mitglied.display_name.as_deref())
        .filter(|name| !name.trim().is_empty())
        .filter(|name| Some(*name) != ich)
        .collect();
    if namen.is_empty() {
        chat.id.clone()
    } else {
        namen.join(", ")
    }
}

pub fn mitglied_json(mitglied: &Mitglied) -> Value {
    json!({
        "id": mitglied.id,
        "name": mitglied.display_name,
        "mail": mitglied.email,
        "benutzer": mitglied.user_id,
        "rollen": mitglied.roles.clone().unwrap_or_default(),
    })
}

/// Suchtreffer als JSON. Teams markiert die Treffer im `summary` mit `<c0>` –
/// das sind keine Inhalte, sondern Markierungen; sie werden entfernt.
pub fn treffer_json(treffer: &Treffer, zone: &Zeitzone, max_zeichen: usize) -> Option<Value> {
    let resource = treffer.resource.as_ref()?;
    if !sichtbar(resource) {
        return None;
    }
    let mut wert = nachricht_json(resource, zone, max_zeichen)?;

    if wert["text"].as_str().is_none_or(str::is_empty) {
        if let Some(zusammenfassung) = treffer.summary.as_deref() {
            let (text, laenge) = text_kuerzen(
                &html_zu_text(&hervorhebung_entfernen(zusammenfassung)),
                max_zeichen,
            );
            wert["text"] = json!(text);
            if let Some(laenge) = laenge {
                wert["gekuerzt"] = json!(true);
                wert["laenge_gesamt"] = json!(laenge);
            }
        }
    }

    if let Some(chat) = resource.chat_id.as_deref() {
        wert["chat"] = json!(chat);
    }
    if let Some(kanal) = resource.channel_identity.as_ref() {
        wert["team"] = json!(kanal.team_id);
        wert["kanal"] = json!(kanal.channel_id);
    }
    Some(wert)
}

/// `<c0>Wort</c0>` → `Wort` (Microsoft Search markiert so die Treffer).
pub fn hervorhebung_entfernen(text: &str) -> String {
    let mut raus = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(stelle) = rest.find('<') {
        raus.push_str(&rest[..stelle]);
        let nach = &rest[stelle..];
        let Some(ende) = nach.find('>') else {
            raus.push_str(nach);
            return raus;
        };
        let tag = &nach[1..ende];
        let name = tag.strip_prefix('/').unwrap_or(tag);
        let markierung = name
            .strip_prefix('c')
            .is_some_and(|zahl| !zahl.is_empty() && zahl.chars().all(|z| z.is_ascii_digit()));
        if !markierung {
            raus.push_str(&nach[..=ende]);
        }
        rest = &nach[ende + 1..];
    }
    raus.push_str(rest);
    raus
}

/// Liegt `zeit` innerhalb der letzten `tage`? Unlesbares bleibt draußen.
pub fn innerhalb(zeit: Option<&str>, tage: u32, jetzt: Timestamp) -> bool {
    let Some(zeit) = zeit.and_then(|roh| roh.trim().parse::<Timestamp>().ok()) else {
        return false;
    };
    let grenze = jetzt.as_second() - i64::from(tage) * 86_400;
    zeit.as_second() >= grenze
}

/// Ist `neu` später als `alt`? Fehlt eine der Zeiten, gilt „nein“.
pub fn ist_neuer(neu: Option<&str>, alt: Option<&str>) -> bool {
    let Some(neu) = neu.and_then(|roh| roh.trim().parse::<Timestamp>().ok()) else {
        return false;
    };
    let Some(alt) = alt.and_then(|roh| roh.trim().parse::<Timestamp>().ok()) else {
        return false;
    };
    neu > alt
}

#[cfg(test)]
mod tests {
    use super::*;
    use m365_graph::Koerper;

    fn nachricht(typ: Option<&str>, html: &str) -> TeamsNachricht {
        TeamsNachricht {
            id: "n1".to_string(),
            message_type: typ.map(str::to_string),
            created_date_time: Some("2026-11-27T09:00:00Z".to_string()),
            body: Some(Koerper {
                content_type: Some("html".to_string()),
                content: Some(html.to_string()),
            }),
            ..TeamsNachricht::default()
        }
    }

    #[test]
    fn html_wird_zu_text() {
        assert_eq!(
            html_zu_text("<p>Hallo <b>Welt</b></p><p>Zweite Zeile</p>"),
            "Hallo Welt\nZweite Zeile"
        );
        assert_eq!(html_zu_text("eins<br>zwei<br/>drei"), "eins\nzwei\ndrei");
        assert_eq!(html_zu_text("<div>a</div><div>b</div>"), "a\nb");
        assert_eq!(
            html_zu_text("<ul><li>eins</li><li>zwei</li></ul>"),
            "eins\nzwei"
        );
        assert_eq!(
            html_zu_text("<p>Milch &amp; Brot &lt;wichtig&gt; &quot;so&quot;</p>"),
            "Milch & Brot <wichtig> \"so\""
        );
        assert_eq!(html_zu_text("a&nbsp;&nbsp;b"), "a  b");
        assert_eq!(
            html_zu_text("<at id=\"0\">Anna Muster</at> schrieb"),
            "@Anna Muster schrieb"
        );
        assert_eq!(html_zu_text("&unbekannt; bleibt"), "&unbekannt; bleibt");
        assert_eq!(html_zu_text("kein & Zeichen"), "kein & Zeichen");
    }

    #[test]
    fn leerzeilen_werden_zusammengefasst() {
        assert_eq!(
            html_zu_text("<p>a</p><br><br><br><br><p>b</p>"),
            "a\n\n\nb",
            "mehr als zwei Leerzeilen werden zwei"
        );
        assert_eq!(html_zu_text("\n\n<p>a</p>"), "a", "kein Rand");
    }

    #[test]
    fn text_wird_an_der_wortgrenze_gekuerzt() {
        let lang = "eins zwei drei vier fünf";
        assert_eq!(
            text_kuerzen(lang, 16),
            ("eins zwei drei".to_string(), Some(lang.chars().count()))
        );
        assert_eq!(text_kuerzen("kurz", 16), ("kurz".to_string(), None));
        assert_eq!(
            text_kuerzen(&"a".repeat(50), 10),
            ("a".repeat(10), Some(50)),
            "ein langes Wort wird hart geschnitten"
        );
    }

    #[test]
    fn system_und_geloeschte_nachrichten_fallen_weg() {
        assert!(sichtbar(&nachricht(None, "Hallo")));
        assert!(!sichtbar(&nachricht(
            Some("systemEventMessage"),
            "Beigetreten"
        )));
        let mut geloescht = nachricht(None, "weg");
        geloescht.deleted_date_time = Some("2026-11-27T10:00:00Z".to_string());
        assert!(!sichtbar(&geloescht));
        assert!(nachricht_json(&geloescht, &Zeitzone::neu("Europe/Berlin"), 2000).is_none());
    }

    #[test]
    fn nachricht_json_traegt_zeit_absender_und_anhang() {
        let zone = Zeitzone::neu("Europe/Berlin");
        let mut n = nachricht(None, "<p>Hallo <b>Welt</b></p>");
        n.from = Some(m365_graph::Absender {
            user: Some(m365_graph::Identitaet {
                id: Some("u1".to_string()),
                display_name: Some("Anna Muster".to_string()),
            }),
            application: None,
        });
        n.attachments = Some(vec![m365_graph::Anhang {
            id: Some("a1".to_string()),
            content_type: Some("reference".to_string()),
            name: Some("Bericht.pdf".to_string()),
            content_url: None,
        }]);
        n.web_url = Some("https://teams.example/n1".to_string());

        let wert = nachricht_json(&n, &zone, 2000).expect("sichtbar");
        assert_eq!(wert["id"], "n1");
        assert_eq!(wert["von"], "Anna Muster");
        assert_eq!(wert["zeit"], "2026-11-27T10:00:00+01:00");
        assert_eq!(wert["text"], "Hallo Welt");
        assert_eq!(wert["anhaenge"], json!(["Bericht.pdf"]));
        assert_eq!(wert["link"], "https://teams.example/n1");
        assert!(wert.get("gekuerzt").is_none());
    }

    #[test]
    fn nachricht_json_sagt_wenn_gekuerzt_wurde() {
        let zone = Zeitzone::neu("Europe/Berlin");
        let lang = "Wort ".repeat(100);
        let n = nachricht(None, &lang);
        let wert = nachricht_json(&n, &zone, 20).expect("sichtbar");
        assert_eq!(wert["gekuerzt"], true);
        assert!(wert["laenge_gesamt"].as_u64().unwrap_or(0) > 400);
    }

    #[test]
    fn chat_titel_nimmt_thema_sonst_die_anderen_mitglieder() {
        let mut chat = Chat {
            id: "19:chat@thread.v2".to_string(),
            topic: Some("Projekt Nord".to_string()),
            ..Chat::default()
        };
        assert_eq!(chat_titel(&chat, Some("Anna Muster")), "Projekt Nord");

        chat.topic = None;
        chat.members = Some(vec![
            m365_graph::Mitglied {
                display_name: Some("Anna Muster".to_string()),
                ..Mitglied::default()
            },
            m365_graph::Mitglied {
                display_name: Some("Bert Beispiel".to_string()),
                ..Mitglied::default()
            },
        ]);
        assert_eq!(chat_titel(&chat, Some("Anna Muster")), "Bert Beispiel");
        chat.members = None;
        assert_eq!(chat_titel(&chat, None), "19:chat@thread.v2");
    }

    #[test]
    fn suchtreffer_markierungen_werden_entfernt() {
        assert_eq!(
            hervorhebung_entfernen("Ein <c0>Treffer</c0> und <c1>noch</c1> einer"),
            "Ein Treffer und noch einer"
        );
        assert_eq!(hervorhebung_entfernen("ohne Markierung"), "ohne Markierung");
        assert_eq!(hervorhebung_entfernen("kaputt <c hier"), "kaputt <c hier");
    }

    #[test]
    fn zeiten_werden_verglichen() {
        let jetzt = "2026-11-30T00:00:00Z"
            .parse::<Timestamp>()
            .expect("Zeitpunkt");
        assert!(innerhalb(Some("2026-11-29T00:00:00Z"), 3, jetzt));
        assert!(!innerhalb(Some("2020-01-01T00:00:00Z"), 3, jetzt));
        assert!(!innerhalb(Some("gestern"), 3, jetzt));
        assert!(!innerhalb(None, 3, jetzt));

        assert!(ist_neuer(
            Some("2026-11-27T10:00:00Z"),
            Some("2026-11-27T09:00:00Z")
        ));
        assert!(!ist_neuer(
            Some("2026-11-27T09:00:00Z"),
            Some("2026-11-27T10:00:00Z")
        ));
        assert!(!ist_neuer(None, Some("2026-11-27T10:00:00Z")));
    }
}
