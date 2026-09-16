extends RefCounted


# Generated contracts do not guarantee a combat option (even in column one).
# Keep the actual Rust lifecycle and coordinator, but mount combat content for
# the chosen node when it would otherwise load an event/shop/elite scene.
# This tests combat, not random graph generation or the scene registry.
static func enter(flow: Node) -> Dictionary:
	var options: Array = flow.selectable_encounters()
	if options.is_empty():
		return {"ok": false, "error": "fixture needs a reachable encounter"}
	var option: Dictionary = options[0]
	for candidate: Dictionary in options:
		if candidate.type == "combat":
			option = candidate
			break

	var result: Dictionary = flow.select_encounter(option.id)
	if result.get("ok", false) and option.type != "combat":
		flow._replace_screen(load("res://scenes/combat.tscn").instantiate())
	return result
