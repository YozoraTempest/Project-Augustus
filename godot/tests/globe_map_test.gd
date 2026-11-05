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
	root.size = Vector2i(1100, 760)
	await process_frame
	var session := root.get_node("GameSession")
	var error: String = session.load_game(ProjectSettings.globalize_path("res://tests/data/map_modes"))
	check(error.is_empty(), "应能载入球形地图测试内容：" + error)
	if not error.is_empty():
		quit(1)
		return
	check(session.start_game("AAA").is_empty(), "地图测试应先确认玩家国家")
	change_scene_to_file("res://scenes/game/game.tscn")
	await frames()
	var screen = current_scene
	var view: ProvinceMapView = screen.map_view
	var globe: GlobeMapView = view.globe_view
	var backend: AugustusMap = screen.backend
	var texture := backend.get_map_texture()
	var owners := backend.get_owner_texture()
	check(view.projection == MapMode.ViewType.FLAT and not globe.visible, "开局默认保持平面视图")
	view.select_at(Vector2i(0, 0))
	view.zoom_at(view.size * 0.5, 1.5)
	var flat_transform := view.map_viewport.canvas_transform
	# 通过显示方式控件切换，选择范围仍保持国家模式。
	view.projection_option.get_popup().index_pressed.emit(view.projection_option.get_item_index(MapMode.ViewType.GLOBE))
	await frames()
	check(view.projection == MapMode.ViewType.GLOBE and globe.visible and not view.flat_view.visible, "球形视图应替代平面视图显示")
	check(view.projection_option.get_selected_id() == view.projection, "显示方式控件应与地图状态一致")
	check(globe.globe.mesh is SphereMesh and globe.camera.projection == Camera3D.PROJECTION_PERSPECTIVE, "球形视图应使用真实球体网格和透视相机")
	check(globe.surface_material.get_shader_parameter("province_ids") == texture and globe.surface_material.get_shader_parameter("owner_ids") == owners, "球面应复用同一地图编号与归属纹理")
	check(view.selected_country == "AAA" and view.mode == MapMode.Type.COUNTRY, "切换显示方式应保留国家选择和选择模式")
	check(screen.navigation_hint.text.contains("旋转") and screen.reset_view_button.text == "重置球体", "球形视图应显示旋转提示和重置入口")
	var center := view.size * 0.5
	var pixel := view.texture_pixel_at(center)
	check(pixel == Vector2i(3, 0), "球体正面中心应对应同一经纬度的地图像素：" + str(pixel))
	await click_at(view.global_position + center)
	check(view.selected_country == "BBB" and view.selected_province == 0, "球形视图应依据点击位置的所有者选择整个国家")
	check(screen.info_panel.country_title.text == "Second Republic", "球面选择应更新国家信息")
	check(globe.surface_material.get_shader_parameter("selected_country") == backend.get_country_render_id("BBB"), "国家高亮应传入球面材质")
	check(view.texture_pixel_at(Vector2(3, view.size.y - 3)) == Vector2i(-1, -1), "球体外的背景不能映射为地图省份")
	await click_at(view.global_position + Vector2(3, view.size.y - 3))
	check(view.selected_country == "BBB", "点击球体外应保持原选择")

	view.set_mode(MapMode.Type.PROVINCE)
	var base_color := Color()
	if DisplayServer.get_name() != "headless":
		await RenderingServer.frame_post_draw
		var image := globe.map_viewport.get_texture().get_image()
		base_color = image.get_pixelv(globe.map_viewport.size / 2)
		check(not base_color.is_equal_approx(image.get_pixel(3, image.get_height() - 3)), "实际渲染应显示球体而非只有背景")
	await click_at(view.global_position + center)
	check(view.selected_province == 16777217 and view.selected_country.is_empty(), "球面拾取应保留超过 24 位的省份编号")
	check(screen.info_panel.province_title.text == "Highland" and screen.info_panel.tabs.current_tab == 0, "球面省份选择应显示省份详情")
	if DisplayServer.get_name() != "headless":
		await RenderingServer.frame_post_draw
		var image := globe.map_viewport.get_texture().get_image()
		var selected := image.get_pixelv(globe.map_viewport.size / 2)
		check(selected.r + selected.g > base_color.r + base_color.g + 0.1, "真实渲染的球面高亮必须与拾取到的省份一致")

	# 通过相机正前方检查两侧经线、北部和南部，避免球面首尾或纵向颠倒。
	for sample in [Vector3(-PI + 0.02, 0.3, 7), Vector3(PI - 0.02, 0.3, 9), Vector3(0.0, -0.3, 10)]:
		globe.yaw = sample.x
		globe.pitch = sample.y
		globe.update_camera()
		var sampled := view.texture_pixel_at(center)
		check(backend.province_at(sampled) == int(sample.z), "球面经线与上下方向应匹配原始地图：" + str(sampled))
	globe.reset_view()
	var before_drag := globe.camera.transform
	await mouse_button(view.global_position + center, MOUSE_BUTTON_MIDDLE, true)
	await mouse_motion(view.global_position + center + Vector2(100, 40), Vector2(100, 40), MOUSE_BUTTON_MASK_MIDDLE)
	check(not globe.camera.transform.is_equal_approx(before_drag), "中键拖动应旋转 3D 视角")
	check(view.selected_province == 16777217, "旋转不能更改选择")
	await mouse_button(Vector2(3, 3), MOUSE_BUTTON_MIDDLE, false)
	var after_drag := globe.camera.transform
	await mouse_motion(view.global_position + center, Vector2(30, 30), 0)
	check(globe.camera.transform.is_equal_approx(after_drag), "地图外松开中键应结束球体旋转")
	var previous_distance := globe.camera.position.length()
	await mouse_button(view.global_position + center, MOUSE_BUTTON_WHEEL_UP, true)
	await mouse_button(view.global_position + center, MOUSE_BUTTON_WHEEL_UP, false)
	check(globe.camera.position.length() < previous_distance and globe.zoom_level > 1.0, "滚轮应拉近球体相机")
	view.zoom_at(center, 10000.0)
	check(is_equal_approx(globe.zoom_level, GlobeMapView.MAX_ZOOM) and globe.camera.position.length() > GlobeMapView.RADIUS, "最大缩放不能穿入球体")
	view.zoom_at(center, 0.000001)
	check(is_equal_approx(globe.zoom_level, 1.0), "缩小应以完整球体为下限")
	globe.rotate_by(Vector2(100000, 100000))
	check(absf(globe.pitch) <= GlobeMapView.MAX_PITCH and absf(globe.yaw) <= PI, "旋转应处理环绕并限制极点翻转")
	await click_control(screen.reset_view_button)
	check(is_equal_approx(globe.zoom_level, 1.0) and is_equal_approx(globe.yaw, GlobeMapView.DEFAULT_YAW) and is_equal_approx(globe.pitch, GlobeMapView.DEFAULT_PITCH), "重置球体应恢复默认朝向与距离")
	check(view.selected_province == 16777217, "重置球体应保留选择")

	root.size = Vector2i(740, 900)
	await frames()
	check(view.texture_pixel_at(view.size * 0.5) == pixel, "调整窗口比例后球体中心应保持同一地图位置")
	check(globe.map_viewport.size == Vector2i(globe.size), "球体渲染尺寸应随地图区域更新")
	root.size = Vector2i(1100, 760)
	await frames()
	# 暂停沿用地图模块的拖动生命周期，恢复后不能延续已经松开的旋转。
	center = view.size * 0.5
	await mouse_button(view.global_position + center, MOUSE_BUTTON_MIDDLE, true)
	screen.settings_button.pressed.emit()
	await frames()
	var dialog: Window = screen.get_node("SettingsDialog")
	var paused_camera := globe.camera.transform
	check(paused and not view._dragging, "打开设置应暂停球体并结束旋转")
	await mouse_button(view.global_position + center, MOUSE_BUTTON_MIDDLE, false)
	dialog.cancel_button.pressed.emit()
	await frames()
	await mouse_motion(view.global_position + center, Vector2(40, 40), 0)
	check(not paused and globe.camera.transform.is_equal_approx(paused_camera), "关闭设置后不能继续之前的旋转")
	view.set_projection(MapMode.ViewType.FLAT)
	await frames()
	check(view.map_viewport.canvas_transform.is_equal_approx(flat_transform), "切回平面应恢复其原有缩放与位置")
	check(view.selected_province == 16777217 and view.map_sprite.material.get_shader_parameter("selected_province") == 16777217, "切回平面应保持球面选中的省份与高亮")
	check(backend.get_map_texture() == texture and backend.get_owner_texture() == owners, "切换显示方式和交互不能重建纹理")
	if failures.is_empty():
		print("Godot 球形地图测试通过")
	quit(0 if failures.is_empty() else 1)


func frames() -> void:
	for frame in range(3):
		await process_frame


func click_control(control: Control) -> void:
	await click_at(control.global_position + control.size * 0.5)


func click_at(position: Vector2) -> void:
	await mouse_motion(position, Vector2.ZERO, 0)
	await mouse_button(position, MOUSE_BUTTON_LEFT, true)
	await mouse_button(position, MOUSE_BUTTON_LEFT, false)


func mouse_button(position: Vector2, button: MouseButton, pressed: bool) -> void:
	var event := InputEventMouseButton.new()
	event.position = position
	event.button_index = button
	event.pressed = pressed
	event.factor = 1.0
	Input.parse_input_event(event)
	await process_frame


func mouse_motion(position: Vector2, relative: Vector2, buttons: int) -> void:
	var event := InputEventMouseMotion.new()
	event.position = position
	event.relative = relative
	event.button_mask = buttons
	Input.parse_input_event(event)
	await process_frame
