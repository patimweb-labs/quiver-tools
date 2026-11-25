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
use quiver_zep::ZepKonfig;
use rmcp::model::CallToolRequestParams;

use attrappe::{Attrappe, AttrappenBrowser, BENUTZER, GEHEIM_MARKE, PASSWORT};

/// Der Takt der Hintergrund-Erneuerung in den Tests.
const TAKT: Duration = Duration::from_millis(50);
/// Die Wartezeit nach einem Fehlschlag vor dem Absenden (wie in der Attrappe).
const PAUSE: Duration = Duration::from_millis(50);

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

/// Eine Konfiguration, deren ZEP es nicht gibt: jeder Versuch scheitert, bevor
/// Benutzer und Passwort abgeschickt werden.
fn tote_konfig(zustand: std::path::PathBuf) -> ZepKonfig {
    ZepKonfig {
        url: "http://127.0.0.1:9/firma/next/mcp".to_string(),
        benutzer: BENUTZER.to_string(),
        passwort: PASSWORT.to_string(),
        zustand,
        takt: TAKT,
        wartezeit: Duration::from_secs(2),
        pause: PAUSE,
    }
}

/// Die Sperrdatei im Zustandsordner des Tests.
fn sperrdatei(ordner: &tempfile::TempDir) -> std::path::PathBuf {
    ordner.path().join("routine-gesperrt.json")
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
        .any(|werkzeug| werkzeug.name == "get_project_info"));
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
    // Benutzer und Passwort wurden gesendet und abgelehnt: das ist ein
    // Fehlversuch bei ZEP, also sperrt die Routine (in einer Datei).
    assert!(
        sperrdatei(&ordner).exists(),
        "ohne Sperrdatei gäbe es keinen Schutz vor dem nächsten Versuch"
    );

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
    // Die Seite kam **vor** dem Formular: ZEP hat nichts bekommen, also gibt es
    // keine Sperre (Aufgabe 11).
    assert!(
        anmeldung.sperrgrund().is_none(),
        "ein Fehler vor dem Absenden darf nicht sperren"
    );
    assert!(!sperrdatei(&ordner).exists());
    assert_eq!(attrappe.login_posts(), 0);
}

#[tokio::test]
async fn ein_netzfehler_vor_dem_abschicken_sperrt_nicht() {
    let ordner = tempfile::tempdir().unwrap();
    let anmeldung = Anmeldung::neu(
        tote_konfig(ordner.path().to_path_buf()),
        speicher(),
        browser(),
    );

    let fehler = anmeldung.access_token().await.unwrap_err();
    assert!(fehler.ist_voruebergehend(), "{fehler}");
    assert!(!fehler.ist_gesperrt());
    assert!(!fehler.to_string().contains(PASSWORT), "{fehler}");
    assert!(
        fehler.to_string().contains("ZEP-Anmeldung nötig"),
        "{fehler}"
    );

    // Keine Sperrdatei und kein Sperrgrund: ZEP hat die Zugangsdaten nie gesehen.
    assert!(anmeldung.sperrgrund().is_none());
    assert!(!sperrdatei(&ordner).exists());
    assert_eq!(anmeldung.status(), Status::NichtAngemeldet);

    // Der nächste Versuch wartet eine Weile (Merker im Speicher, nicht in der Datei).
    assert!(anmeldung.wartezeit_bis_neuer_versuch().is_some());
    let zweiter = anmeldung.access_token().await.unwrap_err();
    assert!(zweiter.ist_voruebergehend(), "{zweiter}");
    assert!(!sperrdatei(&ordner).exists());
}

#[tokio::test]
async fn nach_einem_fehlschlag_vor_dem_abschicken_klappt_der_zweite_versuch() {
    let attrappe = Attrappe::starte().await;
    let ordner = tempfile::tempdir().unwrap();
    let anmeldung = baue(&attrappe, &ordner, speicher(), browser());
    // Eine unbekannte Seite vor dem Formular: ZEP sieht keine Zugangsdaten.
    attrappe.unklare_seite_zeigen();

    let erster = anmeldung.access_token().await.unwrap_err();
    assert!(erster.ist_voruebergehend(), "{erster}");
    assert!(anmeldung.sperrgrund().is_none());
    assert!(
        !sperrdatei(&ordner).exists(),
        "kein Fehlversuch, keine Sperre"
    );
    assert_eq!(attrappe.login_posts(), 0);

    // Vor der Wartezeit gibt es keinen zweiten Versuch …
    let zu_frueh = anmeldung.access_token().await.unwrap_err();
    assert!(zu_frueh.ist_voruebergehend(), "{zu_frueh}");
    assert_eq!(attrappe.login_posts(), 0);

    // … danach klappt die Anmeldung ohne Zutun des Users.
    attrappe.unklare_seite_aus();
    tokio::time::sleep(PAUSE * 3).await;
    let token = anmeldung.access_token().await.expect("zweiter Versuch");
    assert!(!token.is_empty());
    assert_eq!(attrappe.login_posts(), 1, "genau ein Login-POST");
    assert!(matches!(anmeldung.status(), Status::Angemeldet { .. }));
    assert!(!sperrdatei(&ordner).exists());
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
        .any(|werkzeug| werkzeug.name == "get_attendances"));

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
        .aufrufen(
            CallToolRequestParams::new("get_project_info".to_string()),
            true,
        )
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
        .aufrufen(
            CallToolRequestParams::new("get_attendances".to_string()),
            true,
        )
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

#[tokio::test]
async fn ein_schreibender_aufruf_wird_nach_einem_401_nicht_wiederholt() {
    let attrappe = Attrappe::starte().await;
    let ordner = tempfile::tempdir().unwrap();
    let tresor = speicher();
    let anmeldung = baue(&attrappe, &ordner, Arc::clone(&tresor), browser());
    anmeldung.access_token().await.expect("erste Anmeldung");

    let vermittler = Vermittler::neu(Arc::new(anmeldung.clone()));
    vermittler.werkzeuge().await.expect("Werkzeugliste");

    // ZEP kennt das Access-Token nicht mehr.
    attrappe.access_tokens_entwerten();
    let erneuerungen = attrappe.refresh_aufrufe();
    let ergebnis = vermittler
        .aufrufen(
            CallToolRequestParams::new("zep_umschalten".to_string()),
            false,
        )
        .await;

    // Kein zweiter Versuch: der Aufruf scheitert, und es wurde nichts erneuert.
    assert!(
        ergebnis.is_err(),
        "ein Schreibaufruf darf nicht wiederholt werden"
    );
    assert_eq!(
        attrappe.refresh_aufrufe(),
        erneuerungen,
        "für ein Schreibwerkzeug darf kein Token erneuert und wiederholt werden"
    );
}
