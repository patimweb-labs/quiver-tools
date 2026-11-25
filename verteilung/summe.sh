#!/usr/bin/env bash
# SHA-256 einer Datei, mit dem Werkzeug, das es auf diesem System gibt.
#
#   verteilung/summe.sh <datei>
#
# Linux und Windows (Git Bash) haben meist `sha256sum`, macOS `shasum -a 256`,
# und `openssl` gibt es überall — die Reihenfolge ist die der Verfügbarkeit.
# Alle drei schreiben „<summe>  <datei>“ bzw. „<summe> *<datei>“; ausgegeben
# wird nur die Summe.
set -euo pipefail

datei=${1:?Datei fehlt}

if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$datei" | cut -d' ' -f1
elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$datei" | cut -d' ' -f1
elif command -v openssl >/dev/null 2>&1; then
    openssl dgst -sha256 "$datei" | sed 's/.*= //'
else
    echo "FEHLER: weder sha256sum noch shasum noch openssl gefunden" >&2
    exit 1
fi
