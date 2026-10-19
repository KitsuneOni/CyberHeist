# The display options on the settings screen (story #46): windowed or
# fullscreen, the window's size, vsync, and the CRT scanline overlay most
# screens draw over themselves.
#
# The options live in the [display] section of the same settings file the
# volumes use (AudioDirector keeps [audio]); each side only ever writes its own
# section, so saving one never loses the other. They are applied as soon as this
# autoload is ready, so they carry over between sessions.
#
# Without a real window (the headless driver the tests run under) the options
# are still stored, read back and saved exactly as usual, but nothing is asked
# of the window, which has nothing to change.
#
# The CRT overlay is a ColorRect named CrtOverlay on each screen that has one.
# Rather than every screen reading the option, this hides or shows each one as
# it enters the tree, and every one already in the tree when the option
# changes.
extends Node

const SETTINGS_PATH := "user://settings.cfg"
const DISPLAY_SECTION := "display"
const CRT_OVERLAY_NAME := "CrtOverlay"

# The window sizes on offer, smallest first. The game is laid out for
# 1280x720 and scales up from it, so every size is 16:9.
const WINDOW_SIZES: Array[Vector2i] = [
	Vector2i(1280, 720),
	Vector2i(1600, 900),
	Vector2i(1920, 1080),
]

const DEFAULTS := {
	"fullscreen": false,
	"window_size": 0,
	"vsync": true,
	"crt_effect": true,
}

# Where settings are read from and written to. Only a test points this
# anywhere else, so it never overwrites the player's real settings.
var settings_path := SETTINGS_PATH

var _has_window := DisplayServer.get_name() != "headless"
var _options := DEFAULTS.duplicate()


func _ready() -> void:
	get_tree().node_added.connect(_on_node_added)
	load_settings()


func is_fullscreen() -> bool:
	return bool(_options["fullscreen"])


# Which of WINDOW_SIZES the window uses when it is not fullscreen.
func window_size_index() -> int:
	return int(_options["window_size"])


func window_size() -> Vector2i:
	return WINDOW_SIZES[window_size_index()]


func is_vsync_on() -> bool:
	return bool(_options["vsync"])


func is_crt_effect_on() -> bool:
	return bool(_options["crt_effect"])


# Whether there is a real window for the options to change. False headless.
func has_window() -> bool:
	return _has_window


func set_fullscreen(on: bool) -> void:
	_options["fullscreen"] = on
	_apply_window()


# Picks one of WINDOW_SIZES by index, clamped into range. It takes effect on
# the window straight away when windowed, and is kept for when fullscreen is
# turned off otherwise.
func set_window_size_index(index: int) -> void:
	_options["window_size"] = clampi(index, 0, WINDOW_SIZES.size() - 1)
	_apply_window()


func set_vsync(on: bool) -> void:
	_options["vsync"] = on
	_apply_vsync()


func set_crt_effect(on: bool) -> void:
	_options["crt_effect"] = on
	for overlay in get_tree().root.find_children(CRT_OVERLAY_NAME, "CanvasItem", true, false):
		overlay.visible = on


# "1600 x 900", as the settings screen shows a window size.
static func size_text(size: Vector2i) -> String:
	return "%d x %d" % [size.x, size.y]


# Writes the display options to the settings file, keeping every other
# section, such as the volumes, as it is. A file that exists but cannot be
# read is left alone rather than replaced by one holding only these options
# (AudioDirector copies such a file aside the next time it saves), and the
# error is returned so the screen can say the change was not kept.
func save_settings() -> Error:
	var config := ConfigFile.new()
	if FileAccess.file_exists(settings_path):
		var loaded := config.load(settings_path)
		if loaded != OK:
			push_error(
				"Settings at %s could not be read (error %d), so they were not overwritten."
				% [settings_path, loaded]
			)
			return loaded

	for option: String in DEFAULTS:
		config.set_value(DISPLAY_SECTION, option, _options[option])

	var result := config.save(settings_path)
	if result != OK:
		push_error("Could not save settings to %s (error %d)." % [settings_path, result])
	return result


# Reads the display options from the settings file and applies them. Anything
# missing, of the wrong type or out of range falls back to its default, so a
# first launch or a damaged file still opens a sensible window.
func load_settings() -> void:
	var config := ConfigFile.new()
	var loaded := config.load(settings_path) == OK

	for option: String in DEFAULTS:
		var fallback = DEFAULTS[option]
		var stored = config.get_value(DISPLAY_SECTION, option, fallback) if loaded else fallback
		if typeof(stored) != typeof(fallback):
			stored = fallback
		_options[option] = stored
	if window_size_index() < 0 or window_size_index() >= WINDOW_SIZES.size():
		_options["window_size"] = DEFAULTS["window_size"]

	_apply_window()
	_apply_vsync()
	set_crt_effect(is_crt_effect_on())


func _apply_window() -> void:
	if not _has_window:
		return
	if is_fullscreen():
		DisplayServer.window_set_mode(DisplayServer.WINDOW_MODE_FULLSCREEN)
		return

	DisplayServer.window_set_mode(DisplayServer.WINDOW_MODE_WINDOWED)
	var size := window_size()
	DisplayServer.window_set_size(size)
	# Centred on the screen the window is on, so a bigger size never pushes it
	# off the edge.
	var screen := DisplayServer.window_get_current_screen()
	var screen_origin := DisplayServer.screen_get_position(screen)
	var screen_size := DisplayServer.screen_get_size(screen)
	DisplayServer.window_set_position(screen_origin + (screen_size - size) / 2)


func _apply_vsync() -> void:
	if not _has_window:
		return
	DisplayServer.window_set_vsync_mode(
		DisplayServer.VSYNC_ENABLED if is_vsync_on() else DisplayServer.VSYNC_DISABLED
	)


func _on_node_added(node: Node) -> void:
	if node.name == CRT_OVERLAY_NAME and node is CanvasItem:
		node.visible = is_crt_effect_on()
