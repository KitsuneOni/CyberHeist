# Real scene/extension coverage for story #4. No shuffled-hand assumptions:
# fixtures draw the complete starter deck, then select cards by name.
# Run: godot --headless --path godot -s res://tests/lower_detection_test.gd
extends SceneTree

var _flow: Node
var _combat: Node
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
	return root.get_node("Main/ScreenHost").get_child(0)


func _finish() -> void:
	if _failures.is_empty():
		print("lower detection test: PASSED")
		quit(0)
	else:
		print("lower detection test: FAILED (%d)" % _failures.size())
		for failure in _failures:
			print("  - %s" % failure)
		quit(1)


# Fixture only: use the existing draw API rather than add a debug gameplay API.
func _arrange_hand(noise: int) -> void:
	var draw: Node = _combat.draw_phase
	draw.hand_size = (
		draw.hand_names().size() + draw.draw_pile_count() + draw.discard_pile_count()
	)
	_combat.selected_index = -1
	_combat._draw_hand()
	_combat.sentry.add_noise(noise - _combat.sentry.noise())
	_combat._refresh_status_labels()


func _select(name: String) -> bool:
	var index: int = _combat.draw_phase.hand_names().find(name)
	_check(index >= 0, "%s is in the deterministic full-deck hand" % name)
	if index < 0:
		return false
	_combat.card_buttons[index].pressed.emit()
	return true


func _press_play() -> void:
	_combat.get_node("VBoxContainer/Buttons/PlayButton").pressed.emit()


func _state() -> Dictionary:
	var draw: Node = _combat.draw_phase
	return {
		"hand": draw.hand_names(),
		"energy": draw.energy,
		"draw": draw.draw_pile_count(),
		"discard": draw.discard_pile_count(),
		"plays": draw.play_count,
		"turn": draw.turn_number,
		"noise": _combat.sentry.noise(),
		"intent": _combat.sentry.queued_action_name(),
	}


func _run() -> void:
	await process_frame
	_flow = root.get_node("FlowCoordinator")
	var main: Node = load("res://scenes/main.tscn").instantiate()
	root.add_child(main)
	await process_frame

	var combat_id := -1
	for encounter in _flow.selectable_encounters():
		if encounter.get("type", "") == "combat":
			combat_id = encounter.id
			break
	_check(combat_id >= 0, "a combat encounter is selectable")
	if combat_id < 0:
		_finish()
		return
	var selected: Dictionary = _flow.select_encounter(combat_id)
	_check(selected.get("ok", false), "combat loads through FlowCoordinator")
	if not selected.get("ok", false):
		_finish()
		return
	_combat = _screen()
	await process_frame

	_check(
		not _combat.has_node("VBoxContainer/Buttons/DrawButton"),
		"combat offers no free redraw/energy refresh button"
	)
	_arrange_hand(9)
	if not _select("VPN"):
		_finish()
		return
	_check(_combat.noise_label.text == "Noise: 9 / 10", "near-cap noise is displayed")
	_check(
		_combat.detail_stats.text == "Costs 2 energy · Reduces noise by 10",
		"VPN displays its energy cost and recovery amount"
	)
	_check(
		_combat.detail_description.text == "Lower noise by 10 (minimum 0). Your turn continues."
		and _combat.detail_keywords.text.is_empty(),
		"VPN does not promise unimplemented keywords or an automatic end turn"
	)
	var before := _state()
	_press_play()
	_check(_combat.noise_label.text == "Noise: 0 / 10", "VPN immediately displays lower noise")
	_check(_combat.draw_phase.energy == 1, "VPN spends exactly 2 energy")
	_check(_combat.draw_phase.play_count == before.plays + 1, "one successful play is counted")
	_check(_combat.draw_phase.discard_pile_count() == 1, "VPN is discarded exactly once")
	_check(not _combat.draw_phase.hand_names().has("VPN"), "VPN leaves the hand")
	_check(_combat.draw_phase.turn_number == before.turn, "recovery does not end the turn")
	_check(_combat.sentry.queued_action_name() == before.intent, "recovery preserves queued intent")
	_check(_flow.run_snapshot().phase == "encounter_active", "recovery avoids caught")
	_check(_screen() == _combat, "the combat screen remains active")
	_check(
		_combat.status_label.text == "VPN played. Noise change: -9.",
		"feedback shows actual clamped change"
	)
	var after := _state()
	_press_play()
	_check(_state() == after, "a repeated Play press with no selection cannot apply recovery again")

	# Normal turn ordering still applies after recovery. Use the normal hand size.
	_combat.draw_phase.hand_size = 3
	_combat.get_node("VBoxContainer/Buttons/EndTurnButton").pressed.emit()
	_check(
		_combat.noise_label.text == "Noise: 1 / 10",
		"the next sentry turn acts on the recovered meter"
	)
	_check(_combat.intent_label.text.contains("Trace Sweep"), "the next intent advances once")
	_check(_combat.status_label.text.contains("Packet Sniff"), "the originally queued action ran")
	_check(_combat.draw_phase.energy == 3, "ending the turn refreshes energy")
	_check(_combat.draw_phase.hand_names().size() == 3, "ending the turn deals a normal hand")
	_check(
		_flow.run_snapshot().phase == "encounter_active",
		"the enemy turn does not catch the recovered player"
	)

	_arrange_hand(0)
	if not _select("VPN"):
		_finish()
		return
	_press_play()
	_check(
		_combat.noise_label.text == "Noise: 0 / 10",
		"recovery at zero never displays negative noise"
	)
	_check(_combat.draw_phase.energy == 1, "recovery at zero still costs energy")
	_check(_combat.draw_phase.discard_pile_count() == 1, "recovery at zero still discards the card")

	_arrange_hand(9)
	_combat.draw_phase.energy = 1
	if not _select("VPN"):
		_finish()
		return
	before = _state()
	_press_play()
	_check(_state() == before, "an unaffordable UI play changes no gameplay state")
	_check(
		_combat.noise_label.text == "Noise: 9 / 10",
		"rejected recovery leaves displayed noise alone"
	)
	_check(_combat.status_label.text.contains("Not enough energy"), "rejected play explains the cost")

	for index in [-1, 1000]:
		var invalid: Dictionary = _combat.draw_phase.play_card(index, _combat.sentry)
		_check(
			not invalid.ok and invalid.error == "invalid_index",
			"invalid index %d is rejected" % index
		)
		_check(_state() == before, "invalid index %d changes no state" % index)

	# The same signed path must apply ordinary positive/zero noise exactly once.
	_arrange_hand(1)
	if not _select("Trojan"):
		_finish()
		return
	_press_play()
	_check(_combat.noise_label.text == "Noise: 6 / 10", "positive noise is applied once, not twice")
	_check(_combat.draw_phase.energy == 2, "positive-noise play pays its cost")
	if not _select("Strike"):
		_finish()
		return
	_press_play()
	_check(_combat.noise_label.text == "Noise: 6 / 10", "zero-noise play leaves the meter alone")

	_arrange_hand(9)
	if not _select("Trojan"):
		_finish()
		return
	var credits_before: int = root.get_node("PlayerStateGlobal").money()
	_press_play()
	_check(_combat.noise_label.text == "Noise: 10 / 10", "loud card noise clamps to the cap")
	_check(_flow.run_snapshot().phase == "caught", "positive card noise immediately enters caught")
	_check(_screen().name == "CaughtScreen", "card detection uses the existing caught screen")
	_check(_combat.draw_phase.energy == 2, "the fatal card still pays its cost")
	_check(_combat.draw_phase.discard_pile_count() == 1, "the fatal card is discarded once")

	# The old screen lives until the deferred free. Neither its queued input nor
	# a direct adapter call may revive it after the meter has triggered detection.
	_combat.draw_phase.energy = 3
	before = _state()
	var vpn_index: int = _combat.draw_phase.hand_names().find("VPN")
	var detected: Dictionary = _combat.draw_phase.play_card(vpn_index, _combat.sentry)
	_check(
		not detected.ok and detected.error == "detected",
		"the model rejects recovery after detection"
	)
	_combat.selected_index = vpn_index
	_press_play()
	_combat.get_node("VBoxContainer/Buttons/EndTurnButton").pressed.emit()
	_combat.get_node("VBoxContainer/Buttons/CompleteEncounterButton").pressed.emit()
	_check(_state() == before, "queued old-screen input cannot change gameplay after caught")
	_check(_flow.run_snapshot().phase == "caught", "recovery cannot revive a failed encounter")
	_check(
		root.get_node("PlayerStateGlobal").money() == credits_before - 50,
		"card detection applies the existing penalty only once"
	)
	await process_frame
	_check(not is_instance_valid(_combat), "the old combat screen is freed")
	_finish()
