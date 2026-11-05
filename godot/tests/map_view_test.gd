# SPDX-License-Identifier: GPL-3.0-only
extends SceneTree

const MAIN_MENU := "res://scenes/main_menu/main_menu.tscn"
const NEW_GAME := "res://scenes/new_game/new_game.tscn"
const GAME_SCENE := "res://scenes/game/game.tscn"

var failures: Array[String] = []
var session: Node
var preferences: Node


func _init() -> void:
	_run.call_deferred()


func check(condition: bool, message: String) -> void:
	if not condition:
		failures.append(message)
		push_error(message)


func _run() -> void:
	root.size = Vector2i(1000, 700)
	await process_frame
	session = root.get_node("GameSession")
	preferences = root.get_node("AppSettings")
	# 设置测试使用构建目录中的独立文件，避免改动本机用户偏好。
	var settings_directory := ProjectSettings.globalize_path("res://../target/godot-settings-tests")
	check(DirAccess.make_dir_recursive_absolute(settings_directory) == OK, "应能创建设置测试目录")
	preferences.config_path = settings_directory.path_join("display-%d.cfg" % OS.get_process_id())
	check(preferences.save_and_apply(Window.MODE_WINDOWED, true) == OK, "应能写入初始设置")
	change_scene_to_file(ProjectSettings.get_setting("application/run/main_scene"))
	var menu = await wait_for_scene(MAIN_MENU)
	if menu == null:
		return
	check(not session.has_game(), "正常启动应显示主菜单且不创建游戏会话")
	await menu_settings_checks(menu)
	await click_control(menu.get_node("%NewGame"))
	var setup = await wait_for_scene(NEW_GAME)
	if setup == null:
		return
	check(setup.enter_button.disabled, "开局尚未载入时不能进入游戏")
	check(setup.load_button.disabled, "空目录不能发起载入")
	# 新游戏页可以取消，返回后不遗留会话。
	await click_control(setup.back_button)
	menu = await wait_for_scene(MAIN_MENU)
	if menu == null:
		return
	check(not session.has_game(), "取消新游戏不应创建会话")
	await click_control(menu.get_node("%NewGame"))
	setup = await wait_for_scene(NEW_GAME)
	if setup == null:
		return
	var directory := ProjectSettings.globalize_path("res://tests/data/game")
	await load_directory(setup, directory.path_join("missing"))
	check(current_scene == setup and setup.enter_button.disabled, "载入失败应留在新游戏页且不能进入游戏")
	check(setup.status_label.text.contains("载入失败") and not session.has_game(), "首次载入失败应显示原因且没有会话")
	await load_directory(setup, directory)
	check(not setup.enter_button.disabled and session.has_game(), "修正目录后应能成功载入开局")
	check(current_scene == setup, "手动载入成功后应等待用户点击进入游戏")
	var loaded_backend = session.backend
	# 修改来源后必须重新载入，失败不能把旧会话当成新目录的开局。
	await load_directory(setup, directory.path_join("missing"))
	check(session.backend == loaded_backend, "新开局载入失败应保留先前成功的会话")
	check(setup.enter_button.disabled, "失败的开局不能通过进入按钮继续")
	await load_directory(setup, directory)
	loaded_backend = session.backend
	var loaded_texture: Texture2D = loaded_backend.get_map_texture()
	await click_control(setup.enter_button)
	var screen = await wait_for_scene(GAME_SCENE)
	if screen == null:
		return
	check(not is_instance_valid(setup), "进入游戏后应释放新游戏页面")
	check(screen.backend == loaded_backend, "场景切换应复用已载入的 Rust 会话")
	var view: ProvinceMapView = screen.map_view
	check(view.map_sprite.texture == loaded_texture, "进入游戏不应重新生成纹理")
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
	check(screen.backend.get_province_info(-1).is_empty() and screen.backend.get_province_info(999).is_empty(), "未知省份不应返回信息")
	check(screen.backend.get_country_info("ZZZ").is_empty(), "未知国家不应返回信息")
	var details: Dictionary = screen.backend.get_province_info(7)
	check(details.name == "Riverland" and details.owner_name == "First Kingdom", "省份与国家名称应读取本地化")
	check(details.controller == "AAA", "没有显式控制者时应使用最终所有者")
	check(details.culture == "River Culture" and details.religion == "Catholic" and details.trade_good == "Grain", "省份文化、宗教和商品应本地化")
	check(details.base_tax == 3 and details.base_production == 3 and details.base_manpower == 4 and details.development == 10, "绑定应返回开局当天的三项发展度及总和")
	var country_details: Dictionary = screen.backend.get_country_info("BBB")
	check(country_details.capital == 16777217 and country_details.capital_name == "Highland", "国家首都应保留完整编号并读取名称")
	check(country_details.province_count == 1 and country_details.development == 15, "国家应只统计本国领土与发展度")
	check(country_details.primary_culture == "Highland Culture" and country_details.religion == "Orthodox", "国家文化与国教应读取历史并本地化")
	var inspector: ProvinceInfoPanel = screen.info_panel
	check(inspector.province_title.text == "未选择省份" and inspector.country_title.text == "未选择国家", "信息面板初始应为空")

	# 通过窗口输入分发点击，检查坐标转换、绑定查询和界面更新的完整路径。
	await click_pixel(view, Vector2i(0, 0))
	check(screen.selected_province == 7, "点击左上像素应选择省份 7")
	check(screen.selection_label.text.contains("#7"), "底部应显示选中的省份编号")
	check(screen.selection_label.text.contains("所属国家：AAA"), "选择省份应显示其所有者")
	check(view.map_sprite.material.get_shader_parameter("selected_province") == 7, "选中状态应传给着色器")
	check(inspector.province_title.text == "Riverland" and inspector.province_body.text.contains("总发展度：10"), "点击地图应更新省份名称与发展度")
	check(inspector.country_title.text == "First Kingdom" and inspector.country_body.text.contains("Riverland (#7)"), "国家页应显示所属国家与首都")
	await click_control(inspector.country_button)
	check(inspector.tabs.current_tab == 1, "查看所属国家按钮应切换到国家页")
	await click_pixel(view, Vector2i(1, 0))
	check(screen.selected_province == 16777217, "大编号省份应能准确选中")
	check(screen.selection_label.text.contains("所属国家：BBB"), "切换选择后应更新国家归属")
	check(inspector.province_title.text == "Highland" and inspector.country_title.text == "Second Republic", "切换省份应更新两个页签")
	check(inspector.province_body.text.contains("所属国家：Second Republic (BBB)") and inspector.province_body.text.contains("控制者：First Kingdom (AAA)"), "省份面板应区分所有者与控制者")
	check(inspector.country_color.color.is_equal_approx(Color8(210, 50, 20)), "国家色标应与政治地图一致")
	await navigation_checks(screen, view)
	await game_settings_checks(screen, view)

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
	check(inspector.province_title.text == "未选择省份" and inspector.country_title.text == "未选择国家", "清除选择应清除两个信息页签")
	check(not inspector.country_button.visible and not inspector.country_color.visible, "无选择时不应保留国家按钮或色标")

	await click_pixel(view, Vector2i(0, 0))
	var previous_texture: Texture2D = view.map_sprite.texture
	view.zoom_at(view.size * 0.5, 3.0)
	var previous_transform := view.map_viewport.canvas_transform
	check(not session.load_game(directory.path_join("missing")).is_empty(), "不存在的目录应报告载入失败")
	check(view.map_sprite.texture == previous_texture, "载入失败应保留旧地图")
	check(screen.backend.get_political_texture() == political_texture, "载入失败应保留政治颜色")
	check(screen.backend.get_province_owner(7) == "AAA", "载入失败应保留开局归属")
	check(screen.selected_province == 7, "载入失败应保留原选择")
	check(view.map_viewport.canvas_transform.is_equal_approx(previous_transform), "载入失败应保留缩放与位置")
	# 通过实际页面按钮结束会话，再开始一局，检查场景与选择状态的生命周期。
	await click_control(screen.menu_button)
	menu = await wait_for_scene(MAIN_MENU)
	if menu == null:
		return
	check(not is_instance_valid(screen), "返回主菜单后应释放游戏场景")
	check(not session.has_game() and session.backend == null, "返回主菜单应结束游戏会话")
	await click_control(menu.get_node("%NewGame"))
	setup = await wait_for_scene(NEW_GAME)
	if setup == null:
		return
	check(setup.path_input.text == directory, "新开局应记住上一次成功使用的目录")
	check(setup.enter_button.disabled, "新的一局应重新载入开局")
	await load_directory(setup, directory)
	await click_control(setup.enter_button)
	screen = await wait_for_scene(GAME_SCENE)
	if screen == null:
		return
	view = screen.map_view
	check(screen.backend != loaded_backend, "重新开始应创建独立的游戏会话")
	check(screen.selected_province == 0, "重新载入成功后应清除旧选择")
	check(view.map_sprite.material.get_shader_parameter("selected_province") == 0, "重新载入应清除高亮")
	check(is_equal_approx(view.zoom_level, 1.0), "重新载入成功后应显示全图")
	await click_control(screen.menu_button)
	menu = await wait_for_scene(MAIN_MENU)
	if menu == null:
		return
	# 无会话时单独运行游戏场景，应进入开局页而不是显示空地图。
	change_scene_to_file(GAME_SCENE)
	setup = await wait_for_scene(NEW_GAME)
	if setup == null:
		return
	check(not session.has_game(), "单独打开游戏场景不应创建空世界")
	await load_directory(setup, directory)
	await click_control(setup.back_button)
	menu = await wait_for_scene(MAIN_MENU)
	if menu == null:
		return
	check(not session.has_game(), "取消已载入的开局也应释放会话")
	check(DirAccess.remove_absolute(preferences.config_path) == OK, "应能清理本次设置测试文件")
	if failures.is_empty():
		print("Godot 场景流程与地图集成测试通过")
	quit(0 if failures.is_empty() else 1)


func menu_settings_checks(menu: Control) -> void:
	await click_control(menu.settings_button)
	var dialog: Window = menu.get_node("SettingsDialog")
	check(paused and dialog.visible and dialog.exclusive, "设置应以模态窗口打开并暂停底层页面")
	check(dialog.mode_option.get_selected_id() == Window.MODE_WINDOWED and dialog.vsync_toggle.button_pressed, "设置窗口应显示当前偏好")
	dialog.mode_option.select(dialog.mode_option.get_item_index(Window.MODE_FULLSCREEN))
	dialog.vsync_toggle.button_pressed = false
	dialog.cancel_button.pressed.emit()
	await process_frame
	await process_frame
	check(not paused and not is_instance_valid(dialog), "取消应关闭窗口并恢复页面处理")
	check(preferences.window_mode == Window.MODE_WINDOWED and preferences.vsync_enabled, "取消不应应用草稿")
	await click_control(menu.settings_button)
	dialog = menu.get_node("SettingsDialog")
	check(dialog.mode_option.get_selected_id() == Window.MODE_WINDOWED and dialog.vsync_toggle.button_pressed, "重新打开应丢弃之前取消的草稿")
	dialog.mode_option.select(dialog.mode_option.get_item_index(Window.MODE_FULLSCREEN))
	dialog.vsync_toggle.button_pressed = false
	dialog.apply_button.pressed.emit()
	await process_frame
	await process_frame
	check(not paused and not is_instance_valid(dialog), "应用成功应关闭设置并恢复页面处理")
	check(preferences.window_mode == Window.MODE_FULLSCREEN and not preferences.vsync_enabled, "应用后应更新显示偏好")
	# 用新的设置实例读取磁盘，验证下次启动能恢复保存的选项。
	var restored = load("res://autoload/app_settings.gd").new()
	restored.config_path = preferences.config_path
	check(restored.load_settings() == OK, "已保存的设置应能重新读取")
	check(restored.window_mode == Window.MODE_FULLSCREEN and not restored.vsync_enabled, "重新读取应恢复窗口模式与垂直同步")
	restored.free()
	check(preferences.save_and_apply(Window.MODE_WINDOWED, true) == OK, "测试后应恢复窗口化选项")
	root.size = Vector2i(1000, 700)
	await process_frame
	await process_frame
	await click_control(menu.settings_button)
	dialog = menu.get_node("SettingsDialog")
	var escape := InputEventKey.new()
	escape.keycode = KEY_ESCAPE
	escape.pressed = true
	dialog.push_input(escape)
	await process_frame
	await process_frame
	check(not paused and not is_instance_valid(dialog), "Escape 应取消设置并恢复页面处理")
	check(current_scene == menu and not session.has_game(), "主菜单设置不能创建或切换游戏会话")


func game_settings_checks(screen: Control, view: ProvinceMapView) -> void:
	var previous_backend = screen.backend
	var previous_texture := view.map_sprite.texture
	var previous_selection: int = screen.selected_province
	var point := view.size * 0.5
	view.zoom_at(point, 2.0)
	await mouse_button(view.global_position + point, MOUSE_BUTTON_MIDDLE, true)
	check(view._dragging, "中键按下后应开始拖动")
	var previous_transform := view.map_viewport.canvas_transform
	screen.settings_button.pressed.emit()
	await process_frame
	await process_frame
	var dialog: Window = screen.get_node("SettingsDialog")
	check(paused and not view._dragging, "打开设置应暂停地图并结束当前拖动")
	await click_pixel(view, Vector2i(0, 0))
	await mouse_button(view.global_position + point, MOUSE_BUTTON_WHEEL_UP, true)
	await mouse_button(view.global_position + point, MOUSE_BUTTON_WHEEL_UP, false)
	await mouse_button(view.global_position + point, MOUSE_BUTTON_MIDDLE, false)
	check(screen.selected_province == previous_selection, "设置窗口打开时不能选择底层地图")
	check(view.map_viewport.canvas_transform.is_equal_approx(previous_transform), "设置窗口打开时不能缩放或拖动底层地图")
	dialog.cancel_button.pressed.emit()
	await process_frame
	await process_frame
	check(not paused and not is_instance_valid(dialog), "关闭游戏内设置应恢复游戏页面")
	check(session.backend == previous_backend and screen.backend == previous_backend, "关闭设置应保留同一个游戏会话")
	check(view.map_sprite.texture == previous_texture and screen.selected_province == previous_selection, "关闭设置应保留地图纹理和省份选择")
	await mouse_motion(view.global_position + point, Vector2(20, 20), 0)
	check(view.map_viewport.canvas_transform.is_equal_approx(previous_transform), "暂停期间松开中键后不能继续拖动")
	view.reset_view()
	await click_pixel(view, Vector2i(1, 0))
	check(screen.selected_province == 16777217, "关闭设置后地图应继续响应省份选择")


func wait_for_scene(path: String) -> Node:
	# 场景切换在帧结束时完成；等待新页面入树并完成布局后再分发输入。
	for frame in range(30):
		if current_scene != null and current_scene.scene_file_path == path:
			await process_frame
			await process_frame
			return current_scene
		await process_frame
	check(false, "未能进入场景：" + path)
	quit(1)
	return null


func load_directory(setup: Control, directory: String) -> void:
	setup.path_input.text = directory
	setup.path_input.text_changed.emit(directory)
	check(setup.enter_button.disabled, "修改目录后应禁用进入游戏")
	await click_control(setup.load_button)
	for frame in range(30):
		if not setup._loading:
			return
		await process_frame
	check(false, "开局载入未完成")


func click_control(control: Control) -> void:
	await click_at(control.global_position + control.size * 0.5)


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
