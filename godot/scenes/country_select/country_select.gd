# SPDX-License-Identifier: GPL-3.0-only
extends Control

const LAUNCHER := "res://scenes/launcher/launcher.tscn"
const MAIN_MENU := "res://scenes/main_menu/main_menu.tscn"
const GAME_SCENE := "res://scenes/game/game.tscn"

@onready var country_color: ColorRect = %CountryColor
@onready var country_name: Label = %CountryName
@onready var country_hint: Label = %Hint
@onready var error_label: Label = %Error
@onready var back_button: Button = %Back
@onready var start_button: Button = %StartGame
@onready var map_view: ProvinceMapView = %MapView

var _backend: AugustusMap
var _chosen_country := ""


func _ready() -> void:
	if not GameSession.has_game() or GameSession.is_started():
		var target := GAME_SCENE if GameSession.is_started() else LAUNCHER
		get_tree().change_scene_to_file.call_deferred(target)
		return
	AppSettings.apply_game_window()
	_backend = GameSession.backend
	back_button.pressed.connect(_return_to_menu)
	start_button.pressed.connect(_enter_game)
	map_view.selection_changed.connect(_refresh_selection)
	_clear_country()
	# 等地图有了尺寸再铺满，并关掉这张图上的军队和模式开关。
	await get_tree().process_frame
	if not is_inside_tree() or _backend == null:
		return
	map_view.present_for_country_pick()
	map_view.show_map(_backend)
	var pending := GameSession.startup_country
	GameSession.startup_country = ""
	if pending.is_empty():
		return
	map_view.select_country(pending)
	if not _chosen_country.is_empty():
		_enter_game()


func _refresh_selection() -> void:
	if _backend == null:
		return
	var tag := map_view.selected_country
	if tag.is_empty():
		_clear_country()
		return
	var info := _backend.get_country_info(tag)
	if info.is_empty():
		_clear_country()
		return
	country_color.show()
	country_color.color = info.color
	country_name.text = str(info.name)
	country_hint.hide()
	error_label.hide()
	if int(info.province_count) == 0:
		_chosen_country = ""
		start_button.disabled = true
		_show_error("这个国家没有领土，不能开始。")
		return
	# 左键点中的国家就是当前选择，不再要求另按一次确认。
	_chosen_country = tag
	start_button.disabled = false


func _clear_country() -> void:
	_chosen_country = ""
	start_button.disabled = true
	country_color.hide()
	country_name.text = "未选择国家"
	country_hint.text = "在地图上点选一个国家"
	country_hint.show()
	error_label.hide()


func _enter_game() -> void:
	if _backend == null or _backend != GameSession.backend or _chosen_country.is_empty():
		return
	var error: String = GameSession.start_game(_chosen_country)
	if not error.is_empty():
		_show_error("无法开始游戏：" + error)
		return
	GameSession.map_projection = MapMode.ViewType.FLAT
	get_tree().change_scene_to_file(GAME_SCENE)


func _return_to_menu() -> void:
	get_tree().change_scene_to_file(MAIN_MENU)


func _show_error(message: String) -> void:
	error_label.text = message
	error_label.show()
