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
	await process_frame
	var session := root.get_node("GameSession")
	var error: String = session.load_game(ProjectSettings.globalize_path("res://tests/data/map_modes"))
	check(error.is_empty(), "应能载入多省份国家地图：" + error)
	if not error.is_empty():
		quit(1)
		return
	change_scene_to_file("res://scenes/game/game.tscn")
	await process_frame
	await process_frame
	await process_frame
	var screen = current_scene
	var view: ProvinceMapView = screen.map_view
	var backend: AugustusMap = screen.backend
	var inspector: ProvinceInfoPanel = screen.info_panel
	var texture := view.map_sprite.texture
	var owners := backend.get_owner_texture()
	var material: ShaderMaterial = view.map_sprite.material
	check(view.mode == MapMode.Type.COUNTRY, "新地图默认使用国家模式")
	check(view.mode_option.get_selected_id() == view.mode, "模式控件应与地图状态一致")
	check(material.get_shader_parameter("owner_ids") == owners, "归属编号纹理应绑定到地图材质")
	check(backend.get_country_render_id("") == 0 and backend.get_country_render_id("ZZZ") == 0, "没有国家时渲染编号应为零")
	check(backend.get_country_render_id("AAA") != backend.get_country_render_id("BBB"), "同色国家必须使用不同归属编号")

	await click_pixel(view, Vector2i(0, 0))
	check(view.selected_country == "AAA" and view.selected_province == 0, "国家模式应只选择国家")
	check(material.get_shader_parameter("country_mode") and material.get_shader_parameter("selected_country") == backend.get_country_render_id("AAA"), "国家选择应传给着色器")
	check(inspector.tabs.current_tab == 1 and inspector.country_title.text == "First Kingdom", "国家模式应显示国家页")
	check(inspector.country_body.text.contains("领土省份：2"), "国家信息应包含全部领土省份")
	# 使用实际渲染器运行时，同时检查远处分离的本国省份和同色邻国。
	if DisplayServer.get_name() != "headless":
		await RenderingServer.frame_post_draw
		var image := view.map_viewport.get_texture().get_image()
		var selected := pixel_color(view, image, Vector2i(0, 0))
		check(selected.is_equal_approx(pixel_color(view, image, Vector2i(4, 0))), "国家高亮应覆盖远处的另一省份")
		check(not selected.is_equal_approx(pixel_color(view, image, Vector2i(2, 0))), "同色邻国不能随本国一起高亮")
		var boundary := Vector2i(view.map_viewport.canvas_transform * Vector2(1, 0.5))
		check(image.get_pixelv(boundary).is_equal_approx(selected), "国家内部省界不能产生国家选中轮廓")
	await click_pixel(view, Vector2i(4, 0))
	check(view.selected_country == "AAA" and view.selected_province == 0, "点击同国另一省份仍应选择整个国家")
	await click_pixel(view, Vector2i(2, 0))
	check(view.selected_country == "BBB", "国家选择应依据所有者，不能依据控制者")
	view.select_at(Vector2i(-1, -1))
	check(view.selected_country == "BBB", "地图外的位置不能改变选择")
	for pixel in [Vector2i(2, 1), Vector2i(3, 1), Vector2i(4, 1)]:
		await click_pixel(view, pixel)
		check(view.selected_country.is_empty() and view.selected_province == 0, "国家模式点击空白、海洋或无主省份应清除选择")
		check(material.get_shader_parameter("selected_country") == 0 and inspector.country_title.text == "未选择国家", "无归属区域不能保留国家高亮或信息")

	await click_pixel(view, Vector2i(0, 0))
	view.zoom_at(view.size * 0.5, 1.5)
	var transform_before := view.map_viewport.canvas_transform
	# 触发下拉菜单的选项信号，验证控件与地图之间的连接。
	await click_control(view.mode_option)
	var popup := view.mode_option.get_popup()
	popup.index_pressed.emit(view.mode_option.get_item_index(MapMode.Type.PROVINCE))
	popup.hide()
	await process_frame
	await process_frame
	check(view.mode == MapMode.Type.PROVINCE and view.mode_option.get_selected_id() == view.mode, "下拉菜单应切换省份模式")
	check(view.selected_country.is_empty() and view.selected_province == 0, "切换模式应清除旧选择")
	check(material.get_shader_parameter("selected_country") == 0 and material.get_shader_parameter("selected_province") == 0, "切换模式应清除两种高亮")
	check(view.map_viewport.canvas_transform.is_equal_approx(transform_before), "切换模式应保留缩放和位置")
	check(inspector.tabs.current_tab == 0 and inspector.province_title.text == "未选择省份", "省份模式应打开省份页并清除信息")
	view.reset_view()
	await click_pixel(view, Vector2i(0, 0))
	check(view.selected_province == 7 and view.selected_country.is_empty(), "省份模式只能选中单个省份")
	if DisplayServer.get_name() != "headless":
		await RenderingServer.frame_post_draw
		var image := view.map_viewport.get_texture().get_image()
		check(not pixel_color(view, image, Vector2i(0, 0)).is_equal_approx(pixel_color(view, image, Vector2i(4, 0))), "省份高亮不能扩展到同国其他省份")
	view.set_mode(MapMode.Type.PROVINCE)
	check(view.selected_province == 7, "设置相同模式应保留当前选择")
	for pixel in [Vector2i(4, 0), Vector2i(2, 0), Vector2i(3, 1), Vector2i(4, 1)]:
		await click_pixel(view, pixel)
		check(view.selected_province == backend.province_at(pixel), "省份模式应保留完整省份编号，包括海洋和无主省份")
		check(not material.get_shader_parameter("country_mode") and material.get_shader_parameter("selected_province") == view.selected_province, "省份选择应传给着色器")
	await click_pixel(view, Vector2i(2, 1))
	check(view.selected_province == 0, "省份模式点击空白应清除选择")
	view.set_mode(MapMode.Type.COUNTRY)
	check(view.selected_country.is_empty() and view.selected_province == 0, "返回国家模式不能恢复先前选择")
	check(view.map_sprite.texture == texture and backend.get_owner_texture() == owners, "模式与选择变化应复用已载入的纹理")
	check(material.get_shader_parameter("owner_ids") == owners, "切换模式不能重新上传归属纹理")
	if failures.is_empty():
		print("Godot 地图模式测试通过")
	quit(0 if failures.is_empty() else 1)


func pixel_color(view: ProvinceMapView, image: Image, pixel: Vector2i) -> Color:
	return image.get_pixelv(Vector2i(view.map_viewport.canvas_transform * (Vector2(pixel) + Vector2(0.5, 0.5))))


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
