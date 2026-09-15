# Temporary functional harness for exercising route selection. This is not a
# proposed contract-map UI, visual design, or final player experience.
extends Control

@onready var current_label: Label = $Page/VBox/MapPanel/MapMargin/MapContent/CurrentLabel
@onready var credits_label: Label = $Page/VBox/MapPanel/MapMargin/MapContent/CreditsLabel
@onready var progress_label: Label = $Page/VBox/MapPanel/MapMargin/MapContent/ProgressLabel
@onready var map_list: VBoxContainer = $Page/VBox/MapPanel/MapMargin/MapContent/MapList
@onready var instruction_label: Label = $Page/VBox/MapPanel/MapMargin/MapContent/InstructionLabel
@onready var choices: HBoxContainer = $Page/VBox/MapPanel/MapMargin/MapContent/Choices
@onready var key_label: Label = $Page/VBox/MapPanel/MapMargin/MapContent/KeyLabel
@onready var status_label: Label = $Page/VBox/MapPanel/MapMargin/MapContent/StatusLabel

# Colour per node status, so a completed encounter reads differently from one
# still ahead without having to read the marker.
const STATUS_COLOURS := {
	"completed": Color(0.55, 0.82, 0.6, 1),
	"current": Color(0.95, 0.95, 0.98, 1),
	"available": Color(0.55, 0.78, 0.92, 1),
	"locked": Color(0.55, 0.42, 0.45, 1),
	"upcoming": Color(0.5, 0.55, 0.6, 1),
}


func _ready() -> void:
	_build_key()
	_show_available_encounters()


func _show_available_encounters() -> void:
	_clear_choices()
	status_label.text = ""

	# Refreshed on every return to the map, so a change made by an event node
	# is visible as soon as the player lands back here.
	credits_label.text = "Credits: %d" % PlayerStateGlobal.money()

	_update_progress()
	_rebuild_map()

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
		button.custom_minimum_size = Vector2(220.0, 60.0)
		button.pressed.connect(_on_encounter_selected.bind(node_id))
		choices.add_child(button)


# "2 of 5 encounters complete", so progress through the contract is a number
# the player can see rather than something they have to remember.
func _update_progress() -> void:
	var progress: Dictionary = FlowCoordinator.contract_progress()
	progress_label.text = "Contract progress: %d of %d encounters complete" % [
		int(progress.get("completed", 0)),
		int(progress.get("total", 0)),
	]


# Draws every node on the contract with its type marker and its status, so the
# player can see what is done, where they are, and what is still ahead.
func _rebuild_map() -> void:
	for child: Node in map_list.get_children():
		child.queue_free()

	for node: Dictionary in FlowCoordinator.map_progress():
		var row := Label.new()
		# A cleared node is both completed and where the player stands, so the
		# "you are here" marker is appended rather than replacing the status.
		var here := "  [@] you are here" if bool(node.get("is_current", false)) else ""
		row.text = "%s  %s %s  (%s)%s" % [
			str(node.get("status_marker", "")),
			str(node.get("type_marker", "")),
			str(node.get("type_name", "")),
			str(node.get("status", "")),
			here,
		]
		row.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
		row.add_theme_font_size_override("font_size", 14)
		var status := str(node.get("status", ""))
		if STATUS_COLOURS.has(status):
			row.add_theme_color_override("font_color", STATUS_COLOURS[status])
		map_list.add_child(row)


# The key explaining every marker that can appear on the map above.
func _build_key() -> void:
	var key: Dictionary = FlowCoordinator.map_key()

	# Kept to two wrapped lines rather than one per marker, so the key does
	# not push the rest of the map off screen.
	var types: Array[String] = []
	for entry: Dictionary in key.get("encounter_types", []):
		types.append("%s %s" % [str(entry.get("marker", "")), str(entry.get("name", ""))])

	var statuses: Array[String] = []
	for entry: Dictionary in key.get("statuses", []):
		statuses.append("%s %s" % [
			str(entry.get("marker", "")),
			str(entry.get("description", "")).trim_suffix("."),
		])

	key_label.text = "Encounters:  %s\nStatus:  %s" % [
		"   ".join(types),
		"   ".join(statuses),
	]


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
