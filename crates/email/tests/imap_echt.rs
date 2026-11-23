// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Ende-zu-Ende gegen einen **echten** IMAP-Server – nur auf Anforderung.
//!
//! Der Test ist `#[ignore]`, damit `cargo test --workspace` nie ins Netz geht
//! (AGENTS.md §2.9). Er läuft nur, wenn die Umgebung gesetzt ist:
//!
//! ```text
//! EMAIL_TEST_IMAP_HOST=127.0.0.1
//! EMAIL_TEST_IMAP_PORT=3143
//! EMAIL_TEST_IMAP_SICHERHEIT=keine      # tls | starttls | keine
//! EMAIL_TEST_BENUTZER=test@example.test
//! EMAIL_TEST_PASSWORT=…                 # steht in keiner Ausgabe
//! # optional:
//! EMAIL_TEST_ZERTIFIKAT_PRUEFEN=an      # an | aus (nur lokale Brücken)
//! ```
//!
//! Start mit GreenMail im Docker (ein Testserver für IMAP/SMTP):
//!
//! ```bash
//! docker run --rm -d --name greenmail \
//!   -e GREENMAIL_OPTS="-Dgreenmail.setup.test.all -Dgreenmail.hostname=0.0.0.0 -Dgreenmail.auth.disabled" \
//!   -p 3025:3025 -p 3143:3143 -p 3465:3465 -p 3993:3993 \
//!   greenmail/standalone:latest
//!
//! EMAIL_TEST_IMAP_HOST=127.0.0.1 \
//! EMAIL_TEST_IMAP_PORT=3143 \
//! EMAIL_TEST_IMAP_SICHERHEIT=keine \
//! EMAIL_TEST_BENUTZER=test \
//! EMAIL_TEST_PASSWORT=test \
//!   cargo test -p quiver-email --test imap_echt -- --ignored --nocapture
//! ```
//!
//! Mit `-Dgreenmail.auth.disabled` nimmt GreenMail jeden Benutzer an; ohne
//! diese Zeile legt `-Dgreenmail.setup.test.all` den Benutzer `test` mit dem
//! Passwort `test` an. Der Test liest nur; er ändert nichts im Postfach.

use quiver_email::domain::{Sicherheit, Suchkriterien};
use quiver_email::{ImapKonfiguration, ImapPostfach, Postfach};

fn wert(name: &str) -> String {
    std::env::var(name)
        .unwrap_or_else(|_| panic!("{name} fehlt – siehe Kopf von tests/imap_echt.rs"))
}

fn zahl(name: &str, standard: u16) -> u16 {
    std::env::var(name)
        .ok()
        .map(|wert| {
            wert.trim()
                .parse()
                .unwrap_or_else(|_| panic!("{name} ist keine Portnummer"))
        })
        .unwrap_or(standard)
}

#[tokio::test]
#[ignore = "braucht einen echten IMAP-Server (EMAIL_TEST_IMAP_HOST, EMAIL_TEST_BENUTZER, EMAIL_TEST_PASSWORT)"]
async fn ein_echter_imap_server_antwortet() {
    let sicherheit = match std::env::var("EMAIL_TEST_IMAP_SICHERHEIT")
        .unwrap_or_else(|_| "keine".to_string())
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "tls" => Sicherheit::Tls,
        "starttls" => Sicherheit::Starttls,
        "keine" | "none" => Sicherheit::Keine,
        sonst => panic!("EMAIL_TEST_IMAP_SICHERHEIT „{sonst}“ ist unbekannt"),
    };

    let postfach = ImapPostfach::neu(ImapKonfiguration {
        host: wert("EMAIL_TEST_IMAP_HOST"),
        port: zahl("EMAIL_TEST_IMAP_PORT", 993),
        sicherheit,
        benutzer: wert("EMAIL_TEST_BENUTZER"),
        // Das Passwort wird nie ausgegeben, auch nicht in einem panic.
        passwort: wert("EMAIL_TEST_PASSWORT"),
        zertifikat_pruefen: !matches!(
            std::env::var("EMAIL_TEST_ZERTIFIKAT_PRUEFEN")
                .unwrap_or_else(|_| "an".to_string())
                .trim()
                .to_ascii_lowercase()
                .as_str(),
            "aus" | "false" | "0" | "nein"
        ),
    });

    // Anmelden und SELECT INBOX.
    postfach.pruefen().await.expect("Anmeldung fehlgeschlagen");

    let postfaecher = postfach.postfaecher().await.expect("LIST fehlgeschlagen");
    assert!(!postfaecher.is_empty(), "der Server nennt kein Postfach");
    assert!(
        postfaecher
            .iter()
            .any(|eintrag| eintrag.pfad.eq_ignore_ascii_case("INBOX")),
        "INBOX fehlt: {postfaecher:?}"
    );

    // Die letzten fünf Kopfzeilen; ein leeres Postfach ist in Ordnung.
    let seite = postfach
        .kopfzeilen("INBOX", 5, false)
        .await
        .expect("FETCH fehlgeschlagen");
    assert!(seite.kopfzeilen.len() <= 5, "{seite:?}");
    assert_eq!(seite.postfach, "INBOX");

    if let Some(kopfzeile) = seite.kopfzeilen.first() {
        let nachricht = postfach
            .nachricht("INBOX", kopfzeile.uid)
            .await
            .expect("BODY fehlgeschlagen");
        assert_eq!(nachricht.uid, kopfzeile.uid);
    }

    // Eine Suche ohne Einschränkung ist ein gültiger SEARCH-Befehl.
    let treffer = postfach
        .suchen("INBOX", &Suchkriterien::default(), 5)
        .await
        .expect("SEARCH fehlgeschlagen");
    assert!(treffer.kopfzeilen.len() <= 5, "{treffer:?}");
}
