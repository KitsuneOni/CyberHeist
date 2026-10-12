# The contract map: the run drawn as a node graph, with the route already
# taken behind the player and the branches still open ahead.
#
# Layout, status and connections all come from contract_map.rs; this script
# only wires them to the screen, and MapGraph does the drawing.
#
# A new run first asks the player which contract to take (the offer picker),
# and only shows the map once one is accepted. The accepted offer is what
# winning the contract pays, so every run the player starts carries a reward.
# Until they enter an encounter, Change contract on the map brings the picker
# back so they can swap to another offer.
extends Control

const CHOOSING_TITLE := "Choose your contract"
const CHOOSING_SUBTITLE := "Longer contracts take more zones and pay more when you win."
const MAP_TITLE := "Choose your next breach"
const MAP_SUBTITLE := "Each route changes which encounters remain available for this contract."

# How many frames to give the map graph to reach the width it asked for before
# scrolling anyway. Layout normally settles in one or two; the bound just stops
# a contract that never grows from waiting forever.
const MAX_LAYOUT_FRAMES := 8

@onready var title_label: Label = $Page/VBox/Title
@onready var subtitle_label: Label = $Page/VBox/Subtitle
@onready var offer_picker: PanelContainer = $Page/VBox/OfferPicker
@onready var map_panel: PanelContainer = $Page/VBox/MapPanel
@onready var credits_label: Label = $Page/VBox/MapPanel/MapMargin/MapContent/CreditsLabel
@onready var progress_label: Label = $Page/VBox/MapPanel/MapMargin/MapContent/ProgressLabel
@onready var map_scroll: ScrollContainer = $Page/VBox/MapPanel/MapMargin/MapContent/MapScroll
@onready var map_graph: Control = $Page/VBox/MapPanel/MapMargin/MapContent/MapScroll/MapGraph
@onready var instruction_label: Label = $Page/VBox/MapPanel/MapMargin/MapContent/InstructionLabel
@onready
var change_contract_button: Button = $Page/VBox/MapPanel/MapMargin/MapContent/ChangeContractButton
@onready var key_label: Label = $Page/VBox/MapPanel/MapMargin/MapContent/KeyLabel
@onready var status_label: Label = $Page/VBox/MapPanel/MapMargin/MapContent/StatusLabel


func _ready() -> void:
	# The nodes on the map are the only way to choose a route, so selection
	# comes from the graph rather than a separate row of buttons.
	map_graph.node_selected.connect(_on_encounter_selected)
	offer_picker.offer_chosen.connect(_on_offer_chosen)
	change_contract_button.pressed.connect(_on_change_contract_pressed)
	_build_key()

	if FlowCoordinator.needs_contract_choice():
		_show_contract_choice()
	else:
		_show_map()


# Whether the player is choosing a contract rather than looking at the map.
func is_choosing_contract() -> bool:
	return offer_picker.visible


func _show_contract_choice() -> void:
	title_label.text = CHOOSING_TITLE
	subtitle_label.text = CHOOSING_SUBTITLE
	map_panel.visible = false
	offer_picker.visible = true
	# Opens on the contract already accepted, if any, so Accept straight away
	# keeps it.
	var accepted_index := int(FlowCoordinator.accepted_contract_offer().get("index", -1))
	offer_picker.show_offers(FlowCoordinator.contract_offers(), accepted_index)


func _show_map() -> void:
	title_label.text = MAP_TITLE
	subtitle_label.text = MAP_SUBTITLE
	offer_picker.visible = false
	map_panel.visible = true
	change_contract_button.visible = FlowCoordinator.can_change_contract()
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
	#
	# One frame is not always enough. On a hub that has just been instantiated
	# the graph has not yet reached the width it asked for, and the
	# ScrollContainer clamps scroll_horizontal against that width, so scrolling
	# too early silently lands back at the left edge. Wait for the graph to
	# reach its requested width, bounded so a contract that never grows cannot
	# stall the hub.
	for _attempt in MAX_LAYOUT_FRAMES:
		await get_tree().process_frame
		if not is_instance_valid(map_graph) or not is_instance_valid(map_scroll):
			return
		if map_graph.size.x >= map_graph.custom_minimum_size.x:
			break

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


# Accepting rebuilds the run's map from the offer, so the map is drawn only
# after the acceptance has gone through.
func _on_offer_chosen(index: int) -> void:
	var result: Dictionary = FlowCoordinator.accept_contract_offer(index)
	if not result.get("ok", false):
		offer_picker.show_error(
			"Could not accept the contract: %s" % result.get("error", "unknown error")
		)
		return
	_show_map()


func _on_change_contract_pressed() -> void:
	if not FlowCoordinator.can_change_contract():
		change_contract_button.visible = false
		return
	_show_contract_choice()


func _on_encounter_selected(node_id: int) -> void:
	var result: Dictionary = FlowCoordinator.select_encounter(node_id)
	if not result.get("ok", false):
		status_label.text = "Could not start encounter: %s" % result.get("error", "unknown error")
