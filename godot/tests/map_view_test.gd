# SPDX-License-Identifier: GPL-3.0-only
extends SceneTree

var failures: Array[String] = []


func _init() -> void:
	_run.call_deferred()


func check(condition: bool, message: String) -> void:
	if not condition:
		failures.append(message)
		push_error(message)


func _run() -> void:
	root.size = Vector2i(1000, 700)
	var screen = load("res://main.tscn").instantiate()
	root.add_child(screen)
	await process_frame
	var directory := ProjectSettings.globalize_path("res://tests/data/game")
	check(await screen.load_map(directory), "自制地图应成功载入")
	await process_frame
	var view: ProvinceMapView = screen.map_view
	check(screen.backend.get_map_size() == Vector2i(3, 2), "绑定返回的地图尺寸应正确")
	check(screen.backend.get_province_count() == 2, "绑定应统计位图中的省份")
	check(screen.backend.province_at(Vector2i(-1, 0)) == -1, "负坐标应返回越界")
	check(screen.backend.province_at(Vector2i(3, 0)) == -1, "右边界应返回越界")
	check(screen.backend.province_at(Vector2i(2, 0)) == 0, "未定义像素应返回无省份")
	var bytes := view.texture.get_image().get_data()
	check(bytes.slice(0, 8) == PackedByteArray([7, 0, 0, 0, 1, 0, 0, 1]), "纹理应保留完整的 32 位编号")

	# 通过窗口输入分发点击，检查坐标转换、绑定查询和界面更新的完整路径。
	await click_pixel(view, Vector2i(0, 0))
	check(screen.selected_province == 7, "点击左上像素应选择省份 7")
	check(screen.selection_label.text.contains("#7"), "底部应显示选中的省份编号")
	check(view.material.get_shader_parameter("selected_province") == 7, "选中状态应传给着色器")
	await click_pixel(view, Vector2i(1, 0))
	check(screen.selected_province == 16777217, "大编号省份应能准确选中")

	# 改变窗口比例后重新选中，留白区域不能误选边缘省份。
	root.size = Vector2i(740, 900)
	await process_frame
	await process_frame
	await click_pixel(view, Vector2i(0, 1))
	check(screen.selected_province == 16777217, "改变窗口比例后坐标映射应保持正确")
	check(view.texture_pixel_at(Vector2.ZERO) == Vector2i(-1, -1), "地图外的留白应被排除")
	await click_at(view.global_position + Vector2(1, 1))
	check(screen.selected_province == 16777217, "点击留白应保持原选择")
	await click_pixel(view, Vector2i(2, 0))
	check(screen.selected_province == 0, "点击无省份像素应清除选择")
	check(view.material.get_shader_parameter("selected_province") == 0, "清除选择应同步取消高亮")

	await click_pixel(view, Vector2i(0, 0))
	var previous_texture: Texture2D = view.texture
	check(not await screen.load_map(directory.path_join("missing")), "不存在的目录应报告载入失败")
	check(view.texture == previous_texture, "载入失败应保留旧地图")
	check(screen.selected_province == 7, "载入失败应保留原选择")
	check(await screen.load_map(directory), "地图应能重新载入")
	check(screen.selected_province == 0, "重新载入成功后应清除旧选择")
	check(view.material.get_shader_parameter("selected_province") == 0, "重新载入应清除高亮")
	screen.queue_free()
	await process_frame
	if failures.is_empty():
		print("Godot 地图集成测试通过")
	quit(0 if failures.is_empty() else 1)


func click_pixel(view: ProvinceMapView, pixel: Vector2i) -> void:
	var drawn := view.display_rect()
	var local_point := drawn.position + (Vector2(pixel) + Vector2(0.5, 0.5)) / view.texture.get_size() * drawn.size
	await click_at(view.global_position + local_point)


func click_at(position: Vector2) -> void:
	var event := InputEventMouseButton.new()
	event.button_index = MOUSE_BUTTON_LEFT
	event.pressed = true
	event.position = position
	Input.parse_input_event(event)
	await process_frame
	event = InputEventMouseButton.new()
	event.button_index = MOUSE_BUTTON_LEFT
	event.pressed = false
	event.position = position
	Input.parse_input_event(event)
	await process_frame
