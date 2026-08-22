extends Control


@onready var controller: Node = $ContractMapController
@onready var map_panel: PanelContainer = $Page/VBox/MapPanel
@onready var current_label: Label = $Page/VBox/MapPanel/MapMargin/MapContent/CurrentLabel
@onready var instruction_label: Label = $Page/VBox/MapPanel/MapMargin/MapContent/InstructionLabel
@onready var choices: HBoxContainer = $Page/VBox/MapPanel/MapMargin/MapContent/Choices
@onready var feedback_label: Label = $Page/VBox/MapPanel/MapMargin/MapContent/FeedbackLabel
@onready var encounter_panel: PanelContainer = $Page/VBox/EncounterPanel
@onready var encounter_title: Label = $Page/VBox/EncounterPanel/EncounterMargin/EncounterContent/EncounterTitle
@onready var encounter_description: Label = $Page/VBox/EncounterPanel/EncounterMargin/EncounterContent/EncounterDescription
@onready var complete_button: Button = $Page/VBox/EncounterPanel/EncounterMargin/EncounterContent/CompleteButton
@onready var reset_button: Button = $Page/VBox/ResetButton


func _ready() -> void:
	complete_button.pressed.connect(_on_complete_pressed)
	reset_button.pressed.connect(_on_reset_pressed)
	_show_map()


func _show_map() -> void:
	map_panel.show()
	encounter_panel.hide()
	feedback_label.text = ""
	_clear_choices()

	var current: Dictionary = controller.current_encounter()
	current_label.text = "Current position: %s" % str(current.get("encounter_type", "Unknown"))

	var available: Array = controller.selectable_encounters()
	if available.is_empty():
		instruction_label.text = "Contract complete — there are no encounters left on this route."
		return

	instruction_label.text = "Choose your next encounter. This choice permanently locks the other branch."
	for encounter: Dictionary in available:
		var node_id := int(encounter.get("id", -1))
		var encounter_type := str(encounter.get("encounter_type", "Unknown"))
		var button := Button.new()
		button.text = "%s\nnode %d" % [encounter_type, node_id]
		button.tooltip_text = "Start the %s encounter" % encounter_type.to_lower()
		button.custom_minimum_size = Vector2(220.0, 76.0)
		button.pressed.connect(_on_encounter_selected.bind(node_id))
		choices.add_child(button)


func _on_encounter_selected(node_id: int) -> void:
	var result: Dictionary = controller.select_encounter(node_id)
	if not bool(result.get("ok", false)):
		feedback_label.text = str(result.get("error", "That encounter is locked."))
		return

	var encounter_type := str(result.get("encounter_type", "Unknown"))
	encounter_title.text = "%s encounter loaded" % encounter_type
	encounter_description.text = "%s\n\nOther branches from the previous node are now unavailable." % _description_for(encounter_type)
	map_panel.hide()
	encounter_panel.show()


func _on_complete_pressed() -> void:
	var result: Dictionary = controller.complete_current_encounter()
	if not bool(result.get("ok", false)):
		feedback_label.text = str(result.get("error", "The encounter could not be completed."))
		return

	_show_map()


func _on_reset_pressed() -> void:
	controller.reset_demo_contract()
	_show_map()


func _clear_choices() -> void:
	for child: Node in choices.get_children():
		child.queue_free()


func _description_for(encounter_type: String) -> String:
	match encounter_type:
		"Combat":
			return "Security systems engage. A future combat scene can replace this placeholder."
		"Event":
			return "An unexpected opportunity appears inside the target network."
		"Shop":
			return "Spend credits on equipment before continuing the contract."
		"Elite":
			return "A high-risk security encounter guards the end of the route."
		_:
			return "The selected encounter has started."
