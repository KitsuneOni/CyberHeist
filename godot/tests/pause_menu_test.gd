# Covers story #110: Escape on the map or in combat pauses the game and shows
# Resume, Settings, and Save and Quit; Save and Quit saves the run and returns
# to the main menu, where Continue picks it up. Drives the real Main scene,
# coordinator and save file headless, sending Escape the way a key press would.
#
# Run it with:
#
#     godot --headless --path godot -s res://tests/pause_menu_test.gd
extends SceneTree

const WATCHDOG_SECONDS := 60.0
const SAVE_PATH := "user://pause_menu_test.json"
const MAIN_MENU := "res://scenes/main_menu.tscn"
const HUB := "res://scenes/contract_hub.tscn"
const SETTINGS := "res://scenes/settings_menu.tscn"

var _flow: Node
var _pause: CanvasLayer
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
		print("pause menu test: TIMED OUT after %.0fs" % WATCHDOG_SECONDS)
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
	paused = false
	_remove_save()
	_finished = true
	if _failures.is_empty():
		print("pause menu test: PASSED")
		quit(0)
	else:
		print("pause menu test: FAILED (%d)" % _failures.size())
		for failure in _failures:
			print("  - %s" % failure)
		quit(1)


func _remove_save() -> void:
	DirAccess.remove_absolute(ProjectSettings.globalize_path(SAVE_PATH))


func _screen() -> Node:
	var host := root.get_node_or_null("Main/ScreenHost")
	if host == null or host.get_child_count() == 0:
		return null
	return host.get_child(0)


func _showing(path: String) -> bool:
	var screen := _screen()
	return screen != null and screen.scene_file_path == path


func _escape() -> void:
	var escape := InputEventAction.new()
	escape.action = "ui_cancel"
	escape.pressed = true
	root.push_input(escape)


# Pauses on whatever is showing and checks the menu came up as the story says.
func _check_pauses(where: String) -> void:
	_escape()
	_check(_pause.is_open(), "Escape opens the pause menu on %s" % where)
	_check(paused, "the game is paused on %s" % where)
	_check(
		_pause.resume_button.visible
		and _pause.settings_button.visible
		and _pause.save_quit_button.visible,
		"the menu offers Resume, Settings, and Save and Quit on %s" % where
	)
	_check(_pause.resume_button.has_focus(), "Resume has focus on %s" % where)


func _save_and_quit(where: String) -> Dictionary:
	var before := {
		"offer": _flow.accepted_contract_offer().get("name", ""),
		"progress": _flow.map_progress(),
	}
	_pause.save_quit_button.pressed.emit()
	await process_frame
	_check(_showing(MAIN_MENU), "Save and Quit returns to the main menu from %s" % where)
	_check(not _pause.is_open() and not paused, "and the game is no longer paused")
	_check(FileAccess.file_exists(SAVE_PATH), "the run is saved to disk")
	var menu := _screen()
	_check(
		menu != null and not menu.continue_button.disabled,
		"Continue is available on the main menu"
	)
	return before


func _check_continue_resumes(before: Dictionary, where: String) -> void:
	var menu := _screen()
	menu.continue_button.pressed.emit()
	await process_frame
	_check(_showing(HUB), "Continue resumes on the map after quitting from %s" % where)
	_check(
		_flow.accepted_contract_offer().get("name", "") == before["offer"],
		"the same contract is resumed (%s)" % before["offer"]
	)
	_check(_flow.map_progress() == before["progress"], "with the same map progress")


func _run() -> void:
	await process_frame
	_flow = root.get_node("FlowCoordinator")
	_remove_save()
	_flow.use_save_file(SAVE_PATH)
	var main: Node = load("res://scenes/main.tscn").instantiate()
	root.add_child(main)
	await process_frame
	_pause = main.pause_menu

	print("Main menu")
	_check(_showing(MAIN_MENU), "the game opens on the main menu")
	_escape()
	_check(not _pause.is_open() and not paused, "Escape on the main menu does not pause")

	_flow.start_new_contract()
	if _flow.needs_contract_choice():
		_flow.accept_contract_offer(0)
	await process_frame
	_check(_showing(HUB), "a new contract opens the map")

	print("Pause and resume on the map")
	_check_pauses("the map")
	_check(not _pause.note_label.visible, "no restart warning on the map, a safe point")
	_escape()
	_check(not _pause.is_open() and not paused, "Escape again resumes")
	_check(_showing(HUB), "resuming leaves the map on screen")
	_check_pauses("the map")
	_pause.resume_button.pressed.emit()
	_check(not _pause.is_open() and not paused, "Resume resumes")

	print("Settings from the pause menu")
	_check_pauses("the map")
	_pause.settings_button.pressed.emit()
	var settings: Control = _pause.settings
	_check(
		settings != null and settings.scene_file_path == SETTINGS and settings.visible,
		"Settings opens over the paused game"
	)
	_check(_showing(HUB), "the map stays underneath")
	_escape()
	_check(_pause.settings == null, "Escape in settings goes back to the pause menu")
	_check(_pause.is_open() and paused, "still paused after leaving settings")
	_check(_pause.settings_button.has_focus(), "focus returns to Settings")
	_pause.settings_button.pressed.emit()
	_pause.settings.back_button.pressed.emit()
	_check(_pause.settings == null and _pause.is_open(), "Back in settings does the same")
	_check(_showing(HUB), "settings never left the run")
	_pause.resume()

	print("Save and Quit from the map")
	_check_pauses("the map")
	var before: Dictionary = await _save_and_quit("the map")
	await _check_continue_resumes(before, "the map")

	print("In combat")
	var entered: Dictionary = preload("res://tests/combat_fixture.gd").enter(_flow)
	_check(entered.get("ok", false), "the fixture enters a fight")
	if not entered.get("ok", false):
		_finish()
		return
	await process_frame
	var combat := _screen()
	combat.view_deck_button.pressed.emit()
	_escape()
	_check(not combat.deck_viewer.is_open(), "Escape closes the deck viewer first")
	_check(not _pause.is_open() and not paused, "without also pausing")
	_check_pauses("combat")
	_check(_pause.note_label.visible, "mid-fight the menu warns the fight restarts from the map")
	_pause.resume()
	_check(_screen() == combat and not paused, "resuming returns to the same fight")

	print("Save and Quit from combat")
	_check_pauses("combat")
	await _save_and_quit("combat")
	_check(_flow.run_snapshot().get("phase", "") == "hub", "the run behind the menu is not the fight")
	await _check_continue_resumes(before, "combat")

	print("New contract after quitting")
	_check_pauses("the map")
	await _save_and_quit("the map again")
	_screen().new_contract_button.pressed.emit()
	await process_frame
	_check(not _flow.contract_started(), "New contract starts a fresh run, not the quit one")

	_finish()
