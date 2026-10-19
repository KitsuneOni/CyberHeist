# Real scene coverage for credits from combat (Trello card #114). The payout
# rules live in Rust (encounter_reward.rs), but the decision to pay is made by
# FlowCoordinator when an encounter completes, so only a real combat screen can
# show that being caught pays nothing and winning the same fight still pays.
#
# The elite test already covers "bigger fights pay more"; this covers the
# other two scenarios on the card:
#   - Earn credits for a win
#   - No credits when caught
#
# It also checks that the shared combat fixture keeps a run whose first
# encounter the player was caught in, since this is where that fixture's
# misuse checks live.
#
# Run: godot --headless --path godot -s res://tests/combat_credits_test.gd
extends SceneTree

# A first-zone standard combat pays 25 credits. See the payout table under
# "Elite encounters" in docs/game-flow.md.
const STANDARD_COMBAT_CREDITS := 25

const CombatFixture := preload("res://tests/combat_fixture.gd")

var _flow: Node
var _noise_meter: Node
var _player: Node
var _failures: Array[String] = []


func _initialize() -> void:
	_run.call_deferred()


func _check(condition: bool, description: String) -> void:
	if condition:
		print("  ok    %s" % description)
	else:
		print("  FAIL  %s" % description)
		_failures.append(description)


func _screen() -> Node:
	var host := root.get_node_or_null("Main/ScreenHost")
	if host == null or host.get_child_count() == 0:
		return null
	return host.get_child(0)


func _finish() -> void:
	print("")
	if _failures.is_empty():
		print("combat credits test: PASSED")
		quit(0)
	else:
		print("combat credits test: FAILED (%d)" % _failures.size())
		for failure in _failures:
			print("  - %s" % failure)
		quit(1)


func _is_offered(node_id: int) -> bool:
	for option: Dictionary in _flow.selectable_encounters():
		if int(option.id) == node_id:
			return true
	return false


# Enters `node_id` and returns its combat screen, or null if it did not load.
func _enter_combat(node_id: int) -> Node:
	var selected: Dictionary = _flow.select_encounter(node_id)
	if not selected.get("ok", false):
		push_error("could not enter combat %d: %s" % [node_id, selected])
		return null
	await process_frame
	await process_frame

	var combat := _screen()
	if combat == null or combat.name != "CombatScreen":
		return null
	return combat


func _press_end_turn(combat: Node) -> void:
	combat.get_node("%EndTurnButton").pressed.emit()


# Plays `card_name` the way the player does: select it, then press Play.
# False if it is not in the hand.
func _play(combat: Node, card_name: String) -> bool:
	var index: int = combat.draw_phase.hand_names().find(card_name)
	if index < 0:
		return false
	combat.card_buttons[index].pressed.emit()
	combat.get_node("%PlayButton").pressed.emit()
	return true


# Fixture only: reuse the real draw API rather than add a debug gameplay API.
func _draw_whole_deck(combat: Node) -> void:
	var draw: Node = combat.draw_phase
	draw.hand_size = draw.hand_names().size() + draw.draw_pile_count() + draw.discard_pile_count()
	combat.selected_index = -1
	combat._draw_hand()


# Read with [] rather than get() with a default: _run checks the keys exist
# first, so a renamed field fails there instead of both sides of an
# "unchanged" check falling back to the same default.
func _credits_earned() -> int:
	return int(_flow.run_summary()["credits_earned"])


func _combats_won() -> int:
	return int(_flow.run_summary()["combats_won"])


func _run() -> void:
	await process_frame

	_flow = root.get_node_or_null("/root/FlowCoordinator")
	_noise_meter = root.get_node_or_null("/root/NoiseMeterGlobal")
	_player = root.get_node_or_null("/root/PlayerStateGlobal")
	_check(_flow != null, "FlowCoordinator autoload is present")
	_check(_noise_meter != null, "NoiseMeterGlobal autoload is present")
	_check(_player != null, "PlayerStateGlobal autoload is present")
	if _flow == null or _noise_meter == null or _player == null:
		_finish()
		return

	var main: Node = load("res://scenes/main.tscn").instantiate()
	root.add_child(main)
	await process_frame
	await process_frame

	var summary: Dictionary = _flow.run_summary()
	_check(
		summary.has("credits_earned") and summary.has("combats_won"),
		"the run summary reports credits earned and combats won"
	)
	if not (summary.has("credits_earned") and summary.has("combats_won")):
		_finish()
		return

	# --- find a standard combat in the first zone ---------------------------
	var offered_combat: Dictionary = CombatFixture.find_combat(_flow)
	_check(
		not offered_combat.is_empty(),
		"a contract offers a standard combat within %d rolls" % CombatFixture.MAX_CONTRACTS
	)
	if offered_combat.is_empty():
		_finish()
		return
	var combat_id: int = offered_combat.id

	# --- scenario: no credits when caught -----------------------------------
	var combat: Node = await _enter_combat(combat_id)
	_check(combat != null, "the combat node loads the combat screen")
	if combat == null:
		_finish()
		return

	# The shared fixture is only for entering from the hub. Misused during an
	# encounter it has to fail rather than swap the run out from under the
	# screen that is playing it.
	var misused: Dictionary = CombatFixture.enter(_flow)
	_check(not misused.get("ok", false), "the fixture refuses to enter during an encounter")
	_check(
		_screen() == combat and _flow.run_snapshot().phase == "encounter_active",
		"and leaves the combat in progress untouched"
	)

	var credits_before: int = _player.money()
	var earned_before := _credits_earned()
	var wins_before := _combats_won()

	# One point short of the cap, so the security system's next action fills
	# the meter and the player is caught mid-fight with the construct standing.
	_noise_meter.add_noise(_noise_meter.max_noise - 1 - _noise_meter.noise)
	_press_end_turn(combat)
	await process_frame
	await process_frame

	var caught := _screen()
	_check(_flow.run_snapshot().phase == "caught", "a full meter during the fight is caught")
	_check(caught != null and caught.name == "CaughtScreen", "the caught screen takes over")
	if caught == null or caught.name != "CaughtScreen":
		_finish()
		return

	var fine: int = caught.fine_amount
	_check(
		_player.money() == credits_before - fine,
		(
			"only the fine comes off and no payout goes on (%d -> %d, got %d)"
			% [credits_before, credits_before - fine, _player.money()]
		)
	)
	_check(_credits_earned() == earned_before, "being caught adds nothing to credits earned")
	_check(_combats_won() == wins_before, "being caught is not counted as a win")

	caught.get_node("%ContinueButton").pressed.emit()
	await process_frame
	await process_frame

	var hub := _screen()
	_check(hub != null and hub.name == "ContractHub", "continuing from caught returns to the hub")
	_check(
		_player.money() == credits_before - fine,
		"leaving the caught screen pays nothing late either (got %d)" % _player.money()
	)
	_check(_is_offered(combat_id), "the uncleared combat is still on offer to retry")

	# --- scenario: earn credits for a win -----------------------------------
	# The same fight, retried and won, so the caught result above is not just a
	# combat that never pays.
	combat = await _enter_combat(combat_id)
	_check(combat != null, "the retried combat loads the combat screen")
	if combat == null:
		_finish()
		return

	credits_before = _player.money()
	earned_before = _credits_earned()
	wins_before = _combats_won()

	# Won the way a player wins: a card takes the last of the construct's
	# integrity, so the payout comes through the card-play win path rather
	# than the end-of-turn check for a corruption kill.
	var sentry: Node = combat.sentry
	sentry.take_damage(sentry.health() - 1)
	_check(not sentry.is_defeated(), "the construct is left standing on 1 integrity")
	_draw_whole_deck(combat)
	_check(_play(combat, "Strike"), "Strike is in the full-deck hand")
	await process_frame
	await process_frame

	_check(
		_flow.run_snapshot().phase == "hub",
		"a Strike that defeats the construct completes the encounter"
	)
	_check(
		_player.money() == credits_before + STANDARD_COMBAT_CREDITS,
		(
			"winning pays %d credits (%d -> %d, got %d)"
			% [
				STANDARD_COMBAT_CREDITS,
				credits_before,
				credits_before + STANDARD_COMBAT_CREDITS,
				_player.money(),
			]
		)
	)
	_check(
		_credits_earned() == earned_before + STANDARD_COMBAT_CREDITS,
		"the payout counts as credits earned in the run's stats"
	)
	_check(_combats_won() == wins_before + 1, "the win is counted")

	var reward := _screen()
	_check(reward != null and reward.name == "CardReward", "winning offers a card reward")
	if reward != null and reward.name == "CardReward":
		_check(
			reward.credits_label.text.contains("+%d credits" % STANDARD_COMBAT_CREDITS),
			"the reward screen shows the payout (got '%s')" % reward.credits_label.text
		)

	await _check_fixture_retries_a_failed_first_encounter()
	_finish()


# The shared fixture, not the payout. Being caught in a contract's first
# encounter completes nothing, so the contract's progress reads the same as an
# untouched one. The fixture has to treat it as started all the same: entering
# again is the caught encounter's retry, never a reason to roll a new contract.
# An event is used because it is the case that went wrong: the fixture rolls
# for a real combat on an untouched contract, and a caught event leaves no
# combat on offer.
func _check_fixture_retries_a_failed_first_encounter() -> void:
	var left_reward: Dictionary = _flow.finish_reward()
	_check(left_reward.get("ok", false), "the reward screen returns to the hub")
	var event := _roll_contract_offering("event")
	_check(
		not event.is_empty(),
		"a contract offers an opening event within %d rolls" % CombatFixture.MAX_CONTRACTS
	)
	if event.is_empty():
		return

	# Something for a rolled run to lose: a new run starts its stats at zero.
	_flow.change_credits(7)
	var map_before := _map_layout()

	var selected: Dictionary = _flow.select_encounter(event.id)
	var caught: Dictionary = _flow.report_caught()
	var finished: Dictionary = _flow.finish_caught()
	_check(
		selected.get("ok", false) and caught.get("ok", false) and finished.get("ok", false),
		"the player is caught in the opening event and continues to the hub"
	)
	_check(
		int(_flow.contract_progress()["completed"]) == 0 and _flow.contract_started(),
		"the contract has completed nothing but counts as started"
	)

	var retried: Dictionary = CombatFixture.enter(_flow)
	await process_frame
	_check(retried.get("ok", false), "the fixture enters again after being caught")
	var active = _flow.run_snapshot().get("active_encounter")
	_check(
		typeof(active) == TYPE_DICTIONARY and active.id == str(event.id) and active.type == "event",
		"what it enters is the caught event's retry (got %s)" % [active]
	)
	_check(
		_map_layout() == map_before and _credits_earned() == 7,
		"the run is kept rather than replaced with a fresh contract"
	)
	var screen := _screen()
	_check(
		screen != null and screen.name == "CombatScreen",
		"with combat content mounted on the event node"
	)


# The first node of `encounter_type` the hub offers, rolling fresh contracts
# until one does. Empty if none turned up.
func _roll_contract_offering(encounter_type: String) -> Dictionary:
	for attempt in CombatFixture.MAX_CONTRACTS:
		if not _flow.contract_started():
			for option: Dictionary in _flow.selectable_encounters():
				if option.type == encounter_type:
					return option
		var rolled: Dictionary = CombatFixture.roll_new_contract(_flow)
		if not rolled.get("ok", false):
			return {}
	return {}


# Every node's id and type, which a freshly rolled contract would not share.
func _map_layout() -> Array:
	var layout := []
	for node: Dictionary in _flow.map_progress():
		layout.append([int(node.id), node.type])
	return layout
