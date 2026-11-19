# SPDX-License-Identifier: GPL-3.0-only
extends Control

# 挂 interface/eventwindow.gui。用 eu4_gui_file 按文件加载，显示其中的窗口，先不接玩法。
const GUI_FILE := preload("res://ui/eu4/eu4_gui_file.gd")
const SOURCE := "interface/eventwindow.gui"

var gui
var windows := PackedStringArray()
var window_index := 0

var _caption: Label
var _previous: Button
var _next: Button


func _ready() -> void:
	if get_parent() == get_tree().root:
		AppSettings.apply_game_window()
	set_anchors_preset(Control.PRESET_FULL_RECT)
	set_offsets_preset(Control.PRESET_FULL_RECT)
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	set_meta("eu4_source", SOURCE)
	add_to_group("eu4_mounted_gui")
	var background := ColorRect.new()
	background.color = Color(0.035, 0.047, 0.063)
	background.set_anchors_preset(Control.PRESET_FULL_RECT)
	background.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(background)
	_caption = Label.new()
	_caption.position = Vector2(12, 8)
	_caption.add_theme_color_override("font_color", Color(0.96, 0.91, 0.78))
	_caption.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(_caption)
	_previous = Button.new()
	_previous.text = "上一个窗口"
	_previous.position = Vector2(12, 36)
	_previous.custom_minimum_size = Vector2(120, 32)
	_previous.pressed.connect(_step.bind(-1))
	add_child(_previous)
	_next = Button.new()
	_next.text = "下一个窗口"
	_next.position = Vector2(144, 36)
	_next.custom_minimum_size = Vector2(120, 32)
	_next.pressed.connect(_step.bind(1))
	add_child(_next)
	if gui != null:
		if windows.is_empty():
			_caption.text = SOURCE
			_set_switcher_visible(false)
		else:
			show_window(windows[window_index])
		return
	var directory := GameSession.game_directory.strip_edges()
	if directory.is_empty():
		directory = AppSettings.eu4_directory.strip_edges()
	if directory.is_empty() or not DirAccess.dir_exists_absolute(directory):
		_caption.text = "请先在启动器选择安装目录。"
		_set_switcher_visible(false)
		return
	var backend: AugustusMap = GameSession.backend if GameSession.backend != null else AugustusMap.new()
	if not open_file(directory, backend):
		_caption.text = "无法读取 " + SOURCE
		_set_switcher_visible(false)
		return
	if windows.is_empty():
		_caption.text = SOURCE
		_set_switcher_visible(false)
		return
	show_window(windows[0])


func open_file(directory: String, backend: AugustusMap) -> bool:
	if gui != null:
		gui.queue_free()
		gui = null
	gui = GUI_FILE.new()
	add_child(gui)
	if not gui.open(directory, SOURCE, backend):
		gui.queue_free()
		gui = null
		windows = PackedStringArray()
		return false
	windows = _collect_windows(gui)
	window_index = 0
	_raise_chrome()
	return true


func show_window(window_name: String) -> void:
	if gui == null:
		return
	gui.show_window(window_name)
	var found := windows.find(window_name)
	if found >= 0:
		window_index = found
	if _caption:
		_caption.text = "%s · %s（%d/%d）" % [SOURCE, window_name, window_index + 1, maxi(windows.size(), 1)]
	_set_switcher_visible(windows.size() > 1)
	_raise_chrome()


func _step(delta: int) -> void:
	if windows.is_empty():
		return
	window_index = posmod(window_index + delta, windows.size())
	show_window(windows[window_index])


func _unhandled_input(event: InputEvent) -> void:
	if windows.size() < 2 or not event is InputEventKey:
		return
	var key := event as InputEventKey
	if not key.pressed or key.echo:
		return
	if key.keycode == KEY_RIGHT or key.keycode == KEY_DOWN:
		_step(1)
	elif key.keycode == KEY_LEFT or key.keycode == KEY_UP:
		_step(-1)


func _set_switcher_visible(shown: bool) -> void:
	if _previous:
		_previous.visible = shown
	if _next:
		_next.visible = shown


func _raise_chrome() -> void:
	for node in [_caption, _previous, _next]:
		if node != null and is_instance_valid(node) and node.get_parent() == self:
			move_child(node, get_child_count() - 1)


func _collect_windows(node: Node) -> PackedStringArray:
	var names := PackedStringArray()
	_walk(node, names)
	return names


func _walk(node: Node, names: PackedStringArray) -> void:
	if node != gui:
		var kind := str(node.get_meta("eu4_kind", ""))
		if (kind == "window" or kind == "container_window") and node.has_meta("eu4_name"):
			var eu4_name := str(node.get_meta("eu4_name"))
			if not eu4_name.is_empty() and not names.has(eu4_name):
				names.append(eu4_name)
	for child in node.get_children():
		_walk(child, names)
