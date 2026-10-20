# The settings screen, reached from the main menu or the pause menu. It holds
# the three volume sliders from story #111 and, below them, the display options
# from story #46: windowed or fullscreen, the window size, vsync and the CRT
# scanline effect.
#
# A slider applies its volume the moment it moves, so the player hears the
# change straight away. Saving rewrites the settings file, so it waits until
# the sliders have been still for SAVE_DELAY_SECONDS rather than happening on
# every step of a drag, and leaving the screen saves anything still waiting.
# A display option is a row whose button cycles through its choices: pressing
# it moves to the next, and left and right step either way while it is
# selected. Each change applies and saves at once, since there is no drag to
# wait out. The window size only matters in a window, so it is greyed out
# while fullscreen.
#
# Navigation goes through FlowCoordinator like every other screen.
extends Control

# Sent when Back is pressed while shown over the pause menu, which closes it.
signal closed

const SELECTED_PREFIX := "> "
const IDLE_PREFIX := "  "
const BACK_LABEL := "Back"
const SAVE_DELAY_SECONDS := 0.4

# Set by the pause menu, which shows settings over a paused run (story #110).
# Back then closes this overlay instead of going to the main menu.
var embedded := false

@onready var master_slider: HSlider = %MasterSlider
@onready var music_slider: HSlider = %MusicSlider
@onready var sfx_slider: HSlider = %SfxSlider
@onready var master_value: Label = %MasterValue
@onready var music_value: Label = %MusicValue
@onready var sfx_value: Label = %SfxValue
@onready var window_mode_button: Button = %WindowModeButton
@onready var window_size_button: Button = %WindowSizeButton
@onready var vsync_button: Button = %VsyncButton
@onready var crt_button: Button = %CrtButton
@onready var window_size_note: Label = %WindowSizeNote
@onready var back_button: Button = %BackButton
@onready var status_label: Label = %StatusLabel

# Each slider's volume setting and the label showing its value.
var _rows := {}
# Restarted by every slider change; saves once it runs out.
var _save_timer: Timer
# Each display option's button and how to read, change and name its choices.
var _display_rows := {}


func _ready() -> void:
	_rows = {
		master_slider: {"setting": "master", "value_label": master_value},
		music_slider: {"setting": "music", "value_label": music_value},
		sfx_slider: {"setting": "sfx", "value_label": sfx_value},
	}

	for slider: HSlider in _rows:
		var setting: String = _rows[slider]["setting"]
		# Set before connecting, so showing the saved value does not count as
		# the player changing it.
		slider.value = AudioDirector.volume(setting)
		_show_value(slider)
		slider.value_changed.connect(_on_slider_changed.bind(slider))
		# Hovering selects a slider, as it does a menu button.
		slider.mouse_entered.connect(slider.grab_focus)

	_save_timer = Timer.new()
	_save_timer.one_shot = true
	_save_timer.wait_time = SAVE_DELAY_SECONDS
	_save_timer.timeout.connect(_save_settings)
	add_child(_save_timer)

	_display_rows = {
		window_mode_button: {
			"choices": ["Windowed", "Fullscreen"],
			"current": func() -> int: return 1 if DisplaySettings.is_fullscreen() else 0,
			"apply": func(choice: int) -> void: DisplaySettings.set_fullscreen(choice == 1),
		},
		window_size_button: {
			"choices": DisplaySettings.WINDOW_SIZES.map(DisplaySettings.size_text),
			"current": DisplaySettings.window_size_index,
			"apply": DisplaySettings.set_window_size_index,
		},
		vsync_button: {
			"choices": ["On", "Off"],
			"current": func() -> int: return 0 if DisplaySettings.is_vsync_on() else 1,
			"apply": func(choice: int) -> void: DisplaySettings.set_vsync(choice == 0),
		},
		crt_button: {
			"choices": ["On", "Off"],
			"current": func() -> int: return 0 if DisplaySettings.is_crt_effect_on() else 1,
			"apply": func(choice: int) -> void: DisplaySettings.set_crt_effect(choice == 0),
		},
	}
	for button: Button in _display_rows:
		button.pressed.connect(step_display_option.bind(button, 1))
		button.gui_input.connect(_on_display_option_input.bind(button))
		button.focus_entered.connect(_render_display_options)
		button.focus_exited.connect(_render_display_options)
		button.mouse_entered.connect(_focus_if_enabled.bind(button))
	_render_display_options()

	back_button.focus_entered.connect(_render_back_label)
	back_button.focus_exited.connect(_render_back_label)
	back_button.mouse_entered.connect(back_button.grab_focus)
	back_button.pressed.connect(_on_back_pressed)
	_render_back_label()

	master_slider.grab_focus()


func _exit_tree() -> void:
	if not _save_timer.is_stopped():
		_save_timer.stop()
		_save_settings()


func _unhandled_input(event: InputEvent) -> void:
	if event.is_action_pressed("ui_cancel"):
		get_viewport().set_input_as_handled()
		_on_back_pressed()


func _on_slider_changed(value: float, slider: HSlider) -> void:
	var setting: String = _rows[slider]["setting"]
	AudioDirector.set_volume(setting, int(value))
	_show_value(slider)
	_save_timer.start()


# Whether the last change has been written to the settings file yet.
func has_unsaved_changes() -> bool:
	return not _save_timer.is_stopped()


func _save_settings() -> void:
	if AudioDirector.save_settings() == OK:
		status_label.text = ""
	else:
		status_label.text = "Could not save settings. The new volume lasts until you quit."


func _show_value(slider: HSlider) -> void:
	var setting: String = _rows[slider]["setting"]
	var percent := AudioDirector.volume(setting)
	var label: Label = _rows[slider]["value_label"]
	label.text = "muted" if percent == 0 else "%d%%" % percent


# Moves a display option `step` choices along, wrapping at either end, then
# applies and saves it.
func step_display_option(button: Button, step: int) -> void:
	if button.disabled:
		return
	var row: Dictionary = _display_rows[button]
	var count: int = row["choices"].size()
	var choice: int = posmod(int(row["current"].call()) + step, count)
	row["apply"].call(choice)
	_render_display_options()
	if DisplaySettings.save_settings() == OK:
		status_label.text = ""
	else:
		status_label.text = "Could not save settings. The change lasts until you quit."


# The choice a display option's button shows, e.g. "Fullscreen".
func display_choice(button: Button) -> String:
	var row: Dictionary = _display_rows[button]
	return str(row["choices"][int(row["current"].call())])


func _on_display_option_input(event: InputEvent, button: Button) -> void:
	if event.is_action_pressed("ui_left"):
		button.accept_event()
		step_display_option(button, -1)
	elif event.is_action_pressed("ui_right"):
		button.accept_event()
		step_display_option(button, 1)


func _render_display_options() -> void:
	# A fullscreen window takes the screen's own size, so the window size
	# would change nothing until fullscreen is turned off.
	var fullscreen := DisplaySettings.is_fullscreen()
	window_size_button.disabled = fullscreen
	window_size_button.focus_mode = Control.FOCUS_NONE if fullscreen else Control.FOCUS_ALL
	window_size_note.text = "fullscreen" if fullscreen else ""

	for button: Button in _display_rows:
		var prefix := SELECTED_PREFIX if button.has_focus() else IDLE_PREFIX
		button.text = "%s<  %s  >" % [prefix, display_choice(button)]


func _focus_if_enabled(button: Button) -> void:
	if not button.disabled:
		button.grab_focus()


func _render_back_label() -> void:
	var prefix := SELECTED_PREFIX if back_button.has_focus() else IDLE_PREFIX
	back_button.text = prefix + BACK_LABEL


func _on_back_pressed() -> void:
	if embedded:
		closed.emit()
		return
	var result: Dictionary = FlowCoordinator.close_settings()
	if not result.get("ok", false):
		status_label.text = "Could not leave settings: %s" % result.get("error", "unknown error")
