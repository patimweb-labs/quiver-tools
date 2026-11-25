#!/usr/bin/env bash
# Was dieser Lauf mit Tag und Release tun soll.
#
#   verteilung/release-ziel.sh
#
# Liest die Umgebung, die `release.yml` setzt, und schreibt `schlüssel=wert`
# auf die Standardausgabe; der Workflow hängt das an `$GITHUB_OUTPUT`:
#
#   version=0.1.0             aus `[workspace.package]`
#   tag=v0.1.0                leer, wenn dieser Lauf nichts veröffentlicht
#   veroeffentlichen=ja|nein
#
# Alles Lesbare (was der Lauf vorhat, warum er abbricht) geht auf die
# Standardausgabe **nicht** — nur die drei Zeilen oben, sonst landete Prosa in
# `$GITHUB_OUTPUT`.
#
# Umgebung:
#   EREIGNIS      github.event_name        push | pull_request | workflow_dispatch
#   REF           github.ref               refs/heads/main | refs/tags/v0.1.0 | …
#   RP_RELEASED   release-please: release_created
#   RP_TAG        release-please: tag_name
#   REPOSITORY    owner/repo              für die Frage, ob es das Release schon gibt
#   GH_TOKEN      Token für diese Frage (nur Lesen genügt)
#
# Die Regeln:
#
#   Push auf einen Tag vX.Y.Z   bauen und veröffentlichen (der Weg von Hand).
#   Push auf main, und
#   release-please hat gerade
#   ein Release angelegt        dessen Tag bauen und veröffentlichen. Das muss in
#                               diesem Lauf passieren: einen Tag, den
#                               `GITHUB_TOKEN` anlegt, löst keinen neuen
#                               Workflow aus.
#   Alles andere                nur bauen (Push auf main ohne Release, Pull
#                               Request, „Run workflow“).
#
# Zwei Prüfungen kommen dazu:
#
#   1. Der Tag muss `v<version>` aus `[workspace.package]` sein — sonst hieße
#      das Release anders als die Programme in ihrer eigenen Antwort.
#   2. Gibt es zu dem Tag schon ein Release **mit Dateien**, ist diese Version
#      veröffentlicht. Dann bricht der Lauf ab, statt sie neu zu bauen und mit
#      `--clobber` zu ersetzen: dieselbe Version ein zweites Mal ist ein Fehler.
#      Ein Release **ohne** Dateien (release-please hat es gerade angelegt, der
#      Bau ist vorher gescheitert) wird ergänzt.
set -euo pipefail

wurzel=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

# Meldungen auf die Standardfehlerausgabe: die Standardausgabe gehört den
# `schlüssel=wert`-Zeilen.
sag() { printf '%s\n' "$*" >&2; }

ereignis=${EREIGNIS:-}
ref=${REF:-}
rp_veroeffentlicht=${RP_RELEASED:-}
rp_tag=${RP_TAG:-}
repo=${REPOSITORY:-}

version=$("$wurzel/verteilung/version.sh")

tag=
if [ "$ereignis" = push ]; then
    case "$ref" in
        refs/tags/*)
            tag=${ref#refs/tags/}
            ;;
        refs/heads/main)
            # Nur wenn release-please in diesem Lauf Tag und Release angelegt
            # hat; sonst ist dieser Push nur ein Release-PR-Update.
            if [ "$rp_veroeffentlicht" = true ]; then
                tag=$rp_tag
            fi
            ;;
    esac
fi

if [ -n "$tag" ] && [ "$tag" != "v$version" ]; then
    sag "::error::Tag »$tag« passt nicht zu »v$version« aus Cargo.toml."
    exit 1
fi

if [ -n "$tag" ]; then
    if [ -z "$repo" ]; then
        sag "::error::REPOSITORY fehlt — ob es das Release schon gibt, lässt sich nicht prüfen."
        exit 1
    fi

    # `gh api`: ein fehlendes Release endet mit 404 und der Meldung „Not
    # Found“. Alles andere (Rechte, Netz) ist ein echter Fehler und darf nicht
    # als „noch nicht veröffentlicht“ durchgehen.
    fehler=$(mktemp)
    trap 'rm -f "$fehler"' EXIT

    if anhaengsel=$(gh api "repos/$repo/releases/tags/$tag" \
        --jq '[.assets[].name] | length' 2>"$fehler"); then
        :
    elif grep -q 'Not Found\|HTTP 404' "$fehler"; then
        anhaengsel=keiner
    else
        sag "::error::Release zu »$tag« lässt sich nicht lesen:"
        sed 's/^/    /' "$fehler" >&2
        exit 1
    fi

    case "$anhaengsel" in
        keiner)
            sag "==> $tag: noch kein Release, wird angelegt."
            ;;
        0)
            sag "==> $tag: Release besteht ohne Dateien, wird ergänzt."
            ;;
        *)
            sag "::error::Release »$tag« trägt bereits $anhaengsel Dateien — diese Version ist veröffentlicht."
            sag "    Sie wird nicht neu gebaut und nicht überschrieben (kein --clobber)."
            sag "    Für eine neue Fassung die Version anheben; der Release-PR von"
            sag "    release-please macht das aus den Conventional Commits."
            exit 1
            ;;
    esac
fi

printf 'version=%s\n' "$version"
printf 'tag=%s\n' "$tag"
if [ -n "$tag" ]; then
    printf 'veroeffentlichen=ja\n'
    sag "==> Version $version, Tag $tag: bauen und veröffentlichen."
else
    printf 'veroeffentlichen=nein\n'
    sag "==> Version $version, kein Tag: nur bauen."
fi
