# The end screen for being detected (Trello #54), the losing counterpart of
# the victory screen. It says plainly that the player was caught, which
# contract it happened on, what the run had achieved up to that point, and
# what getting caught cost.
#
# Being caught doesn't throw the run away: Back to the map returns to the hub
# with the node the player failed still on offer to retry.
extends Control

const RunSummaryText := preload("res://scenes/run_summary_text.gd")

@export var fine_amount: int = 50

@onready var contract_label: Label = %ContractLabel
@onready var summary_label: Label = %SummaryLabel
@onready var fine_label: Label = %FineLabel
@onready var upgrades_label: Label = %UpgradesLabel
@onready var continue_button: Button = %ContinueButton


func _ready() -> void:
	contract_label.text = RunSummaryText.contract_text(FlowCoordinator.accepted_contract_offer())
	summary_label.text = RunSummaryText.summary_text(FlowCoordinator.run_summary())

	# Charged once per capture by the coordinator, so a caught screen reopened
	# by Continue shows the same penalty without taking it again.
	var result: Dictionary = FlowCoordinator.apply_caught_penalty(fine_amount)
	fine_label.text = "Fined: $%d" % result["fine"]

	var lost = result["lost_upgrades"]
	if lost.is_empty():
		upgrades_label.text = "No upgrades lost."
	else:
		upgrades_label.text = "Lost upgrades: " + ", ".join(lost)

	continue_button.pressed.connect(_on_continue_pressed)
	continue_button.grab_focus()


func _on_continue_pressed() -> void:
	var result: Dictionary = FlowCoordinator.finish_caught()
	if not result.get("ok", false):
		push_error("Could not finish caught flow: %s" % result.get("error", "unknown error"))
