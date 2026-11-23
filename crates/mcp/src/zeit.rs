// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Zeiten für die Tools (SPEC.md §4): Eingabe `2026-11-27T10:00`, Ausgabe in der
//! Form, die Graph für `startDateTime`/`endDateTime` versteht.
//!
//! Die Zeitzone selbst kommt aus der Umgebung (`M365_ZEITZONE`); hier steht keine
//! Zeitzonen-Datenbank, nur Umrechnung einer mitgegebenen Verschiebung.
//! `time` prüft dabei, ob es den Tag wirklich gibt (kein 30. Februar).

use time::{Date, Month, OffsetDateTime, Time, UtcOffset};

const ERWARTET: &str = "erwartet z. B. 2026-11-27T10:00";

/// Wandelt eine ISO-8601-Angabe in die Form für Graph um.
///
/// - `2026-11-27T10:00` → `2026-11-27T10:00:00` (Ortszeit, Zeitzone sagt Graph)
/// - `2026-11-27T10:00:00+02:00` → `2026-11-27T08:00:00Z` (in UTC)
pub fn iso_zeit(roh: &str) -> Result<String, String> {
    let text = roh.trim();
    if text.is_empty() {
        return Err(format!("Zeitangabe fehlt ({ERWARTET})"));
    }
    if !text.is_ascii() {
        return Err(format!(
            "Zeitangabe {text:?} ist kein ISO 8601 ({ERWARTET})"
        ));
    }

    let (rest, verschiebung) = teile_verschiebung(text)?;
    let (datum_teil, zeit_teil) = rest.split_once('T').ok_or_else(|| {
        format!("Zeitangabe {text:?} ohne T zwischen Datum und Uhrzeit ({ERWARTET})")
    })?;
    let datum = datum_lesen(datum_teil, text)?;
    let zeit = zeit_lesen(zeit_teil, text)?;

    match verschiebung {
        // Mit Verschiebung ist der Zeitpunkt eindeutig: in UTC ausgeben.
        Some(verschiebung) => {
            let utc =
                OffsetDateTime::new_in_offset(datum, zeit, verschiebung).to_offset(UtcOffset::UTC);
            Ok(format!("{}Z", als_text(utc.date(), utc.time())))
        }
        None => Ok(als_text(datum, zeit)),
    }
}

/// `…Z` oder `…+02:00` abtrennen.
fn teile_verschiebung(text: &str) -> Result<(&str, Option<UtcOffset>), String> {
    if let Some(rest) = text.strip_suffix('Z') {
        return Ok((rest, Some(UtcOffset::UTC)));
    }
    if text.len() > 6 {
        let (rest, ende) = text.split_at(text.len() - 6);
        let zeichen: Vec<char> = ende.chars().collect();
        let vorzeichen = zeichen[0];
        if (vorzeichen == '+' || vorzeichen == '-') && zeichen[3] == ':' {
            let stunden = zahl(&ende[1..3], text)?;
            let minuten = zahl(&ende[4..6], text)?;
            let stunden = if vorzeichen == '-' {
                -(stunden as i8)
            } else {
                stunden as i8
            };
            let verschiebung = UtcOffset::from_hms(stunden, minuten as i8, 0)
                .map_err(|_| format!("Zeitangabe {text:?} hat eine unmögliche Verschiebung"))?;
            return Ok((rest, Some(verschiebung)));
        }
    }
    Ok((text, None))
}

fn datum_lesen(teil: &str, ganz: &str) -> Result<Date, String> {
    let teile: Vec<&str> = teil.split('-').collect();
    if teile.len() != 3 {
        return Err(format!("Zeitangabe {ganz:?} ohne Datum ({ERWARTET})"));
    }
    let jahr = zahl(teile[0], ganz)? as i32;
    let monat = zahl(teile[1], ganz)? as u8;
    let tag = zahl(teile[2], ganz)? as u8;
    let monat = Month::try_from(monat)
        .map_err(|_| format!("Zeitangabe {ganz:?} hat keinen Monat {monat}"))?;
    Date::from_calendar_date(jahr, monat, tag)
        .map_err(|fehler| format!("Zeitangabe {ganz:?} ist kein Datum: {fehler}"))
}

fn zeit_lesen(teil: &str, ganz: &str) -> Result<Time, String> {
    let teile: Vec<&str> = teil.split(':').collect();
    if teile.len() < 2 || teile.len() > 3 {
        return Err(format!("Zeitangabe {ganz:?} ohne Uhrzeit ({ERWARTET})"));
    }
    let stunde = zahl(teile[0], ganz)? as u8;
    let minute = zahl(teile[1], ganz)? as u8;
    let sekunde = match teile.get(2) {
        Some(sekunde) => zahl(sekunde, ganz)? as u8,
        None => 0,
    };
    Time::from_hms(stunde, minute, sekunde)
        .map_err(|fehler| format!("Zeitangabe {ganz:?} ist keine Uhrzeit: {fehler}"))
}

fn zahl(teil: &str, ganz: &str) -> Result<u32, String> {
    if teil.is_empty() || !teil.chars().all(|zeichen| zeichen.is_ascii_digit()) {
        return Err(format!("Zeitangabe {ganz:?} hat keine Zahl bei {teil:?}"));
    }
    teil.parse::<u32>()
        .map_err(|fehler| format!("Zeitangabe {ganz:?} unlesbar: {fehler}"))
}

/// `2026-11-27T10:00:00` – die Form, die Graph versteht.
fn als_text(datum: Date, zeit: Time) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        datum.year(),
        u8::from(datum.month()),
        datum.day(),
        zeit.hour(),
        zeit.minute(),
        zeit.second()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ohne_sekunden_bekommt_sekunden() {
        assert_eq!(iso_zeit("2026-11-27T10:00").unwrap(), "2026-11-27T10:00:00");
        assert_eq!(
            iso_zeit(" 2026-11-27T10:00:00 ").unwrap(),
            "2026-11-27T10:00:00"
        );
    }

    #[test]
    fn mit_verschiebung_wird_utc() {
        assert_eq!(
            iso_zeit("2026-11-27T10:00:00+02:00").unwrap(),
            "2026-11-27T08:00:00Z"
        );
        assert_eq!(
            iso_zeit("2026-11-27T10:00:00Z").unwrap(),
            "2026-11-27T10:00:00Z"
        );
        assert_eq!(
            iso_zeit("2026-11-27T00:30:00-01:00").unwrap(),
            "2026-11-27T01:30:00Z"
        );
    }

    #[test]
    fn unsinn_wird_abgelehnt() {
        for kaputt in [
            "",
            "   ",
            "morgen",
            "27.11.2026 10:00",
            "2026-11-27",
            "2026-11-27T10",
            "2026-02-30T10:00",
            "2026-13-01T10:00",
            "2026-11-27T25:00",
            "2026-11-27T10:00:00+99:00",
            "2026-1a-27T10:00",
        ] {
            assert!(iso_zeit(kaputt).is_err(), "{kaputt:?} sollte scheitern");
        }
    }
}
