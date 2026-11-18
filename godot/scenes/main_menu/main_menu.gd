# SPDX-License-Identifier: GPL-3.0-only
extends Control

# 局内欧陆菜单。安装目录和扫描留在启动器。画面从已配置的欧陆目录读取，不把贴图放进仓库。
const COUNTRY_SELECT := "res://scenes/country_select/country_select.tscn"
const LOAD_SCENE := preload("res://scenes/launcher/opening_load.tscn")
const SETTINGS_SCENE := preload("res://scenes/settings/settings_dialog.tscn")
const ASSETS_SCRIPT := preload("res://ui/eu4/eu4_ui_assets.gd")
const BUILDER_SCRIPT := preload("res://ui/eu4/eu4_ui_builder.gd")
const GAME_SCENE := "res://scenes/game/game.tscn"
# frontend.gui 的底栏按 1920×1080 落在屏幕底边。再按当前窗口同比放大。
const MENU_REFERENCE := Vector2(1920, 1080)

@onready var host: Control = %Host
@onready var status_label: Label = %Status
@onready var save_dialog: GameSaveDialog = $SaveDialog

var _loading := false
var _layout: Control


func _ready() -> void:
	AppSettings.apply_game_window()
	save_dialog.file_selected.connect(_load_saved_game)
	resized.connect(_apply_menu_scale)
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
	var backdrop := TextureRect.new()
	backdrop.set_anchors_preset(Control.PRESET_FULL_RECT)
	backdrop.mouse_filter = Control.MOUSE_FILTER_IGNORE
	backdrop.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	backdrop.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_COVERED
	backdrop.texture = _menu_background(directory, assets)
	host.add_child(backdrop)
	_layout = Control.new()
	_layout.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_layout.position = Vector2.ZERO
	_layout.size = MENU_REFERENCE
	host.add_child(_layout)
	var builder = BUILDER_SCRIPT.new()
	builder.assets = assets
	builder.plain_windows = true
	for panel_name in ["mainmenu_panel_upperleft", "mainmenu_panel_upperright", "mainmenu_panel_bottom"]:
		builder.build_named(document, panel_name, _layout)
	_span_upper_right()
	_use_our_actions()
	_wire_menu(_layout)
	_apply_menu_scale()


func _apply_menu_scale() -> void:
	if _layout == null:
		return
	var target := size
	if target.x < 1.0 or target.y < 1.0:
		return
	_layout.scale = target / MENU_REFERENCE


func _span_upper_right() -> void:
	var panel := _find_named(_layout, "mainmenu_panel_upperright")
	if panel == null:
		return
	# 原面板只有 1024 宽，右上标志要对齐屏幕右缘。
	panel.offset_left = 0.0
	panel.offset_top = -1.0
	panel.offset_right = MENU_REFERENCE.x
	panel.offset_bottom = 767.0


func _use_our_actions() -> void:
	_set_caption("single_player_button_text", "开始游戏")
	_set_caption("multi_player_button_text", "读取游戏")
	_set_caption("settings_button", "设置")
	_set_caption("exit_button_text", "退出")
	for name in ["tutorial_button", "tutorial_button_text", "credits_button", "nudge_button", "version_label", "version_background", "ironman_icon", "continue_container"]:
		var node := _find_named(_layout, name)
		if node:
			node.hide()


func _set_caption(eu4_name: String, caption: String) -> void:
	var node := _find_named(_layout, eu4_name)
	if node == null:
		return
	if node is Button:
		node.text = caption
		node.add_theme_color_override("font_color", Color(0.96, 0.91, 0.78))
		node.add_theme_font_size_override("font_size", 22)
	elif node is RichTextLabel:
		node.text = caption


func _menu_background(directory: String, assets) -> Texture2D:
	var folder := directory.path_join("gfx/loadingscreens")
	var listing := DirAccess.open(folder)
	if listing == null:
		return null
	var paths: PackedStringArray = PackedStringArray()
	for file_name in listing.get_files():
		var lower := file_name.to_lower()
		if not lower.begins_with("load_") or not lower.ends_with(".dds"):
			continue
		var path := folder.path_join(file_name)
		if _readable_background(path):
			paths.append(path)
	if paths.is_empty():
		return null
	paths.sort()
	return assets.texture_from_file(paths[0])


func _readable_background(path: String) -> bool:
	var file := FileAccess.open(path, FileAccess.READ)
	if file == null:
		return false
	var header := file.get_buffer(128)
	if header.size() < 128:
		return false
	var height := header.decode_u32(12)
	var width := header.decode_u32(16)
	var fourcc := header.slice(84, 88).get_string_from_ascii()
	if fourcc.contains("DXT"):
		return false
	return width > 0 and height > 0 and width <= 2560 and height <= 1600


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
