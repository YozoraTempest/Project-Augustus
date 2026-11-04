# SPDX-License-Identifier: GPL-3.0-only
extends Control

@onready var path_input: LineEdit = %GameDirectory
@onready var browse_button: Button = %Browse
@onready var load_button: Button = %Load
@onready var status_label: Label = %Status
@onready var map_view: ProvinceMapView = %MapView
@onready var empty_label: Label = %EmptyMap
@onready var selection_label: Label = %Selection
@onready var directory_dialog: FileDialog = $DirectoryDialog

var backend := AugustusMap.new()
var selected_province := 0


func _ready() -> void:
	browse_button.pressed.connect(func(): directory_dialog.popup_centered_ratio(0.7))
	load_button.pressed.connect(func(): load_map(path_input.text))
	path_input.text_submitted.connect(load_map)
	directory_dialog.dir_selected.connect(load_map)
	map_view.province_clicked.connect(_on_province_clicked)
	# 命令行路径仅用于启动时自动载入，普通启动仍可通过界面选择目录。
	var arguments := OS.get_cmdline_user_args()
	var path_index := arguments.find("--game-dir")
	if path_index >= 0 and path_index + 1 < arguments.size():
		load_map(arguments[path_index + 1])


func load_map(game_dir: String) -> bool:
	path_input.text = game_dir
	status_label.text = "正在载入地图…"
	status_label.modulate = Color.WHITE
	_set_loading(true)
	# 先显示载入状态，再执行内容读取与纹理创建。
	await get_tree().process_frame
	await get_tree().process_frame
	var error := backend.load_map(game_dir)
	_set_loading(false)
	if not error.is_empty():
		status_label.text = "载入失败：" + error
		status_label.modulate = Color(1.0, 0.55, 0.48)
		return false
	map_view.show_map(backend.get_map_texture())
	empty_label.hide()
	selected_province = 0
	selection_label.text = "未选择省份"
	var map_size := backend.get_map_size()
	status_label.text = "%d × %d 像素 · %d 个省份" % [map_size.x, map_size.y, backend.get_province_count()]
	return true


func _set_loading(loading: bool) -> void:
	load_button.disabled = loading
	browse_button.disabled = loading
	path_input.editable = not loading


func _on_province_clicked(pixel: Vector2i) -> void:
	var province_id := backend.province_at(pixel)
	if province_id < 0:
		return
	selected_province = province_id
	map_view.select_province(province_id)
	if province_id == 0:
		selection_label.text = "此位置没有省份 · 像素 (%d, %d)" % [pixel.x, pixel.y]
	else:
		selection_label.text = "省份 #%d · 像素 (%d, %d)" % [province_id, pixel.x, pixel.y]
