# SPDX-License-Identifier: GPL-3.0-only
extends Control

const NEW_GAME_SCENE := preload("res://scenes/new_game/new_game.tscn")
const DIRECTORY_SCENE := preload("res://scenes/launcher/eu4_directory_dialog.tscn")

# 左侧目前只有欧陆风云 IV。以后可以在同一列加上原生奥古斯都对局，这里不实现。
@onready var eu4_button: Button = %EuropaUniversalis
@onready var play_button: Button = %Play
@onready var settings_button: Button = %Settings
@onready var directory_label: Label = %Directory
@onready var status_label: Label = %Status
@onready var directory_dialog: FileDialog = $DirectoryDialog


func _ready() -> void:
	eu4_button.set_pressed_no_signal(true)
	eu4_button.toggled.connect(func(_pressed: bool) -> void:
		eu4_button.set_pressed_no_signal(true))
	settings_button.icon = _gear_icon()
	play_button.pressed.connect(_on_play)
	settings_button.pressed.connect(_open_directory_settings)
	directory_dialog.dir_selected.connect(_on_play_directory_selected)
	_refresh_directory()
	play_button.grab_focus()
	# --game-dir 仍走原来的开局。主场景换成启动器之后，由这里消费启动请求。
	if GameSession.consume_startup_request():
		_enter_new_game.call_deferred(GameSession.game_directory)


func _on_play() -> void:
	var saved := AppSettings.eu4_directory.strip_edges()
	if saved.is_empty():
		directory_dialog.popup_centered_ratio(0.7)
		return
	_enter_new_game(saved)


func _on_play_directory_selected(path: String) -> void:
	var error := AppSettings.set_eu4_directory(path)
	if error != OK:
		_show_error("无法保存目录：" + error_string(error))
		return
	_refresh_directory()
	_enter_new_game(AppSettings.eu4_directory)


func _open_directory_settings() -> void:
	var dialog := DIRECTORY_SCENE.instantiate()
	add_child(dialog)
	dialog.directory_changed.connect(_refresh_directory)
	dialog.popup_centered()


func _enter_new_game(directory: String) -> void:
	# 与命令行 --game-dir 相同：把目录交给会话，并自动载入现有的新游戏流程。
	GameSession.game_directory = directory
	var screen := NEW_GAME_SCENE.instantiate()
	screen.auto_start = true
	get_tree().change_scene_to_node(screen)


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
