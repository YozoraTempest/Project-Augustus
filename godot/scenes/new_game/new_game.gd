# SPDX-License-Identifier: GPL-3.0-only
extends Control

const MAIN_MENU := "res://scenes/main_menu/main_menu.tscn"
const GAME_SCENE := "res://scenes/game/game.tscn"

@onready var path_input: LineEdit = %GameDirectory
@onready var browse_button: Button = %Browse
@onready var load_button: Button = %Load
@onready var enter_button: Button = %EnterGame
@onready var back_button: Button = %Back
@onready var status_label: Label = %Status
@onready var directory_dialog: FileDialog = $DirectoryDialog
@onready var preview: HBoxContainer = %Preview
@onready var empty_state: Label = %EmptyState
@onready var map_view: ProvinceMapView = %MapView
@onready var info_panel: ProvinceInfoPanel = %InfoPanel
@onready var choose_button: Button = %ChooseCountry
@onready var chosen_label: Label = %ChosenCountry
@onready var reset_view_button: Button = %ResetView
@onready var navigation_hint: Label = %Hint

var auto_start := false
var _loading := false
var _preview_backend: AugustusMap
var _viewed_country := ""
var _chosen_country := ""


func _ready() -> void:
	path_input.text = GameSession.game_directory
	browse_button.pressed.connect(func(): directory_dialog.popup_centered_ratio(0.7))
	directory_dialog.dir_selected.connect(func(path: String):
		path_input.text = path
		_on_directory_changed(path))
	path_input.text_changed.connect(_on_directory_changed)
	path_input.text_submitted.connect(load_game)
	load_button.pressed.connect(func(): load_game(path_input.text))
	enter_button.pressed.connect(_enter_game)
	back_button.pressed.connect(_return_to_menu)
	map_view.selection_changed.connect(_refresh_selection)
	map_view.projection_changed.connect(func(_projection: MapMode.ViewType): _refresh_projection())
	choose_button.pressed.connect(func(): _choose_country(_viewed_country))
	reset_view_button.pressed.connect(map_view.reset_view)
	_on_directory_changed(path_input.text)
	path_input.grab_focus()
	if auto_start:
		_load_startup_game.call_deferred()


func load_game(directory: String) -> bool:
	if _loading:
		return false
	var path := directory.strip_edges()
	if path.is_empty():
		return false
	path_input.text = path
	status_label.text = "正在载入开局与政治地图…"
	status_label.modulate = Color.WHITE
	_set_loading(true)
	# 先绘制载入状态，再调用原生内容读取与纹理创建。
	await get_tree().process_frame
	await get_tree().process_frame
	var error: String = GameSession.load_game(path)
	_set_loading(false)
	if not error.is_empty():
		status_label.text = "载入失败：" + error
		status_label.modulate = Color(1.0, 0.55, 0.48)
		return false
	_preview_backend = GameSession.backend
	preview.show()
	empty_state.hide()
	reset_view_button.disabled = false
	map_view.show_map(_preview_backend)
	_refresh_projection()
	status_label.text = "开局 %s · %d 个有领土国家 · %d 个省份。点击地图查看国家，然后选择该国。" % [_preview_backend.get_start_date(), _preview_backend.get_country_count(), _preview_backend.get_province_count()]
	return true


func _load_startup_game() -> void:
	if await load_game(path_input.text) and not GameSession.startup_country.is_empty() and _choose_country(GameSession.startup_country):
		_enter_game()


func _on_directory_changed(path: String) -> void:
	# 修改开局来源后需重新载入，不能用旧会话进入新的目录。
	_invalidate_preview()
	load_button.disabled = _loading or path.strip_edges().is_empty()
	status_label.modulate = Color.WHITE
	status_label.text = "开局日期：1444.11.11\n请选择本地 EU4 安装目录，然后载入开局。"


func _set_loading(loading: bool) -> void:
	_loading = loading
	load_button.disabled = loading or path_input.text.strip_edges().is_empty()
	browse_button.disabled = loading
	back_button.disabled = loading
	path_input.editable = not loading
	enter_button.disabled = true
	if loading:
		_invalidate_preview()


func _invalidate_preview() -> void:
	_preview_backend = null
	_viewed_country = ""
	_chosen_country = ""
	preview.hide()
	empty_state.show()
	enter_button.disabled = true
	choose_button.disabled = true
	reset_view_button.disabled = true
	chosen_label.text = "玩家国家：未选择"


func _refresh_selection() -> void:
	var province_info: Dictionary = {}
	var country_info: Dictionary = {}
	if map_view.mode == MapMode.Type.COUNTRY:
		country_info = _preview_backend.get_country_info(map_view.selected_country)
	else:
		province_info = _preview_backend.get_province_info(map_view.selected_province)
		if province_info.get("owner") != null:
			country_info = _preview_backend.get_country_info(province_info.owner)
	info_panel.show_info(province_info, country_info)
	info_panel.tabs.current_tab = 1 if map_view.mode == MapMode.Type.COUNTRY else 0
	_viewed_country = "" if country_info.is_empty() else str(country_info.tag)
	choose_button.disabled = country_info.is_empty() or country_info.province_count == 0 or _viewed_country == _chosen_country
	choose_button.text = "已选择该国" if not _viewed_country.is_empty() and _viewed_country == _chosen_country else "选择该国"


func _choose_country(tag: String) -> bool:
	if _preview_backend == null or _preview_backend != GameSession.backend:
		return false
	var country_info := _preview_backend.get_country_info(tag)
	if country_info.is_empty() or country_info.province_count == 0:
		status_label.text = "无法选择玩家国家：请选择开局时拥有领土的国家。"
		status_label.modulate = Color(1.0, 0.55, 0.48)
		return false
	# 此处只确认开局候选；正式玩家身份在点击“开始游戏”时写入 Rust。
	_chosen_country = tag
	chosen_label.text = "玩家国家：%s (%s)" % [country_info.name, country_info.tag]
	chosen_label.tooltip_text = chosen_label.text
	enter_button.disabled = false
	status_label.text = "已选择 %s (%s)，可以开始游戏；也可以继续查看其他国家。" % [country_info.name, country_info.tag]
	status_label.modulate = Color.WHITE
	_refresh_selection()
	return true


func _refresh_projection() -> void:
	var is_globe := map_view.projection == MapMode.ViewType.GLOBE
	reset_view_button.text = "重置球体" if is_globe else "显示全图"
	navigation_hint.text = "滚轮缩放 · 中键旋转 · 左键选国" if is_globe else "滚轮缩放 · 中键拖动 · 左键选国"


func _enter_game() -> void:
	if _preview_backend == null or _preview_backend != GameSession.backend:
		return
	var error: String = GameSession.start_game(_chosen_country)
	if not error.is_empty():
		status_label.text = "无法开始游戏：" + error
		status_label.modulate = Color(1.0, 0.55, 0.48)
		return
	GameSession.map_projection = map_view.projection
	get_tree().change_scene_to_file(GAME_SCENE)


func _return_to_menu() -> void:
	GameSession.clear_game()
	get_tree().change_scene_to_file(MAIN_MENU)
