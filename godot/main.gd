# SPDX-License-Identifier: GPL-3.0-only
extends Control

@onready var path_input: LineEdit = %GameDirectory
@onready var browse_button: Button = %Browse
@onready var load_button: Button = %Load
@onready var reset_view_button: Button = %ResetView
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
	reset_view_button.pressed.connect(map_view.reset_view)
	# 命令行路径仅用于启动时自动载入，普通启动仍可通过界面选择目录。
	var arguments := OS.get_cmdline_user_args()
	var path_index := arguments.find("--game-dir")
	if path_index >= 0 and path_index + 1 < arguments.size():
		load_map(arguments[path_index + 1])


func load_map(game_dir: String) -> bool:
	path_input.text = game_dir
	status_label.text = "正在载入开局与政治地图…"
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
	map_view.show_map(backend.get_map_texture(), backend.get_political_texture())
	reset_view_button.disabled = false
	empty_label.hide()
	selected_province = 0
	selection_label.text = "未选择省份"
	selection_label.tooltip_text = ""
	var map_size := backend.get_map_size()
	status_label.text = "开局 %s · 政治地图 · %d 个有领土国家 · %d 个省份 · %d × %d 像素" % [backend.get_start_date(), backend.get_country_count(), backend.get_province_count(), map_size.x, map_size.y]
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
		var owner := backend.get_province_owner(province_id)
		var ownership := "所属国家：" + owner
		if owner.is_empty():
			match backend.get_province_kind(province_id):
				"sea": ownership = "海洋"
				"lake": ownership = "湖泊"
				"wasteland": ownership = "荒地"
				_: ownership = "未殖民土地"
		selection_label.text = "省份 #%d · %s" % [province_id, ownership]
	# 窄窗口下文字可能被省略，悬停仍可查看完整信息。
	selection_label.tooltip_text = selection_label.text + " · 像素 (%d, %d)" % [pixel.x, pixel.y]
