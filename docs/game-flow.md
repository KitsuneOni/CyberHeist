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
