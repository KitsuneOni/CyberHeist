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

const EXPECTED_NOISE := [91, 93, 96, 97, 99, 100]
const ACTION_NAMES := ["Packet Sniff", "Trace Sweep", "Lockdown Probe"]
const ACTION_NOISE := [1, 2, 3]
const EXPECTED_FINE := 50

# A script error inside _run kills the coroutine part way through, so the run
# never reaches _finish and the engine sits there until CI's job timeout kills
# it. The watchdog turns that into a fast, readable failure instead. Generous
# enough that a slow runner never trips it: the whole run takes well under a
# second.
const WATCHDOG_SECONDS := 60.0

var _flow: Node
var _player_state: Node
var _failures: Array[String] = []
var _finished := false
var _elapsed := 0.0
var _resolved_turns: Array[Dictionary] = []


func _initialize() -> void:
	_run.call_deferred()


# SceneTree calls this every frame, which gives the watchdog somewhere to live
# that a dead coroutine cannot take down with it.
func _process(delta: float) -> bool:
	if _finished:
		return true

	_elapsed += delta
	if _elapsed >= WATCHDOG_SECONDS:
		print("")
		print("combat smoke test: TIMED OUT after %.0fs" % WATCHDOG_SECONDS)
		print("  The run stopped part way through, which usually means a script")
		print("  error above killed it. Check for a parse error in a scene it loads.")
		_finished = true
		quit(1)
		return true

	return false


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


# Observe the real signal without calling back into the Rust nodes while they
# are borrowed by end_turn(). The assertions run after the button handler returns.
func _on_turn_resolved(outcome: Dictionary) -> void:
	_resolved_turns.append(outcome.duplicate())


func _check_intent(combat: Node, action_index: int) -> void:
	var action: String = ACTION_NAMES[action_index]
	var noise: int = ACTION_NOISE[action_index]
	_check(
		combat.sentry.queued_action_name() == action
		and combat.sentry.queued_action_noise() == noise,
		"the queued action is %s with authored noise %d" % [action, noise]
	)
	_check(
		combat.intent_label.is_visible_in_tree()
		and combat.intent_label.text == "WARDEN-7 will: %s (+%d noise)" % [action, noise],
		"the visible intent includes the next action name and value"
	)


func _finish() -> void:
	_finished = true
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
	var selected: Dictionary = preload("res://tests/combat_fixture.gd").enter(_flow)
	_check(selected.get("ok", false), "combat fixture loads through the real flow lifecycle")
	if not selected.get("ok", false):
		_finish()
		return
	await process_frame
	await process_frame

	var combat := _screen()
	_check(combat != null and combat.name == "CombatScreen", "the combat screen is showing")
	if combat == null:
		_finish()
		return

	# A scene whose script fails to parse still loads: Godot substitutes a plain
	# placeholder of the base type, so the name and the node tree look right and
	# every check below would pass against a screen that cannot actually be
	# played. Checking the script attached is what catches that.
	_check(combat.get_script() != null, "the combat script loaded (no parse error above)")
	_check(
		combat.has_method("_on_end_turn_button_pressed"),
		"the combat screen can end a turn"
	)
	if combat.get_script() == null or not combat.has_method("_on_end_turn_button_pressed"):
		_finish()
		return

	var credits_before: int = _player_state.money()
	combat.sentry.turn_resolved.connect(_on_turn_resolved)

	# --- the opening state, before a turn is taken ---
	_check(combat.noise_label.text == "Noise: 0 / 100", "the meter starts empty (got '%s')" % combat.noise_label.text)
	_check_intent(combat, 0)
	_check(combat.draw_phase.hand_names().size() == 3, "intent is shown with a playable hand")

	# Use a real near-cap shared value so six turns cover differing actions,
	# queue wrap and the final authored +3 clamped to an actual +1.
	root.get_node("NoiseMeterGlobal").add_noise(90)
	combat._refresh_status_labels()

	# --- six turns, playing no cards ---
	var detected_on_turn := -1
	for i in EXPECTED_NOISE.size():
		var turn_number := i + 1
		if not is_instance_valid(combat) or combat.is_queued_for_deletion():
			_check(false, "the combat screen survives to turn %d" % turn_number)
			break

		var action_index := i % ACTION_NAMES.size()
		_check_intent(combat, action_index)
		var noise_before: int = combat.sentry.noise()
		var previous_turn: int = combat.draw_phase.turn_number
		combat.get_node("VBoxContainer/Buttons/EndTurnButton").pressed.emit()

		# Check before yielding: one button press must resolve one announced
		# action synchronously, including on the fatal, clamped final turn.
		_check(_resolved_turns.size() == turn_number, "End Turn resolves exactly one sentry action")
		if _resolved_turns.size() != turn_number:
			break
		var outcome: Dictionary = _resolved_turns.back()
		var expected_delta: int = mini(ACTION_NOISE[action_index], 100 - noise_before)
		_check(
			outcome.get("ok", false)
			and outcome.get("action", "") == ACTION_NAMES[action_index]
			and outcome.get("noise_added", -1) == expected_delta
			and outcome.get("noise", -1) == EXPECTED_NOISE[i],
			"the announced action executes with its value, clamped only at the cap"
		)
		_check(combat.sentry.noise() == EXPECTED_NOISE[i], "no unannounced noise is applied")
		_check(
			outcome.get("run_failed", false) == (EXPECTED_NOISE[i] == 100),
			"only the capped turn reports detection"
		)

		if is_instance_valid(combat) and not combat.is_queued_for_deletion():
			_check_intent(combat, (action_index + 1) % ACTION_NAMES.size())
			_check(
				combat.draw_phase.turn_number == previous_turn + 1
				and combat.draw_phase.energy == 3
				and combat.draw_phase.hand_names().size() == 3,
				"the next player turn has a fresh hand and energy with the new intent"
			)
			var expected := "Noise: %d / 100" % EXPECTED_NOISE[i]
			_check(
				combat.noise_label.text == expected,
				"turn %d leaves the meter at %s (got '%s')" % [turn_number, expected, combat.noise_label.text]
			)
		elif detected_on_turn == -1:
			detected_on_turn = turn_number
			# The old screen remains alive until the deferred free, but queued
			# input must not execute the new intent after detection.
			combat.get_node("VBoxContainer/Buttons/EndTurnButton").pressed.emit()
			_check(
				_resolved_turns.size() == turn_number and combat.sentry.noise() == 100,
				"caught combat cannot execute another queued action"
			)

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
