# Real scene/extension coverage for the elite story (Trello card E1). The Rust
# tests own the rules; this drives the parts they cannot see: that an elite
# node actually loads combat against the elite construct, that its Reinforce
# lands on the screen the player is looking at, and that beating it pays more
# credits than a standard combat and offers a rare card.
#
# Run: godot --headless --path godot -s res://tests/elite_encounter_test.gd
extends SceneTree

# The zone-1 elite, against the standard WARDEN-7 it has to out-last.
const ELITE_NAME := "BASTION"
const ELITE_HEALTH := 60
const STANDARD_HEALTH := 40
const ELITE_RESISTANCE := 15
const REINFORCE := 8

# Generated contracts do not guarantee an elite, so the test rolls fresh ones
# until a first-zone route offers one. One in ten nodes is an elite, so this
# many attempts failing would mean elites had stopped being generated at all.
const MAX_CONTRACTS := 60

var _flow: Node
var _combat: Node
var _noise_meter: Node
var _player: Node
var _failures: Array[String] = []
# Credits a standard combat paid out, measured on the way to the elite.
var _standard_combat_credits := -1


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
		print("elite encounter test: PASSED")
		quit(0)
	else:
		print("elite encounter test: FAILED (%d)" % _failures.size())
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


# Walks the first zone of the current contract, stepping into an elite the
# moment one is offered. Encounters on the way are completed without being
# played; a standard combat cleared on the way records what it paid.
# Returns the elite's id once it is entered, or -1 if the zone had none.
func _walk_to_first_zone_elite() -> int:
	for step in 10:
		var options: Array = _flow.selectable_encounters()
		if options.is_empty():
			return -1

		var chosen: Dictionary = {}
		for candidate: Dictionary in options:
			if str(candidate.get("type", "")) == "elite":
				chosen = candidate
				break
		if chosen.is_empty():
			# Bosses close the zone, so an elite past one would not be zone 1.
			for candidate: Dictionary in options:
				if str(candidate.get("type", "")) != "boss":
					chosen = candidate
					break
		if chosen.is_empty():
			return -1

		var kind := str(chosen.get("type", ""))
		var selected: Dictionary = _flow.select_encounter(chosen.id)
		if not selected.get("ok", false):
			push_error("could not enter %s: %s" % [chosen, selected])
			return -1
		if kind == "elite":
			return int(chosen.id)

		var credits_before: int = _player.money()
		var completed: Dictionary = _flow.complete_active_encounter()
		if not completed.get("ok", false):
			push_error("could not complete %s: %s" % [chosen, completed])
			return -1
		if kind == "combat":
			if _standard_combat_credits < 0:
				_standard_combat_credits = _player.money() - credits_before
			_flow.finish_reward()
		await process_frame
		await process_frame
	return -1


func _press_end_turn() -> void:
	_combat.get_node("VBoxContainer/Buttons/EndTurnButton").pressed.emit()


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

	# --- find an elite in the first zone ------------------------------------
	var elite_id := -1
	for attempt in MAX_CONTRACTS:
		if attempt > 0:
			_roll_new_contract()
		elite_id = await _walk_to_first_zone_elite()
		if elite_id >= 0 and _standard_combat_credits >= 0:
			break
		if elite_id >= 0:
			# Found the elite before any standard combat to measure against.
			# Back out to a fresh contract rather than playing this one.
			elite_id = -1
	_check(elite_id >= 0, "a first-zone route offers an elite within %d contracts" % MAX_CONTRACTS)
	_check(_standard_combat_credits > 0, "a standard combat pays credits to compare against")
	if elite_id < 0:
		_finish()
		return
	_check(
		int(_flow.zone_progress().get("unlocked_zones", -1)) == 0,
		"the elite is in the first zone, before its boss"
	)

	# --- scenario 1: the elite node loads a hardened construct --------------
	_combat = _screen()
	_check(
		_combat != null and _combat.has_node("Sentry"),
		"an elite node loads the combat screen, not the placeholder"
	)
	if _combat == null or not _combat.has_node("Sentry"):
		_finish()
		return

	var sentry: Node = _combat.sentry
	_check(
		sentry.construct_name() == ELITE_NAME,
		"the elite fields %s (got '%s')" % [ELITE_NAME, sentry.construct_name()]
	)
	_check(
		sentry.max_health() == ELITE_HEALTH and sentry.max_health() > STANDARD_HEALTH,
		(
			"its integrity is higher than a standard zone-1 construct's (%d > %d)"
			% [sentry.max_health(), STANDARD_HEALTH]
		)
	)
	_check(sentry.has_ability(), "it carries an ability standard constructs do not")
	_check(
		_noise_meter.get_resistance_percent() == ELITE_RESISTANCE,
		"its detection resistance is on the shared meter (got %d)" % _noise_meter.get_resistance_percent()
	)
	_check(
		_combat.health_label.text.contains("%s: %d / %d" % [ELITE_NAME, ELITE_HEALTH, ELITE_HEALTH]),
		"the screen shows the elite's integrity (got '%s')" % _combat.health_label.text
	)

	# --- Reinforce is announced, then restores integrity --------------------
	sentry.take_damage(20)
	var reinforced := false
	for turn in 5:
		var announced: String = sentry.queued_action_ability()
		var health_before: int = sentry.health()
		_press_end_turn()
		if announced == "":
			continue

		reinforced = true
		_check(
			announced == "+%d integrity" % REINFORCE,
			"Reinforce is announced with the intent (got '%s')" % announced
		)
		_check(
			sentry.health() == health_before + REINFORCE,
			"and restores that much integrity (%d -> %d)" % [health_before, sentry.health()]
		)
		_check(
			_combat.status_label.text.contains("Reinforce restored %d integrity" % REINFORCE),
			"and the screen says so (got '%s')" % _combat.status_label.text
		)
		break
	_check(reinforced, "the elite reinforces within its first few turns")
	_check(_flow.run_snapshot().phase == "encounter_active", "the elite fight is still on")

	# --- scenario 2: beating it pays better ---------------------------------
	var credits_before: int = _player.money()
	sentry.take_damage(sentry.health())
	_press_end_turn()
	_check(_flow.run_snapshot().phase == "hub", "defeating the elite completes the encounter")

	var earned: int = _player.money() - credits_before
	_check(
		earned > _standard_combat_credits,
		"the elite pays more than a standard combat (%d > %d)" % [earned, _standard_combat_credits]
	)

	var reward := _screen()
	_check(reward != null and reward.name == "CardReward", "beating the elite offers a card reward")
	if reward == null or reward.name != "CardReward":
		_finish()
		return

	var rare_offered := false
	for card: Dictionary in reward.offered:
		if str(card.get("rarity", "")) == "Rare":
			rare_offered = true
	_check(reward.offered.size() == 3, "the elite reward offers a full hand of choices")
	_check(rare_offered, "and at least one of them is a rare card")
	_check(
		reward.status_label.text.contains("+%d credits" % earned),
		"the reward screen shows the payout (got '%s')" % reward.status_label.text
	)
	_check(_noise_meter.get_resistance_percent() == 0, "elite resistance does not outlive the fight")

	reward._on_skip_pressed()
	_check(_screen().name == "ContractHub", "leaving the reward returns to the hub")
	_check(_flow.pending_reward_credits() == 0, "the payout is not offered twice")

	_finish()
