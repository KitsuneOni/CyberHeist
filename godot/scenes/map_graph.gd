# Draws the contract as a node graph: shapes for encounters, lines for the
# routes between them, laid out from the entry on the left to the target on
# the right.
#
# Positions and connections come from contract_map.rs via
# FlowCoordinator.map_progress(); nothing about the layout is decided here.
extends Control

signal node_selected(node_id: int)

const NODE_RADIUS := 20.0
const TARGET_HALF := 18.0
# A boss is drawn larger than the encounters around it, so where a zone ends
# is legible from the shape of the map alone.
const BOSS_HALF := 24.0
const LINE_WIDTH := 2.0

# A long contract is scrolled sideways rather than squeezed, so the graph
# claims a fixed width per column and lets the viewport move over it.
const COLUMN_WIDTH := 240.0
const ROW_HEIGHT := 84.0

# One colour per status, matching the rest of the hub.
const STATUS_COLOURS := {
	"completed": Color(0.55, 0.82, 0.6, 1),
	"current": Color(0.95, 0.95, 0.98, 1),
	"available": Color(0.55, 0.78, 0.92, 1),
	"locked": Color(0.55, 0.42, 0.45, 1),
	# Amber rather than the dead red of "locked": a sealed node is waiting on
	# the zone's boss, not ruled out for good.
	"sealed": Color(0.85, 0.66, 0.35, 1),
	"upcoming": Color(0.5, 0.55, 0.6, 1),
}
const LINE_COLOUR := Color(0.32, 0.38, 0.44, 1)
const LINE_COLOUR_TAKEN := Color(0.55, 0.82, 0.6, 0.9)
const FILL_DIM := Color(0.09, 0.11, 0.14, 1)

var _nodes: Array = []
var _hovered_id: int = -1


func set_nodes(nodes: Array) -> void:
	_nodes = nodes
	_hovered_id = -1
	tooltip_text = ""
	_resize_to_contract()
	queue_redraw()


# The graph asks for as much room as the contract needs; the ScrollContainer
# around it decides how much of that is on screen at once.
func _resize_to_contract() -> void:
	var columns := {}
	var widest := 1
	for node: Dictionary in _nodes:
		var key := int(round(float(node.get("x", 0.0)) * 1000.0))
		columns[key] = int(columns.get(key, 0)) + 1
		widest = maxi(widest, int(columns[key]))

	custom_minimum_size = Vector2(
		maxf(float(columns.size()) * COLUMN_WIDTH, COLUMN_WIDTH),
		maxf(float(widest) * ROW_HEIGHT, ROW_HEIGHT * 2.0)
	)


func _node_by_id(node_id: int) -> Dictionary:
	for node: Dictionary in _nodes:
		if int(node.get("id", -1)) == node_id:
			return node
	return {}


# The space the graph occupies once the container has laid it out.
#
# `size` only catches up with a newly requested `custom_minimum_size` after the
# parent re-sorts its children, so reading it in the same frame as set_nodes()
# measures the previous, narrower contract. Taking whichever is larger gives the
# right answer both before and after layout settles: the container never makes a
# child smaller than its minimum, and it may stretch it beyond that.
func _layout_size() -> Vector2:
	return Vector2(
		maxf(size.x, custom_minimum_size.x),
		maxf(size.y, custom_minimum_size.y),
	)


# Maps the 0..1 layout onto the space available, keeping a node's worth of
# padding so nothing clips at the edges.
func _position_of(node: Dictionary) -> Vector2:
	var pad := NODE_RADIUS * 1.6
	var space := _layout_size()
	return Vector2(
		pad + float(node.get("x", 0.0)) * maxf(space.x - pad * 2.0, 1.0),
		pad + float(node.get("y", 0.5)) * maxf(space.y - pad * 2.0, 1.0)
	)


# Pixel x of the node the player is standing on, or -1 when there is none.
# The hub uses this to keep the current position in view as the run moves
# right across a long contract.
func current_node_x() -> float:
	for node: Dictionary in _nodes:
		if bool(node.get("is_current", false)):
			return _position_of(node).x
	return -1.0

func _is_selectable(node: Dictionary) -> bool:
	return str(node.get("status", "")) == "available"


func _draw() -> void:
	if _nodes.is_empty():
		return

	# Routes first, so the nodes sit on top of them.
	for node: Dictionary in _nodes:
		var from := _position_of(node)
		var from_done := str(node.get("status", "")) == "completed"
		for connection in node.get("connections", []):
			var target := _node_by_id(int(connection))
			if target.is_empty():
				continue
			# A line reads as travelled only when both ends are behind you.
			var taken := from_done and str(target.get("status", "")) == "completed"
			draw_line(
				from,
				_position_of(target),
				LINE_COLOUR_TAKEN if taken else LINE_COLOUR,
				LINE_WIDTH
			)

	var font := get_theme_default_font()
	var font_size := 13

	for node: Dictionary in _nodes:
		var centre := _position_of(node)
		var status := str(node.get("status", ""))
		var colour: Color = STATUS_COLOURS.get(status, Color.WHITE)
		var node_id := int(node.get("id", -1))

		# Cleared nodes are filled in; everything else is an outline.
		var fill := colour if status == "completed" else FILL_DIM
		var outline_width := 3.0 if node_id == _hovered_id else 2.0

		# Bosses are diamonds, the fixed points of a run are squares, and the
		# encounters you choose between are circles.
		if bool(node.get("is_boss", false)):
			var diamond := PackedVector2Array([
				centre + Vector2(0.0, -BOSS_HALF),
				centre + Vector2(BOSS_HALF, 0.0),
				centre + Vector2(0.0, BOSS_HALF),
				centre + Vector2(-BOSS_HALF, 0.0),
			])
			draw_colored_polygon(diamond, fill)
			var outline := diamond.duplicate()
			outline.append(diamond[0])
			draw_polyline(outline, colour, outline_width)
		elif bool(node.get("is_entry", false)) or bool(node.get("is_target", false)):
			var box := Rect2(
				centre - Vector2(TARGET_HALF, TARGET_HALF),
				Vector2(TARGET_HALF * 2.0, TARGET_HALF * 2.0)
			)
			draw_rect(box, fill, true)
			draw_rect(box, colour, false, outline_width)
		else:
			draw_circle(centre, NODE_RADIUS, fill)
			draw_arc(centre, NODE_RADIUS, 0.0, TAU, 32, colour, outline_width)

		# A second ring marks where the player is standing. Read from its own
		# flag, since a cleared node is both completed and where they are.
		if bool(node.get("is_current", false)):
			draw_arc(centre, NODE_RADIUS + 5.0, 0.0, TAU, 32, colour, 1.5)

		var marker := str(node.get("type_marker", "")).strip_edges()
		if marker.begins_with("[") and marker.ends_with("]"):
			marker = marker.substr(1, marker.length() - 2)
		if font != null:
			var text_size := font.get_string_size(marker, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size)
			var text_colour := FILL_DIM if status == "completed" else colour
			draw_string(
				font,
				centre + Vector2(-text_size.x * 0.5, text_size.y * 0.32),
				marker,
				HORIZONTAL_ALIGNMENT_LEFT,
				-1,
				font_size,
				text_colour
			)


func _gui_input(event: InputEvent) -> void:
	if event is InputEventMouseMotion:
		var hovered := _node_at(event.position)
		var hovered_id := int(hovered.get("id", -1)) if not hovered.is_empty() else -1
		if hovered_id != _hovered_id:
			_hovered_id = hovered_id
			_update_tooltip(hovered)
			queue_redraw()
	elif event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
		var clicked := _node_at(event.position)
		if not clicked.is_empty() and _is_selectable(clicked):
			node_selected.emit(int(clicked.get("id", -1)))


func _node_at(point: Vector2) -> Dictionary:
	for node: Dictionary in _nodes:
		if point.distance_to(_position_of(node)) <= NODE_RADIUS + 4.0:
			return node
	return {}


# Enough to plan a route, without giving away what is waiting inside.
func _update_tooltip(node: Dictionary) -> void:
	if node.is_empty():
		tooltip_text = ""
		return

	var lines: Array[String] = [
		"%s — %s" % [str(node.get("type_name", "?")), str(node.get("status", ""))],
		str(node.get("description", "")),
	]
	if str(node.get("status", "")) == "sealed":
		lines.append("Sealed until the boss of zone %d is beaten." % int(node.get("zone", 0)))
	if _is_selectable(node):
		lines.append("Click to start this encounter.")
	tooltip_text = "\n".join(lines)
