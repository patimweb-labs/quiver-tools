// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der Mandant aus dem Access-Token (`tid`), docs/m365/aufgaben/07-fehler-aus-dem-livetest.md.
//!
//! `markChatReadForUser` verlangt die **Tenant-Id** des Users im Rumpf. Sie
//! steht nicht in `/me`, sondern im Anspruch `tid` des Tokens – deshalb wird
//! hier nur der mittlere Teil des JWT (die Nutzlast) gelesen, **nicht** das
//! Token geloggt und nichts geprüft. Die Signatur hat Microsoft geprüft; wer
//! den Token-Wert fälschen kann, ist schon angemeldet.
//!
//! Ein Token ohne `tid` (z. B. ein von Hand gesetztes `M365_ACCESS_TOKEN`)
//! ergibt `None` – der Aufrufer sagt dann, was zu tun ist, statt einen
//! unverständlichen 400 von Graph weiterzureichen.

/// `tid` aus einem JWT-Access-Token, sonst `None`.
pub fn mandant_aus_token(token: &str) -> Option<String> {
    let nutzlast = token.trim().split('.').nth(1)?;
    let roh = base64url(nutzlast)?;
    let wert: serde_json::Value = serde_json::from_slice(&roh).ok()?;
    // Entra liefert `tid`; ältere Tokens tragen den langen Anspruchsnamen.
    [
        "tid",
        "http://schemas.microsoft.com/identity/claims/tenantid",
    ]
    .iter()
    .find_map(|name| wert.get(*name).and_then(|wert| wert.as_str()))
    .map(str::trim)
    .filter(|mandant| !mandant.is_empty())
    .map(str::to_string)
}

/// Base64 nach RFC 4648 §5 (URL-Variante, ohne `=`-Füllung).
fn base64url(text: &str) -> Option<Vec<u8>> {
    let mut raus = Vec::with_capacity(text.len() * 3 / 4);
    let mut puffer: u32 = 0;
    let mut bits: u32 = 0;
    for zeichen in text.trim_end_matches('=').chars() {
        let wert = match zeichen {
            'A'..='Z' => zeichen as u32 - 'A' as u32,
            'a'..='z' => zeichen as u32 - 'a' as u32 + 26,
            '0'..='9' => zeichen as u32 - '0' as u32 + 52,
            '-' | '+' => 62,
            '_' | '/' => 63,
            _ => return None,
        };
        puffer = (puffer << 6) | wert;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            raus.push((puffer >> bits) as u8);
            puffer &= (1 << bits) - 1;
        }
    }
    Some(raus)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Kopf `{"alg":"none","typ":"JWT"}`, Nutzlast `{"tid":"test-mandant"}`.
    const MIT_TID: &str = "eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0.eyJ0aWQiOiJ0ZXN0LW1hbmRhbnQifQ.";

    #[test]
    fn mandant_wird_aus_der_nutzlast_gelesen() {
        assert_eq!(mandant_aus_token(MIT_TID).as_deref(), Some("test-mandant"));
    }

    #[test]
    fn langer_anspruchsname_zaehlt_auch() {
        // {"http://schemas.microsoft.com/identity/claims/tenantid":"lang-1"}
        let nutzlast =
            base64url_mit(r#"{"http://schemas.microsoft.com/identity/claims/tenantid":"lang-1"}"#);
        assert_eq!(
            mandant_aus_token(&format!("kopf.{nutzlast}.signatur")).as_deref(),
            Some("lang-1")
        );
    }

    #[test]
    fn ohne_tid_kein_mandant() {
        assert_eq!(mandant_aus_token("at-1"), None, "kein JWT");
        assert_eq!(mandant_aus_token(""), None);
        // {"sub":"a"} ohne tid
        assert_eq!(
            mandant_aus_token("eyJhbGciOiJub25lIn0.eyJzdWIiOiJhIn0."),
            None
        );
        assert_eq!(mandant_aus_token("kopf.@@@.signatur"), None, "kein Base64");
        assert_eq!(
            mandant_aus_token("eyJhbGciOiJub25lIn0.eyJzdWIiOiJhIn0"),
            None,
            "auch ohne Signaturteil"
        );
    }

    /// Nur für den Test: Text → Base64url (damit kein Literal gepflegt werden muss).
    fn base64url_mit(text: &str) -> String {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let bytes = text.as_bytes();
        let mut raus = String::new();
        for stueck in bytes.chunks(3) {
            let mut puffer = 0u32;
            for (stelle, byte) in stueck.iter().enumerate() {
                puffer |= (*byte as u32) << (16 - 8 * stelle);
            }
            for stelle in 0..=stueck.len() {
                let sechs = (puffer >> (18 - 6 * stelle)) & 0b11_1111;
                raus.push(ALPHABET[sechs as usize] as char);
            }
        }
        raus
    }
}
