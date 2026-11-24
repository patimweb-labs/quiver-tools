// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Tests der Ablage gegen den **öffentlichen** Weg: jeder Test in einem eigenen
//! temporären Ordner, nie im Benutzerverzeichnis.

use quiver_diagramm::ablage::{Ablage, AblageFehler, Stand, MAX_BYTES};
use std::fs;
use std::sync::{Arc, Barrier};
use std::thread;
use tempfile::TempDir;

fn ablage(ordner: &TempDir) -> Ablage {
    Ablage::neu(&ordner.path().to_string_lossy(), "excalidraw").expect("Arbeitsordner")
}

fn temp() -> TempDir {
    tempfile::tempdir().expect("temporärer Ordner")
}

/// Prüft, ob ein Fehler die erwartete Art hat.
type Pruefung = fn(&AblageFehler) -> bool;

#[test]
fn anlegen_lesen_und_liste() {
    let ordner = temp();
    let ablage = ablage(&ordner);

    let stand = ablage
        .anlegen("entwurf.excalidraw", "{\"type\":\"excalidraw\"}")
        .expect("anlegen");
    assert_eq!(stand.as_str().len(), 64);

    let inhalt = ablage.lesen("entwurf.excalidraw").expect("lesen");
    assert_eq!(inhalt.text, "{\"type\":\"excalidraw\"}");
    assert_eq!(inhalt.stand, stand);
    assert_eq!(inhalt.groesse, 21);
    assert_eq!(ablage.stand("entwurf.excalidraw").unwrap(), stand);

    let liste = ablage.liste().expect("liste");
    assert_eq!(liste.len(), 1);
    assert_eq!(liste[0].pfad, "entwurf.excalidraw");
    assert_eq!(liste[0].groesse, 21);
    assert!(
        ordner.path().join(".sperre").is_file(),
        "die Sperrdatei liegt im Arbeitsordner"
    );
    assert!(
        !liste.iter().any(|e| e.pfad.starts_with('.')),
        "Verstecktes erscheint nicht in der Liste"
    );
}

#[test]
fn unterordner_sind_erlaubt_und_erscheinen_in_der_liste() {
    let ordner = temp();
    let ablage = ablage(&ordner);
    ablage
        .anlegen("kunde/a/entwurf.excalidraw", "x")
        .expect("Unterordner wird angelegt");
    let liste = ablage.liste().expect("liste");
    assert_eq!(liste[0].pfad, "kunde/a/entwurf.excalidraw");
    assert!(ordner.path().join("kunde/a").is_dir());
}

#[test]
fn anlegen_ueberschreibt_nie() {
    let ordner = temp();
    let ablage = ablage(&ordner);
    ablage.anlegen("a.excalidraw", "erster").expect("anlegen");
    let fehler = ablage
        .anlegen("a.excalidraw", "zweiter")
        .expect_err("überschreiben verboten");
    assert!(
        matches!(fehler, AblageFehler::ExistiertBereits(_)),
        "{fehler}"
    );
    assert_eq!(ablage.lesen("a.excalidraw").unwrap().text, "erster");
}

#[test]
fn aendern_nur_mit_passendem_stand_und_mit_einer_sicherung() {
    let ordner = temp();
    let ablage = ablage(&ordner);
    let erster = ablage.anlegen("a.excalidraw", "eins").expect("anlegen");

    let zweiter = ablage
        .aendern("a.excalidraw", "zwei", &erster)
        .expect("ändern");
    assert_eq!(ablage.lesen("a.excalidraw").unwrap().text, "zwei");
    assert_eq!(
        fs::read_to_string(ordner.path().join("a.excalidraw.bak")).unwrap(),
        "eins"
    );

    let dritter = ablage
        .aendern("a.excalidraw", "drei", &zweiter)
        .expect("ändern");
    assert_eq!(
        fs::read_to_string(ordner.path().join("a.excalidraw.bak")).unwrap(),
        "zwei",
        "die Sicherung ist genau eine und hält den vorigen Stand"
    );
    assert_eq!(ablage.lesen("a.excalidraw").unwrap().stand, dritter);

    let sicherungen: Vec<String> = fs::read_dir(ordner.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".bak"))
        .collect();
    assert_eq!(sicherungen, vec!["a.excalidraw.bak".to_string()]);
    assert_eq!(
        ablage.liste().expect("liste").len(),
        1,
        "die Sicherung ist kein Diagramm"
    );
}

#[test]
fn falscher_stand_schreibt_nichts() {
    let ordner = temp();
    let ablage = ablage(&ordner);
    ablage.anlegen("a.excalidraw", "eins").expect("anlegen");
    let veraltet = Stand::aus_hex(&"a".repeat(64)).expect("Hex");
    let fehler = ablage
        .aendern("a.excalidraw", "zwei", &veraltet)
        .expect_err("falscher Stand");
    assert!(matches!(fehler, AblageFehler::InzwischenGeaendert(_)));
    assert_eq!(ablage.lesen("a.excalidraw").unwrap().text, "eins");
    assert!(!ordner.path().join("a.excalidraw.bak").exists());
}

#[test]
fn gleichzeitiges_aendern_laesst_nur_einen_gewinnen() {
    let ordner = temp();
    let ablage = Arc::new(ablage(&ordner));
    let stand = ablage.anlegen("streit.excalidraw", "alt").expect("anlegen");
    let start = Arc::new(Barrier::new(2));

    let mut griffe = Vec::new();
    for nummer in 0..2 {
        let ablage = Arc::clone(&ablage);
        let start = Arc::clone(&start);
        let stand = stand.clone();
        griffe.push(thread::spawn(move || {
            // Beide lesen denselben Stand, dann schreiben beide.
            let _ = ablage.lesen("streit.excalidraw").expect("lesen");
            start.wait();
            ablage.aendern("streit.excalidraw", &format!("neu-{nummer}"), &stand)
        }));
    }
    let ergebnisse: Vec<Result<Stand, AblageFehler>> =
        griffe.into_iter().map(|g| g.join().unwrap()).collect();
    let erfolge = ergebnisse.iter().filter(|e| e.is_ok()).count();
    let abgelehnt = ergebnisse
        .iter()
        .filter(|e| matches!(e, Err(AblageFehler::InzwischenGeaendert(_))))
        .count();
    assert_eq!(erfolge, 1, "{ergebnisse:?}");
    assert_eq!(abgelehnt, 1, "{ergebnisse:?}");
}

#[test]
fn ablehnungsregeln_fuer_pfade() {
    let ordner = temp();
    let ablage = ablage(&ordner);

    let faelle: Vec<(&str, Pruefung)> = vec![
        ("", |f| matches!(f, AblageFehler::PfadLeer)),
        ("/etc/passwd.excalidraw", |f| {
            matches!(f, AblageFehler::AbsoluterPfad(_))
        }),
        ("../ausbruch.excalidraw", |f| {
            matches!(f, AblageFehler::PfadAusbruch(_))
        }),
        ("a/../../ausbruch.excalidraw", |f| {
            matches!(f, AblageFehler::PfadAusbruch(_))
        }),
        ("C:/temp/a.excalidraw", |f| {
            matches!(f, AblageFehler::Verboten { .. })
        }),
        ("a\\b.excalidraw", |f| {
            matches!(f, AblageFehler::Verboten { .. })
        }),
        ("a:b.excalidraw", |f| {
            matches!(f, AblageFehler::Verboten { .. })
        }),
        ("a\0b.excalidraw", |f| {
            matches!(f, AblageFehler::Verboten { .. })
        }),
        (".versteckt.excalidraw", |f| {
            matches!(f, AblageFehler::Versteckt(_))
        }),
        ("unter/.versteckt.excalidraw", |f| {
            matches!(f, AblageFehler::Versteckt(_))
        }),
        ("CON.excalidraw", |f| {
            matches!(f, AblageFehler::Geraetename(_))
        }),
        ("unter/com1.excalidraw", |f| {
            matches!(f, AblageFehler::Geraetename(_))
        }),
        ("NUL.txt.excalidraw", |f| {
            matches!(f, AblageFehler::Geraetename(_))
        }),
        ("ohne_endung", |f| {
            matches!(f, AblageFehler::FalscheEndung { .. })
        }),
        ("bild.png", |f| {
            matches!(f, AblageFehler::FalscheEndung { .. })
        }),
        ("a//b.excalidraw", |f| {
            matches!(f, AblageFehler::Verboten { .. })
        }),
        ("a/./b.excalidraw", |f| {
            matches!(f, AblageFehler::Verboten { .. })
        }),
        ("a/b/c/d/e/f/g/h/i/j/k/l/m/n/o/p/q/r.excalidraw", |f| {
            matches!(f, AblageFehler::Verboten { .. })
        }),
    ];

    for (pfad, pruefung) in faelle {
        let fehler = ablage.pfad(pfad).expect_err(pfad);
        assert!(pruefung(&fehler), "{pfad:?} ergab {fehler:?}");
        let fehler = ablage.anlegen(pfad, "x").expect_err(pfad);
        assert!(pruefung(&fehler), "anlegen {pfad:?} ergab {fehler:?}");
    }
}

#[test]
fn endung_ist_gross_klein_egal() {
    let ordner = temp();
    let ablage = ablage(&ordner);
    ablage.anlegen("GROSS.EXCALIDRAW", "x").expect("anlegen");
    assert_eq!(ablage.lesen("GROSS.EXCALIDRAW").unwrap().text, "x");
}

#[test]
fn groessengrenzen_gelten_fuer_lesen_und_schreiben() {
    let ordner = temp();
    let ablage = ablage(&ordner);

    let zu_gross = "a".repeat(MAX_BYTES as usize + 1);
    let fehler = ablage
        .anlegen("gross.excalidraw", &zu_gross)
        .expect_err("zu groß");
    assert!(matches!(fehler, AblageFehler::ZuGross { .. }), "{fehler}");
    assert!(!ordner.path().join("gross.excalidraw").exists());

    // Am Werkzeug vorbei geschrieben: das Lesen prüft trotzdem.
    fs::write(ordner.path().join("fremd.excalidraw"), &zu_gross).unwrap();
    let fehler = ablage
        .lesen("fremd.excalidraw")
        .expect_err("zu groß beim Lesen");
    assert!(matches!(fehler, AblageFehler::ZuGross { .. }), "{fehler}");
}

#[test]
fn nur_texte_und_nur_normale_dateien() {
    let ordner = temp();
    let ablage = ablage(&ordner);

    fs::write(ordner.path().join("bin.excalidraw"), [0xff, 0xfe, 0x00]).unwrap();
    let fehler = ablage.lesen("bin.excalidraw").expect_err("kein UTF-8");
    assert!(matches!(fehler, AblageFehler::KeinText(_)), "{fehler}");

    fs::create_dir(ordner.path().join("ordner.excalidraw")).unwrap();
    let fehler = ablage.lesen("ordner.excalidraw").expect_err("kein File");
    assert!(matches!(fehler, AblageFehler::KeineDatei(_)), "{fehler}");

    let fehler = ablage.lesen("fehlt.excalidraw").expect_err("fehlt");
    assert!(matches!(fehler, AblageFehler::NichtGefunden(_)), "{fehler}");
}

#[test]
fn ausgaben_sind_svg_oder_png_und_ueberschreiben_nie() {
    let ordner = temp();
    let ablage = ablage(&ordner);

    ablage
        .ausgabe_schreiben("bild.png", b"png")
        .expect("PNG schreiben");
    assert!(ordner.path().join("bild.png").is_file());

    let fehler = ablage
        .ausgabe_schreiben("bild.png", b"nochmal")
        .expect_err("nie überschreiben");
    assert!(matches!(fehler, AblageFehler::ExistiertBereits(_)));

    let fehler = ablage
        .ausgabe_schreiben("bild.txt", b"x")
        .expect_err("falsche Endung");
    assert!(
        matches!(fehler, AblageFehler::FalscheEndung { .. }),
        "{fehler}"
    );

    ablage
        .ausgabe_schreiben("plan.svg", b"<svg/>")
        .expect("SVG schreiben");

    let liste = ablage.liste().expect("liste");
    assert!(liste.is_empty(), "Ausgaben sind keine Diagramme: {liste:?}");
}

#[test]
fn arbeitsordner_muss_absolut_sein() {
    let fehler = Ablage::neu("", "excalidraw").expect_err("leer");
    assert!(matches!(fehler, AblageFehler::OrdnerFehlt), "{fehler}");

    let fehler = Ablage::neu("relativ/ordner", "excalidraw").expect_err("relativ");
    assert!(
        matches!(fehler, AblageFehler::OrdnerNichtAbsolut(_)),
        "{fehler}"
    );

    let fehler = Ablage::neu("/tmp/quiver-diagramm", "").expect_err("ohne Endung");
    assert!(
        matches!(fehler, AblageFehler::EndungUngueltig(_)),
        "{fehler}"
    );
}

#[test]
fn tilde_wird_aufgeloest_ohne_nebenwirkungen() {
    let ablage = Ablage::neu("~", "excalidraw").expect("Home");
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
        .expect("Home");
    assert_eq!(ablage.wurzel(), fs::canonicalize(home).unwrap());
    assert_eq!(ablage.endung(), "excalidraw");
}

#[test]
fn stand_ist_ein_sha256() {
    assert!(Stand::aus_hex(&"AB".repeat(32)).is_some());
    assert!(Stand::aus_hex("abc").is_none());
    assert!(Stand::aus_hex(&"z".repeat(64)).is_none());
    assert_eq!(
        Stand::aus_hex(&"AB".repeat(32)).unwrap().as_str(),
        "ab".repeat(32)
    );
}

#[cfg(unix)]
#[test]
fn symlinks_sind_auf_jeder_ebene_verboten() {
    use std::os::unix::fs::symlink;

    let aussen = temp();
    let ordner = temp();
    let ablage = ablage(&ordner);

    fs::write(aussen.path().join("dahinter.excalidraw"), "geheim").unwrap();

    // Datei selbst ist ein Symlink.
    symlink(
        aussen.path().join("dahinter.excalidraw"),
        ordner.path().join("verweis.excalidraw"),
    )
    .unwrap();
    let fehler = ablage.lesen("verweis.excalidraw").expect_err("Symlink");
    assert!(matches!(fehler, AblageFehler::Symlink(_)), "{fehler}");
    let fehler = ablage
        .aendern(
            "verweis.excalidraw",
            "neu",
            &Stand::aus_hex(&"0".repeat(64)).unwrap(),
        )
        .expect_err("Symlink beim Ändern");
    assert!(matches!(fehler, AblageFehler::Symlink(_)), "{fehler}");
    assert_eq!(
        fs::read_to_string(aussen.path().join("dahinter.excalidraw")).unwrap(),
        "geheim",
        "über den Symlink darf nichts geschrieben werden"
    );

    // Ordner auf dem Weg ist ein Symlink.
    symlink(aussen.path(), ordner.path().join("verzeichnis")).unwrap();
    let fehler = ablage
        .anlegen("verzeichnis/neu.excalidraw", "x")
        .expect_err("Symlink-Ordner");
    assert!(matches!(fehler, AblageFehler::Symlink(_)), "{fehler}");
    assert!(!aussen.path().join("neu.excalidraw").exists());

    // Auch tiefer dahinter darf nichts entstehen.
    let fehler = ablage
        .anlegen("verzeichnis/tief/noch-tiefer/neu.excalidraw", "x")
        .expect_err("Symlink-Ordner mit Unterordnern");
    assert!(matches!(fehler, AblageFehler::Symlink(_)), "{fehler}");
    assert!(
        !aussen.path().join("tief").exists(),
        "außerhalb darf kein Ordner entstehen"
    );

    // Ein leerer Ordner, der erst beim Anlegen entsteht, ist erlaubt.
    ablage.anlegen("echt/neu.excalidraw", "x").expect("anlegen");
}
