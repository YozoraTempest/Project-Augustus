# SPDX-License-Identifier: GPL-3.0-only
class_name ArmyMarkers
extends Control

signal army_selected(army_id: int)

var project_point: Callable
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
			add_child(button)
			_buttons[id] = button
		var button: Button = _buttons[id]
		button.text = "军 %s · %d" % [army.country, army.strength]
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
	for army in _armies:
		var button: Button = _buttons[army.id]
		var point: Vector2 = project_point.call(army.position)
		button.visible = Rect2(Vector2.ZERO, size).has_point(point)
		if button.visible:
			button.position = point - button.size * 0.5
