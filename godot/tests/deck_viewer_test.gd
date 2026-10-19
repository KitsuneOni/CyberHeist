# Covers story #55: viewing every card in the run deck from the map and from
# combat. Checks the listing against the run deck, that a newly gained card
# shows up, and that the viewer opens, blocks the screen and closes again.
#
# Run it with:
#
#     godot --headless --path godot -s res://tests/deck_viewer_test.gd
extends SceneTree

const DeckViewer := preload("res://scenes/deck_viewer.gd")
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
		print("deck viewer test: TIMED OUT after %.0fs" % WATCHDOG_SECONDS)
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
		print("deck viewer test: PASSED")
		quit(0)
	else:
		print("deck viewer test: FAILED (%d)" % _failures.size())
		for failure in _failures:
			print("  - %s" % failure)
		quit(1)


func _check_sorting() -> void:
	var listing := [
		{"id": "c", "name": "Zeta", "type": "Skill", "count": 1},
		{"id": "a", "name": "Beta", "type": "Attack", "count": 2},
		{"id": "b", "name": "Alpha", "type": "Skill", "count": 3},
		{"id": "d", "name": "Alpha", "type": "Attack", "count": 1},
	]
	var ids := []
	for card: Dictionary in DeckViewer.sorted_listing(listing):
		ids.append(card.id)
	_check(ids == ["d", "a", "b", "c"], "cards are grouped by type, then by name (got %s)" % [ids])
	_check(DeckViewer.total_cards(listing) == 7, "the total counts every copy")
	_check(
		DeckViewer.heading_for(listing[1]) == "2×  Beta — Attack · ?",
		"a row leads with how many copies there are"
	)


# The listing the viewer is built from has to agree with the deck the run will
# actually draw from: every owned card once, with every copy counted.
func _check_listing_matches_deck() -> void:
	var owned: Dictionary = {}
	for id in root.get_node("FlowCoordinator/RunDeck").card_ids():
		owned[id] = int(owned.get(id, 0)) + 1
	var listing: Array = _flow.run_deck_listing()
	var listed: Dictionary = {}
	for card: Dictionary in listing:
		listed[card.id] = int(card.count)
	_check(listed == owned, "the listing has every owned card with its copies")
	_check(
		DeckViewer.total_cards(listing) == _flow.run_deck_size(),
		"the listing adds up to the deck size (%d)" % _flow.run_deck_size()
	)
	var described := true
	for card: Dictionary in listing:
		described = described and not str(card.get("description", "")).is_empty()
	_check(described, "every listed card carries its description")


func _check_viewer(screen: Node, where: String) -> void:
	var button := screen.get_node_or_null("%ViewDeckButton") as Button
	if button == null:
		button = screen.find_child("ViewDeckButton", true, false) as Button
	var viewer := screen.get_node_or_null("DeckViewer") as Control
	_check(button != null and viewer != null, "%s has a deck button and viewer" % where)
	if button == null or viewer == null:
		return
	_check(
		button.text.contains("(%d)" % _flow.run_deck_size()),
		"%s's button shows the deck size (got '%s')" % [where, button.text]
	)
	_check(not viewer.is_open(), "the viewer starts closed on %s" % where)

	button.pressed.emit()
	_check(viewer.is_open(), "pressing the button on %s opens the viewer" % where)
	_check(
		viewer.mouse_filter == Control.MOUSE_FILTER_STOP,
		"the open viewer stops clicks reaching %s underneath" % where
	)
	_check(
		viewer.shown_cards.size() == _flow.run_deck_listing().size(),
		"the viewer on %s shows every distinct card" % where
	)
	_check(
		viewer.title_label.text.contains("%d cards" % _flow.run_deck_size()),
		"the title counts the whole deck (got '%s')" % viewer.title_label.text
	)
	_check(viewer.close_button.has_focus(), "Close has focus while the viewer is open")

	viewer.close_button.pressed.emit()
	_check(not viewer.is_open(), "Close shuts the viewer on %s" % where)
	_check(button.has_focus(), "focus goes back to the deck button on %s" % where)

	button.pressed.emit()
	var escape := InputEventAction.new()
	escape.action = "ui_cancel"
	escape.pressed = true
	root.push_input(escape)
	_check(not viewer.is_open(), "Escape also shuts the viewer on %s" % where)


func _run() -> void:
	print("Ordering")
	_check_sorting()

	await process_frame
	_flow = root.get_node("FlowCoordinator")
	var main: Node = load("res://scenes/main.tscn").instantiate()
	root.add_child(main)
	await process_frame

	print("Listing")
	_check_listing_matches_deck()

	print("On the map")
	_check(_flow.start_new_contract().get("ok", false), "a new contract opens the desktop")
	await process_frame
	_check(_flow.open_contracts().get("ok", false), "contracts.exe opens the hub")
	await process_frame
	await process_frame
	var hub := _screen()
	_check(hub != null and hub.name == "ContractHub", "the map screen is up")
	if hub == null or hub.name != "ContractHub":
		_finish()
		return
	_check_viewer(hub, "the map")

	print("A new card")
	var reward_id := "wannacry"
	var before := int(root.get_node("FlowCoordinator/RunDeck").count_of(reward_id))
	_check(_flow.take_card_reward(reward_id), "a reward card is added to the run deck")
	hub.view_deck_button.pressed.emit()
	var shown := -1
	for card: Dictionary in hub.deck_viewer.shown_cards:
		if card.id == reward_id:
			shown = int(card.count)
	_check(shown == before + 1, "reopening shows the new card's copies (got %d)" % shown)
	_check(
		hub.deck_viewer.list.find_child("Card_%s" % reward_id, false, false) != null,
		"the new card has its own row"
	)
	hub.deck_viewer.close()
	_check_listing_matches_deck()

	print("In combat")
	var selected: Dictionary = CombatFixture.enter(_flow)
	_check(selected.get("ok", false), "combat fixture loads through the real flow lifecycle")
	if not selected.get("ok", false):
		_finish()
		return
	await process_frame
	var combat := _screen()
	_check(combat != null and combat.name == "CombatScreen", "the combat screen is up")
	if combat != null and combat.name == "CombatScreen":
		_check_viewer(combat, "combat")

	_finish()
