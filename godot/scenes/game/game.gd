# SPDX-License-Identifier: GPL-3.0-only
extends Control

const MAIN_MENU := "res://scenes/main_menu/main_menu.tscn"
const NEW_GAME := "res://scenes/new_game/new_game.tscn"
const SETTINGS_SCENE := preload("res://scenes/settings/settings_dialog.tscn")

@onready var status_label: Label = %Status
@onready var map_view: ProvinceMapView = %MapView
@onready var selection_label: Label = %Selection
@onready var reset_view_button: Button = %ResetView
@onready var menu_button: Button = %MainMenu
@onready var settings_button: Button = %Settings

var backend: AugustusMap
var selected_province := 0


func _ready() -> void:
	# 从编辑器单独运行游戏场景时，也通过正常开局流程取得会话。
	if not GameSession.has_game():
		get_tree().change_scene_to_file.call_deferred(NEW_GAME)
		return
	backend = GameSession.backend
	map_view.province_clicked.connect(_on_province_clicked)
	reset_view_button.pressed.connect(map_view.reset_view)
	menu_button.pressed.connect(_return_to_menu)
	settings_button.pressed.connect(_open_settings)
	# 复用开局页已生成的纹理，进入游戏无需再次读取游戏文件。
	map_view.show_map(backend.get_map_texture(), backend.get_political_texture())
	var map_size := backend.get_map_size()
	status_label.text = "开局 %s · 政治地图 · %d 个有领土国家 · %d 个省份 · %d × %d 像素" % [backend.get_start_date(), backend.get_country_count(), backend.get_province_count(), map_size.x, map_size.y]


func _return_to_menu() -> void:
	GameSession.clear_game()
	get_tree().change_scene_to_file(MAIN_MENU)


func _open_settings() -> void:
	var dialog := SETTINGS_SCENE.instantiate()
	add_child(dialog)
	dialog.popup_centered()


func _on_province_clicked(pixel: Vector2i) -> void:
	var province_id := backend.province_at(pixel)
	if province_id < 0:
		return
	selected_province = province_id
	map_view.select_province(province_id)
	if province_id == 0:
		selection_label.text = "此位置没有省份"
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
