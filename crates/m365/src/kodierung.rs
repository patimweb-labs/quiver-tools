// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Prozent-Kodierung für Adressen und Pfade – bewusst klein, damit kein
//! weiteres Crate nötig wird.

/// Kodiert alles außer `A–Z a–z 0–9 - . _ ~` (RFC 3986 „unreserved“).
pub fn kodieren(text: &str) -> String {
    use std::fmt::Write as _;

    let mut raus = String::with_capacity(text.len());
    for byte in text.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                raus.push(*byte as char)
            }
            anderes => {
                let _ = write!(raus, "%{anderes:02X}");
            }
        }
    }
    raus
}

/// Gegenstück zu [`kodieren`]; `+` bleibt `+` (Tenants und Ids enthalten es nicht).
pub fn entkodieren(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut raus = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hoch), Some(tief)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                raus.push(hoch * 16 + tief);
                i += 3;
                continue;
            }
        }
        raus.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&raus).to_string()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kodieren_und_entkodieren_sind_gegenstuecke() {
        let roh = "https://graph.microsoft.com/User.Read Mail.Send+ö";
        assert_eq!(entkodieren(&kodieren(roh)), roh);
        assert_eq!(kodieren("a b"), "a%20b");
        assert_eq!(kodieren("q='x'"), "q%3D%27x%27");
        assert_eq!(kodieren("AA-_."), "AA-_.");
    }

    #[test]
    fn unfug_bleibt_stehen() {
        assert_eq!(entkodieren("%zz"), "%zz");
        assert_eq!(entkodieren("100%"), "100%");
    }
}
