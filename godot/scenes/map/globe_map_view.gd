# SPDX-License-Identifier: GPL-3.0-only
class_name GlobeMapView
extends Control

const RADIUS := 1.0
const MAX_ZOOM := 8.0
const ROTATION_SPEED := 0.006
const MAX_PITCH := deg_to_rad(85.0)
const DEFAULT_YAW := deg_to_rad(10.0)
const DEFAULT_PITCH := deg_to_rad(25.0)

@onready var map_viewport: SubViewport = $ViewportContainer/GlobeViewport
@onready var globe: MeshInstance3D = $ViewportContainer/GlobeViewport/Globe
@onready var camera: Camera3D = $ViewportContainer/GlobeViewport/Camera
@onready var surface_material: ShaderMaterial = globe.material_override

var zoom_level := 1.0
var yaw := DEFAULT_YAW
var pitch := DEFAULT_PITCH
var _map_size := Vector2i.ZERO


func _ready() -> void:
	resized.connect(update_camera.call_deferred)
	update_camera.call_deferred()


func show_map(backend: AugustusMap) -> void:
	_map_size = backend.get_map_size()
	surface_material.set_shader_parameter("province_ids", backend.get_map_texture())
	surface_material.set_shader_parameter("owner_ids", backend.get_owner_texture())
	surface_material.set_shader_parameter("political_colors", backend.get_political_texture())
	reset_view()


func texture_pixel_at(local_position: Vector2) -> Vector2i:
	if _map_size == Vector2i.ZERO or not Rect2(Vector2.ZERO, size).has_point(local_position):
		return Vector2i(-1, -1)
	var viewport_point := local_position * Vector2(map_viewport.size) / size
	var origin := camera.project_ray_origin(viewport_point)
	var direction := camera.project_ray_normal(viewport_point)
	# 使用相机的真实透视射线和 Godot 的球体相交接口，只取球面近侧。
	var hit := Geometry3D.segment_intersects_sphere(origin, origin + direction * camera.far, Vector3.ZERO, RADIUS)
	if hit.is_empty():
		return Vector2i(-1, -1)
	var point := hit[0].normalized()
	var uv := Vector2(fposmod(atan2(point.x, point.z) / TAU + 0.5, 1.0), acos(clampf(point.y, -1.0, 1.0)) / PI)
	return Vector2i(mini(floori(uv.x * _map_size.x), _map_size.x - 1), mini(floori(uv.y * _map_size.y), _map_size.y - 1))


func rotate_by(relative: Vector2) -> void:
	yaw = wrapf(yaw - relative.x * ROTATION_SPEED, -PI, PI)
	pitch = clampf(pitch + relative.y * ROTATION_SPEED, -MAX_PITCH, MAX_PITCH)
	update_camera()


func project_marker(pixel: Vector2) -> Vector2:
	if _map_size == Vector2i.ZERO:
		return Vector2.INF
	return _project_surface(_surface_point(pixel))


func project_path_point(from: Vector2, to: Vector2, fraction: float) -> Vector2:
	if _map_size == Vector2i.ZERO:
		return Vector2.INF
	# 球面路线沿大圆弧插值，不把平面直线直接覆盖到球体上。
	return _project_surface(_surface_point(from).slerp(_surface_point(to), fraction))


func focus_pixel(pixel: Vector2) -> void:
	_focus_surface(_surface_point(pixel))


func focus_path(from: Vector2, to: Vector2, fraction: float) -> void:
	_focus_surface(_surface_point(from).slerp(_surface_point(to), fraction))


func _focus_surface(point: Vector3) -> void:
	var direction := point.normalized()
	yaw = atan2(direction.x, direction.z)
	pitch = clampf(asin(direction.y), -MAX_PITCH, MAX_PITCH)
	zoom_level = maxf(zoom_level, 3.0)
	update_camera()


func _surface_point(pixel: Vector2) -> Vector3:
	var uv := pixel / Vector2(_map_size)
	var longitude := (uv.x - 0.5) * TAU
	var latitude := uv.y * PI
	return Vector3(sin(longitude) * sin(latitude), cos(latitude), cos(longitude) * sin(latitude)) * RADIUS


func _project_surface(point: Vector3) -> Vector2:
	var world_point := globe.to_global(point)
	# 透视投影前排除球体背面；球后的驻军不能透过地图出现或被选中。
	if (world_point - globe.global_position).dot(camera.global_position - world_point) <= 0.0 or camera.is_position_behind(world_point):
		return Vector2.INF
	return camera.unproject_position(world_point) * size / Vector2(map_viewport.size)


func zoom_by(factor: float) -> void:
	zoom_level = clampf(zoom_level * factor, 1.0, MAX_ZOOM)
	update_camera()


func reset_view() -> void:
	zoom_level = 1.0
	yaw = DEFAULT_YAW
	pitch = DEFAULT_PITCH
	update_camera()


func update_camera() -> void:
	if size.x <= 0.0 or size.y <= 0.0:
		return
	var half_fov := deg_to_rad(camera.fov * 0.5)
	var limiting_fov := minf(half_fov, atan(tan(half_fov) * size.x / size.y))
	# 全图距离随窗口宽高比调整；放大后相机始终停留在球面外。
	var fit_distance := RADIUS * 1.15 / sin(limiting_fov)
	var distance := RADIUS + (fit_distance - RADIUS) / zoom_level
	camera.position = Vector3(sin(yaw) * cos(pitch), sin(pitch), cos(yaw) * cos(pitch)) * distance
	# 局部朝向直接由轨道位置计算，场景切换期间也不依赖全局变换。
	camera.basis = Basis.looking_at(-camera.position, Vector3.UP)
