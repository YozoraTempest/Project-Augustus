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
	var bytes := view.map_sprite.texture.get_image().get_data()
	check(bytes.slice(0, 8) == PackedByteArray([7, 0, 0, 0, 1, 0, 0, 1]), "纹理应保留完整的 32 位编号")
	check(screen.backend.get_start_date() == "1444.11.11", "界面应使用固定开局日期")
	check(screen.backend.get_country_count() == 2, "绑定应统计有领土国家")
	check(screen.backend.get_province_owner(7) == "AAA", "省份归属应包含开局当天的变化")
	check(screen.backend.get_province_owner(16777217) == "BBB", "占领者不能覆盖所有者")
	check(screen.backend.get_province_kind(7) == "land", "普通省份应识别为陆地")
	var political_texture: Texture2D = screen.backend.get_political_texture()
	var political_bytes := political_texture.get_image().get_data()
	check(political_bytes.slice(0, 12) == PackedByteArray([20, 50, 210, 255, 210, 50, 20, 255, 15, 20, 26, 255]), "政治地图应使用国家颜色并区分空白像素")
	check(view.map_sprite.material.get_shader_parameter("political_colors") == political_texture, "政治颜色纹理应传入着色器")
	check(screen.status_label.text.contains("1444.11.11") and screen.status_label.text.contains("2 个有领土国家"), "状态栏应显示日期与开局国家数")

	# 通过窗口输入分发点击，检查坐标转换、绑定查询和界面更新的完整路径。
	await click_pixel(view, Vector2i(0, 0))
	check(screen.selected_province == 7, "点击左上像素应选择省份 7")
	check(screen.selection_label.text.contains("#7"), "底部应显示选中的省份编号")
	check(screen.selection_label.text.contains("所属国家：AAA"), "选择省份应显示其所有者")
	check(view.map_sprite.material.get_shader_parameter("selected_province") == 7, "选中状态应传给着色器")
	await click_pixel(view, Vector2i(1, 0))
	check(screen.selected_province == 16777217, "大编号省份应能准确选中")
	check(screen.selection_label.text.contains("所属国家：BBB"), "切换选择后应更新国家归属")
	await navigation_checks(screen, view)

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
	check(view.map_sprite.material.get_shader_parameter("selected_province") == 0, "清除选择应同步取消高亮")

	await click_pixel(view, Vector2i(0, 0))
	var previous_texture: Texture2D = view.map_sprite.texture
	view.zoom_at(view.size * 0.5, 3.0)
	var previous_transform := view.map_viewport.canvas_transform
	check(not await screen.load_map(directory.path_join("missing")), "不存在的目录应报告载入失败")
	check(view.map_sprite.texture == previous_texture, "载入失败应保留旧地图")
	check(screen.backend.get_political_texture() == political_texture, "载入失败应保留政治颜色")
	check(screen.backend.get_province_owner(7) == "AAA", "载入失败应保留开局归属")
	check(screen.selected_province == 7, "载入失败应保留原选择")
	check(view.map_viewport.canvas_transform.is_equal_approx(previous_transform), "载入失败应保留缩放与位置")
	check(await screen.load_map(directory), "地图应能重新载入")
	check(screen.selected_province == 0, "重新载入成功后应清除旧选择")
	check(view.map_sprite.material.get_shader_parameter("selected_province") == 0, "重新载入应清除高亮")
	check(is_equal_approx(view.zoom_level, 1.0), "重新载入成功后应显示全图")
	screen.queue_free()
	await process_frame
	if failures.is_empty():
		print("Godot 地图集成测试通过")
	quit(0 if failures.is_empty() else 1)


func navigation_checks(screen: Control, view: ProvinceMapView) -> void:
	var original_texture := view.map_sprite.texture
	var original_rect := view.display_rect()
	var anchor := view.size * 0.5 + Vector2(35.0, 20.0)
	var world_before := view.map_viewport.canvas_transform.affine_inverse() * anchor
	await mouse_button(view.global_position + anchor, MOUSE_BUTTON_WHEEL_UP, true)
	await mouse_button(view.global_position + anchor, MOUSE_BUTTON_WHEEL_UP, false)
	var world_after := view.map_viewport.canvas_transform.affine_inverse() * anchor
	check(view.zoom_level > 1.0, "滚轮向上应放大地图")
	check(world_before.is_equal_approx(world_after), "缩放应保持鼠标指向的地图位置：%s -> %s，视口 %s" % [world_before, world_after, view.size])
	check(screen.selected_province == 16777217, "缩放不应改变选中的省份")
	check(view.map_sprite.texture == original_texture, "导航不应重新生成地图纹理")
	await mouse_button(view.global_position + anchor, MOUSE_BUTTON_WHEEL_DOWN, true)
	await mouse_button(view.global_position + anchor, MOUSE_BUTTON_WHEEL_DOWN, false)
	check(is_equal_approx(view.zoom_level, 1.0), "滚轮向下应恢复全图比例")
	check(view.display_rect().is_equal_approx(original_rect), "回到最小比例后地图应重新居中")

	view.zoom_at(view.size * 0.5, 2.0)
	var point := view.size * 0.5
	var drag := Vector2(45.0, 25.0)
	var pixel_before := view.texture_pixel_at(point)
	var transform_before := view.map_viewport.canvas_transform
	await mouse_button(view.global_position + point, MOUSE_BUTTON_MIDDLE, true)
	await mouse_motion(view.global_position + point + drag, drag, MOUSE_BUTTON_MASK_MIDDLE)
	check(not view.map_viewport.canvas_transform.is_equal_approx(transform_before), "中键拖拽应平移地图")
	check(view.texture_pixel_at(point + drag) == pixel_before, "拖拽后的同一位置应仍对应原始省份像素")
	check(screen.selected_province == 16777217, "中键拖拽不应选择省份")
	# 在地图外松开中键，随后进入地图的移动不能继续拖拽。
	await mouse_button(Vector2(5, 5), MOUSE_BUTTON_MIDDLE, false)
	var after_release := view.map_viewport.canvas_transform
	await mouse_motion(view.global_position + point, -drag, 0)
	check(view.map_viewport.canvas_transform.is_equal_approx(after_release), "地图外松开中键后应结束拖拽")
	await click_at(view.global_position + point + drag)
	check(screen.selected_province == screen.backend.province_at(pixel_before), "平移后的左键查询应使用实际显示坐标")

	# 连续拖到边界后，反向拖动应立即响应，不能积累地图外的相机位置。
	await mouse_button(view.global_position + point, MOUSE_BUTTON_MIDDLE, true)
	await mouse_motion(view.global_position + point, Vector2(10000, 10000), MOUSE_BUTTON_MASK_MIDDLE)
	var at_limit := view.map_viewport.canvas_transform
	var drawn := view.display_rect()
	check(drawn.position.x <= 1.0 and drawn.position.y <= 1.0, "平移到边缘时地图应覆盖视口")
	check(drawn.end.x >= view.size.x - 1.0 and drawn.end.y >= view.size.y - 1.0, "相机边界不应露出地图外区域")
	await mouse_motion(view.global_position + point, Vector2(-20, -20), MOUSE_BUTTON_MASK_MIDDLE)
	check(not view.map_viewport.canvas_transform.is_equal_approx(at_limit), "边界处反向拖动应立即移动地图")
	await mouse_button(view.global_position + point, MOUSE_BUTTON_MIDDLE, false)

	view.reset_view()
	view.zoom_at(view.size * 0.5, 4.0)
	var focus_before := view.camera.get_screen_center_position()
	root.size = Vector2i(800, 820)
	await process_frame
	await process_frame
	await process_frame
	check(view.camera.get_screen_center_position().is_equal_approx(focus_before), "改变窗口大小应保留地图视图中心")
	check(is_equal_approx(view.zoom_level, 4.0), "改变窗口大小应保留缩放级别")
	var center_pixel := view.texture_pixel_at(view.size * 0.5)
	await click_at(view.global_position + view.size * 0.5)
	check(screen.selected_province == screen.backend.province_at(center_pixel), "窗口变化后应准确选择中心省份")
	var selected_before_reset: int = screen.selected_province
	var reset: Button = screen.reset_view_button
	await click_at(reset.global_position + reset.size * 0.5)
	check(is_equal_approx(view.zoom_level, 1.0), "显示全图按钮应重置缩放")
	check(screen.selected_province == selected_before_reset, "显示全图应保留省份选择")
	check(view.camera.get_screen_center_position().is_equal_approx(Vector2(1.5, 1.0)), "显示全图应居中显示地图")

	view.zoom_at(view.size * 0.5, 10000.0)
	check(is_equal_approx(view.zoom_level, ProvinceMapView.MAX_ZOOM), "放大应有上限")
	view.zoom_at(view.size * 0.5, 0.000001)
	check(is_equal_approx(view.zoom_level, 1.0), "缩小应以显示全图为下限")
	view.reset_view()


func click_pixel(view: ProvinceMapView, pixel: Vector2i) -> void:
	var drawn := view.display_rect()
	var local_point := drawn.position + (Vector2(pixel) + Vector2(0.5, 0.5)) / view.map_sprite.texture.get_size() * drawn.size
	await click_at(view.global_position + local_point)


func click_at(position: Vector2) -> void:
	# 更新鼠标所在的视口和悬停控件，再分发按键，顺序与实际鼠标操作一致。
	await mouse_motion(position, Vector2.ZERO, 0)
	await mouse_button(position, MOUSE_BUTTON_LEFT, true)
	await mouse_button(position, MOUSE_BUTTON_LEFT, false)


func mouse_button(position: Vector2, button: MouseButton, pressed: bool) -> void:
	var event := InputEventMouseButton.new()
	event.button_index = button
	event.pressed = pressed
	event.factor = 1.0
	event.position = position
	Input.parse_input_event(event)
	await process_frame


func mouse_motion(position: Vector2, relative: Vector2, buttons: int) -> void:
	var event := InputEventMouseMotion.new()
	event.position = position
	event.relative = relative
	event.button_mask = buttons
	Input.parse_input_event(event)
	await process_frame
