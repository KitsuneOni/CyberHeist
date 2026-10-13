# The settings screen, reached from the main menu. For now it holds the three
# volume sliders story #111 needs; display options belong to the settings menu
# story and can be added below them.
#
# A slider applies its volume the moment it moves, so the player hears the
# change straight away. Saving rewrites the settings file, so it waits until
# the sliders have been still for SAVE_DELAY_SECONDS rather than happening on
# every step of a drag, and leaving the screen saves anything still waiting.
# Navigation goes through FlowCoordinator like every other screen.
extends Control

const SELECTED_PREFIX := "> "
const IDLE_PREFIX := "  "
const BACK_LABEL := "Back"
const SAVE_DELAY_SECONDS := 0.4

@onready var master_slider: HSlider = %MasterSlider
@onready var music_slider: HSlider = %MusicSlider
@onready var sfx_slider: HSlider = %SfxSlider
@onready var master_value: Label = %MasterValue
@onready var music_value: Label = %MusicValue
@onready var sfx_value: Label = %SfxValue
@onready var back_button: Button = %BackButton
@onready var status_label: Label = %StatusLabel

# Each slider's volume setting and the label showing its value.
var _rows := {}
# Restarted by every slider change; saves once it runs out.
var _save_timer: Timer


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


func _render_back_label() -> void:
	var prefix := SELECTED_PREFIX if back_button.has_focus() else IDLE_PREFIX
	back_button.text = prefix + BACK_LABEL


func _on_back_pressed() -> void:
	var result: Dictionary = FlowCoordinator.close_settings()
	if not result.get("ok", false):
		status_label.text = "Could not leave settings: %s" % result.get("error", "unknown error")
