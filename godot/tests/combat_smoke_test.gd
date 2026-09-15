# Drives a real combat encounter end to end in a headless engine, which is the
# part CI could never check: the Rust tests exercise the rules, but nothing
# exercised the scene connections or the screen transition on detection.
#
# Run it with:
#
#     godot --headless --path godot -s res://tests/combat_smoke_test.gd
#
# Exits non-zero on the first failed expectation, so it works as a CI gate.
extends SceneTree

const EXPECTED_NOISE := [1, 3, 6, 7, 9, 10]
const EXPECTED_FINE := 50

var _flow: Node
var _player_state: Node
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
		print("combat smoke test: PASSED")
		quit(0)
	else:
		print("combat smoke test: FAILED (%d)" % _failures.size())
		for failure in _failures:
			print("  - %s" % failure)
		quit(1)


func _run() -> void:
	await process_frame

	_flow = root.get_node_or_null("/root/FlowCoordinator")
	_player_state = root.get_node_or_null("/root/PlayerStateGlobal")
	_check(_flow != null, "FlowCoordinator autoload is present")
	_check(_player_state != null, "PlayerStateGlobal autoload is present")
	if _flow == null or _player_state == null:
		_finish()
		return

	var main: Node = load("res://scenes/main.tscn").instantiate()
	root.add_child(main)
	await process_frame
	await process_frame

	var booted := _screen()
	_check(booted != null and booted.name == "ContractHub", "the run opens on the contract hub")

	# --- enter combat ---
	var combat_id := -1
	for encounter in _flow.selectable_encounters():
		if combat_id == -1 and str(encounter.get("type", "")) == "combat":
			combat_id = int(encounter.get("id", -1))
	_check(combat_id != -1, "a combat encounter is selectable from the hub")
	if combat_id == -1:
		_finish()
		return

	var selected: Dictionary = _flow.select_encounter(combat_id)
	_check(selected.get("ok", false), "selecting the combat encounter succeeds")
	await process_frame
	await process_frame

	var combat := _screen()
	_check(combat != null and combat.name == "CombatScreen", "the combat screen is showing")
	if combat == null:
		_finish()
		return

	var credits_before: int = _player_state.money()

	# --- the opening state, before a turn is taken ---
	_check(combat.noise_label.text == "Noise: 0 / 10", "the meter starts empty (got '%s')" % combat.noise_label.text)
	_check(
		combat.intent_label.text.contains("Packet Sniff"),
		"the first intent is shown (got '%s')" % combat.intent_label.text
	)

	# --- six turns, playing no cards ---
	var detected_on_turn := -1
	for i in EXPECTED_NOISE.size():
		var turn_number := i + 1
		if not is_instance_valid(combat) or combat.is_queued_for_deletion():
			_check(false, "the combat screen survives to turn %d" % turn_number)
			break

		combat._on_end_turn_button_pressed()

		if is_instance_valid(combat) and not combat.is_queued_for_deletion():
			var expected := "Noise: %d / 10" % EXPECTED_NOISE[i]
			_check(
				combat.noise_label.text == expected,
				"turn %d leaves the meter at %s (got '%s')" % [turn_number, expected, combat.noise_label.text]
			)
		elif detected_on_turn == -1:
			detected_on_turn = turn_number

		await process_frame

		if detected_on_turn == -1 and _screen() != combat:
			detected_on_turn = turn_number

	# --- detection ---
	await process_frame
	await process_frame

	_check(
		detected_on_turn == EXPECTED_NOISE.size(),
		"detection happens on turn %d, not earlier (got %d)" % [EXPECTED_NOISE.size(), detected_on_turn]
	)

	var caught := _screen()
	_check(caught != null and caught.name == "CaughtScreen", "the caught screen takes over")
	_check(
		not (is_instance_valid(combat) and not combat.is_queued_for_deletion()),
		"the combat screen is gone, so no extra turn is playable"
	)
	_check(str(_flow.run_snapshot().get("phase", "")) == "caught", "the run state records the caught phase")

	# The fine is applied by the caught screen's _ready, so it must land exactly
	# once however many frames pass afterwards.
	var expected_credits := credits_before - EXPECTED_FINE
	_check(
		_player_state.money() == expected_credits,
		"the fine is applied once: %d -> %d (got %d)" % [credits_before, expected_credits, _player_state.money()]
	)

	if caught != null:
		var box := caught.get_node_or_null("VBoxContainer")
		if box:
			var fine_label := box.get_node_or_null("FineLabel")
			var upgrades_label := box.get_node_or_null("UpgradesLabel")
			_check(
				fine_label != null and fine_label.text == "Fined: $%d" % EXPECTED_FINE,
				"the fine is shown on the caught screen"
			)
			_check(upgrades_label != null, "the upgrades outcome is shown on the caught screen")

	_finish()
