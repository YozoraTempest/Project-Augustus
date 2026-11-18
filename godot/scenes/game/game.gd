# SPDX-License-Identifier: GPL-3.0-only
extends Control

const MAIN_MENU := "res://scenes/main_menu/main_menu.tscn"
const LAUNCHER := "res://scenes/launcher/launcher.tscn"
const COUNTRY_SELECT := "res://scenes/country_select/country_select.tscn"
const SETTINGS_SCENE := preload("res://scenes/settings/settings_dialog.tscn")
const Eu4Type := preload("res://ui/eu4/eu4_type.gd")
const SESSION_TOPBAR := preload("res://scenes/game/session_topbar.gd")

@onready var map_view: ProvinceMapView = %MapView
@onready var info_panel: ProvinceInfoPanel = %InfoPanel
@onready var military_overview: CountryMilitaryOverview = %MilitaryOverview
@onready var military_overview_button: Button = %MilitaryOverviewButton
@onready var selection_label: Label = %Selection
@onready var reset_view_button: Button = %ResetView
@onready var menu_button: Button = %MainMenu
@onready var settings_button: Button = %Settings
@onready var player_country_label: Label = %PlayerCountry
@onready var save_button: Button = %SaveGame
@onready var save_status: Label = %SaveStatus
@onready var save_dialog: GameSaveDialog = $SaveDialog
@onready var save_error_dialog: AcceptDialog = $SaveError
@onready var disband_dialog: ConfirmationDialog = $DisbandArmyDialog
@onready var war_dialog: ConfirmationDialog = $DeclareWarDialog
@onready var peace_dialog: Window = $PeaceDialog

var backend: AugustusMap
var _session_topbar: Control


func _paint_match_type(node: Node) -> void:
	if node.name == "MapView":
		return
	if node is Button and node.icon == null:
		_paint_match_button(node)
	elif node is Label:
		var size := Eu4Type.VIC_22 if node.get_theme_font_size("font_size") >= Eu4Type.VIC_22 else Eu4Type.VIC_18
		Eu4Type.paint_label(node, size)
	for child in node.get_children():
		_paint_match_type(child)


func _paint_match_button(button: Button) -> void:
	var directory := ""
	if GameSession.backend != null:
		directory = str(GameSession.backend.get_game_directory())
	var assets = preload("res://ui/eu4/eu4_ui_assets.gd").new()
	var texture: Texture2D = assets.texture_from_file(directory.path_join("gfx/interface/standard_button_88.dds"))
	if texture != null:
		var style := StyleBoxTexture.new()
		style.texture = texture
		for state in ["normal", "hover", "pressed", "disabled", "focus"]:
			button.add_theme_stylebox_override(state, style)
	# standard_button_88 是 88×32，蓝底比金边窄一圈。文字在这块蓝底里居中。
	Eu4Type.paint_button(button, Eu4Type.VIC_18, Eu4Type.STANDARD_BUTTON, Vector4(8, 7, 8, 7))
	button.add_theme_color_override("font_color", Color(0.96, 0.91, 0.78))
	button.add_theme_color_override("font_hover_color", Color(1, 0.95, 0.8))
	button.add_theme_color_override("font_disabled_color", Color(0.55, 0.52, 0.46))


func _ready() -> void:
	# 从编辑器单独运行游戏场景时，回到启动器或选国家，不再打开旧载入页。
	if not GameSession.is_started():
		var target := COUNTRY_SELECT if GameSession.has_game() else LAUNCHER
		get_tree().change_scene_to_file.call_deferred(target)
		return
	AppSettings.apply_game_window()
	backend = GameSession.backend
	map_view.selection_changed.connect(_refresh_selection)
	map_view.movement_requested.connect(_move_army)
	military_overview_button.toggled.connect(_show_military_overview)
	military_overview.army_selected.connect(map_view.focus_army)
	military_overview.recruitment_selected.connect(_locate_recruitment)
	info_panel.construction_requested.connect(_start_construction)
	info_panel.recruitment_requested.connect(_start_recruitment)
	info_panel.recruitment_cancel_requested.connect(_cancel_recruitment)
	info_panel.army_selected.connect(map_view.select_army)
	info_panel.army_stop_requested.connect(_stop_army)
	info_panel.armies_merge_requested.connect(_merge_armies)
	info_panel.army_split_requested.connect(_split_army)
	info_panel.army_disband_requested.connect(_open_disband_dialog)
	info_panel.army_retreat_requested.connect(_retreat_army)
	info_panel.war_declare_requested.connect(_open_war_dialog)
	info_panel.peace_requested.connect(_open_peace_dialog)
	disband_dialog.army_disband_requested.connect(_disband_army)
	war_dialog.confirmed_war.connect(_declare_war)
	peace_dialog.confirmed_peace.connect(_make_peace)
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
	_session_topbar = SESSION_TOPBAR.new()
	add_child(_session_topbar)
	_session_topbar.setup(str(backend.get_game_directory()))
	resized.connect(_layout_session_topbar)
	_layout_session_topbar()
	var modes := ProvinceMapView.add_surface_modes($TopBar/Top/Header, str(backend.get_game_directory()), map_view)
	_paint_match_type(self)
	modes.alignment = BoxContainer.ALIGNMENT_CENTER
	_refresh_projection()
	var player_info := backend.get_country_info(GameSession.get_player_country())
	player_country_label.text = "玩家国家：" + str(player_info.name)
	player_country_label.tooltip_text = player_country_label.text


func _layout_session_topbar() -> void:
	if _session_topbar == null or size.x < 1.0:
		return
	var factor := size.x / 1920.0
	_session_topbar.scale = Vector2(factor, factor)
	_session_topbar.position = Vector2.ZERO
	var bar_height := 160.0 * factor
	var panel_height := 232.0
	$TopBar.offset_top = bar_height + 8.0
	$TopBar.offset_bottom = bar_height + 8.0 + panel_height


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


func _start_construction(province_id: int, building: String) -> void:
	var error := GameSession.start_construction(province_id, building)
	if not error.is_empty():
		_refresh_selection(false)
		info_panel.show_construction_error(error)


func _refresh_selection(select_tab: bool = true) -> void:
	if select_tab:
		_show_military_overview(false)
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
	info_panel.show_info(province_info, country_info, backend.get_construction_info(province_id), backend.get_recruitment_info(province_id), map_view.selected_army, backend.get_army_organization_info(map_view.selected_army))
	# 日期推进会刷新财政；保留玩家手动选中的信息页签。
	if select_tab:
		info_panel.tabs.current_tab = 3 if not army.is_empty() else (1 if map_view.mode == MapMode.Type.COUNTRY else 0)
	if not army.is_empty():
		var location := "驻扎 " + str(army.province_name)
		if bool(army.get("in_battle", false)):
			location = "交战中 · " + str(army.province_name)
		elif bool(army.get("retreating", false)):
			location = "撤退至 %s · 剩余 %d 天" % [army.movement.destination_name, army.movement.days_remaining]
		elif bool(army.get("exiled", false)) and army.movement != null:
			location = "流放返回 %s · 剩余 %d 天" % [army.movement.destination_name, army.movement.days_remaining]
		elif army.movement != null:
			location = "行军至 %s · 剩余 %d 天" % [army.movement.destination_name, army.movement.days_remaining]
		var relation := ""
		match str(army.get("stance", "neutral")):
			"own":
				relation = "我方 · "
			"enemy":
				relation = "敌方 · "
		selection_label.text = "%s%s · %d 人 · %s" % [relation, army.name, army.strength, location]
	elif map_view.mode == MapMode.Type.COUNTRY:
		selection_label.text = "未选择国家" if country_info.is_empty() else "国家：" + str(country_info.name)
	elif province_id == 0:
		selection_label.text = "未选择省份"
	else:
		var owner := backend.get_province_owner(province_id)
		var owner_name := str(province_info.get("owner_name", ""))
		var ownership := "所属国家：" + owner_name if not owner_name.is_empty() else "所属国家"
		if owner.is_empty():
			match backend.get_province_kind(province_id):
				"sea": ownership = "海洋"
				"lake": ownership = "湖泊"
				"wasteland": ownership = "荒地"
				_: ownership = "未殖民土地"
		selection_label.text = "省份 #%d · %s" % [province_id, ownership]
	# 窄窗口下文字可能被省略，悬停仍可查看完整信息。
	selection_label.tooltip_text = selection_label.text


func _start_recruitment(province_id: int, kind: String) -> void:
	var error := GameSession.start_recruitment(province_id, kind)
	if not error.is_empty():
		_refresh_selection(false)
		info_panel.show_recruitment_error(error)


func _cancel_recruitment(province_id: int, order_id: int) -> void:
	var error := GameSession.cancel_recruitment(province_id, order_id)
	if not error.is_empty():
		_refresh_selection(false)
		info_panel.show_recruitment_error(error)


func _move_army(army_id: int, destination: int) -> void:
	var error := GameSession.move_army(army_id, destination)
	if not error.is_empty():
		_refresh_selection(false)
		info_panel.show_movement_error(error)


func _stop_army(army_id: int) -> void:
	var error := GameSession.stop_army(army_id)
	if not error.is_empty():
		_refresh_selection(false)
		info_panel.show_movement_error(error)


func _retreat_army(army_id: int) -> void:
	var error := GameSession.retreat_army(army_id)
	_refresh_selection(false)
	if not error.is_empty():
		info_panel.show_retreat_error(error)


func _merge_armies(target: int, sources: PackedInt64Array) -> void:
	var error := GameSession.merge_armies(target, sources)
	if not error.is_empty():
		_refresh_selection(false)
		info_panel.show_organization_error(error)


func _split_army(army_id: int, count: int) -> void:
	var result := GameSession.split_army(army_id, count)
	if not result.error.is_empty():
		_refresh_selection(false)
		info_panel.show_organization_error(result.error)
		return
	map_view.select_army(result.army_id)


func _open_war_dialog(tag: String) -> void:
	var error: String = war_dialog.open_for_country(backend, tag)
	if not error.is_empty():
		info_panel.show_war_error(error)


func _declare_war(tag: String) -> void:
	var error := GameSession.declare_war(tag)
	_refresh_selection(false)
	info_panel.tabs.current_tab = 1
	if not error.is_empty():
		info_panel.show_war_error(error)


func _open_peace_dialog(tag: String) -> void:
	var error: String = peace_dialog.open_for_country(backend, tag)
	if not error.is_empty():
		info_panel.show_war_error(error)


func _make_peace(tag: String, provinces: PackedInt64Array) -> void:
	var error := GameSession.make_peace(tag, provinces)
	_refresh_selection(false)
	info_panel.tabs.current_tab = 1
	if not error.is_empty():
		info_panel.show_war_error(error)


func _open_disband_dialog(army_id: int) -> void:
	var error: String = disband_dialog.open_for_army(backend, army_id)
	if not error.is_empty():
		info_panel.show_disband_error(error)


func _disband_army(army_id: int) -> void:
	var army := backend.get_army_info(army_id)
	var error := GameSession.disband_army(army_id)
	if not error.is_empty():
		_refresh_selection(false)
		info_panel.show_disband_error(error)
		return
	map_view.select_province(army.province)
	info_panel.tabs.current_tab = 3


func _show_military_overview(show_overview: bool) -> void:
	military_overview_button.set_pressed_no_signal(show_overview)
	info_panel.visible = not show_overview
	military_overview.visible = show_overview


func _locate_recruitment(province_id: int) -> void:
	map_view.focus_province(province_id)
	info_panel.tabs.current_tab = 3
