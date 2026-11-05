# SPDX-License-Identifier: GPL-3.0-only
extends HBoxContainer

@onready var date_label: Label = %CurrentDate
@onready var pause_button: Button = %PauseTime
@onready var speed_option: OptionButton = %GameSpeed
@onready var state_label: Label = %TimeState


func _ready() -> void:
	for multiplier in [1, 2, 4, 8]:
		speed_option.add_item("%d×" % multiplier, multiplier)
	pause_button.pressed.connect(GameSession.toggle_pause)
	speed_option.item_selected.connect(func(index: int): GameSession.set_game_speed(speed_option.get_item_id(index)))
	GameSession.clock_changed.connect(_refresh)
	_refresh()


func _input(event: InputEvent) -> void:
	# 先处理空格，避免有焦点的普通按钮同时把它当作“确认”执行。
	if not event is InputEventKey or not event.pressed or event.echo:
		return
	if event.keycode != KEY_SPACE or event.alt_pressed or event.ctrl_pressed or event.meta_pressed or event.shift_pressed:
		return
	var focus := get_viewport().gui_get_focus_owner()
	if focus is LineEdit or focus is TextEdit:
		return
	GameSession.toggle_pause()
	get_viewport().set_input_as_handled()


func _refresh() -> void:
	var state := GameSession.get_clock_state()
	var available := not state.is_empty()
	pause_button.disabled = not available
	speed_option.disabled = not available
	if not available:
		date_label.text = "日期：—"
		state_label.text = "尚未开局"
		return
	date_label.text = "日期：" + state.date
	pause_button.text = "继续（空格）" if state.paused else "暂停（空格）"
	state_label.text = "已暂停" if state.paused else "进行中"
	state_label.modulate = Color(1, 0.84, 0.43) if state.paused else Color(0.55, 0.88, 0.66)
	speed_option.select(speed_option.get_item_index(state.speed))
