# SPDX-License-Identifier: GPL-3.0-only
extends Node

# 会话由 Autoload 持有，页面切换不会重建 Rust 世界和地图纹理。
var backend: AugustusMap
var game_directory := ""
var _startup_requested := false


func _ready() -> void:
	var arguments := OS.get_cmdline_user_args()
	var index := arguments.find("--game-dir")
	if index >= 0 and index + 1 < arguments.size():
		game_directory = arguments[index + 1]
		_startup_requested = true


func consume_startup_request() -> bool:
	# 命令行自动进入只执行一次，返回主菜单后不能再次触发。
	var requested := _startup_requested
	_startup_requested = false
	return requested


func has_game() -> bool:
	return backend != null


func load_game(directory: String) -> String:
	var candidate := AugustusMap.new()
	var error := candidate.load_map(directory)
	if error.is_empty():
		# 只在载入成功后替换会话，失败时保留之前可用的数据。
		backend = candidate
		game_directory = directory
	return error


func clear_game() -> void:
	backend = null
	# 目录保留供下次开局使用，世界状态和纹理随当前会话释放。
