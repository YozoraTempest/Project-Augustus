# SPDX-License-Identifier: GPL-3.0-only
extends Control

const NEW_GAME_SCENE := preload("res://scenes/new_game/new_game.tscn")
const SETTINGS_SCENE := preload("res://scenes/settings/settings_dialog.tscn")
const GAME_SCENE := "res://scenes/game/game.tscn"

@onready var new_game_button: Button = %NewGame
@onready var quit_button: Button = %Quit
@onready var settings_button: Button = %Settings
@onready var load_button: Button = %LoadGame
@onready var status_label: Label = %Status
@onready var save_dialog: GameSaveDialog = $SaveDialog

var _loading := false


func _ready() -> void:
	new_game_button.pressed.connect(_open_new_game)
	quit_button.pressed.connect(func(): get_tree().quit())
	settings_button.pressed.connect(_open_settings)
	load_button.pressed.connect(_open_load_dialog)
	save_dialog.file_selected.connect(_load_saved_game)
	new_game_button.grab_focus()
	if GameSession.consume_startup_request():
		_open_new_game.call_deferred(true)


func _open_new_game(auto_start := false) -> void:
	var screen := NEW_GAME_SCENE.instantiate()
	screen.auto_start = auto_start
	# 在加入节点树之前传入启动参数，普通菜单操作与命令行共用同一载入流程。
	get_tree().change_scene_to_node(screen)


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
	for button in [new_game_button, load_button, settings_button, quit_button]:
		button.disabled = true
	status_label.modulate = Color.WHITE
	status_label.text = "正在读取存档与地图…"
	# 先显示载入状态，再读取本地内容并创建地图纹理。
	await get_tree().process_frame
	await get_tree().process_frame
	var error := GameSession.load_saved_game(path)
	if not error.is_empty():
		_loading = false
		for button in [new_game_button, load_button, settings_button, quit_button]:
			button.disabled = false
		_show_error("读取失败：" + error)
		return
	get_tree().change_scene_to_file(GAME_SCENE)


func _show_error(message: String) -> void:
	status_label.text = message
	status_label.modulate = Color(1.0, 0.55, 0.48)
