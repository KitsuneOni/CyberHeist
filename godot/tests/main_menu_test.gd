# Real scene/extension coverage for story #33: a main menu when the game
# launches, to start a new run, continue a saved run, or exit.
#
#   Scenario: The game opens on the main menu
#     Given I launch the game
#     Then I see the main menu rather than the contract map
#     And I can choose New contract, Continue run or Quit
#
#   Scenario: Start a new run
#     Given I am on the main menu
#     When I choose New contract
#     Then the contract map opens on a fresh run
#
#   Scenario: Continue is unavailable without a save
#     Given there is no saved run
#     Then Continue run is greyed out and cannot be chosen
#
#   Scenario: The menu works from the keyboard
#     Given I am on the main menu
#     Then New contract is selected
#     And moving down skips Continue run when it is unavailable
#
# Run: godot --headless --path godot -s res://tests/main_menu_test.gd
extends SceneTree

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
		print("main menu test: PASSED")
		quit(0)
	else:
		print("main menu test: FAILED (%d)" % _failures.size())
		for failure in _failures:
			print("  - %s" % failure)
		quit(1)


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

	# --- the game opens on the main menu ---
	var menu := _screen()
	_check(menu != null and menu.name == "MainMenu", "the game opens on the main menu")
	if menu == null or menu.name != "MainMenu":
		_finish()
		return

	var new_contract: Button = menu.get_node_or_null("%NewContractButton")
	var continue_run: Button = menu.get_node_or_null("%ContinueButton")
	var quit_button: Button = menu.get_node_or_null("%QuitButton")
	_check(new_contract != null, "the menu offers New contract")
	_check(continue_run != null, "the menu offers Continue run")
	_check(quit_button != null, "the menu offers Quit")
	if new_contract == null or continue_run == null or quit_button == null:
		_finish()
		return

	# --- continue is unavailable without a save ---
	_check(not _flow.has_saved_run(), "a fresh launch has no saved run")
	_check(continue_run.disabled, "Continue run is greyed out when there is no save")

	# --- keyboard navigation ---
	_check(new_contract.has_focus(), "New contract is selected when the menu opens")
	_check(
		new_contract.find_valid_focus_neighbor(SIDE_BOTTOM) == quit_button,
		"moving down from New contract skips the unavailable Continue run"
	)

	# --- quit is wired up (pressing it would end this test, so check the wiring) ---
	_check(
		quit_button.pressed.get_connections().size() > 0,
		"Quit is connected to an action"
	)

	# --- start a new run ---
	new_contract.pressed.emit()
	await process_frame
	await process_frame

	var hub := _screen()
	_check(hub != null and hub.name == "ContractHub", "New contract opens the contract map")
	_check(_flow.run_snapshot().phase == "hub", "the new run starts with no encounter active")
	_check(
		not _flow.selectable_encounters().is_empty(),
		"the new run has encounters ready to choose"
	)

	_finish()
