# SPDX-License-Identifier: GPL-3.0-only
extends Control

# 局内欧陆菜单。安装目录和扫描留在启动器。画面从已配置的欧陆目录读取，不把贴图放进仓库。
const COUNTRY_SELECT := "res://scenes/country_select/country_select.tscn"
const LOAD_SCENE := preload("res://scenes/launcher/opening_load.tscn")
const SETTINGS_SCENE := preload("res://scenes/settings/settings_dialog.tscn")
const ASSETS_SCRIPT := preload("res://ui/eu4/eu4_ui_assets.gd")
const BUILDER_SCRIPT := preload("res://ui/eu4/eu4_ui_builder.gd")
const GAME_SCENE := "res://scenes/game/game.tscn"
const MAP_SCENE := preload("res://scenes/map/province_map_view.tscn")
# frontend.gui 的坐标和贴图都是界面像素。面板按原始方向钉在窗口上，不随窗口放大。
const Eu4Type := preload("res://ui/eu4/eu4_type.gd")
# common/defines.lua 的 NFrontend。Y 是高度，视线落在地面。
const FRONTEND_POS := Vector3(2958.0, 900.0, 1500.0)
const FRONTEND_LOOK := Vector3(2958.0, 0.0, 1519.0)

@onready var host: Control = %Host
@onready var status_label: Label = %Status
@onready var save_dialog: GameSaveDialog = $SaveDialog

var _loading := false
var _layout: Control


func _ready() -> void:
	AppSettings.apply_game_window()
	save_dialog.file_selected.connect(_load_saved_game)
	_show_menu_map()
	_dress_from_eu4()


func _start_game() -> void:
	if _loading:
		return
	if GameSession.has_game() and not GameSession.is_started():
		get_tree().change_scene_to_file(COUNTRY_SELECT)
		return
	var directory := GameSession.game_directory.strip_edges()
	if directory.is_empty():
		directory = AppSettings.eu4_directory.strip_edges()
	if directory.is_empty():
		_show_error("请先在启动器选择安装目录。")
		return
	_loading = true
	_set_menu_buttons_disabled(true)
	status_label.hide()
	var loader := LOAD_SCENE.instantiate()
	add_child(loader)
	var error: String = await loader.run(directory)
	if not is_inside_tree():
		return
	loader.queue_free()
	_loading = false
	_set_menu_buttons_disabled(false)
	if not error.is_empty():
		_show_error(error)
		return
	get_tree().change_scene_to_file(COUNTRY_SELECT)


func _open_settings() -> void:
	var dialog := SETTINGS_SCENE.instantiate()
	add_child(dialog)
	dialog.popup_centered()


func _open_load_dialog() -> void:
	var error: String = save_dialog.open_for_loading()
	if not error.is_empty():
		_show_error(error)


func _load_saved_game(path: String) -> void:
	if _loading:
		return
	_loading = true
	_set_menu_buttons_disabled(true)
	status_label.modulate = Color.WHITE
	status_label.text = "正在读取存档…"
	status_label.show()
	# 先显示载入状态，再读取本地内容并创建地图纹理。
	await get_tree().process_frame
	await get_tree().process_frame
	var error := GameSession.load_saved_game(path)
	if not error.is_empty():
		_loading = false
		_set_menu_buttons_disabled(false)
		_show_error("读取失败：" + error)
		return
	get_tree().change_scene_to_file(GAME_SCENE)


func _show_error(message: String) -> void:
	status_label.text = message
	status_label.modulate = Color(1.0, 0.55, 0.48)
	status_label.show()


func _dress_from_eu4() -> void:
	for child in host.get_children():
		child.queue_free()
	_layout = null
	var directory := GameSession.game_directory.strip_edges()
	if directory.is_empty():
		directory = AppSettings.eu4_directory.strip_edges()
	if directory.is_empty() or not DirAccess.dir_exists_absolute(directory):
		return
	var session := AugustusMap.new()
	var loaded: Dictionary = session.load_eu4_ui_document(directory, "interface/frontend.gui")
	if loaded.get("ok", false) != true:
		return
	var document: Variant = JSON.parse_string(str(loaded.get("json", "")))
	if typeof(document) != TYPE_DICTIONARY:
		return
	var assets = ASSETS_SCRIPT.new()
	assets.backend = session
	_layout = Control.new()
	_layout.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_layout.set_anchors_preset(Control.PRESET_FULL_RECT)
	_layout.scale = Vector2.ONE
	host.add_child(_layout)
	var builder = BUILDER_SCRIPT.new()
	builder.assets = assets
	builder.plain_windows = true
	for panel_name in ["mainmenu_panel_upperleft", "mainmenu_panel_upperright", "mainmenu_panel_bottom"]:
		builder.build_named(document, panel_name, _layout)
	_use_our_actions()
	_wire_menu(_layout)


func _use_our_actions() -> void:
	# 字落在按钮贴图的蓝底里，避开上下金边和左右花角。边距来自贴图像素和 frontend.gui 的文字盒。
	_seat_on_button("single_player_button_text", "single_player_button", "开始游戏", 31.0, 16.0, 13.0, 16.0)
	_seat_on_button("multi_player_button_text", "multi_player_button", "读取游戏", 10.0, 16.0, 34.0, 16.0)
	_seat_on_button("settings_button", "settings_button", "设置", 8.0, 13.0, 8.0, 12.0)
	_seat_on_button("exit_button_text", "exit_button", "退出", 14.0, 12.0, 16.0, 12.0)
	for name in ["tutorial_button", "tutorial_button_text", "credits_button", "nudge_button", "version_label", "version_background", "ironman_icon", "continue_container"]:
		var node := _find_named(_layout, name)
		if node:
			node.hide()


func _seat_on_button(text_name: String, button_name: String, caption: String, left: float, top: float, right: float, bottom: float) -> void:
	var button := _find_named(_layout, button_name) as Control
	var node := _find_named(_layout, text_name)
	if button == null or node == null:
		return
	var inner := Rect2(
		button.offset_left + left,
		button.offset_top + top,
		maxf(button.offset_right - button.offset_left - left - right, 1.0),
		maxf(button.offset_bottom - button.offset_top - top - bottom, 1.0)
	)
	if node is Button:
		node.text = caption
		node.clip_text = false
		node.add_theme_color_override("font_color", Color(0.96, 0.91, 0.78))
		Eu4Type.paint_button(node, Eu4Type.VIC_22, Vector2(button.offset_right - button.offset_left, button.offset_bottom - button.offset_top), Vector4(left, top, right, bottom))
		return
	if node is RichTextLabel:
		node.text = caption
		node.bbcode_enabled = false
		node.scroll_active = false
		node.autowrap_mode = TextServer.AUTOWRAP_OFF
		node.fit_content = false
		node.clip_contents = false
		node.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
		node.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
		Eu4Type.paint_rich_text(node, Eu4Type.VIC_22)
		node.add_theme_color_override("default_color", Color(0.96, 0.91, 0.78))
		node.add_theme_constant_override("line_separation", 0)
		node.add_theme_stylebox_override("normal", StyleBoxEmpty.new())
		node.offset_left = inner.position.x
		node.offset_top = inner.position.y
		node.offset_right = inner.end.x
		node.offset_bottom = inner.end.y


func _show_menu_map() -> void:
	if not GameSession.has_game():
		return
	var map_view: ProvinceMapView = MAP_SCENE.instantiate()
	map_view.set_anchors_preset(Control.PRESET_FULL_RECT)
	map_view.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(map_view)
	move_child(map_view, 0)
	map_view.show_map(GameSession.backend)
	map_view.present_for_country_pick()
	map_view.frame_frontend_camera.call_deferred(FRONTEND_LOOK, FRONTEND_POS)


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


func _wire_menu(node: Node) -> void:
	if node is BaseButton:
		var eu4_name := str(node.get_meta("eu4_name", ""))
		match eu4_name:
			"single_player_button":
				node.pressed.connect(_start_game)
			"multi_player_button":
				node.pressed.connect(_open_load_dialog)
			"settings_button":
				node.pressed.connect(_open_settings)
			"exit_button":
				node.pressed.connect(func() -> void: get_tree().quit())
	for child in node.get_children():
		_wire_menu(child)


func _set_menu_buttons_disabled(disabled: bool) -> void:
	_set_named_disabled(host, disabled)


func _set_named_disabled(node: Node, disabled: bool) -> void:
	if node is BaseButton:
		var eu4_name := str(node.get_meta("eu4_name", ""))
		if eu4_name in ["single_player_button", "multi_player_button", "settings_button", "exit_button"]:
			node.disabled = disabled
	for child in node.get_children():
		_set_named_disabled(child, disabled)
