# Run through run_shared_noise_test.sh: its isolated starter-deck fixture adds
# authored social cards without changing the game's starter deck or Rust code.
extends SceneTree

var _flow: Node
var _combat: Node
var _meter: Node
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


func _select(name: String) -> bool:
	var index: int = _combat.draw_phase.hand_names().find(name)
	_check(index >= 0, "%s is in the fixture hand" % name)
	if index < 0:
		return false
	_combat.card_buttons[index].pressed.emit()
	return true


func _play(name: String) -> bool:
	if not _select(name):
		return false
	_combat.get_node("VBoxContainer/Buttons/PlayButton").pressed.emit()
	return true


func _check_shared_noise(expected: int) -> void:
	_check(
		_meter.noise == expected
		and _meter.get_noise() == expected
		and _combat.sentry.noise() == expected
		and _combat.noise_bar.value == expected
		and _combat.noise_label.text == "Noise: %d / 100" % expected,
		"autoload, sentry, bar and label share noise %d" % expected
	)
	_check(
		_meter.max_noise == 100
		and _combat.sentry.max_noise() == 100
		and _combat.noise_bar.max_value == 100,
		"cap remains 100 in every component"
	)


func _check_social(name: String, damage: int) -> void:
	if not _select(name):
		return
	var index: int = _combat.selected_index
	var detail: Dictionary = _combat.draw_phase.card_detail(index)
	_check(
		detail.name == name
		and detail.description == "Deal %d damage" % damage
		and detail.keywords[0].label == "Damage %d" % damage,
		"card detail uses the active social side: %s / damage %d" % [name, damage]
	)
	_check(
		_combat.card_buttons[index].text == "%s [1]" % name
		and _combat.detail_title.text.begins_with(name)
		and _combat.detail_description.text == detail.description
		and _combat.detail_keywords.text.begins_with("Damage %d" % damage),
		"card button name, description and keyword panel agree with the shared meter"
	)


func _finish() -> void:
	if _failures.is_empty():
		print("shared noise test: PASSED")
		quit(0)
	else:
		print("shared noise test: FAILED (%d)" % _failures.size())
		quit(1)


func _run() -> void:
	await process_frame
	_flow = root.get_node("FlowCoordinator")
	_meter = root.get_node("NoiseMeterGlobal")
	root.add_child(load("res://scenes/main.tscn").instantiate())
	await process_frame
	var entered: Dictionary = preload("res://tests/combat_fixture.gd").enter(_flow)
	_check(entered.get("ok", false), "combat loads with the real lifecycle")
	if not entered.get("ok", false):
		_finish()
		return
	_combat = _screen()
	_combat.draw_phase.hand_size = 4
	_meter.add_noise(54)
	_combat._draw_hand()
	_check_shared_noise(54)
	_check_social("Nigerian Prince", 8)

	# Recovery crosses the authored threshold downwards; the next loud card
	# stays just below it, then the announced security action crosses it again.
	if not _play("VPN"):
		_finish()
		return
	_check_shared_noise(44)
	_check_social("Nigerian King", 18)
	_check(_combat.draw_phase.energy == 1, "VPN spends two energy")
	_check(_combat.draw_phase.discard_pile_count() == 1, "VPN is discarded once")
	_check(_combat.draw_phase.turn_number == 1, "VPN leaves the player turn active")
	_check(_combat.sentry.queued_action_name() == "Packet Sniff", "VPN preserves queued intent")

	if not _play("Trojan"):
		_finish()
		return
	_check_shared_noise(49)
	_check_social("Nigerian King", 18)
	_combat.get_node("VBoxContainer/Buttons/EndTurnButton").pressed.emit()
	_check_shared_noise(50)
	_check_social("Nigerian Prince", 8)
	_check(_combat.sentry.queued_action_name() == "Trace Sweep", "only the announced action advances")

	# A social card itself pays once, reports its pre-play side, and changes
	# the same meter. No damage/keyword execution is claimed by this slice.
		if not _play("Nigerian Prince"):
		_finish()
		return
	_check_shared_noise(65)
	_check(_combat.draw_phase.energy == 2, "social play pays its authored cost")
	_check(_combat.draw_phase.discard_pile_count() == 1, "social play discards once")
	_check(
		_combat.status_label.text == "Nigerian Prince played. Noise change: +15. Damage: 8.",
		"play feedback preserves the active social side and signed actual change"
	)
	_check_social("Nigerian Prince", 8)

	# Reconstruct the active screen through the coordinator's existing binding
	# path. The meter survives; only the encounter-local intent queue is new.
	var old_combat := _combat
	_flow.bind_screen_host(root.get_node("Main/ScreenHost"))
	_combat = _screen()
	# The generated node may be non-combat; the fixture only replaces content.
	if _combat.name != "CombatScreen":
		_flow._replace_screen(load("res://scenes/combat.tscn").instantiate())
		_combat = _screen()
	_check(root.get_node("NoiseMeterGlobal") == _meter, "screen replacement preserves the autoload identity")
	_check_shared_noise(65)
	_check(old_combat.sentry.noise() == 65, "detached sentry safely reads the same global value")
	var old_turn: int = old_combat.draw_phase.turn_number
	var old_hand: PackedStringArray = old_combat.draw_phase.hand_names()
	var old_intent: String = old_combat.sentry.queued_action_name()
	old_combat.get_node("VBoxContainer/Buttons/EndTurnButton").pressed.emit()
	old_combat.draw_phase.end_turn()
	var rejected: Dictionary = old_combat.draw_phase.play_card(0)
	var skipped: Dictionary = old_combat.sentry.perform_queued_action()
	_check(
		not rejected.ok and rejected.error == "inactive_encounter" and not skipped.ok
		and old_combat.draw_phase.turn_number == old_turn
		and old_combat.draw_phase.hand_names() == old_hand
		and old_combat.sentry.queued_action_name() == old_intent,
		"detached screen cannot play cards or advance a turn while the new screen is active"
	)
	_check_shared_noise(65)
	await process_frame
	_check(not is_instance_valid(old_combat), "old screen is freed without losing the meter")

	var completed: Dictionary = _flow.complete_active_encounter()
	_check(completed.ok and _meter.noise == 65, "normal completion preserves shared noise on the hub")
	entered = preload("res://tests/combat_fixture.gd").enter(_flow)
	_check(entered.get("ok", false), "the next normal encounter loads")
	if not entered.get("ok", false):
		_finish()
		return
	_combat = _screen()
	_check_shared_noise(65)
	var invalid_finish: Dictionary = _flow.finish_caught()
	_check(
		not invalid_finish.ok and _meter.noise == 65,
		"a rejected caught exit cannot clear noise during an active encounter"
	)

	# Detection stays visible until Continue successfully finishes the caught
	# flow. The next attempt then starts fresh rather than remaining stuck.
	_combat.draw_phase.hand_size = 4
	_combat._draw_hand()
	_meter.add_noise(34)
	_combat._refresh_status_labels()
	if not _play("Trojan"):
		_finish()
		return
	_check(_flow.run_snapshot().phase == "caught" and _meter.noise == 100, "loud card catches at the global cap")
	var finished: Dictionary = _flow.finish_caught()
	_check(finished.ok and _meter.noise == 0, "finishing caught resets shared noise to zero on the hub")
	entered = preload("res://tests/combat_fixture.gd").enter(_flow)
	_check(entered.get("ok", false), "the failed encounter remains retryable")
	if entered.get("ok", false):
		_combat = _screen()
		_check_shared_noise(0)
		_combat.draw_phase.hand_size = 4
		_combat._draw_hand()
		_meter.add_noise(5)
		_combat._refresh_status_labels()
		if not _play("VPN"):
			_finish()
			return
		_check_shared_noise(0)
		_check(_combat.draw_phase.energy == 1, "retry permits paid recovery again")
		_combat.get_node("VBoxContainer/Buttons/EndTurnButton").pressed.emit()
		_check_shared_noise(1)
		_check(_flow.run_snapshot().phase == "encounter_active", "retry remains playable after the sentry turn")
	_finish()
