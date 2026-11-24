// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! PKCE (RFC 7636, `S256`) und der `state`-Wert für den Authorize-Ablauf.
//!
//! Der Verifier ist ein Zufallswert, die Challenge sein SHA-256 in
//! Base64url ohne Füllzeichen. Beides gehört nicht in ein Log; `Debug` zeigt
//! darum nur, dass es Werte gibt.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::RngCore;
use sha2::{Digest, Sha256};

/// Ein PKCE-Paar: der Verifier bleibt im Prozess, die Challenge geht an den
/// Authorize-Endpunkt.
#[derive(Clone, PartialEq, Eq)]
pub struct Pkce {
    verifier: String,
    challenge: String,
}

impl std::fmt::Debug for Pkce {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pkce")
            .field("verifier", &"<geheim>")
            .field("challenge", &self.challenge)
            .finish()
    }
}

impl Pkce {
    /// Ein frisches Paar aus 32 zufälligen Bytes.
    pub fn neu() -> Pkce {
        let verifier = base64url(&zufall(32));
        let challenge = challenge(&verifier);
        Pkce {
            verifier,
            challenge,
        }
    }

    /// Der Wert für `code_verifier` beim Token-Tausch.
    pub fn verifier(&self) -> &str {
        &self.verifier
    }

    /// Der Wert für `code_challenge` beim Authorize-Aufruf.
    pub fn challenge(&self) -> &str {
        &self.challenge
    }
}

/// `BASE64URL(SHA256(verifier))` ohne Füllzeichen (RFC 7636 §4.2).
pub fn challenge(verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    base64url(&hasher.finalize())
}

/// Ein `state`-Wert (CSRF-Schutz): 32 zufällige Bytes als Hex.
pub fn state() -> String {
    hex(&zufall(32))
}

fn base64url(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(text, "{byte:02x}");
    }
    text
}

fn zufall(anzahl: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; anzahl];
    rand::rng().fill_bytes(&mut bytes);
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_ist_die_beispielrechnung_aus_der_rfc() {
        // RFC 7636, Anhang B: Verifier und erwartete Challenge.
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn verifier_und_state_sind_lang_und_zufaellig() {
        let a = Pkce::neu();
        let b = Pkce::neu();
        assert_ne!(a.verifier(), b.verifier());
        assert_ne!(a.challenge(), b.challenge());
        // RFC 7636 §4.1: 43 bis 128 Zeichen aus [A-Za-z0-9-._~].
        assert!(a.verifier().len() >= 43 && a.verifier().len() <= 128);
        assert!(a
            .verifier()
            .chars()
            .all(|zeichen| zeichen.is_ascii_alphanumeric() || "-._~".contains(zeichen)));
        assert_eq!(a.challenge(), challenge(a.verifier()));

        let state = state();
        assert_eq!(state.len(), 64);
        assert!(state.chars().all(|zeichen| zeichen.is_ascii_hexdigit()));
        assert_ne!(state, super::state());
    }

    #[test]
    fn debug_zeigt_den_verifier_nicht() {
        let pkce = Pkce::neu();
        let text = format!("{pkce:?}");
        assert!(!text.contains(pkce.verifier()));
        assert!(text.contains(pkce.challenge()));
    }
}
