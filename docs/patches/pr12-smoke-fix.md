# Standalone PR #12 smoke-test fix

Patch: [pr12-shared-meter-smoke.patch](pr12-shared-meter-smoke.patch)

Targets `Implimenting-Further-Card-Functions` at
`0d21b1bb1a56bd0acfb04c04373c6c17ed1f0019`. Changes only
`godot/tests/combat_smoke_test.gd`; no dependency on the VPN recovery or
security-intent work.

## Why CI fails

The test expects a cap of 10 and detection after six turns. PR #12 changes
the shared meter's cap to 100, so those turns correctly produce 12/100,
without detection or a fine.

## What changes

- Read the cap from `NoiseMeterGlobal` rather than hardcoding the display denominator.
- Still verify that a fresh game starts with zero noise.
- Set up the boundary scenario ten points below the actual cap through `add_noise()`.
- Preserve the six-turn sentry sequence, final clamping, detection timing,
  screen transition, run state, and once-only fine assertions.
- Assert the shared meter value after every turn, including the fatal turn.

At cap 100, the fixture starts at 90 and expects 91, 93, 96, 97, 99, 100.
This changes the test setup, not gameplay or the cap.

## Apply on the PR branch

Save the patch outside the source tree, then from the repository root:

```sh
git apply --check /path/to/pr12-shared-meter-smoke.patch
git apply /path/to/pr12-shared-meter-smoke.patch
cd cyber_heist
cargo build --locked
cd ..
godot --headless --path godot -s res://tests/combat_smoke_test.gd
```

## Verification scope

The patch applies cleanly to the exact PR head above, and the resulting script
passes Godot 4.7.1's parser check. Its full runtime execution against the
standalone PR head remains unverified: this parent session cannot execute
Cargo (`Permission denied`). The parser check used the existing integrated
project's extension, so it does not establish upstream runtime compatibility.

Separately, the integrated shared-meter branch passed its expanded smoke,
recovery and social-threshold suites under a reduced-binding build. That is
not a substitute for running this standalone patch on PR #12.
