// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Abnahme aus `docs/aufgaben/01-anmeldung.md` – nur gegen die Attrappe,
//! nie gegen echte Microsoft-Server, nie gegen den echten Schlüsselbund.

mod attrappe;

use std::sync::Arc;
use std::time::Duration;

use attrappe::{Attrappe, AttrappenBrowser, SCHLUESSEL};
use m365_anmeldung::{
    AnmeldeFehler, Anmeldung, EntraAnmeldung, Konfig, SpeicherTresor, Token, Tresor,
};

/// Tresor, den der Test auch nach der Anmeldung noch lesen kann.
#[derive(Clone)]
struct GeteilterTresor(Arc<SpeicherTresor>);

impl Tresor for GeteilterTresor {
    fn lesen(&self, schluessel: &str) -> anyhow::Result<Option<String>> {
        self.0.lesen(schluessel)
    }

    fn schreiben(&self, schluessel: &str, wert: &str) -> anyhow::Result<()> {
        self.0.schreiben(schluessel, wert)
    }

    fn loeschen(&self, schluessel: &str) -> anyhow::Result<()> {
        self.0.loeschen(schluessel)
    }
}

type AnmeldungUnterTest = EntraAnmeldung<GeteilterTresor, AttrappenBrowser>;

fn bauen(attrappe: &Attrappe, browser: AttrappenBrowser) -> (AnmeldungUnterTest, GeteilterTresor) {
    bauen_mit(attrappe::konfig(attrappe), browser)
}

fn bauen_mit(konfig: Konfig, browser: AttrappenBrowser) -> (AnmeldungUnterTest, GeteilterTresor) {
    let tresor = GeteilterTresor(Arc::new(SpeicherTresor::neu()));
    // Ohne `neu()`: dieses Tests sollen nicht von `M365_ACCESS_TOKEN` abhängen.
    let anmeldung = EntraAnmeldung::mit_festem_token(konfig, tresor.clone(), browser, None)
        .expect("Anmeldung baut");
    (anmeldung, tresor)
}

fn bereit(token: Token) -> String {
    match token {
        Token::Bereit(access) => access,
        Token::WartetAufBrowser => panic!("hat auf den Browser gewartet"),
    }
}

fn gespeichertes_refresh(tresor: &GeteilterTresor) -> Option<String> {
    tresor.lesen(SCHLUESSEL).expect("Tresor lesbar")
}

#[tokio::test]
async fn erste_anmeldung_ueber_den_browser() {
    let attrappe = Attrappe::starten().await;
    let browser = AttrappenBrowser::neu();
    let (anmeldung, tresor) = bauen(&attrappe, browser.clone());

    assert_eq!(
        bereit(anmeldung.access_token().await.expect("angemeldet")),
        "at-1"
    );

    assert_eq!(browser.aufrufe(), 1, "genau ein Browser-Aufruf");
    assert_eq!(attrappe.autorisierungen(), 1);
    assert_eq!(attrappe.token_aufrufe(), 1);
    assert_eq!(
        gespeichertes_refresh(&tresor).as_deref(),
        Some("rt-1"),
        "Refresh-Token liegt im Tresor"
    );
}

#[tokio::test]
async fn zweiter_aufruf_kommt_aus_dem_speicher() {
    let attrappe = Attrappe::starten().await;
    let browser = AttrappenBrowser::neu();
    let (anmeldung, _tresor) = bauen(&attrappe, browser.clone());

    assert_eq!(
        bereit(anmeldung.access_token().await.expect("erst")),
        "at-1"
    );
    assert_eq!(
        bereit(anmeldung.access_token().await.expect("zweit")),
        "at-1"
    );

    assert_eq!(attrappe.token_aufrufe(), 1, "kein weiterer Token-Aufruf");
    assert_eq!(browser.aufrufe(), 1);
}

#[tokio::test]
async fn abgelaufenes_token_wird_erneuert_und_rotiert() {
    let attrappe = Attrappe::starten().await;
    let browser = AttrappenBrowser::neu();
    let (anmeldung, tresor) = bauen(&attrappe, browser.clone());

    assert_eq!(
        bereit(anmeldung.access_token().await.expect("erst")),
        "at-1"
    );
    assert_eq!(gespeichertes_refresh(&tresor).as_deref(), Some("rt-1"));

    // Ein 401 von Graph verwirft nur das Access-Token.
    anmeldung.verwerfen();
    assert_eq!(bereit(anmeldung.access_token().await.expect("neu")), "at-2");

    assert_eq!(attrappe.refresh_aufrufe(), 1, "ein Refresh");
    assert_eq!(browser.aufrufe(), 1, "kein neuer Browser");
    assert_eq!(
        gespeichertes_refresh(&tresor).as_deref(),
        Some("rt-2"),
        "rotiertes Refresh-Token gespeichert"
    );
}

#[tokio::test]
async fn kurze_gueltigkeit_erneuert_beim_naechsten_aufruf() {
    let attrappe = Attrappe::starten().await;
    let browser = AttrappenBrowser::neu();
    // 30 s < 60 s Vorlauf: das Token gilt sofort als abgelaufen.
    attrappe.setze_gueltigkeit(30);
    let (anmeldung, tresor) = bauen(&attrappe, browser.clone());

    assert_eq!(
        bereit(anmeldung.access_token().await.expect("erst")),
        "at-1"
    );
    assert_eq!(
        bereit(anmeldung.access_token().await.expect("zweit")),
        "at-2"
    );

    assert_eq!(attrappe.refresh_aufrufe(), 1, "ein Refresh");
    assert_eq!(browser.aufrufe(), 1, "kein neuer Browser");
    assert_eq!(
        gespeichertes_refresh(&tresor).as_deref(),
        Some("rt-2"),
        "rotiertes Refresh-Token gespeichert"
    );
}

#[tokio::test]
async fn invalid_grant_oeffnet_den_browser_erneut() {
    let attrappe = Attrappe::starten().await;
    let browser = AttrappenBrowser::neu();
    let (anmeldung, tresor) = bauen(&attrappe, browser.clone());

    assert_eq!(
        bereit(anmeldung.access_token().await.expect("erst")),
        "at-1"
    );

    // Der User hat die App in Entra entzogen: Refresh-Token ist widerrufen.
    attrappe.widerrufen("rt-1");
    anmeldung.verwerfen();

    assert_eq!(bereit(anmeldung.access_token().await.expect("neu")), "at-2");
    assert_eq!(attrappe.refresh_aufrufe(), 1);
    assert_eq!(browser.aufrufe(), 2, "Browser ein zweites Mal geöffnet");
    assert_eq!(attrappe.autorisierungen(), 2);
    assert_eq!(gespeichertes_refresh(&tresor).as_deref(), Some("rt-2"));
}

#[tokio::test]
async fn falscher_state_wird_verworfen() {
    let attrappe = Attrappe::starten().await;
    let browser = AttrappenBrowser::mit_state("falsch");
    let (anmeldung, tresor) = bauen(&attrappe, browser);

    let fehler = anmeldung.access_token().await.expect_err("muss scheitern");
    assert!(matches!(fehler, AnmeldeFehler::StateFalsch), "{fehler:?}");
    assert!(
        gespeichertes_refresh(&tresor).is_none(),
        "nichts gespeichert"
    );
}

#[tokio::test]
async fn wartet_auf_browser_und_erbt_spaeter_das_ergebnis() {
    let attrappe = Attrappe::starten().await;
    // Browser kommt erst nach 600 ms zurück, das Tool wartet nur 200 ms.
    let browser = AttrappenBrowser::traege(Duration::from_millis(600));
    let mut konfig = attrappe::konfig(&attrappe);
    konfig.warten = Duration::from_millis(200);
    konfig.lauscher_hoechstens = Duration::from_secs(30);
    let (anmeldung, _tresor) = bauen_mit(konfig, browser.clone());

    let erst = anmeldung.access_token().await.expect("kein Fehler");
    assert_eq!(erst, Token::WartetAufBrowser);

    // Der nächste Aufruf bekommt das Ergebnis, ohne dass der Browser neu aufgeht.
    let mut ergebnis = anmeldung.access_token().await.expect("kein Fehler");
    for _ in 0..30 {
        if matches!(ergebnis, Token::Bereit(_)) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        ergebnis = anmeldung.access_token().await.expect("kein Fehler");
    }

    assert_eq!(bereit(ergebnis), "at-1");
    assert_eq!(browser.aufrufe(), 1, "nur ein Browser-Aufruf");
    assert_eq!(attrappe.autorisierungen(), 1);
}

#[tokio::test]
async fn zwei_gleichzeitige_aufrufe_oeffnen_nur_einen_browser() {
    let attrappe = Attrappe::starten().await;
    let browser = AttrappenBrowser::neu();
    let (anmeldung, _tresor) = bauen(&attrappe, browser.clone());

    let (eins, zwei) = tokio::join!(anmeldung.access_token(), anmeldung.access_token());
    assert_eq!(bereit(eins.expect("eins")), "at-1");
    assert_eq!(bereit(zwei.expect("zwei")), "at-1");

    assert_eq!(browser.aufrufe(), 1, "nur ein Anmeldevorgang");
    assert_eq!(attrappe.autorisierungen(), 1);
}

#[tokio::test]
async fn festes_token_aus_der_umgebung_ohne_netz() {
    std::env::set_var("M365_ACCESS_TOKEN", "von-quiver-gereicht");

    let attrappe = Attrappe::starten().await;
    let browser = AttrappenBrowser::neu();
    let tresor = GeteilterTresor(Arc::new(SpeicherTresor::neu()));
    let anmeldung =
        EntraAnmeldung::neu(attrappe::konfig(&attrappe), tresor.clone(), browser.clone())
            .expect("Anmeldung baut");

    assert_eq!(
        bereit(anmeldung.access_token().await.expect("bereit")),
        "von-quiver-gereicht"
    );
    assert_eq!(browser.aufrufe(), 0, "kein Browser");
    assert_eq!(attrappe.token_aufrufe(), 0, "kein Token-Endpunkt");
    assert!(gespeichertes_refresh(&tresor).is_none(), "kein Tresor");

    std::env::remove_var("M365_ACCESS_TOKEN");
}

#[tokio::test]
async fn kein_token_in_fehlermeldungen() {
    let attrappe = Attrappe::starten().await;
    let browser = AttrappenBrowser::neu();
    let (anmeldung, tresor) = bauen(&attrappe, browser);

    // Erste Anmeldung stellt at-1/rt-1 aus (bekannte Werte für den Test).
    assert_eq!(
        bereit(anmeldung.access_token().await.expect("erst")),
        "at-1"
    );

    // Abmelden, dann scheitert der Code-Tausch → Fehlertext.
    anmeldung.abmelden().expect("abgemeldet");
    attrappe.fehler_im_token_endpunkt();
    let fehler = anmeldung.access_token().await.expect_err("muss scheitern");
    let text = format!("{fehler}");

    let bekannt = [
        "at-1",
        "at-2",
        "rt-1",
        "rt-2",
        "von-quiver-gereicht",
        attrappe::CODE,
    ];
    for wert in bekannt {
        assert!(
            !text.contains(wert),
            "Token-Wert {wert:?} steht im Fehlertext: {text}"
        );
    }
    assert!(gespeichertes_refresh(&tresor).is_none(), "Tresor ist leer");
}

#[test]
fn konfig_aus_der_umgebung() {
    std::env::remove_var("M365_CLIENT_ID");
    std::env::remove_var("M365_TENANT");
    std::env::remove_var("M365_SCOPES");
    std::env::remove_var("M365_AUTHORITY");

    // Ohne `M365_CLIENT_ID` gilt die eingebaute Registrierung (SPEC.md §2a).
    let konfig = Konfig::aus_umgebung().expect("konfig");
    assert_eq!(konfig.client_id, m365_anmeldung::STANDARD_CLIENT_ID);
    assert_eq!(konfig.tenant, "organizations");
    assert_eq!(konfig.scopes.len(), m365_anmeldung::STANDARD_SCOPES.len());
    assert_eq!(konfig.authority, "https://login.microsoftonline.com");
    assert_eq!(konfig.warten, Duration::from_secs(45));
    assert_eq!(konfig.lauscher_hoechstens, Duration::from_secs(300));

    std::env::set_var("M365_CLIENT_ID", "  abc-123  ");
    let konfig = Konfig::aus_umgebung().expect("konfig");
    assert_eq!(konfig.client_id, "abc-123", "eigene Registrierung gewinnt");

    std::env::set_var("M365_TENANT", "mein-tenant");
    std::env::set_var("M365_SCOPES", "User.Read, Mail.Send");
    std::env::set_var("M365_AUTHORITY", "http://127.0.0.1:1/");
    let konfig = Konfig::aus_umgebung().expect("konfig");
    assert_eq!(konfig.tenant, "mein-tenant");
    assert_eq!(konfig.scopes, vec!["User.Read", "Mail.Send"]);
    assert_eq!(konfig.authority, "http://127.0.0.1:1");

    std::env::remove_var("M365_CLIENT_ID");
    std::env::remove_var("M365_TENANT");
    std::env::remove_var("M365_SCOPES");
    std::env::remove_var("M365_AUTHORITY");
}
