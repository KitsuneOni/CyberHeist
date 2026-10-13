# The dark web shop a shop node opens (Trello #38, with S1 #104 and S2 #105).
#
# Five cards are for sale, each with its full details and a price set by its
# rarity. The stock is rolled the first time this screen opens during a shop
# encounter and kept on the run, so it stays the same for the visit even if the
# screen is rebuilt; a bought card's slot shows as sold. A card the player
# cannot afford has its buy button disabled and its price in red. Leave
# completes the shop node and returns to the contract map.
#
# The rules live in ../../cyber_heist/src/shop.rs, and ShopNode (shop_node.rs)
# is the bridge. Buying is one Rust call that checks the price against the
# player's credits, takes it and adds the card to the run's deck together.
#
# The S1/S2 cards give this screen's visual design to Neel, so the layout here
# is deliberately plain and functional, in the main menu's terminal style. It
# reads everything from ShopNode, so a redesign only has to replace this file
# and the scene.
extends Control

const PRICE_COLOUR := Color(0.2, 1, 0.4, 1)
const UNAFFORDABLE_COLOUR := Color(1, 0.2, 0.333333, 1)
# Sold slots stay in place, dimmed, so the rest of the stock does not move.
const SOLD_ROW_ALPHA := 0.45

@onready var shop_node: Node = $ShopNode
@onready var credits_label: Label = %CreditsLabel
@onready var stock_list: VBoxContainer = %StockList
@onready var status_label: Label = %StatusLabel
@onready var leave_button: Button = %LeaveButton

# Each stock slot's controls and card name, keyed by the slot number ShopNode
# reports (and buy() takes), never by position in the stock list, so a row and
# the card it sells cannot drift apart.
var _rows: Dictionary[int, Control] = {}
var _price_labels: Dictionary[int, Label] = {}
var _buy_buttons: Dictionary[int, Button] = {}
var _card_names: Dictionary[int, String] = {}
# Guards against a double click leaving twice.
var _leaving := false


func _ready() -> void:
	leave_button.pressed.connect(_on_leave_pressed)
	_build_stock_rows()
	_refresh()
	_focus_first_control()


# Builds one row per card for sale. The stock is fixed for the visit, so this
# only runs once; _refresh updates the rows after that.
func _build_stock_rows() -> void:
	var stock: Array = shop_node.stock()
	if stock.is_empty():
		status_label.text = "The shop has nothing for sale."
		return

	for card: Dictionary in stock:
		_add_stock_row(card)


func _add_stock_row(card: Dictionary) -> void:
	var slot := int(card.get("slot", -1))
	if slot < 0:
		push_error("ShopNode listed a card without a slot: %s" % card)
		return

	var row := HBoxContainer.new()
	row.name = "Slot%d" % slot
	row.add_theme_constant_override("separation", 24)

	var details := Label.new()
	details.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	details.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	details.add_theme_font_size_override("font_size", 16)
	details.text = _card_details_text(card)
	details.tooltip_text = _keyword_explanations(card)
	details.mouse_filter = Control.MOUSE_FILTER_PASS
	row.add_child(details)

	var purchase := VBoxContainer.new()
	purchase.custom_minimum_size = Vector2(160, 0)
	purchase.alignment = BoxContainer.ALIGNMENT_CENTER
	row.add_child(purchase)

	var price_label := Label.new()
	price_label.name = "PriceLabel"
	price_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	purchase.add_child(price_label)

	var buy_button := Button.new()
	buy_button.name = "BuyButton"
	buy_button.theme_type_variation = &"TerminalMenuButton"
	buy_button.add_theme_font_size_override("font_size", 22)
	buy_button.alignment = HORIZONTAL_ALIGNMENT_RIGHT
	buy_button.pressed.connect(_on_buy_pressed.bind(slot))
	purchase.add_child(buy_button)

	stock_list.add_child(row)
	_rows[slot] = row
	_price_labels[slot] = price_label
	_buy_buttons[slot] = buy_button
	_card_names[slot] = String(card.get("name", "the card"))


# Three lines, so all five cards fit on screen at once: name, type and rarity;
# what it costs to play, the noise it makes and the effects it carries; then
# what it does.
func _card_details_text(card: Dictionary) -> String:
	var heading := "%s  —  %s · %s" % [
		String(card.get("name", "?")).to_upper(),
		card.get("type", "?"),
		card.get("rarity", "?"),
	]

	var facts: Array[String] = [
		String(card.get("cost_text", "")),
		String(card.get("noise_text", "")),
	]
	var labels: Array[String] = []
	for keyword: Dictionary in card.get("keywords", []):
		labels.append(String(keyword.get("label", "")))
	if not labels.is_empty():
		facts.append("Effects: %s" % ", ".join(labels))

	return "\n".join([heading, " · ".join(facts), String(card.get("description", ""))])


# Hovering a card explains each of its effects, as the combat card detail does.
func _keyword_explanations(card: Dictionary) -> String:
	var lines: Array[String] = []
	for keyword: Dictionary in card.get("keywords", []):
		lines.append("%s: %s" % [keyword.get("label", ""), keyword.get("explanation", "")])
	return "\n".join(lines)


# Re-reads the stock and the balance, so every slot shows whether it is sold
# and whether it is still affordable after the last purchase.
func _refresh() -> void:
	credits_label.text = "$ %d  credits on hand" % PlayerStateGlobal.money()

	for card: Dictionary in shop_node.stock():
		var slot := int(card.get("slot", -1))
		if not _rows.has(slot):
			continue
		var price := int(card.get("price", 0))
		var price_label := _price_labels[slot]
		var buy_button := _buy_buttons[slot]

		if card.get("sold", false):
			_rows[slot].modulate.a = SOLD_ROW_ALPHA
			price_label.text = "SOLD"
			price_label.remove_theme_color_override("font_color")
			buy_button.text = "sold"
			buy_button.disabled = true
		else:
			var affordable: bool = card.get("affordable", false)
			price_label.text = "%d cr" % price
			price_label.add_theme_color_override(
				"font_color", PRICE_COLOUR if affordable else UNAFFORDABLE_COLOUR
			)
			buy_button.text = "[ buy ]"
			buy_button.disabled = not affordable

		# A disabled button is skipped by keyboard navigation, as the main
		# menu does for Continue run.
		buy_button.focus_mode = Control.FOCUS_NONE if buy_button.disabled else Control.FOCUS_ALL


func _focus_first_control() -> void:
	for buy_button: Button in _buy_buttons.values():
		if not buy_button.disabled:
			buy_button.grab_focus()
			return
	leave_button.grab_focus()


func _on_buy_pressed(slot: int) -> void:
	var result: Dictionary = shop_node.buy(slot)
	if result.get("ok", false):
		status_label.text = "Bought %s for %d credits. It is in your deck." % [
			_card_names.get(slot, "the card"),
			int(result.get("price", 0)),
		]
	else:
		status_label.text = _refusal_text(String(result.get("error", "")))

	_refresh()
	if not _buy_buttons.has(slot) or _buy_buttons[slot].disabled:
		_focus_first_control()


func _refusal_text(error: String) -> String:
	match error:
		"not_enough_credits":
			return "Not enough credits for that card."
		"already_sold":
			return "That card has already been sold."
		_:
			return "Could not buy that card: %s" % error


func _on_leave_pressed() -> void:
	if _leaving:
		return
	_leaving = true

	var result: Dictionary = FlowCoordinator.complete_active_encounter()
	if not result.get("ok", false):
		# Let the player try again rather than stranding them in the shop.
		_leaving = false
		status_label.text = "Could not leave the shop: %s" % result.get("error", "unknown error")
