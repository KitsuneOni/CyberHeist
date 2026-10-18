# Lets the player pay once per shop visit to remove a card from their deck for
# good. Reached from the shop; Back returns to the same shop visit. Removing
# takes two presses (the first arms the row, the second confirms) because
# nothing replaces the card and it cannot be undone.
#
# The rules are in shop.rs (remove_card), and ShopNode is the bridge.
extends Control

const PRICE_COLOUR := Color(0.2, 1, 0.4, 1)
const UNAFFORDABLE_COLOUR := Color(1, 0.2, 0.333333, 1)

@onready var shop_node: Node = $ShopNode
@onready var credits_label: Label = %CreditsLabel
@onready var card_list: VBoxContainer = %StockList
@onready var status_label: Label = %StatusLabel
@onready var back_button: Button = %LeaveButton

var _remove_buttons: Dictionary[String, Button] = {}
# The card whose row is waiting for a second press, or "" if none.
var _armed_id := ""
var _price := 0
# Guards against a double click leaving twice.
var _leaving := false


func _ready() -> void:
	back_button.pressed.connect(_on_back_pressed)
	_refresh()
	if shop_node.removal_offer().get("sold", false):
		status_label.text = "You have already removed a card in this shop."
	_focus_first_control()


# Rebuilds the list from the deck, since a removal changes it.
func _refresh() -> void:
	credits_label.text = "$ %d  credits on hand" % PlayerStateGlobal.money()

	for child in card_list.get_children():
		card_list.remove_child(child)
		child.queue_free()
	_remove_buttons.clear()
	_armed_id = ""

	var offer: Dictionary = shop_node.removal_offer()
	_price = int(offer.get("price", 0))

	var cards: Array = shop_node.removable_cards()
	if cards.is_empty():
		var empty := Label.new()
		empty.text = "Your deck is empty."
		card_list.add_child(empty)
		return

	for entry: Dictionary in cards:
		_add_row(entry, offer)


func _add_row(entry: Dictionary, offer: Dictionary) -> void:
	var card_id := String(entry.get("id", ""))
	var card: Dictionary = entry.get("card", {})
	var can_remove: bool = offer.get("affordable", false)

	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 24)

	var details := Label.new()
	details.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	details.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	details.add_theme_font_size_override("font_size", 16)
	details.mouse_filter = Control.MOUSE_FILTER_PASS
	details.text = _details_text(entry)
	details.tooltip_text = _keyword_explanations(card)
	row.add_child(details)

	var purchase := VBoxContainer.new()
	purchase.custom_minimum_size = Vector2(160, 0)
	purchase.alignment = BoxContainer.ALIGNMENT_CENTER
	row.add_child(purchase)

	var price_label := Label.new()
	price_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	if offer.get("sold", false):
		price_label.text = "USED"
	else:
		price_label.text = "%d cr" % _price
		price_label.add_theme_color_override(
			"font_color", PRICE_COLOUR if can_remove else UNAFFORDABLE_COLOUR
		)
	purchase.add_child(price_label)

	var button := Button.new()
	button.theme_type_variation = &"TerminalMenuButton"
	button.add_theme_font_size_override("font_size", 22)
	button.alignment = HORIZONTAL_ALIGNMENT_RIGHT
	button.text = "[ remove ]"
	button.disabled = not can_remove
	button.focus_mode = Control.FOCUS_NONE if button.disabled else Control.FOCUS_ALL
	button.pressed.connect(_on_remove_pressed.bind(card_id, String(card.get("name", "the card"))))
	purchase.add_child(button)

	card_list.add_child(row)
	_remove_buttons[card_id] = button


# Three lines like the shop's rows: name with how many copies are owned, then
# cost and noise, then what it does.
func _details_text(entry: Dictionary) -> String:
	var card: Dictionary = entry.get("card", {})
	var count := int(entry.get("count", 1))
	var copies := "  ×%d" % count if count > 1 else ""

	var heading := "%s%s  —  %s · %s" % [
		String(card.get("name", "?")).to_upper(),
		copies,
		card.get("type", "?"),
		card.get("rarity", "?"),
	]
	var stats := "%s · %s" % [card.get("cost_text", ""), card.get("noise_text", "")]
	return "\n".join([heading, stats, String(card.get("description", ""))])


func _keyword_explanations(card: Dictionary) -> String:
	var lines: Array[String] = []
	for keyword: Dictionary in card.get("keywords", []):
		lines.append("%s: %s" % [keyword.get("label", ""), keyword.get("explanation", "")])
	return "\n".join(lines)


func _focus_first_control() -> void:
	for button in _remove_buttons.values():
		if not button.disabled:
			button.grab_focus()
			return
	back_button.grab_focus()


func _on_remove_pressed(card_id: String, card_name: String) -> void:
	# First press arms this row; pressing a different row moves the arming.
	if _armed_id != card_id:
		_armed_id = card_id
		for id in _remove_buttons:
			_remove_buttons[id].text = "[ remove ]"
		_remove_buttons[card_id].text = "[ confirm? ]"
		status_label.text = "Press again to remove %s for %d credits. It will not come back." % [
			card_name,
			_price,
		]
		return

	var result: Dictionary = shop_node.remove_card(card_id)
	if result.get("ok", false):
		status_label.text = "Removed %s from your deck for %d credits." % [
			card_name,
			int(result.get("price", 0)),
		]
		# A removal is a safe point, like a purchase.
		FlowCoordinator.save_progress()
	else:
		status_label.text = _refusal_text(String(result.get("error", "")), card_name)

	_refresh()
	_focus_first_control()


func _refusal_text(error: String, card_name: String) -> String:
	match error:
		"not_enough_credits":
			return "Not enough credits to remove %s." % card_name
		"already_sold":
			return "You have already removed a card in this shop."
		"deck_too_small":
			return "That is the last card in your deck. It cannot be removed."
		"not_in_deck":
			return "%s is no longer in your deck." % card_name
		_:
			return "Could not remove %s: %s" % [card_name, error]


func _on_back_pressed() -> void:
	if _leaving:
		return
	_leaving = true

	var result: Dictionary = FlowCoordinator.return_to_shop()
	if not result.get("ok", false):
		_leaving = false
		status_label.text = "Could not go back to the shop: %s" % result.get("error", "unknown error")
