# The card reward offered after clearing an encounter (Trello card 53).
#
# Cards taken here go into the run's deck, which lives on the FlowCoordinator
# autoload rather than on any one encounter screen, so a card picked up now is
# still owned at the next encounter.
extends Control

const CardRarity := preload("res://scenes/card_rarity.gd")

@onready var offer_container: VBoxContainer = $Page/RewardPanel/Margin/Content/OfferContainer
@onready var deck_label: Label = $Page/RewardPanel/Margin/Content/DeckLabel
@onready var status_label: Label = $Page/RewardPanel/Margin/Content/StatusLabel

# The cards on offer this time, as dictionaries from Rust:
# {id, name, type, rarity, cost_text, noise_text, description}.
var offered: Array = []
# Guards against a double click adding two cards from one reward.
var choice_made := false


func _ready() -> void:
	offered = FlowCoordinator.offer_card_reward(FlowCoordinator.card_reward_offer_size())
	_build_offer_buttons()
	_update_deck_label()
	_show_credits_earned()


func _build_offer_buttons() -> void:
	for child in offer_container.get_children():
		child.queue_free()

	if offered.is_empty():
		status_label.text = "No cards are available to offer."
		return

	for i in offered.size():
		var card: Dictionary = offered[i]
		var button := Button.new()
		button.custom_minimum_size = Vector2(560, 84)
		button.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		button.text = "%s — %s · %s\n%s · %s\n%s" % [
			card.get("name", "?"),
			card.get("type", "?"),
			card.get("rarity", "?"),
			card.get("cost_text", ""),
			card.get("noise_text", ""),
			card.get("description", ""),
		]
		button.pressed.connect(_on_card_chosen.bind(i))
		offer_container.add_child(_with_rarity_stripe(button, str(card.get("rarity", ""))))


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


func _on_card_chosen(index: int) -> void:
	if choice_made or index < 0 or index >= offered.size():
		return

	var card: Dictionary = offered[index]
	var card_id: String = card.get("id", "")
	if not FlowCoordinator.take_card_reward(card_id):
		status_label.text = "'%s' could not be added to your deck." % card.get("name", card_id)
		return

	# Only now is the choice spent, so a rejected card can still be replaced
	# with another from the same offer.
	choice_made = true
	_update_deck_label()
	_leave()


func _on_skip_pressed() -> void:
	if choice_made:
		return
	choice_made = true
	_leave()


# The payout has already landed by the time this screen opens; this only says
# so, since an elite paying more is half of what makes it worth the risk.
func _show_credits_earned() -> void:
	var credits := FlowCoordinator.pending_reward_credits()
	if credits > 0 and status_label.text == "":
		status_label.text = "+%d credits — balance now %d" % [credits, PlayerStateGlobal.money()]


func _update_deck_label() -> void:
	deck_label.text = "Deck: %d cards" % FlowCoordinator.run_deck_size()


func _leave() -> void:
	var result: Dictionary = FlowCoordinator.finish_reward()
	if not result.get("ok", false):
		# Let the player try again rather than stranding them on this screen.
		choice_made = false
		status_label.text = "Could not continue: %s" % result.get("error", "unknown error")
