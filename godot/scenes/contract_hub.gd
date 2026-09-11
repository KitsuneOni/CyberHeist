# Temporary functional harness for exercising route selection. This is not a
# proposed contract-map UI, visual design, or final player experience.
extends Control

@onready var current_label: Label = $Page/VBox/MapPanel/MapMargin/MapContent/CurrentLabel
@onready var instruction_label: Label = $Page/VBox/MapPanel/MapMargin/MapContent/InstructionLabel
@onready var choices: HBoxContainer = $Page/VBox/MapPanel/MapMargin/MapContent/Choices
@onready var status_label: Label = $Page/VBox/MapPanel/MapMargin/MapContent/StatusLabel


func _ready() -> void:
	_show_available_encounters()


func _show_available_encounters() -> void:
	_clear_choices()
	status_label.text = ""

	var current: Dictionary = FlowCoordinator.current_contract_node()
	var current_type: String = current.get("type", "unknown")
	current_label.text = "Current position: %s" % current_type.capitalize()

	var available: Array = FlowCoordinator.selectable_encounters()
	if available.is_empty():
		instruction_label.text = "Contract complete — there are no encounters left on this route."
		return

	instruction_label.text = "Choose your next encounter. Unchosen branches will be locked for this contract."
	for encounter: Dictionary in available:
		var node_id := int(encounter.get("id", -1))
		var encounter_type := str(encounter.get("type", "unknown"))
		var button := Button.new()
		button.text = "%s\nNode %d" % [encounter_type.capitalize(), node_id]
		button.tooltip_text = "Start the %s encounter" % encounter_type
		button.custom_minimum_size = Vector2(220.0, 76.0)
		button.pressed.connect(_on_encounter_selected.bind(node_id))
		choices.add_child(button)


func _on_encounter_selected(node_id: int) -> void:
	_disable_choices()
	var result: Dictionary = FlowCoordinator.select_encounter(node_id)
	if not result.get("ok", false):
		status_label.text = "Could not start encounter: %s" % result.get("error", "unknown error")
		_enable_choices()


func _clear_choices() -> void:
	for child: Node in choices.get_children():
		child.queue_free()


func _disable_choices() -> void:
	for child: Node in choices.get_children():
		if child is Button:
			child.disabled = true


func _enable_choices() -> void:
	for child: Node in choices.get_children():
		if child is Button:
			child.disabled = false
