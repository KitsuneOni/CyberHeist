extends Control

@onready var draw_phase: Node = $DrawPhase
@onready var pile_label: Label = $VBoxContainer/PileLabel
@onready var play_count_label: Label = $VBoxContainer/PlayCountLabel
@onready var card_container: HBoxContainer = $VBoxContainer/CardContainer

var selected_index := -1
var card_buttons: Array[Button] = []


func _ready() -> void:
	_draw_hand()


func _on_draw_button_pressed() -> void:
	_draw_hand()


func _on_end_turn_button_pressed() -> void:
	draw_phase.discard_hand()
	selected_index = -1
	_draw_hand()


func _draw_hand() -> void:
	var hand: PackedStringArray = draw_phase.draw_hand()
	_rebuild_card_buttons(hand)
	_update_pile_label()


func _rebuild_card_buttons(names: PackedStringArray) -> void:
	for child in card_container.get_children():
		child.queue_free()
	card_buttons.clear()

	for i in names.size():
		var button := Button.new()
		button.text = names[i]
		button.toggle_mode = true 
		button.pressed.connect(_on_card_clicked.bind(i))
		card_container.add_child(button)
		card_buttons.append(button)

	_update_selection_visuals()


func _on_card_clicked(index: int) -> void:
	selected_index = index
	_update_selection_visuals()


func _update_selection_visuals() -> void:
	for i in card_buttons.size():
		card_buttons[i].button_pressed = (i == selected_index)


func _on_play_button_pressed() -> void:
	if selected_index == -1:
		return

	var played_name: String = draw_phase.play_card(selected_index)
	if played_name != "":
		play_count_label.text = "Cards played: %d" % draw_phase.play_count
		selected_index = -1
		_rebuild_card_buttons(draw_phase.hand_names())

	_update_pile_label()


func _update_pile_label() -> void:
	pile_label.text = "Draw pile: %d | Discard pile: %d" % [
		draw_phase.draw_pile_count(),
		draw_phase.discard_pile_count(),
	]
