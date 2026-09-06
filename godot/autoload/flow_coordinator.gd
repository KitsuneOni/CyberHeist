extends Node

const HUB_SCENE_PATH := "res://scenes/contract_hub.tscn"
const COMBAT_SCENE_PATH := "res://scenes/combat.tscn"
const PLACEHOLDER_ENCOUNTER_SCENE_PATH := "res://scenes/placeholder_encounter.tscn"
const CAUGHT_SCENE_PATH := "res://scenes/caught_screen.tscn"
const ENCOUNTER_SCENES := {
	"combat": COMBAT_SCENE_PATH,
	"event": PLACEHOLDER_ENCOUNTER_SCENE_PATH,
	"shop": PLACEHOLDER_ENCOUNTER_SCENE_PATH,
	"elite": PLACEHOLDER_ENCOUNTER_SCENE_PATH,
}

@onready var _run_state: Node = $RunState

var _screen_host: Node


func bind_screen_host(host: Node) -> void:
	if host == null:
		push_error("FlowCoordinator requires a valid ScreenHost.")
		return

	_screen_host = host
	var destination := _destination_for_snapshot(run_snapshot())
	if not destination.get("ok", false):
		push_error(destination.get("error", "Could not resolve the current run screen."))
		return

	var prepared := _prepare_screen(destination["path"])
	if not prepared.get("ok", false):
		push_error(prepared.get("error", "Could not load the current run screen."))
		return

	_replace_screen(prepared["screen"])


func selectable_encounters() -> Array:
	return _run_state.selectable_encounters()


func current_contract_node() -> Dictionary:
	return _run_state.current_contract_node()


func select_encounter(node_id: int) -> Dictionary:
	var option: Dictionary = _run_state.encounter_option(node_id)
	if not option.get("ok", false):
		return option

	var encounter_type: String = option.get("type", "")
	if not ENCOUNTER_SCENES.has(encounter_type):
		return _failure("No gameplay scene is registered for encounter type '%s'." % encounter_type)

	var prepared := _prepare_screen(ENCOUNTER_SCENES[encounter_type])
	if not prepared.get("ok", false):
		return prepared

	var transition: Dictionary = _run_state.select_encounter(node_id)
	if not transition.get("ok", false):
		_dispose_screen(prepared["screen"])
		return transition

	_replace_screen(prepared["screen"])
	return transition


func complete_active_encounter() -> Dictionary:
	var prepared := _prepare_screen(HUB_SCENE_PATH)
	if not prepared.get("ok", false):
		return prepared

	var transition: Dictionary = _run_state.complete_encounter()
	if not transition.get("ok", false):
		_dispose_screen(prepared["screen"])
		return transition

	_replace_screen(prepared["screen"])
	return transition


func report_caught() -> Dictionary:
	var prepared := _prepare_screen(CAUGHT_SCENE_PATH)
	if not prepared.get("ok", false):
		return prepared

	var transition: Dictionary = _run_state.report_caught()
	if not transition.get("ok", false):
		_dispose_screen(prepared["screen"])
		return transition

	_replace_screen(prepared["screen"])
	return transition


func finish_caught() -> Dictionary:
	var prepared := _prepare_screen(HUB_SCENE_PATH)
	if not prepared.get("ok", false):
		return prepared

	var transition: Dictionary = _run_state.finish_caught()
	if not transition.get("ok", false):
		_dispose_screen(prepared["screen"])
		return transition

	_replace_screen(prepared["screen"])
	return transition


func run_snapshot() -> Dictionary:
	return _run_state.snapshot()


func _prepare_screen(path: String) -> Dictionary:
	if not _screen_host_is_ready():
		return _failure("FlowCoordinator has not been bound to a valid ScreenHost.")
	if not ResourceLoader.exists(path):
		return _failure("Gameplay scene does not exist: %s" % path)

	var packed_scene: PackedScene = ResourceLoader.load(path) as PackedScene
	if packed_scene == null:
		return _failure("Gameplay scene could not be loaded: %s" % path)

	var screen: Node = packed_scene.instantiate()
	if screen == null:
		return _failure("Gameplay scene could not be instantiated: %s" % path)

	return {
		"ok": true,
		"screen": screen,
	}


func _replace_screen(screen: Node) -> void:
	for child in _screen_host.get_children():
		_screen_host.remove_child(child)
		child.queue_free()
	_screen_host.add_child(screen)


func _dispose_screen(screen: Node) -> void:
	if is_instance_valid(screen):
		screen.free()


func _screen_host_is_ready() -> bool:
	return _screen_host != null and is_instance_valid(_screen_host)


func _destination_for_snapshot(snapshot: Dictionary) -> Dictionary:
	var phase: String = snapshot.get("phase", "")
	match phase:
		"hub":
			return {"ok": true, "path": HUB_SCENE_PATH}
		"encounter_active":
			var active_encounter = snapshot.get("active_encounter")
			if typeof(active_encounter) != TYPE_DICTIONARY:
				return _failure("Encounter-active run state has no active encounter.")
			var encounter_type: String = active_encounter.get("type", "")
			if not ENCOUNTER_SCENES.has(encounter_type):
				return _failure("No gameplay scene is registered for encounter type '%s'." % encounter_type)
			return {"ok": true, "path": ENCOUNTER_SCENES[encounter_type]}
		"caught":
			return {"ok": true, "path": CAUGHT_SCENE_PATH}
		_:
			return _failure("Unknown contract phase '%s'." % phase)


func _failure(error: String) -> Dictionary:
	return {
		"ok": false,
		"error": error,
	}
