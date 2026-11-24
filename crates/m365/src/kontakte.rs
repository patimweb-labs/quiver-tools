// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Kontakte und Personen für die Antworten (docs/aufgaben/06-graph-erweiterung.md).
//! Reine Funktionen ohne Netz – damit prüfbar, ohne einen Tenant zu brauchen.

use m365_graph::{Kontakt, MailAdresse, Person, Telefon};
use serde_json::{json, Value};

/// „Name \<mail\>“ bzw. nur eins von beidem.
pub fn mail_text(adresse: &MailAdresse) -> Option<String> {
    match (
        adresse.name.as_deref().map(str::trim),
        adresse.address.as_deref().map(str::trim),
    ) {
        (Some(name), Some(mail)) if !name.is_empty() && !mail.is_empty() => {
            Some(format!("{name} <{mail}>"))
        }
        (Some(name), _) if !name.is_empty() => Some(name.to_string()),
        (_, Some(mail)) if !mail.is_empty() => Some(mail.to_string()),
        _ => None,
    }
}

/// Alle Mail-Adressen, leere fallen weg.
pub fn mail_texte(adressen: Option<&[MailAdresse]>) -> Vec<String> {
    adressen
        .unwrap_or_default()
        .iter()
        .filter_map(mail_text)
        .collect()
}

/// Telefonnummern mit Art, z. B. `+49 30 1234 (business)`.
pub fn telefon_texte(telefone: Option<&[Telefon]>) -> Vec<String> {
    telefone
        .unwrap_or_default()
        .iter()
        .filter_map(|telefon| {
            let nummer = telefon.number.as_deref()?.trim();
            if nummer.is_empty() {
                return None;
            }
            Some(match telefon.art.as_deref().map(str::trim) {
                Some(art) if !art.is_empty() => format!("{nummer} ({art})"),
                _ => nummer.to_string(),
            })
        })
        .collect()
}

/// Ein Kontakt als knappes JSON.
pub fn kontakt_json(kontakt: &Kontakt) -> Value {
    let telefone: Vec<String> = kontakt
        .business_phones
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|nummer| nummer.trim().to_string())
        .filter(|nummer| !nummer.is_empty())
        .collect();
    let mut wert = json!({
        "id": kontakt.id,
        "name": kontakt.display_name,
        "vorname": kontakt.given_name,
        "nachname": kontakt.surname,
        "firma": kontakt.company_name,
        "position": kontakt.job_title,
        "mails": mail_texte(kontakt.email_addresses.as_deref()),
    });
    if !telefone.is_empty() {
        wert["telefone"] = json!(telefone);
    }
    if let Some(mobil) = kontakt
        .mobile_phone
        .as_deref()
        .map(str::trim)
        .filter(|mobil| !mobil.is_empty())
    {
        wert["mobil"] = json!(mobil);
    }
    wert
}

/// Eine Person als knappes JSON: Abteilung, Position, Mail, Telefon.
pub fn person_json(person: &Person) -> Value {
    json!({
        "id": person.id,
        "name": person.display_name,
        "abteilung": person.department,
        "position": person.job_title,
        "firma": person.company_name,
        "mails": mail_texte(person.scored_email_addresses.as_deref()),
        "telefone": telefon_texte(person.phones.as_deref()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mails_werden_mit_namen_ausgegeben() {
        assert_eq!(
            mail_text(&MailAdresse {
                name: Some("Anna".to_string()),
                address: Some("anna@firma.de".to_string()),
            })
            .as_deref(),
            Some("Anna <anna@firma.de>")
        );
        assert_eq!(
            mail_text(&MailAdresse {
                name: None,
                address: Some("anna@firma.de".to_string()),
            })
            .as_deref(),
            Some("anna@firma.de")
        );
        assert_eq!(
            mail_text(&MailAdresse {
                name: Some("Anna".to_string()),
                address: None,
            })
            .as_deref(),
            Some("Anna")
        );
        assert!(mail_text(&MailAdresse::default()).is_none());
        assert!(mail_texte(None).is_empty());
    }

    #[test]
    fn telefone_tragen_ihre_art() {
        let telefone = vec![
            Telefon {
                number: Some("+49 30 1234".to_string()),
                art: Some("business".to_string()),
            },
            Telefon {
                number: Some("  ".to_string()),
                art: None,
            },
            Telefon {
                number: Some("+49 170 1".to_string()),
                art: None,
            },
        ];
        assert_eq!(
            telefon_texte(Some(&telefone)),
            vec!["+49 30 1234 (business)", "+49 170 1"]
        );
    }

    #[test]
    fn kontakt_json_laesst_leere_felder_weg() {
        let kontakt = Kontakt {
            id: "k1".to_string(),
            display_name: Some("Bert Beispiel".to_string()),
            email_addresses: Some(vec![MailAdresse {
                name: Some("Bert".to_string()),
                address: Some("bert@firma.de".to_string()),
            }]),
            mobile_phone: Some("+49 170 2".to_string()),
            ..Kontakt::default()
        };
        let wert = kontakt_json(&kontakt);
        assert_eq!(wert["name"], "Bert Beispiel");
        assert_eq!(wert["mails"], json!(["Bert <bert@firma.de>"]));
        assert_eq!(wert["mobil"], "+49 170 2");
        assert!(wert.get("telefone").is_none());
    }

    #[test]
    fn person_json_traegt_abteilung_und_mail() {
        let person = Person {
            id: Some("u-1".to_string()),
            display_name: Some("Clara Claudia".to_string()),
            department: Some("Vertrieb".to_string()),
            scored_email_addresses: Some(vec![MailAdresse {
                name: None,
                address: Some("clara@firma.de".to_string()),
            }]),
            ..Person::default()
        };
        let wert = person_json(&person);
        assert_eq!(wert["abteilung"], "Vertrieb");
        assert_eq!(wert["mails"], json!(["clara@firma.de"]));
        assert_eq!(wert["telefone"], json!([]));
    }
}
