# Game flow foundation

## Stable bootstrap

`godot/scenes/main.tscn` is the permanent `run/main_scene`. It contains only a
`ScreenHost` and binds that host to the `FlowCoordinator` autoload. The
coordinator swaps the host's child; it never changes the Godot scene tree's
current scene.

Feature scenes must not change `run/main_scene`, call
`change_scene_to_file()`, or name one another's scene paths. All gameplay
navigation goes through `FlowCoordinator`.

`FlowCoordinator` owns the persistent `RunStateNode`. Replacing a screen does
not recreate that node, so contract state survives every screen transition.
Player money and upgrades remain a separate concern and use the
`PlayerStateGlobal` autoload rather than being added to run state.

## Lifecycle

The plain Rust model in `cyber_heist/src/run_state.rs` enforces these states:

- `hub`: no active encounter.
- `encounter_active`: exactly one active encounter with an ID and type.
- `caught`: retains that active encounter so caught UI and penalty code can
  inspect it. Finishing the caught flow returns to the hub and clears it.

Rejected transitions do not mutate run state. The Godot adapter returns
`{"ok": false, "error": "..."}` for a rejection and `{"ok": true}` for
success. `FlowCoordinator.run_snapshot()` returns:

```gdscript
{
    "phase": "hub", # or "encounter_active" / "caught"
    "active_encounter": null, # or {"id": String, "type": String}
}
```

Supported lifecycle encounter types are `combat`, `event`, `shop`, and
`elite`. Combat has its gameplay scene; the other types currently use a shared
placeholder scene until their separate stories are implemented.

## Contract route selection

This slice implements only the capability to choose an onward path, load its
encounter, and lock paths not taken. It is **not** a contract-map UI design,
visual design, or UX proposal. `ContractMap` is the domain name for the node
graph, while the current hub and placeholder scenes are functional test
harnesses that future UI/UX work may replace completely.

The persistent `RunStateNode` also owns the authored contract graph. The hub
asks `FlowCoordinator.selectable_encounters()` for every directly reachable
node and shows the returned node ID and encounter type. A selection must go
through `FlowCoordinator.select_encounter(node_id)` so the destination is
loaded before the Rust model commits to the route.

Selecting a node permanently locks every sibling branch from the previous
node. Completing the loaded encounter returns to the hub, where only onward
nodes from the chosen route are offered. Graph rules and lock-out behaviour are
implemented in `cyber_heist/src/contract_map.rs`; encounter contents remain a
separate concern.

## The sentry's turn

Combat alternates a player turn and the sentry's turn. A sentry is the named
security construct guarding an encounter, WARDEN-7 by default. `DrawPhase` owns
the turn boundary: `end_turn()` discards what is left in hand, emits the
`security_phase` signal, then refreshes energy and deals the next hand. Godot
delivers that signal straight away, so anything the sentry does has already
happened by the time the new hand exists.

`cyber_heist/src/sentry.rs` holds that side as plain Rust: a name and a list
of actions the sentry cycles through. It owns no noise value. The autoload
`/root/NoiseMeterGlobal` owns the single `NoiseLevel`, with the incoming
100-point cap. The action at the front of the queue is shown to the player during their own turn, which
is what makes ending a turn a decision rather than a formality. Reactive or
varied actions are a later story. `sentry_node.rs` exposes it to the combat
scene:

```gdscript
$Sentry.construct_name()          # "WARDEN-7"
$Sentry.noise()
$Sentry.max_noise()
$Sentry.queued_action_name()      # "" when nothing is queued
$Sentry.queued_action_noise()
$Sentry.add_noise(amount)         # noise from anywhere else, clamped
$Sentry.perform_queued_action()
```

The name is data, not code. `SentryNode` exports a `sentry_name` property, so
a scene can field a different construct from the Inspector without touching
any of the rules. Leaving it blank keeps the built-in WARDEN-7.

Both connections live in `combat.tscn` rather than in code, so they are not
made twice: `DrawPhase.security_phase` runs the sentry's turn, and the sentry's
own `turn_resolved` signal hands the result to the combat screen as:

```gdscript
{
    "ok": true, # false if no action, already detected, or detached
    "sentry": "WARDEN-7",
    "action": "Trace Sweep",
    "noise_added": 2,
    "noise": 5,
    "max_noise": 100,
    "run_failed": false,
}
```

The displayed intent is the **authored** action name and noise value. Reading
it does not advance the queue; performing it advances exactly once and wraps.
`noise_added` reports the **actual** clamped change: an announced +3 at 99/100
still runs that action but adds only 1. Those quantities are not interchangeable.

Noise is clamped to the meter, so an action can never push it past the cap.
Reaching the cap means the player has been detected and the run is over: the
combat screen calls `FlowCoordinator.report_caught()`, which moves run state to
`caught` and shows the detection screen, with the usual fine and loss of this
contract's upgrades. The hand `end_turn()` dealt for the turn that will never
happen is simply discarded along with the screen.

## Card noise and recovery (story #4)

VPN is a playable recovery action: it costs 2 energy, lowers noise by 10 with
a floor of zero, and goes to discard. The player's turn continues and the
queued sentry intent does not change. It must be played **before** the meter
fills; recovery cannot revive a detected encounter. VPN's earlier intangible
and end-turn promises are removed rather than implying those effects exist.

`DrawPhase.play_card(index)` returns `{ok: true, name, noise_change}`
or `{ok: false, error}`. The plain Rust `card_play` transaction rejects an
invalid index, insufficient energy or an already-full meter before changing
anything. On success it spends energy, removes and discards the card, and
applies its signed `noise_generated` exactly once to the borrowed `NoiseLevel`.
The transaction has no Godot objects or sentry dependency. `noise_change` is the
actual clamped change, which can be zero. The bridge increments the play counter only
on success; the combat screen refreshes the labels and immediately routes a
full meter through `FlowCoordinator.report_caught()`.

This integrates GitHub PR12's shared noise and social-card presentation (not
Trello STORY12, which covers queued security intent). Positive, zero and negative
card noise share the same paid transaction. Hand names and card details select
the strong/weak side from that same global value; play feedback names the side
selected before the card's own noise lands. Damage, block and other keywords
are still not executed by this slice. No keyword engine is included.

`NoiseMeter.noise` and `max_noise` are computed read-only Godot properties, not
writable exported mirrors. Card transactions and sentry actions both mutate the
private `NoiseLevel`, so display, threshold and detection reads cannot drift.
Adapters retain a handle to the autoload, not a shadow meter, so old-screen reads
remain safe after detachment. Detached play/turn calls cannot mutate gameplay;
the combat UI also ignores queued input after screen replacement.

The harness's free **Draw Hand** button is removed from combat, because it
would let players repeatedly refresh energy and redraw VPN without a sentry
turn. Initial dealing and **End Turn** still use the existing draw APIs.

Run the real-scene regression with a built extension:

```sh
godot --headless --path godot -s res://tests/lower_detection_test.gd
```

The test draws the complete starter deck only as a fixture and selects by card
name, so recovery, rejection and positive-noise coverage do not depend on
shuffling. Near-cap recovery uses 99 -> 89, not the retired ten-point scale.
The smoke test seeds 90/100, then checks six announced actions through queue
wrap and the final clamped +1. Both run in the stress workflow.

The generated contract does not guarantee an opening combat option. The shared
`tests/combat_fixture.gd` keeps the real coordinator and Rust lifecycle, selecting
a reachable node and mounting combat content if that node has another type.
These tests exercise combat, not the random encounter scene registry.

For cross-component sharing and the authored social threshold (50), run:

```sh
GODOT_BIN=godot bash godot/tests/run_shared_noise_test.sh
```

The runner copies resources to a private temporary project and uses a four-card
fixture deck. `nigerian_king` is authored but is **not** in the production starter
deck. The test checks bar/labels, strong/weak names, descriptions and keywords
after VPN, loud cards and security actions, as well as screen replacement.
`CYBER_HEIST_LIBRARY` may select an explicitly labelled substitute build. The
runner pre-registers the extension for engine startup; first-discovery editor
import aborted on exit with the local Godot 4.7.1/reduced-binding setup, while
startup registration and runtime tests succeeded. It does not suppress import
or test failures and does not prove that first-discovery path works.

### Shared lifetime: unresolved retry policy

PR12's autoload lifetime is preserved. Completing or replacing an encounter,
entering caught, and returning to the hub do not reset noise. A recreated combat
screen reads the same value, rather than silently starting a second meter.
Concretely, retry after caught starts at 100/100 and paid recovery is rejected.
The shared-meter test records that behaviour; it is not a proposed reset policy.
A run/retry reset decision remains product work, not an implicit part of this
integration.

## Scene-facing API

```gdscript
FlowCoordinator.selectable_encounters()
FlowCoordinator.current_contract_node()
FlowCoordinator.select_encounter(node_id: int)
FlowCoordinator.complete_active_encounter()
FlowCoordinator.report_caught()
FlowCoordinator.finish_caught()
FlowCoordinator.run_snapshot()
```

Every transition method returns a result dictionary. A destination is checked
and instantiated before lifecycle state changes, then the current screen is
replaced only after the Rust transition succeeds.

A contract-map screen starts a selected encounter without navigating directly:

```gdscript
var result := FlowCoordinator.select_encounter(encounter.id)
```

A losing encounter reports its outcome in the same way:

```gdscript
var result := FlowCoordinator.report_caught()
```

`caught_screen.tscn` is integrated with the coordinator. Its Continue button
finishes the caught lifecycle through:

```gdscript
FlowCoordinator.finish_caught()
```

Finishing caught does not complete the selected map node. It returns to the hub
with only that committed encounter available to retry; the branch declined at
selection remains locked.

The caught screen applies its separate `PlayerStateGlobal` penalty once, while
the coordinator continues to own navigation. Both autoloads remain registered,
and the Rust crate registers the caught-penalty and game-flow classes together.
