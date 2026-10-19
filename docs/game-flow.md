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
not recreate that node, so contract state survives every screen transition. Between sessions the
run is saved to disk at safe points and Continue restores it (see
[Saving and continuing](#saving-and-continuing-story-10)).
Player money and upgrades remain a separate concern and use the
`PlayerStateGlobal` autoload rather than being added to run state.

## Lifecycle

The plain Rust model in `cyber_heist/src/run_state.rs` enforces these states:

- `hub`: no active encounter.
- `encounter_active`: exactly one active encounter with an ID and type.
- `caught`: retains that active encounter so caught UI and penalty code can
  inspect it. Finishing the caught flow returns to the hub and clears it.
- `contract_won`: the final zone's boss has been beaten and the contract is
  complete. There is no active encounter and nothing left to select; the run
  ends on the victory screen, where a new run can start (see
  [Winning a contract](#winning-a-contract-stories-89-and-23)).

Rejected transitions do not mutate run state. The Godot adapter returns
`{"ok": false, "error": "..."}` for a rejection and `{"ok": true}` for
success. `FlowCoordinator.run_snapshot()` returns:

```gdscript
{
    "phase": "hub", # or "encounter_active" / "caught" / "contract_won"
    "active_encounter": null, # or {"id": String, "type": String}
}
```

Supported lifecycle encounter types are `combat`, `event`, `shop`, `elite`
and `boss`. Combat, elite and boss share the combat gameplay scene, while
events and shops each have their own (see [the dark web shop](#the-dark-web-shop-story-38)).

## Contract route selection

This slice implements only the capability to choose an onward path, load its
encounter, and lock paths not taken. It is **not** a contract-map UI design,
visual design, or UX proposal. `ContractMap` is the domain name for the node
graph, while the current hub scene is a functional test harness that future
UI/UX work may replace completely.

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
player may change their mind and accept a different offer, which the hub's
**Change contract** button lets them do (below).

A run is still created on a playable **default contract** (three zones of two
columns), so headless tests that drive `FlowCoordinator` directly keep working
without accepting anything. That default contract is not one of the offers, so
`accepted_contract_offer()` reports `ok: false` for it and it has no credit
reward to pay: winning it still ends the contract on the victory screen, but
the contract reward is 0. A player never plays it, because the hub asks for a
contract first (below).

### Choosing a contract on the hub

While `FlowCoordinator.needs_contract_choice()` is true, the contract hub shows
the **offer picker** (`scenes/contract_offer_picker.tscn`) in place of the map.
That is a new run at the hub with offers on the table, none accepted yet and
no encounter entered (`RunState::needs_contract_choice`), which covers both
**New contract** on the main menu and **Take another contract** on the
victory screen. An authored run has no offers, so it goes straight to the map.

The picker follows sheet 3 of the UI mockups: one offer on screen at a time
with its name, target, length in zones and encounters, and payout; arrows
cycle through the offers, and **Accept contract** commits. The picker only
reports the chosen offer's `index`; the hub accepts it through
`accept_contract_offer` and then draws the new map. A rejected acceptance
leaves the picker up with the reason under the offer.

Accepting does not lock the contract in. Until the player enters an encounter,
the map shows **Change contract** (`FlowCoordinator.can_change_contract()`,
which is `RunState::can_accept_contract_offer`). It brings the picker back,
opened on the contract already accepted, so Accept keeps it and choosing
another offer swaps the run onto that one. The button disappears once the
first encounter is entered and the run is committed to its contract.

```gdscript
FlowCoordinator.contract_offers()
# [{"index": 0, "name": "Glass Hammer", "target": "Kestrel Biotech",
#   "credit_reward": 165, "zone_count": 1, "encounter_count": 3}, ...]

FlowCoordinator.accept_contract_offer(1)
# {"ok": true}, or {"ok": false, "error": "..."}

FlowCoordinator.needs_contract_choice()
# true while the hub should show the offer picker instead of the map

FlowCoordinator.can_change_contract()
# true while the map should offer Change contract

FlowCoordinator.accepted_contract_offer()
# the accepted offer's fields plus "ok": true, or {"ok": false}

FlowCoordinator.contract_started()
# true once an encounter on this contract has been entered
```

Offers are listed shortest first. An offer can be swapped for another until
`contract_started()` turns true, when the first encounter is entered. That
stays true if the player is caught in it, even though nothing has been
completed. Accepting only changes run state; it does not navigate. The hub's
offer picker accepts and then redraws the map itself. The accepted offer's
`credit_reward` is what winning the contract pays (see below).

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
    which updates `PlayerStateGlobal` and records the change together. The
    contract reward therefore counts towards `credits_earned`.
  - The caught fine is a loss, so it is not recorded and does not need to be.
- **Cards added** counts cards that go into the run's deck on top of the
  starter deck. Every card is added through
  `FlowCoordinator.add_card_to_deck(card_id)`, which only counts a card the
  deck accepted. `take_card_reward` uses it. A shop purchase is the one
  exception: `ShopNode` takes the price and adds the card in a single Rust
  call, then records the card (and the spend) on the run itself, the same way
  `EventNode` records event credits.

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

A new run can start from the hub, the caught screen or the victory screen,
which is where a run ends. It is refused during an encounter, with nothing changed, because that
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

## Random events (story #116)

`event_catalogue()` in `events.rs` holds every event an event node can show,
eight at the moment. Each one has at least two choices, and each choice's
preview quotes the credits and chances it resolves to, which a unit test
checks.

The run remembers which events it has shown, so one run does not repeat an
event. `EventNode.roll_event()` asks `RunStateNode`, which rolls from the
events the run has not seen yet and records the one it picked. When a run has
seen every event, the roll picks from the whole catalogue again, so an event
node always has something to show. The seen list lives on `RunState`, so a new
run starts with the full pool, and it is saved with the run, so a continued run
keeps avoiding the events it has shown. A save from before the list existed
loads as a run that has seen none, and one naming an event this build does not
have is refused. With no run behind it, as when the event scene is opened on
its own, `EventNode` picks any event.

## Winning a contract (stories #89 and #23)

A contract is won by beating the boss of its **final** zone. Every earlier
boss still only opens the next zone and leads back to the map through its card
reward, exactly as before.

`RunState::complete_encounter` decides which it is with
`active_encounter_wins_contract()`: the active encounter is a boss and sits in
the contract's last zone. Completing it moves the run to `contract_won` rather
than `hub` and, in the same transition, records the contract reward on the run
(`RunState::contract_reward`). The win is still counted like any other fight.
A contract with no boss on it, such as the authored test contract, is never
won this way.

`FlowCoordinator.complete_active_encounter()` reads everything it needs about
the encounter in one call, `active_encounter_completion()` (`{ok, type,
credits, guaranteed_rare, wins_contract}`), before it completes. A winning
encounter prepares the victory screen (`scenes/victory_screen.tscn`) instead of
the card reward. The run is over, so no card is offered for a deck that is
about to be reset. Once the transition succeeds it:

1. pays the final boss's own encounter credits, like any boss, then
2. reads the contract reward the transition recorded and pays it with
   `change_credits`, so it lands on `PlayerStateGlobal` and counts as credits
   earned on the run.

**The reward is the accepted offer's `credit_reward`**, which grows with the
contract's length and boss count (see Contract offers). Players always accept
one on the hub's offer picker before the map appears. The default contract a
run is created on was never offered, so it pays 0, but only a test that skips
the picker can win it.

The reward cannot be paid twice. It is recorded only by the transition into
`contract_won`, and that phase is terminal, so the transition and the payment
straight after it happen once per contract. A caught contract never reaches
`contract_won`, so being caught pays no contract reward.

```gdscript
FlowCoordinator.contract_reward()
# {"ok": true, "credits": 495} once the contract is won, else {"ok": false}
```

The reward lives in run state rather than in FlowCoordinator, so the victory
screen shows the same thing whenever the run's current screen is rebuilt while
it is `contract_won`, including by Continue run (see Saving and continuing).
The final boss's own encounter credits are FlowCoordinator's pending reward,
which the save keeps too, so Continue shows that line again; a screen rebuilt
without it leaves the line out.

The victory screen names the contract, summarises the run from
`run_summary()`, says what the contract and final boss paid, and counts the
balance up to the player's current balance from the reward below it over 1.5
seconds. The credits are already in the balance when it opens; the count is
only how the screen shows them arriving. For tests it exposes
`displayed_balance`, `is_counting()`, and the `displayed_balance_changed` and
`balance_count_finished` signals.

The combat screen's **Complete Encounter** button is a testing shortcut that
skips a fight but still collects its payout. It is disabled on the final boss
(`FlowCoordinator.active_encounter_wins_contract()`), and its handler refuses
there too, so skipping can never win the contract reward. The final boss has
to be beaten.

**Take another contract** calls `FlowCoordinator.start_new_run()`, the same
clean reset as after being caught: a fresh contract and offers, no zone
progress or stats, noise and knowledge reset, the starter deck and no
upgrades. The credit balance, reward included, carries over. The hub then
opens on the offer picker, so the next contract is chosen there.

Run the headless check with a built extension:

```sh
godot --headless --path godot -s res://tests/contract_victory_test.gd
```

It checks a new run opens on the offer picker with the map hidden, cycles the
picker to the offer with the most zones and accepts it there, as a player
would, then uses Change contract to swap to another offer and back. It walks
the contract without playing the ordinary encounters, checking that each
earlier boss returns to the map with the next zone open, no contract reward
and Change contract gone. On the final boss it checks the Complete Encounter
shortcut is disabled and does nothing when pressed anyway, then beats the boss
with a real Trojan play. It checks the victory screen replaces the map, the
offer's reward is paid exactly once and counted as earned, the balance counts
up in steps to the new total, and the screen shows the same reward when it is
rebuilt from run state alone. Finally it takes another contract from the
victory screen, checks the hub asks for a contract again, accepts one through
the picker, and checks that being caught on it costs the fine and pays no
reward.

## The dark web shop (story #38)

A shop node opens `scenes/shop.tscn`, a dark web market where the player
spends credits on cards for the rest of the run. It delivers the epic through
four stories: S1 shop stock and pricing (#104), S2 buying with credits (#105),
S3 the Scrub Logs service (#106) and S4 rerolling the stock (#107). The rules
are plain Rust in `cyber_heist/src/shop.rs`; `ShopNode` (`shop_node.rs`) sits
on the shop screen as its bridge, as `EventNode` does for events.

**Stock.** The first time the screen opens during a shop encounter, `ShopNode`
asks the run to open its `ShopVisit` (`RunState::open_shop_visit`), which rolls
five distinct cards (`STOCK_SIZE`) from the card database. Cursed cards are not
in that pool. Each card's price comes from its rarity (`price_for`):

| Rarity | Price |
| --- | --- |
| Common | 50 |
| Uncommon | 75 |
| Rare | 100 |
| Legendary | 150 |

The visit is kept on the run, not on the screen, until the shop encounter
ends. Rebuilding the screen during the visit therefore shows the same stock,
sold slots, Scrub Logs and reroll price rather than a free reroll; leaving (or
any other end to the encounter) drops it, so the next shop rolls its own. The
stock only changes when the player pays to reroll it (see below). A bought
card's slot is marked sold rather than removed, so the other cards keep their
places and the screen shows `SOLD` in that row.

**Buying.** `RunState::buy_from_shop` is the only way to buy a card, and it
only works while the run is in a shop encounter whose visit is open (otherwise
`not_in_shop`). Through `ShopVisit::buy` it checks the slot and the balance
before changing anything, then takes the exact price from `PlayerStateGlobal`,
adds the card to the run's deck on `RunDeckNode` and records the spend and the
card on the run's stats, all in one call. A refusal (`not_enough_credits`,
`already_sold`, `no_such_slot`, `not_in_shop`) changes neither the credits, the
deck, the stock nor the stats, and a purchase can never take the balance below
zero. A balance already below zero from a caught fine simply affords nothing.
A bought card is in the run's deck at once, so `DrawPhase` deals it in the
next encounter.

This is the shop's counterpart to `FlowCoordinator.change_credits` and
`add_card_to_deck`, which cannot refuse a payment the player cannot cover, so
a purchase does not go through them. Every card the shop sells was drawn from
the card database when the stock was rolled, which is the check
`RunDeckNode.add_card` would otherwise make.

The screen reads `affordable` for each slot against the current balance. A
card the player cannot afford has its buy button disabled and its price in
red, and the screen re-reads the stock after every purchase. Its rows are
matched to the stock by `slot`, the same number `buy` takes.

**Leaving.** Leave calls `FlowCoordinator.complete_active_encounter()`. Shops
pay no encounter reward and are not in `REWARDING_ENCOUNTERS`, so the player
goes straight back to the map with the shop node marked completed. Shops are
not fights, so they do not count as wins.

```gdscript
$ShopNode.stock()
# [{"slot": 0, "id": "vpn", "name": "VPN", "type": "Skill", "rarity": "Common",
#   "cost": 2, "cost_text": "Costs 2 energy", "noise_text": "Reduces noise by 10",
#   "description": "...", "keywords": [{"label", "explanation"}],
#   "price": 50, "sold": false, "affordable": true}, ...]

$ShopNode.buy(0)
# {"ok": true, "id": "vpn", "price": 50, "credits": 70},
# or {"ok": false, "error": "not_enough_credits"} with nothing changed
```

**Services.** Two paid services sit beside Leave at the foot of the screen,
so the five cards keep the rest of it. Their prices are constants in
`shop.rs`:

| Service | Price | Effect |
| --- | --- | --- |
| Scrub Logs (#106) | 60 (`SCRUB_LOGS_PRICE`) | Noise −20 (`SCRUB_LOGS_NOISE_REDUCTION`), once per visit |
| Reroll (#107) | 25, then 50, 75, … (`REROLL_BASE_PRICE` + `REROLL_PRICE_STEP` per earlier reroll) | A new card in every unsold slot |

- **Scrub Logs** (`ShopVisit::scrub_logs`) lowers the shared
  `NoiseMeterGlobal` by 20, clamped at zero by the meter. 60 credits is a bit
  more than a common card, for two plays' worth of a common recovery card
  without spending a turn on them. Once bought it shows `SOLD OUT` for the rest
  of the visit; the next shop sells it again. When it would not lower noise at
  all it is refused (`nothing_to_scrub`), and `scrub_logs_offer` reports it
  unaffordable so its button is disabled and its price red: it never takes
  credits for nothing. That check is on the noise it would actually remove
  after detection resistance (`scrub_logs_reduction`), not just on noise being
  above zero. Shops have no sentry, so resistance is zero there today, but the
  rule does not depend on it.
- **Reroll** (`ShopVisit::reroll`) replaces the card in every unsold slot with
  a fresh roll from the same pool, priced by its rarity. Sold slots stay sold
  and every slot keeps its place. A replacement is always a card that was not
  in the stock before the reroll, unless the pool is too small to have enough
  of them, and the stock never holds the same card twice. The first reroll in a
  visit costs one zone 1 combat's pay; each one after costs 25 more, so it is a
  way to hunt for one card rather than to see the whole pool for a flat fee.
  The price starts at 25 again at the next shop. A reroll is refused
  (`nothing_to_reroll`) when it could not change anything: every card is sold,
  or every card in the pool is already in the stock. The replacements are
  rolled before the price is taken, so a reroll that is charged always refills
  every unsold slot.

**One charging path.** Buying a card, Scrub Logs and rerolling all spend
credits through `ShopVisit`'s one private `charge` step, after every other
check, and each goes through one `RunState` call (`buy_from_shop`,
`scrub_logs_in_shop`, `reroll_shop`) that also records the spend on the run as
a credit change, which does not count towards `credits_earned`. A refusal
(`not_enough_credits`, `already_sold`, `no_such_slot`, `nothing_to_scrub`,
`nothing_to_reroll`, `not_in_shop`) changes neither the credits, the deck, the
noise meter, the stock nor the stats, and nothing can take the balance below
zero. A balance already below zero from a caught fine simply affords nothing.
What a service's offer reports as `affordable` is the same check its action
makes (`can_buy_scrub_logs`, `can_buy_reroll`), so like a card, a service the
player cannot buy right now has its button disabled and its price in red.

```gdscript
$ShopNode.scrub_logs_offer()
# {"price": 60, "reduction": 20, "sold": false, "affordable": true,
#  "noise": 70, "max_noise": 100}

$ShopNode.scrub_logs()
# {"ok": true, "price": 60, "noise_removed": 20, "noise": 50, "credits": 40},
# or {"ok": false, "error": "already_sold"} with nothing changed

$ShopNode.reroll_offer()
# {"price": 25, "affordable": true, "available": true}

$ShopNode.reroll()
# {"ok": true, "price": 25, "next_price": 50, "credits": 25},
# or {"ok": false, "error": "not_enough_credits"} with nothing changed;
# read stock() again for the new cards
```

The visual design of this screen belongs to Neel (see the S1 to S4 cards). The
current screen is a plain, functional version in the main menu's terminal
style that reads everything from `ShopNode`, so a redesign only has to replace
`shop.gd` and `shop.tscn`.

Run the headless check with a built extension:

```sh
godot --headless --path godot -s res://tests/shop_test.gd
```

It starts from the main menu, accepts a contract on the hub's offer picker, and
rolls new runs until a first-zone route offers a shop, then clicks the shop on
the map with 120 credits. It checks the five rows and their details and prices,
buys through the buy buttons until a card is out of reach, and checks the exact
deduction, the sold slot, the unchanged stock (also after rebuilding the shop
screen mid-visit), and that the out-of-reach card's button is disabled, its
price red and a forced attempt changes nothing. It then plays the services,
reading their prices from `ShopNode` (the Rust tests pin the numbers): Scrub
Logs held back and red with no noise, refused one credit short, then bought at
70 noise to lower it by its reduction for exactly its price, then shown as sold
out and refused a second time; a reroll from twice its price that takes exactly
that price, keeps the sold slots and puts a new card in every unsold one, then
a dearer next reroll that is red and refused, and paid for with more. It buys
one of the rerolled cards. Then it presses Leave and checks the map, not the
offer picker, and the completed node. Finally it enters the next encounter and
draws the whole deck to find every card it bought, by id, since at the noise
Scrub Logs left some cards show their weak-side names.

## Sound and music (story #111)

The `AudioDirector` autoload (`godot/autoload/audio_director.tscn`) owns
everything the player hears. Sound goes through three buses, defined in
`godot/default_bus_layout.tres`:

| Bus | Carries | Sends to |
| --- | --- | --- |
| `Master` | everything | the output |
| `Music` | the background loop (`MusicPlayer`) | `Master` |
| `SFX` | card sounds (one player per cue, `Sfx_<cue>`) | `Master` |

**Card sounds.** Every card type has its own sound. `card_play` reports the
played card's type, and `DrawPhase.play_card()` adds `type` (the display name,
e.g. `"Social Engineering"`) and `sound_cue` to its result. The cue comes from
`SoundCue::for_card_type` in `cyber_heist/src/audio_mix.rs`, and the combat
screen hands it to `AudioDirector.play_card_sound(cue)`:

| Card type | `sound_cue` | Sound |
| --- | --- | --- |
| Attack | `attack` | a hard downward zap with a burst of static |
| Defence | `defence` | a rising, humming shield swell |
| Skill | `skill` | two quick rising blips |
| Recon | `recon` | a sonar ping with one echo |
| Social Engineering | `social_engineering` | a sly, voice-like murmur |

Each cue's sound is `godot/assets/audio/sfx/<cue>.wav`, so adding a card type
means adding a cue in Rust (including `SoundCue::ALL`) and a file of the same
name; a Rust test checks every card type the card data uses has both.

Each cue plays on its own `AudioStreamPlayer`, made when the cues load, whose
stream never changes. Giving a player a new stream stops whatever it was
playing, so a single shared player would cut one card's sound off as soon as
the next card went down. Each player allows four overlapping plays
(`EFFECT_POLYPHONY`), so the same card twice in quick succession layers rather
than restarting.

**Music.** An eight-second tense loop (`assets/audio/music/heist_loop.wav`)
starts when the game launches and keeps going across every screen. It loops
because its `.import` file sets `edit/loop_mode=2`. The autoload runs with
`process_mode = always`, so the music carries on under a pause menu later.

**Volume.** There was no settings screen before this story, so it adds a
minimal one: **Settings** on the main menu opens `settings_menu.tscn`, with a
slider each for Master, Music and Effects. Moving a slider applies to its bus
at once. Saving to `user://settings.cfg` (section `audio`, keys `master`,
`music`, `sfx`) waits until the sliders have been still for 0.4 seconds, so a
drag is one write rather than one per step, and leaving the screen saves
anything still waiting. `AudioDirector` loads the file on every launch.
Missing or unreadable values fall back to the defaults (80, 60, 80). A file
that exists but cannot be parsed is copied to `settings.cfg.broken` before a
fresh one is written, so nothing in it is silently lost. Display options are
left to the settings menu story, which can add them to the same screen and
file.

Settings open only from the main menu (`FlowCoordinator.open_settings()`
refuses anywhere else), because Back always returns there. A pause menu that
opens settings mid-run will need them to remember where they came from.

A volume is a whole percentage, clamped to 0..100 by `Volume` in
`audio_mix.rs`. The bus level follows the square of the slider (100% is 0 dB,
50% about -12 dB, 10% -40 dB). **0 is silence**: the bus is muted outright and
set to -80 dB, so Music at 0 silences the music while effects carry on, and
Master at 0 silences everything. GDScript reaches these rules through the
static `AudioRules` class:

```gdscript
AudioDirector.volume("music")              # 0..100
AudioDirector.set_volume("music", 0)       # applies now; false for an unknown setting
AudioDirector.save_settings()              # OK, or the ConfigFile error
AudioRules.volume_db(50)                   # -12.04
AudioRules.is_silent(0)                    # true
```

**Headless runs.** With no audio device (the `Dummy` driver that `--headless`
uses) `AudioDirector` sets streams and buses up exactly as usual but does not
start the players. Nothing could be heard, and Godot 4.7.1 only frees a
stopped playback on the audio server's next update, which never comes once
the engine is quitting: a headless playback is reported at exit as leaked
objects and `ERROR: ... resources still in use`, even when the players are
stopped in `_exit_tree`. Every suite starts the music, so that line would
trip the error checks. `is_music_on()` (true only once the music stream has
loaded) and `last_sound_cue` say what would be playing. The headless test
checks the routing that way:

```sh
godot --headless --path godot -s res://tests/audio_test.gd
```

It plays one card of every type and checks each plays on its own SFX player
holding its own stream, and that no player's stream was swapped by the others.
It checks the music loops on the Music bus, that setting Music, Master or
Effects to 0 in the settings screen mutes that bus and only that bus, that the
settings are saved once the sliders settle (not on every step) and on Back,
that they load back, that an unparseable settings file is kept as `.broken`,
and that settings refuse to open from inside a run. It uses scratch settings
files so it never overwrites a developer's own.

The sounds are synthesised by `tools/generate_audio.py` (Python and numpy),
so there is no third-party audio to license; see `godot/assets/CREDITS.md`.

## Display options (story #46)

Below the volumes, the settings screen has four display options. Each row's
button cycles through its choices: pressing it moves to the next one, and
left/right step either way while it is selected. A change applies at once and
is saved straight away, since there is no slider drag to wait for.

| Option | Choices | Default |
| --- | --- | --- |
| Display mode | Windowed, Fullscreen | Windowed |
| Window size | 1280 x 720, 1600 x 900, 1920 x 1080 | 1280 x 720 |
| VSync | On, Off | On |
| CRT effect | On, Off | On |

The window size is greyed out while fullscreen, which uses the screen's own
size. A windowed size change re-centres the window on its screen.

The `DisplaySettings` autoload (`autoload/display_settings.gd`) owns the
options. It keeps them in the `[display]` section of `user://settings.cfg`,
next to AudioDirector's `[audio]` section. Each one only writes its own
section, so saving one keeps the other. The options are applied at launch.
A missing value, a value of the wrong type or an out-of-range size falls back
to its default. An unreadable file is never overwritten by a display save
(AudioDirector copies it aside the next time it saves).

The CRT effect is the `CrtOverlay` ColorRect that most screens draw on top.
`DisplaySettings` shows or hides each one as it enters the tree, and every one
already showing when the option changes, so screens don't have to read the
option themselves. A new screen with an overlay only needs to name it
`CrtOverlay`.

Headless there is no window, so the options are stored, saved and loaded as
usual but nothing is asked of the window. The test checks what is chosen,
shown and saved, using a scratch settings file:

```sh
godot --headless --path godot -s res://tests/display_settings_test.gd
```

## Saving and continuing (story #10)

The game saves itself; there is no save button. One JSON file,
`user://cyberheist_save.json`, holds two things:

- **Meta progress**, what the player keeps between runs: the credit balance.
  It is applied as soon as the game opens, so it carries into a new contract
  as well as a continued one. Upgrades last only for the contract they were
  earned on and zones open per contract, so both are part of the run instead;
  nothing else is unlocked between runs yet.
- **The run** at its last safe point: the contract phase, the offers and the
  accepted one, whether an encounter on the contract has been entered yet,
  the map with its completed and locked nodes and open zones, the contract
  reward once the contract is won, the run's stats, a shop's stock while the
  run is in one, its deck, the contract's upgrades, noise and knowledge.

The format is plain Rust in `cyber_heist/src/save_game.rs`, serialised with
serde. `SaveGameNode` (`save_game_node.rs`) sits under `FlowCoordinator` beside
`RunState` and `RunDeck`, gathers those pieces from the scene tree to save them
and puts each one back on Continue.

**Safe points.** `FlowCoordinator.save_progress()` replaces the save at every
one: New contract, accepting an offer, entering a shop and each purchase in it
(the shop screen calls it), every completed encounter, leaving the card reward,
being caught (once the fine is taken) and leaving the caught screen, winning a
contract, and `start_new_run()`. Nothing is saved in the middle of a fight or
an event: `SaveGame::new` refuses that phase. Quitting during one therefore
resumes at the map as it was before the encounter was entered, with the noise
picked up inside it gone and the node still to play. A shop is the one
encounter that is a safe point. Its stock (`ShopVisit`) lives on the run (see
the dark web shop), opened as the shop screen appears and before the save made
on entering, so it is saved with the run and a resumed shop has the same cards
and sold slots rather than a reroll.

**Continue** on the main menu is enabled when a run was read at launch.
`continue_run()` restores it and opens the screen it was saved on: the map, a
fight's card reward still to choose, the shop, the caught screen or the
victory screen. It is all or nothing: the screen is prepared from the saved
run (`SaveGameNode.saved_run_snapshot()`) before anything is restored, and the
restore finds every node it writes to before changing any, so a failure leaves
the run behind the menu exactly as it was. The coordinator's screen results are saved with the run, so a
resumed screen shows what was already paid rather than paying it again: the
pending card reward and the caught penalty. The caught
screen takes its fine through `FlowCoordinator.apply_caught_penalty(fine)`,
which charges once per capture, so a caught screen reopened by Continue never
fines twice. The card reward's offer is rolled once and kept with the pending
reward (`offered_card_ids`), so a resumed card reward offers the same three
cards rather than a free reroll.

A won contract needs no screen result of its own. `RunState` records the
contract reward (`contract_reward`) in the transition into `contract_won`, and
it is saved with the run, as is the balance it was already paid into. The
resumed victory screen reads the reward from run state and counts up to the
saved balance, and nothing pays it again: only completing the final boss pays
it, and a won contract has no encounter left to complete.

Accepting an offer does not commit the run to it until an encounter is
entered, and that survives Continue: run state saves whether the contract has
started, so a run saved after accepting, or changing, an offer resumes on the
map with **Change contract** offered, and one saved before accepting resumes
on the offer picker. Once an encounter has been entered, Continue no longer
offers the change.

**New contract** on the menu replaces the saved run at once with the fresh run
behind the menu. The credit balance read from the save stays.

**Missing, corrupt or incompatible saves.** Every save has a `version`
(`SAVE_VERSION`, currently 3), read before anything else. A missing file is a
first launch: Continue is unavailable and no error is shown. A file that is not
valid JSON, is missing fields, was written with another version, or describes a
run the game could never reach is refused. That covers, for example: a
current node that is not on the map; a map that is not the one the accepted
offer builds from its seed and shape; a contract under way on no accepted
offer; an accepted offer that was never offered; an offer whose shape or reward
no contract is offered with; a fight in progress; a card this build does not
have; a contract reward on a contract not won, or one that is not the accepted
offer's `credit_reward`; a won contract without one; progress on a contract
marked as not started, which Change contract would throw away; noise or
knowledge outside the meters' caps (`MAX_NOISE`, `MAX_KNOWLEDGE`, checked as
constants so the check does not depend on the order the autoloads start in);
a shop's stock outside a shop, with more than five cards, a card twice or a
price that is not its card's rarity price; and a card reward whose credits or
Rare guarantee are not those of the fight just won, that offers more cards
than the reward screen (`card_reward::OFFER_SIZE`) or a card twice, or that
guarantees a Rare and offers none.

A refused save's run is not loaded. The game starts with Continue
unavailable, and the menu says the save could not be loaded. Before anything
can replace the file, it is copied to `<save>.unreadable`, and if it still
names a credit balance (an older version, say) that balance is kept, since it
is the one thing meant to carry between runs. The next save then replaces the
original.

Saves are written to a temporary file, flushed to disk, renamed into place,
and the directory flushed in turn, so closing the game or losing power
mid-write leaves either the previous save or the new one. Bump `SAVE_VERSION`
whenever a change to anything saved would stop an older file reading back
correctly. Version 2 replaced version 1's claimed-reward flag and separate
contract payout with the reward run state records, and version 3 moved a
shop's stock into the run state, so earlier saves are refused as
incompatible.

```gdscript
FlowCoordinator.has_saved_run()       # true when Continue can resume a run
FlowCoordinator.continue_run()        # {"ok": true}, or {"ok": false, "error"}
FlowCoordinator.save_progress()       # {"ok": true}, or {"ok": false, "error"}
FlowCoordinator.save_load_error()     # "" unless the save could not be read
FlowCoordinator.use_save_file(path)   # {"ok": true, "has_run"}, or {"ok": false, "error"}
```

**Tests and the save file.** Headless tests drive the game from their own
`SceneTree` script under `res://tests/`, and `FlowCoordinator` turns saving off
when the main loop runs one of those, so no test reads the player's save,
another test's, or writes one. Only a script under `res://tests/` counts, so a
custom `MainLoop` the game itself might use would keep saving. A test that
wants saving calls `use_save_file()` with its own `user://` file.

Run the headless check with a built extension:

```sh
godot --headless --path godot -s res://tests/save_continue_test.gd
```

It opens the game with no save, then with a corrupt one, one from an unknown
version and a version 1 one, and checks Continue stays unavailable, the corrupt
file is kept as `.unreadable`, the balance an incompatible file names is kept
and New contract replaces the bad file. It checks a card reward naming
something that is not a card id is refused at save time. It accepts an offer on the
picker and uses Change contract to swap it, then resumes in a second process,
which checks the map still offers Change contract and that changing again is
saved. It plays two encounters and checks each was saved without being asked,
passes through any shops that are all the map offers next, then enters a
fight or an event and quits partway through it. A second Godot process opens the same save, presses
Continue and checks the contract, offers, map, completed nodes, zone, credits,
deck, upgrades, noise, knowledge, stats, phase and screen against what the
first process wrote down at the hub. It does the same on the caught screen,
where the fine must not be taken twice, in a shop after buying a card,
where the stock must come back with that slot sold, and on a fight's card
reward before a card is chosen, where the same cards must be offered again.
After the hub resume it checks Change contract is no longer offered. Last, it
plays a contract to its final boss, checks the save records the contract
reward in run state and keeps no separate payout, and resumes on the victory
screen: the same reward and boss payout are shown, the count ends on the saved
balance without paying again, and Take another contract starts a new run at the
offer picker.

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
is what makes ending a turn a decision rather than a formality. Even standard
security does more than make noise (see "Standard security's actions" below),
and standard and elite security turn louder once noise reaches 60 (see
"Security on alert" below).
`sentry_node.rs` exposes it to the combat scene:

```gdscript
$Sentry.construct_name()          # "WARDEN-7"
$Sentry.noise()
$Sentry.max_noise()
$Sentry.queued_action_name()      # "" when nothing is queued
$Sentry.queued_action_noise()
$Sentry.queued_action_ability()   # "-1 energy", or "" when it only makes noise
$Sentry.queued_action_escalated() # true when it comes from the alert set
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

Standard security is the authored WARDEN script with its noise scaled by zone,
so zone 1 is WARDEN-7 as authored. A zone's boss is then defined against the
security around it rather than against a fixed bar:

- every one of its actions is louder than the loudest standard action in that
  zone,
- it resists detection 30 points harder than standard security there,
- it has more integrity than standard security's 40, starting at 80 for
  ICEBREAKER and rising 20 per zone (BLACK MONOLITH 100, THE ARCHITECT 120),
  so every boss is a longer fight than the one before it, and
- it carries a signature ability no standard construct has. BLACK MONOLITH's
  Firewall is the closest: a heavier version of standard security's Firewall
  Up (12 against 8).

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

### Standard security's actions

Ordinary nodes are not noise alone (Trello #102, E4). Every standard construct
cycles the same five actions, with the noise multiplied by the zone (zone 1 is
WARDEN-7 as listed):

| Action | Noise | Ability |
| --- | --- | --- |
| Packet Sniff | 1 | |
| Firewall Up | 1 | blocks the next 8 plain damage |
| Trace Sweep | 2 | |
| Inject Malware | 1 | adds 1 Injected Malware to your discard |
| Lockdown Probe | 3 | |

**Firewall Up** (E4a) is the same firewall as BLACK MONOLITH's, smaller: it
goes up on the construct's turn, absorbs plain damage before integrity, and
comes down when its next turn starts. Its 8 do not scale by zone.

**Inject Malware** (E4b) plants an **Injected Malware** card in the player's
discard pile. The card lives in `cursed_cards.ron` as `injected_malware`; it is
not called Malware because `cards.ron` already has a player attack with that
id and name. It costs 1 energy, makes no noise, does nothing, and has
**Exhaust**, so playing it removes it from the encounter instead of
discarding it. `play_card` reports that as `exhausted`.

The planting is split because the deck belongs to `DrawPhase`. `SentryNode`
records the count, reports it as `malware_planted` in the turn outcome, and
`DrawPhase.end_turn()` takes it straight after the `security_phase` signal,
before the next hand is drawn, and puts the cards in its own discard pile
(`card_play::plant_junk`). It never touches `RunDeckNode`, and each encounter
builds its deck fresh from the run's deck, so Injected Malware never outlives
the encounter it was planted in. `DrawPhase.draw_pile_ids()` and
`discard_pile_ids()` let a screen or test see what is in each pile.

### Elite encounters

An elite node is a fight the player chooses to take on for a better payout
(Trello card E1). It sits between the two other tiers, so it is measured
against the same zone's standard security:

| | Standard | Elite | Boss |
| --- | --- | --- | --- |
| Integrity | 40 | 60 | 80 (+20 per zone) |
| Resistance | +10 per zone | standard + 15 | standard + 30 |
| Ability | Firewall Up, Inject Malware | standard's, plus Reinforce | its own signature (see above) |
| Credits (zone 1, +per zone) | 25 (+15) | 2x combat | 3x combat |
| Card reward | any 3 | at least one Rare | at least one Rare |

The elites are BASTION, HYDRA and CERBERUS, in zone order. Their script is the
zone's standard one, renamed and one point louder per action (Deep Scan,
Hardened Firewall, Hardened Trace, Payload Drop, Counter Probe), followed by
**Reinforce**: one point of noise, plus 8 integrity restored to the construct
(4 more per zone deeper). It is announced with the intent as `+8 integrity`, so
a player can try to finish the elite before it lands. Because it acts on the
construct itself, `SentryNode` applies it and reports the clamped amount
regained as `integrity_restored` in the turn outcome. There is nothing for the
combat screen to apply, unlike a lockdown; it only says so.

Payouts live in `cyber_heist/src/encounter_reward.rs`. `FlowCoordinator`
reads `RunStateNode.active_encounter_completion()` before completing an encounter
(completing clears it), and adds the credits to `PlayerStateGlobal` only once
the transition succeeds. The card reward screen then reads the guarantee
through `offer_card_reward()` and shows the payout with
`pending_reward_credits()`. Events and shops settle their own credits, so they
pay nothing here.

`card_reward::offer_with_guarantee` builds a guaranteed offer. It picks one
card of the guaranteed rarity or better first, fills the rest from everything
else so the offer stays distinct, and shuffles where the guaranteed card lands.
A pool with no such card falls back to an ordinary offer.

### Security on alert (story #101)

Standard and elite security react to how loud the player is being. Each has an
alert set alongside its script, one action for each slot. While noise is at or
above `ALERT_NOISE` (60 of 100) the construct takes the alert action for the
queued slot instead of the standard one. Zone 1 values:

| Slot | Standard | Alert | Alert noise | Alert ability |
| --- | --- | --- | --- | --- |
| 1 | Packet Sniff | Deep Packet Inspection | 2 | |
| 2 | Firewall Up | Firewall Surge | 2 | blocks the next 10 plain damage |
| 3 | Trace Sweep | Active Trace | 3 | |
| 4 | Inject Malware | Malware Flood | 2 | adds 2 Injected Malware |
| 5 | Lockdown Probe | Lockdown Sweep | 4 | |

Each alert action is the standard one's noise (scaled by zone as usual) plus
one, so an alert turn is louder than a calm one but still quieter than the
zone's boss. Firewall Surge stays under BLACK MONOLITH's 12. An elite's alert
set uses the same actions one point louder again, under its own names (Total
Scan, Fortified Firewall, Relentless Trace, Payload Barrage, Counter Lockdown),
then **Emergency Reinforce**, which restores what Reinforce does. Bosses do not
go on alert: they are already louder than everything around them, and THE
ARCHITECT's second phase is its own change of action set.

Noise only chooses which of the two actions fills the slot. It never moves the
queue, so the next slot comes next whichever set the last action came from.
The choice is live: `Sentry::queued_action(noise)` is read at the current
noise, and the combat screen reads the intent again after every card. A loud
card that takes the meter to 60 flips the intent to the alert action straight
away, and recovery that takes it back under 60 flips it back, before the turn
ends. When the turn begins, `SentryNode` performs the action for the noise at
that moment, which is the one the intent showed last.

The intent marks an alert action as `[ESCALATED]`, as does the turn report
(`WARDEN-7 ran Active Trace [ESCALATED] and added 3 noise.`). The turn outcome
carries `alert`. In code this is called the alert set so it does not clash
with THE ARCHITECT's `escalated`, which means its change of phase.

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
    "firewall": 0,      # the firewall now standing, from Firewall Up or a boss
    "malware_planted": 0, # Injected Malware headed for the discard pile
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

`DrawPhase.play_card(index)` returns `{ok: true, name, type, sound_cue,
noise_change, ...}` or `{ok: false, error}`. The plain Rust `card_play` transaction rejects an
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
`tests/combat_fixture.gd` keeps the real coordinator and Rust lifecycle and only
enters from the hub. On a contract where nothing has been entered yet
(`contract_started()` is false) it always enters a real combat node, rolling
fresh contracts in place (`RunStateNode.start_new_run`) until column one offers
one. Once an encounter has been entered it never replaces the run, even after
being caught in the first one, so re-entering is that encounter's retry. It
prefers a combat node, otherwise mounts combat content on an event or shop
node, and fails if only elites or the boss are offered, since those field their
own construct rather than WARDEN-7. These tests exercise combat, not the random
encounter scene registry. The shared noise test moves to a fresh contract
explicitly when the column after its first encounter holds only elites.

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

For credits from combat (#114), run:

```sh
godot --headless --path godot -s res://tests/combat_credits_test.gd
```

It fills the meter during a first-zone standard combat and checks that being
caught pays nothing: only the fine comes off, credits earned and wins are
unchanged, and nothing is paid when leaving the caught screen. It then retries
the same fight and wins it by playing a Strike, the card-play win path, to check
that it pays 25 credits, counts them in the run's stats and shows them on the
reward screen.

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
startup registration and runtime tests succeeded. It does not prove that
first-discovery path works. Godot 4.7.1 itself occasionally crashes or hangs
during the fixture's headless import, so the runner retries that import up to
three times, but only for crash and hang exits (124, 134, 137, 139). Any other
exit fails at once. Each retry starts from an empty resource cache, so the
attempt that succeeds imports every resource itself and its log is the one
scanned for errors. It does not suppress import errors or test failures: the
runner fails if every attempt crashes, if the successful import logs an error,
or if the test run (which is never retried) fails.

### Shared lifetime and caught recovery

Completing or replacing a normal encounter preserves shared noise. Entering
caught also retains the full meter while the detection screen is displayed.
After `finish_caught()` successfully transitions back to the hub, the coordinator
clears the shared meter to zero before showing the hub. A rejected transition
never clears noise. The next attempt can play cards and recover normally, while
ordinary encounter progression still carries noise forward.

## Scene-facing API

```gdscript
FlowCoordinator.open_settings()
FlowCoordinator.close_settings()
FlowCoordinator.contract_offers()
FlowCoordinator.accept_contract_offer(index: int)
FlowCoordinator.accepted_contract_offer()
FlowCoordinator.contract_started()
FlowCoordinator.needs_contract_choice()
FlowCoordinator.can_change_contract()
FlowCoordinator.active_encounter_wins_contract()
FlowCoordinator.contract_reward()
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
FlowCoordinator.has_saved_run()
FlowCoordinator.continue_run()
FlowCoordinator.save_progress()
FlowCoordinator.apply_caught_penalty(fine: int)
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

The caught screen applies its separate `PlayerStateGlobal` penalty once per
capture, through `FlowCoordinator.apply_caught_penalty(fine)`, so a caught
screen reopened by Continue does not charge it again. The coordinator continues
to own navigation. Both autoloads remain registered,
and the Rust crate registers the caught-penalty and game-flow classes together.
