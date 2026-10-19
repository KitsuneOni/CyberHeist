# Covers story #115: each zone of a contract has its own look. Walks a
# three-zone contract through the real coordinator and checks that, in each
# zone, the map and the combat screen show that zone's background and the map
# names the zone.
#
# Run it with:
#
#     godot --headless --path godot -s res://tests/zone_theme_test.gd
extends SceneTree

const ZoneTheme := preload("res://scenes/zone_theme.gd")
const WATCHDOG_SECONDS := 120.0
const FIGHTS := ["combat", "elite", "boss"]

var _flow: Node
var _failures: Array[String] = []
var _finished := false
var _elapsed := 0.0


func _initialize() -> void:
	_run.call_deferred()


func _process(delta: float) -> bool:
	if _finished:
		return true
	_elapsed += delta
	if _elapsed >= WATCHDOG_SECONDS:
		print("zone theme test: TIMED OUT after %.0fs" % WATCHDOG_SECONDS)
		_finished = true
		quit(1)
		return true
	return false


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


func _settle() -> void:
	await process_frame
	await process_frame
	await process_frame


func _finish() -> void:
	_finished = true
	if _failures.is_empty():
		print("zone theme test: PASSED")
		quit(0)
	else:
		print("zone theme test: FAILED (%d)" % _failures.size())
		for failure in _failures:
			print("  - %s" % failure)
		quit(1)


func _current_zone() -> int:
	return int(_flow.zone_progress().get("current_zone", -1))


# The backdrop a screen shows, or null when it has none or it is hidden.
func _backdrop_path(screen: Node) -> String:
	var art: TextureRect = screen.get_node_or_null("ZoneBackdrop")
	if art == null or not art.visible or art.texture == null:
		return ""
	return art.texture.resource_path


# Completes encounters, always taking a boss when one is offered, until the
# player stands on the map in `zone`. False if the route runs out first.
func _walk_to_zone(zone: int) -> bool:
	for step in 40:
		if _current_zone() >= zone and _flow.run_snapshot().get("phase", "") == "hub":
			return _current_zone() == zone
		var options: Array = _flow.selectable_encounters()
		if options.is_empty():
			return false
		var chosen: Dictionary = options[0]
		for candidate: Dictionary in options:
			if str(candidate.get("type", "")) == "boss":
				chosen = candidate
		if not _flow.select_encounter(chosen.id).get("ok", false):
			return false
		await _settle()
		if not _flow.complete_active_encounter().get("ok", false):
			return false
		await _settle()
		if str(chosen.get("type", "")) in FIGHTS:
			_flow.finish_reward()
			await _settle()
	return false


func _run() -> void:
	await process_frame
	_flow = root.get_node("FlowCoordinator")
	root.add_child(load("res://scenes/main.tscn").instantiate())
	await process_frame

	print("Every zone has a name and art")
	for zone in ZoneTheme.ZONES.size():
		var look: Dictionary = ZoneTheme.for_zone(zone)
		_check(not String(look.get("name", "")).is_empty(), "zone %d has a name" % (zone + 1))
		_check(load(look.get("background", "")) is Texture2D, "zone %d's background loads" % (zone + 1))
	_check(
		ZoneTheme.for_zone(ZoneTheme.ZONES.size()) == ZoneTheme.for_zone(ZoneTheme.ZONES.size() - 1),
		"a zone past the last reuses the deepest look"
	)

	_flow.start_new_contract()
	await _settle()
	var offers: Array = _flow.contract_offers()
	var deepest := -1
	for i in offers.size():
		if int(offers[i].get("zone_count", 0)) == ZoneTheme.ZONES.size():
			deepest = i
	# Offers are random, so roll fresh ones until a three-zone contract turns up.
	for attempt in 30:
		if deepest >= 0:
			break
		preload("res://tests/combat_fixture.gd").roll_new_contract(_flow)
		offers = _flow.contract_offers()
		for i in offers.size():
			if int(offers[i].get("zone_count", 0)) == ZoneTheme.ZONES.size():
				deepest = i
	_check(deepest >= 0, "a three-zone contract is on offer")
	if deepest < 0:
		_finish()
		return

	var hub := _screen()
	_check(
		hub != null and hub.name == "ContractHub" and _backdrop_path(hub) == "",
		"choosing a contract shows no zone yet"
	)
	_flow.accept_contract_offer(deepest)
	await _settle()

	var seen := {}
	for zone in ZoneTheme.ZONES.size():
		var look: Dictionary = ZoneTheme.for_zone(zone)
		var label := "zone %d (%s)" % [zone + 1, look["name"]]
		print("Entering %s" % label)
		var reached: bool = await _walk_to_zone(zone)
		_check(reached, "the route reaches %s" % label)
		if not reached:
			break

		_flow._show_current_run_screen()
		await _settle()
		hub = _screen()
		_check(hub != null and hub.name == "ContractHub", "%s: the map is showing" % label)
		if hub == null:
			break
		_check(
			_backdrop_path(hub) == look["background"],
			"%s: the map shows the zone's background (got '%s')" % [label, _backdrop_path(hub)]
		)
		_check(
			hub.eyebrow_label.text.contains(String(look["name"]).to_upper()),
			"%s: the map names the zone (got '%s')" % [label, hub.eyebrow_label.text]
		)
		_check(
			hub.progress_label.text.contains(look["name"]),
			"%s: the progress line names it too" % label
		)
		seen[_backdrop_path(hub)] = true

		var entered: Dictionary = preload("res://tests/combat_fixture.gd").enter(_flow)
		_check(entered.get("ok", false), "%s: a fight loads" % label)
		if not entered.get("ok", false):
			break
		await _settle()
		var combat := _screen()
		_check(
			combat != null and _backdrop_path(combat) == look["background"],
			"%s: combat shows the zone's background" % label
		)
		_flow.complete_active_encounter()
		await _settle()
		_flow.finish_reward()
		await _settle()

	_check(seen.size() == ZoneTheme.ZONES.size(), "each zone looks different")
	_finish()
