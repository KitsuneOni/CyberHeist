# The card reward offered after clearing an encounter (Trello card 53).
#
# Cards taken here go into the run's deck, which lives on the FlowCoordinator
# autoload rather than on any one encounter screen, so a card picked up now is
# still owned at the next encounter.
extends Control

const OFFER_SIZE := 3

@onready var offer_container: VBoxContainer = $Page/RewardPanel/Margin/Content/OfferContainer
@onready var deck_label: Label = $Page/RewardPanel/Margin/Content/DeckLabel
@onready var status_label: Label = $Page/RewardPanel/Margin/Content/StatusLabel

# The cards on offer this time, as dictionaries from Rust:
# {id, name, type, rarity, cost_text, noise_text, description}.
var offered: Array = []
# Guards against a double click adding two cards from one reward.
var choice_made := false


func _ready() -> void:
	offered = FlowCoordinator.offer_card_reward(OFFER_SIZE)
	_build_offer_buttons()
	_update_deck_label()


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
		offer_container.add_child(button)


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


func _update_deck_label() -> void:
	deck_label.text = "Deck: %d cards" % FlowCoordinator.run_deck_size()


func _leave() -> void:
	var result: Dictionary = FlowCoordinator.finish_reward()
	if not result.get("ok", false):
		# Let the player try again rather than stranding them on this screen.
		choice_made = false
		status_label.text = "Could not continue: %s" % result.get("error", "unknown error")
