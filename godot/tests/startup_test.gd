# SPDX-License-Identifier: GPL-3.0-only
extends SceneTree

const MAIN_MENU := "res://scenes/main_menu/main_menu.tscn"
const GAME_SCENE := "res://scenes/game/game.tscn"
const NEW_GAME := "res://scenes/new_game/new_game.tscn"


func _init() -> void:
	_run.call_deferred()


func _run() -> void:
	root.size = Vector2i(1000, 700)
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
	index = arguments.find("--country")
	var country := arguments[index + 1] if index >= 0 and index + 1 < arguments.size() else ""
	var should_start := country in ["AAA", "BBB"]
	change_scene_to_file(ProjectSettings.get_setting("application/run/main_scene"))
	for frame in range(60):
		if current_scene != null and current_scene.scene_file_path == GAME_SCENE:
			break
		if not should_start and current_scene != null and current_scene.scene_file_path == NEW_GAME and session.has_game() and not current_scene._loading:
			break
		await process_frame
	if not should_start:
		if current_scene == null or current_scene.scene_file_path != NEW_GAME or not session.has_game() or session.is_started() or not current_scene.enter_button.disabled:
			push_error("未指定有效玩家国家时应停留选国页，不能直接进入游戏")
			quit(1)
			return
		if not country.is_empty() and not current_scene.status_label.text.contains("无法选择"):
			push_error("无效命令行国家应显示选国失败原因")
			quit(1)
			return
		print("Godot 命令行选国测试通过")
		quit(0)
		return
	if current_scene == null or current_scene.scene_file_path != GAME_SCENE:
		push_error("命令行启动应自动载入开局并进入游戏")
		quit(1)
		return
	if not session.is_started() or session.get_player_country() != country or session.backend.get_province_count() != 2:
		push_error("命令行启动应使用指定目录和玩家国家")
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
