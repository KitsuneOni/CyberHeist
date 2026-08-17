# Temporary scene script demonstrating how GDScript calls into Rust.
# `BridgeCheck` is defined in ../../cyber_heist/src/lib.rs and registered with
# Godot by the GDExtension, so it is used here like any built-in node type.
# Delete this together with BridgeCheck once real gameplay classes exist.

extends Node


func _ready() -> void:
	# `ping` is a Rust method exposed with #[func].
	print("CyberHeist: GDScript called Rust and got -> ", $BridgeCheck.ping())
