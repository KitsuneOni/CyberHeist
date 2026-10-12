# The contract choice at the start of a run (Trello #120, #89 and #23): one
# offer on screen at a time, arrows to cycle through the others, and Accept
# to commit. Layout follows sheet 3 of the UI mockups
# (docs/ui-reference/cyberheist-ui-mockups.pdf).
#
# This panel only shows the offers and reports which one the player chose. The
# contract hub owns it, accepts the offer through FlowCoordinator and swaps to
# the map, so the panel never changes run state itself.
extends PanelContainer

# Emitted when the player commits to the offer on screen. `index` is the
# offer's own index, which is what FlowCoordinator.accept_contract_offer takes.
signal offer_chosen(index: int)

@onready var position_label: Label = %OfferPositionLabel
@onready var previous_button: Button = %PreviousOfferButton
@onready var next_button: Button = %NextOfferButton
@onready var name_label: Label = %OfferNameLabel
@onready var target_label: Label = %OfferTargetLabel
@onready var length_label: Label = %OfferLengthLabel
@onready var payout_label: Label = %OfferPayoutLabel
@onready var accept_button: Button = %AcceptOfferButton
@onready var status_label: Label = %OfferStatusLabel

# The offers being chosen between, as FlowCoordinator.contract_offers()
# returns them: shortest first.
var offers: Array = []
# Position in `offers` of the offer on screen.
var shown_position := 0


func _ready() -> void:
	previous_button.pressed.connect(show_previous_offer)
	next_button.pressed.connect(show_next_offer)
	accept_button.pressed.connect(_on_accept_pressed)


# Fills the panel with a fresh set of offers, starting from the first.
func show_offers(new_offers: Array) -> void:
	offers = new_offers
	shown_position = 0
	status_label.text = ""
	_render_shown_offer()
	accept_button.grab_focus()


# The offer currently on screen, or an empty dictionary when there are none.
func shown_offer() -> Dictionary:
	if offers.is_empty():
		return {}
	return offers[shown_position]


# Cycles round, so the arrows never run out and the layout never changes
# between offers.
func show_next_offer() -> void:
	if offers.is_empty():
		return
	shown_position = (shown_position + 1) % offers.size()
	_render_shown_offer()


func show_previous_offer() -> void:
	if offers.is_empty():
		return
	shown_position = (shown_position - 1 + offers.size()) % offers.size()
	_render_shown_offer()


# Shown under the offer when accepting it fails, so the player can try again.
func show_error(message: String) -> void:
	status_label.text = message


func _render_shown_offer() -> void:
	var has_offers := not offers.is_empty()
	previous_button.disabled = offers.size() < 2
	next_button.disabled = offers.size() < 2
	accept_button.disabled = not has_offers

	if not has_offers:
		position_label.text = "No contracts on offer"
		name_label.text = ""
		target_label.text = ""
		length_label.text = ""
		payout_label.text = ""
		return

	var offer: Dictionary = shown_offer()
	position_label.text = "Offer %d of %d" % [shown_position + 1, offers.size()]
	name_label.text = str(offer.get("name", "?"))
	target_label.text = "Target: %s" % offer.get("target", "?")
	length_label.text = "Length: %s, %s" % [
		_count_text(int(offer.get("zone_count", 0)), "zone"),
		_count_text(int(offer.get("encounter_count", 0)), "encounter"),
	]
	payout_label.text = "Payout: %d credits" % int(offer.get("credit_reward", 0))


# "1 zone", "3 zones".
func _count_text(count: int, noun: String) -> String:
	if count == 1:
		return "1 %s" % noun
	return "%d %ss" % [count, noun]


func _on_accept_pressed() -> void:
	if offers.is_empty():
		return
	status_label.text = ""
	offer_chosen.emit(int(shown_offer().get("index", -1)))
