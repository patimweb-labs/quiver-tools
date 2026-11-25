#!/usr/bin/env bash
# macOS: die Programme ad hoc signieren (Aufgabe 38).
#
#   verteilung/macos-signieren.sh <ordner>
#
# Ohne Apple-Konto gibt es keine Entwickler-Signatur; ad hoc (`-`) genügt, damit
# das Programm auf demselben Gerät startet. Gatekeeper fragt beim ersten Start
# trotzdem nach — das steht so im Release-Text.
set -euo pipefail

ordner=${1:?Ordner mit den Programmen fehlt}

if [ "$(uname -s)" != Darwin ]; then
    echo "FEHLER: codesign gibt es nur unter macOS." >&2
    exit 1
fi

for programm in "$ordner"/*; do
    [ -f "$programm" ] || continue
    codesign --force --sign - "$programm"
    codesign --verify --strict "$programm"
done
echo "==> Programme in $ordner ad hoc signiert"
