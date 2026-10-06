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
# Run: godot --headless --path godot -s res://tests/combat_credits_test.gd
extends SceneTree

# A first-zone standard combat pays 25 credits. See the payout table under
# "Elite encounters" in docs/game-flow.md.
const STANDARD_COMBAT_CREDITS := 25

# Generated contracts do not guarantee a combat among the first options, so
# the test rolls fresh ones until one is offered. A run of failures this long
# would mean combats had stopped being generated at all.
const MAX_CONTRACTS := 60

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


# Fixture only: replaces the coordinator's run with a freshly generated
# contract, the same thing a new launch does. The node keeps its name, so the
# SentryNode on a combat screen still finds it at /root/FlowCoordinator/RunState.
func _roll_new_contract() -> void:
	var old: Node = _flow.get_node("RunState")
	_flow.remove_child(old)
	old.free()
	var fresh: Node = ClassDB.instantiate("RunStateNode")
	fresh.name = "RunState"
	_flow.add_child(fresh)
	_flow._run_state = fresh


# The id of a standard combat the hub is offering right now, or -1 if none.
func _offered_combat_id() -> int:
	for option: Dictionary in _flow.selectable_encounters():
		if str(option.get("type", "")) == "combat":
			return int(option.id)
	return -1


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
	combat.get_node("VBoxContainer/Buttons/EndTurnButton").pressed.emit()


func _credits_earned() -> int:
	return int(_flow.run_summary().get("credits_earned", -1))


func _combats_won() -> int:
	return int(_flow.run_summary().get("combats_won", -1))


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

	# --- find a standard combat in the first zone ---------------------------
	var combat_id := _offered_combat_id()
	for attempt in MAX_CONTRACTS:
		if combat_id >= 0:
			break
		_roll_new_contract()
		combat_id = _offered_combat_id()
	_check(combat_id >= 0, "a contract offers a standard combat within %d rolls" % MAX_CONTRACTS)
	if combat_id < 0:
		_finish()
		return

	# --- scenario: no credits when caught -----------------------------------
	var combat: Node = await _enter_combat(combat_id)
	_check(combat != null, "the combat node loads the combat screen")
	if combat == null:
		_finish()
		return

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
	_check(_flow.pending_reward_credits() == 0, "no payout is waiting for a reward screen")

	caught.get_node("VBoxContainer/ContinueButton").pressed.emit()
	await process_frame
	await process_frame

	_check(_screen().name == "ContractHub", "continuing from caught returns to the hub")
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

	var sentry: Node = combat.sentry
	sentry.take_damage(sentry.health())
	_press_end_turn(combat)
	await process_frame
	await process_frame

	_check(_flow.run_snapshot().phase == "hub", "reducing integrity to 0 completes the encounter")
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
			reward.status_label.text.contains("+%d credits" % STANDARD_COMBAT_CREDITS),
			"the reward screen shows the payout (got '%s')" % reward.status_label.text
		)

	_finish()
