# SPDX-License-Identifier: GPL-3.0-only
class_name ProvinceMapView
extends Control

signal mode_changed(mode: MapMode.Type)
signal selection_changed

const ZOOM_STEP := 1.25
const MAX_ZOOM := 32.0

@onready var map_viewport: SubViewport = $ViewportContainer/MapViewport
@onready var map_sprite: Sprite2D = $ViewportContainer/MapViewport/Map
@onready var camera: Camera2D = $ViewportContainer/MapViewport/Camera
@onready var mode_option: OptionButton = %MapMode

var zoom_level := 1.0
var mode := MapMode.Type.COUNTRY
var selected_province := 0
var selected_country := ""
var _backend: AugustusMap
var _dragging := false


func _ready() -> void:
	# 容器调整尺寸后再更新相机，确保使用子视口的新尺寸。
	resized.connect(func(): _update_camera.call_deferred())
	get_window().focus_exited.connect(_end_drag)
	mode_option.add_item("国家模式", MapMode.Type.COUNTRY)
	mode_option.add_item("省份模式", MapMode.Type.PROVINCE)
	mode_option.item_selected.connect(func(index: int): set_mode(mode_option.get_item_id(index)))


func display_rect() -> Rect2:
	if map_sprite.texture == null:
		return Rect2()
	var transform := map_viewport.canvas_transform
	return Rect2(transform * Vector2.ZERO, map_sprite.texture.get_size() * transform.get_scale())


func texture_pixel_at(local_position: Vector2) -> Vector2i:
	if map_sprite.texture == null or not Rect2(Vector2.ZERO, size).has_point(local_position):
		return Vector2i(-1, -1)
	# 反向使用绘制地图的相机变换，缩放、平移后仍查询原始像素。
	var pixel := map_viewport.canvas_transform.affine_inverse() * local_position
	if not Rect2(Vector2.ZERO, map_sprite.texture.get_size()).has_point(pixel):
		return Vector2i(-1, -1)
	return Vector2i(pixel.floor())


func show_map(backend: AugustusMap) -> void:
	_backend = backend
	map_sprite.texture = backend.get_map_texture()
	map_sprite.material.set_shader_parameter("province_ids", backend.get_map_texture())
	map_sprite.material.set_shader_parameter("owner_ids", backend.get_owner_texture())
	map_sprite.material.set_shader_parameter("political_colors", backend.get_political_texture())
	clear_selection()
	reset_view()


func set_mode(next_mode: MapMode.Type) -> void:
	if mode == next_mode:
		return
	mode = next_mode
	mode_option.select(mode_option.get_item_index(mode))
	_end_drag()
	# 不把国家选择隐式转换为任意省份，也不恢复另一个模式的旧选择。
	clear_selection()
	mode_changed.emit(mode)


func select_at(pixel: Vector2i) -> void:
	var province_id := _backend.province_at(pixel)
	if province_id < 0:
		return
	if mode == MapMode.Type.COUNTRY:
		selected_province = 0
		selected_country = _backend.get_province_owner(province_id)
	else:
		selected_province = province_id
		selected_country = ""
	_update_selection()


func clear_selection() -> void:
	selected_province = 0
	selected_country = ""
	_update_selection()


func _update_selection() -> void:
	# 编号纹理只在载入时创建；切换模式和选择只更新着色器参数。
	map_sprite.material.set_shader_parameter("country_mode", mode == MapMode.Type.COUNTRY)
	map_sprite.material.set_shader_parameter("selected_province", selected_province)
	var country_id := _backend.get_country_render_id(selected_country) if _backend != null else 0
	map_sprite.material.set_shader_parameter("selected_country", country_id)
	selection_changed.emit()


func reset_view() -> void:
	_end_drag()
	zoom_level = 1.0
	if map_sprite.texture != null:
		camera.position = map_sprite.texture.get_size() * 0.5
		_update_camera()


func zoom_at(local_position: Vector2, factor: float) -> void:
	if map_sprite.texture == null:
		return
	var next_zoom := clampf(zoom_level * factor, 1.0, MAX_ZOOM)
	if is_equal_approx(next_zoom, zoom_level):
		return
	var anchor := map_viewport.canvas_transform.affine_inverse() * local_position
	zoom_level = next_zoom
	_update_camera()
	# 补偿相机中心的变化，使鼠标指向的地图位置保持不动；到边界时由相机限位。
	camera.position += anchor - map_viewport.canvas_transform.affine_inverse() * local_position
	_update_camera()


func _update_camera() -> void:
	if map_sprite.texture == null or size.x <= 0.0 or size.y <= 0.0:
		return
	var image_size := map_sprite.texture.get_size()
	var viewport_size := Vector2(map_viewport.size)
	var fit_scale := minf(viewport_size.x / image_size.x, viewport_size.y / image_size.y)
	camera.zoom = Vector2.ONE * fit_scale * zoom_level
	var half_view := viewport_size / camera.zoom * 0.5
	var opposite_edge := image_size - half_view
	# 地图大于视口时不露底，小于视口时保持完整可见，并允许利用留白维持缩放锚点。
	var minimum := Vector2(minf(half_view.x, opposite_edge.x), minf(half_view.y, opposite_edge.y))
	var maximum := Vector2(maxf(half_view.x, opposite_edge.x), maxf(half_view.y, opposite_edge.y))
	camera.position = camera.position.clamp(minimum, maximum)
	# 相机限位使用整数，先按浮点范围约束中心，再向外取整以保留亚像素精度。
	camera.limit_left = floori(minf(0.0, image_size.x - half_view.x * 2.0))
	camera.limit_top = floori(minf(0.0, image_size.y - half_view.y * 2.0))
	camera.limit_right = ceili(maxf(image_size.x, half_view.x * 2.0))
	camera.limit_bottom = ceili(maxf(image_size.y, half_view.y * 2.0))
	camera.force_update_scroll()
	# 限位可能改变实际视图中心，将其同步回位置，拖到边缘后反向移动可以立即响应。
	camera.position = camera.get_screen_center_position()
	camera.force_update_scroll()


func _end_drag() -> void:
	_dragging = false
	mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND


func _notification(what: int) -> void:
	# 设置窗口暂停场景时结束拖动，避免暂停期间松开中键后仍保留拖动状态。
	if what == NOTIFICATION_PAUSED:
		_end_drag()


func _input(event: InputEvent) -> void:
	# 在地图外松开中键或窗口失焦，也应结束拖拽。
	if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_MIDDLE and not event.pressed:
		_end_drag()


func _gui_input(event: InputEvent) -> void:
	if map_sprite.texture == null:
		return
	if event is InputEventMouseMotion and _dragging:
		camera.position -= event.relative / camera.zoom
		_update_camera()
		accept_event()
	elif event is InputEventMouseButton:
		if event.button_index == MOUSE_BUTTON_MIDDLE:
			_dragging = event.pressed
			mouse_default_cursor_shape = Control.CURSOR_DRAG if _dragging else Control.CURSOR_POINTING_HAND
			accept_event()
		elif event.pressed and event.button_index in [MOUSE_BUTTON_WHEEL_UP, MOUSE_BUTTON_WHEEL_DOWN]:
			var factor := pow(ZOOM_STEP, event.factor)
			zoom_at(event.position, factor if event.button_index == MOUSE_BUTTON_WHEEL_UP else 1.0 / factor)
			accept_event()
		elif event.pressed and event.button_index == MOUSE_BUTTON_LEFT and not _dragging:
			var pixel := texture_pixel_at(event.position)
			if pixel.x >= 0:
				select_at(pixel)
				accept_event()
