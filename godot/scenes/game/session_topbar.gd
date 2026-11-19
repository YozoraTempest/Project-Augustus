# SPDX-License-Identifier: GPL-3.0-only
extends Control

# 顶栏挂 topbar.gui 这一个组件。控件树留在组件里，按父子坐标摆，不随窗口放大。
const GUI_FILE := preload("res://ui/eu4/eu4_gui_file.gd")
const Eu4Type := preload("res://ui/eu4/eu4_type.gd")
const CREAM := Color(0.96, 0.91, 0.78)
const DEBT := Color(0.86, 0.28, 0.24)

var _placeholders: Dictionary = {}

var _manpower: RichTextLabel

var _gold: RichTextLabel


func setup(directory: String) -> void:
	scale = Vector2.ONE
	mouse_filter = Control.MOUSE_FILTER_STOP
	if GameSession.backend == null:
		return
	_placeholders = GameSession.backend.gui_placeholders()
	var gui = GUI_FILE.new()
	add_child(gui)
	if not gui.open(directory, "interface/topbar.gui", GameSession.backend):
		return
	gui.show_window("topbar")
	_paint_loaded_text(gui)
	var background: Control = gui.find_named("topbar_upper_left_bg") as Control
	if background:
		custom_minimum_size = background.size
		size = background.size
	_gold = gui.find_named("text_gold") as RichTextLabel
	_manpower = gui.find_named("text_manpower") as RichTextLabel
	if not GameSession.clock_changed.is_connected(_refresh):
		GameSession.clock_changed.connect(_refresh)
	if not GameSession.world_changed.is_connected(_refresh):
		GameSession.world_changed.connect(_refresh)
	_show_gold_frame(1)
	_refresh()


func _paint_loaded_text(node: Node) -> void:
	if node is RichTextLabel:
		var size: int = node.get_theme_font_size("normal_font_size")
		Eu4Type.paint_rich_text(node, size)
		node.add_theme_color_override("default_color", CREAM)
	for child in node.get_children():
		_paint_loaded_text(child)


func _find_named(node: Node, eu4_name: String) -> Node:
	if node != null and str(node.get_meta("eu4_name", "")) == eu4_name:
		return node
	if node == null:
		return null
	for child in node.get_children():
		var found := _find_named(child, eu4_name)
		if found:
			return found
	return null


func _show_gold_frame(frame_number: int) -> void:
	var icon := _find_named(self, "icon_gold") as TextureRect
	if icon == null or icon.texture == null or GameSession.backend == null:
		return
	var full: Texture2D = icon.get_meta("eu4_gold_sheet", icon.texture)
	if not icon.has_meta("eu4_gold_sheet"):
		icon.set_meta("eu4_gold_sheet", icon.texture)
		full = icon.texture
	var sprite := str(icon.get_meta("eu4_sprite", "GFX_icon_gold_strip"))
	var info: Dictionary = GameSession.backend.resolve_eu4_sprite(sprite)
	var frames := int(info.get("frames", 1))
	if frames < 1:
		frames = 1
	# 1 基帧号先夹进 1…noOfFrames，再减 1。默认按宽度水平切开，不画整条。
	var clamped := clampi(frame_number, 1, frames)
	var index := clamped - 1
	var frame_width := int(full.get_width() / frames)
	var frame_height := full.get_height()
	var region := AtlasTexture.new()
	region.atlas = full
	region.region = Rect2(index * frame_width, 0, frame_width, frame_height)
	icon.texture = region
	icon.custom_minimum_size = region.region.size
	icon.size = region.region.size


func _apply_placeholders() -> void:
	for node_name: String in _placeholders:
		var label := _find_named(self, node_name) as RichTextLabel
		if label:
			label.text = str(_placeholders[node_name])


func _refresh() -> void:
	if _gold == null:
		return
	var country := GameSession.get_player_country()
	if country.is_empty() or GameSession.backend == null:
		return
	var finances: Dictionary = GameSession.backend.get_country_finances(country)
	var negative := bool(finances.get("treasury_negative", false))
	_gold.text = str(finances.get("treasury", "0"))
	_gold.add_theme_color_override("default_color", DEBT if negative else CREAM)
	# 非负画第 1 帧（左半），为负画第 2 帧（右半）。差额暂用国库正负，见 gui_mock_list.md。
	_show_gold_frame(2 if negative else 1)
	if _manpower:
		var manpower: Dictionary = GameSession.backend.get_country_manpower(country)
		_manpower.text = str(manpower.get("available", 0))
	_apply_placeholders()
