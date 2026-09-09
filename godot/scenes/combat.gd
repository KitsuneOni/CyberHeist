# Combat scene: lets you test the draw-cards feature by hand.
# `DrawPhase` is defined in ../../cyber_heist/src/lib.rs and wraps the
# Deck/Card logic in deck.rs, card.rs and pack.rs.

extends Control

@onready var draw_phase: Node = $DrawPhase
@onready var hand_label: Label = $VBoxContainer/HandLabel
@onready var pile_label: Label = $VBoxContainer/PileLabel


func _ready() -> void:
	_draw_hand()


func _on_draw_button_pressed() -> void:
	_draw_hand()


func _on_end_turn_button_pressed() -> void:
	draw_phase.discard_hand()
	_draw_hand()


func _on_complete_encounter_pressed() -> void:
	var result: Dictionary = FlowCoordinator.complete_active_encounter()
	if not result.get("ok", false):
		push_error("Could not complete encounter: %s" % result.get("error", "unknown error"))


func _draw_hand() -> void:
	var hand: PackedStringArray = draw_phase.draw_hand()
	hand_label.text = "Hand: %s" % ", ".join(hand)
	pile_label.text = "Draw pile: %d | Discard pile: %d" % [
		draw_phase.draw_pile_count(),
		draw_phase.discard_pile_count(),
	]
