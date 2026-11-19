# SPDX-License-Identifier: GPL-3.0-only
extends Control

# 一个 .gui 文件对应一个组件。文件里的控件树挂在本组件下面，子控件按文件里的坐标相对父控件摆放，不随窗口缩放。
signal action_requested(eu4_name: String, properties: Dictionary)

const BUILDER_SCRIPT := preload("res://ui/eu4/eu4_ui_builder.gd")
const ASSETS_SCRIPT := preload("res://ui/eu4/eu4_ui_assets.gd")

var source_file := ""
# 画面要看的那一棵窗口。查找名字时从这里往下，避免命中文件里另一个同名控件。
var focus: Node
var diagnostics: PackedStringArray = PackedStringArray()


func open(directory: String, relative: String, backend: AugustusMap) -> bool:
	source_file = relative
	focus = self
	set_anchors_preset(Control.PRESET_FULL_RECT)
	offset_left = 0.0
	offset_top = 0.0
	offset_right = 0.0
	offset_bottom = 0.0
	scale = Vector2.ONE
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	if directory.is_empty() or backend == null:
		return false
	var loaded: Dictionary = backend.load_eu4_ui_document(directory, relative)
	if loaded.get("ok", false) != true:
		return false
	var parsed: Variant = JSON.parse_string(str(loaded.get("json", "")))
	if typeof(parsed) != TYPE_DICTIONARY:
		return false
	var assets = ASSETS_SCRIPT.new()
	assets.backend = backend
	var builder = BUILDER_SCRIPT.new()
	builder.assets = assets
	builder.plain_windows = true
	builder.action_requested.connect(_relay_action)
	builder.build_document(parsed, self)
	diagnostics = builder.diagnostics
	_pass_clicks(self)
	return true


# 只显示这条窗口和它的父链。其余窗口仍是本组件的子节点，但不画出来。
func show_window(window_name: String) -> void:
	var target: Node = _search(self, window_name)
	if target == null:
		focus = self
		return
	focus = target
	var on_path: Dictionary = {}
	var node: Node = target
	while node != null and node != self:
		on_path[node] = true
		node = node.get_parent()
	_keep_path(self, on_path, target)


func find_named(eu4_name: String) -> Node:
	var root: Node = focus if focus != null else self
	return _search(root, eu4_name)


func _relay_action(eu4_name: String, properties: Dictionary) -> void:
	action_requested.emit(eu4_name, properties)


func _search(node: Node, eu4_name: String) -> Node:
	if node != self and node.has_meta("eu4_name") and str(node.get_meta("eu4_name")) == eu4_name:
		return node
	for child in node.get_children():
		var found: Node = _search(child, eu4_name)
		if found != null:
			return found
	return null


func _keep_path(node: Node, on_path: Dictionary, target: Node) -> void:
	for child in node.get_children():
		if not child is CanvasItem:
			continue
		var item: CanvasItem = child as CanvasItem
		if child == target:
			item.visible = true
			continue
		if on_path.has(child):
			item.visible = true
			_keep_path(child, on_path, target)
		else:
			item.visible = false


func _pass_clicks(node: Node) -> void:
	if node != self and node is Control and not node is BaseButton and not node is LineEdit:
		(node as Control).mouse_filter = Control.MOUSE_FILTER_IGNORE
	for child in node.get_children():
		_pass_clicks(child)
