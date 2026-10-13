extends RefCounted

# Encounter types whose node fields first-zone standard security (WARDEN-7),
# which is what the fixture's callers are written against. An elite or boss
# node loads its own construct (BASTION, a zone boss), with a different script,
# integrity and resistance, so it is never a stand-in.
const STANDARD_SECURITY_TYPES := ["combat", "event", "shop"]

# Generated contracts are random, so column one does not always offer a combat.
# A run of this many contracts without one would mean combats had stopped being
# generated at all.
const MAX_CONTRACTS := 60


# Enters a fight against standard security from the hub, through the real
# coordinator and Rust lifecycle. Returns select_encounter's result, or
# {ok: false, error} when there is nothing suitable to enter.
#
# On a contract where no encounter has been entered yet, this always enters a
# real combat node, rolling fresh contracts until column one offers one.
# Rolling replaces the run's contract and stats, but not the deck, credits or
# meters.
#
# Once an encounter has been entered the run is never replaced, even if the
# player was caught in it and so completed nothing. A real combat is
# preferred; failing that, combat content is mounted on an event or shop node,
# whose completion then pays and routes like that node type rather than a
# combat. If the column only offers elites or the boss, this fails, and a
# caller that is happy to move to a fresh contract does so with
# roll_new_contract.
#
# The choice depends only on what is on offer, and after being caught the
# failed node is the only one offered, so re-entering is always its retry.
static func enter(flow: Node) -> Dictionary:
	var phase: String = flow.run_snapshot().get("phase", "")
	if phase != "hub":
		return _failure("the fixture enters from the hub, but the run is in phase '%s'" % phase)

	var option := {}
	if not flow.contract_started():
		option = find_combat(flow)
		if option.is_empty():
			return _failure("no contract offered a combat within %d rolls" % MAX_CONTRACTS)
	else:
		option = _standard_security_option(flow)
		if option.is_empty():
			return _failure(
				"the next column only offers elites or the boss; call roll_new_contract first"
			)

	var result: Dictionary = flow.select_encounter(option.id)
	if result.get("ok", false) and option.type != "combat":
		flow._replace_screen(load("res://scenes/combat.tscn").instantiate())
	return result


# A combat node the hub offers on this contract or, failing that, on one of
# the fresh contracts rolled in its place. Empty if none turned up, or if the
# run could not be rolled because an encounter is active.
static func find_combat(flow: Node) -> Dictionary:
	for attempt in MAX_CONTRACTS:
		var combat := _offered_combat(flow)
		if not combat.is_empty():
			return combat
		var rolled := roll_new_contract(flow)
		if not rolled.get("ok", false):
			return {}
	return _offered_combat(flow)


# Whether the hub offers a node enter() can use part-way through a contract.
static func offers_standard_security(flow: Node) -> bool:
	return not _standard_security_option(flow).is_empty()


# Replaces the run with a freshly generated contract, offers and stats, in
# place, through RunStateNode.start_new_run. The deck, credits and meters are
# left alone, unlike FlowCoordinator.start_new_run, and no screen changes.
# Returns {ok: true}, or {ok: false, error} during an encounter, when nothing
# changes.
static func roll_new_contract(flow: Node) -> Dictionary:
	return flow.get_node("RunState").start_new_run()


static func _offered_combat(flow: Node) -> Dictionary:
	for candidate: Dictionary in flow.selectable_encounters():
		if candidate.type == "combat":
			return candidate
	return {}


# A real combat if one is offered, otherwise the first event or shop, or an
# empty dictionary when only elites or the boss are on offer.
static func _standard_security_option(flow: Node) -> Dictionary:
	var combat := _offered_combat(flow)
	if not combat.is_empty():
		return combat
	for candidate: Dictionary in flow.selectable_encounters():
		if candidate.type in STANDARD_SECURITY_TYPES:
			return candidate
	return {}


static func _failure(error: String) -> Dictionary:
	return {"ok": false, "error": error}
