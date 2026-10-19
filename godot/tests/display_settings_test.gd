# Real scene/extension coverage for story #46: a settings menu where the
# player adjusts audio volume and display options to their preference.
# The volumes are covered by audio_test.gd; this covers the display options.
#
#   Scenario: Change the display options
#     Given I open Settings from the main menu
#     Then I can choose windowed or fullscreen, the window size, vsync and the
#       CRT effect
#     And each change applies straight away
#
#   Scenario: The window size only applies in a window
#     Given I have chosen fullscreen
#     Then the window size is greyed out and cannot be changed
#
#   Scenario: Turn the CRT effect off
#     When I turn the CRT effect off
#     Then the scanline overlay disappears from this screen and every screen
#       after it
#
#   Scenario: Display options are kept between sessions
#     Given I have changed the display options
#     When I launch the game again
#     Then they are as I left them, and my volumes are untouched
#
# Settings are written to a scratch file, so a developer's real settings are
# never touched. Headless there is no window to resize, so the test checks
# what is chosen, shown and saved rather than the window itself.
#
# Run: godot --headless --path godot -s res://tests/display_settings_test.gd
extends SceneTree

const TEST_SETTINGS_PATH := "user://display_settings_test.cfg"

var _display: Node
var _audio: Node
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


func _remove_scratch_settings() -> void:
	DirAccess.remove_absolute(ProjectSettings.globalize_path(TEST_SETTINGS_PATH))


func _finish() -> void:
	_remove_scratch_settings()
	if _failures.is_empty():
		print("display settings test: PASSED")
		quit(0)
	else:
		print("display settings test: FAILED (%d)" % _failures.size())
		for failure in _failures:
			print("  - %s" % failure)
		quit(1)


func _saved() -> ConfigFile:
	var config := ConfigFile.new()
	config.load(TEST_SETTINGS_PATH)
	return config


func _press_key(button: Button, action: String) -> void:
	var event := InputEventAction.new()
	event.action = action
	event.pressed = true
	button.gui_input.emit(event)


func _overlay(screen: Node) -> CanvasItem:
	return screen.get_node_or_null("CrtOverlay") as CanvasItem


func _run() -> void:
	_display = root.get_node_or_null("/root/DisplaySettings")
	_audio = root.get_node_or_null("/root/AudioDirector")
	_check(_display != null, "DisplaySettings autoload is present")
	if _display == null or _audio == null:
		_finish()
		return

	_remove_scratch_settings()
	_display.settings_path = TEST_SETTINGS_PATH
	_audio.settings_path = TEST_SETTINGS_PATH
	_display.load_settings()
	_audio.load_settings()

	print("Defaults")
	_check(not _display.is_fullscreen(), "a first launch is windowed")
	_check(_display.window_size() == Vector2i(1280, 720), "at 1280 x 720")
	_check(_display.is_vsync_on(), "with vsync on")
	_check(_display.is_crt_effect_on(), "and the CRT effect on")

	var main: Node = load("res://scenes/main.tscn").instantiate()
	root.add_child(main)
	await process_frame
	await process_frame

	var menu := _screen()
	if menu == null or menu.name != "MainMenu":
		_check(false, "the game opens on the main menu")
		_finish()
		return
	_check(_overlay(menu).visible, "the main menu shows its CRT overlay")
	menu.get_node("%SettingsButton").pressed.emit()
	await process_frame
	var settings := _screen()
	if settings == null or settings.name != "SettingsMenu":
		_check(false, "Settings opens the settings screen")
		_finish()
		return

	var mode_button: Button = settings.window_mode_button
	var size_button: Button = settings.window_size_button
	var vsync_button: Button = settings.vsync_button
	var crt_button: Button = settings.crt_button

	print("The settings screen shows the display options")
	_check(settings.display_choice(mode_button) == "Windowed", "display mode shows Windowed")
	_check(settings.display_choice(size_button) == "1280 x 720", "window size shows 1280 x 720")
	_check(settings.display_choice(vsync_button) == "On", "vsync shows On")
	_check(settings.display_choice(crt_button) == "On", "CRT effect shows On")
	_check(mode_button.text.contains("Windowed"), "the button itself reads %s" % mode_button.text)

	print("Window size")
	size_button.grab_focus()
	_press_key(size_button, "ui_right")
	_check(_display.window_size() == Vector2i(1600, 900), "right steps the size up to 1600 x 900")
	_check(size_button.text.contains("1600 x 900"), "and the button shows it")
	size_button.pressed.emit()
	_check(_display.window_size() == Vector2i(1920, 1080), "pressing steps on to 1920 x 1080")
	size_button.pressed.emit()
	_check(_display.window_size() == Vector2i(1280, 720), "and wraps round to 1280 x 720")
	_press_key(size_button, "ui_left")
	_check(_display.window_size() == Vector2i(1920, 1080), "left from the smallest wraps to the largest")
	_check(
		int(_saved().get_value("display", "window_size", -1)) == 2,
		"each change is saved at once"
	)

	print("Fullscreen")
	mode_button.pressed.emit()
	_check(_display.is_fullscreen(), "the display mode switches to fullscreen")
	_check(settings.display_choice(mode_button) == "Fullscreen", "and shows Fullscreen")
	_check(size_button.disabled, "the window size is greyed out while fullscreen")
	_check(settings.window_size_note.text == "fullscreen", "with a note saying why")
	settings.step_display_option(size_button, 1)
	_check(_display.window_size_index() == 2, "and cannot be changed while fullscreen")
	_check(bool(_saved().get_value("display", "fullscreen", false)), "fullscreen is saved")

	print("VSync")
	_press_key(vsync_button, "ui_right")
	_check(not _display.is_vsync_on(), "vsync turns off")
	_check(settings.display_choice(vsync_button) == "Off", "and shows Off")

	print("CRT effect")
	_check(_overlay(settings).visible, "the settings screen shows its CRT overlay")
	crt_button.pressed.emit()
	_check(not _display.is_crt_effect_on(), "the CRT effect turns off")
	_check(not _overlay(settings).visible, "the overlay disappears from the settings screen at once")
	settings.get_node("%BackButton").pressed.emit()
	await process_frame
	menu = _screen()
	_check(
		menu != null and menu.name == "MainMenu" and not _overlay(menu).visible,
		"screens opened afterwards have no overlay either"
	)

	print("Kept between sessions")
	_audio.set_volume("music", 35)
	_check(_audio.save_settings() == OK, "the volumes save into the same file")
	var saved := _saved()
	_check(
		saved.get_value("display", "fullscreen", false) == true
		and int(saved.get_value("display", "window_size", -1)) == 2
		and saved.get_value("display", "vsync", true) == false
		and saved.get_value("display", "crt_effect", true) == false,
		"saving the volumes keeps the display options"
	)
	_check(_display.save_settings() == OK, "the display options save again")
	_check(
		int(_saved().get_value("audio", "music", -1)) == 35,
		"saving the display options keeps the volumes"
	)

	# Change everything live without saving, then load as the next launch would.
	_display.set_fullscreen(false)
	_display.set_window_size_index(0)
	_display.set_vsync(true)
	_display.set_crt_effect(true)
	_display.load_settings()
	_check(_display.is_fullscreen(), "loading restores fullscreen")
	_check(_display.window_size_index() == 2, "loading restores the window size")
	_check(not _display.is_vsync_on(), "loading restores vsync")
	_check(not _display.is_crt_effect_on(), "loading restores the CRT effect")
	_check(not _overlay(menu).visible, "and applies it to the screen showing")

	print("Damaged settings")
	var damaged := ConfigFile.new()
	damaged.set_value("display", "fullscreen", "yes please")
	damaged.set_value("display", "window_size", 99)
	damaged.save(TEST_SETTINGS_PATH)
	_display.load_settings()
	_check(not _display.is_fullscreen(), "a value of the wrong type falls back to its default")
	_check(_display.window_size_index() == 0, "an out-of-range size falls back to the default")
	_check(_display.is_crt_effect_on(), "a missing value falls back to its default")

	var unreadable := FileAccess.open(TEST_SETTINGS_PATH, FileAccess.WRITE)
	unreadable.store_string("[display\nfullscreen = tru")
	unreadable.close()
	_check(_display.save_settings() != OK, "an unreadable settings file is not overwritten")
	_check(
		FileAccess.get_file_as_string(TEST_SETTINGS_PATH).begins_with("[display\n"),
		"and is left as it was"
	)

	_display.settings_path = _display.SETTINGS_PATH
	_audio.settings_path = _audio.SETTINGS_PATH
	_finish()
