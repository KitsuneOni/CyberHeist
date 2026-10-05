# Real scene/extension coverage for story #120: the contracts a run is offered,
# and accepting one. The Rust tests own the rules; this checks that they reach
# GDScript through FlowCoordinator in the shape the selection and details
# screens will read, and that accepting an offer really changes the map the hub
# draws from.
#
# Run: godot --headless --path godot -s res://tests/contract_offer_test.gd
extends SceneTree

const OFFER_FIELDS := ["index", "name", "target", "credit_reward", "zone_count", "encounter_count"]

var _flow: Node
var _failures: Array[String] = []


func _initialize() -> void:
	_run.call_deferred()


func _check(condition: bool, description: String) -> void:
	if condition:
		print("  ok    %s" % description)
	else:
		print("  FAIL  %s" % description)
		_failures.append(description)


func _finish() -> void:
	print("")
	if _failures.is_empty():
		print("contract offer test: PASSED")
		quit(0)
	else:
		print("contract offer test: FAILED (%d)" % _failures.size())
		for failure in _failures:
			print("  - %s" % failure)
		quit(1)


func _has_every_field(offer: Dictionary) -> bool:
	for field in OFFER_FIELDS:
		if not offer.has(field):
			return false
	return true


func _run() -> void:
	await process_frame

	_flow = root.get_node_or_null("/root/FlowCoordinator")
	_check(_flow != null, "FlowCoordinator autoload is present")
	if _flow == null:
		_finish()
		return

	_check(_flow.has_method("contract_offers"), "FlowCoordinator lists contract offers")
	_check(_flow.has_method("accept_contract_offer"), "FlowCoordinator accepts a contract offer")
	_check(
		_flow.has_method("accepted_contract_offer"),
		"FlowCoordinator reports the accepted contract"
	)
	if not _failures.is_empty():
		_finish()
		return

	var main: Node = load("res://scenes/main.tscn").instantiate()
	root.add_child(main)
	await process_frame
	await process_frame

	# --- acceptance test 1: three offers, each fully described -------------
	var offers: Array = _flow.contract_offers()
	_check(offers.size() == 3, "a new run is offered three contracts (got %d)" % offers.size())
	if offers.size() != 3:
		_finish()
		return

	for position in offers.size():
		var offer: Dictionary = offers[position]
		_check(_has_every_field(offer), "offer %d carries every field: %s" % [position, offer])
		_check(int(offer.get("index", -1)) == position, "offer %d reports its own index" % position)
		_check(not String(offer.get("name", "")).is_empty(), "offer %d has a name" % position)
		_check(not String(offer.get("target", "")).is_empty(), "offer %d has a target" % position)
		_check(int(offer.get("credit_reward", 0)) > 0, "offer %d pays credits" % position)
		_check(int(offer.get("zone_count", 0)) > 0, "offer %d has zones" % position)

	var nothing_accepted: Dictionary = _flow.accepted_contract_offer()
	_check(
		not nothing_accepted.get("ok", true),
		"no contract is accepted until the player chooses one"
	)
	_check(
		not _flow.selectable_encounters().is_empty(),
		"the hub still has a playable contract before anything is accepted"
	)

	# --- acceptance test 2: accepting the second offer builds its map ------
	var second: Dictionary = offers[1]
	var accepted: Dictionary = _flow.accept_contract_offer(1)
	_check(accepted.get("ok", false), "the second offer can be accepted: %s" % accepted)

	var reported: Dictionary = _flow.accepted_contract_offer()
	_check(reported.get("ok", false), "the run reports an accepted contract")
	_check(int(reported.get("index", -1)) == 1, "the accepted contract is the second offer")
	_check(
		String(reported.get("name", "")) == String(second.get("name", "")),
		"the accepted contract keeps the offer's name"
	)
	_check(
		int(_flow.zone_progress().get("total_zones", 0)) == int(second.get("zone_count", -1)),
		"the map has as many zones as the accepted offer"
	)
	_check(
		int(_flow.contract_progress().get("total", 0)) == int(second.get("encounter_count", -1)),
		"a route through the map is as long as the accepted offer"
	)

	# --- rejections change nothing ----------------------------------------
	for bad_index in [3, -1]:
		var rejected: Dictionary = _flow.accept_contract_offer(bad_index)
		_check(not rejected.get("ok", true), "offer index %d is rejected" % bad_index)
		_check(
			not String(rejected.get("error", "")).is_empty(),
			"rejecting offer index %d says why" % bad_index
		)
	_check(
		int(_flow.accepted_contract_offer().get("index", -1)) == 1,
		"rejected indexes leave the accepted contract alone"
	)

	# --- once the contract is under way it cannot be swapped ---------------
	var options: Array = _flow.selectable_encounters()
	var entered: Dictionary = _flow.select_encounter(int(options[0].get("id", -1)))
	_check(entered.get("ok", false), "an encounter on the accepted contract can be entered")
	await process_frame
	await process_frame

	var too_late: Dictionary = _flow.accept_contract_offer(0)
	_check(not too_late.get("ok", true), "a contract cannot be swapped once it is under way")
	_check(
		int(_flow.accepted_contract_offer().get("index", -1)) == 1,
		"the contract under way is still the one accepted"
	)

	_finish()
