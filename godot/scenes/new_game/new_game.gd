# SPDX-License-Identifier: GPL-3.0-only
extends Control

const MAIN_MENU := "res://scenes/main_menu/main_menu.tscn"
const GAME_SCENE := "res://scenes/game/game.tscn"

@onready var path_input: LineEdit = %GameDirectory
@onready var browse_button: Button = %Browse
@onready var load_button: Button = %Load
@onready var enter_button: Button = %EnterGame
@onready var back_button: Button = %Back
@onready var status_label: Label = %Status
@onready var directory_dialog: FileDialog = $DirectoryDialog

var auto_start := false
var _loading := false


func _ready() -> void:
	path_input.text = GameSession.game_directory
	browse_button.pressed.connect(func(): directory_dialog.popup_centered_ratio(0.7))
	directory_dialog.dir_selected.connect(func(path: String):
		path_input.text = path
		_on_directory_changed(path))
	path_input.text_changed.connect(_on_directory_changed)
	path_input.text_submitted.connect(load_game)
	load_button.pressed.connect(func(): load_game(path_input.text))
	enter_button.pressed.connect(_enter_game)
	back_button.pressed.connect(_return_to_menu)
	_on_directory_changed(path_input.text)
	path_input.grab_focus()
	if auto_start:
		_load_startup_game.call_deferred()


func load_game(directory: String) -> bool:
	if _loading:
		return false
	var path := directory.strip_edges()
	if path.is_empty():
		return false
	path_input.text = path
	status_label.text = "正在载入开局与政治地图…"
	status_label.modulate = Color.WHITE
	_set_loading(true)
	# 先绘制载入状态，再调用原生内容读取与纹理创建。
	await get_tree().process_frame
	await get_tree().process_frame
	var error: String = GameSession.load_game(path)
	_set_loading(false)
	if not error.is_empty():
		status_label.text = "载入失败：" + error
		status_label.modulate = Color(1.0, 0.55, 0.48)
		return false
	var backend: AugustusMap = GameSession.backend
	var map_size := backend.get_map_size()
	status_label.text = "开局 %s\n%d 个有领土国家 · %d 个省份\n地图 %d × %d 像素\n\n开局已就绪，可以进入游戏。" % [backend.get_start_date(), backend.get_country_count(), backend.get_province_count(), map_size.x, map_size.y]
	enter_button.disabled = false
	enter_button.grab_focus()
	return true


func _load_startup_game() -> void:
	if await load_game(path_input.text):
		_enter_game()


func _on_directory_changed(path: String) -> void:
	# 修改开局来源后需重新载入，不能用旧会话进入新的目录。
	enter_button.disabled = true
	load_button.disabled = _loading or path.strip_edges().is_empty()
	status_label.modulate = Color.WHITE
	status_label.text = "开局日期：1444.11.11\n请选择本地 EU4 安装目录，然后载入开局。"


func _set_loading(loading: bool) -> void:
	_loading = loading
	load_button.disabled = loading or path_input.text.strip_edges().is_empty()
	browse_button.disabled = loading
	back_button.disabled = loading
	path_input.editable = not loading
	enter_button.disabled = true


func _enter_game() -> void:
	get_tree().change_scene_to_file(GAME_SCENE)


func _return_to_menu() -> void:
	GameSession.clear_game()
	get_tree().change_scene_to_file(MAIN_MENU)
