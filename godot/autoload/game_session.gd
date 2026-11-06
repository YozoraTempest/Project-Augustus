# SPDX-License-Identifier: GPL-3.0-only
extends Node

signal clock_changed
signal world_changed

# 会话由 Autoload 持有，页面切换不会重建 Rust 世界和地图纹理。
var backend: AugustusMap
var game_directory := ""
var startup_country := ""
var map_projection := MapMode.ViewType.FLAT
var _startup_requested := false
var _last_tick_usec := 0


func _ready() -> void:
	var arguments := OS.get_cmdline_user_args()
	var index := arguments.find("--game-dir")
	if index >= 0 and index + 1 < arguments.size():
		game_directory = arguments[index + 1]
		_startup_requested = true
	index = arguments.find("--country")
	if index >= 0 and index + 1 < arguments.size():
		startup_country = arguments[index + 1]


func _process(_delta: float) -> void:
	_advance_clock()


func _notification(what: int) -> void:
	if what == NOTIFICATION_PAUSED or what == NOTIFICATION_UNPAUSED:
		# 设置窗口暂停场景时丢弃现实时间间隔，关闭后不会补算停留时间。
		_last_tick_usec = 0


func _advance_clock() -> void:
	# 使用单调时钟，日期推进不受 Engine.time_scale 或渲染帧率影响。
	var now := Time.get_ticks_usec()
	if _last_tick_usec != 0 and is_started():
		if backend.advance_time(now - _last_tick_usec):
			clock_changed.emit()
	_last_tick_usec = now


func get_clock_state() -> Dictionary:
	return {} if backend == null else backend.get_clock_state()


func toggle_pause() -> void:
	if not is_started():
		return
	# 先按旧状态结算到本次操作时刻，切速、暂停不会多算或少算这一帧。
	_advance_clock()
	backend.set_game_paused(not backend.get_clock_state().paused)
	clock_changed.emit()


func set_game_speed(multiplier: int) -> void:
	if not is_started():
		return
	_advance_clock()
	var error := backend.set_game_speed(multiplier)
	if not error.is_empty():
		push_error(error)
		return
	clock_changed.emit()


func start_construction(province_id: int, building: String) -> String:
	if not is_started():
		return "请先开始游戏"
	_advance_clock()
	var error := backend.start_construction(province_id, building)
	if error.is_empty():
		# 建造即时改变国库与省份状态；日期没有变化时也需要刷新界面。
		world_changed.emit()
	return error


func consume_startup_request() -> bool:
	# 命令行自动进入只执行一次，返回主菜单后不能再次触发。
	var requested := _startup_requested
	_startup_requested = false
	return requested


func start_recruitment(province_id: int, kind: String) -> String:
	if not is_started():
		return "请先开始游戏"
	_advance_clock()
	var error := backend.start_recruitment(province_id, kind)
	if error.is_empty():
		world_changed.emit()
	return error


func cancel_recruitment(province_id: int, order_id: int) -> String:
	if not is_started():
		return "请先开始游戏"
	_advance_clock()
	var error := backend.cancel_recruitment(province_id, order_id)
	if error.is_empty():
		world_changed.emit()
	return error


func has_game() -> bool:
	return backend != null


func move_army(army_id: int, destination: int) -> String:
	if not is_started():
		return "请先开始游戏"
	_advance_clock()
	var error := backend.move_army(army_id, destination)
	if error.is_empty():
		world_changed.emit()
	return error


func stop_army(army_id: int) -> String:
	if not is_started():
		return "请先开始游戏"
	_advance_clock()
	var error := backend.stop_army(army_id)
	if error.is_empty():
		world_changed.emit()
	return error


func merge_armies(target: int, sources: PackedInt64Array) -> String:
	if not is_started():
		return "请先开始游戏"
	_advance_clock()
	var error := backend.merge_armies(target, sources)
	if error.is_empty():
		world_changed.emit()
	return error


func split_army(army_id: int, count: int) -> Dictionary:
	if not is_started():
		return {"army_id": 0, "error": "请先开始游戏"}
	_advance_clock()
	var result := backend.split_army(army_id, count)
	if result.error.is_empty():
		world_changed.emit()
	return result


func declare_war(tag: String) -> String:
	if not is_started():
		return "请先开始游戏"
	_advance_clock()
	var error := backend.declare_war(tag)
	if error.is_empty():
		world_changed.emit()
	return error


func disband_army(army_id: int) -> String:
	if not is_started():
		return "请先开始游戏"
	_advance_clock()
	var error := backend.disband_army(army_id)
	if error.is_empty():
		world_changed.emit()
	return error


func get_player_country() -> String:
	# 玩家身份由原生会话持有，页面只查询，不维护第二份可写状态。
	return "" if backend == null else backend.get_player_country()


func is_started() -> bool:
	return not get_player_country().is_empty()


func start_game(country: String) -> String:
	if backend == null:
		return "请先载入开局"
	var error := backend.start_game(country)
	if error.is_empty():
		_last_tick_usec = 0
		clock_changed.emit()
	return error


func load_game(directory: String) -> String:
	var candidate := AugustusMap.new()
	var error := candidate.load_map(directory)
	if error.is_empty():
		# 只在载入成功后替换会话，失败时保留之前可用的数据。
		backend = candidate
		game_directory = directory
		map_projection = MapMode.ViewType.FLAT
		_last_tick_usec = 0
	return error


func save_game(path: String) -> String:
	if not is_started():
		return "请先开始游戏"
	_advance_clock()
	var error := backend.save_game(ProjectSettings.globalize_path(path))
	# 文件读写耗时不作为下一帧的模拟时间。
	_last_tick_usec = 0
	return error


func load_saved_game(path: String) -> String:
	var candidate := AugustusMap.new()
	var error := candidate.load_saved_game(ProjectSettings.globalize_path(path))
	if error.is_empty():
		backend = candidate
		game_directory = backend.get_game_directory()
		map_projection = MapMode.ViewType.FLAT
		_last_tick_usec = 0
		clock_changed.emit()
		world_changed.emit()
	return error


func clear_game() -> void:
	backend = null
	map_projection = MapMode.ViewType.FLAT
	_last_tick_usec = 0
	clock_changed.emit()
	# 目录保留供下次开局使用，世界状态和纹理随当前会话释放。
