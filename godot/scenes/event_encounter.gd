# Event encounter screen. Presents a random event with at least two choices,
# each stating its outcome or risk up front, then applies the chosen result to
# the run before handing control back to the contract map.
#
# The event catalogue and resolution rules live in ../../cyber_heist/src/events.rs;
# EventNode (events_node.rs) is the bridge.
extends Control

@onready var event_node: Node = $EventNode
@onready var title_label: Label = $Page/VBox/TitleLabel
@onready var description_label: Label = $Page/VBox/DescriptionLabel
@onready var choices: VBoxContainer = $Page/VBox/Choices
@onready var outcome_label: Label = $Page/VBox/OutcomeLabel
@onready var continue_button: Button = $Page/VBox/ContinueButton


func _ready() -> void:
	continue_button.visible = false
	continue_button.pressed.connect(_on_continue_pressed)
	_present_event()


func _present_event() -> void:
	var event: Dictionary = event_node.roll_event()
	if not event.get("ok", false):
		outcome_label.text = "Could not load an event: %s" % event.get("error", "unknown error")
		_show_continue()
		return

	title_label.text = str(event.get("title", "Event"))
	description_label.text = str(event.get("description", ""))
	outcome_label.text = ""

	var choice_list: Array = event.get("choices", [])
	for i in choice_list.size():
		var choice: Dictionary = choice_list[i]
		var button := Button.new()
		# The preview is on the button itself so the risk is visible before
		# committing, not hidden behind a hover.
		button.text = "%s\n%s" % [choice.get("label", "?"), choice.get("preview", "")]
		button.custom_minimum_size = Vector2(520.0, 64.0)
		button.pressed.connect(_on_choice_pressed.bind(i))
		choices.add_child(button)


func _on_choice_pressed(index: int) -> void:
	var result: Dictionary = event_node.choose(index)
	if not result.get("ok", false):
		outcome_label.text = "Could not resolve that choice: %s" % result.get("error", "unknown error")
		return

	# Committed: lock the options so an outcome cannot be taken twice.
	for child in choices.get_children():
		if child is Button:
			child.disabled = true

	var lines: Array[String] = [str(result.get("summary", ""))]
	var upgrade := str(result.get("upgrade", ""))
	if upgrade != "":
		lines.append("Upgrade gained: %s" % upgrade)
	lines.append(
		"Credits %+d — balance now %d"
		% [int(result.get("credits", 0)), int(result.get("credits_total", 0))]
	)
	outcome_label.text = "\n".join(lines)
	_show_continue()


func _show_continue() -> void:
	continue_button.visible = true
	continue_button.grab_focus()


func _on_continue_pressed() -> void:
	var result: Dictionary = FlowCoordinator.complete_active_encounter()
	if not result.get("ok", false):
		push_error("Could not complete event encounter: %s" % result.get("error", "unknown error"))
