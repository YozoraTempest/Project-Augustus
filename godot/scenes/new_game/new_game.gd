# SPDX-License-Identifier: GPL-3.0-only
extends Control

# 原先的载入页已退休：不再显示路径框、空白底和「正在载入…」。
const LAUNCHER := "res://scenes/launcher/launcher.tscn"
const COUNTRY_SELECT := "res://scenes/country_select/country_select.tscn"
const GAME_SCENE := "res://scenes/game/game.tscn"


func _ready() -> void:
	var target := LAUNCHER
	if GameSession.is_started():
		target = GAME_SCENE
	elif GameSession.has_game():
		target = COUNTRY_SELECT
	get_tree().change_scene_to_file.call_deferred(target)
