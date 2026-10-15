# Covers story #21: the combat detection meter changes colour with the
# detection level. Checks the band edges directly, then drives the real
# combat screen through each band and watches the meter follow.
#
# Run it with:
#
#     godot --headless --path godot -s res://tests/detection_meter_test.gd
extends SceneTree

const DetectionMeter := preload("res://scenes/detection_meter.gd")

# Long enough for the colour to ease most of the way to a new band's.
const SETTLE_SECONDS := 0.6
# How close the eased colour has to get to the band colour to count as there.
const COLOUR_TOLERANCE := 0.05
const WATCHDOG_SECONDS := 60.0

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
		print("detection meter test: TIMED OUT after %.0fs" % WATCHDOG_SECONDS)
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
		print("detection meter test: PASSED")
		quit(0)
	else:
		print("detection meter test: FAILED (%d)" % _failures.size())
		for failure in _failures:
			print("  - %s" % failure)
		quit(1)


func _colour_distance(a: Color, b: Color) -> float:
	return Vector3(a.r - b.r, a.g - b.g, a.b - b.b).length()


func _check_band_edges() -> void:
	var calm := DetectionMeter.Band.CALM
	var alert := DetectionMeter.Band.ALERT
	var lockdown := DetectionMeter.Band.LOCKDOWN
	var cases := [
		[0, 100, calm],
		[33, 100, calm],
		[34, 100, alert],
		[66, 100, alert],
		[67, 100, lockdown],
		[100, 100, lockdown],
		# A cap that splits exactly into thirds puts each edge in the band above.
		[0, 3, calm],
		[1, 3, alert],
		[2, 3, lockdown],
		# Nothing to fill, so nothing to divide by.
		[0, 0, calm],
		[5, 0, calm],
	]
	for case in cases:
		var got: int = DetectionMeter.band_for(case[0], case[1])
		_check(
			got == case[2],
			"%d / %d is %s (got %s)" % [
				case[0], case[1],
				DetectionMeter.BAND_NAMES[case[2]], DetectionMeter.BAND_NAMES[got],
			]
		)


# Puts the shared meter at an exact level the way the other fixtures do, then
# lets the combat screen refresh its labels as it would after a play.
func _set_noise(noise: int) -> void:
	_combat.sentry.add_noise(noise - _combat.sentry.noise())
	_combat._refresh_status_labels()


func _check_level(noise: int, band: int) -> void:
	_set_noise(noise)
	await create_timer(SETTLE_SECONDS).timeout

	var meter: Control = _combat.detection_meter
	var name: String = DetectionMeter.BAND_NAMES[band]
	_check(meter.current_band_name() == name, "%d noise shows %s" % [noise, name])
	_check(
		meter._header_label.text == "DETECTION // %s" % name,
		"the header names the %s band, not just its colour" % name
	)
	var distance := _colour_distance(meter._shown_colour, DetectionMeter.BAND_COLOURS[band])
	_check(
		distance < COLOUR_TOLERANCE,
		"the meter has eased to the %s colour (off by %.3f)" % [name, distance]
	)
	_check(
		_combat.noise_label.text.begins_with("Noise: %d / " % noise),
		"the numeric readout still shows %d" % noise
	)


func _run() -> void:
	print("Band edges")
	_check_band_edges()

	print("Combat screen")
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

	await _check_level(0, DetectionMeter.Band.CALM)
	await _check_level(50, DetectionMeter.Band.ALERT)
	await _check_level(90, DetectionMeter.Band.LOCKDOWN)
	# Recovering drops the meter back down a band.
	await _check_level(20, DetectionMeter.Band.CALM)

	_finish()
