# Real scene/extension coverage for different types of security system
# (Trello #40). The Rust tests own the scripts; this checks the map shows each
# combat's type before the player commits, and that entering two combats in
# the same zone really fields two different constructs on the combat screen.
#
#   Scenario: Different security system types have distinct attack patterns
#     Given the player enters two different encounters in the same zone
#     When each encounter begins
#     Then each security system type has a different sequence of actions
#     And the player must adapt their card choices to counter each pattern
#
#   Scenario: The security system type is visible before the player commits
#     Given the player is viewing the contract map
#     When they hover over or inspect an encounter node
#     Then the security system type icon or label is displayed
#     And it is distinguishable from other encounter node types on the map
#
# Run: godot --headless --path godot -s res://tests/security_types_test.gd
extends SceneTree

# The construct each type fields, by name (sentry.rs).
const CONSTRUCT_PREFIX := {
	"warden": "WARDEN-",
	"firewall": "AEGIS-",
	"tracer": "TRACER-",
	"antivirus": "VIRUSCAN-",
}
# Every type's script is four or five actions long, so this many turns shows
# where two sequences part ways.
const TURNS_TO_COMPARE := 4

# A first zone whose first column offers a combat leading on to another combat
# turns up in most runs; this many without one would mean something broke.
const MAX_RUNS := 60

var _flow: Node
var _noise_meter: Node
var _failures: Array[String] = []


func _initialize() -> void:
	_run.call_deferred()


func _check(condition: bool, description: String) -> void:
	if condition:
		print("  ok    %s" % description)
	else:
		print("  FAIL  %s" % description)
		_failures.append(description)


func _screen() -> Node:
	var host := root.get_node_or_null("Main/ScreenHost")
	if host == null or host.get_child_count() == 0:
		return null
	return host.get_child(0)


func _finish() -> void:
	print("")
	if _failures.is_empty():
		print("security types test: PASSED")
		quit(0)
	else:
		print("security types test: FAILED (%d)" % _failures.size())
		for failure in _failures:
			print("  - %s" % failure)
		quit(1)


func _settle() -> void:
	await process_frame
	await process_frame


func _map_node(node_id: int) -> Dictionary:
	for entry: Dictionary in _flow.map_progress():
		if int(entry.get("id", -1)) == node_id:
			return entry
	return {}


# From the desktop of a new run, opens contracts.exe and accepts the offer
# the picker shows. Returns the hub showing the map, or null.
func _open_map() -> Node:
	var desktop := _screen()
	if desktop != null and desktop.name == "ComputerDesktop":
		desktop.contracts_app.pressed.emit()
		await _settle()
	var hub := _screen()
	if hub == null or hub.name != "ContractHub":
		return null
	if hub.is_choosing_contract():
		hub.offer_picker.accept_button.pressed.emit()
		await _settle()
	return null if hub.is_choosing_contract() else hub


# A first-column combat with a combat straight after it in the same zone, as
# [first_id, second_id], or [] when this run's map has none.
func _combat_pair() -> Array:
	for option: Dictionary in _flow.selectable_encounters():
		if str(option.get("type", "")) != "combat":
			continue
		var first := _map_node(int(option.id))
		for next_id in first.get("connections", []):
			var next := _map_node(int(next_id))
			if str(next.get("type", "")) == "combat" and int(next.get("zone", -1)) == int(first.get("zone", -2)):
				return [int(first.id), int(next.id)]
	return []


# Clicks `node_id` on the map and reads what the combat screen fields: the
# construct's name, its security type as the screen explains it, and the
# actions it takes over its first few turns.
func _enter_and_read(hub: Node, node_id: int) -> Dictionary:
	_noise_meter.add_noise(-_noise_meter.noise)
	hub.map_graph.node_selected.emit(node_id)
	await _settle()
	var combat := _screen()
	if combat == null or combat.name != "CombatScreen":
		return {}

	var sentry: Node = combat.sentry
	var read := {
		"name": str(sentry.construct_name()),
		"security": sentry.security_type(),
		"tooltip": str(combat.enemy_name.tooltip_text),
		"actions": [],
	}
	for turn in TURNS_TO_COMPARE:
		read.actions.append(str(sentry.queued_action_name()))
		sentry.perform_queued_action()
	_noise_meter.add_noise(-_noise_meter.noise)

	# Skip the rest of the fight and take the reward, back to the map.
	_flow.complete_active_encounter()
	await _settle()
	if _screen() != null and _screen().name == "CardReward":
		_flow.finish_reward()
		await _settle()
	return read


func _check_map_shows_security(hub: Node) -> void:
	var encounter_markers: Array[String] = []
	for entry: Dictionary in _flow.map_key().get("encounter_types", []):
		encounter_markers.append(str(entry.get("marker", "")).trim_prefix("[").trim_suffix("]"))

	var combats := 0
	var all_labelled := true
	var all_distinct := true
	var all_in_tooltip := true
	var others_unbadged := true
	for node: Dictionary in _flow.map_progress():
		if str(node.get("type", "")) != "combat":
			others_unbadged = others_unbadged and str(node.get("security_marker", "")).is_empty()
			continue
		combats += 1
		var badge := str(node.get("security_marker", ""))
		var security_name := str(node.get("security_name", ""))
		all_labelled = all_labelled and not badge.is_empty() and not security_name.is_empty()
		# The badge sits under the node's own [C], and is never a letter the
		# key uses for an encounter type.
		all_distinct = all_distinct and not encounter_markers.has(badge)

		# Hovering the node is what a player does to inspect it.
		hub.map_graph._update_tooltip(node)
		all_in_tooltip = (
			all_in_tooltip
			and hub.map_graph.tooltip_text.contains("Security: %s" % security_name)
		)

	_check(combats > 0, "the map has combat nodes")
	_check(all_labelled, "every combat node carries a security badge and name")
	_check(all_distinct, "no security badge is also an encounter marker")
	_check(others_unbadged, "only combat nodes carry a security badge")
	_check(all_in_tooltip, "hovering a combat node names its security type and how to beat it")

	var key_text: String = hub.key_label.text
	var key_names := true
	for entry: Dictionary in _flow.map_key().get("security_types", []):
		key_names = key_names and key_text.contains("%s %s" % [entry.marker, entry.name])
	_check(key_text.contains("Security:") and key_names, "the map key explains every security badge")


func _run() -> void:
	await process_frame

	_flow = root.get_node_or_null("/root/FlowCoordinator")
	_noise_meter = root.get_node_or_null("/root/NoiseMeterGlobal")
	_check(_flow != null, "FlowCoordinator autoload is present")
	_check(_noise_meter != null, "NoiseMeterGlobal autoload is present")
	if _flow == null or _noise_meter == null:
		_finish()
		return

	var main: Node = load("res://scenes/main.tscn").instantiate()
	root.add_child(main)
	await _settle()
	var menu := _screen()
	_check(menu != null and menu.name == "MainMenu", "the game opens on the main menu")
	if menu == null:
		_finish()
		return
	menu.get_node("%NewContractButton").pressed.emit()
	await _settle()

	# --- Scenario 2: the type is on the map before committing ---------------
	var hub := await _open_map()
	_check(hub != null, "accepting a contract shows the map")
	if hub == null:
		_finish()
		return
	_check_map_shows_security(hub)

	# --- Scenario 1: two combats in one zone, two different constructs -------
	var pair := []
	for attempt in MAX_RUNS:
		pair = _combat_pair()
		if not pair.is_empty():
			break
		_flow.start_new_run()
		await _settle()
		hub = await _open_map()
		if hub == null:
			break
	_check(not pair.is_empty(), "a first zone has a combat leading to another within %d runs" % MAX_RUNS)
	if pair.is_empty():
		_finish()
		return

	var expected_first := str(_map_node(pair[0]).get("security", ""))
	var expected_second := str(_map_node(pair[1]).get("security", ""))
	_check(expected_first != expected_second, "the map gives the two combats different types")

	var first := await _enter_and_read(hub, pair[0])
	hub = _screen()
	var second := {}
	if hub != null and hub.name == "ContractHub":
		second = await _enter_and_read(hub, pair[1])
	_check(not first.is_empty() and not second.is_empty(), "both combats open the combat screen")
	if first.is_empty() or second.is_empty():
		_finish()
		return

	for read: Dictionary in [first, second]:
		var expected := expected_first if read == first else expected_second
		_check(
			str(read.name).begins_with(CONSTRUCT_PREFIX.get(expected, "?")),
			"a %s node fields %s (got %s)" % [expected, CONSTRUCT_PREFIX.get(expected, "?"), read.name]
		)
		_check(
			str(read.security.get("type", "")) == expected,
			"the combat screen knows it is facing a %s" % expected
		)
		_check(
			str(read.tooltip).begins_with(str(read.security.get("name", "?"))),
			"hovering %s explains its type (%s)" % [read.name, read.tooltip]
		)
	_check(
		first.actions != second.actions,
		"the two constructs run different sequences (%s vs %s)" % [first.actions, second.actions]
	)

	_finish()
