#!/usr/bin/env node
// Erzeugt docs/bilder/einrichtung-{dark,light}.png aus der **echten** Seite
// (AGENTS.md §7: Bilder werden erzeugt, nicht von Hand abgelegt).
//
// Die Seite kommt aus `crates/kern/examples/demo.rs` mit
// `QUIVER_TOOLS_KEIN_BROWSER=1`: sie öffnet kein Fenster, sondern gibt ihre
// Adresse auf stdout aus. Angesehen wird sie mit Playwright – geliehen aus
// mcp-quiver, damit dieses Projekt keine Node-Abhängigkeit bekommt.
//
//   node docs/bilder/erzeugen.mjs
//   PLAYWRIGHT_PFAD=/pfad/zu/playwright node docs/bilder/erzeugen.mjs

import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdir, mkdtemp, readdir, rm, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import path from "node:path";

const WURZEL = path.resolve(import.meta.dirname, "../..");
const BILDER = path.join(WURZEL, "docs", "bilder");
const PLAYWRIGHT =
  process.env.PLAYWRIGHT_PFAD ??
  path.join(
    process.env.HOME ?? ".",
    "Documents",
    "Workspace",
    "mcp-quiver",
    "admin",
    "node_modules",
    "playwright",
  );

const require = createRequire(import.meta.url);
const { chromium } = require(PLAYWRIGHT);

/**
 * Der Chromium, den Playwright nimmt. Passt die installierte Playwright-Fassung
 * nicht zum heruntergeladenen Browser, wird der neueste gefundene genommen –
 * oder der aus CHROME_PFAD.
 */
async function chromPfad() {
  if (process.env.CHROME_PFAD) return process.env.CHROME_PFAD;
  const lager = path.join(
    process.env.HOME ?? ".",
    "Library",
    "Caches",
    "ms-playwright",
  );
  let eintraege = [];
  try {
    eintraege = await readdir(lager);
  } catch {
    return undefined;
  }
  const kandidaten = eintraege
    .filter((name) => name.startsWith("chromium-"))
    .sort((a, b) => Number(b.split("-")[1]) - Number(a.split("-")[1]));
  for (const name of kandidaten) {
    const pfade = [
      path.join(
        lager,
        name,
        "chrome-mac-arm64",
        "Google Chrome for Testing.app",
        "Contents",
        "MacOS",
        "Google Chrome for Testing",
      ),
      path.join(lager, name, "chrome-linux", "chrome"),
      path.join(lager, name, "chrome-win", "chrome.exe"),
    ];
    const treffer = pfade.find((pfad) => existsSync(pfad));
    if (treffer) return treffer;
  }
  return undefined;
}

// Zwei Beispielprofile, damit die Profilliste und die Aktionsspalte zu sehen
// sind: eines ist Standard, eines nicht.
const PROFILE = `standard = "test"

[[profil]]
name = "arbeit"
beschreibung = "Wiki im Büro"

[profil.werte]
url = "https://wiki.example.test"
umgebung = "produktion"
streng = "true"

[[profil]]
name = "test"
beschreibung = "Zum Ausprobieren"

[profil.werte]
url = "https://test.example.test"
umgebung = "test"
streng = "true"
`;

/** Startet das Beispiel und liefert die Adresse der Seite. */
async function seiteStarten(ordner) {
  const kind = spawn(
    "cargo",
    [
      "run",
      "--quiet",
      "-p",
      "quiver-tools-kern",
      "--example",
      "demo",
      "--",
      "einrichten",
    ],
    {
      cwd: WURZEL,
      env: {
        ...process.env,
        QUIVER_TOOLS_KEIN_BROWSER: "1",
        QUIVER_TOOLS_TRESOR: "speicher",
        QUIVER_TOOLS_DIR: ordner,
      },
      stdio: ["ignore", "pipe", "inherit"],
    },
  );

  const adresse = await new Promise((fertig, scheitern) => {
    let gelesen = "";
    kind.stdout.on("data", (stueck) => {
      gelesen += stueck.toString();
      const zeile = gelesen.split("\n")[0];
      if (zeile.startsWith("http://")) fertig(zeile.trim());
    });
    kind.on("exit", (code) =>
      scheitern(new Error(`die Seite endete vorzeitig (Code ${code})`)),
    );
  });
  return { kind, adresse };
}

const ordner = await mkdtemp(path.join(tmpdir(), "quiver-bild-"));
// Das Beispiel heißt `demo`: seine Profile liegen unter <ordner>/demo/.
await mkdir(path.join(ordner, "demo"), { recursive: true });
await writeFile(path.join(ordner, "demo", "profile.toml"), PROFILE, {
  mode: 0o600,
});

const { kind, adresse } = await seiteStarten(ordner);
const browser = await chromium.launch({ executablePath: await chromPfad() });

try {
  for (const thema of ["dark", "light"]) {
    const seite = await browser.newPage({
      viewport: { width: 1200, height: 1000 },
      deviceScaleFactor: 2,
    });
    // Die Seite liest das Thema beim Laden aus dem localStorage.
    await seite.addInitScript(
      (wahl) => localStorage.setItem("quiver-thema", wahl),
      thema,
    );
    // Die Liste und das Formular zugleich: so ist die Aktionsspalte der
    // Profilliste und ein ausgefülltes Formular im Bild.
    await seite.goto(`${adresse}bearbeiten?name=arbeit`, {
      waitUntil: "networkidle",
    });
    await seite.evaluate((wahl) => {
      document.documentElement.dataset.theme = wahl;
    }, thema);
    const ziel = path.join(BILDER, `einrichtung-${thema}.png`);
    await seite.screenshot({ path: ziel, fullPage: true });
    console.log(`geschrieben: ${path.relative(WURZEL, ziel)}`);
    await seite.close();
  }
} finally {
  await browser.close();
  kind.kill();
  await rm(ordner, { recursive: true, force: true });
}
