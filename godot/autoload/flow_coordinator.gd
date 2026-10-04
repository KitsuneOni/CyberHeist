extends Node

const HUB_SCENE_PATH := "res://scenes/contract_hub.tscn"
const COMBAT_SCENE_PATH := "res://scenes/combat.tscn"
const PLACEHOLDER_ENCOUNTER_SCENE_PATH := "res://scenes/placeholder_encounter.tscn"
const EVENT_SCENE_PATH := "res://scenes/event_encounter.tscn"
const CAUGHT_SCENE_PATH := "res://scenes/caught_screen.tscn"
const CARD_REWARD_SCENE_PATH := "res://scenes/card_reward.tscn"
const ENCOUNTER_SCENES := {
	"combat": COMBAT_SCENE_PATH,
	"event": EVENT_SCENE_PATH,
	"shop": PLACEHOLDER_ENCOUNTER_SCENE_PATH,
	"elite": PLACEHOLDER_ENCOUNTER_SCENE_PATH,
	# A boss is a combat encounter; what makes it one is the construct behind
	# it, which SentryNode builds from the run rather than from the scene.
	"boss": COMBAT_SCENE_PATH,
}

# Encounter types that earn a card reward when cleared. Event and shop nodes
# hand out their own rewards, so they go straight back to the hub.
const REWARDING_ENCOUNTERS := ["combat", "elite", "boss"]

@onready var _run_state: Node = $RunState
@onready var _run_deck: Node = $RunDeck

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


func map_progress() -> Array:
	return _run_state.map_progress()


func contract_progress() -> Dictionary:
	return _run_state.contract_progress()


func zone_progress() -> Dictionary:
	return _run_state.zone_progress()


func map_key() -> Dictionary:
	return _run_state.map_key()


# The contracts this run can choose between, shortest first. Each is a
# dictionary of {index, name, target, credit_reward, zone_count, encounter_count}.
func contract_offers() -> Array:
	return _run_state.contract_offers()


# Takes on the offer at `index` and rebuilds the contract map from it. Returns
# {ok: true} or {ok: false, error}; a rejection changes nothing. Allowed only
# from the hub before any encounter on the contract has been entered.
#
# This only changes run state. It does not navigate, so the selection or
# details screen decides where to go next once it has the result.
func accept_contract_offer(index: int) -> Dictionary:
	return _run_state.accept_contract_offer(index)


# The accepted offer's fields plus ok: true, or {ok: false} while the run is
# still on the contract it started with.
func accepted_contract_offer() -> Dictionary:
	return _run_state.accepted_contract_offer()


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
	# Read the encounter type before completing, because completing clears it.
	var next_scene := HUB_SCENE_PATH
	var active = run_snapshot().get("active_encounter")
	if typeof(active) == TYPE_DICTIONARY and active.get("type", "") in REWARDING_ENCOUNTERS:
		next_scene = CARD_REWARD_SCENE_PATH

	var prepared := _prepare_screen(next_scene)
	if not prepared.get("ok", false):
		return prepared

	var transition: Dictionary = _run_state.complete_encounter()
	if not transition.get("ok", false):
		_dispose_screen(prepared["screen"])
		return transition

	_replace_screen(prepared["screen"])
	return transition


# Cards on offer after clearing an encounter. Each is a dictionary of
# {id, name, type, rarity, cost_text, noise_text, description}.
func offer_card_reward(count: int) -> Array:
	return _run_deck.offer_reward(count)


# Adds a chosen reward to the run's deck. False for an id the card database
# does not know, so the reward screen can say so rather than silently dropping it.
func take_card_reward(card_id: String) -> bool:
	return add_card_to_deck(card_id)


# Adds a card to the run's deck and counts it in the run's stats. Every way of
# gaining a card (rewards today, a shop later) goes through here, so the end
# screen's count cannot miss one or count one twice. False, with nothing
# counted, for an id the card database does not know.
func add_card_to_deck(card_id: String) -> bool:
	if not _run_deck.add_card(card_id):
		return false
	_run_state.record_card_added()
	return true


# Changes the player's credits by `amount` (negative to take them away) and
# records the change on the run, where only gains count as earned. Returns the
# new balance. Use this for credits applied from GDScript, such as a combat or
# contract payout, so the end screen's total includes them. Events apply and
# record their credits in Rust, through EventNode, so they do not call this.
func change_credits(amount: int) -> int:
	var player_state: Node = get_node("/root/PlayerStateGlobal")
	player_state.add_credits(amount)
	_run_state.record_credit_change(amount)
	return player_state.money()


# What the run has achieved so far, for the end screen:
# {combats_won, credits_earned, cards_added, encounters_completed,
#  total_encounters, zones_cleared, total_zones}.
func run_summary() -> Dictionary:
	return _run_state.run_summary()


func run_deck_size() -> int:
	return _run_deck.size()


# Leaves the reward screen for the hub. The encounter was already completed by
# complete_active_encounter, so there is no run state transition left to make.
func finish_reward() -> Dictionary:
	var prepared := _prepare_screen(HUB_SCENE_PATH)
	if not prepared.get("ok", false):
		return prepared

	_replace_screen(prepared["screen"])
	return {"ok": true}


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

	# Clear detection only after a successful caught exit. Normal encounter
	# completion and rejected transitions must preserve the shared meter.
	var noise_meter: Node = get_node("/root/NoiseMeterGlobal")
	noise_meter.add_noise(-noise_meter.noise)
	_replace_screen(prepared["screen"])
	return transition


func run_snapshot() -> Dictionary:
	return _run_state.snapshot()


# Throws the current run away and starts a new one at the hub: a fresh
# contract and offers, no zone progress or stats, noise at 0, knowledge back to
# its starting level and the starter deck. Returns {ok: true}, or
# {ok: false, error} with nothing changed.
#
# Allowed from the hub and the caught screen, where a run ends. Refused during
# an encounter, because its screen would carry on against a run that no longer
# exists. Credits and upgrades on PlayerStateGlobal are the player's, not the
# run's, so they carry over.
func start_new_run() -> Dictionary:
	var prepared := _prepare_screen(HUB_SCENE_PATH)
	if not prepared.get("ok", false):
		return prepared

	var transition: Dictionary = _run_state.start_new_run()
	if not transition.get("ok", false):
		_dispose_screen(prepared["screen"])
		return transition

	_run_deck.reset_to_starter()
	_reset_run_meters()
	_replace_screen(prepared["screen"])
	return transition


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

	# Detection resistance belongs to the construct being faced, so it is
	# cleared on the way out of every screen. The incoming screen's SentryNode
	# sets its own during the add_child below, which is why this has to happen
	# first: a screen with no sentry on it then simply leaves it at zero.
	# Noise itself is deliberately untouched, since it carries across a run.
	var noise_meter: Node = get_node_or_null("/root/NoiseMeterGlobal")
	if noise_meter != null:
		noise_meter.set_resistance_percent(0)

	_screen_host.add_child(screen)


# Clears what the shared meters carried through the old run. Resistance goes
# first because it would blunt the noise reduction, and shield goes too in case
# the run ended partway through a turn with some still up.
func _reset_run_meters() -> void:
	var noise_meter: Node = get_node("/root/NoiseMeterGlobal")
	noise_meter.set_resistance_percent(0)
	noise_meter.clear_shield()
	noise_meter.add_noise(-noise_meter.noise)

	var knowledge_meter: Node = get_node("/root/KnowledgeMeterGlobal")
	knowledge_meter.reset_knowledge()


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
				return _failure(
					"No gameplay scene is registered for encounter type '%s'." % encounter_type
				)
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
