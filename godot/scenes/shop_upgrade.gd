# Lets the player pay credits to upgrade a card in their deck. Reached from the
# shop; Back returns to the same shop visit. Each row shows a card as it is now
# and as it would become, with the price. Pressing its button upgrades one copy.
#
# The rules are in shop.rs (upgrade_card), and ShopNode is the bridge.
extends Control

const PRICE_COLOUR := Color(0.2, 1, 0.4, 1)
const UNAFFORDABLE_COLOUR := Color(1, 0.2, 0.333333, 1)

@onready var shop_node: Node = $ShopNode
@onready var credits_label: Label = %CreditsLabel
@onready var card_list: VBoxContainer = %StockList
@onready var status_label: Label = %StatusLabel
@onready var back_button: Button = %LeaveButton

var _upgrade_buttons: Array[Button] = []
# Guards against a double click leaving twice.
var _leaving := false


func _ready() -> void:
	back_button.text = "back to shop"
	back_button.pressed.connect(_on_back_pressed)
	_refresh()
	_focus_first_control()


# Rebuilds the list from the deck, since an upgrade can change how many rows
# there are.
func _refresh() -> void:
	credits_label.text = "$ %d  credits on hand" % PlayerStateGlobal.money()

	for child in card_list.get_children():
		card_list.remove_child(child)
		child.queue_free()
	_upgrade_buttons.clear()

	var cards: Array = shop_node.upgradable_cards()
	if cards.is_empty():
		var empty := Label.new()
		empty.text = "No card in your deck can be upgraded."
		card_list.add_child(empty)
		return

	for entry: Dictionary in cards:
		_add_row(entry)


func _add_row(entry: Dictionary) -> void:
	var card_id := String(entry.get("id", ""))
	var price := int(entry.get("price", 0))
	var affordable: bool = entry.get("affordable", false)
	var before: Dictionary = entry.get("card", {})
	var after: Dictionary = entry.get("upgraded", {})

	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 24)

	var details := Label.new()
	details.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	details.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	details.add_theme_font_size_override("font_size", 16)
	details.mouse_filter = Control.MOUSE_FILTER_PASS
	details.text = _upgrade_details_text(entry)
	details.tooltip_text = _keyword_explanations(after)
	row.add_child(details)

	var purchase := VBoxContainer.new()
	purchase.custom_minimum_size = Vector2(160, 0)
	purchase.alignment = BoxContainer.ALIGNMENT_CENTER
	row.add_child(purchase)

	var price_label := Label.new()
	price_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	price_label.text = "%d cr" % price
	price_label.add_theme_color_override(
		"font_color", PRICE_COLOUR if affordable else UNAFFORDABLE_COLOUR
	)
	purchase.add_child(price_label)

	var button := Button.new()
	button.theme_type_variation = &"TerminalMenuButton"
	button.add_theme_font_size_override("font_size", 22)
	button.alignment = HORIZONTAL_ALIGNMENT_RIGHT
	button.text = "[ upgrade ]"
	button.disabled = not affordable
	button.focus_mode = Control.FOCUS_NONE if button.disabled else Control.FOCUS_ALL
	button.pressed.connect(_on_upgrade_pressed.bind(card_id, String(before.get("name", "the card"))))
	purchase.add_child(button)

	card_list.add_child(row)
	_upgrade_buttons.append(button)


# Three lines like the shop's rows: the change in name, then cost and noise,
# then what it does. A line that did not change is shown once.
func _upgrade_details_text(entry: Dictionary) -> String:
	var before: Dictionary = entry.get("card", {})
	var after: Dictionary = entry.get("upgraded", {})
	var count := int(entry.get("count", 1))
	var copies := "  ×%d" % count if count > 1 else ""

	var heading := "%s%s  →  %s  —  %s · %s" % [
		String(before.get("name", "?")).to_upper(),
		copies,
		String(after.get("name", "?")).to_upper(),
		after.get("type", "?"),
		after.get("rarity", "?"),
	]

	var was_stats := "%s · %s" % [before.get("cost_text", ""), before.get("noise_text", "")]
	var now_stats := "%s · %s" % [after.get("cost_text", ""), after.get("noise_text", "")]
	var was_text := String(before.get("description", ""))
	var now_text := String(after.get("description", ""))

	return "\n".join([
		heading,
		now_stats if was_stats == now_stats else "%s  →  %s" % [was_stats, now_stats],
		now_text if was_text == now_text else "%s  →  %s" % [was_text, now_text],
	])


# Hovering a row explains each effect of the upgraded card.
func _keyword_explanations(card: Dictionary) -> String:
	var lines: Array[String] = []
	for keyword: Dictionary in card.get("keywords", []):
		lines.append("%s: %s" % [keyword.get("label", ""), keyword.get("explanation", "")])
	return "\n".join(lines)


func _focus_first_control() -> void:
	for button in _upgrade_buttons:
		if not button.disabled:
			button.grab_focus()
			return
	back_button.grab_focus()


func _on_upgrade_pressed(card_id: String, card_name: String) -> void:
	var result: Dictionary = shop_node.upgrade_card(card_id)
	if result.get("ok", false):
		status_label.text = "Upgraded %s for %d credits." % [
			card_name,
			int(result.get("price", 0)),
		]
		# An upgrade is a safe point, like a purchase.
		FlowCoordinator.save_progress()
	else:
		status_label.text = _refusal_text(String(result.get("error", "")), card_name)

	_refresh()
	_focus_first_control()


func _refusal_text(error: String, card_name: String) -> String:
	match error:
		"not_enough_credits":
			return "Not enough credits to upgrade %s." % card_name
		"not_upgradable":
			return "%s cannot be upgraded." % card_name
		"not_in_deck":
			return "%s is no longer in your deck." % card_name
		_:
			return "Could not upgrade %s: %s" % [card_name, error]


func _on_back_pressed() -> void:
	if _leaving:
		return
	_leaving = true

	var result: Dictionary = FlowCoordinator.return_to_shop()
	if not result.get("ok", false):
		_leaving = false
		status_label.text = "Could not go back to the shop: %s" % result.get("error", "unknown error")