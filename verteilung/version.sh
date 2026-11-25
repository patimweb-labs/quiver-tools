#!/usr/bin/env bash
# Die Version des Arbeitsbereichs — eine Quelle für `paket.sh` und für
# `release.yml` (dort muss der Tag dazu passen).
#
# Gelesen wird `[workspace.package] version`; die Kisten erben sie
# (`version.workspace = true`), angehoben wird sie also nur an einer Stelle.
set -euo pipefail

wurzel=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

version=$(
    sed -n '/^\[workspace\.package\]/,/^\[/p' "$wurzel/Cargo.toml" |
        sed -n 's/^version = "\([^"]*\)"/\1/p' |
        head -1
)

if [ -z "$version" ]; then
    echo "FEHLER: keine Version in $wurzel/Cargo.toml ([workspace.package])" >&2
    exit 1
fi

printf '%s\n' "$version"
