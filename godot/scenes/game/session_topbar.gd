# SPDX-License-Identifier: GPL-3.0-only
extends Control

# 顶栏的底图、金币和三项点数从 topbar.gui 加载。坐标和字号用文件里的值，不随窗口放大。
const ASSETS_SCRIPT := preload("res://ui/eu4/eu4_ui_assets.gd")
const BUILDER_SCRIPT := preload("res://ui/eu4/eu4_ui_builder.gd")
const Eu4Type := preload("res://ui/eu4/eu4_type.gd")
const CREAM := Color(0.96, 0.91, 0.78)
const DEBT := Color(0.86, 0.28, 0.24)
const NODE_NAMES := [
	"topbar_upper_left_bg",
	"icon_gold",
	"text_gold",
	"icon_ADM",
	"text_ADM",
	"icon_DIP",
	"text_DIP",
	"icon_MIL",
	"text_MIL",
]

var _gold: RichTextLabel
var _adm: RichTextLabel
var _dip: RichTextLabel
var _mil: RichTextLabel


func setup(directory: String) -> void:
	scale = Vector2.ONE
	mouse_filter = Control.MOUSE_FILTER_STOP
	if GameSession.backend == null:
		return
	var loaded: Dictionary = GameSession.backend.load_eu4_ui_document(directory, "interface/topbar.gui")
	if loaded.get("ok", false) != true:
		return
	var document: Variant = JSON.parse_string(str(loaded.get("json", "")))
	if typeof(document) != TYPE_DICTIONARY:
		return
	var assets = ASSETS_SCRIPT.new()
	assets.backend = GameSession.backend
	var builder = BUILDER_SCRIPT.new()
	builder.assets = assets
	builder.plain_windows = true
	var host := Control.new()
	host.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var window := _find_json(document, "topbar")
	var origin := _vec(window.get("position", {}) if window is Dictionary else {})
	host.position = origin
	add_child(host)
	for node_name in NODE_NAMES:
		builder.build_named(document, node_name, host)
	_paint_loaded_text(host)
	var background := _find_named(host, "topbar_upper_left_bg") as Control
	if background:
		custom_minimum_size = background.size
		size = background.size
	_gold = _find_named(host, "text_gold") as RichTextLabel
	_adm = _find_named(host, "text_ADM") as RichTextLabel
	_dip = _find_named(host, "text_DIP") as RichTextLabel
	_mil = _find_named(host, "text_MIL") as RichTextLabel
	if not GameSession.clock_changed.is_connected(_refresh):
		GameSession.clock_changed.connect(_refresh)
	if not GameSession.world_changed.is_connected(_refresh):
		GameSession.world_changed.connect(_refresh)
	_refresh()


func _paint_loaded_text(node: Node) -> void:
	if node is RichTextLabel:
		var size := node.get_theme_font_size("normal_font_size")
		Eu4Type.paint_rich_text(node, size)
		node.add_theme_color_override("default_color", CREAM)
	for child in node.get_children():
		_paint_loaded_text(child)


func _find_json(node: Variant, wanted: String) -> Variant:
	if node is Dictionary:
		if str(node.get("name", "")) == wanted:
			return node
		for child: Variant in node.get("children", []):
			var found := _find_json(child, wanted)
			if found != null:
				return found
		for root: Variant in node.get("roots", []):
			var found := _find_json(root, wanted)
			if found != null:
				return found
	return null


func _vec(value: Variant) -> Vector2:
	if value is Dictionary:
		return Vector2(_number(value.get("x", 0)), _number(value.get("y", 0)))
	return Vector2.ZERO


func _number(value: Variant) -> float:
	match typeof(value):
		TYPE_FLOAT, TYPE_INT:
			return float(value)
		TYPE_STRING:
			return float(value) if value.is_valid_float() or value.is_valid_int() else 0.0
		_:
			return 0.0


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


func _refresh() -> void:
	if _gold == null:
		return
	var country := GameSession.get_player_country()
	if country.is_empty() or GameSession.backend == null:
		return
	var finances: Dictionary = GameSession.backend.get_country_finances(country)
	_gold.text = str(finances.get("treasury", "0"))
	_gold.add_theme_color_override("default_color", DEBT if finances.get("treasury_negative", false) else CREAM)
	# 君主点数还没有进入模拟，库存按开局 0 显示，不把国家代码写出来。
	if _adm:
		_adm.text = "0"
	if _dip:
		_dip.text = "0"
	if _mil:
		_mil.text = "0"
