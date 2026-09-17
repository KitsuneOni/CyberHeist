# The contract map: the run drawn as a node graph, with the route already
# taken behind the player and the branches still open ahead.
#
# Layout, status and connections all come from contract_map.rs; this script
# only wires them to the screen, and MapGraph does the drawing.
extends Control

@onready var credits_label: Label = $Page/VBox/MapPanel/MapMargin/MapContent/CreditsLabel
@onready var progress_label: Label = $Page/VBox/MapPanel/MapMargin/MapContent/ProgressLabel
@onready var map_scroll: ScrollContainer = $Page/VBox/MapPanel/MapMargin/MapContent/MapScroll
@onready var map_graph: Control = $Page/VBox/MapPanel/MapMargin/MapContent/MapScroll/MapGraph
@onready var instruction_label: Label = $Page/VBox/MapPanel/MapMargin/MapContent/InstructionLabel
@onready var key_label: Label = $Page/VBox/MapPanel/MapMargin/MapContent/KeyLabel
@onready var status_label: Label = $Page/VBox/MapPanel/MapMargin/MapContent/StatusLabel


func _ready() -> void:
	# The nodes on the map are the only way to choose a route, so selection
	# comes from the graph rather than a separate row of buttons.
	map_graph.node_selected.connect(_on_encounter_selected)
	_build_key()
	_show_available_encounters()


func _show_available_encounters() -> void:
	status_label.text = ""

	# Refreshed on every return to the map, so a change made by an event node
	# is visible as soon as the player lands back here.
	credits_label.text = "Credits: %d" % PlayerStateGlobal.money()

	_update_progress()
	_rebuild_map()

	var available: Array = FlowCoordinator.selectable_encounters()
	if available.is_empty():
		instruction_label.text = "Contract complete — there are no encounters left on this route."
	elif _boss_is_next(available):
		instruction_label.text = "The zone's boss is the only way on. Beat it to unlock the next zone."
	else:
		instruction_label.text = "Click a highlighted node to breach it. Unchosen branches lock for this contract."


# "Zone 2 of 3 · 4 of 9 encounters complete", so both how far through the
# contract the player is and which zone they are in are numbers they can see
# rather than things they have to remember.
func _update_progress() -> void:
	var progress: Dictionary = FlowCoordinator.contract_progress()
	var zones: Dictionary = FlowCoordinator.zone_progress()
	progress_label.text = "Zone %d of %d  ·  Contract progress: %d of %d encounters complete" % [
		int(zones.get("current_zone", 0)) + 1,
		int(zones.get("total_zones", 1)),
		int(progress.get("completed", 0)),
		int(progress.get("total", 0)),
	]


func _rebuild_map() -> void:
	map_graph.set_nodes(FlowCoordinator.map_progress())
	_focus_current_node()


# A contract is wider than the viewport, so the view follows the player
# rather than snapping back to the entry every time they return to the map.
func _focus_current_node() -> void:
	# The graph resizes to the contract, and container sizes settle a frame
	# later, so wait before measuring where to scroll to.
	await get_tree().process_frame

	var node_x: float = map_graph.current_node_x()
	if node_x < 0.0:
		return

	# Centre it where there is room; the container clamps the ends itself.
	map_scroll.scroll_horizontal = int(node_x - map_scroll.size.x * 0.5)


# The key explaining every marker that can appear on the map above.
func _build_key() -> void:
	var key: Dictionary = FlowCoordinator.map_key()

	# Kept to two wrapped lines rather than one per marker, so the key does
	# not push the rest of the map off screen.
	var types: Array[String] = []
	for entry: Dictionary in key.get("encounter_types", []):
		types.append("%s %s" % [str(entry.get("marker", "")), str(entry.get("name", ""))])

	var statuses: Array[String] = []
	for entry: Dictionary in key.get("statuses", []):
		statuses.append("%s %s" % [
			str(entry.get("marker", "")),
			str(entry.get("description", "")).trim_suffix("."),
		])

	key_label.text = "Encounters:  %s\nStatus:  %s" % [
		"   ".join(types),
		"   ".join(statuses),
	]


# A boss column is a single node, so reaching it means there is nothing else
# left to choose in this zone.
func _boss_is_next(available: Array) -> bool:
	for encounter: Dictionary in available:
		if str(encounter.get("type", "")) != "boss":
			return false
	return not available.is_empty()


func _on_encounter_selected(node_id: int) -> void:
	var result: Dictionary = FlowCoordinator.select_encounter(node_id)
	if not result.get("ok", false):
		status_label.text = "Could not start encounter: %s" % result.get("error", "unknown error")
