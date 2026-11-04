# SPDX-License-Identifier: GPL-3.0-only
extends Control

const NEW_GAME_SCENE := preload("res://scenes/new_game/new_game.tscn")
const SETTINGS_SCENE := preload("res://scenes/settings/settings_dialog.tscn")

@onready var new_game_button: Button = %NewGame
@onready var quit_button: Button = %Quit
@onready var settings_button: Button = %Settings


func _ready() -> void:
	new_game_button.pressed.connect(_open_new_game)
	quit_button.pressed.connect(func(): get_tree().quit())
	settings_button.pressed.connect(_open_settings)
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
