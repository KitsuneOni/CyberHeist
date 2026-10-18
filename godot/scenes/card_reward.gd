# The victory screen after a fight (Trello card 117), which also offers the
# card reward (Trello card 53).
#
# It shows the credits the fight paid, then the cards on offer. Clicking a
# card only selects it; Continue adds the selected card and returns to the
# map in one step, so a save can never fall between the two and offer the
# same reward again on Continue run.
#
# Cards taken here go into the run's deck, which lives on the FlowCoordinator
# autoload rather than on any one encounter screen, so a card picked up now is
# still owned at the next encounter.
extends Control

const CardRarity := preload("res://scenes/card_rarity.gd")

# The selected offer, outlined in the panel's teal so it stands out from the
# others at a glance.
const SELECTED_FILL := Color(0.06, 0.2, 0.17, 1)
const SELECTED_BORDER := Color(0.25, 0.95, 0.78, 1)

@onready var offer_container: VBoxContainer = $Page/RewardPanel/Margin/Content/OfferContainer
@onready var deck_label: Label = $Page/RewardPanel/Margin/Content/DeckLabel
@onready var status_label: Label = $Page/RewardPanel/Margin/Content/StatusLabel
@onready var credits_label: Label = %CreditsLabel
@onready var balance_label: Label = %BalanceLabel
@onready var continue_button: Button = %ContinueButton

# The cards on offer this time, as dictionaries from Rust:
# {id, name, type, rarity, cost_text, noise_text, description}.
var offered: Array = []
# One button per offered card, in offer order.
var offer_buttons: Array[Button] = []
# The offered card Continue will take, or -1 to take none.
var selected_index := -1
# Guards against a double click on Continue leaving twice.
var choice_made := false
# Set once the selected card is in the deck, so retrying a Continue that
# could not leave does not add it a second time.
var card_taken := false


func _ready() -> void:
	offered = FlowCoordinator.offer_card_reward(FlowCoordinator.card_reward_offer_size())
	_build_offer_buttons()
	_update_deck_label()
	_show_credits_earned()
	_sync_selection()
	continue_button.grab_focus()


func _build_offer_buttons() -> void:
	for child in offer_container.get_children():
		child.queue_free()

	offer_buttons.clear()

	if offered.is_empty():
		status_label.text = "No cards are available to offer."
		return

	for i in offered.size():
		var card: Dictionary = offered[i]
		var button := Button.new()
		button.custom_minimum_size = Vector2(560, 84)
		button.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		button.toggle_mode = true
		var selected_style := _selected_style()
		button.add_theme_stylebox_override("pressed", selected_style)
		button.add_theme_stylebox_override("hover_pressed", selected_style)
		button.text = "%s — %s · %s\n%s · %s\n%s" % [
			card.get("name", "?"),
			card.get("type", "?"),
			card.get("rarity", "?"),
			card.get("cost_text", ""),
			card.get("noise_text", ""),
			card.get("description", ""),
		]
		button.pressed.connect(_on_card_chosen.bind(i))
		offer_buttons.append(button)
		offer_container.add_child(_with_rarity_stripe(button, str(card.get("rarity", ""))))


func _selected_style() -> StyleBoxFlat:
	var style := StyleBoxFlat.new()
	style.bg_color = SELECTED_FILL
	style.border_color = SELECTED_BORDER
	style.set_border_width_all(2)
	style.set_corner_radius_all(4)
	style.set_content_margin_all(4)
	return style


# The offer's button behind a bar in the card's rarity colour (#109). A
# separate bar rather than a button style, so hover and focus keep the theme's.
func _with_rarity_stripe(button: Button, rarity: String) -> HBoxContainer:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 8)
	var stripe := ColorRect.new()
	stripe.name = "RarityStripe"
	stripe.color = CardRarity.colour_for(rarity)
	stripe.custom_minimum_size = Vector2(CardRarity.STRIPE_WIDTH, 0)
	stripe.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(stripe)
	row.add_child(button)
	return row


# Selects the card at `index`, or clears the selection when it is already
# selected. Nothing is added to the deck until Continue.
func _on_card_chosen(index: int) -> void:
	if choice_made or card_taken or index < 0 or index >= offered.size():
		return
	selected_index = -1 if index == selected_index else index
	status_label.text = ""
	_sync_selection()


# Shows which card is selected and says on Continue what it will do.
func _sync_selection() -> void:
	for i in offer_buttons.size():
		offer_buttons[i].set_pressed_no_signal(i == selected_index)
	if selected_index < 0:
		continue_button.text = "Continue without a card"
	else:
		continue_button.text = "Take %s and continue" % offered[selected_index].get("name", "card")


func _on_continue_pressed() -> void:
	if choice_made:
		return

	if selected_index >= 0 and not card_taken:
		var card: Dictionary = offered[selected_index]
		var card_id: String = card.get("id", "")
		if not FlowCoordinator.take_card_reward(card_id):
			# The selection stays, so another card from the same offer can
			# be picked instead.
			status_label.text = "'%s' could not be added to your deck." % card.get("name", card_id)
			return
		card_taken = true
		_update_deck_label()
		# Once it is in the deck the choice is made, even if leaving fails.
		for button in offer_buttons:
			button.disabled = true

	choice_made = true
	_leave()


# The payout has already landed by the time this screen opens; this only says
# so, since an elite paying more is half of what makes it worth the risk.
func _show_credits_earned() -> void:
	credits_label.text = "+%d credits" % FlowCoordinator.pending_reward_credits()
	balance_label.text = "Balance: %d credits" % PlayerStateGlobal.money()


func _update_deck_label() -> void:
	deck_label.text = "Deck: %d cards" % FlowCoordinator.run_deck_size()


func _leave() -> void:
	var result: Dictionary = FlowCoordinator.finish_reward()
	if not result.get("ok", false):
		# Let the player try again rather than stranding them on this screen.
		choice_made = false
		status_label.text = "Could not continue: %s" % result.get("error", "unknown error")
