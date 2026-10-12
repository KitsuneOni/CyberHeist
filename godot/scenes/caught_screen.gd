
extends Control

@export var fine_amount: int = 50

func _ready() -> void:
	# Charged once per capture by the coordinator, so a caught screen reopened
	# by Continue shows the same penalty without taking it again.
	var result: Dictionary = FlowCoordinator.apply_caught_penalty(fine_amount)
	$VBoxContainer/FineLabel.text = "Fined: $%d" % result["fine"]

	var lost = result["lost_upgrades"]
	if lost.is_empty():
		$VBoxContainer/UpgradesLabel.text = "No upgrades lost."
	else:
		$VBoxContainer/UpgradesLabel.text = "Lost upgrades: " + ", ".join(lost)

	$VBoxContainer/ContinueButton.pressed.connect(_on_continue_pressed)

func _on_continue_pressed() -> void:
	var result: Dictionary = FlowCoordinator.finish_caught()
	if not result.get("ok", false):
		push_error("Could not finish caught flow: %s" % result.get("error", "unknown error"))
