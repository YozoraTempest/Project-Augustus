# SPDX-License-Identifier: GPL-3.0-only
extends Control

# 局内欧陆菜单。安装目录和扫描留在启动器。这里的开始游戏进入选国家。
const COUNTRY_SELECT := "res://scenes/country_select/country_select.tscn"
const LOAD_SCENE := preload("res://scenes/launcher/opening_load.tscn")
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
	AppSettings.apply_game_window()
	settings_button.icon = _gear_icon()
	new_game_button.pressed.connect(_start_game)
	quit_button.pressed.connect(func(): get_tree().quit())
	settings_button.pressed.connect(_open_settings)
	load_button.pressed.connect(_open_load_dialog)
	save_dialog.file_selected.connect(_load_saved_game)
	new_game_button.grab_focus()


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
	for button in [new_game_button, load_button, settings_button, quit_button]:
		button.disabled = true
	status_label.hide()
	var loader := LOAD_SCENE.instantiate()
	add_child(loader)
	var error: String = await loader.run(directory)
	if not is_inside_tree():
		return
	loader.queue_free()
	_loading = false
	for button in [new_game_button, load_button, settings_button, quit_button]:
		button.disabled = false
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
	for button in [new_game_button, load_button, settings_button, quit_button]:
		button.disabled = true
	status_label.modulate = Color.WHITE
	status_label.text = "正在读取存档…"
	status_label.show()
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
	status_label.show()


func _gear_icon() -> Texture2D:
	var size := 32
	var image := Image.create(size, size, false, Image.FORMAT_RGBA8)
	image.fill(Color(0, 0, 0, 0))
	var center := (size - 1) / 2.0
	var color := Color(0.96, 0.93, 0.86)
	for y in size:
		for x in size:
			var dx := x - center
			var dy := y - center
			var distance := sqrt(dx * dx + dy * dy)
			var tooth := cos(atan2(dy, dx) * 8.0) > 0.12
			var radius := 15.0 if tooth else 11.0
			if distance <= radius and distance >= 5.0:
				image.set_pixel(x, y, color)
	return ImageTexture.create_from_image(image)
