// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Was `abap_query` und `abap_table` überhaupt schicken dürfen.
//!
//! Die Datenvorschau von SAP führt aus, was man ihr gibt. Ein als „nur lesend“
//! gekennzeichnetes Werkzeug darf darum **nur** ein einzelnes `SELECT` senden:
//!
//! - nur `SELECT` am Anfang (nach Kommentaren und Leerraum),
//! - keine Kette (`;` außerhalb einer Zeichenkette),
//! - kein `INTO` und kein `FOR UPDATE`,
//! - Kommentare und Zeichenketten werden beim Prüfen **überlesen**, nicht
//!   mitgezählt: ein Semikolon in `'a;b'` ist harmlos, eines dahinter nicht.
//!
//! Für den Filter von `abap_table` gilt dasselbe ohne `SELECT`: kein `;`, kein
//! Kommentarzeichen (es steht hinter einem `WHERE`, das das Werkzeug selbst
//! baut).
//!
//! Die Prüfung ersetzt keine Rechte auf dem System – sie verhindert nur, dass
//! ein lesendes Werkzeug versehentlich veränderndes SQL weiterreicht.

/// Was an einem SQL-Text nicht erlaubt ist. Der Text ist eine Meldung an den
/// Agenten, ohne den geprüften Inhalt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SqlFehler {
    #[error("Es ist nur ein einzelnes SELECT erlaubt – der Text beginnt nicht mit SELECT.")]
    KeinSelect,
    #[error("Mehrere Anweisungen sind nicht erlaubt – das Semikolon trennt Befehle.")]
    Kette,
    #[error("`INTO` ist in einer Abfrage nicht erlaubt.")]
    Into,
    #[error("`FOR UPDATE` ist in einer Abfrage nicht erlaubt.")]
    ForUpdate,
    #[error("Kommentarzeichen sind hier nicht erlaubt.")]
    Kommentar,
    #[error("Eine Zeichenkette oder ein Kommentar ist nicht geschlossen.")]
    Offen,
}

/// Überliest Zeichenketten und Kommentare.
///
/// Liefert den Text ohne Kommentare (Zeichenketten durch `'…'` ersetzt) und ob
/// überhaupt ein Kommentar darin stand – der Filter von `abap_table` erlaubt
/// keinen, die Abfrage vor dem `SELECT` schon.
fn ueberlesen(sql: &str) -> Result<(String, bool), SqlFehler> {
    let zeichen: Vec<char> = sql.chars().collect();
    let mut raus = String::with_capacity(sql.len());
    let mut kommentar = false;
    let mut i = 0;
    while i < zeichen.len() {
        let c = zeichen[i];
        match c {
            // Zeichenkette: `'...'`, verdoppelte Anführungszeichen sind ein `'`.
            '\'' => {
                raus.push('\'');
                i += 1;
                let mut geschlossen = false;
                while i < zeichen.len() {
                    if zeichen[i] == '\'' {
                        if zeichen.get(i + 1) == Some(&'\'') {
                            i += 2;
                            continue;
                        }
                        i += 1;
                        geschlossen = true;
                        break;
                    }
                    i += 1;
                }
                if !geschlossen {
                    return Err(SqlFehler::Offen);
                }
                raus.push('\'');
            }
            // Zeilenkommentar `--` oder `"`.
            '-' if zeichen.get(i + 1) == Some(&'-') => {
                kommentar = true;
                while i < zeichen.len() && zeichen[i] != '\n' {
                    i += 1;
                }
            }
            '"' => {
                kommentar = true;
                while i < zeichen.len() && zeichen[i] != '\n' {
                    i += 1;
                }
            }
            // Blockkommentar `/* ... */`.
            '/' if zeichen.get(i + 1) == Some(&'*') => {
                kommentar = true;
                i += 2;
                let mut geschlossen = false;
                while i + 1 < zeichen.len() {
                    if zeichen[i] == '*' && zeichen[i + 1] == '/' {
                        i += 2;
                        geschlossen = true;
                        break;
                    }
                    i += 1;
                }
                if !geschlossen {
                    return Err(SqlFehler::Offen);
                }
            }
            _ => {
                raus.push(c);
                i += 1;
            }
        }
    }
    Ok((raus, kommentar))
}

/// Enthält `text` das Wort `wort` (Groß-/Kleinschreibung egal, Wortgrenzen)?
fn enthaelt_wort(text: &str, wort: &str) -> bool {
    let klein = text.to_ascii_lowercase();
    let mut ab = 0;
    while let Some(stelle) = klein[ab..].find(wort) {
        let anfang = ab + stelle;
        let ende = anfang + wort.len();
        let davor = klein[..anfang].chars().next_back();
        let danach = klein[ende..].chars().next();
        let grenze = |zeichen: Option<char>| {
            zeichen.is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_'))
        };
        if grenze(davor) && grenze(danach) {
            return true;
        }
        ab = ende;
    }
    false
}

/// Prüft ein `abap_query`-SQL: genau ein `SELECT`, nichts Veränderndes.
pub fn nur_select(sql: &str) -> Result<(), SqlFehler> {
    let (sauber, _kommentar) = ueberlesen(sql)?;
    let text = sauber.trim();
    if !text
        .get(..6)
        .is_some_and(|anfang| anfang.eq_ignore_ascii_case("select"))
    {
        return Err(SqlFehler::KeinSelect);
    }
    // `SELECT` muss ein eigenes Wort sein (`SELECTIVE` gibt es nicht).
    if text.len() > 6 {
        let naechstes = text[6..].chars().next();
        if naechstes.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(SqlFehler::KeinSelect);
        }
    }
    if text.contains(';') {
        return Err(SqlFehler::Kette);
    }
    if enthaelt_wort(text, "into") {
        return Err(SqlFehler::Into);
    }
    if enthaelt_wort(text, "for update") {
        return Err(SqlFehler::ForUpdate);
    }
    Ok(())
}

/// Prüft den Filter von `abap_table`: kein `;`, kein Kommentarzeichen.
pub fn filter(filter: &str) -> Result<(), SqlFehler> {
    let (sauber, kommentar) = ueberlesen(filter)?;
    if kommentar {
        return Err(SqlFehler::Kommentar);
    }
    if sauber.contains(';') {
        return Err(SqlFehler::Kette);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ein_einfaches_select_geht_durch() {
        assert_eq!(nur_select("SELECT * FROM sflight"), Ok(()));
        assert_eq!(nur_select("  select carrid from sflight  "), Ok(()));
        assert_eq!(nur_select("SELECT a FROM t WHERE b = 'x'"), Ok(()));
        // Ein Semikolon in einer Zeichenkette ist harmlos.
        assert_eq!(nur_select("SELECT a FROM t WHERE b = 'a;b'"), Ok(()));
        // Ein Kommentar vor dem SELECT wird überlesen.
        assert_eq!(nur_select("-- nur lesen\nSELECT a FROM t"), Ok(()));
        assert_eq!(nur_select("/* probe */ SELECT a FROM t"), Ok(()));
    }

    #[test]
    fn nur_select_und_nichts_anderes() {
        assert_eq!(
            nur_select("DELETE FROM sflight"),
            Err(SqlFehler::KeinSelect)
        );
        assert_eq!(
            nur_select("UPDATE sflight SET x = 1"),
            Err(SqlFehler::KeinSelect)
        );
        assert_eq!(nur_select("SELECTIVE"), Err(SqlFehler::KeinSelect));
        assert_eq!(nur_select(""), Err(SqlFehler::KeinSelect));
    }

    #[test]
    fn ketten_into_und_for_update_werden_abgelehnt() {
        assert_eq!(
            nur_select("SELECT a FROM t; DELETE FROM t"),
            Err(SqlFehler::Kette)
        );
        assert_eq!(
            nur_select("SELECT a INTO TABLE @data(x) FROM t"),
            Err(SqlFehler::Into)
        );
        assert_eq!(
            nur_select("SELECT a FROM t FOR UPDATE"),
            Err(SqlFehler::ForUpdate)
        );
        // Ein `INTO` in einer Zeichenkette zählt nicht.
        assert_eq!(nur_select("SELECT a FROM t WHERE b = 'into'"), Ok(()));
        // Ein Wortanfang allein auch nicht.
        assert_eq!(nur_select("SELECT into_x FROM t"), Ok(()));
    }

    #[test]
    fn offene_zeichenketten_und_kommentare_sind_fehler() {
        assert_eq!(
            nur_select("SELECT a FROM t WHERE b = 'x"),
            Err(SqlFehler::Offen)
        );
        assert_eq!(nur_select("/* SELECT a FROM t"), Err(SqlFehler::Offen));
    }

    #[test]
    fn der_filter_kennt_kein_semikolon_und_keine_kommentare() {
        assert_eq!(filter("carrid = 'LH'"), Ok(()));
        assert_eq!(filter("carrid = 'a;b'"), Ok(()));
        assert_eq!(filter("carrid = 'LH'; DROP TABLE t"), Err(SqlFehler::Kette));
        assert_eq!(filter("1 = 1 -- immer wahr"), Err(SqlFehler::Kommentar));
        assert_eq!(filter("1 = 1 /* x */"), Err(SqlFehler::Kommentar));
        assert_eq!(filter("b = 'x"), Err(SqlFehler::Offen));
    }
}
