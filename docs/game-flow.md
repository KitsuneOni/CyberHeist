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

`cyber_heist/src/sentry.rs` holds that side as plain Rust: a name, a noise
meter with a cap, and a list of actions the sentry cycles through. The action
at the front of the queue is shown to the player during their own turn, which
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
$Sentry.take_turn()
```

The name is data, not code. `SentryNode` exports a `sentry_name` property, so
a scene can field a different construct from the Inspector without touching
any of the rules. Leaving it blank keeps the built-in WARDEN-7.

Both connections live in `combat.tscn` rather than in code, so they are not
made twice: `DrawPhase.security_phase` runs the sentry's turn, and the sentry's
own `turn_resolved` signal hands the result to the combat screen as:

```gdscript
{
    "ok": true, # false only if nothing was queued, so the turn was skipped
    "sentry": "WARDEN-7",
    "action": "Trace Sweep",
    "noise_added": 2,
    "noise": 5,
    "max_noise": 10,
    "run_failed": false,
}
```

Noise is clamped to the meter, so an action can never push it past the cap.
Reaching the cap means the player has been detected and the run is over: the
combat screen calls `FlowCoordinator.report_caught()`, which moves run state to
`caught` and shows the detection screen, with the usual fine and loss of this
contract's upgrades. The hand `end_turn()` dealt for the turn that will never
happen is simply discarded along with the screen.

Card `noise_generated` values are not on the meter yet. `add_noise` is the way
in for them when card effects are implemented.

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
