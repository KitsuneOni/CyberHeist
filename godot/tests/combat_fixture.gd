extends RefCounted

# Encounter types whose combat screen fields a construct with first-zone
# standard security, which is what the fixture's callers are written against.
# An elite or boss node loads its own construct (BASTION, a zone boss), with a
# different script, integrity and resistance, so it is never a stand-in.
const STANDARD_SECURITY_TYPES := ["combat", "event", "shop"]

# Generated contracts are random, so a column can occasionally offer nothing
# but elites. A run of this many such contracts would mean standard nodes had
# stopped being generated at all.
const MAX_CONTRACTS := 60


# Generated contracts do not guarantee a combat option (even in column one).
# Keep the actual Rust lifecycle and coordinator, but mount combat content for
# the chosen node when it would otherwise load an event or shop scene.
# This tests combat, not random graph generation or the scene registry.
#
# The choice is deterministic for a given set of options, so a caller that
# re-enters after being caught gets the same node back as its retry.
static func enter(flow: Node) -> Dictionary:
	var option := _standard_security_option(flow)
	for attempt in MAX_CONTRACTS:
		if not option.is_empty():
			break
		_roll_new_contract(flow)
		option = _standard_security_option(flow)
	if option.is_empty():
		return {"ok": false, "error": "fixture needs a reachable standard-security encounter"}

	var result: Dictionary = flow.select_encounter(option.id)
	if result.get("ok", false) and option.type != "combat":
		flow._replace_screen(load("res://scenes/combat.tscn").instantiate())
	return result


# A real combat if one is offered, otherwise the first event or shop, or an
# empty dictionary when only elites or a boss are on offer.
static func _standard_security_option(flow: Node) -> Dictionary:
	var options: Array = flow.selectable_encounters()
	for candidate: Dictionary in options:
		if candidate.type == "combat":
			return candidate
	for candidate: Dictionary in options:
		if candidate.type in STANDARD_SECURITY_TYPES:
			return candidate
	return {}


# Replaces the coordinator's run with a freshly generated contract, the same
# thing a new launch does. Only reached when the current contract offers no
# usable node, which is before the caller has entered anything on it.
static func _roll_new_contract(flow: Node) -> void:
	var old: Node = flow.get_node("RunState")
	flow.remove_child(old)
	old.free()
	var fresh: Node = ClassDB.instantiate("RunStateNode")
	fresh.name = "RunState"
	flow.add_child(fresh)
	flow._run_state = fresh
