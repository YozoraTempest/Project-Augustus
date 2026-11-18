# SPDX-License-Identifier: GPL-3.0-only
extends Control

# 选国家沿用欧陆大厅：左栏日期、顶栏提示、右栏国旗和资料。贴图从安装目录读取。
const LAUNCHER := "res://scenes/launcher/launcher.tscn"
const MAIN_MENU := "res://scenes/main_menu/main_menu.tscn"
const GAME_SCENE := "res://scenes/game/game.tscn"
const ASSETS_SCRIPT := preload("res://ui/eu4/eu4_ui_assets.gd")
const Eu4Type := preload("res://ui/eu4/eu4_type.gd")
const CREAM := Color(0.96, 0.91, 0.78)

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
	_chrome.set_anchors_preset(Control.PRESET_FULL_RECT)
	_chrome.scale = Vector2.ONE
	add_child(_chrome)
	# frontend.gui：left / left_sidepanel，贴图用自身像素。
	_plate(_chrome, assets, directory, "gfx/interface/lobby_left_bg.dds", Vector2(4, 31), "UPPER_LEFT")
	var date := _plate(_chrome, assets, directory, "gfx/interface/lobby_date_bg.dds", Vector2(10, 401), "UPPER_LEFT")
	_label(date, "开局日期", Vector2(6, 9), Vector2(260, 30), Eu4Type.VIC_18, HORIZONTAL_ALIGNMENT_CENTER)
	var year := ""
	var daymonth := ""
	var parts := str(_backend.get_start_date()).split(".")
	if parts.size() >= 3:
		year = parts[0]
		daymonth = "%s.%s" % [parts[1], parts[2]]
	else:
		year = str(_backend.get_start_date())
	_label(date, year, Vector2(120, 57), Vector2(80, 30), Eu4Type.VIC_22, HORIZONTAL_ALIGNMENT_CENTER)
	_label(date, daymonth, Vector2(82, 113), Vector2(110, 20), Eu4Type.VIC_18, HORIZONTAL_ALIGNMENT_CENTER)
	var back := _text_button(_chrome, assets, directory, "gfx/interface/button_type_4.dds", "返回主菜单", Vector2(18, -12), "LOWER_LEFT", Eu4Type.VIC_18, Vector4(12, 7, 12, 7))
	back.pressed.connect(_return_to_menu)
	# top 窗口钉在屏幕上沿中点，横幅和文字用 gui 里的相对坐标。
	var top := Control.new()
	top.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_chrome.add_child(top)
	_pin(top, Vector2.ZERO, "CENTER_UP", Vector2(420, 500))
	_plate(top, assets, directory, "gfx/interface/lobby_top_bg.dds", Vector2(-235, 6), "UPPER_LEFT")
	_label(top, "世界 %s" % _backend.get_start_date(), Vector2(-150, 18), Vector2(300, 30), Eu4Type.VIC_22, HORIZONTAL_ALIGNMENT_CENTER)
	var select_label := _label(top, "选择国家", Vector2(-375, 92), Vector2(300, 60), Eu4Type.VIC_18, HORIZONTAL_ALIGNMENT_CENTER)
	_pin(select_label, Vector2(-375, 92), "CENTER_UP", Vector2(300, 60))
	var modes := ProvinceMapView.add_surface_modes(top, directory, map_view)
	modes.position = Vector2(-194, 51)
	modes.add_theme_constant_override("separation", -3)
	# right / right_sidepanel，以及右下角的开始按钮。
	_plate(_chrome, assets, directory, "gfx/interface/lobby_right_bg.dds", Vector2(-266, 16), "UPPER_RIGHT")
	var card := Control.new()
	card.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_chrome.add_child(card)
	_pin(card, Vector2(-236, 40), "UPPER_RIGHT", Vector2(200, 340))
	_flag = TextureRect.new()
	_flag.position = Vector2(60, 32)
	_flag.size = Vector2(92, 92)
	_flag.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	_flag.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	_flag.mouse_filter = Control.MOUSE_FILTER_IGNORE
	card.add_child(_flag)
	_color = ColorRect.new()
	_color.position = Vector2(24, 122)
	_color.size = Vector2(160, 4)
	_color.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_color.hide()
	card.add_child(_color)
	_name = _label(card, "未选择国家", Vector2(24, 130), Vector2(160, 24), Eu4Type.VIC_22, HORIZONTAL_ALIGNMENT_CENTER)
	_body = _label(card, "在地图上点选一个国家", Vector2(20, 160), Vector2(175, 150), Eu4Type.VIC_18, HORIZONTAL_ALIGNMENT_CENTER)
	_body.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	_error = _label(card, "", Vector2(12, 300), Vector2(176, 36), Eu4Type.VIC_18, HORIZONTAL_ALIGNMENT_CENTER)
	_error.add_theme_color_override("font_color", Color(1, 0.55, 0.48))
	_error.hide()
	_start = _text_button(_chrome, assets, directory, "gfx/interface/big_button_232.dds", "开始游戏", Vector2(-246, -40), "LOWER_RIGHT", Eu4Type.VIC_22, Vector4(18, 13, 18, 12))
	_start.pressed.connect(_enter_game)
	_start.disabled = true


func _plate(parent: Control, assets, directory: String, relative: String, at: Vector2, orientation: String) -> TextureRect:
	var plate := TextureRect.new()
	plate.texture = assets.texture_from_file(directory.path_join(relative))
	plate.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var plate_size := plate.texture.get_size() if plate.texture else Vector2.ZERO
	parent.add_child(plate)
	_pin(plate, at, orientation, plate_size)
	return plate


func _pin(control: Control, at: Vector2, orientation: String, plate_size: Vector2) -> void:
	# gui 的 position 是控件左上角相对该方向锚点的像素，尺寸用贴图原大。
	var x := at.x
	var y := at.y
	match orientation:
		"UPPER_RIGHT":
			control.anchor_left = 1.0
			control.anchor_right = 1.0
			control.anchor_top = 0.0
			control.anchor_bottom = 0.0
		"LOWER_LEFT":
			control.anchor_left = 0.0
			control.anchor_right = 0.0
			control.anchor_top = 1.0
			control.anchor_bottom = 1.0
			y -= plate_size.y
		"LOWER_RIGHT":
			control.anchor_left = 1.0
			control.anchor_right = 1.0
			control.anchor_top = 1.0
			control.anchor_bottom = 1.0
			y -= plate_size.y
		"CENTER_UP":
			control.anchor_left = 0.5
			control.anchor_right = 0.5
			control.anchor_top = 0.0
			control.anchor_bottom = 0.0
		_:
			control.set_anchors_preset(Control.PRESET_TOP_LEFT)
	control.offset_left = x
	control.offset_top = y
	control.offset_right = x + plate_size.x
	control.offset_bottom = y + plate_size.y


func _label(parent: Control, text: String, at: Vector2, box: Vector2, font_size: int, align: HorizontalAlignment) -> Label:
	var label := Label.new()
	label.text = text
	label.position = at
	label.size = box
	label.horizontal_alignment = align
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	Eu4Type.paint_label(label, font_size)
	label.add_theme_color_override("font_color", Color(0.96, 0.91, 0.78))
	parent.add_child(label)
	return label


func _text_button(parent: Control, assets, directory: String, relative: String, caption: String, at: Vector2, orientation: String, font_size: int, margins: Vector4) -> Button:
	var button := Button.new()
	button.text = caption
	button.focus_mode = Control.FOCUS_NONE
	button.add_theme_color_override("font_color", CREAM)
	button.add_theme_color_override("font_hover_color", Color(1, 0.95, 0.8))
	button.add_theme_color_override("font_disabled_color", Color(0.55, 0.52, 0.46))
	var texture: Texture2D = assets.texture_from_file(directory.path_join(relative))
	var plate_size := texture.get_size() if texture else Vector2(88, 32)
	if texture:
		var style := StyleBoxTexture.new()
		style.texture = texture
		for state in ["normal", "hover", "pressed", "disabled", "focus"]:
			button.add_theme_stylebox_override(state, style)
	parent.add_child(button)
	_pin(button, at, orientation, plate_size)
	Eu4Type.paint_button(button, font_size, plate_size, margins)
	return button


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
