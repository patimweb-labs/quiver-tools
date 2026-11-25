// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Eine absichtlich **kaputte** Attrappe: sie antwortet mit einem zu kurzen
//! Rumpf und schließt die Verbindung.
//!
//! Damit lässt sich prüfen, was passiert, wenn `text().await` scheitert – beim
//! Lesen (Aufgabe 15 b, Regel 3: „ein Fehler, nie ‚leer‘“) und nach einer
//! Änderung („ausgeführt, Antwort nicht lesbar“). Kein Test spricht je mit einem
//! echten Zephyr.

#![allow(dead_code)]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Was kaputt ist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Art {
    /// Jede lesende Antwort ist abgeschnitten.
    Lesen,
    /// Lesen klappt; die Antwort auf die Änderung (`PUT`) ist abgeschnitten.
    Aenderung,
    /// Lesen klappt; auf die Änderung (`PUT`) kommt **gar keine** Antwort.
    AenderungOhneAntwort,
}

/// Eine laufende kaputte Attrappe.
pub struct Defekt {
    /// `http://127.0.0.1:<port>` – ohne Schrägstrich am Ende.
    pub adresse: String,
    ende: tokio::task::JoinHandle<()>,
}

impl Drop for Defekt {
    fn drop(&mut self) {
        self.ende.abort();
    }
}

impl Defekt {
    /// Die kaputte Attrappe auf einem freien Port starten.
    pub async fn starte(art: Art) -> Defekt {
        let hoerer =
            tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
                .await
                .expect("kein freier Port");
        let adresse = format!("http://127.0.0.1:{}", hoerer.local_addr().unwrap().port());
        let ende = tokio::spawn(async move {
            loop {
                let Ok((mut strom, _)) = hoerer.accept().await else {
                    break;
                };
                let _ = bedienen(&mut strom, art).await;
            }
        });
        Defekt { adresse, ende }
    }

    /// Die Basisadresse inklusive `/v2`, wie sie im Zugang steht.
    pub fn basis_url(&self) -> String {
        format!("{}/v2", self.adresse)
    }
}

/// Volle Antwort mit `content-length`, die zur Länge passt.
fn heil(status: &str, rumpf: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\
         connection: close\r\n\r\n{rumpf}",
        rumpf.len()
    )
}

/// Antwort, deren `content-length` **größer** ist als der Rumpf: der Client
/// bricht beim Lesen ab.
fn kaputt(status: &str, rumpf: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\
         connection: close\r\n\r\n{rumpf}",
        rumpf.len() + 40
    )
}

async fn bedienen(strom: &mut tokio::net::TcpStream, art: Art) -> std::io::Result<()> {
    let mut puffer = vec![0u8; 8192];
    let mut gelesen = Vec::new();
    loop {
        let anzahl = strom.read(&mut puffer).await?;
        if anzahl == 0 {
            return Ok(());
        }
        gelesen.extend_from_slice(&puffer[..anzahl]);
        if gelesen.windows(4).any(|fenster| fenster == b"\r\n\r\n") {
            break;
        }
    }
    let kopf = String::from_utf8_lossy(&gelesen).to_string();
    let erste = kopf.lines().next().unwrap_or_default();
    let mut teile = erste.split_whitespace();
    let methode = teile.next().unwrap_or_default();
    let pfad = teile.next().unwrap_or_default();

    let antwort = match art {
        // Alles Lesende ist kaputt.
        Art::Lesen => kaputt("200 OK", "{\"values\": ["),
        // Die Verbindung endet ohne jede Antwort: der Ausgang ist offen.
        Art::AenderungOhneAntwort if methode == "PUT" => {
            let _ = strom.shutdown().await;
            return Ok(());
        }
        Art::Aenderung | Art::AenderungOhneAntwort => {
            match (methode, pfad.split('?').next().unwrap_or(pfad)) {
                ("PUT", _) => kaputt("200 OK", "{\"id\": 101,"),
                ("GET", pfad)
                    if pfad == "/v2/testexecutions/101" || pfad == "/v2/testexecutions/QA-E1" =>
                {
                    heil("200 OK", AUSFUEHRUNG)
                }
                ("GET", "/v2/projects/1") => heil("200 OK", PROJEKT),
                ("GET", "/v2/statuses") => heil("200 OK", STATUS),
                _ => heil("404 Not Found", "{\"message\":\"gibt es nicht\"}"),
            }
        }
    };
    strom.write_all(antwort.as_bytes()).await?;
    strom.flush().await?;
    // Der Rumpf ist zu kurz: die Verbindung endet mittendrin.
    let _ = strom.shutdown().await;
    Ok(())
}

/// Eine Ausführung mit Verweisen auf Projekt, Status und Testfall.
const AUSFUEHRUNG: &str = r#"{"id":101,"key":"QA-E1",
"project":{"id":1,"self":"https://zephyr.example.test/v2/projects/1"},
"testCase":{"id":11,"self":"https://zephyr.example.test/v2/testcases/QA-T1/versions/1"},
"testExecutionStatus":{"id":1,"self":"https://zephyr.example.test/v2/statuses/1"},
"automated":false}"#;

/// Ein Projekt, wie `GET /projects/{id-or-key}` es liefert.
const PROJEKT: &str = r#"{"id":1,"jiraProjectId":10000,"key":"QA","enabled":true}"#;

/// Die Statusliste von `QA` mit genau einem Status.
const STATUS: &str = r#"{"startAt":0,"maxResults":100,"total":1,"isLast":true,"next":null,
"values":[{"id":1,"project":{"id":1,"self":"https://zephyr.example.test/v2/projects/1"},
"name":"Pass","index":0,"default":true,"archived":false}]}"#;
