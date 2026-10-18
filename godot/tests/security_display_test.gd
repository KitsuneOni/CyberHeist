# Covers story #103: during combat the security system is on screen with its
# sprite, its name, its integrity as a bar with current and max values, and an
# icon and value for its next action; and the bar follows damage. Drives the
# real combat screen headless through the coordinator.
#
# Run it with:
#
#     godot --headless --path godot -s res://tests/security_display_test.gd
extends SceneTree

const WATCHDOG_SECONDS := 60.0
# Noise at which standard security switches to its alert actions.
const ALERT_NOISE := 60

var _flow: Node
var _combat: Node
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
		print("security display test: TIMED OUT after %.0fs" % WATCHDOG_SECONDS)
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


func _finish() -> void:
	_finished = true
	if _failures.is_empty():
		print("security display test: PASSED")
		quit(0)
	else:
		print("security display test: FAILED (%d)" % _failures.size())
		for failure in _failures:
			print("  - %s" % failure)
		quit(1)


# The bar can ease towards a new value, so it is read once it has settled.
func _settle() -> void:
	await create_timer(0.8).timeout


func _check_intent_matches_queue(context: String) -> void:
	var intent: Dictionary = _combat.sentry.queued_intent()
	var kind: String = intent.get("kind", "")
	_check(_combat.INTENT_ICONS.has(kind), "%s: the queued move has a known kind (%s)" % [context, kind])
	_check(
		_combat.intent_icon.visible
		and _combat.intent_icon.texture != null
		and _combat.intent_icon.texture.resource_path == _combat.INTENT_ICONS.get(kind, ""),
		"%s: the intent shows the %s icon" % [context, kind]
	)
	var value_format: String = _combat.INTENT_VALUE_FORMATS.get(kind, "")
	if value_format != "":
		var expected: String = value_format % int(intent.get("value", 0))
		_check(
			_combat.intent_value.visible and _combat.intent_value.text == expected,
			"%s: the intent shows its value %s (got '%s')" % [context, expected, _combat.intent_value.text]
		)


func _run() -> void:
	await process_frame
	_flow = root.get_node("FlowCoordinator")
	var main: Node = load("res://scenes/main.tscn").instantiate()
	root.add_child(main)
	await process_frame

	var selected: Dictionary = preload("res://tests/combat_fixture.gd").enter(_flow)
	_check(selected.get("ok", false), "combat fixture loads through the real flow lifecycle")
	if not selected.get("ok", false):
		_finish()
		return
	await process_frame
	_combat = _screen()
	var sentry: Node = _combat.sentry

	print("Every intent icon loads")
	for kind: String in _combat.INTENT_ICONS:
		_check(load(_combat.INTENT_ICONS[kind]) is Texture2D, "the %s icon loads" % kind)
		_check(_combat.INTENT_VALUE_FORMATS.has(kind), "the %s intent says how its value reads" % kind)

	print("Security system is visible during combat")
	_check(_combat.enemy_icon.texture != null, "the construct's sprite is shown")
	_check(_combat.enemy_name.text.begins_with(sentry.construct_name()), "its name is shown")
	_check(
		int(_combat.integrity_bar.max_value) == sentry.max_health()
		and int(_combat.integrity_bar.value) == sentry.health(),
		"its integrity is a bar at %d of %d" % [sentry.health(), sentry.max_health()]
	)
	_check(
		_combat.health_label.text.contains("%d / %d" % [sentry.health(), sentry.max_health()]),
		"with current and max values written beside it"
	)
	_check_intent_matches_queue("the opening turn")

	print("Integrity bar updates on damage")
	var full: int = sentry.health()
	sentry.take_damage(10)
	_combat._refresh_status_labels()
	await _settle()
	_check(
		int(_combat.integrity_bar.value) == full - 10,
		"10 damage takes the bar from %d to %d (got %d)" % [full, full - 10, int(_combat.integrity_bar.value)]
	)
	_check(
		_combat.health_label.text.contains("%d / %d" % [full - 10, sentry.max_health()]),
		"and the values read %d / %d" % [full - 10, sentry.max_health()]
	)

	print("The intent follows the queue")
	_combat._on_end_turn_button_pressed()
	if _screen() != _combat:
		_finish()
		return
	_check_intent_matches_queue("the next turn")

	print("An escalated intent turns red")
	root.get_node("NoiseMeterGlobal").add_noise(ALERT_NOISE)
	_combat._refresh_status_labels()
	_check(sentry.queued_action_escalated(), "loud play puts security on alert")
	_check(_combat.intent_icon.modulate == _combat.RED, "the intent icon turns red")
	_check_intent_matches_queue("on alert")

	print("A long intent wraps rather than spreading into the readout")
	_combat.intent_label.text = "WARDEN-7 will: an extremely long action name (+9 noise, adds 3 Injected Malware to your discard)"
	_combat._fit_intent_label()
	_check(
		_combat.intent_label.get_minimum_size().x <= _combat.INTENT_MAX_WIDTH,
		"the intent line is held to %d px" % _combat.INTENT_MAX_WIDTH
	)
	_combat.intent_label.text = "short"
	_combat._fit_intent_label()
	_check(
		_combat.intent_label.custom_minimum_size.x == 0,
		"a short intent shrinks back to its own width"
	)

	_finish()
