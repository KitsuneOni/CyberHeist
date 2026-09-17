#!/usr/bin/env bash
# Linux/native Godot fixture runner. Never alters production card/deck data.
set -euo pipefail

project=$(cd "$(dirname "$0")/.." && pwd)
engine=${GODOT_BIN:-godot}
library=${CYBER_HEIST_LIBRARY:-"$project/../cyber_heist/target/debug/libcyber_heist.so"}
library=$(realpath "$library")
if [[ ! -f "$library" ]]; then
    echo "Build the extension first: missing $library" >&2
    exit 1
fi

fixture=$(mktemp -d)
cleanup() {
    local status=$?
    if [[ $status -eq 0 ]]; then
        rm -rf "$fixture"
    else
        echo "Failed fixture retained for diagnosis: $fixture" >&2
    fi
}
trap cleanup EXIT
mkdir -p "$fixture/godot/.godot" "$fixture/cyber_heist/target/debug" "$fixture/tmp"
# Copy authored resources, not a stale resource/script cache.
tar -C "$project" --exclude='./.godot' -cf - . | tar -C "$fixture/godot" -xf -
cp "$project/tests/fixtures/shared_noise_starter_deck.ron" \
    "$fixture/godot/data/starter_deck.ron"
ln -s "$library" "$fixture/cyber_heist/target/debug/libcyber_heist.so"
# Load the built extension at engine startup rather than discovering/loading it
# halfway through the editor's first scan. The latter aborts at editor shutdown
# with the local 4.7.1/reduced-binding setup. This fixture verifies runtime and
# resource import, not that first-discovery editor path.
printf 'res://cyber_heist.gdextension\n' > "$fixture/godot/.godot/extension_list.cfg"
export TMPDIR="$fixture/tmp"

timeout 30s "$engine" --headless --path "$fixture/godot" \
    --log-file "$fixture/import.log" --import
timeout 30s "$engine" --headless --path "$fixture/godot" \
    --log-file "$fixture/test.log" -s res://tests/shared_noise_test.gd

# Godot can return zero for a script error. Require the test's own completion
# marker and reject errors rather than treating a process exit as test evidence.
grep -q 'shared noise test: PASSED' "$fixture/test.log"
if grep -E 'SCRIPT ERROR:|ERROR:|FAIL' "$fixture/import.log" "$fixture/test.log"; then
    exit 1
fi
