# Real scene/extension coverage for story #118: keyboard shortcuts to play
# cards and end the turn, so the player can play quickly without the mouse.
#
#   Scenario: Play a card with a number key
#     Given it is my turn with cards in hand
#     When I press 3
#     Then the third card in my hand is played
#
#   Scenario: End turn with a key
#     Given it is my turn
#     When I press E
#     Then my turn ends as if I pressed End Turn
#
# Keys go through Input.parse_input_event, so they reach the combat screen the
# way a real key press does. Also checked: each card shows its key, the keypad
# digits work, a number past the end of the hand or an unaffordable card plays
# nothing, held keys do not repeat, and keys do nothing while the deck viewer
# is open or the game is paused.
#
# Run: godot --headless --path godot -s res://tests/keyboard_shortcuts_test.gd
extends SceneTree

const CombatFixture := preload("res://tests/combat_fixture.gd")
const WATCHDOG_SECONDS := 60.0

var _flow: Node
var _failures: Array[String] = []
var _finished := false
var _elapsed := 0.0


func _initialize() -> void:
	_run.call_deferred()


func _process(delta: float) -> bool:
	if _finished:
		return true
	_elapsed += delta
	if _elapsed >= WATCHDOG_SECONDS:
		print("keyboard shortcuts test: TIMED OUT after %.0fs" % WATCHDOG_SECONDS)
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


func _finish() -> void:
	_finished = true
	paused = false
	if _failures.is_empty():
		print("keyboard shortcuts test: PASSED")
		quit(0)
	else:
		print("keyboard shortcuts test: FAILED (%d)" % _failures.size())
		for failure in _failures:
			print("  - %s" % failure)
		quit(1)


# Presses and releases a key as the keyboard would, then lets it arrive.
func _press(keycode: Key, echo := false) -> void:
	var down := InputEventKey.new()
	down.keycode = keycode
	down.physical_keycode = keycode
	down.pressed = true
	down.echo = echo
	Input.parse_input_event(down)
	var up := down.duplicate() as InputEventKey
	up.pressed = false
	up.echo = false
	Input.parse_input_event(up)
	await process_frame
	await process_frame


func _digit_key(place: int) -> Key:
	return KEY_1 + place - 1 as Key


func _first_card(combat: Node, affordable: bool) -> int:
	for index in combat.card_buttons.size():
		if combat.card_buttons[index].disabled != affordable:
			return index
	return -1


func _run() -> void:
	_flow = root.get_node_or_null("/root/FlowCoordinator")
	_check(_flow != null, "FlowCoordinator autoload is present")
	if _flow == null:
		_finish()
		return

	var main: Node = load("res://scenes/main.tscn").instantiate()
	root.add_child(main)
	await process_frame
	_flow.start_new_contract()
	await process_frame
	var entered: Dictionary = CombatFixture.enter(_flow)
	_check(entered.get("ok", false), "combat loads through the real flow")
	await process_frame
	await process_frame
	var combat := _screen()
	if combat == null or combat.name != "CombatScreen":
		_check(false, "the combat screen is up")
		_finish()
		return
	var draw_phase: Node = combat.draw_phase

	print("Each card shows its key")
	var hand_size: int = combat.card_buttons.size()
	_check(hand_size == draw_phase.hand_names().size() and hand_size >= 3, "the hand is on screen (%d cards)" % hand_size)
	var hints_match := true
	for index in hand_size:
		var hint: Label = combat.card_buttons[index].get_node_or_null("KeyHint")
		hints_match = hints_match and hint != null and hint.visible and hint.text == str(index + 1)
	_check(hints_match, "each card is labelled from 1, left to right")
	_check(
		combat.get_node("%EndTurnButton").text.contains("[E]"),
		"End Turn shows its key"
	)

	print("A number key plays that card")
	# The story's own example is pressing 3, so use the third card when it can
	# be afforded and the first affordable one otherwise.
	var place := 2 if hand_size >= 3 and not combat.card_buttons[2].disabled else _first_card(combat, true)
	_check(place >= 0, "the hand has a card that can be afforded")
	if place < 0:
		_finish()
		return
	var names: PackedStringArray = draw_phase.hand_names()
	var expected: PackedStringArray = names.duplicate()
	expected.remove_at(place)
	var plays_before: int = draw_phase.play_count
	await _press(_digit_key(place + 1))
	_check(draw_phase.play_count == plays_before + 1, "pressing %d plays a card" % (place + 1))
	_check(
		draw_phase.hand_names() == expected,
		"it was card %d, %s, that left the hand" % [place + 1, names[place]]
	)
	_check(combat.selected_index == -1, "nothing is left selected after the play")
	var hints_renumbered := true
	for index in combat.card_buttons.size():
		var hint: Label = combat.card_buttons[index].get_node("KeyHint")
		hints_renumbered = hints_renumbered and hint.text == str(index + 1)
	_check(hints_renumbered, "the remaining cards are renumbered from 1")

	print("Keys that play nothing")
	plays_before = draw_phase.play_count
	await _press(KEY_9)
	_check(draw_phase.play_count == plays_before, "a number past the end of the hand plays nothing")
	_check(
		combat.status_label.text.contains("no card 9"),
		"and says there is no such card: %s" % combat.status_label.text
	)

	var unaffordable := _first_card(combat, false)
	if unaffordable >= 0:
		await _press(_digit_key(unaffordable + 1))
		_check(draw_phase.play_count == plays_before, "an unaffordable card is not played")
		_check(
			combat.status_label.text.begins_with("Not enough energy"),
			"and the player is told why"
		)

	place = _first_card(combat, true)
	if place >= 0:
		await _press(_digit_key(place + 1), true)
		_check(draw_phase.play_count == plays_before, "a held key repeating plays nothing more")

		combat.deck_viewer.open()
		await _press(_digit_key(place + 1))
		_check(draw_phase.play_count == plays_before, "keys do nothing while the deck is open")
		combat.deck_viewer.close()

		paused = true
		await _press(_digit_key(place + 1))
		_check(draw_phase.play_count == plays_before, "keys do nothing while paused")
		paused = false
		await process_frame

		await _press(KEY_KP_1 + place as Key)
		_check(
			draw_phase.play_count == plays_before + 1,
			"the keypad's %d plays a card too" % (place + 1)
		)
	else:
		print("  --    no affordable card left this turn; skipped the repeat and keypad checks")

	print("E ends the turn")
	var turn_before: int = draw_phase.turn_number
	var full_hand := hand_size
	await _press(KEY_E)
	var after := _screen()
	if after == combat:
		_check(draw_phase.turn_number == turn_before + 1, "pressing E ends the turn")
		_check(
			combat.card_buttons.size() == full_hand,
			"and a new hand of %d is dealt" % full_hand
		)
		_check(draw_phase.energy == draw_phase.max_energy, "with energy refilled")
	else:
		# The sentry's turn can end the fight, which is still the turn ending.
		_check(after != null, "pressing E ended the turn and the fight moved on")

	_finish()
