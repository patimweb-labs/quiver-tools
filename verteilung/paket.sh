#!/usr/bin/env bash
# Die Programme eines Systems als Archiv (Aufgabe 38).
#
#   verteilung/paket.sh
#
# Baut alle `quiver-*`-Programme aus dem Arbeitsbereich, strippt sie, signiert
# sie unter macOS ad hoc und legt daraus
#
#   dist/quiver-tools-<version>-<system>-<arch>.tar.gz
#
# ab. Im Archiv liegen die Programme flach nebeneinander und `LISTE.json`
# (Name, Version, Größe, SHA-256 je Programm).
#
# Gebaut wird **nativ** auf dem jeweiligen Läufer, nicht über Kreuz: Rust unter
# Emulation braucht eine Stunde statt Minuten. Deshalb bestimmt das Skript
# System und Architektur aus `uname` — auf dem Läufer stimmt das, und lokal
# entsteht dasselbe Archiv wie in der CI.
#
# `CARGO_TARGET_DIR` wird beachtet (die Aufgaben hier bauen nach `target-pi/`),
# sonst gilt `target/`.
set -euo pipefail

wurzel=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$wurzel"

version=$(verteilung/version.sh)
ziel=${CARGO_TARGET_DIR:-target}
dist="$wurzel/dist"

# System und Architektur so benennen, wie das Archiv sie trägt.
case "$(uname -s)" in
    Darwin) system=macos ;;
    Linux) system=linux ;;
    MINGW* | MSYS* | CYGWIN*) system=windows ;;
    *)
        echo "FEHLER: unbekanntes System: $(uname -s)" >&2
        exit 1
        ;;
esac

case "$(uname -m)" in
    arm64 | aarch64) arch=aarch64 ;;
    x86_64 | amd64) arch=x86_64 ;;
    *)
        echo "FEHLER: unbekannte Architektur: $(uname -m)" >&2
        exit 1
        ;;
esac

exe=
[ "$system" = windows ] && exe=.exe

archiv="quiver-tools-$version-$system-$arch.tar.gz"
zwischen="$ziel/paket/$system-$arch"

echo "==> $archiv"

# Windows ohne zusätzliche Laufzeit-Abhängigkeiten: mit statischer C-Runtime
# braucht das Programm weder das VC++-Redistributable noch MSYS- oder
# Cygwin-DLLs. Die Konsole bleibt der Subsystem-Typ — MCP läuft über
# stdin/stdout, ein „windows“-Subsystem würde beides kappen.
if [ "$system" = windows ]; then
    export RUSTFLAGS="${RUSTFLAGS:-} -C target-feature=+crt-static"
fi

echo "==> Bauen (release, --locked)"
cargo build --release --locked --workspace --bins

# Die Programme einsammeln: die Kisten heißen wie ihr Programm, also genügt das
# Präfix. Zwischendateien von Cargo (`*.d`) und Windows-Beiwerk (`*.pdb`)
# fallen dabei heraus.
programme=()
for datei in "$ziel"/release/quiver-*; do
    [ -f "$datei" ] || continue
    case "$datei" in
        *.d) continue ;;
    esac
    if [ -n "$exe" ]; then
        case "$datei" in
            *"$exe") ;;
            *) continue ;;
        esac
    else
        [ -x "$datei" ] || continue
    fi
    programme+=("$datei")
done

# Ein Programm, das nicht `quiver-` heißt, fehlt hier still — deshalb der
# Abgleich mit den `[[bin]]`-Abschnitten des Arbeitsbereichs.
erwartet=$(grep -l '^\[\[bin\]\]' crates/*/Cargo.toml | wc -l | tr -d ' ')
gefunden=${#programme[@]}
if [ "$gefunden" -ne "$erwartet" ]; then
    echo "FEHLER: $gefunden Programme gefunden, $erwartet erwartet." >&2
    exit 1
fi
echo "==> $gefunden Programme"

rm -rf "$zwischen"
mkdir -p "$zwischen" "$dist"
for datei in "${programme[@]}"; do
    cp "$datei" "$zwischen/"
done

# Strippen: ohne Symboltabelle sind die Programme ein Bruchteil so groß.
# Windows wird nicht gestrippt — dort liegen die Debug-Informationen in einer
# eigenen `.pdb`, das Programm selbst trägt keine; ein Fremd-`strip` an einer
# MSVC-Datei wäre Risiko ohne Gewinn.
strip_werkzeug() {
    for kandidat in strip llvm-strip; do
        if command -v "$kandidat" >/dev/null 2>&1; then
            printf '%s' "$kandidat"
            return 0
        fi
    done
    return 1
}

if [ "$system" = windows ]; then
    echo "==> Windows: kein Strippen nötig"
elif strip=$(strip_werkzeug); then
    for programm in "$zwischen"/*"$exe"; do
        if [ "$system" = macos ]; then
            "$strip" -x "$programm"
        else
            "$strip" "$programm"
        fi
    done
    echo "==> gestrippt mit $strip"
else
    echo "FEHLER: kein strip gefunden — die Programme blieben ungestrippt." >&2
    exit 1
fi

# macOS: ad hoc signieren, sonst verweigert Gatekeeper den Start. Nach dem
# Strippen, weil eine Signatur über den Bytes liegt.
if [ "$system" = macos ]; then
    verteilung/macos-signieren.sh "$zwischen"
fi

# `LISTE.json`: je Programm eine Zeile, damit sich eine Summe mit `grep`
# zuordnen lässt (`pruefen.sh`); oben Version, System und Architektur.
eintraege=()
for datei in "$zwischen"/*"$exe"; do
    name=$(basename "$datei" "$exe")
    groesse=$(wc -c <"$datei" | tr -d ' ')
    summe=$(verteilung/summe.sh "$datei")
    eintraege+=("    { \"name\": \"$name\", \"version\": \"$version\", \"groesse\": $groesse, \"sha256\": \"$summe\" }")
done

{
    printf '{\n'
    printf '  "version": "%s",\n' "$version"
    printf '  "system": "%s",\n' "$system"
    printf '  "arch": "%s",\n' "$arch"
    printf '  "programme": [\n'
    for i in "${!eintraege[@]}"; do
        printf '%s' "${eintraege[$i]}"
        if [ "$i" -lt $((${#eintraege[@]} - 1)) ]; then
            printf ','
        fi
        printf '\n'
    done
    printf '  ]\n'
    printf '}\n'
} >"$zwischen/LISTE.json"

# Das Archiv liegt im `dist/` der Wurzel — flach, ohne führendes `./`.
(
    cd "$zwischen"
    tar -czf "$dist/$archiv" quiver-*"$exe" LISTE.json
)

echo "==> $dist/$archiv"
verteilung/summe.sh "$dist/$archiv"
