# SPDX-License-Identifier: GPL-3.0-only
extends Control

# interface/topbar.gui 里金钱和君主点数那一段。素材运行时从欧陆目录读取。
const ASSETS_SCRIPT := preload("res://ui/eu4/eu4_ui_assets.gd")
const Eu4Type := preload("res://ui/eu4/eu4_type.gd")
const BAR_SIZE := Vector2(927, 160)
const CREAM := Color(0.96, 0.91, 0.78)
const DEBT := Color(0.86, 0.28, 0.24)

var _gold: Label
var _adm: Label
var _dip: Label
var _mil: Label


func setup(directory: String) -> void:
	mouse_filter = Control.MOUSE_FILTER_STOP
	custom_minimum_size = BAR_SIZE
	size = BAR_SIZE
	var assets = ASSETS_SCRIPT.new()
	_plate(assets, directory, "gfx/interface/topbar_UL.dds", Vector2.ZERO, BAR_SIZE)
	var gold := _icon(assets, directory, "gfx/interface/icon_gold_strip.dds", Vector2(146, 10))
	if gold != null and gold.texture != null and gold.texture.get_width() >= gold.texture.get_height() * 2:
		var frame := AtlasTexture.new()
		frame.atlas = gold.texture
		frame.region = Rect2(0, 0, gold.texture.get_width() * 0.5, gold.texture.get_height())
		gold.texture = frame
		gold.size = frame.region.size
	_icon(assets, directory, "gfx/interface/icon_powers_administrative.dds", Vector2(498, 49))
	_icon(assets, directory, "gfx/interface/icon_powers_diplomatic.dds", Vector2(558, 49))
	_icon(assets, directory, "gfx/interface/icon_powers_military.dds", Vector2(616, 49))
	_gold = _value("0", Vector2(152, 13), Vector2(60, 32), HORIZONTAL_ALIGNMENT_RIGHT, "金钱")
	_adm = _value("0", Vector2(520, 52), Vector2(36, 32), HORIZONTAL_ALIGNMENT_LEFT, "行政")
	_dip = _value("0", Vector2(581, 52), Vector2(36, 32), HORIZONTAL_ALIGNMENT_LEFT, "外交")
	_mil = _value("0", Vector2(639, 52), Vector2(36, 32), HORIZONTAL_ALIGNMENT_LEFT, "军事")
	if not GameSession.clock_changed.is_connected(_refresh):
		GameSession.clock_changed.connect(_refresh)
	if not GameSession.world_changed.is_connected(_refresh):
		GameSession.world_changed.connect(_refresh)
	_refresh()


func _plate(assets, directory: String, relative: String, at: Vector2, plate_size: Vector2) -> TextureRect:
	var rect := TextureRect.new()
	rect.texture = assets.texture_from_file(directory.path_join(relative))
	rect.position = at
	rect.size = plate_size
	rect.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	rect.stretch_mode = TextureRect.STRETCH_SCALE
	rect.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(rect)
	return rect


func _icon(assets, directory: String, relative: String, at: Vector2) -> TextureRect:
	var rect := _plate(assets, directory, relative, at, Vector2.ZERO)
	if rect.texture:
		rect.size = rect.texture.get_size()
		rect.expand_mode = TextureRect.EXPAND_KEEP_SIZE
		rect.stretch_mode = TextureRect.STRETCH_KEEP
	return rect


func _value(text: String, at: Vector2, box: Vector2, align: HorizontalAlignment, tip: String) -> Label:
	var label := Label.new()
	label.text = text
	label.position = at
	label.size = box
	label.horizontal_alignment = align
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	label.tooltip_text = tip
	Eu4Type.paint_label(label, Eu4Type.VIC_18)
	label.add_theme_color_override("font_color", CREAM)
	add_child(label)
	return label


func _refresh() -> void:
	var country := GameSession.get_player_country()
	if country.is_empty() or GameSession.backend == null:
		return
	var finances: Dictionary = GameSession.backend.get_country_finances(country)
	_gold.text = str(finances.get("treasury", "0"))
	_gold.add_theme_color_override("font_color", DEBT if finances.get("treasury_negative", false) else CREAM)
	# 君主点数还没有进入模拟，库存按开局 0 显示，不把国家代码写出来。
	_adm.text = "0"
	_dip.text = "0"
	_mil.text = "0"
