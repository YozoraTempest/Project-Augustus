# SPDX-License-Identifier: GPL-3.0-only
extends Control

const MAIN_MENU := "res://scenes/main_menu/main_menu.tscn"
const LOAD_SCENE := preload("res://scenes/launcher/opening_load.tscn")
const DIRECTORY_SCENE := preload("res://scenes/launcher/eu4_directory_dialog.tscn")

# 左侧目前只有欧陆风云 IV。以后可以在同一列加上原生奥古斯都对局，这里不实现。
@onready var eu4_button: Button = %EuropaUniversalis
@onready var play_button: Button = %Play
@onready var settings_button: Button = %Settings
@onready var directory_label: Label = %Directory
@onready var status_label: Label = %Status

var _loading := false


func _ready() -> void:
	AppSettings.apply_launcher_window()
	eu4_button.set_pressed_no_signal(true)
	eu4_button.toggled.connect(func(_pressed: bool) -> void:
		eu4_button.set_pressed_no_signal(true))
	settings_button.icon = _gear_icon()
	play_button.pressed.connect(_on_play)
	settings_button.pressed.connect(_open_directory_settings)
	_refresh_directory()
	play_button.grab_focus()
	# 命令行目录也在启动器里载入，完成后再进入主菜单。
	if GameSession.consume_startup_request():
		_begin_game.call_deferred(GameSession.game_directory)


func _on_play() -> void:
	_begin_game(AppSettings.eu4_directory)


func _begin_game(directory: String) -> void:
	if _loading:
		return
	var path := directory.strip_edges()
	if path.is_empty():
		_open_directory_settings()
		_show_error("请先选择或扫描安装目录，再开始游戏。")
		return
	_loading = true
	play_button.disabled = true
	settings_button.disabled = true
	status_label.hide()
	var loader := LOAD_SCENE.instantiate()
	add_child(loader)
	var error: String = await loader.run(path)
	if not is_inside_tree():
		return
	loader.queue_free()
	_loading = false
	play_button.disabled = false
	settings_button.disabled = false
	if not error.is_empty():
		_show_error(error)
		return
	get_tree().change_scene_to_file(MAIN_MENU)


func _open_directory_settings() -> void:
	if _loading:
		return
	var dialog := DIRECTORY_SCENE.instantiate()
	add_child(dialog)
	dialog.directory_changed.connect(_refresh_directory)
	dialog.popup_centered()


func _refresh_directory() -> void:
	var path := AppSettings.eu4_directory.strip_edges()
	directory_label.text = "尚未设置安装目录" if path.is_empty() else path
	directory_label.tooltip_text = directory_label.text
	status_label.hide()


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
