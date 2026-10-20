# Safehouse screen (story #112). The player lies low to cut their noise, or
# upgrades one card in their deck for free, but not both. Move on completes
# the node and returns to the map, whether or not the safehouse was used.
#
# The rules are in safehouse.rs, and SafehouseNode is the bridge.
extends Control

const CardRarity := preload("res://scenes/card_rarity.gd")

@onready var safehouse_node: Node = $SafehouseNode
@onready var noise_label: Label = %NoiseLabel
@onready var lie_low_label: Label = %LieLowLabel
@onready var lie_low_button: Button = %LieLowButton
@onready var upgrade_list: VBoxContainer = %UpgradeList
@onready var status_label: Label = %StatusLabel
@onready var leave_button: Button = %LeaveButton

var _upgrade_buttons: Array[Button] = []
# Guards against a double click leaving twice.
var _leaving := false


func _ready() -> void:
	lie_low_button.pressed.connect(_on_lie_low_pressed)
	leave_button.pressed.connect(_on_leave_pressed)
	_refresh()
	_focus_first_control()


# Rebuilds everything from SafehouseNode, so using either option greys out
# both straight away.
func _refresh() -> void:
	var status: Dictionary = safehouse_node.status()
	noise_label.text = "noise %d / %d" % [int(status.get("noise", 0)), int(status.get("max_noise", 0))]
	_show_lie_low(status)
	_show_upgrades(status)


func _show_lie_low(status: Dictionary) -> void:
	var noise := int(status.get("noise", 0))
	var removed := int(status.get("noise_removed", 0))
	var choice := String(status.get("choice", ""))
	var lines: Array[String] = [
		"LIE LOW — noise −%d" % int(status.get("reduction", 0)),
	]
	match choice:
		"lie_low":
			lines.append("You lay low here. The heat is off for now.")
		"upgrade":
			lines.append("No time left to lie low: you spent it on an upgrade.")
		_:
			if removed > 0:
				lines.append("Noise %d → %d" % [noise, noise - removed])
			else:
				lines.append("No noise to shake off right now.")
	lie_low_label.text = "\n".join(lines)
	lie_low_button.disabled = not status.get("can_lie_low", false)
	lie_low_button.focus_mode = Control.FOCUS_NONE if lie_low_button.disabled else Control.FOCUS_ALL


func _show_upgrades(status: Dictionary) -> void:
	for child in upgrade_list.get_children():
		upgrade_list.remove_child(child)
		child.queue_free()
	_upgrade_buttons.clear()

	var cards: Array = safehouse_node.upgradable_cards()
	if cards.is_empty():
		var empty := Label.new()
		empty.text = "No card in your deck can be upgraded."
		upgrade_list.add_child(empty)
		return

	var can_upgrade: bool = status.get("can_upgrade", false)
	for entry: Dictionary in cards:
		_add_upgrade_row(entry, can_upgrade)


func _add_upgrade_row(entry: Dictionary, can_upgrade: bool) -> void:
	var card_id := String(entry.get("id", ""))
	var before: Dictionary = entry.get("card", {})
	var after: Dictionary = entry.get("upgraded", {})

	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 24)

	var details := Label.new()
	details.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	details.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	details.add_theme_font_size_override("font_size", 16)
	details.mouse_filter = Control.MOUSE_FILTER_PASS
	# A bar down the left in the card's rarity colour (#109).
	details.add_theme_stylebox_override("normal", CardRarity.stripe_style(str(after.get("rarity", ""))))
	details.text = _upgrade_details_text(entry)
	row.add_child(details)

	var button := Button.new()
	button.theme_type_variation = &"TerminalMenuButton"
	button.add_theme_font_size_override("font_size", 22)
	button.custom_minimum_size = Vector2(160, 0)
	button.alignment = HORIZONTAL_ALIGNMENT_RIGHT
	button.text = "[ upgrade ]"
	button.disabled = not can_upgrade
	button.focus_mode = Control.FOCUS_NONE if button.disabled else Control.FOCUS_ALL
	button.pressed.connect(_on_upgrade_pressed.bind(card_id, String(before.get("name", "the card"))))
	row.add_child(button)

	upgrade_list.add_child(row)
	_upgrade_buttons.append(button)


# The change in name, then what the upgraded card does, as the shop's upgrade
# screen shows it.
func _upgrade_details_text(entry: Dictionary) -> String:
	var before: Dictionary = entry.get("card", {})
	var after: Dictionary = entry.get("upgraded", {})
	var count := int(entry.get("count", 1))
	var copies := "  ×%d" % count if count > 1 else ""
	return "\n".join([
		"%s%s  →  %s  —  %s · %s" % [
			String(before.get("name", "?")).to_upper(),
			copies,
			String(after.get("name", "?")).to_upper(),
			after.get("type", "?"),
			after.get("rarity", "?"),
		],
		"%s · %s" % [after.get("cost_text", ""), after.get("noise_text", "")],
		String(after.get("description", "")),
	])


func _focus_first_control() -> void:
	if not lie_low_button.disabled:
		lie_low_button.grab_focus()
		return
	for button in _upgrade_buttons:
		if not button.disabled:
			button.grab_focus()
			return
	leave_button.grab_focus()


func _on_lie_low_pressed() -> void:
	var result: Dictionary = safehouse_node.lie_low()
	if result.get("ok", false):
		status_label.text = "You went dark for a while. Noise −%d, now %d." % [
			int(result.get("noise_removed", 0)),
			int(result.get("noise", 0)),
		]
	else:
		status_label.text = _refusal_text(String(result.get("error", "")))
	_refresh()
	_focus_first_control()


func _on_upgrade_pressed(card_id: String, card_name: String) -> void:
	var result: Dictionary = safehouse_node.upgrade_card(card_id)
	if result.get("ok", false):
		status_label.text = "Upgraded %s for the rest of the run." % card_name
	else:
		status_label.text = _refusal_text(String(result.get("error", "")), card_name)
	_refresh()
	_focus_first_control()


func _refusal_text(error: String, card_name := "") -> String:
	match error:
		"already_used":
			return "You have already used this safehouse."
		"nothing_to_lie_low":
			return "There is no noise to shake off."
		"not_upgradable":
			return "%s cannot be upgraded." % card_name
		"not_in_deck":
			return "%s is no longer in your deck." % card_name
		_:
			return "That did not work: %s" % error


func _on_leave_pressed() -> void:
	if _leaving:
		return
	_leaving = true

	var result: Dictionary = FlowCoordinator.complete_active_encounter()
	if not result.get("ok", false):
		_leaving = false
		status_label.text = "Could not leave the safehouse: %s" % result.get("error", "unknown error")
