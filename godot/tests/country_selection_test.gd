# SPDX-License-Identifier: GPL-3.0-only
extends SceneTree

const NEW_GAME := "res://scenes/new_game/new_game.tscn"
const GAME_SCENE := "res://scenes/game/game.tscn"

var failures: Array[String] = []


func _init() -> void:
	_run.call_deferred()


func check(condition: bool, message: String) -> void:
	if not condition:
		failures.append(message)
		push_error(message)


func _run() -> void:
	root.size = Vector2i(1100, 760)
	await process_frame
	var session := root.get_node("GameSession")
	check(not session.start_game("AAA").is_empty() and not session.is_started(), "没有世界时不能开始游戏")
	change_scene_to_file(NEW_GAME)
	await frames()
	var setup = current_scene
	var directory := ProjectSettings.globalize_path("res://tests/data/map_modes")
	check(await setup.load_game(directory), "应能载入选国预览")
	await frames()
	var backend: AugustusMap = session.backend
	var texture := backend.get_map_texture()
	check(setup.preview.visible and not setup.empty_state.visible, "载入后应显示可交互地图预览")
	check(setup.enter_button.disabled and setup.choose_button.disabled, "没有候选国家时不能确认或开始")
	check(not session.is_started() and backend.get_player_country().is_empty(), "载入世界不能自动确定玩家国家")
	check(not backend.start_game("ZZZ").is_empty(), "不存在的国家不能开局")
	check(not backend.start_game("CCC").is_empty() and backend.get_player_country().is_empty(), "已注册但没有领土的国家不能开局")
	setup.enter_button.pressed.emit()
	await frames()
	check(current_scene == setup and not session.is_started(), "直接触发开始事件也必须先选有效国家")
	for pixel in [Vector2i(2, 1), Vector2i(3, 1), Vector2i(4, 1)]:
		await click_pixel(setup.map_view, pixel)
		check(setup.choose_button.disabled and setup.enter_button.disabled, "空白、海洋和无主区域不能选为玩家国家")
	await click_pixel(setup.map_view, Vector2i(0, 0))
	check(setup.info_panel.country_title.text == "First Kingdom" and not setup.choose_button.disabled, "地图点击应显示可选国家的概况")
	check(setup.enter_button.disabled and not session.is_started(), "浏览国家不等于确认玩家身份")
	await click_control(setup.choose_button)
	check(setup.chosen_label.text.contains("First Kingdom (AAA)") and not setup.enter_button.disabled, "确认国家后应显示候选并允许开始")
	check(not session.is_started(), "候选国家在正式开始前不应写入会话身份")
	await click_pixel(setup.map_view, Vector2i(2, 0))
	check(setup.info_panel.country_title.text == "Second Republic" and setup.chosen_label.text.contains("(AAA)"), "浏览其他国家不能替换已确认的候选")
	check(not setup.enter_button.disabled and not session.is_started(), "继续浏览应保留开始资格而不提前开始")
	await click_control(setup.choose_button)
	check(setup.chosen_label.text.contains("Second Republic (BBB)") and setup.choose_button.disabled, "开始前可以明确改选另一个国家")
	setup.map_view.set_projection(MapMode.ViewType.GLOBE)
	setup.map_view.set_mode(MapMode.Type.PROVINCE)
	await frames()
	await click_at(setup.map_view.global_position + setup.map_view.size * 0.5)
	check(setup.info_panel.province_title.text == "Highland" and setup.chosen_label.text.contains("(BBB)"), "球面省份浏览应与已确认国家分别保存")
	await click_control(setup.enter_button)
	await frames()
	var screen = current_scene
	check(screen.scene_file_path == GAME_SCENE and session.is_started(), "确认后应进入正式游戏")
	if screen.scene_file_path != GAME_SCENE:
		quit(1)
		return
	check(session.get_player_country() == "BBB" and backend.get_player_country() == "BBB", "Rust 会话应持有最终确认的玩家国家")
	check(screen.backend == backend and screen.map_view.map_sprite.texture == texture, "进入游戏应复用预览中已载入的世界和纹理")
	check(screen.map_view.projection == MapMode.ViewType.GLOBE, "进入游戏应保留开局选择的显示方式")
	check(screen.player_country_label.text.contains("Second Republic (BBB)"), "游戏页面应固定显示玩家国家")
	screen.map_view.select_at(Vector2i(0, 0))
	check(screen.info_panel.country_title.text == "First Kingdom" and screen.player_country_label.text.contains("(BBB)"), "查看他国只能更改浏览信息")
	check(not session.start_game("AAA").is_empty() and session.get_player_country() == "BBB", "正式开局后重复开始不能更换玩家国家")
	check(not session.load_game(directory.path_join("missing")).is_empty() and session.get_player_country() == "BBB", "载入失败应保留已有世界与玩家身份")
	check(not backend.load_map(directory.path_join("missing")).is_empty() and backend.get_player_country() == "BBB", "原生载入失败也不能清除玩家身份")
	await click_control(screen.menu_button)
	await frames()
	check(not session.has_game() and session.get_player_country().is_empty(), "结束游戏应清除会话中的玩家身份")
	# 同一原生对象成功重载也表示新开局，不能继承原先的身份。
	check(backend.load_map(directory).is_empty() and backend.get_player_country().is_empty(), "成功重载世界应重置玩家身份")
	change_scene_to_file(NEW_GAME)
	await frames()
	setup = current_scene
	check(await setup.load_game(directory), "第二次开局应能重新载入")
	await frames()
	check(setup.chosen_label.text == "玩家国家：未选择" and setup.enter_button.disabled, "新开局不能继承上局候选")
	await click_pixel(setup.map_view, Vector2i(0, 0))
	await click_control(setup.choose_button)
	setup.path_input.text = directory.path_join("missing")
	setup.path_input.text_changed.emit(setup.path_input.text)
	check(not setup.preview.visible and setup.enter_button.disabled and setup.chosen_label.text == "玩家国家：未选择", "修改来源应清除旧预览和候选")
	setup.enter_button.pressed.emit()
	check(not session.is_started(), "来源失效后不能通过旧候选开始游戏")
	check(not await setup.load_game(setup.path_input.text), "无效来源应报告载入失败")
	check(not setup.preview.visible and setup.enter_button.disabled, "载入失败不能重新启用旧预览或候选")
	await click_control(setup.back_button)
	await frames()
	check(not session.has_game() and not session.is_started(), "取消开局应清除预览会话")
	# 已载入但未确定玩家的会话也不能直接运行游戏场景。
	check(session.load_game(directory).is_empty(), "应能准备未开始的世界")
	change_scene_to_file(GAME_SCENE)
	await frames()
	check(current_scene.scene_file_path == NEW_GAME and not session.is_started(), "缺少玩家身份时游戏场景应转到选国页")
	if failures.is_empty():
		print("Godot 玩家国家与开局流程测试通过")
	quit(0 if failures.is_empty() else 1)


func frames() -> void:
	for frame in range(3):
		await process_frame


func click_pixel(view: ProvinceMapView, pixel: Vector2i) -> void:
	await click_at(view.global_position + view.map_viewport.canvas_transform * (Vector2(pixel) + Vector2(0.5, 0.5)))


func click_control(control: Control) -> void:
	await click_at(control.global_position + control.size * 0.5)


func click_at(position: Vector2) -> void:
	var motion := InputEventMouseMotion.new()
	motion.position = position
	Input.parse_input_event(motion)
	await process_frame
	for pressed in [true, false]:
		var event := InputEventMouseButton.new()
		event.position = position
		event.button_index = MOUSE_BUTTON_LEFT
		event.pressed = pressed
		Input.parse_input_event(event)
		await process_frame
