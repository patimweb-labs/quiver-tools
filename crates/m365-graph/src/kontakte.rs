// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Kontakte (`/me/contacts`) und Personen (`/me/people`), Aufgabe 06.
//!
//! Personen sind das, was Microsoft über Kollegen weiß: Abteilung, Telefon,
//! Mail – gespeist aus Mail, Chats und Adressbuch. Kontakte sind die Einträge,
//! die der User selbst pflegt.

use serde::{Deserialize, Serialize};

/// `{"address": …, "name": …}` in `emailAddresses` bzw. `scoredEmailAddresses`.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MailAdresse {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub address: Option<String>,
}

/// `{"number": …, "type": …}` in `phones`.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Telefon {
    #[serde(default)]
    pub number: Option<String>,
    #[serde(default, rename = "type")]
    pub art: Option<String>,
}

/// `GET /me/contacts`
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Kontakt {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub given_name: Option<String>,
    #[serde(default)]
    pub surname: Option<String>,
    #[serde(default)]
    pub company_name: Option<String>,
    #[serde(default)]
    pub job_title: Option<String>,
    #[serde(default)]
    pub email_addresses: Option<Vec<MailAdresse>>,
    #[serde(default)]
    pub business_phones: Option<Vec<String>>,
    #[serde(default)]
    pub mobile_phone: Option<String>,
}

/// `GET /me/people?$search=…`
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Person {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub given_name: Option<String>,
    #[serde(default)]
    pub surname: Option<String>,
    #[serde(default)]
    pub company_name: Option<String>,
    #[serde(default)]
    pub department: Option<String>,
    #[serde(default)]
    pub job_title: Option<String>,
    #[serde(default)]
    pub scored_email_addresses: Option<Vec<MailAdresse>>,
    #[serde(default)]
    pub phones: Option<Vec<Telefon>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kontakt_liest_mail_und_telefon() {
        let kontakt: Kontakt = serde_json::from_value(serde_json::json!({
            "id": "k1",
            "displayName": "Bert Beispiel",
            "emailAddresses": [{"name": "Bert", "address": "bert@firma.de"}],
            "mobilePhone": "+49 170 1234567",
            "businessPhones": ["+49 30 1234"],
        }))
        .expect("Modell passt");
        assert_eq!(
            kontakt
                .email_addresses
                .as_deref()
                .and_then(|liste| liste.first())
                .and_then(|mail| mail.address.as_deref()),
            Some("bert@firma.de")
        );
        assert_eq!(
            kontakt.business_phones.as_deref(),
            Some(&[String::from("+49 30 1234")][..])
        );
        assert_eq!(kontakt.mobile_phone.as_deref(), Some("+49 170 1234567"));
    }

    #[test]
    fn person_liest_abteilung_und_telefone() {
        let person: Person = serde_json::from_value(serde_json::json!({
            "id": "u-1",
            "displayName": "Clara Claudia",
            "department": "Vertrieb",
            "jobTitle": "Key Account",
            "scoredEmailAddresses": [{"address": "clara@firma.de"}],
            "phones": [{"number": "+49 30 1", "type": "business"}],
        }))
        .expect("Modell passt");
        assert_eq!(person.department.as_deref(), Some("Vertrieb"));
        assert_eq!(
            person
                .phones
                .as_deref()
                .and_then(|liste| liste.first())
                .and_then(|telefon| telefon.art.as_deref()),
            Some("business")
        );
    }
}
