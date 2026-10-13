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

# Godot 4.7.1 itself intermittently dies during a headless --import before any
# test code runs: "double free or corruption" (SIGABRT, exit 134), a
# segmentation fault while reimporting a font (exit 139), or a hang (exit 124,
# or 137 if it ignores SIGTERM and has to be killed). Only those crash and hang
# codes are retried, a bounded number of times. Any other exit, such as 1 for a
# real import error or 126/127 for a bad GODOT_BIN, fails immediately.
#
# Each retry starts from an empty resource cache, so a crashed attempt cannot
# leave a half-written import behind, and the attempt that succeeds has
# imported every resource itself. That is why the error scan below only needs
# the successful attempt's log: a broken asset that errored in a crashed
# attempt errors again there. Godot also prints its import output and crash
# reports to the console, so every attempt is visible in the CI log.
# Only the import is retried; the test run below gets exactly one attempt.
import_attempts=3
is_engine_crash_or_hang() {
    case $1 in
        124 | 134 | 137 | 139) return 0 ;;
        *) return 1 ;;
    esac
}
reset_import_cache() {
    find "$fixture/godot/.godot" -mindepth 1 -maxdepth 1 \
        ! -name extension_list.cfg -exec rm -rf {} +
}
import_fixture() {
    local attempt status
    for ((attempt = 1; attempt <= import_attempts; attempt++)); do
        status=0
        timeout -k 5s 30s "$engine" --headless --path "$fixture/godot" \
            --log-file "$fixture/import.log" --import || status=$?
        if [[ $status -eq 0 ]]; then
            return 0
        fi
        echo "Fixture import attempt $attempt/$import_attempts failed with exit $status." >&2
        if ! is_engine_crash_or_hang "$status"; then
            echo "Exit $status is not an engine crash or hang, so it is not retried." >&2
            return "$status"
        fi
        reset_import_cache
    done
    echo "Fixture import crashed or hung on all $import_attempts attempts." >&2
    return 1
}

import_fixture
timeout -k 5s 30s "$engine" --headless --path "$fixture/godot" \
    --log-file "$fixture/test.log" -s res://tests/shared_noise_test.gd

# Godot can return zero for a script error. Require the test's own completion
# marker and reject errors rather than treating a process exit as test evidence.
grep -q 'shared noise test: PASSED' "$fixture/test.log"
if grep -E 'SCRIPT ERROR:|ERROR:|FAIL' "$fixture/import.log" "$fixture/test.log"; then
    exit 1
fi
