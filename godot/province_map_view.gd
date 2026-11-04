# SPDX-License-Identifier: GPL-3.0-only
class_name ProvinceMapView
extends TextureRect

signal province_clicked(pixel: Vector2i)


func display_rect() -> Rect2:
	if texture == null:
		return Rect2()
	var image_size := texture.get_size()
	var factor := minf(size.x / image_size.x, size.y / image_size.y)
	var drawn_size := image_size * factor
	return Rect2((size - drawn_size) * 0.5, drawn_size)


func texture_pixel_at(local_position: Vector2) -> Vector2i:
	var drawn := display_rect()
	# 必须先排除保持宽高比产生的留白，不能将留白映射到边缘省份。
	if not drawn.has_point(local_position) or not drawn.has_area():
		return Vector2i(-1, -1)
	var uv := (local_position - drawn.position) / drawn.size
	return Vector2i((uv * texture.get_size()).floor())


func show_map(map_texture: Texture2D) -> void:
	texture = map_texture
	material.set_shader_parameter("province_ids", map_texture)
	select_province(0)


func select_province(province_id: int) -> void:
	# 只修改着色器参数，点击时无需重新生成或上传整张地图。
	material.set_shader_parameter("selected_province", province_id)


func _gui_input(event: InputEvent) -> void:
	if event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
		var pixel := texture_pixel_at(event.position)
		if pixel.x >= 0:
			province_clicked.emit(pixel)
			accept_event()
