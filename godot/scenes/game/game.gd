# SPDX-License-Identifier: GPL-3.0-only
extends Control

const MAIN_MENU := "res://scenes/main_menu/main_menu.tscn"
const NEW_GAME := "res://scenes/new_game/new_game.tscn"
const SETTINGS_SCENE := preload("res://scenes/settings/settings_dialog.tscn")

@onready var status_label: Label = %Status
@onready var map_view: ProvinceMapView = %MapView
@onready var info_panel: ProvinceInfoPanel = %InfoPanel
@onready var selection_label: Label = %Selection
@onready var reset_view_button: Button = %ResetView
@onready var menu_button: Button = %MainMenu
@onready var settings_button: Button = %Settings
@onready var navigation_hint: Label = %Hint
@onready var player_country_label: Label = %PlayerCountry
@onready var save_button: Button = %SaveGame
@onready var save_status: Label = %SaveStatus
@onready var save_dialog: GameSaveDialog = $SaveDialog
@onready var save_error_dialog: AcceptDialog = $SaveError

var backend: AugustusMap


func _ready() -> void:
	# 从编辑器单独运行游戏场景时，也通过正常开局流程取得会话。
	if not GameSession.is_started():
		get_tree().change_scene_to_file.call_deferred(NEW_GAME)
		return
	backend = GameSession.backend
	map_view.selection_changed.connect(_refresh_selection)
	info_panel.construction_requested.connect(_start_construction)
	info_panel.recruitment_requested.connect(_start_recruitment)
	info_panel.army_selected.connect(map_view.select_army)
	GameSession.clock_changed.connect(func(): _refresh_selection(false))
	GameSession.world_changed.connect(func(): _refresh_selection(false))
	map_view.projection_changed.connect(func(_projection: MapMode.ViewType): _refresh_projection())
	reset_view_button.pressed.connect(map_view.reset_view)
	menu_button.pressed.connect(_return_to_menu)
	settings_button.pressed.connect(_open_settings)
	save_button.pressed.connect(_open_save_dialog)
	save_dialog.file_selected.connect(_save_game)
	# 复用开局页已生成的纹理，进入游戏无需再次读取游戏文件。
	map_view.show_map(backend)
	map_view.set_projection(GameSession.map_projection)
	_refresh_projection()
	var player_info := backend.get_country_info(GameSession.get_player_country())
	player_country_label.text = "玩家国家：%s (%s)" % [player_info.name, player_info.tag]
	player_country_label.tooltip_text = player_country_label.text
	var map_size := backend.get_map_size()
	status_label.text = "开局 %s · 政治地图 · %d 个有领土国家 · %d 个省份 · %d × %d 像素" % [backend.get_start_date(), backend.get_country_count(), backend.get_province_count(), map_size.x, map_size.y]


func _return_to_menu() -> void:
	GameSession.clear_game()
	get_tree().change_scene_to_file(MAIN_MENU)


func _open_settings() -> void:
	var dialog := SETTINGS_SCENE.instantiate()
	add_child(dialog)
	dialog.popup_centered()


func _open_save_dialog() -> void:
	var filename := "%s_%s.augustus" % [GameSession.get_player_country(), GameSession.get_clock_state().date]
	var error := save_dialog.open_for_saving(filename)
	if not error.is_empty():
		_show_save_error(error)


func _save_game(path: String) -> void:
	var error := GameSession.save_game(path)
	if not error.is_empty():
		_show_save_error(error)
		return
	save_status.text = "已保存 · " + str(GameSession.get_clock_state().date)
	save_status.tooltip_text = ProjectSettings.globalize_path(path)


func _show_save_error(message: String) -> void:
	save_error_dialog.dialog_text = message
	save_error_dialog.popup_centered()


func _refresh_projection() -> void:
	var is_globe := map_view.projection == MapMode.ViewType.GLOBE
	reset_view_button.text = "重置球体" if is_globe else "显示全图"
	navigation_hint.text = "滚轮缩放 · 中键旋转 · 左键选择" if is_globe else "滚轮缩放 · 中键拖动 · 左键选择"


func _start_construction(province_id: int, building: String) -> void:
	var error := GameSession.start_construction(province_id, building)
	if not error.is_empty():
		_refresh_selection(false)
		info_panel.show_construction_error(error)


func _refresh_selection(select_tab: bool = true) -> void:
	map_view.refresh_armies()
	# 地图模块决定选中了什么；游戏场景只查询并显示对应信息。
	var province_id := map_view.selected_province
	var province_info: Dictionary = {}
	var country_info: Dictionary = {}
	var army := backend.get_army_info(map_view.selected_army)
	if not army.is_empty():
		province_id = army.province
		province_info = backend.get_province_info(province_id)
		country_info = backend.get_country_info(army.country)
	elif map_view.mode == MapMode.Type.COUNTRY:
		country_info = backend.get_country_info(map_view.selected_country)
	else:
		province_info = backend.get_province_info(province_id)
		if province_info.get("owner") != null:
			country_info = backend.get_country_info(province_info.owner)
	info_panel.show_info(province_info, country_info, backend.get_construction_info(province_id), backend.get_recruitment_info(province_id), map_view.selected_army)
	# 日期推进会刷新财政；保留玩家手动选中的信息页签。
	if select_tab:
		info_panel.tabs.current_tab = 3 if not army.is_empty() else (1 if map_view.mode == MapMode.Type.COUNTRY else 0)
	if not army.is_empty():
		selection_label.text = "%s · %d 人 · 驻扎 %s" % [army.name, army.strength, army.province_name]
	elif map_view.mode == MapMode.Type.COUNTRY:
		selection_label.text = "未选择国家" if country_info.is_empty() else "国家：%s (%s)" % [country_info.name, country_info.tag]
	elif province_id == 0:
		selection_label.text = "未选择省份"
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
	selection_label.tooltip_text = selection_label.text


func _start_recruitment(province_id: int) -> void:
	var error := GameSession.start_recruitment(province_id)
	if not error.is_empty():
		_refresh_selection(false)
		info_panel.show_recruitment_error(error)
