# Covers the rarity colours from story #109: Common white, Uncommon green,
# Rare blue, Legendary gold, and that cards are drawn in them in the combat
# hand, its detail panel and tooltip, and the deck viewer. The shop's rows are
# checked in shop_test.gd.
#
# Run it with:
#
#     godot --headless --path godot -s res://tests/card_rarity_test.gd
extends SceneTree

const CardRarity := preload("res://scenes/card_rarity.gd")
const WATCHDOG_SECONDS := 60.0
const RARE_CARD := "burn_the_server"

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
		print("card rarity test: TIMED OUT after %.0fs" % WATCHDOG_SECONDS)
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


func _finish() -> void:
	_finished = true
	if _failures.is_empty():
		print("card rarity test: PASSED")
		quit(0)
	else:
		print("card rarity test: FAILED (%d)" % _failures.size())
		for failure in _failures:
			print("  - %s" % failure)
		quit(1)


func _is_whiteish(colour: Color) -> bool:
	return colour.r > 0.8 and colour.g > 0.8 and colour.b > 0.8


func _check_palette() -> void:
	print("Palette")
	var common := CardRarity.colour_for("Common")
	var uncommon := CardRarity.colour_for("Uncommon")
	var rare := CardRarity.colour_for("Rare")
	var legendary := CardRarity.colour_for("Legendary")
	_check(_is_whiteish(common), "Common is white")
	_check(uncommon.g > 0.8 and uncommon.r < 0.5 and uncommon.b < 0.6, "Uncommon is green")
	_check(rare.b > 0.8 and rare.r < 0.5, "Rare is blue")
	_check(legendary.r > 0.9 and legendary.g > 0.5 and legendary.b < 0.3, "Legendary is gold")
	var distinct := {}
	for colour in [common, uncommon, rare, legendary]:
		distinct[colour] = true
	_check(distinct.size() == 4, "every rarity has its own colour")
	var unknown := CardRarity.colour_for("Mythic")
	_check(not distinct.has(unknown), "an unknown rarity is not shown as a real one")
	_check(
		CardRarity.stripe_style("Rare").border_color == rare
		and CardRarity.stripe_style("Rare").border_width_left == CardRarity.STRIPE_WIDTH,
		"a row stripe is a left bar in the rarity's colour"
	)


func _frame_colour(button: Button) -> Color:
	return (button.get_theme_stylebox("normal") as StyleBoxFlat).border_color


func _run() -> void:
	_check_palette()

	await process_frame
	var flow: Node = root.get_node("FlowCoordinator")
	var main: Node = load("res://scenes/main.tscn").instantiate()
	root.add_child(main)
	await process_frame
	_check(flow.add_card_to_deck(RARE_CARD), "a rare card joins the deck")

	var selected: Dictionary = preload("res://tests/combat_fixture.gd").enter(flow)
	_check(selected.get("ok", false), "combat fixture loads through the real flow lifecycle")
	if not selected.get("ok", false):
		_finish()
		return
	await process_frame
	var combat: Node = root.get_node("Main/ScreenHost").get_child(0)
	var draw: Node = combat.draw_phase
	draw.hand_size = draw.hand_names().size() + draw.draw_pile_count() + draw.discard_pile_count()
	combat.selected_index = -1
	combat._draw_hand()

	print("Combat hand")
	var seen := {}
	for i in combat.card_buttons.size():
		var rarity := str(draw.card_detail(i).get("rarity", ""))
		seen[rarity] = true
		var expected := CardRarity.colour_for(rarity)
		var frame := _frame_colour(combat.card_buttons[i])
		_check(
			frame.r == expected.r and frame.g == expected.g and frame.b == expected.b,
			"card %d's frame is its %s colour" % [i, rarity]
		)
	_check(seen.has("Common") and seen.has("Rare"), "the hand has both common and rare cards")

	print("Detail panel and tooltip")
	var rare_index := -1
	for i in combat.card_buttons.size():
		if str(draw.card_detail(i).get("rarity", "")) == "Rare":
			rare_index = i
			break
	combat.card_buttons[rare_index].pressed.emit()
	_check(
		combat.detail_title.get_theme_color("font_color") == CardRarity.colour_for("Rare"),
		"the detail panel's title is rare blue"
	)
	combat.card_buttons[rare_index].mouse_entered.emit()
	_check(
		combat.card_tooltip.title_label.get_theme_color("font_color") == CardRarity.colour_for("Rare"),
		"the tooltip's title is rare blue"
	)
	combat.card_buttons[rare_index].mouse_exited.emit()

	print("Deck viewer")
	combat.deck_viewer.open()
	var row: Node = combat.deck_viewer.list.get_node_or_null("Card_%s" % RARE_CARD)
	_check(row != null, "the deck viewer lists the rare card")
	if row != null:
		_check(
			row.get_child(0).get_theme_color("font_color") == CardRarity.colour_for("Rare"),
			"its heading is rare blue"
		)
	combat.deck_viewer.close()

	_finish()
