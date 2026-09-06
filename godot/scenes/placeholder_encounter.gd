extends Control

@onready var encounter_title: Label = $Page/EncounterPanel/Margin/Content/EncounterTitle
@onready var encounter_description: Label = $Page/EncounterPanel/Margin/Content/EncounterDescription
@onready var status_label: Label = $Page/EncounterPanel/Margin/Content/StatusLabel


func _ready() -> void:
	var snapshot: Dictionary = FlowCoordinator.run_snapshot()
	var active_encounter = snapshot.get("active_encounter")
	if typeof(active_encounter) != TYPE_DICTIONARY:
		status_label.text = "No encounter is active."
		return

	var encounter_type: String = active_encounter.get("type", "unknown")
	encounter_title.text = "%s encounter" % encounter_type.capitalize()
	encounter_description.text = _description_for(encounter_type)


func _on_complete_pressed() -> void:
	var result: Dictionary = FlowCoordinator.complete_active_encounter()
	if not result.get("ok", false):
		status_label.text = "Could not complete encounter: %s" % result.get("error", "unknown error")


func _description_for(encounter_type: String) -> String:
	match encounter_type:
		"event":
			return "An unexpected opportunity appears inside the target network."
		"shop":
			return "Spend credits on equipment before continuing the contract."
		"elite":
			return "A high-risk security encounter guards the end of the route."
		_:
			return "The selected encounter has loaded."
