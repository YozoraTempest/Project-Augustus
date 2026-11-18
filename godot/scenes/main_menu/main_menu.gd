# SPDX-License-Identifier: GPL-3.0-only
extends Control

# 局内欧陆菜单。安装目录和扫描留在启动器。画面从已配置的欧陆目录读取，不把贴图放进仓库。
const COUNTRY_SELECT := "res://scenes/country_select/country_select.tscn"
const LOAD_SCENE := preload("res://scenes/launcher/opening_load.tscn")
const SETTINGS_SCENE := preload("res://scenes/settings/settings_dialog.tscn")
const ASSETS_SCRIPT := preload("res://ui/eu4/eu4_ui_assets.gd")
const BUILDER_SCRIPT := preload("res://ui/eu4/eu4_ui_builder.gd")
const GAME_SCENE := "res://scenes/game/game.tscn"

@onready var host: Control = %Host
@onready var status_label: Label = %Status
@onready var save_dialog: GameSaveDialog = $SaveDialog

var _loading := false


func _ready() -> void:
	AppSettings.apply_game_window()
	save_dialog.file_selected.connect(_load_saved_game)
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
	var builder = BUILDER_SCRIPT.new()
	builder.assets = assets
	builder.plain_windows = true
	# 只搭 mainmenu 自己的三块：底栏、左上、右上。背景不使用载入图。
	for panel_name in ["mainmenu_panel_upperleft", "mainmenu_panel_upperright", "mainmenu_panel_bottom"]:
		builder.build_named(document, panel_name, host)
	_wire_menu(host)


func _wire_menu(node: Node) -> void:
	if node is BaseButton:
		var eu4_name := str(node.get_meta("eu4_name", ""))
		match eu4_name:
			"single_player_button":
				node.pressed.connect(_start_game)
			"settings_button":
				node.pressed.connect(_open_settings)
			"exit_button":
				node.pressed.connect(func() -> void: get_tree().quit())
			"continue_game_button":
				node.pressed.connect(_open_load_dialog)
	for child in node.get_children():
		_wire_menu(child)


func _set_menu_buttons_disabled(disabled: bool) -> void:
	_set_named_disabled(host, disabled)


func _set_named_disabled(node: Node, disabled: bool) -> void:
	if node is BaseButton:
		var eu4_name := str(node.get_meta("eu4_name", ""))
		if eu4_name in ["single_player_button", "settings_button", "exit_button", "continue_game_button"]:
			node.disabled = disabled
	for child in node.get_children():
		_set_named_disabled(child, disabled)
