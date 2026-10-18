extends Node

# The pause menu, here for the whole session (story #110).
var pause_menu: CanvasLayer


func _ready() -> void:
	pause_menu = preload("res://scenes/pause_menu.gd").new()
	add_child(pause_menu)
	# Before the screen host, so the screen gets Escape first: unhandled input
	# reaches later siblings before earlier ones.
	move_child(pause_menu, 0)
	FlowCoordinator.bind_screen_host($ScreenHost)
