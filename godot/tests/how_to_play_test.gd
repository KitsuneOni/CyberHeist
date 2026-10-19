# Real scene/extension coverage for story #113: a short How to play screen
# from the main menu, so a new player understands noise, energy and intents
# before their first run.
#
#   Scenario: Open How to play
#     Given I am on the main menu
#     When I choose How to play
#     Then I see short explanations of energy, noise, detection, intents and
#       the map
#     And a Back button returns me to the main menu
#
# Escape goes back too, the pause menu stays shut (there is no run to pause),
# and the screen is refused from anywhere but the main menu.
#
# Run: godot --headless --path godot -s res://tests/how_to_play_test.gd
extends SceneTree

const REQUIRED_TOPICS := ["ENERGY", "NOISE", "DETECTION", "INTENTS", "THE MAP"]

var _flow: Node
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
	if _failures.is_empty():
		print("how to play test: PASSED")
		quit(0)
	else:
		print("how to play test: FAILED (%d)" % _failures.size())
		for failure in _failures:
			print("  - %s" % failure)
		quit(1)


# Opens How to play from the main menu's button, as a player would. Returns
# the screen, or null with a failure recorded.
func _open_from_menu() -> Node:
	var menu := _screen()
	if menu == null or menu.name != "MainMenu":
		_check(false, "the main menu is showing")
		return null
	menu.how_to_play_button.pressed.emit()
	await process_frame
	var screen := _screen()
	_check(screen != null and screen.name == "HowToPlay", "How to play opens its screen")
	if screen == null or screen.name != "HowToPlay":
		return null
	return screen


func _run() -> void:
	_flow = root.get_node_or_null("/root/FlowCoordinator")
	_check(_flow != null, "FlowCoordinator autoload is present")
	if _flow == null:
		_finish()
		return

	var main: Node = load("res://scenes/main.tscn").instantiate()
	root.add_child(main)
	await process_frame
	await process_frame

	print("The main menu offers How to play")
	var menu := _screen()
	if menu == null or menu.name != "MainMenu":
		_check(false, "the game opens on the main menu")
		_finish()
		return
	_check(
		menu.how_to_play_button.text.strip_edges().ends_with("How to play"),
		"the menu lists How to play"
	)
	_check(
		menu.how_to_play_button.get_index() == menu.settings_button.get_index() - 1,
		"it sits just above Settings"
	)

	print("The screen explains the basics")
	var screen := await _open_from_menu()
	if screen == null:
		_finish()
		return
	var headings: Array[String] = screen.shown_headings()
	for topic: String in REQUIRED_TOPICS:
		_check(headings.has(topic), "it explains %s" % topic.to_lower())
	for block in screen.topics_grid.get_children():
		var body: Label = block.get_node("Body")
		_check(
			not body.text.is_empty() and body.text.length() <= 360,
			"the %s explanation is short" % block.get_node("Heading").text.to_lower()
		)
	_check(screen.back_button.has_focus(), "Back is selected, so Enter returns at once")
	_check(not _flow.can_pause(), "there is no run to pause from How to play")

	print("Back and Escape return to the main menu")
	screen.back_button.pressed.emit()
	await process_frame
	menu = _screen()
	_check(menu != null and menu.name == "MainMenu", "Back returns to the main menu")

	screen = await _open_from_menu()
	if screen == null:
		_finish()
		return
	var escape := InputEventAction.new()
	escape.action = "ui_cancel"
	escape.pressed = true
	Input.parse_input_event(escape)
	await process_frame
	await process_frame
	menu = _screen()
	_check(menu != null and menu.name == "MainMenu", "Escape returns to the main menu")

	print("Only from the main menu")
	_flow.start_new_contract()
	await process_frame
	var away := _screen()
	_check(
		not _flow.open_how_to_play().get("ok", true),
		"How to play is refused once a run is on screen"
	)
	await process_frame
	_check(_screen() == away, "and the run's screen stays put")

	_finish()
