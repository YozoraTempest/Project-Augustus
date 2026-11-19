# SPDX-License-Identifier: GPL-3.0-only
extends Control

const MAIN_MENU := "res://scenes/main_menu/main_menu.tscn"
const LAUNCHER := "res://scenes/launcher/launcher.tscn"
const COUNTRY_SELECT := "res://scenes/country_select/country_select.tscn"
const SESSION_TOPBAR := preload("res://scenes/game/session_topbar.gd")

@onready var map_view: ProvinceMapView = %MapView

var backend: AugustusMap
var _session_topbar: Control


func _ready() -> void:
	# 从编辑器单独运行游戏场景时，回到启动器或选国家，不再打开旧载入页。
	if not GameSession.is_started():
		var target := COUNTRY_SELECT if GameSession.has_game() else LAUNCHER
		get_tree().change_scene_to_file.call_deferred(target)
		return
	AppSettings.apply_game_window()
	backend = GameSession.backend
	map_view.selection_changed.connect(_refresh_armies)
	map_view.movement_requested.connect(_move_army)
	GameSession.clock_changed.connect(_refresh_armies)
	GameSession.world_changed.connect(_refresh_armies)
	var toolbar := map_view.get_node_or_null("ModeToolbar")
	if toolbar:
		toolbar.hide()
	# 复用开局页已生成的纹理，进入游戏无需再次读取游戏文件。
	map_view.show_map(backend)
	map_view.set_projection(GameSession.map_projection)
	_session_topbar = SESSION_TOPBAR.new()
	add_child(_session_topbar)
	_session_topbar.setup(str(backend.get_game_directory()))
	resized.connect(_layout_session_topbar)
	_layout_session_topbar()


func _layout_session_topbar() -> void:
	if _session_topbar == null:
		return
	# 顶栏保持 topbar.gui 的原始像素，不按窗口宽度放大。
	_session_topbar.scale = Vector2.ONE
	_session_topbar.position = Vector2.ZERO


func _refresh_armies() -> void:
	map_view.refresh_armies()


func _move_army(army_id: int, destination: int) -> void:
	GameSession.move_army(army_id, destination)
	_refresh_armies()
