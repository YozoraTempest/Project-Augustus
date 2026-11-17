# SPDX-License-Identifier: GPL-3.0-only
extends Control

const LAUNCHER := "res://scenes/launcher/launcher.tscn"
const MAIN_MENU := "res://scenes/main_menu/main_menu.tscn"
const GAME_SCENE := "res://scenes/game/game.tscn"

@onready var back_button: Button = %Back
@onready var status_label: Label = %Status
@onready var map_view: ProvinceMapView = %MapView
@onready var info_panel: ProvinceInfoPanel = %InfoPanel
@onready var choose_button: Button = %ChooseCountry
@onready var chosen_label: Label = %ChosenCountry
@onready var reset_view_button: Button = %ResetView
@onready var enter_button: Button = %EnterGame

var _backend: AugustusMap
var _viewed_country := ""
var _chosen_country := ""


func _ready() -> void:
	if not GameSession.has_game() or GameSession.is_started():
		var target := GAME_SCENE if GameSession.is_started() else LAUNCHER
		get_tree().change_scene_to_file.call_deferred(target)
		return
	_backend = GameSession.backend
	back_button.pressed.connect(_return_to_menu)
	map_view.selection_changed.connect(_refresh_selection)
	map_view.projection_changed.connect(func(_projection: MapMode.ViewType) -> void: _refresh_projection())
	choose_button.pressed.connect(func() -> void: _choose_country(_viewed_country))
	reset_view_button.pressed.connect(map_view.reset_view)
	enter_button.pressed.connect(_enter_game)
	# 等布局完成后再铺地图，避免选国家页先停在一块空底上。
	await get_tree().process_frame
	if not is_inside_tree() or _backend == null:
		return
	map_view.show_map(_backend)
	_refresh_projection()
	status_label.text = "开局日期：" + _backend.get_start_date()
	var pending := GameSession.startup_country
	GameSession.startup_country = ""
	if not pending.is_empty() and _choose_country(pending):
		_enter_game()


func _refresh_selection() -> void:
	if _backend == null:
		return
	var province_info: Dictionary = {}
	var country_info: Dictionary = {}
	if map_view.mode == MapMode.Type.COUNTRY:
		country_info = _backend.get_country_info(map_view.selected_country)
	else:
		province_info = _backend.get_province_info(map_view.selected_province)
		if province_info.get("owner") != null:
			country_info = _backend.get_country_info(province_info.owner)
	info_panel.show_info(province_info, country_info, _backend.get_construction_info(map_view.selected_province), _backend.get_recruitment_info(map_view.selected_province))
	info_panel.tabs.current_tab = 1 if map_view.mode == MapMode.Type.COUNTRY else 0
	_viewed_country = "" if country_info.is_empty() else str(country_info.tag)
	choose_button.disabled = country_info.is_empty() or country_info.province_count == 0 or _viewed_country == _chosen_country
	choose_button.text = "已选择该国" if not _viewed_country.is_empty() and _viewed_country == _chosen_country else "选择该国"


func _choose_country(tag: String) -> bool:
	if _backend == null or _backend != GameSession.backend:
		return false
	var country_info := _backend.get_country_info(tag)
	if country_info.is_empty() or country_info.province_count == 0:
		_show_error("无法选择玩家国家：请选择开局时拥有领土的国家。")
		return false
	# 这里只记下候选。正式玩家身份在进入游戏时写入。
	_chosen_country = tag
	chosen_label.text = "玩家国家：%s (%s)" % [country_info.name, country_info.tag]
	chosen_label.tooltip_text = chosen_label.text
	enter_button.disabled = false
	status_label.text = "开局日期：" + _backend.get_start_date()
	status_label.modulate = Color.WHITE
	_refresh_selection()
	return true


func _refresh_projection() -> void:
	var is_globe := map_view.projection == MapMode.ViewType.GLOBE
	reset_view_button.text = "重置球体" if is_globe else "显示全图"


func _enter_game() -> void:
	if _backend == null or _backend != GameSession.backend or _chosen_country.is_empty():
		return
	var error: String = GameSession.start_game(_chosen_country)
	if not error.is_empty():
		_show_error("无法进入游戏：" + error)
		return
	GameSession.map_projection = map_view.projection
	get_tree().change_scene_to_file(GAME_SCENE)


func _return_to_menu() -> void:
	# 开局已经在启动器载入，返回主菜单时保留，便于再次选择国家。
	get_tree().change_scene_to_file(MAIN_MENU)


func _show_error(message: String) -> void:
	status_label.text = message
	status_label.modulate = Color(1.0, 0.55, 0.48)
