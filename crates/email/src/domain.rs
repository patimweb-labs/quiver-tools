// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Daten des Postfachs und die Regeln, die ohne Netz auskommen.
//!
//! Alles hier ist Form und reine Funktion: Adressen, Kopfzeilen, Nachrichten,
//! die Empfänger einer Antwort und die Kette für `References`. Die
//! Protokollschicht (`postfach`, `versand`) macht daraus IMAP bzw. SMTP; die
//! Werkzeuge in [`crate::server`] reichen die Daten nur weiter.

use std::fmt;
use std::future::Future;
use std::pin::Pin;

/// Das Postfach, das die Vorlage als Standard nimmt.
pub const INBOX: &str = "INBOX";
/// Wie viele Kopfzeilen ein Aufruf ohne Angabe liefert.
pub const ANZAHL_STANDARD: usize = 20;
/// Mehr Kopfzeilen gibt kein Werkzeug heraus (wie die Vorlage).
pub const ANZAHL_MAX: usize = 100;
/// Wie viel Text `email_read` ohne Angabe zeigt.
pub const MAX_ZEICHEN_STANDARD: usize = 6000;

/// Die Zukunft einer Protokolloperation – so bleiben [`Postfach`] und
/// [`Versand`] als Trait-Objekte brauchbar (async fn in Trait ist das nicht).
///
/// [`Postfach`]: crate::postfach::Postfach
/// [`Versand`]: crate::versand::Versand
pub type Zukunft<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, crate::fehler::MailFehler>> + Send + 'a>>;

/// Wie die Verbindung gesichert wird.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sicherheit {
    /// TLS ab dem ersten Byte (IMAP 993, SMTP 465).
    Tls,
    /// Klartext, dann STARTTLS (IMAP 143, SMTP 587).
    Starttls,
    /// Unverschlüsselt – nur für lokale Brücken.
    Keine,
}

impl Sicherheit {
    /// Liest den Wert aus dem quiver-Feld. `None` heißt: unbekannter Wert.
    pub fn aus_text(text: &str) -> Option<Sicherheit> {
        match text.trim().to_ascii_lowercase().as_str() {
            "tls" | "ssl" | "implicit" => Some(Sicherheit::Tls),
            "starttls" | "start-tls" | "opportunistic" => Some(Sicherheit::Starttls),
            "keine" | "kein" | "none" | "ohne" | "klartext" | "plain" => Some(Sicherheit::Keine),
            _ => None,
        }
    }

    /// Wie die Verbindung in einer Antwort heißt.
    pub fn wort(self) -> &'static str {
        match self {
            Sicherheit::Tls => "TLS",
            Sicherheit::Starttls => "STARTTLS",
            Sicherheit::Keine => "ohne Verschlüsselung",
        }
    }
}

/// Die eine IMAP-Verbindung. `Debug` zeigt das Passwort nie.
#[derive(Clone)]
pub struct ImapKonfiguration {
    pub host: String,
    pub port: u16,
    pub sicherheit: Sicherheit,
    pub benutzer: String,
    pub passwort: String,
    pub zertifikat_pruefen: bool,
}

/// Die eine SMTP-Verbindung. `Debug` zeigt das Passwort nie.
#[derive(Clone)]
pub struct SmtpKonfiguration {
    pub host: String,
    pub port: u16,
    pub sicherheit: Sicherheit,
    pub benutzer: String,
    pub passwort: String,
    pub zertifikat_pruefen: bool,
}

/// Der eine Zugang: Absender, IMAP und SMTP.
#[derive(Clone)]
pub struct Konto {
    /// Die Absenderadresse (`EMAIL_ADRESSE`).
    pub adresse: String,
    /// Der Anzeigename, falls gesetzt.
    pub name: Option<String>,
    pub imap: ImapKonfiguration,
    pub smtp: SmtpKonfiguration,
}

impl fmt::Debug for ImapKonfiguration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImapKonfiguration")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("sicherheit", &self.sicherheit)
            .field("benutzer", &self.benutzer)
            .field("passwort", &"[geheim]")
            .field("zertifikat_pruefen", &self.zertifikat_pruefen)
            .finish()
    }
}

impl fmt::Debug for SmtpKonfiguration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SmtpKonfiguration")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("sicherheit", &self.sicherheit)
            .field("benutzer", &self.benutzer)
            .field("passwort", &"[geheim]")
            .field("zertifikat_pruefen", &self.zertifikat_pruefen)
            .finish()
    }
}

impl fmt::Debug for Konto {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Konto")
            .field("adresse", &self.adresse)
            .field("name", &self.name)
            .field("imap", &self.imap)
            .field("smtp", &self.smtp)
            .finish()
    }
}

impl Konto {
    /// Ob das Zertifikat geprüft wird. Ausnahmen gelten nur für lokale Brücken
    /// (Proton-Bridge, selbstsigniert) – sonst bleibt die Prüfung an und es
    /// gibt eine Warnung.
    pub fn prueft_zertifikat(gewuenscht: bool, host: &str) -> bool {
        if gewuenscht {
            return true;
        }
        if ist_lokaler_host(host) {
            return false;
        }
        // Einmal je Prozess: `tracing` hat im Programm keinen Abonnenten
        // (docs/offen.md), deshalb geht der Hinweis zusätzlich auf stderr –
        // dort landet das Log, das quiver vom Werkzeug sieht.
        static EINMAL: std::sync::Once = std::sync::Once::new();
        EINMAL.call_once(|| {
            tracing::warn!(
                host,
                "Zertifikatsprüfung abgeschaltet, aber der Host ist nicht lokal – Prüfung bleibt an"
            );
            eprintln!(
                "Hinweis: zertifikat_pruefen ist aus, gilt aber nur für 127.0.0.1 und localhost – {host} wird weiter geprüft."
            );
        });
        true
    }
}

/// `127.0.0.1`, `localhost`, `::1` – die Adressen einer lokalen Brücke.
pub fn ist_lokaler_host(host: &str) -> bool {
    let host = host.trim().trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        // 127.0.0.0/8 und ::1 sind die Schleife; „127.0.0.1.example.test“
        // ist keine Adresse und damit auch nicht lokal.
        return ip.is_loopback();
    }
    host.eq_ignore_ascii_case("localhost")
}

/// Ein Schalter aus der Umgebung: `an`/`aus`, `true`/`false`, `1`/`0`, `ja`/`nein`.
/// Alles andere und ein leerer Wert gelten als [`standard`].
pub fn schalter(wert: Option<&str>, standard: bool) -> bool {
    match wert.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
        Some("an" | "ein" | "true" | "wahr" | "ja" | "yes" | "1") => true,
        Some("aus" | "false" | "falsch" | "nein" | "no" | "0") => false,
        _ => standard,
    }
}

/// Eine Zahl aus der Umgebung, hart begrenzt; ein unbrauchbarer Wert ist ein
/// Fehler mit Feldnamen, nie mit Wert.
pub fn zahl(
    wert: Option<&str>,
    standard: u16,
    feld: &str,
) -> Result<u16, crate::fehler::MailFehler> {
    let Some(wert) = wert.map(str::trim).filter(|wert| !wert.is_empty()) else {
        return Ok(standard);
    };
    wert.parse::<u16>().map_err(|_| {
        crate::fehler::MailFehler::Konfiguration(format!("{feld} „{wert}“ ist keine Portnummer"))
    })
}

/// Eine Anzahl aus einem Werkzeug-Parameter: mindestens 1, höchstens [`ANZAHL_MAX`].
pub fn anzahl(gewuenscht: Option<u32>) -> usize {
    gewuenscht
        .map(|wert| wert.clamp(1, ANZAHL_MAX as u32) as usize)
        .unwrap_or(ANZAHL_STANDARD)
}

/// Eine Adresse, wie sie in einer Kopfzeile steht.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Adresse {
    pub name: Option<String>,
    pub adresse: Option<String>,
}

impl Adresse {
    /// Die reine Mailadresse, leer, wenn keine gesetzt ist.
    pub fn mail(&self) -> String {
        self.adresse.clone().unwrap_or_default()
    }

    /// „Name \<mail\>“, sonst was da ist.
    pub fn anzeige(&self) -> String {
        match (
            self.name
                .as_deref()
                .map(str::trim)
                .filter(|t| !t.is_empty()),
            self.adresse
                .as_deref()
                .map(str::trim)
                .filter(|t| !t.is_empty()),
        ) {
            (Some(name), Some(mail)) => format!("{name} <{mail}>"),
            (Some(name), None) => name.to_string(),
            (None, Some(mail)) => mail.to_string(),
            (None, None) => String::new(),
        }
    }
}

/// Die Anzeigetexte einer Adressliste.
pub fn adressen_text(adressen: &[Adresse]) -> String {
    adressen
        .iter()
        .map(Adresse::anzeige)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Nur die Mailadressen einer Liste.
pub fn mails(adressen: &[Adresse]) -> Vec<String> {
    adressen
        .iter()
        .filter_map(|adresse| adresse.adresse.as_deref())
        .map(str::trim)
        .filter(|mail| !mail.is_empty())
        .map(str::to_string)
        .collect()
}

/// Ein Anhang, wie ihn die Liste nennt – ohne Inhalt.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Anhang {
    pub name: String,
    pub typ: String,
    /// Größe in Bytes.
    pub groesse: usize,
}

/// Eine Zeile aus `email_fetch` oder `email_search`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Kopfzeilen {
    pub uid: u32,
    pub von: String,
    pub an: String,
    pub cc: String,
    pub betreff: String,
    /// RFC 3339 mit Versatz, wenn die Kopfzeile ein Datum hatte.
    pub datum: Option<String>,
    /// IMAP-Flaggen wie `\Seen`.
    pub flaggen: Vec<String>,
}

/// Das Ergebnis einer Abfrage: die Zeilen und wie viele es insgesamt gibt.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KopfzeilenSeite {
    pub postfach: String,
    pub kopfzeilen: Vec<Kopfzeilen>,
    pub gesamt: usize,
}

/// Eine vollständig gelesene Nachricht.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Nachricht {
    pub uid: u32,
    pub von: Vec<Adresse>,
    /// `Reply-To`; gewinnt beim Antworten über `From`.
    pub antwort_an: Vec<Adresse>,
    pub an: Vec<Adresse>,
    pub cc: Vec<Adresse>,
    pub bcc: Vec<Adresse>,
    pub betreff: String,
    /// RFC 3339 mit Versatz.
    pub datum: Option<String>,
    pub message_id: Option<String>,
    pub antworten_auf: Vec<String>,
    pub referenzen: Vec<String>,
    /// Text, bei HTML-only aus dem HTML gewonnen und gekürzt erst in der Ausgabe.
    pub text: String,
    pub anhaenge: Vec<Anhang>,
}

/// Ein Postfach (Ordner) des Servers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PostfachInfo {
    /// Voller Pfad, z. B. `[Gmail]/Sent Mail`.
    pub pfad: String,
    /// Der letzte Teil des Pfades, wie ihn die Vorlage anzeigt.
    pub name: String,
    pub waehlbar: bool,
    /// Tiefe in der Hierarchie (0 = oberste Ebene).
    pub tiefe: usize,
    /// IMAP-Attribute wie `\Sent` oder `\Noselect`.
    pub attribute: Vec<String>,
}

impl PostfachInfo {
    /// Ein Postfach ohne Attribute, wie es Tests brauchen.
    pub fn einfach(pfad: &str) -> Self {
        let name = pfad.rsplit('/').next().unwrap_or(pfad).to_string();
        Self {
            pfad: pfad.to_string(),
            name,
            waehlbar: true,
            tiefe: pfad.matches('/').count(),
            attribute: Vec::new(),
        }
    }

    /// Ob der Server dieses Postfach als „Gesendet“ kennzeichnet (RFC 6154).
    pub fn ist_gesendet(&self) -> bool {
        self.attribut("\\Sent")
    }

    /// Ob der Server dieses Postfach als „Entwürfe“ kennzeichnet (RFC 6154).
    pub fn ist_entwuerfe(&self) -> bool {
        self.attribut("\\Drafts")
    }

    fn attribut(&self, name: &str) -> bool {
        self.attribute
            .iter()
            .any(|attribut| attribut.eq_ignore_ascii_case(name))
    }
}

/// Was eine Suche einschränkt. Alle Felder sind optional.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Suchkriterien {
    pub von: Option<String>,
    pub betreff: Option<String>,
    pub text: Option<String>,
    /// `YYYY-MM-DD`, einschließlich.
    pub seit: Option<String>,
    /// `YYYY-MM-DD`, ausschließlich.
    pub vor: Option<String>,
    pub ungelesen: bool,
}

impl Suchkriterien {
    /// Ob überhaupt etwas eingeschränkt ist.
    pub fn ist_leer(&self) -> bool {
        self.von.is_none()
            && self.betreff.is_none()
            && self.text.is_none()
            && self.seit.is_none()
            && self.vor.is_none()
            && !self.ungelesen
    }
}

/// Eine Nachricht, die verschickt werden soll.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VersandNachricht {
    pub von: Adresse,
    pub an: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub betreff: String,
    pub text: String,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
}

/// Was der Versand zurückmeldet: die Kennung und die Bytes für die Ablage.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VersandErgebnis {
    pub message_id: String,
    /// Die fertige RFC-822-Nachricht, wie sie auf die Leitung ging.
    pub roh: Vec<u8>,
}

/// Was das Löschen bewirkt hat.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoeschErgebnis {
    /// `true`, wenn die Nachricht endgültig entfernt wurde (UID EXPUNGE).
    pub endgueltig: bool,
}

/// Ein Flaggenname aus einem Werkzeug-Parameter als IMAP-Flagge.
///
/// „Seen“, „ungelesen“, `\Seen` und `\Flagged` kommen alle an; ein
/// unbekannter Name wird zu `\<Name>`, wie es die Vorlage macht.
///
/// Erlaubt sind nur Buchstaben, Ziffern und `$ _ - .` (höchstens 64 Zeichen):
/// der Name steht unmaskiert in einem IMAP-Befehl.
pub fn imap_flag(name: &str) -> Result<String, String> {
    let name = name.trim();
    let kern = name.strip_prefix('\\').unwrap_or(name);
    let gueltig = !kern.is_empty()
        && kern.len() <= 64
        && kern
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '$' | '_' | '-' | '.'));
    if !gueltig {
        return Err(
            "Ungültiger Name einer Flagge: erlaubt sind Buchstaben, Ziffern und $ _ - ."
                .to_string(),
        );
    }
    if name.starts_with('\\') {
        return Ok(name.to_string());
    }
    Ok(match name.to_ascii_lowercase().as_str() {
        "seen" | "read" | "gelesen" | "ungelesen" | "unread" => "\\Seen".to_string(),
        "flagged" | "starred" | "markiert" => "\\Flagged".to_string(),
        "answered" | "replied" | "beantwortet" => "\\Answered".to_string(),
        "draft" | "entwurf" => "\\Draft".to_string(),
        "deleted" | "geloescht" => "\\Deleted".to_string(),
        _ => format!("\\{name}"),
    })
}

/// Die Empfänger einer Antwort: `to` und `cc`, als reine Mailadressen.
///
/// `Reply-To` gewinnt über `From`; bei `allen` kommen die ursprünglichen
/// Empfänger als CC hinzu, ohne den Absender, das Reply-To-Ziel, die eigene
/// Adresse oder Doppelte.
pub fn antwort_empfaenger(
    nachricht: &Nachricht,
    eigene: &str,
    allen: bool,
) -> (Vec<String>, Vec<String>) {
    let ziel = if nachricht.antwort_an.is_empty() {
        mails(&nachricht.von)
    } else {
        mails(&nachricht.antwort_an)
    };

    if !allen {
        return (ziel, Vec::new());
    }

    let mut bekannt: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for adresse in ziel
        .iter()
        .map(String::as_str)
        .chain(mails(&nachricht.von).iter().map(String::as_str))
        .chain(std::iter::once(eigene))
    {
        if !adresse.trim().is_empty() {
            bekannt.insert(adresse.trim().to_ascii_lowercase());
        }
    }

    let mut cc = Vec::new();
    for adresse in mails(&nachricht.an).into_iter().chain(mails(&nachricht.cc)) {
        if bekannt.insert(adresse.trim().to_ascii_lowercase()) {
            cc.push(adresse);
        }
    }
    (ziel, cc)
}

/// Die Kette für `References`: die vorhandene Kette plus die `Message-ID` der
/// beantworteten Nachricht.
pub fn antwort_kette(nachricht: &Nachricht) -> Vec<String> {
    let mut kette = nachricht.referenzen.clone();
    if let Some(kennung) = nachricht.message_id.as_deref() {
        let kennung = kennung.trim();
        if !kennung.is_empty() && !kette.iter().any(|teil| teil.trim() == kennung) {
            kette.push(kennung.to_string());
        }
    }
    kette
}

/// „Re:“ davor, aber kein zweites auf einem schon beantworteten Betreff.
pub fn antwort_betreff(betreff: &str) -> String {
    let betreff = if betreff.trim().is_empty() {
        "(kein Betreff)"
    } else {
        betreff.trim()
    };
    if hat_vorsatz(betreff, &["re:", "aw:", "antw:"]) {
        betreff.to_string()
    } else {
        format!("Re: {betreff}")
    }
}

/// „Fwd:“ davor, aber kein zweites auf einer schon weitergeleiteten Nachricht.
pub fn weiter_betreff(betreff: &str) -> String {
    let betreff = if betreff.trim().is_empty() {
        "(kein Betreff)"
    } else {
        betreff.trim()
    };
    if hat_vorsatz(betreff, &["fwd:", "fw:", "wg:"]) {
        betreff.to_string()
    } else {
        format!("Fwd: {betreff}")
    }
}

fn hat_vorsatz(betreff: &str, vorsaetze: &[&str]) -> bool {
    let klein = betreff.to_ascii_lowercase();
    vorsaetze.iter().any(|vorsatz| klein.starts_with(vorsatz))
}

/// Der zitierte Ursprung unter einer Antwort (wie in der Vorlage).
pub fn zitat(text: &str, nachricht: &Nachricht) -> String {
    let zitiert = nachricht.text.replace('\n', "\n> ");
    format!(
        "{text}\n\n--- Ursprüngliche Nachricht ---\n> Von: {}\n> Datum: {}\n> Betreff: {}\n>\n> {zitiert}",
        adressen_text(&nachricht.von),
        nachricht.datum.as_deref().unwrap_or("-"),
        nachricht.betreff,
    )
}

/// Der Rumpf einer Weiterleitung, mit den Kopfzeilen der Ursprungsnachricht.
pub fn weitergeleitet(kommentar: Option<&str>, nachricht: &Nachricht) -> String {
    let mut teile: Vec<String> = Vec::new();
    if let Some(kommentar) = kommentar.map(str::trim).filter(|text| !text.is_empty()) {
        teile.push(kommentar.to_string());
        teile.push(String::new());
    }
    teile.push("---------- Weitergeleitete Nachricht ----------".to_string());
    teile.push(format!("Von: {}", adressen_text(&nachricht.von)));
    teile.push(format!(
        "Datum: {}",
        nachricht.datum.as_deref().unwrap_or("-")
    ));
    teile.push(format!("Betreff: {}", nachricht.betreff));
    teile.push(format!("An: {}", adressen_text(&nachricht.an)));
    let cc = adressen_text(&nachricht.cc);
    if !cc.is_empty() {
        teile.push(format!("CC: {cc}"));
    }
    if !nachricht.anhaenge.is_empty() {
        let namen: Vec<&str> = nachricht
            .anhaenge
            .iter()
            .map(|anhang| anhang.name.as_str())
            .collect();
        teile.push(format!("Anhänge: {}", namen.join(", ")));
    }
    teile.push(String::new());
    teile.push(nachricht.text.clone());
    teile.join("\n")
}

/// Der Rumpf eines Entwurfs (wie `email_draft_reply`: `X-Unsent`, Threading).
pub fn entwurf_roh(
    von: &Adresse,
    an: &[String],
    cc: &[String],
    betreff: &str,
    text: &str,
    in_reply_to: Option<&str>,
    references: &[String],
) -> Vec<u8> {
    let mut kopf: Vec<String> = vec![
        "X-Unsent: 1".to_string(),
        format!("From: {}", kopfzeile(&von.anzeige())),
        format!("To: {}", kopfzeile(&an.join(", "))),
    ];
    if !cc.is_empty() {
        kopf.push(format!("Cc: {}", kopfzeile(&cc.join(", "))));
    }
    kopf.push(format!("Subject: {}", kopfzeile(betreff)));
    if let Some(kennung) = in_reply_to.map(str::trim).filter(|id| !id.is_empty()) {
        kopf.push(format!("In-Reply-To: {}", kopfzeile(kennung)));
    }
    if !references.is_empty() {
        kopf.push(format!("References: {}", kopfzeile(&references.join(" "))));
    }
    kopf.push("MIME-Version: 1.0".to_string());
    kopf.push("Content-Type: text/plain; charset=UTF-8".to_string());
    kopf.push("Content-Transfer-Encoding: 8bit".to_string());

    let rumpf = text.replace("\r\n", "\n").replace('\n', "\r\n");
    format!("{}\r\n\r\n{rumpf}\r\n", kopf.join("\r\n")).into_bytes()
}

/// Ein Kopfzeilenwert ohne Zeilenumbruch (kein Header-Injection).
fn kopfzeile(wert: &str) -> String {
    wert.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adresse(mail: &str) -> Adresse {
        Adresse {
            name: None,
            adresse: Some(mail.to_string()),
        }
    }

    fn nachricht() -> Nachricht {
        Nachricht {
            uid: 42,
            von: vec![Adresse {
                name: Some("Ada".to_string()),
                adresse: Some("ada@example.test".to_string()),
            }],
            antwort_an: Vec::new(),
            an: vec![adresse("ich@example.test"), adresse("bob@example.test")],
            cc: vec![adresse("carla@example.test"), adresse("bob@example.test")],
            bcc: Vec::new(),
            betreff: "Bericht".to_string(),
            datum: Some("2026-09-02T10:11:12+02:00".to_string()),
            message_id: Some("<abc@example.test>".to_string()),
            antworten_auf: vec!["<vorher@example.test>".to_string()],
            referenzen: vec!["<vorher@example.test>".to_string()],
            text: "Zeile eins\nZeile zwei".to_string(),
            anhaenge: vec![Anhang {
                name: "bericht.pdf".to_string(),
                typ: "application/pdf".to_string(),
                groesse: 2048,
            }],
        }
    }

    #[test]
    fn sicherheit_kennt_die_drei_werte() {
        assert_eq!(Sicherheit::aus_text("tls"), Some(Sicherheit::Tls));
        assert_eq!(
            Sicherheit::aus_text(" STARTTLS "),
            Some(Sicherheit::Starttls)
        );
        assert_eq!(Sicherheit::aus_text("keine"), Some(Sicherheit::Keine));
        assert_eq!(Sicherheit::aus_text("ssl"), Some(Sicherheit::Tls));
        assert_eq!(Sicherheit::aus_text("quatsch"), None);
        assert_eq!(Sicherheit::Tls.wort(), "TLS");
    }

    #[test]
    fn schalter_und_zahlen_lesen_die_umgebung() {
        assert!(schalter(Some("an"), false));
        assert!(schalter(Some("true"), false));
        assert!(!schalter(Some("aus"), true));
        assert!(!schalter(None, false));
        assert!(schalter(None, true));
        assert_eq!(zahl(None, 993, "imap_port").unwrap(), 993);
        assert_eq!(zahl(Some(" 143 "), 993, "imap_port").unwrap(), 143);
        let fehler = zahl(Some("viele"), 993, "imap_port")
            .unwrap_err()
            .to_string();
        assert!(fehler.contains("imap_port"), "{fehler}");
        assert!(fehler.contains("keine Portnummer"), "{fehler}");
    }

    #[test]
    fn nur_lokale_hosts_duerfen_das_zertifikat_ignorieren() {
        // Der nicht-lokale Fall schreibt einmal einen Hinweis auf stderr; das
        // ist gewollt und steht in keiner Antwort.
        assert!(!Konto::prueft_zertifikat(false, "127.0.0.1"));
        assert!(!Konto::prueft_zertifikat(false, "localhost"));
        assert!(!Konto::prueft_zertifikat(false, "[::1]"));
        assert!(Konto::prueft_zertifikat(false, "imap.gmail.com"));
        assert!(Konto::prueft_zertifikat(true, "127.0.0.1"));
        assert!(ist_lokaler_host("127.0.0.2"));
        assert!(!ist_lokaler_host("gmail.com"));
        assert!(!ist_lokaler_host("127.0.0.1.example.test"));
    }

    #[test]
    fn anzahl_ist_begrenzt() {
        assert_eq!(anzahl(None), 20);
        assert_eq!(anzahl(Some(0)), 1);
        assert_eq!(anzahl(Some(20)), 20);
        assert_eq!(anzahl(Some(500)), 100);
    }

    #[test]
    fn antwort_geht_an_den_absender() {
        let (an, cc) = antwort_empfaenger(&nachricht(), "ich@example.test", false);
        assert_eq!(an, vec!["ada@example.test"]);
        assert!(cc.is_empty());
    }

    #[test]
    fn antwort_an_alle_laesst_sich_selbst_und_doppelte_weg() {
        let (an, cc) = antwort_empfaenger(&nachricht(), "ich@example.test", true);
        assert_eq!(an, vec!["ada@example.test"]);
        assert_eq!(cc, vec!["bob@example.test", "carla@example.test"]);
    }

    #[test]
    fn reply_to_gewinnt_ueber_from() {
        let mut nachricht = nachricht();
        nachricht.antwort_an = vec![adresse("liste@example.test")];
        let (an, _) = antwort_empfaenger(&nachricht, "ich@example.test", false);
        assert_eq!(an, vec!["liste@example.test"]);
        // Die eigene Adresse und der Absender landen auch bei „allen“ nicht im CC.
        let (_, cc) = antwort_empfaenger(&nachricht, "ich@example.test", true);
        assert_eq!(cc, vec!["bob@example.test", "carla@example.test"]);
    }

    #[test]
    fn betreff_bekommt_nur_ein_re_und_nur_ein_fwd() {
        assert_eq!(antwort_betreff("Bericht"), "Re: Bericht");
        assert_eq!(antwort_betreff("Re: Bericht"), "Re: Bericht");
        assert_eq!(antwort_betreff("AW: Bericht"), "AW: Bericht");
        assert_eq!(antwort_betreff("  "), "Re: (kein Betreff)");
        assert_eq!(weiter_betreff("Bericht"), "Fwd: Bericht");
        assert_eq!(weiter_betreff("Fwd: Bericht"), "Fwd: Bericht");
        assert_eq!(weiter_betreff("FW: Bericht"), "FW: Bericht");
    }

    #[test]
    fn die_kette_haengt_die_message_id_an() {
        let kette = antwort_kette(&nachricht());
        assert_eq!(kette, vec!["<vorher@example.test>", "<abc@example.test>"]);

        let mut nachricht = nachricht();
        nachricht.referenzen.clear();
        assert_eq!(antwort_kette(&nachricht), vec!["<abc@example.test>"]);
    }

    #[test]
    fn zitat_und_weiterleitung_nennen_den_ursprung() {
        let nachricht = nachricht();
        let zitat = zitat("Danke", &nachricht);
        assert!(zitat.contains("> Von: Ada <ada@example.test>"), "{zitat}");
        assert!(zitat.contains("> Zeile eins\n> Zeile zwei"), "{zitat}");

        let weiter = weitergeleitet(Some("FYI"), &nachricht);
        assert!(weiter.starts_with("FYI\n\n----------"), "{weiter}");
        assert!(weiter.contains("Anhänge: bericht.pdf"), "{weiter}");
        assert!(weiter.contains("Zeile eins"), "{weiter}");
    }

    #[test]
    fn der_entwurf_hat_threading_und_x_unsent() {
        let roh = entwurf_roh(
            &Adresse {
                name: Some("Ich".to_string()),
                adresse: Some("ich@example.test".to_string()),
            },
            &["ada@example.test".to_string()],
            &[],
            "Re: Bericht",
            "Danke\nZeile zwei",
            Some("<abc@example.test>"),
            &["<abc@example.test>".to_string()],
        );
        let text = String::from_utf8(roh).unwrap();
        assert!(text.starts_with("X-Unsent: 1\r\n"), "{text}");
        assert!(text.contains("From: Ich <ich@example.test>"), "{text}");
        assert!(text.contains("To: ada@example.test"), "{text}");
        assert!(text.contains("In-Reply-To: <abc@example.test>"), "{text}");
        assert!(text.contains("References: <abc@example.test>"), "{text}");
        assert!(text.contains("\r\n\r\nDanke\r\nZeile zwei\r\n"), "{text}");
        assert!(!text.contains("Cc:"), "{text}");
    }

    #[test]
    fn kopfzeilen_werden_nicht_gespalten() {
        let roh = entwurf_roh(
            &Adresse {
                name: None,
                adresse: Some("ich@example.test".to_string()),
            },
            &["ada@example.test".to_string()],
            &[],
            "Betreff\r\nBcc: boese@example.test",
            "Text",
            None,
            &[],
        );
        let text = String::from_utf8(roh).unwrap();
        assert!(!text.contains("\r\nBcc:"), "{text}");
        assert!(
            text.contains("Subject: Betreff Bcc: boese@example.test"),
            "{text}"
        );
    }

    #[test]
    fn flaggennamen_werden_zu_imap_flaggen() {
        assert_eq!(imap_flag("Seen").unwrap(), "\\Seen");
        assert_eq!(imap_flag("ungelesen").unwrap(), "\\Seen");
        assert_eq!(imap_flag("Starred").unwrap(), "\\Flagged");
        assert_eq!(imap_flag("replied").unwrap(), "\\Answered");
        assert_eq!(imap_flag("\\Deleted").unwrap(), "\\Deleted");
        assert_eq!(imap_flag("$Label1").unwrap(), "\\$Label1");
        for boese in [
            "",
            "\\",
            "\\Seen)\r\nA1 NOOP",
            "a b",
            "a)",
            "(a",
            "a\nb",
            "a\"b",
            "ä",
        ] {
            assert!(imap_flag(boese).is_err(), "{boese:?}");
        }
        assert!(imap_flag(&"x".repeat(65)).is_err());
    }

    #[test]
    fn debug_zeigt_kein_passwort() {
        let konto = Konto {
            adresse: "ich@example.test".to_string(),
            name: None,
            imap: ImapKonfiguration {
                host: "imap.example.test".to_string(),
                port: 993,
                sicherheit: Sicherheit::Tls,
                benutzer: "ich@example.test".to_string(),
                passwort: "nur-im-test-4711".to_string(),
                zertifikat_pruefen: true,
            },
            smtp: SmtpKonfiguration {
                host: "smtp.example.test".to_string(),
                port: 587,
                sicherheit: Sicherheit::Starttls,
                benutzer: "ich@example.test".to_string(),
                passwort: "nur-im-test-4711".to_string(),
                zertifikat_pruefen: true,
            },
        };
        let text = format!("{konto:?}");
        assert!(text.contains("imap.example.test"), "{text}");
        assert!(!text.contains("nur-im-test-4711"), "{text}");
    }
}
