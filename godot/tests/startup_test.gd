# SPDX-License-Identifier: GPL-3.0-only
extends SceneTree

const MAIN_MENU := "res://scenes/main_menu/main_menu.tscn"
const GAME_SCENE := "res://scenes/game/game.tscn"


func _init() -> void:
	_run.call_deferred()


func _run() -> void:
	await process_frame
	var session := root.get_node("GameSession")
	var arguments := OS.get_cmdline_user_args()
	var index := arguments.find("--game-dir")
	if index < 0 or index + 1 >= arguments.size():
		push_error("启动测试需要通过 --game-dir 传入自制地图目录")
		quit(1)
		return
	if session.game_directory != arguments[index + 1]:
		push_error("会话未读取命令行中的游戏目录")
		quit(1)
		return
	change_scene_to_file(ProjectSettings.get_setting("application/run/main_scene"))
	for frame in range(60):
		if current_scene != null and current_scene.scene_file_path == GAME_SCENE:
			break
		await process_frame
	if current_scene == null or current_scene.scene_file_path != GAME_SCENE:
		push_error("命令行启动应自动载入开局并进入游戏")
		quit(1)
		return
	if not session.has_game() or session.backend.get_province_count() != 2:
		push_error("命令行启动应使用指定目录中的开局")
		quit(1)
		return
	# 保留启动参数，模拟实际按钮返回菜单，随后数帧也不能再次自动进入。
	current_scene.get_node("%MainMenu").pressed.emit()
	for frame in range(20):
		await process_frame
	if current_scene == null or current_scene.scene_file_path != MAIN_MENU or session.has_game():
		push_error("自动启动后的返回操作应停留主菜单并结束会话")
		quit(1)
		return
	print("Godot 命令行启动测试通过")
	quit(0)
