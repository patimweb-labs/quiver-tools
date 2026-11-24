// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Abnahme für `quiver-m365 freigeben` (SPEC.md §2a): die Admin-Freigabe läuft
//! gegen die Attrappe aus Aufgabe 01, nie gegen login.microsoftonline.com.

#[path = "../../m365-anmeldung/tests/attrappe.rs"]
mod attrappe;

use attrappe::{Attrappe, AttrappenBrowser};
use quiver_m365::freigeben::{freigabe_scopes, freigeben};

#[tokio::test]
async fn freigeben_wartet_auf_die_rueckleitung() {
    let attrappe = Attrappe::starten().await;
    let konfig = attrappe::konfig(&attrappe);

    let freigabe = freigeben(&konfig, "test-tenant", &AttrappenBrowser::neu())
        .await
        .expect("freigegeben");
    assert_eq!(freigabe.tenant, "test-tenant");

    let anfrage = attrappe.freigabe_anfrage().expect("adminconsent kam an");
    assert_eq!(anfrage.client_id.as_deref(), Some(attrappe::CLIENT_ID));

    let scope = anfrage.scope.expect("scope");
    assert_eq!(
        scope,
        freigabe_scopes(&konfig.scopes).join(" "),
        "die Rechte der Konfiguration, mit Graph-Präfix"
    );
    assert!(
        scope.contains("https://graph.microsoft.com/User.Read"),
        "{scope}"
    );
    assert!(
        scope.contains("https://graph.microsoft.com/Mail.Send"),
        "{scope}"
    );
    assert!(!scope.contains("offline_access"), "{scope}");

    let umleitung = anfrage.redirect_uri.expect("redirect_uri");
    assert!(
        umleitung.starts_with("http://127.0.0.1:"),
        "Rückleitung auf 127.0.0.1: {umleitung}"
    );

    let state = anfrage.state.expect("state");
    assert_eq!(state.len(), 64, "state aus 32 zufälligen Bytes");
}

#[tokio::test]
async fn ablehnung_des_admins_wird_gemeldet() {
    let attrappe = Attrappe::starten().await;
    attrappe.freigabe_ablehnen();
    let konfig = attrappe::konfig(&attrappe);

    let fehler = freigeben(&konfig, "test-tenant", &AttrappenBrowser::neu())
        .await
        .expect_err("muss scheitern");
    let text = format!("{fehler}");
    assert!(text.contains("abgelehnt"), "{text}");
}

#[tokio::test]
async fn ohne_bestimmten_tenant_kein_freigeben() {
    let attrappe = Attrappe::starten().await;
    let konfig = attrappe::konfig(&attrappe);

    for unbrauchbar in ["", "   ", "organizations"] {
        let fehler = freigeben(&konfig, unbrauchbar, &AttrappenBrowser::neu())
            .await
            .expect_err("muss scheitern");
        assert!(
            format!("{fehler}").contains("--tenant"),
            "Hinweis auf --tenant: {fehler}"
        );
    }
    assert!(
        attrappe.freigabe_anfrage().is_none(),
        "keine Anfrage an den Anmeldedienst"
    );
}
