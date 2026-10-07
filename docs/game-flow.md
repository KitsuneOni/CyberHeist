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

Supported lifecycle encounter types are `combat`, `event`, `shop`, `elite`
and `boss`. Combat, elite and boss share the combat gameplay scene, and events
have their own. Shops still use a shared placeholder scene until their story
is implemented.

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

## Zones and their bosses

A contract is a run of **zones**. Each zone is a few columns of ordinary
encounters closed off by a single **boss** column, so the boss is always the
last node of its zone and the only way into the next one. `ContractShape`
decides the split; the default is three zones of two encounter columns each,
which is the same ten-column contract as before.

A node's zone is derived from where the bosses sit rather than stored on the
node: a node's zone is the number of boss columns before it. An authored
contract with no boss on it is therefore a single open zone, which is what
every contract predating this slice keeps being.

Zones gate selection. `unlocked_zones` starts at zero and is raised **only** by
completing a boss encounter, so nodes in a later zone cannot be entered however
directly the graph leads into them — an authored edge going around a boss is
refused with `ZoneSealed`. Failing a boss leaves the zone shut, and the boss
remains the only thing on offer to retry.

Those nodes report a `sealed` status, which is deliberately distinct from
`locked`: a locked node was ruled out by the player's own branch choice and is
gone for good, while a sealed one opens the moment the zone's boss goes down.
Where both apply, `locked` wins, because being permanently ruled out is the
more useful thing to tell the player. The map draws bosses as diamonds and
sealed nodes in amber, and the key explains both.

```gdscript
FlowCoordinator.zone_progress()
# {"current_zone": 0, "unlocked_zones": 1, "total_zones": 3}
```

`current_zone` is the zone the player is standing in. Clearing a boss raises
`unlocked_zones` but leaves `current_zone` alone: the boss closes the zone it
guards, so the player is still in it until they step into the next one.

## Contract offers (story #120)

A new run is offered a choice of **three** contracts (`OFFER_COUNT` in
`cyber_heist/src/contract_offer.rs`). Each offer has a name, a target
organisation, a credit reward and a shape: how many zones it has and how long
each zone is. Offers in one set never share a name or a target, and no two run
the same length, so the choice is always between a shorter, cheaper job and a
longer one that pays more. The reward is 30 credits per encounter on a route
through the contract plus 75 per boss. Zone counts are capped at the bosses
that have been written in `sentry.rs`, currently three.

Every offer carries a `map_seed`, so accepting the same offer always builds the
same contract map. That keeps accepting deterministic for tests and for saving
a run later.

`RunState` owns the offers for the whole run and records which one was
accepted. Accepting an offer by index replaces the run's contract map with the
one that offer describes. It is rejected, without changing anything, for an
index that is not on offer, outside the hub, or once any encounter on the
current contract has been entered (completed or caught). Before then the
player may change their mind and accept a different offer.

**Until the contract selection screen lands, a run still starts on a playable
default contract** (three zones of two columns), exactly as before. The hub
and every headless test keep working without accepting anything. That default
contract is not one of the offers, so `accepted_contract_offer()` reports
`ok: false` for it and it has no credit reward to pay.

```gdscript
FlowCoordinator.contract_offers()
# [{"index": 0, "name": "Glass Hammer", "target": "Kestrel Biotech",
#   "credit_reward": 165, "zone_count": 1, "encounter_count": 3}, ...]

FlowCoordinator.accept_contract_offer(1)
# {"ok": true}, or {"ok": false, "error": "..."}

FlowCoordinator.accepted_contract_offer()
# the accepted offer's fields plus "ok": true, or {"ok": false}
```

Offers are listed shortest first. Accepting only changes run state; it does
not navigate. The selection or details screen decides where to go next, and
the hub reads the new map when it is next shown. The accepted offer's
`credit_reward` is kept on the run for the victory payout to read.

Run the headless check with a built extension:

```sh
godot --headless --path godot -s res://tests/contract_offer_test.gd
```

## Run stats and new runs (story #121)

`RunState` keeps a `RunStats` (`cyber_heist/src/run_stats.rs`) for the run: how
many fights were won, how many credits were earned and how many cards were
added to the deck. The stats belong to the run, so they start at zero with it
and a new run starts them again.

- **Wins** are counted by `RunState::complete_encounter` itself, so nothing
  else has to remember to. Completing a combat, elite or boss encounter is a
  win (`EncounterType::is_fight`). Events and shops are not fights, and an
  encounter the player was caught in is never completed, so neither counts.
- **Credits earned** adds up every gain. A loss, such as a fine or a bad event
  gamble, changes the balance but not what the run earned. Whatever applies
  credits records the change once, gain or loss alike:
  - `EventNode` applies event credits in Rust and records them on the run
    itself.
  - Anything applying credits from GDScript (the combat payout in #114, the
    contract payout in #89/#23) calls `FlowCoordinator.change_credits(amount)`,
    which updates `PlayerStateGlobal` and records the change together.
  - The caught fine is a loss, so it is not recorded and does not need to be.
- **Cards added** counts cards that go into the run's deck on top of the
  starter deck. Every card is added through
  `FlowCoordinator.add_card_to_deck(card_id)`, which only counts a card the
  deck accepted. `take_card_reward` uses it, and so should a shop.

The end screen reads everything in one call:

```gdscript
FlowCoordinator.run_summary()
# {"combats_won": 4, "credits_earned": 180, "cards_added": 3,
#  "encounters_completed": 5, "total_encounters": 9,
#  "zones_cleared": 1, "total_zones": 3}
```

`encounters_completed` and `total_encounters` are the same numbers as
`contract_progress()`. `zones_cleared` counts the zones whose boss has been
beaten.

### Starting a new run

```gdscript
FlowCoordinator.start_new_run()
# {"ok": true}, or {"ok": false, "error": "..."}
```

A new run gets a fresh default contract and a fresh set of offers, with nothing
accepted, no zone progress and no stats. In Rust it is exactly what
`RunState::generated` builds, so nothing from the old run can leak into it. The
coordinator then resets what lives outside the Rust run state: the deck goes
back to the starter deck, noise and shield go to 0, detection resistance is
cleared, knowledge goes back to its starting level and the contract's upgrades
are dropped from `PlayerStateGlobal`. Finally it shows the hub.

A new run can start from the hub or the caught screen, which is where a run
ends. It is refused during an encounter, with nothing changed, because that
encounter's screen would otherwise carry on playing against a run that no
longer exists.

**Credits carry over; upgrades do not.** The credit balance on
`PlayerStateGlobal` is the player's rather than the run's: it is what the shop
and contract stories spend between runs. Only the run's own stats
(`credits_earned`) start again from zero. Upgrades are earned for one contract
(see `EventEffect::upgrade`), so `start_new_run()` clears them with
`PlayerStateGlobal.clear_upgrades()`. Being caught already loses them through
the caught screen's penalty; this also covers a run that ends at the hub.

Run the headless check with a built extension:

```sh
godot --headless --path godot -s res://tests/run_stats_test.gd
```

## The sentry's turn

Combat alternates a player turn and the sentry's turn. A sentry is the named
security construct guarding an encounter, WARDEN-7 in the first zone. `DrawPhase` owns
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
$Sentry.queued_action_ability()   # "-1 energy", or "" when it only makes noise
$Sentry.has_ability()
$Sentry.detection_resistance()
$Sentry.add_noise(amount)         # noise from anywhere else, clamped
$Sentry.perform_queued_action()
```

The name is data, not code. `SentryNode` exports a `sentry_name` property, so
a scene can field a different construct from the Inspector without touching
any of the rules. Leaving it blank keeps the construct chosen for the encounter.

### Which construct guards an encounter

`SentryNode` builds its sentry on `ready` from the run, not from the scene: it
asks `RunStateNode.active_encounter_profile()` for the active encounter's zone
and whether it is an elite or that zone's boss (`is_elite`, `is_boss`), then
calls `Sentry::for_encounter` with the matching `SecurityTier`. With no
run behind the screen — a combat scene opened on its own — it falls back to
first-zone security, the same way `DrawPhase` falls back to the starter deck.

Standard security is the authored WARDEN script scaled by zone, so zone 1 is
WARDEN-7 exactly as before. A zone's boss is then defined against the security
around it rather than against a fixed bar:

- every one of its actions is louder than the loudest standard action in that
  zone,
- it resists detection 30 points harder than standard security there,
- it has more integrity than standard security's 40, starting at 80 for
  ICEBREAKER and rising 20 per zone (BLACK MONOLITH 100, THE ARCHITECT 120),
  so every boss is a longer fight than the one before it, and
- it carries a signature ability no standard construct has.

Each boss's signature is its second action, announced with the intent before
it lands, so ending the turn into it is a decision rather than a surprise.
They each ask something different of the deck (Trello #100, E2b):

| Zone | Boss | Signature | What it does |
| --- | --- | --- | --- |
| 1 | ICEBREAKER | **Purge** | clears all corruption on it |
| 2 | BLACK MONOLITH | **Firewall** | blocks the next 12 plain damage |
| 3+ | THE ARCHITECT | **Grid Lockdown** | takes energy off the next turn, one per zone deep (3 here) |

**Purge** runs after that turn's corruption tick, so it clears what would have
ticked next. A corruption deck has to land its damage before the Purge comes
round. `SentryNode` reports the amount cleared as `corruption_purged`.

**Firewall** goes up on the boss's turn and covers the player's next turn
only: `SentryNode` drops it before anything else when the boss's following
turn starts. It absorbs a card's plain Damage and wears down by what it
blocked. Penetrating damage and corruption go straight through it, which is
what the Penetrating card text has always said ("straight through the
target's block"). `play_card` reports what it soaked up as `damage_blocked`,
the turn outcome reports the firewall raised as `firewall`, and the health
line shows `[Firewall N]` while one is up.

**Grid Lockdown** is reported in the turn outcome as `energy_drain`, and the
combat screen applies it through `DrawPhase.drain_energy()` after `end_turn()`
has refreshed the pool — so it bites into the turn it opens, and cards the
player can no longer afford come up disabled rather than failing when clicked.
Bosses past the authored three keep the lockdown.

### THE ARCHITECT's second phase

THE ARCHITECT closes the longest contract on offer, so it is the final boss,
and it is the only construct with a second phase (Trello #100, E2c). The
first time its integrity drops **below** half (under 60 of 120; exactly 60 is
still phase 1), it switches to a new action set, starting from its first
action:

| Action | Noise | Ability |
| --- | --- | --- |
| Rewrite Protocol | 12 | Purge |
| Fortify Core | 12 | Firewall 12 |
| Total Lockdown | 14 | -3 energy |

The check is in `Sentry::take_damage`, so a card, penetrating damage or a
corruption tick can all set it off. It happens once: healing back above half
does not undo it, and a blow that takes it straight to zero ends the fight
rather than starting a phase.

From then on the intent reads `THE ARCHITECT [PHASE 2] will: ...`, and the
status line says once that it escalated. `SentryNode.phase()` exposes the
phase, and the turn outcome carries `phase` and `escalated`.

If the corruption tick at the start of its own turn is what takes it below
half, it spends that turn escalating: no action runs, `ok` is false and
`escalated` is true. The action it had announced belonged to the old set, and
the new set's first action has not been shown yet, so running either would
spring something on the player they never saw coming.

### Elite encounters

An elite node is a fight the player chooses to take on for a better payout
(Trello card E1). It sits between the two other tiers, so it is measured
against the same zone's standard security:

| | Standard | Elite | Boss |
| --- | --- | --- | --- |
| Integrity | 40 | 60 | 80 (+20 per zone) |
| Resistance | +10 per zone | standard + 15 | standard + 30 |
| Ability | none | Reinforce | its own signature (see above) |
| Credits (zone 1, +per zone) | 25 (+15) | 2x combat | 3x combat |
| Card reward | any 3 | at least one Rare | at least one Rare |

The elites are BASTION, HYDRA and CERBERUS, in zone order. Their script is the
zone's standard one, renamed and one point louder per action, followed by
**Reinforce**: one point of noise, plus 8 integrity restored to the construct
(4 more per zone deeper). It is announced with the intent as `+8 integrity`, so
a player can try to finish the elite before it lands. Because it acts on the
construct itself, `SentryNode` applies it and reports the clamped amount
regained as `integrity_restored` in the turn outcome. There is nothing for the
combat screen to apply, unlike a lockdown; it only says so.

Payouts live in `cyber_heist/src/encounter_reward.rs`. `FlowCoordinator`
reads `RunStateNode.active_encounter_reward()` before completing an encounter
(completing clears it), and adds the credits to `PlayerStateGlobal` only once
the transition succeeds. The card reward screen then reads the guarantee
through `offer_card_reward()` and shows the payout with
`pending_reward_credits()`. Events and shops settle their own credits, so they
pay nothing here.

`card_reward::offer_with_guarantee` builds a guaranteed offer. It picks one
card of the guaranteed rarity or better first, fills the rest from everything
else so the offer stays distinct, and shuffles where the guaranteed card lands.
A pool with no such card falls back to an ordinary offer.

### Detection resistance

Resistance is a percentage that works against the player pulling noise **down**,
never against the security system putting it up: a hardened construct is hard to
hide from, not louder by itself. It lives on the one shared `NoiseLevel`
alongside the noise, so there is no second value to drift, and it is rounded
towards zero so a blunted recovery can come to nothing but never pays out.

It belongs to the encounter, not the run. `FlowCoordinator` clears it to zero in
`_replace_screen` before the incoming screen is added; that screen's `SentryNode`
then sets its own, and a screen with no sentry simply leaves it at zero. Noise
itself is deliberately untouched there, because noise carries across a run.

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
    "energy_drain": 0,  # >0 only for a boss lockdown
    "ability": "",      # "-1 energy" when the action carries one
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

For the zone boss slice, run:

```sh
godot --headless --path godot -s res://tests/boss_encounter_test.gd
```

It walks a generated contract to its first boss without playing the encounters
on the way, then checks the parts the Rust tests cannot see: that a boss node
loads the boss construct, that its resistance reaches the shared meter and
blunts a real VPN play, that its lockdown takes energy off the turn it opens,
and that beating it turns the next zone's nodes from sealed into ones the hub
offers. It also checks the resistance does not outlive the encounter.

For the elite slice, run:

```sh
godot --headless --path godot -s res://tests/elite_encounter_test.gd
```

Generated contracts do not guarantee an elite, so it rolls fresh contracts
until a first-zone route offers one, clearing a standard combat on the way to
measure its payout. It checks that the elite node loads combat against BASTION
with 60 integrity, an ability and its resistance on the shared meter. It then
checks that Reinforce is announced, restores integrity and is reported on
screen, and that beating it pays more than that standard combat and offers a
Rare.

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

### Shared lifetime and caught recovery

Completing or replacing a normal encounter preserves shared noise. Entering
caught also retains the full meter while the detection screen is displayed.
After `finish_caught()` successfully transitions back to the hub, the coordinator
clears the shared meter to zero before showing the hub. A rejected transition
never clears noise. The next attempt can play cards and recover normally, while
ordinary encounter progression still carries noise forward.

## Scene-facing API

```gdscript
FlowCoordinator.contract_offers()
FlowCoordinator.accept_contract_offer(index: int)
FlowCoordinator.accepted_contract_offer()
FlowCoordinator.run_summary()
FlowCoordinator.change_credits(amount: int)
FlowCoordinator.add_card_to_deck(card_id: String)
FlowCoordinator.start_new_run()
FlowCoordinator.selectable_encounters()
FlowCoordinator.current_contract_node()
FlowCoordinator.zone_progress()
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
