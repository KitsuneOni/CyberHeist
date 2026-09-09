extends Control

@onready var phase_label: Label = $VBoxContainer/PhaseLabel
@onready var status_label: Label = $VBoxContainer/StatusLabel


func _ready() -> void:
	_refresh_phase()


func _on_start_test_combat_pressed() -> void:
	var result: Dictionary = FlowCoordinator.start_encounter("combat-demo-1", "combat")
	if not result.get("ok", false):
		status_label.text = "Could not start combat: %s" % result.get("error", "unknown error")


func _refresh_phase() -> void:
	var snapshot: Dictionary = FlowCoordinator.run_snapshot()
	phase_label.text = "Phase: %s" % snapshot.get("phase", "unknown")
