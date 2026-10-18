# Covers story #45: playing cards is animated. The hand deals in, a played
# card flies off as a ghost while the rest of the hand closes the gap, a hit
# shakes the enemy and drains its integrity bar, numbers float off what
# changed, and ending the turn throws the hand away and deals a new one. Every
# effect is checked to finish and clean up after itself, and to leave the
# game state exactly as the play left it.
#
# Run it with:
#
#     godot --headless --path godot -s res://tests/combat_animation_test.gd
extends SceneTree

const WATCHDOG_SECONDS := 60.0
# Long enough for the slowest effect, a floating number, to finish.
const SETTLE_SECONDS := 1.2

var _flow: Node
var _combat: Node
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
		print("combat animation test: TIMED OUT after %.0fs" % WATCHDOG_SECONDS)
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
	if _failures.is_empty():
		print("combat animation test: PASSED")
		quit(0)
	else:
		print("combat animation test: FAILED (%d)" % _failures.size())
		for failure in _failures:
			print("  - %s" % failure)
		quit(1)


func _settle() -> void:
	await create_timer(SETTLE_SECONDS).timeout


# Fixture only: draw the whole deck with energy to spare, so there is an
# attack to play and cards either side of it.
func _draw_full_deck() -> void:
	var draw: Node = _combat.draw_phase
	draw.hand_size = (
		draw.hand_names().size() + draw.draw_pile_count() + draw.discard_pile_count()
	)
	draw.max_energy = 10
	_combat.selected_index = -1
	_combat._draw_hand()


func _hand_at_rest() -> bool:
	for button: Button in _combat.card_buttons:
		if not is_equal_approx(button.position.y, _combat.CARD_LIFT):
			return false
		if not is_zero_approx(button.position.x) or not is_equal_approx(button.modulate.a, 1.0):
			return false
	return true


func _floating(prefix: String) -> bool:
	for child in _combat.fx.get_children():
		if child is Label and child.text.begins_with(prefix):
			return true
	return false


func _floating_ending(suffix: String) -> bool:
	for child in _combat.fx.get_children():
		if child is Label and child.text.ends_with(suffix):
			return true
	return false


# A playable attack that is not the last card, so cards sit to its right.
func _attack_index() -> int:
	for i in _combat.card_buttons.size() - 1:
		var detail: Dictionary = _combat.draw_phase.card_detail(i)
		if str(detail.get("type", "")) == "Attack" and not _combat.card_buttons[i].disabled:
			return i
	return -1


func _run() -> void:
	await process_frame
	_flow = root.get_node("FlowCoordinator")
	var main: Node = load("res://scenes/main.tscn").instantiate()
	root.add_child(main)
	await process_frame

	var selected: Dictionary = preload("res://tests/combat_fixture.gd").enter(_flow)
	_check(selected.get("ok", false), "combat fixture loads through the real flow lifecycle")
	if not selected.get("ok", false):
		_finish()
		return
	await process_frame
	_combat = _screen()
	_check(_combat.fx != null, "the combat screen has an effects layer")

	print("Dealing the hand")
	_draw_full_deck()
	var first: Button = _combat.card_buttons[0]
	_check(
		first.modulate.a < 1.0 and first.position.y > _combat.CARD_LIFT,
		"a fresh hand starts below its place, faded out"
	)
	await _settle()
	_check(_hand_at_rest(), "every dealt card ends at rest, fully shown")

	print("Hover lift")
	first.mouse_entered.emit()
	await _settle()
	_check(is_zero_approx(first.position.y), "a hovered card eases up out of the hand")
	first.mouse_exited.emit()
	await _settle()
	_check(is_equal_approx(first.position.y, _combat.CARD_LIFT), "and back down when left")

	print("Playing an attack")
	var index := _attack_index()
	_check(index >= 0, "the full deck has a playable attack with cards after it")
	if index < 0:
		_finish()
		return
	var icon_tint: Color = _combat.enemy_icon.modulate
	var hand_before: int = _combat.card_buttons.size()
	var follower_x: float = _combat.card_buttons[index + 1].global_position.x
	_combat.card_buttons[index].pressed.emit()
	_combat._on_play_button_pressed()
	var health: int = _combat.sentry.health()

	_check(_combat.card_buttons.size() == hand_before - 1, "the play takes the card out of the hand")
	_check(_combat.fx.active_effects() > 0, "the played card flies off as a ghost")
	_check(_floating("-"), "the damage floats off the enemy")
	if int(_combat.status_label.text.get_slice("Noise change: ", 1).to_int()) != 0:
		_check(_floating_ending(" noise"), "the noise change floats off the meter")
	await process_frame
	var follower: Button = _combat.card_buttons[index]
	_check(
		absf(follower.global_position.x - follower_x) < 8.0,
		"the next card starts where it was before the gap closed"
	)
	await _settle()
	_check(_hand_at_rest(), "the rest of the hand has closed the gap")
	_check(_combat.fx.active_effects() == 0, "every ghost and number has cleaned up")
	_check(
		is_equal_approx(_combat.integrity_bar.value, health),
		"the integrity bar has drained to the enemy's health (%s, want %d)"
		% [_combat.integrity_bar.value, health]
	)
	_check(
		_combat.enemy_icon.modulate.is_equal_approx(icon_tint),
		"the hit flash fades back to the icon's own tint (%s, want %s)"
		% [_combat.enemy_icon.modulate, icon_tint]
	)
	_check(is_equal_approx(_combat.enemy_icon.scale.x, 1.0), "the enemy settles after the shake")

	print("Ending the turn")
	var old_hand: int = _combat.card_buttons.size()
	_combat._on_end_turn_button_pressed()
	if _combat.is_queued_for_deletion() or _screen() != _combat:
		# The sentry's turn ended the fight; nothing left to watch.
		_finish()
		return
	_check(_combat.fx.active_effects() >= old_hand, "the old hand flies off to the discard pile")
	_check(
		_combat.card_buttons.size() > 0 and _combat.card_buttons[0].modulate.a < 1.0,
		"the new hand is being dealt"
	)
	await _settle()
	_check(_hand_at_rest(), "the new hand ends at rest")
	_check(_combat.fx.active_effects() == 0, "the discarded ghosts have cleaned up")

	print("Two plays before a redraw")
	_combat.selected_index = -1
	for attempt in 2:
		for i in _combat.card_buttons.size():
			if not _combat.card_buttons[i].disabled:
				_combat.card_buttons[i].pressed.emit()
				_combat._on_play_button_pressed()
				break
		if _screen() != _combat:
			_finish()
			return
	await _settle()
	_check(_hand_at_rest(), "back-to-back plays still leave the hand at rest")

	_finish()
