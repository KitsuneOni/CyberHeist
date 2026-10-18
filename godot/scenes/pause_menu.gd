# The pause menu (story #110). Escape on any screen of a run pauses the game
# and offers Resume, Settings, and Save and Quit; Escape again resumes.
#
# It lives beside the screen host for the whole session rather than on any one
# screen, and keeps running while the tree is paused. Main puts it before the
# screen host, so the screen's own Escape handling (closing the deck viewer,
# say) gets the key first and a pause never opens over it.
#
# Built in code like the deck viewer, so there is no scene file to keep in step.
extends CanvasLayer

signal opened
signal closed

const SELECTED_PREFIX := "> "
const IDLE_PREFIX := "  "
const SETTINGS_SCENE := "res://scenes/settings_menu.tscn"
const TERMINAL_THEME := "res://themes/terminal_theme.tres"
const GREEN := Color(0.2, 1, 0.4, 1)
const PANEL_BG := Color(0.0313726, 0.0588235, 0.0470588, 0.97)
# Mid-fight the save holds the map before it, so the player is told the fight
# will start again rather than finding out on Continue.
const FIGHT_RESTART_NOTE := "Quitting now restarts this encounter from the map when you continue."

var resume_button: Button
var settings_button: Button
var save_quit_button: Button
var note_label: Label
var status_label: Label
# The settings screen while it is shown over the menu, otherwise null.
var settings: Control

var _root: Control
var _labels := {}


func _init() -> void:
	name = "PauseMenu"
	# Above every screen, the deck viewer and the card tooltip included.
	layer = 50
	process_mode = Node.PROCESS_MODE_ALWAYS
	visible = false
	_build()


func is_open() -> bool:
	return visible


func open() -> void:
	if visible or not FlowCoordinator.can_pause():
		return
	note_label.text = "" if FlowCoordinator.is_at_safe_point() else FIGHT_RESTART_NOTE
	note_label.visible = not note_label.text.is_empty()
	status_label.text = ""
	visible = true
	get_tree().paused = true
	resume_button.grab_focus()
	opened.emit()


func resume() -> void:
	if not visible:
		return
	_close_settings()
	visible = false
	get_tree().paused = false
	closed.emit()


func _unhandled_input(event: InputEvent) -> void:
	if not event.is_action_pressed("ui_cancel"):
		return
	# Settings over the menu handles its own Escape as Back.
	if settings != null:
		return
	if visible:
		resume()
		get_viewport().set_input_as_handled()
	elif FlowCoordinator.can_pause():
		open()
		get_viewport().set_input_as_handled()


func _on_settings_pressed() -> void:
	if settings != null:
		return
	settings = load(SETTINGS_SCENE).instantiate()
	settings.embedded = true
	settings.closed.connect(_close_settings)
	_root.add_child(settings)


func _close_settings() -> void:
	if settings == null:
		return
	_root.remove_child(settings)
	settings.queue_free()
	settings = null
	if visible:
		settings_button.grab_focus()


func _on_save_quit_pressed() -> void:
	_close_settings()
	# Unpaused first: the main menu that replaces the screen has to run.
	get_tree().paused = false
	var result: Dictionary = FlowCoordinator.save_and_quit_to_menu()
	if not result.get("ok", false):
		get_tree().paused = true
		status_label.text = "Could not save and quit: %s" % result.get("error", "unknown error")
		return
	visible = false
	closed.emit()


func _build() -> void:
	_root = Control.new()
	_root.name = "Root"
	_root.theme = load(TERMINAL_THEME)
	_root.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	add_child(_root)

	var dim := ColorRect.new()
	dim.color = Color(0, 0, 0, 0.72)
	dim.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_root.add_child(dim)

	var centre := CenterContainer.new()
	centre.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_root.add_child(centre)

	var panel := PanelContainer.new()
	panel.custom_minimum_size = Vector2(420, 0)
	var frame := StyleBoxFlat.new()
	frame.bg_color = PANEL_BG
	frame.set_border_width_all(2)
	frame.border_color = GREEN
	frame.set_content_margin_all(28)
	panel.add_theme_stylebox_override("panel", frame)
	centre.add_child(panel)

	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 10)
	panel.add_child(column)

	var title := Label.new()
	title.text = "// SESSION PAUSED"
	title.add_theme_font_size_override("font_size", 28)
	column.add_child(title)

	var rule := HSeparator.new()
	rule.add_theme_constant_override("separation", 12)
	column.add_child(rule)

	resume_button = _menu_button(column, "ResumeButton", "Resume")
	resume_button.pressed.connect(resume)
	settings_button = _menu_button(column, "SettingsButton", "Settings")
	settings_button.pressed.connect(_on_settings_pressed)
	save_quit_button = _menu_button(column, "SaveQuitButton", "Save and Quit")
	save_quit_button.pressed.connect(_on_save_quit_pressed)

	note_label = Label.new()
	note_label.theme_type_variation = &"DimLabel"
	note_label.add_theme_font_size_override("font_size", 14)
	note_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	note_label.custom_minimum_size = Vector2(360, 0)
	column.add_child(note_label)

	status_label = Label.new()
	status_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	status_label.custom_minimum_size = Vector2(360, 0)
	column.add_child(status_label)


# A button styled like the main menu's: the selected one reads "> Label".
func _menu_button(parent: Node, button_name: String, text: String) -> Button:
	var button := Button.new()
	button.name = button_name
	button.theme_type_variation = &"TerminalMenuButton"
	button.add_theme_font_size_override("font_size", 26)
	button.alignment = HORIZONTAL_ALIGNMENT_LEFT
	_labels[button] = text
	button.focus_entered.connect(_render_label.bind(button))
	button.focus_exited.connect(_render_label.bind(button))
	button.mouse_entered.connect(button.grab_focus)
	parent.add_child(button)
	_render_label(button)
	return button


func _render_label(button: Button) -> void:
	var prefix := SELECTED_PREFIX if button.has_focus() else IDLE_PREFIX
	button.text = prefix + _labels[button]
