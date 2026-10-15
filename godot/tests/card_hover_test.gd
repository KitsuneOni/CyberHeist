# Covers story #14: hovering a card in hand shows its full description, and
# moving off it dismisses it. Drives the real combat screen headless, sending
# the hover signals the way the other tests send button presses.
#
# Run it with:
#
#     godot --headless --path godot -s res://tests/card_hover_test.gd
extends SceneTree

const CardTooltip := preload("res://scenes/card_tooltip.gd")
const WATCHDOG_SECONDS := 60.0

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
		print("card hover test: TIMED OUT after %.0fs" % WATCHDOG_SECONDS)
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
		print("card hover test: PASSED")
		quit(0)
	else:
		print("card hover test: FAILED (%d)" % _failures.size())
		for failure in _failures:
			print("  - %s" % failure)
		quit(1)


# Fixture only: draw the whole deck so every card type is in hand.
func _draw_full_deck() -> void:
	var draw: Node = _combat.draw_phase
	draw.hand_size = (
		draw.hand_names().size() + draw.draw_pile_count() + draw.discard_pile_count()
	)
	_combat.selected_index = -1
	_combat._draw_hand()


func _hover(index: int) -> void:
	_combat.card_buttons[index].mouse_entered.emit()


func _unhover(index: int) -> void:
	_combat.card_buttons[index].mouse_exited.emit()


func _index_with_keywords() -> int:
	for i in _combat.card_buttons.size():
		if not _combat.draw_phase.card_detail(i).get("keywords", []).is_empty():
			return i
	return -1


func _check_tooltip_matches(index: int) -> void:
	var tooltip: PanelContainer = _combat.card_tooltip
	var detail: Dictionary = _combat.draw_phase.card_detail(index)
	var name: String = _combat.draw_phase.hand_names()[index]
	_check(tooltip.visible, "hovering %s shows a tooltip" % name)
	_check(
		tooltip.describing() == _combat.card_buttons[index],
		"the tooltip describes the hovered card"
	)
	_check(
		tooltip.title_label.text.begins_with(name),
		"the tooltip is titled %s (got '%s')" % [name, tooltip.title_label.text]
	)
	_check(
		tooltip.stats_label.text == CardTooltip.stats_for(detail),
		"the tooltip shows %s's energy and noise" % name
	)
	_check(
		tooltip.description_label.text == str(detail.get("description", ""))
		and not tooltip.description_label.text.is_empty(),
		"the tooltip shows %s's full description" % name
	)


func _check_on_screen() -> void:
	var tooltip: PanelContainer = _combat.card_tooltip
	var screen: Rect2 = tooltip.get_viewport_rect()
	_check(
		screen.encloses(tooltip.get_global_rect()),
		"the tooltip stays on screen (%s inside %s)" % [tooltip.get_global_rect(), screen]
	)


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
	_draw_full_deck()
	await process_frame

	var tooltip: PanelContainer = _combat.card_tooltip
	_check(not tooltip.visible, "no tooltip before anything is hovered")

	print("Hover and dismiss")
	_hover(0)
	_check_tooltip_matches(0)
	_check_on_screen()
	_check(_combat.selected_index == -1, "hovering does not select the card")
	_unhover(0)
	_check(not tooltip.visible, "moving off the card dismisses the tooltip")

	print("Moving across the hand")
	var last: int = _combat.card_buttons.size() - 1
	_hover(0)
	_hover(last)
	# Godot can deliver the old card's exit after the new card's enter.
	_unhover(0)
	_check_tooltip_matches(last)
	_check_on_screen()
	_unhover(last)

	print("Keywords")
	var keyword_index := _index_with_keywords()
	_check(keyword_index >= 0, "the full deck has a card with keywords")
	if keyword_index >= 0:
		_hover(keyword_index)
		var detail: Dictionary = _combat.draw_phase.card_detail(keyword_index)
		_check(
			tooltip.keywords_label.visible
			and tooltip.keywords_label.text == CardTooltip.keywords_for(detail),
			"the tooltip explains every keyword the card carries"
		)
		_unhover(keyword_index)

	print("Matches the detail panel")
	_combat.card_buttons[0].pressed.emit()
	_hover(0)
	_check(
		tooltip.title_label.text == _combat.detail_title.text
		and tooltip.stats_label.text == _combat.detail_stats.text
		and tooltip.description_label.text == _combat.detail_description.text
		and tooltip.keywords_label.text == _combat.detail_keywords.text,
		"the tooltip and the detail panel say the same thing"
	)
	_unhover(0)

	print("Unaffordable cards")
	_combat.draw_phase.energy = 0
	_combat.selected_index = -1
	_combat._rebuild_card_buttons(_combat.draw_phase.hand_names())
	var disabled_index := -1
	for i in _combat.card_buttons.size():
		if _combat.card_buttons[i].disabled:
			disabled_index = i
			break
	_check(disabled_index >= 0, "with no energy some card is unplayable")
	if disabled_index >= 0:
		_hover(disabled_index)
		_check_tooltip_matches(disabled_index)
		_unhover(disabled_index)

	print("Hand rebuilt under the mouse")
	_hover(0)
	_combat._rebuild_card_buttons(_combat.draw_phase.hand_names())
	_check(not tooltip.visible, "rebuilding the hand dismisses a tooltip left on an old card")

	_finish()
