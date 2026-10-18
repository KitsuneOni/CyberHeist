# The combat screen's motion (story #45): cards flying out of the hand,
# the hand dealing in and closing up, hover lift, the enemy taking a hit, and
# numbers floating off whatever changed. Nothing here touches game state; the
# screen has already applied every result before it asks for an effect, so
# an effect cut short by the screen closing loses nothing.
#
# Ghost cards and floating numbers live on this full-screen layer rather than
# in the hand, so rebuilding the hand never cuts a flight short. Each tween
# belongs to the node it moves, so it dies with that node.
extends Control

const FLY_SECONDS := 0.32
const DEAL_SECONDS := 0.22
const DEAL_STAGGER := 0.05
# How far below its resting place a card starts when it is dealt.
const DEAL_DROP := 90.0
const SLIDE_SECONDS := 0.18
const LIFT_SECONDS := 0.1
const FLOAT_SECONDS := 0.9
const FLOAT_RISE := 48.0
const SHAKE_SECONDS := 0.25
const SHAKE_DISTANCE := 7.0
const PUNCH_SECONDS := 0.2
const PUNCH_SCALE := 1.3

const HIT_FLASH := Color(1, 0.35, 0.4, 1)

# Meta keys for the tween currently driving one axis of a node, so a new move
# on that axis replaces the old one instead of fighting it.
const Y_TWEEN := &"fx_y_tween"
const X_TWEEN := &"fx_x_tween"
const TINT_TWEEN := &"fx_tint_tween"
# A node's own tint, which a flash fades back to.
const REST_TINT := &"fx_rest_tint"
# Anchors closer than this to the top of the screen float their numbers
# downward instead, so they are not cut off.
const FLOAT_DOWN_ABOVE := 90.0


func _init() -> void:
	name = "CombatFx"
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	# Drawn over the hand and the arena, in screen coordinates.
	top_level = true
	z_index = 10


func _ready() -> void:
	set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)


# A copy of `card` flies from where it sits to `target` (a global point),
# shrinking and fading as it goes. `spin` tips it a little on the way.
func fly_card(card: Control, target: Vector2, delay := 0.0, spin := 0.0) -> void:
	var ghost := _ghost_of(card)
	if ghost == null:
		return
	var end := target - ghost.size * 0.25 - global_position
	var tween := ghost.create_tween().set_parallel()
	tween.tween_property(ghost, "position", end, FLY_SECONDS).set_delay(delay) \
		.set_trans(Tween.TRANS_CUBIC).set_ease(Tween.EASE_IN)
	tween.tween_property(ghost, "scale", Vector2(0.5, 0.5), FLY_SECONDS).set_delay(delay)
	tween.tween_property(ghost, "rotation", spin, FLY_SECONDS).set_delay(delay)
	tween.tween_property(ghost, "modulate:a", 0.0, FLY_SECONDS * 0.6) \
		.set_delay(delay + FLY_SECONDS * 0.4)
	tween.chain().tween_callback(ghost.queue_free)


# A copy of `card` swells and fades where it stands: a card that leaves the
# encounter rather than going to the discard pile.
func dissolve_card(card: Control) -> void:
	var ghost := _ghost_of(card)
	if ghost == null:
		return
	var tween := ghost.create_tween().set_parallel()
	tween.tween_property(ghost, "scale", Vector2(1.35, 1.35), FLY_SECONDS)
	tween.tween_property(ghost, "modulate", Color(HIT_FLASH, 0.0), FLY_SECONDS)
	tween.chain().tween_callback(ghost.queue_free)


# `card` rises into its resting height `rest_y` from below, fading in, after
# `delay`. Hovering it part way through takes over the height straight away.
func deal_in(card: Control, rest_y: float, delay: float) -> void:
	card.position.y = rest_y + DEAL_DROP
	card.modulate.a = 0.0
	_move_axis(card, Y_TWEEN, "position:y", rest_y, DEAL_SECONDS, delay) \
		.set_trans(Tween.TRANS_BACK).set_ease(Tween.EASE_OUT)
	var fade := card.create_tween()
	fade.tween_property(card, "modulate:a", 1.0, DEAL_SECONDS).set_delay(delay)


# `card` has just been laid out somewhere new; it slides there from the
# global x it used to sit at.
func slide_from(card: Control, old_global_x: float) -> void:
	card.position.x = old_global_x - card.get_parent().global_position.x
	_move_axis(card, X_TWEEN, "position:x", 0.0, SLIDE_SECONDS, 0.0) \
		.set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)


# Eases `card` to height `y`, replacing any rise already under way.
func lift_to(card: Control, y: float) -> void:
	_move_axis(card, Y_TWEEN, "position:y", y, LIFT_SECONDS, 0.0) \
		.set_ease(Tween.EASE_OUT)


# `target` shakes side to side and flashes red: it has just taken damage.
func hit(target: Control) -> void:
	var tween := target.create_tween()
	var base := target.position
	var step := SHAKE_SECONDS / 5.0
	for offset in [SHAKE_DISTANCE, -SHAKE_DISTANCE, SHAKE_DISTANCE * 0.5, -SHAKE_DISTANCE * 0.5]:
		tween.tween_property(target, "position:x", base.x + offset, step)
	tween.tween_property(target, "position:x", base.x, step)
	flash(target, HIT_FLASH)


# `target` is tinted `colour` for a moment, then fades back to its own tint.
# The tint is remembered from the first flash, so a flash landing on another
# one part way through does not mistake the half-faded colour for it.
func flash(target: CanvasItem, colour: Color) -> void:
	if not target.has_meta(REST_TINT):
		target.set_meta(REST_TINT, target.modulate)
	var rest: Color = target.get_meta(REST_TINT)
	target.modulate = colour
	_move_axis(target, TINT_TWEEN, "modulate", rest, SHAKE_SECONDS * 1.6, 0.0)


# `target` swells and settles about its centre: its number just changed.
func punch(target: Control) -> void:
	target.pivot_offset = target.size / 2.0
	var tween := target.create_tween()
	tween.tween_property(target, "scale", Vector2(PUNCH_SCALE, PUNCH_SCALE), PUNCH_SECONDS * 0.4) \
		.set_ease(Tween.EASE_OUT)
	tween.tween_property(target, "scale", Vector2.ONE, PUNCH_SECONDS * 0.6) \
		.set_ease(Tween.EASE_IN)


# `text` in `colour` rises off `anchor` and fades out. Several at once on the
# same anchor are stepped up so they do not sit on top of each other.
func float_text(text: String, anchor: Control, colour: Color) -> Label:
	var label := Label.new()
	label.text = text
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	label.add_theme_font_size_override("font_size", 22)
	label.add_theme_color_override("font_color", colour)
	label.add_theme_color_override("font_outline_color", Color.BLACK)
	label.add_theme_constant_override("outline_size", 6)
	add_child(label)

	var stacked := _floats_over(anchor)
	label.set_meta(&"fx_anchor", anchor.get_instance_id())
	var rect := anchor.get_global_rect()
	var downward := rect.position.y < FLOAT_DOWN_ABOVE
	var step := 26.0 if downward else -26.0
	var start := Vector2(
		rect.get_center().x - label.get_minimum_size().x / 2.0,
		(rect.end.y + 4.0 if downward else rect.position.y - 8.0) + stacked * step,
	) - global_position
	label.position = start

	var rise := FLOAT_RISE if downward else -FLOAT_RISE
	var tween := label.create_tween().set_parallel()
	tween.tween_property(label, "position:y", start.y + rise, FLOAT_SECONDS) \
		.set_ease(Tween.EASE_OUT)
	tween.tween_property(label, "modulate:a", 0.0, FLOAT_SECONDS * 0.5) \
		.set_delay(FLOAT_SECONDS * 0.5)
	tween.chain().tween_callback(label.queue_free)
	return label


# Moves `bar` to `value`, draining or filling rather than jumping.
func ease_bar(bar: Range, value: float) -> void:
	if is_equal_approx(bar.value, value):
		return
	_move_axis(bar, X_TWEEN, "value", value, SHAKE_SECONDS * 1.6, 0.0) \
		.set_ease(Tween.EASE_OUT)


# Ghost cards and floating numbers still on screen.
func active_effects() -> int:
	var count := 0
	for child in get_children():
		if not child.is_queued_for_deletion():
			count += 1
	return count


func _ghost_of(card: Control) -> Control:
	if not is_instance_valid(card) or not card.is_inside_tree():
		return null
	# Signals stay behind, so the ghost cannot be clicked or hovered.
	var ghost: Control = card.duplicate(0)
	ghost.mouse_filter = Control.MOUSE_FILTER_IGNORE
	ghost.modulate = card.modulate
	ghost.pivot_offset = card.size / 2.0
	add_child(ghost)
	ghost.size = card.size
	ghost.position = card.global_position - global_position
	return ghost


func _floats_over(anchor: Control) -> int:
	var count := 0
	for child in get_children():
		if (
			not child.is_queued_for_deletion()
			and child.get_meta(&"fx_anchor", 0) == anchor.get_instance_id()
		):
			count += 1
	return count


func _move_axis(
	target: Node, key: StringName, property: String, to: Variant, seconds: float, delay: float
) -> PropertyTweener:
	if target.has_meta(key):
		var running: Tween = target.get_meta(key)
		if running.is_valid():
			running.kill()
	var tween := target.create_tween()
	target.set_meta(key, tween)
	return tween.tween_property(target, property, to, seconds).set_delay(delay)
