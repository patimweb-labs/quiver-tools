#!/usr/bin/env bash
# Jedes Programm startet auf seinem System (Aufgabe 38).
#
#   verteilung/pruefen.sh [ordner | archiv.tar.gz]
#
# Ohne Argument wird `target/release` geprüft (bzw. `$CARGO_TARGET_DIR`). Mit
# einem Archiv wird es in einen temporären Ordner entpackt, `LISTE.json` gegen
# die Programme gehalten und dann dasselbe geprüft.
#
# Geprüft wird je Programm:
#   1. `--version` nennt Name und Version.
#   2. `eintrag` gibt den Bibliothekseintrag als JSON aus (Exit 0).
#   3. MCP über stdin: `initialize` und `tools/list` antworten.
#
# Punkt 3 ist der eigentliche Startbeweis: das Programm spricht über Pipes, ohne
# Konsole und ohne Zugang. Fehlt der Zugang, antworten die Werkzeuge mit dem
# Hinweis — der Start scheitert daran nicht.
set -euo pipefail

wurzel=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$wurzel"

# Frist für den MCP-Teil. `timeout` gibt es nicht überall (Windows, macOS ohne
# coreutils), deshalb ein eigener Wächter weiter unten.
frist=30

anfragen='
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"release-pruefung","version":"0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/list"}
'

aufraeumen() {
    [ -n "${entpackt:-}" ] && rm -rf "$entpackt"
    return 0
}
trap aufraeumen EXIT

quelle=${1:-${CARGO_TARGET_DIR:-target}/release}

case "$quelle" in
    *.tar.gz)
        archiv=$(cd "$(dirname "$quelle")" && pwd)/$(basename "$quelle")
        entpackt=$(mktemp -d)
        tar -xzf "$archiv" -C "$entpackt"
        pruefort=$entpackt

        # `LISTE.json` muss dasselbe sagen wie die Programme daneben: je
        # Programm eine Zeile mit Name und Summe, keine Lücke in beide
        # Richtungen.
        if [ ! -f "$pruefort/LISTE.json" ]; then
            echo "FEHLER: im Archiv fehlt LISTE.json." >&2
            exit 1
        fi
        dateien=$(find "$pruefort" -maxdepth 1 -type f -name 'quiver-*' | wc -l | tr -d ' ')
        # `|| true`: `grep -c` endet mit 1, wenn nichts passt — das soll hier
        # kein stiller Abbruch sein, sondern der Vergleich unten.
        gelistet=$(grep -c '"sha256"' "$pruefort/LISTE.json" || true)
        gelistet=$(printf '%s' "$gelistet" | tr -d ' ')
        if [ "$dateien" -ne "$gelistet" ]; then
            echo "FEHLER: $dateien Programme im Archiv, $gelistet in LISTE.json." >&2
            exit 1
        fi
        for programm in "$pruefort"/quiver-*; do
            [ -f "$programm" ] || continue
            name=$(basename "$programm" .exe)
            summe=$(verteilung/summe.sh "$programm")
            if ! grep -q "\"name\": \"$name\".*\"sha256\": \"$summe\"" "$pruefort/LISTE.json"; then
                echo "FEHLER: $name fehlt in LISTE.json oder die Summe stimmt nicht." >&2
                exit 1
            fi
        done
        echo "==> $archiv: $dateien Programme, Summen stimmen"
        ;;
    *)
        pruefort=$quelle
        ;;
esac

if [ ! -d "$pruefort" ]; then
    echo "FEHLER: $pruefort gibt es nicht." >&2
    exit 1
fi

anzahl=0
for programm in "$pruefort"/quiver-*; do
    [ -f "$programm" ] || continue
    case "$programm" in
        *.d) continue ;;
    esac
    if [ ! -x "$programm" ] && [ "${programm##*.}" != exe ]; then
        continue
    fi

    name=$(basename "$programm" .exe)
    anzahl=$((anzahl + 1))

    # 1. Version. Das Programm meldet sich mit seinem Werkzeugnamen — beim einen
    #    „abap“, beim anderen „quiver-m365“; beides ist gemeint.
    kurz=${name#quiver-}
    if ! version_ausgabe=$("$programm" --version 2>/dev/null); then
        echo "FEHLER: $name antwortet nicht auf --version." >&2
        exit 1
    fi
    gemeldet=${version_ausgabe%% *}
    if [ "$gemeldet" != "$kurz" ] && [ "$gemeldet" != "$name" ]; then
        echo "FEHLER: $name meldet sich als: $version_ausgabe" >&2
        exit 1
    fi

    # 2. Bibliothekseintrag
    if ! eintrag=$("$programm" eintrag 2>/dev/null); then
        echo "FEHLER: $name gibt keinen Eintrag aus." >&2
        exit 1
    fi
    # `case` statt `printf | grep -q`: mit `pipefail` gilt ein früher
    # abbrechendes `grep -q` (SIGPIPE im `printf`) als Fehler — je nach Größe
    # des Eintrags mal so, mal so.
    for feld in name titel laufzeit; do
        case "$eintrag" in
            *"\"$feld\""*) ;;
            *)
                echo "FEHLER: $name: im Eintrag fehlt \"$feld\"." >&2
                exit 1
                ;;
        esac
    done

    # 3. MCP über Pipes
    eingabe=$(mktemp)
    ausgabe=$(mktemp)
    fehler=$(mktemp)
    marke=$(mktemp)
    rm -f "$marke"
    printf '%s\n' "$anfragen" >"$eingabe"

    "$programm" <"$eingabe" >"$ausgabe" 2>"$fehler" &
    pid=$!
    (
        sleep "$frist"
        : >"$marke"
        kill -9 "$pid" 2>/dev/null
    ) &
    waechter=$!

    status=0
    wait "$pid" || status=$?

    kill "$waechter" 2>/dev/null || true
    wait "$waechter" 2>/dev/null || true

    if [ -f "$marke" ]; then
        echo "FEHLER: $name antwortet nach $frist s nicht auf MCP." >&2
        rm -f "$eingabe" "$ausgabe" "$fehler" "$marke"
        exit 1
    fi
    if [ "$status" -ne 0 ]; then
        echo "FEHLER: $name endet beim MCP-Start mit $status:" >&2
        cat "$fehler" >&2
        rm -f "$eingabe" "$ausgabe" "$fehler" "$marke"
        exit 1
    fi
    if ! grep -q '"serverInfo"' "$ausgabe"; then
        echo "FEHLER: $name antwortet nicht auf initialize:" >&2
        cat "$ausgabe" >&2
        rm -f "$eingabe" "$ausgabe" "$fehler" "$marke"
        exit 1
    fi
    if ! grep -q '"tools":\[{' "$ausgabe"; then
        echo "FEHLER: $name antwortet nicht auf tools/list:" >&2
        cat "$ausgabe" >&2
        rm -f "$eingabe" "$ausgabe" "$fehler" "$marke"
        exit 1
    fi
    rm -f "$eingabe" "$ausgabe" "$fehler" "$marke"

    echo "==> $name: $version_ausgabe"
done

if [ "$anzahl" -eq 0 ]; then
    echo "FEHLER: in $pruefort liegt kein Programm." >&2
    exit 1
fi

echo "==> $anzahl Programme gestartet, alle gesund"
