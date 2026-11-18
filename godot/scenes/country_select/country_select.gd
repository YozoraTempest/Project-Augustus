# SPDX-License-Identifier: GPL-3.0-only
extends Control

# 选国家沿用欧陆大厅：左栏日期、顶栏提示、右栏国旗和资料。贴图从安装目录读取。
const LAUNCHER := "res://scenes/launcher/launcher.tscn"
const MAIN_MENU := "res://scenes/main_menu/main_menu.tscn"
const GAME_SCENE := "res://scenes/game/game.tscn"
const ASSETS_SCRIPT := preload("res://ui/eu4/eu4_ui_assets.gd")
const MENU_REFERENCE := Vector2(1920, 1080)

@onready var map_view: ProvinceMapView = %MapView

var _backend: AugustusMap
var _chosen_country := ""
var _chrome: Control
var _flag: TextureRect
var _color: ColorRect
var _name: Label
var _body: Label
var _error: Label
var _start: Button


func _ready() -> void:
	if not GameSession.has_game() or GameSession.is_started():
		var target := GAME_SCENE if GameSession.is_started() else LAUNCHER
		get_tree().change_scene_to_file.call_deferred(target)
		return
	AppSettings.apply_game_window()
	_backend = GameSession.backend
	resized.connect(_apply_chrome_scale)
	_build_chrome()
	map_view.selection_changed.connect(_refresh_selection)
	_clear_country()
	await get_tree().process_frame
	if not is_inside_tree() or _backend == null:
		return
	map_view.present_for_country_pick()
	map_view.show_map(_backend)
	var pending := GameSession.startup_country
	GameSession.startup_country = ""
	if pending.is_empty():
		return
	map_view.select_country(pending)
	if not _chosen_country.is_empty():
		_enter_game()


func _build_chrome() -> void:
	var directory := str(GameSession.backend.get_game_directory())
	var assets = ASSETS_SCRIPT.new()
	_chrome = Control.new()
	_chrome.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_chrome.position = Vector2.ZERO
	_chrome.size = MENU_REFERENCE
	add_child(_chrome)
	_plate(_chrome, assets, directory, "gfx/interface/lobby_left_bg.dds", Vector2(12, 36))
	_plate(_chrome, assets, directory, "gfx/interface/lobby_date_bg.dds", Vector2(18, 470))
	_plate(_chrome, assets, directory, "gfx/interface/lobby_top_bg.dds", Vector2(725, 8))
	var modes := ProvinceMapView.add_surface_modes(self, directory, map_view)
	modes.layout_mode = 1
	modes.set_anchors_preset(Control.PRESET_CENTER_TOP)
	modes.offset_left = -90.0
	modes.offset_top = 188.0
	modes.offset_right = 90.0
	modes.offset_bottom = 236.0
	modes.grow_horizontal = Control.GROW_DIRECTION_BOTH
	var right := _plate(_chrome, assets, directory, "gfx/interface/lobby_right_bg.dds", Vector2(1636, 48))
	_label(_chrome, "选择国家", Vector2(725, 18), Vector2(470, 32), 22, HORIZONTAL_ALIGNMENT_CENTER)
	_label(_chrome, "世界 %s" % _backend.get_start_date(), Vector2(725, 48), Vector2(470, 28), 18, HORIZONTAL_ALIGNMENT_CENTER)
	_label(_chrome, "开局日期", Vector2(36, 488), Vector2(244, 28), 18, HORIZONTAL_ALIGNMENT_CENTER)
	_label(_chrome, str(_backend.get_start_date()), Vector2(36, 530), Vector2(244, 36), 22, HORIZONTAL_ALIGNMENT_CENTER)
	var back := _text_button(_chrome, assets, directory, "gfx/interface/big_button_232.dds", "返回主菜单", Vector2(42, 980))
	back.pressed.connect(_return_to_menu)
	_start = _text_button(right, assets, directory, "gfx/interface/big_button_232.dds", "开始游戏", Vector2(20, 520))
	_start.pressed.connect(_enter_game)
	_start.disabled = true
	_flag = TextureRect.new()
	_flag.position = Vector2(86, 36)
	_flag.size = Vector2(96, 96)
	_flag.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	_flag.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	_flag.mouse_filter = Control.MOUSE_FILTER_IGNORE
	right.add_child(_flag)
	_color = ColorRect.new()
	_color.position = Vector2(36, 140)
	_color.size = Vector2(200, 8)
	_color.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_color.hide()
	right.add_child(_color)
	_name = _label(right, "未选择国家", Vector2(16, 156), Vector2(240, 36), 22, HORIZONTAL_ALIGNMENT_CENTER)
	_body = _label(right, "在地图上点选一个国家", Vector2(24, 200), Vector2(224, 280), 18, HORIZONTAL_ALIGNMENT_LEFT)
	_body.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	_error = _label(right, "", Vector2(20, 470), Vector2(232, 44), 16, HORIZONTAL_ALIGNMENT_CENTER)
	_error.add_theme_color_override("font_color", Color(1, 0.55, 0.48))
	_error.hide()
	_apply_chrome_scale()


func _plate(parent: Control, assets, directory: String, relative: String, at: Vector2) -> TextureRect:
	var plate := TextureRect.new()
	plate.texture = assets.texture_from_file(directory.path_join(relative))
	plate.mouse_filter = Control.MOUSE_FILTER_IGNORE
	plate.position = at
	if plate.texture:
		plate.size = plate.texture.get_size()
	parent.add_child(plate)
	return plate


func _label(parent: Control, text: String, at: Vector2, box: Vector2, font_size: int, align: HorizontalAlignment) -> Label:
	var label := Label.new()
	label.text = text
	label.position = at
	label.size = box
	label.horizontal_alignment = align
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	label.add_theme_font_size_override("font_size", font_size)
	label.add_theme_color_override("font_color", Color(0.96, 0.91, 0.78))
	parent.add_child(label)
	return label


func _text_button(parent: Control, assets, directory: String, relative: String, caption: String, at: Vector2) -> Button:
	var button := Button.new()
	button.text = caption
	button.position = at
	button.focus_mode = Control.FOCUS_NONE
	button.add_theme_font_size_override("font_size", 20)
	button.add_theme_color_override("font_color", Color(0.96, 0.91, 0.78))
	button.add_theme_color_override("font_hover_color", Color(1, 0.95, 0.8))
	button.add_theme_color_override("font_disabled_color", Color(0.55, 0.52, 0.46))
	var texture: Texture2D = assets.texture_from_file(directory.path_join(relative))
	if texture:
		button.custom_minimum_size = texture.get_size()
		button.size = texture.get_size()
		var style := StyleBoxTexture.new()
		style.texture = texture
		for state in ["normal", "hover", "pressed", "disabled", "focus"]:
			button.add_theme_stylebox_override(state, style)
	parent.add_child(button)
	return button


func _apply_chrome_scale() -> void:
	if _chrome == null:
		return
	if size.x < 1.0 or size.y < 1.0:
		return
	_chrome.scale = size / MENU_REFERENCE


func _refresh_selection() -> void:
	if _backend == null:
		return
	var tag := map_view.selected_country
	if tag.is_empty():
		_clear_country()
		return
	var info := _backend.get_country_info(tag)
	if info.is_empty():
		_clear_country()
		return
	_flag.texture = _flag_texture(tag)
	_flag.visible = _flag.texture != null
	_color.show()
	_color.color = info.color
	_name.text = str(info.name)
	var capital := "—" if info.capital_name == null else str(info.capital_name)
	var culture := "—" if info.primary_culture == null else str(info.primary_culture)
	var religion := "—" if info.religion == null else str(info.religion)
	_body.text = "首都　%s\n文化　%s\n宗教　%s\n\n省份　%d\n发展度　%s" % [capital, culture, religion, int(info.province_count), info.development]
	_error.hide()
	if int(info.province_count) == 0:
		_chosen_country = ""
		_start.disabled = true
		_error.text = "这个国家没有领土，不能开始。"
		_error.show()
		return
	_chosen_country = tag
	_start.disabled = false


func _flag_texture(tag: String) -> Texture2D:
	var directory := str(_backend.get_game_directory())
	var path := directory.path_join("gfx/flags").path_join(tag + ".tga")
	if not FileAccess.file_exists(path):
		return null
	var image := Image.new()
	if image.load(path) != OK:
		return null
	return ImageTexture.create_from_image(image)


func _clear_country() -> void:
	_chosen_country = ""
	if _start:
		_start.disabled = true
	if _flag:
		_flag.texture = null
	if _color:
		_color.hide()
	if _name:
		_name.text = "未选择国家"
	if _body:
		_body.text = "在地图上点选一个国家"
	if _error:
		_error.hide()


func _enter_game() -> void:
	if _backend == null or _backend != GameSession.backend or _chosen_country.is_empty():
		return
	var error: String = GameSession.start_game(_chosen_country)
	if not error.is_empty():
		_error.text = "无法开始游戏：" + error
		_error.show()
		return
	GameSession.map_projection = MapMode.ViewType.FLAT
	get_tree().change_scene_to_file(GAME_SCENE)


func _return_to_menu() -> void:
	get_tree().change_scene_to_file(MAIN_MENU)
