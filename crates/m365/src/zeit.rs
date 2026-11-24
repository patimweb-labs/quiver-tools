// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Zeiten für die Tools (SPEC.md §4): rein und raus, in **einer** Zone.
//!
//! * **Eingabe** – `2026-11-27T10:00` (Ortszeit in [`Zeitzone`], so wie heute
//!   schon) oder mit Versatz (`…+01:00`, `…Z`) → die Form, die Graph für
//!   `startDateTime`/`endDateTime` und `start.dateTime` versteht: Ortszeit in
//!   der Zone des Servers, den Versatz trägt das Feld `timeZone`.
//! * **Ausgabe** – was Graph liefert (`calendarView`: Ortszeit **und**
//!   `timeZone`; Mails und Dateien: UTC mit `Z`) → RFC 3339 **mit Versatz** in
//!   der Zielzone, ohne Sekundenbruchteile: `2026-11-27T10:00:00+01:00`.
//! * **Ganztägige Termine** kennen keine Uhrzeit: [`Zeitzone::datum`] macht aus
//!   dem Zeitpunkt nur `2026-11-24`.
//!
//! Die Umrechnung mit Sommerzeit braucht eine Zeitzonendatenbank. Hier steht
//! `jiff` (`jiff = "0.2"`) statt `time` + `time-tz`: **eine** Abhängigkeit statt
//! zweier, und die Datenbank reist mit (kein Blick in `/usr/share/zoneinfo`).
//! `jiff` prüft außerdem, ob es den Tag wirklich gibt (kein 30. Februar).

use jiff::civil::{Date, DateTime, Time};
use jiff::tz::{Offset, TimeZone};
use jiff::{Timestamp, Zoned};

const ERWARTET: &str = "erwartet z. B. 2026-11-27T10:00";
/// Zeitpunkt in einer Antwort: RFC 3339 mit Versatz, ohne Sekundenbruchteile.
const MIT_VERSATZ: &str = "%Y-%m-%dT%H:%M:%S%:z";
/// Ganztägiger Termin in einer Antwort: nur das Datum.
const NUR_DATUM: &str = "%Y-%m-%d";
/// Zeitpunkt für Graph: Ortszeit, die Zone steht daneben in `timeZone`.
const FUER_GRAPH: &str = "%Y-%m-%dT%H:%M:%S";

/// Die Zeitzone, in der Termine gelesen und geschrieben werden
/// (`M365_ZEITZONE`, Standard `Europe/Berlin`).
#[derive(Debug, Clone)]
pub struct Zeitzone {
    tz: TimeZone,
    name: String,
}

impl Zeitzone {
    /// Baut die Zone aus ihrem Namen. Ein unbekannter Name wird zu UTC (mit
    /// Warnung im Log): Graph lehnt ihn ohnehin ab, aber eine Antwort soll
    /// daran nicht scheitern.
    pub fn neu(name: impl Into<String>) -> Self {
        let name = name.into().trim().to_string();
        let tz = match TimeZone::get(&name) {
            Ok(tz) => tz,
            Err(fehler) => {
                tracing::warn!("Zeitzone {name:?} ist unbekannt ({fehler}) – nehme UTC");
                TimeZone::UTC
            }
        };
        Self { tz, name }
    }

    /// Der Name, wie ihn Graph bekommt (`Prefer: outlook.timezone`, `timeZone`).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Eingabe → Ortszeit in dieser Zone, so wie Graph sie erwartet.
    ///
    /// - `2026-11-27T10:00` → `2026-11-27T10:00:00` (Ortszeit, Zone sagt Graph)
    /// - `2026-11-27T10:00:00+02:00` → `2026-11-27T09:00:00` (Winterzeit in Berlin)
    pub fn eingabe(&self, roh: &str) -> Result<String, String> {
        let text = roh.trim();
        if text.is_empty() {
            return Err(format!("Zeitangabe fehlt ({ERWARTET})"));
        }
        if !text.is_ascii() {
            return Err(format!(
                "Zeitangabe {text:?} ist kein ISO 8601 ({ERWARTET})"
            ));
        }

        let (rest, versatz) = teile_versatz(text)?;
        let (datum_teil, zeit_teil) = rest.split_once('T').ok_or_else(|| {
            format!("Zeitangabe {text:?} ohne T zwischen Datum und Uhrzeit ({ERWARTET})")
        })?;
        let ortszeit = datum_lesen(datum_teil, text)?.to_datetime(zeit_lesen(zeit_teil, text)?);

        // Mit Versatz ist der Zeitpunkt eindeutig: in die Zielzone umrechnen.
        let ortszeit = match versatz {
            None => ortszeit,
            Some(sekunden) => self
                .im_versatz(ortszeit, sekunden)
                .map_err(|fehler| format!("Zeitangabe {text:?} unbrauchbar: {fehler}"))?,
        };
        Ok(ortszeit.strftime(FUER_GRAPH).to_string())
    }

    /// Graph-Zeitpunkt → `2026-11-27T10:00:00+01:00`; `None`, wenn nichts
    /// Lesbares kommt. `zone` ist die Zone aus der Antwort, falls sie eine nennt.
    pub fn zeitpunkt(&self, roh: &str, zone: Option<&str>) -> Option<String> {
        self.zoned(roh, zone)
            .map(|zoned| zoned.strftime(MIT_VERSATZ).to_string())
    }

    /// Graph-Zeitpunkt → nur das Datum: `2026-11-24` (ganztägiger Termin).
    pub fn datum(&self, roh: &str, zone: Option<&str>) -> Option<String> {
        self.zoned(roh, zone)
            .map(|zoned| zoned.strftime(NUR_DATUM).to_string())
    }

    /// Graph-Zeitpunkt in dieser Zone – der eine Weg für [`Self::zeitpunkt`]
    /// und [`Self::datum`].
    fn zoned(&self, roh: &str, zone: Option<&str>) -> Option<Zoned> {
        let text = roh.trim();
        if text.is_empty() {
            return None;
        }
        // Mit Versatz (`…Z`) ist der Zeitpunkt eindeutig: nur die Zone wechseln.
        if let Ok(zeitpunkt) = text.parse::<Timestamp>() {
            return Some(zeitpunkt.to_zoned(self.tz.clone()));
        }
        // Ohne Versatz gilt die Zone aus der Antwort. Graph gibt bei
        // `calendarView` genau die Zone zurück, die wir mit `Prefer` erfragt
        // haben; ist sie unbekannt, bleibt diese Zone als beste Näherung.
        let ortszeit = text.parse::<DateTime>().ok()?;
        let quelle = zone
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .and_then(|name| TimeZone::get(name).ok())
            .unwrap_or_else(|| self.tz.clone());
        // Der Zeitpunkt steht jetzt fest – nur die Zone ist noch die falsche.
        let zoned = quelle.to_zoned(ortszeit).ok()?;
        Some(zoned.timestamp().to_zoned(self.tz.clone()))
    }

    /// Ortszeit mit festem Versatz → Ortszeit in dieser Zone.
    fn im_versatz(&self, ortszeit: DateTime, sekunden: i32) -> Result<DateTime, jiff::Error> {
        let versatz = TimeZone::fixed(Offset::from_seconds(sekunden)?);
        Ok(versatz
            .to_zoned(ortszeit)?
            .timestamp()
            .to_zoned(self.tz.clone())
            .datetime())
    }
}

/// `…Z` oder `…+02:00` abtrennen; das Ergebnis ist der Versatz in Sekunden
/// (östlich von UTC positiv).
fn teile_versatz(text: &str) -> Result<(&str, Option<i32>), String> {
    if let Some(rest) = text.strip_suffix('Z') {
        return Ok((rest, Some(0)));
    }
    if text.len() > 6 {
        let (rest, ende) = text.split_at(text.len() - 6);
        let zeichen: Vec<char> = ende.chars().collect();
        let vorzeichen = zeichen[0];
        if (vorzeichen == '+' || vorzeichen == '-') && zeichen[3] == ':' {
            let stunden = zahl(&ende[1..3], text)?;
            let minuten = zahl(&ende[4..6], text)?;
            if stunden > 23 || minuten > 59 {
                return Err(format!(
                    "Zeitangabe {text:?} hat eine unmögliche Verschiebung"
                ));
            }
            let sekunden = (stunden * 3600 + minuten * 60) as i32;
            let sekunden = if vorzeichen == '-' {
                -sekunden
            } else {
                sekunden
            };
            return Ok((rest, Some(sekunden)));
        }
    }
    Ok((text, None))
}

fn datum_lesen(teil: &str, ganz: &str) -> Result<Date, String> {
    let teile: Vec<&str> = teil.split('-').collect();
    if teile.len() != 3 {
        return Err(format!("Zeitangabe {ganz:?} ohne Datum ({ERWARTET})"));
    }
    let jahr = zahl(teile[0], ganz)? as i16;
    let monat = zahl(teile[1], ganz)? as i8;
    let tag = zahl(teile[2], ganz)? as i8;
    Date::new(jahr, monat, tag)
        .map_err(|fehler| format!("Zeitangabe {ganz:?} ist kein Datum: {fehler}"))
}

fn zeit_lesen(teil: &str, ganz: &str) -> Result<Time, String> {
    let teile: Vec<&str> = teil.split(':').collect();
    if teile.len() < 2 || teile.len() > 3 {
        return Err(format!("Zeitangabe {ganz:?} ohne Uhrzeit ({ERWARTET})"));
    }
    let stunde = zahl(teile[0], ganz)? as i8;
    let minute = zahl(teile[1], ganz)? as i8;
    let sekunde = match teile.get(2) {
        Some(sekunde) => zahl(sekunde, ganz)? as i8,
        None => 0,
    };
    Time::new(stunde, minute, sekunde, 0)
        .map_err(|fehler| format!("Zeitangabe {ganz:?} ist keine Uhrzeit: {fehler}"))
}

fn zahl(teil: &str, ganz: &str) -> Result<u32, String> {
    if teil.is_empty() || !teil.chars().all(|zeichen| zeichen.is_ascii_digit()) {
        return Err(format!("Zeitangabe {ganz:?} hat keine Zahl bei {teil:?}"));
    }
    teil.parse::<u32>()
        .map_err(|fehler| format!("Zeitangabe {ganz:?} unlesbar: {fehler}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn berlin() -> Zeitzone {
        Zeitzone::neu("Europe/Berlin")
    }

    #[test]
    fn ohne_sekunden_bekommt_sekunden() {
        assert_eq!(
            berlin().eingabe("2026-11-27T10:00").unwrap(),
            "2026-11-27T10:00:00"
        );
        assert_eq!(
            berlin().eingabe(" 2026-11-27T10:00:00 ").unwrap(),
            "2026-11-27T10:00:00"
        );
    }

    /// Eingaben mit Versatz: der Zeitpunkt steht fest, gesendet wird Ortszeit.
    #[test]
    fn eingabe_mit_versatz_wird_ortszeit() {
        // 10:00+02:00 ist 08:00 UTC, im Winter (UTC+1) also 09:00 in Berlin.
        assert_eq!(
            berlin().eingabe("2026-11-27T10:00:00+02:00").unwrap(),
            "2026-11-27T09:00:00"
        );
        assert_eq!(
            berlin().eingabe("2026-11-27T10:00:00Z").unwrap(),
            "2026-11-27T11:00:00"
        );
        assert_eq!(
            berlin().eingabe("2026-11-27T00:30:00-01:00").unwrap(),
            "2026-11-27T02:30:00"
        );
        // Im Sommer ist Berlin selbst +02:00 – die Ortszeit bleibt stehen.
        assert_eq!(
            berlin().eingabe("2026-07-15T10:00:00+02:00").unwrap(),
            "2026-07-15T10:00:00"
        );
        // UTC als Zielzone.
        assert_eq!(
            Zeitzone::neu("UTC")
                .eingabe("2026-11-27T10:00:00+01:00")
                .unwrap(),
            "2026-11-27T09:00:00"
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
            "2026-11-27T10:00:00+02:70",
            "2026-1a-27T10:00",
        ] {
            assert!(
                berlin().eingabe(kaputt).is_err(),
                "{kaputt:?} sollte scheitern"
            );
        }
    }

    /// `calendarView`: Ortszeit + `timeZone` → RFC 3339 mit Versatz.
    #[test]
    fn ortszeit_mit_zone_wird_zu_versatz() {
        let zone = berlin();
        // Winter: +01:00, Sekundenbruchteile fallen weg.
        assert_eq!(
            zone.zeitpunkt("2026-11-27T10:00:00.0000000", Some("Europe/Berlin")),
            Some("2026-11-27T10:00:00+01:00".to_string())
        );
        // Sommer: +02:00.
        assert_eq!(
            zone.zeitpunkt("2026-07-15T08:40:00.0000000", Some("Europe/Berlin")),
            Some("2026-07-15T08:40:00+02:00".to_string())
        );
        // Die Zone aus der Antwort gilt, nicht die eigene.
        assert_eq!(
            zone.zeitpunkt("2026-11-27T09:00:00.0000000", Some("UTC")),
            Some("2026-11-27T10:00:00+01:00".to_string())
        );
        // Unbekannte Zone in der Antwort: die eigene Zone bleibt die Näherung.
        assert_eq!(
            zone.zeitpunkt("2026-11-27T10:00:00.0000000", Some("Quatsch/Zone")),
            Some("2026-11-27T10:00:00+01:00".to_string())
        );
    }

    /// Mails und Dateien: Graph liefert UTC mit `Z`.
    #[test]
    fn utc_wird_in_die_zielzone_umgerechnet() {
        let zone = berlin();
        assert_eq!(
            zone.zeitpunkt("2026-11-27T09:00:00Z", None),
            Some("2026-11-27T10:00:00+01:00".to_string())
        );
        assert_eq!(
            zone.zeitpunkt("2026-07-15T06:40:00Z", None),
            Some("2026-07-15T08:40:00+02:00".to_string())
        );
        assert_eq!(
            zone.zeitpunkt("2026-11-20T08:00:00Z", None),
            Some("2026-11-20T09:00:00+01:00".to_string())
        );
    }

    #[test]
    fn ganztags_ist_nur_das_datum() {
        assert_eq!(
            berlin().datum("2026-11-24T00:00:00.0000000", Some("Europe/Berlin")),
            Some("2026-11-24".to_string())
        );
        // Ein Zeitpunkt in UTC kann in der Zielzone schon der nächste Tag sein.
        assert_eq!(
            berlin().datum("2026-11-24T23:30:00Z", None),
            Some("2026-11-25".to_string())
        );
    }

    #[test]
    fn unbekannte_zielzone_bleibt_bei_utc() {
        let zone = Zeitzone::neu("Quatsch/Zone");
        assert_eq!(
            zone.zeitpunkt("2026-11-27T09:00:00Z", None),
            Some("2026-11-27T09:00:00+00:00".to_string())
        );
    }

    #[test]
    fn unlesbare_antwort_ergibt_nichts() {
        let zone = berlin();
        assert_eq!(zone.zeitpunkt("", None), None);
        assert_eq!(zone.zeitpunkt("   ", None), None);
        assert_eq!(zone.zeitpunkt("gestern", None), None);
        assert_eq!(zone.datum("", None), None);
    }
}
