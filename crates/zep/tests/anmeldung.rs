// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Anmeldung gegen die ZEP-Attrappe (Aufgabe 06, Punkte 1–6).
//!
//! Alle Fälle laufen gegen `tests/attrappe`: nie gegen zep-online.de, nie
//! gegen die echte Tresor-Datei (es ist immer ein [`SpeicherTresor`]), nie
//! gegen einen echten Browser (der Test-Browser spricht selbst HTTP).

mod attrappe;

use std::sync::Arc;
use std::time::Duration;

use quiver_zep::anmeldung::{Anmeldung, Status};
use quiver_zep::tresor::{schluessel_refresh, SpeicherTresor, Tresor};
use quiver_zep::vermittler::Vermittler;
use rmcp::model::CallToolRequestParams;

use attrappe::{Attrappe, AttrappenBrowser, BENUTZER, GEHEIM_MARKE, PASSWORT};

/// Der Takt der Hintergrund-Erneuerung in den Tests.
const TAKT: Duration = Duration::from_millis(50);

fn baue(
    attrappe: &Attrappe,
    ordner: &tempfile::TempDir,
    tresor: Arc<SpeicherTresor>,
    browser: Arc<AttrappenBrowser>,
) -> Anmeldung {
    Anmeldung::neu(
        attrappe.konfig(ordner.path().to_path_buf(), TAKT),
        tresor,
        browser,
    )
}

fn speicher() -> Arc<SpeicherTresor> {
    Arc::new(SpeicherTresor::neu())
}

fn browser() -> Arc<AttrappenBrowser> {
    Arc::new(AttrappenBrowser::neu(BENUTZER, PASSWORT))
}

#[tokio::test]
async fn erstanmeldung_laeuft_ueber_die_routine_ohne_browser() {
    let attrappe = Attrappe::starte().await;
    let ordner = tempfile::tempdir().unwrap();
    let tresor = speicher();
    let browser = browser();
    let anmeldung = baue(
        &attrappe,
        &ordner,
        Arc::clone(&tresor),
        Arc::clone(&browser),
    );

    let token = anmeldung.access_token().await.expect("Anmeldung");
    assert!(!token.is_empty());
    // Die Routine, nicht der Browser.
    assert_eq!(browser.aufrufe(), 0, "der Browser darf nicht aufgehen");
    assert_eq!(attrappe.login_posts(), 1);
    assert_eq!(attrappe.zustimmungen(), 1);
    assert_eq!(attrappe.registrierungen(), 1, "DCR genau einmal");

    // Das Refresh-Token liegt im Tresor, das Passwort nirgends.
    let refresh = tresor
        .lesen(&schluessel_refresh(&attrappe.mcp_adresse()))
        .unwrap();
    assert!(refresh.is_some_and(|wert| wert.starts_with("rt-")));
    assert_eq!(
        anmeldung.status(),
        Status::Angemeldet {
            benutzer: BENUTZER.to_string()
        }
    );
    assert!(!anmeldung.status().text().contains(PASSWORT));
    assert!(anmeldung.sperrgrund().is_none());

    // Ein zweiter Aufruf kommt aus dem Speicher: kein neuer Login.
    anmeldung
        .access_token()
        .await
        .expect("Token aus dem Speicher");
    assert_eq!(attrappe.login_posts(), 1);

    // Eine zweite Anmeldung mit demselben Tresor registriert den Client nicht neu.
    let zweite = baue(&attrappe, &ordner, Arc::clone(&tresor), browser);
    zweite.access_token().await.expect("Refresh-Token nutzen");
    assert_eq!(
        attrappe.registrierungen(),
        1,
        "DCR nur einmal je Installation"
    );
}

#[tokio::test]
async fn der_bericht_der_routine_nennt_die_schritte_und_keine_werte() {
    let attrappe = Attrappe::starte().await;
    let ordner = tempfile::tempdir().unwrap();
    let anmeldung = baue(&attrappe, &ordner, speicher(), browser());

    let lauf = anmeldung.routine_lauf().await;
    assert!(lauf.gelungen(), "{lauf:?}");
    let bericht = lauf.bericht.join("\n");

    assert!(bericht.contains("Authorization-Server"), "{bericht}");
    assert!(bericht.contains("Client-Registrierung"), "{bericht}");
    assert!(bericht.contains("Anmeldeformular absenden"), "{bericht}");
    assert!(bericht.contains("Zustimmungsseite"), "{bericht}");
    assert!(bericht.contains("Rückleitung"), "{bericht}");
    assert!(bericht.contains("angemeldet"), "{bericht}");
    // Formular- und Feldnamen ja, Werte nein.
    assert!(bericht.contains("passwort"), "{bericht}");
    assert!(!bericht.contains(PASSWORT), "{bericht}");
    assert!(!bericht.contains("rt-"), "{bericht}");
    assert!(!bericht.contains("at-"), "{bericht}");
}

#[tokio::test]
async fn der_notfall_ueber_den_browser_meldet_an() {
    let attrappe = Attrappe::starte().await;
    let ordner = tempfile::tempdir().unwrap();
    let browser = browser();
    let anmeldung = baue(&attrappe, &ordner, speicher(), Arc::clone(&browser));

    let lauf = anmeldung.browser_lauf().await;
    assert!(lauf.gelungen(), "{lauf:?}");
    assert!(lauf.bericht.iter().any(|zeile| zeile.contains("Browser")));
    assert!(browser.aufrufe() >= 1, "der Browser wurde nicht geöffnet");
    assert_eq!(attrappe.login_posts(), 1, "der Browser meldet sich an");
    assert!(matches!(anmeldung.status(), Status::Angemeldet { .. }));
    // Das Access-Token funktioniert: die Werkzeugliste kommt.
    let vermittler = Vermittler::neu(Arc::new(anmeldung.clone()));
    let werkzeuge = vermittler.werkzeuge().await.expect("Werkzeugliste");
    assert!(werkzeuge
        .iter()
        .any(|werkzeug| werkzeug.name == "zep_projekte"));
}

#[tokio::test]
async fn falsches_passwort_ergibt_genau_einen_versuch_dann_sperre() {
    let attrappe = Attrappe::starte().await;
    let ordner = tempfile::tempdir().unwrap();
    let anmeldung = baue(&attrappe, &ordner, speicher(), browser());
    attrappe.setze_passwort_richtig(false);

    let erster = anmeldung.access_token().await.unwrap_err();
    assert!(!erster.to_string().contains(PASSWORT), "{erster}");
    assert_eq!(attrappe.login_posts(), 1, "genau ein Anmeldeversuch");

    // Der zweite Aufruf versucht es nicht noch einmal.
    let zweiter = anmeldung.access_token().await.unwrap_err();
    assert!(zweiter.ist_gesperrt(), "{zweiter}");
    assert_eq!(attrappe.login_posts(), 1, "kein zweiter Login-POST");
    match anmeldung.status() {
        Status::Gesperrt { grund } => assert!(!grund.contains(PASSWORT)),
        sonst => panic!("kein Sperrzustand: {sonst:?}"),
    }

    // Der Knopf darf wieder – und hebt die Sperre mit einer Anmeldung auf.
    attrappe.setze_passwort_richtig(true);
    let lauf = anmeldung.routine_lauf().await;
    assert!(lauf.gelungen(), "{lauf:?}");
    assert!(anmeldung.sperrgrund().is_none(), "Sperre bleibt stehen");
    assert_eq!(attrappe.login_posts(), 2);
    assert!(matches!(anmeldung.status(), Status::Angemeldet { .. }));
}

#[tokio::test]
async fn eine_unbekannte_seite_bricht_ab_und_nennt_nur_feldnamen() {
    let attrappe = Attrappe::starte().await;
    let ordner = tempfile::tempdir().unwrap();
    let anmeldung = baue(&attrappe, &ordner, speicher(), browser());
    attrappe.unklare_seite_zeigen();

    let lauf = anmeldung.routine_lauf().await;
    let fehler = lauf.fehler.clone().expect("kein Fehler gemeldet");
    assert!(fehler.contains("Unbekannte Seite"), "{fehler}");
    assert!(fehler.contains("kennwort"), "{fehler}");
    assert!(fehler.contains("sprache"), "{fehler}");
    // Kein Wert – auch nicht der, der in der Seite stand.
    assert!(!fehler.contains(GEHEIM_MARKE), "{fehler}");
    assert!(!fehler.contains(PASSWORT), "{fehler}");
    assert!(anmeldung.sperrgrund().is_some(), "ein Versuch, dann Sperre");
    assert_eq!(attrappe.login_posts(), 0);
}

#[tokio::test]
async fn fremder_host_bekommt_kein_passwort() {
    let attrappe = Attrappe::starte().await;
    let ordner = tempfile::tempdir().unwrap();
    let anmeldung = baue(&attrappe, &ordner, speicher(), browser());
    attrappe.fremdes_formular_zeigen();

    let lauf = anmeldung.routine_lauf().await;
    let fehler = lauf.fehler.clone().expect("kein Fehler gemeldet");
    assert!(fehler.contains("anderen Host"), "{fehler}");
    assert!(!fehler.contains(PASSWORT), "{fehler}");
    // Nichts wurde abgeschickt: weder an den fremden Host noch an ZEP.
    assert_eq!(attrappe.login_posts(), 0, "Zugangsdaten wurden gesendet");
    assert_eq!(attrappe.zustimmungen(), 0);
}

#[tokio::test]
async fn invalid_grant_fuehrt_zur_routine_und_die_werkzeuge_gehen_weiter() {
    let attrappe = Attrappe::starte().await;
    let ordner = tempfile::tempdir().unwrap();
    let tresor = speicher();
    let anmeldung = baue(&attrappe, &ordner, Arc::clone(&tresor), browser());
    anmeldung.access_token().await.expect("erste Anmeldung");

    let vermittler = Vermittler::neu(Arc::new(anmeldung.clone()));
    let werkzeuge = vermittler.werkzeuge().await.expect("Werkzeugliste");
    assert!(werkzeuge
        .iter()
        .any(|werkzeug| werkzeug.name == "zep_zeiten"));

    // Refresh-Token widerrufen und das Access-Token verwerfen.
    let refresh = tresor
        .lesen(&schluessel_refresh(&attrappe.mcp_adresse()))
        .unwrap()
        .expect("kein Refresh-Token");
    attrappe.widerrufen(&refresh);
    anmeldung.verwerfen();

    let neues = anmeldung.access_token().await.expect("neue Anmeldung");
    assert!(!neues.is_empty());
    assert_eq!(attrappe.login_posts(), 2, "die Routine nach invalid_grant");

    // Und die Werkzeuge gehen weiter.
    let ergebnis = vermittler
        .aufrufen(CallToolRequestParams::new("zep_projekte".to_string()))
        .await
        .expect("Werkzeugaufruf");
    let text = ergebnis
        .content
        .first()
        .and_then(|block| block.as_text())
        .map(|text| text.text.clone())
        .unwrap_or_default();
    assert!(text.contains("Projekte der Attrappe"), "{text}");
}

#[tokio::test]
async fn ein_abgelehntes_access_token_wird_erneuert_und_einmal_wiederholt() {
    let attrappe = Attrappe::starte().await;
    let ordner = tempfile::tempdir().unwrap();
    let tresor = speicher();
    let anmeldung = baue(&attrappe, &ordner, Arc::clone(&tresor), browser());
    anmeldung.access_token().await.expect("erste Anmeldung");

    let vermittler = Vermittler::neu(Arc::new(anmeldung.clone()));
    vermittler.werkzeuge().await.expect("Werkzeugliste");

    // ZEP kennt das Access-Token nicht mehr: der nächste Aufruf bekommt 401.
    attrappe.access_tokens_entwerten();
    let ergebnis = vermittler
        .aufrufen(CallToolRequestParams::new("zep_zeiten".to_string()))
        .await
        .expect("nach der Erneuerung");
    let text = ergebnis
        .content
        .first()
        .and_then(|block| block.as_text())
        .map(|text| text.text.clone())
        .unwrap_or_default();
    assert!(text.contains("Zeiten der Attrappe"), "{text}");
    assert_eq!(attrappe.login_posts(), 1, "kein neuer Login nötig");
    assert!(attrappe.refresh_aufrufe() >= 1, "kein Refresh nach dem 401");
}

#[tokio::test]
async fn die_hintergrund_erneuerung_haelt_das_refresh_token_am_leben() {
    let attrappe = Attrappe::starte().await;
    attrappe.setze_gueltigkeit(2);
    let ordner = tempfile::tempdir().unwrap();
    let anmeldung = Arc::new(baue(&attrappe, &ordner, speicher(), browser()));
    anmeldung.access_token().await.expect("erste Anmeldung");
    let vorher = attrappe.refresh_aufrufe();

    let hintergrund = anmeldung.starte_hintergrund();
    tokio::time::sleep(Duration::from_millis(2500)).await;
    hintergrund.abort();

    assert!(
        attrappe.refresh_aufrufe() > vorher,
        "ohne Werkzeugaufruf muss trotzdem erneuert werden (vorher {vorher}, jetzt {})",
        attrappe.refresh_aufrufe()
    );
    // Weiter angemeldet, ohne einen zweiten Login.
    assert_eq!(attrappe.login_posts(), 1);
    assert!(anmeldung.access_token().await.is_ok());
}

#[tokio::test]
async fn abmelden_loescht_das_refresh_token_und_die_sperre() {
    let attrappe = Attrappe::starte().await;
    let ordner = tempfile::tempdir().unwrap();
    let tresor = speicher();
    let anmeldung = baue(&attrappe, &ordner, Arc::clone(&tresor), browser());
    anmeldung.access_token().await.expect("Anmeldung");
    assert!(matches!(anmeldung.status(), Status::Angemeldet { .. }));

    anmeldung.abmelden().expect("abmelden");
    assert_eq!(anmeldung.status(), Status::NichtAngemeldet);
    assert!(tresor
        .lesen(&schluessel_refresh(&attrappe.mcp_adresse()))
        .unwrap()
        .is_none());
    assert!(anmeldung.sperrgrund().is_none());
}

#[tokio::test]
async fn die_inertia_zustimmung_wie_bei_zep_wird_bestaetigt() {
    let attrappe = Attrappe::starte().await;
    let ordner = tempfile::tempdir().unwrap();
    let browser = browser();
    let anmeldung = baue(&attrappe, &ordner, speicher(), Arc::clone(&browser));
    attrappe.inertia_zustimmung_zeigen(false);

    let lauf = anmeldung.routine_lauf().await;
    assert!(lauf.gelungen(), "{lauf:?}");
    assert_eq!(attrappe.zustimmungen(), 1);
    assert_eq!(attrappe.login_posts(), 1);
    assert_eq!(browser.aufrufe(), 0, "kein Browser");
    let bericht = lauf.bericht.join("\n");
    assert!(bericht.contains("Zustimmungsseite (Inertia)"), "{bericht}");
    assert!(
        !bericht.contains("auth-"),
        "kein auth_token im Bericht: {bericht}"
    );
    assert!(matches!(anmeldung.status(), Status::Angemeldet { .. }));
}

#[tokio::test]
async fn eine_inertia_zustimmung_fuer_einen_fremden_client_wird_nicht_bestaetigt() {
    let attrappe = Attrappe::starte().await;
    let ordner = tempfile::tempdir().unwrap();
    let anmeldung = baue(&attrappe, &ordner, speicher(), browser());
    attrappe.inertia_zustimmung_zeigen(true);

    let lauf = anmeldung.routine_lauf().await;
    let fehler = lauf.fehler.clone().expect("muss scheitern");
    assert!(fehler.contains("anderen Client"), "{fehler}");
    assert_eq!(attrappe.zustimmungen(), 0, "nichts bestätigt");
}
