# Real scene/extension coverage for the zone-boss story. The Rust tests own the
# rules; this drives the parts they cannot see: that the construct a boss node
# actually loads is the boss one, that its resistance reaches the shared meter,
# that its lockdown takes energy off the player's next turn, and that beating it
# opens the next zone on the map the hub draws from.
#
# Run: godot --headless --path godot -s res://tests/boss_encounter_test.gd
extends SceneTree

# The zone-1 boss, and the loudest thing standard security does in that zone.
const BOSS_NAME := "ICEBREAKER"
const BOSS_RESISTANCE := 30
const LOUDEST_STANDARD_NOISE := 3

var _flow: Node
var _combat: Node
var _noise_meter: Node
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
		print("boss encounter test: PASSED")
		quit(0)
	else:
		print("boss encounter test: FAILED (%d)" % _failures.size())
		for failure in _failures:
			print("  - %s" % failure)
		quit(1)


# Walks the generated run up to its first boss without playing the encounters
# on the way out: this test is about the boss, not about combat in general.
# Returns the boss node's id, or -1 if the walk never reached one.
func _advance_to_first_boss() -> int:
	# Screen replacement is deferred, so let each transition settle before the
	# next node is chosen, the same way the smoke test does.
	for step in 20:
		var options: Array = _flow.selectable_encounters()
		if options.is_empty():
			return -1

		var chosen: Dictionary = options[0]
		for candidate: Dictionary in options:
			if str(candidate.get("type", "")) == "boss":
				chosen = candidate
				break

		var is_boss := str(chosen.get("type", "")) == "boss"
		var selected: Dictionary = _flow.select_encounter(chosen.id)
		if not selected.get("ok", false):
			push_error("could not enter %s: %s" % [chosen, selected])
			return -1
		if is_boss:
			return int(chosen.id)

		var completed: Dictionary = _flow.complete_active_encounter()
		if not completed.get("ok", false):
			push_error("could not complete %s: %s" % [chosen, completed])
			return -1
		if str(chosen.get("type", "")) in ["combat", "elite"]:
			var skipped: Dictionary = _flow.finish_reward()
			if not skipped.get("ok", false):
				push_error("could not leave the card reward: %s" % skipped)
				return -1
		await process_frame
		await process_frame
	return -1


func _nodes_in_zone(zone: int) -> Array:
	var found: Array = []
	for node: Dictionary in _flow.map_progress():
		if int(node.get("zone", -1)) == zone:
			found.append(node)
	return found


func _press_end_turn() -> void:
	_combat.get_node("VBoxContainer/Buttons/EndTurnButton").pressed.emit()


# Fixture only: reuse the real draw API rather than add a debug gameplay API.
func _draw_whole_deck() -> void:
	var draw: Node = _combat.draw_phase
	draw.hand_size = (draw.hand_names().size() + draw.draw_pile_count() + draw.discard_pile_count())
	_combat.selected_index = -1
	_combat._draw_hand()


func _run() -> void:
	await process_frame

	_flow = root.get_node_or_null("/root/FlowCoordinator")
	_noise_meter = root.get_node_or_null("/root/NoiseMeterGlobal")
	_check(_flow != null, "FlowCoordinator autoload is present")
	_check(_noise_meter != null, "NoiseMeterGlobal autoload is present")
	if _flow == null or _noise_meter == null:
		_finish()
		return

	var main: Node = load("res://scenes/main.tscn").instantiate()
	root.add_child(main)
	await process_frame
	await process_frame

	var booted := _screen()
	_check(booted != null and booted.name == "ContractHub", "the run opens on the contract hub")

	# --- the contract is laid out in zones, each closed off by one boss -----
	var zones: Dictionary = _flow.zone_progress()
	_check(
		int(zones.get("total_zones", 0)) > 1,
		"a generated contract is split into zones (got %d)" % int(zones.get("total_zones", 0))
	)
	_check(int(zones.get("unlocked_zones", -1)) == 0, "a fresh run has only its first zone open")

	var key: Dictionary = _flow.map_key()
	var key_types: Array = []
	for entry: Dictionary in key.get("encounter_types", []):
		key_types.append(str(entry.get("type", "")))
	var key_statuses: Array = []
	for entry: Dictionary in key.get("statuses", []):
		key_statuses.append(str(entry.get("status", "")))
	_check("boss" in key_types, "the map key explains the boss marker")
	_check("sealed" in key_statuses, "the map key explains the sealed marker")

	# --- the next zone is sealed while its boss is still standing -----------
	var boss_id := await _advance_to_first_boss()
	_check(boss_id >= 0, "the run reaches a boss at the end of its first zone")
	if boss_id < 0:
		_finish()
		return

	var sealed_before: Array = _nodes_in_zone(1)
	var all_sealed := not sealed_before.is_empty()
	for node: Dictionary in sealed_before:
		if str(node.get("status", "")) != "sealed":
			all_sealed = false
	_check(all_sealed, "every node in the next zone reads as sealed beforehand")

	# --- the construct behind a boss node is the boss one -------------------
	_combat = _screen()
	_check(_combat != null and _combat.has_node("Sentry"), "a boss node loads the combat screen")
	if _combat == null or not _combat.has_node("Sentry"):
		_finish()
		return

	_check(
		_combat.sentry.construct_name() == BOSS_NAME,
		"the boss encounter fields %s (got '%s')" % [BOSS_NAME, _combat.sentry.construct_name()]
	)
	_check(
		_combat.sentry.queued_action_noise() > LOUDEST_STANDARD_NOISE,
		(
			"the boss opens louder than standard security in this zone (%d > %d)"
			% [_combat.sentry.queued_action_noise(), LOUDEST_STANDARD_NOISE]
		)
	)
	_check(_combat.sentry.has_ability(), "the boss carries an ability")

	# Resistance reaches the one shared meter, rather than a second copy of it.
	_check(
		(
			_combat.sentry.detection_resistance() == BOSS_RESISTANCE
			and _noise_meter.get_resistance_percent() == BOSS_RESISTANCE
		),
		(
			"the boss's detection resistance is set on the shared meter (got %d)"
			% _noise_meter.get_resistance_percent()
		)
	)
	_check(
		_combat.noise_label.text.contains("recovery -%d%%" % BOSS_RESISTANCE),
		"the screen says recovery is being resisted (got '%s')" % _combat.noise_label.text
	)

	# --- resistance blunts a recovery card ----------------------------------
	_combat.sentry.add_noise(50 - _combat.sentry.noise())
	_draw_whole_deck()
	var vpn_index: int = _combat.draw_phase.hand_names().find("VPN")
	_check(vpn_index >= 0, "VPN is in the full-deck hand")
	if vpn_index >= 0:
		_combat.card_buttons[vpn_index].pressed.emit()
		_combat.get_node("VBoxContainer/Buttons/PlayButton").pressed.emit()
		# VPN authors -10; 30% resistance lands it as -7.
		_check(
			_combat.status_label.text == "VPN played. Noise change: -7.",
			"a resisted VPN recovers less than it authors (got '%s')" % _combat.status_label.text
		)
		_check(_combat.sentry.noise() == 43, "and the meter moved by that much, not by 10")

	# --- the lockdown takes energy off the turn it opens --------------------
	var max_energy: int = _combat.draw_phase.max_energy
	var drained_turn_seen := false
	for turn in 4:
		var announced_drain: String = _combat.sentry.queued_action_ability()
		_press_end_turn()
		if announced_drain == "":
			continue

		drained_turn_seen = true
		_check(
			announced_drain == "-1 energy",
			"the lockdown is announced before it lands (got '%s')" % announced_drain
		)
		_check(
			_combat.draw_phase.energy == max_energy - 1,
			(
				"the lockdown leaves the new turn short of energy (%d of %d)"
				% [_combat.draw_phase.energy, max_energy]
			)
		)
		_check(
			_combat.energy_label.text == "Energy: %d / %d" % [max_energy - 1, max_energy],
			"and the screen shows the reduced pool (got '%s')" % _combat.energy_label.text
		)
		_check(
			_combat.status_label.text.contains("Lockdown cost you 1 energy"),
			"and says why (got '%s')" % _combat.status_label.text
		)
		break
	_check(drained_turn_seen, "the boss uses its lockdown within its first few turns")
	var costs: PackedInt32Array = _combat.draw_phase.hand_costs()
	for index in costs.size():
		_check(
			_combat.card_buttons[index].disabled == (costs[index] > max_energy - 1),
			"lockdown updates card %d's affordability" % index
		)
	_press_end_turn()
	_check(
		_combat.draw_phase.energy == max_energy,
		"lockdown lasts one turn, not the rest of the encounter"
	)

	# --- losing the boss does not unlock the zone ---------------------------
	_noise_meter.add_noise(_noise_meter.max_noise - 1 - _noise_meter.noise)
	_press_end_turn()
	_check(_flow.run_snapshot().phase == "caught", "a boss turn at the cap enters caught")
	_check(_screen().name == "CaughtScreen", "losing the boss shows the caught screen")
	_check(_noise_meter.get_resistance_percent() == 0, "caught clears boss resistance")
	var recovered: Dictionary = _flow.finish_caught()
	_check(recovered.get("ok", false), "caught returns to the hub")
	_check(_noise_meter.noise == 0, "caught recovery fully clears noise after a boss")
	_check(_flow.zone_progress().unlocked_zones == 0, "losing leaves the next zone sealed")
	var retry_options: Array = _flow.selectable_encounters()
	_check(
		retry_options.size() == 1 and int(retry_options[0].id) == boss_id,
		"the failed boss is the only retry offered"
	)
	var retried: Dictionary = _flow.select_encounter(boss_id)
	_check(retried.get("ok", false), "the boss can be retried")
	_check(
		_noise_meter.get_resistance_percent() == BOSS_RESISTANCE, "retry restores boss resistance"
	)

	# --- beating the boss unlocks the next zone -----------------------------
	var completed: Dictionary = _flow.complete_active_encounter()
	_check(completed.get("ok", false), "the boss encounter can be completed")
	var reward := _screen()
	_check(reward != null and reward.name == "CardReward", "beating a boss offers a card reward")
	if reward == null or reward.name != "CardReward":
		_finish()
		return
	_check(reward.offered.size() > 0, "the boss reward offers cards to choose from")
	_check(
		_noise_meter.get_resistance_percent() == 0,
		"boss resistance is cleared on the reward screen"
	)
	var deck_size: int = _flow.run_deck_size()
	reward._on_skip_pressed()
	_check(_screen().name == "ContractHub", "skipping the boss reward returns to the hub")
	_check(_flow.run_deck_size() == deck_size, "skipping a boss reward leaves the deck unchanged")

	var after: Dictionary = _flow.zone_progress()
	_check(
		int(after.get("unlocked_zones", -1)) == 1,
		"beating the boss unlocks the next zone (got %d)" % int(after.get("unlocked_zones", -1))
	)

	var newly_open: Array = []
	for node: Dictionary in _nodes_in_zone(1):
		if str(node.get("status", "")) == "available":
			newly_open.append(node)
	_check(
		not newly_open.is_empty(),
		"nodes from the next zone are now available to enter (%d of them)" % newly_open.size()
	)
	_check(
		_flow.selectable_encounters().size() == newly_open.size(),
		"and they are exactly what the hub offers next"
	)

	if newly_open.is_empty():
		_finish()
		return

	# Walking into one of them is what actually moves the player on a zone.
	var stepped: Dictionary = _flow.select_encounter(newly_open[0].id)
	_check(stepped.get("ok", false), "a newly opened node can be entered")
	if stepped.get("ok", false):
		_flow.complete_active_encounter()
		_check(
			int(_flow.zone_progress().get("current_zone", -1)) == 1,
			"and doing so puts the player in the next zone"
		)

	# Resistance belongs to the encounter, not the run: it must not follow the
	# player out of the boss fight.
	_check(
		_noise_meter.get_resistance_percent() == 0,
		(
			"the boss's resistance does not outlive its encounter (got %d)"
			% _noise_meter.get_resistance_percent()
		)
	)

	_finish()
