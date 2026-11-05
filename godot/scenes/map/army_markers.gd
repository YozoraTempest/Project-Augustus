# SPDX-License-Identifier: GPL-3.0-only
class_name ArmyMarkers
extends Control

signal army_selected(army_id: int)
signal province_targeted(province_id: int)

var project_point: Callable
var project_path: Callable
var _armies: Array[Dictionary] = []
var _buttons: Dictionary[int, Button] = {}
var _selected := 0


func show_armies(armies: Array[Dictionary]) -> void:
	_armies = armies
	var present: Dictionary[int, bool] = {}
	for army in armies:
		var id: int = army.id
		present[id] = true
		if not _buttons.has(id):
			var button := Button.new()
			button.toggle_mode = true
			button.focus_mode = Control.FOCUS_NONE
			button.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
			button.pressed.connect(func(): army_selected.emit(id))
			button.gui_input.connect(func(event: InputEvent):
				if event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_RIGHT:
					province_targeted.emit(int(button.get_meta("province")))
					button.accept_event())
			add_child(button)
			_buttons[id] = button
		var button: Button = _buttons[id]
		button.set_meta("province", army.province)
		button.text = "%s %s · %d" % ["→" if army.movement != null else "军", army.country, army.strength]
		button.tooltip_text = "%s\n%s · %d 团 · %d 人" % [army.name, army.province_name, army.regiment_count, army.strength]
		button.add_theme_color_override("font_color", Color.WHITE.lerp(army.color, 0.35))
		button.set_pressed_no_signal(id == _selected)
		button.reset_size()
	for id: int in _buttons.keys():
		if not present.has(id):
			_buttons[id].queue_free()
			_buttons.erase(id)
	_update_positions()


func select_army(id: int) -> void:
	_selected = id
	for army_id: int in _buttons:
		_buttons[army_id].set_pressed_no_signal(army_id == id)


func _process(_delta: float) -> void:
	_update_positions()


func _update_positions() -> void:
	if not project_point.is_valid():
		return
	var stacks: Dictionary[Vector2i, int] = {}
	for army in _armies:
		var button: Button = _buttons[army.id]
		var point: Vector2 = project_point.call(army.position) if army.movement == null else project_path.call(army.position, army.destination_position, army.movement.leg_progress / 100.0)
		button.visible = Rect2(Vector2.ZERO, size).has_point(point)
		if button.visible:
			var stack := Vector2i(point.round())
			var index: int = stacks.get(stack, 0)
			stacks[stack] = index + 1
			# 多支军队抵达同一省份时分别显示，保持每支军队都能选中。
			button.position = point - button.size * 0.5 + Vector2(0, index * (button.size.y + 2))
	queue_redraw()


func _draw() -> void:
	if not project_path.is_valid():
		return
	for army in _armies:
		if army.id != _selected or army.movement == null:
			continue
		var movement: Dictionary = army.movement
		for index in movement.route.size():
			var leg: Dictionary = movement.route[index]
			_draw_leg(leg.from_position, leg.to_position, index, movement)
		var last: Dictionary = movement.route.back()
		var end: Vector2 = project_path.call(last.from_position, last.to_position, 1.0)
		var before: Vector2 = project_path.call(last.from_position, last.to_position, 0.95)
		if end.is_finite() and before.is_finite() and not end.is_equal_approx(before):
			var direction := (end - before).normalized()
			for angle in [-0.5, 0.5]:
				draw_line(end, end - direction.rotated(angle) * 12.0, Color(1.0, 0.81, 0.3), 3.0, true)


func _draw_leg(from: Vector2, to: Vector2, index: int, movement: Dictionary) -> void:
	var previous: Vector2 = project_path.call(from, to, 0.0)
	for step in range(1, 33):
		var fraction := step / 32.0
		var point: Vector2 = project_path.call(from, to, fraction)
		# 每段独立投影；背面及接缝处断开，避免穿过球体或横跨整个视口。
		if previous.is_finite() and point.is_finite() and previous.distance_to(point) < size.length() * 0.5:
			var travelled: bool = index < movement.leg_index or (index == movement.leg_index and fraction <= movement.leg_progress / 100.0)
			var color := Color(1.0, 0.81, 0.3) if travelled else Color(0.9, 0.9, 0.9, 0.8)
			draw_line(previous, point, Color(0, 0, 0, 0.8), 6.0, true)
			draw_line(previous, point, color, 3.0, true)
		previous = point
