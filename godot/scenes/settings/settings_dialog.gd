# SPDX-License-Identifier: GPL-3.0-only
extends Window

@onready var mode_option: OptionButton = %WindowMode
@onready var vsync_toggle: CheckButton = %VSync
@onready var apply_button: Button = %Apply
@onready var cancel_button: Button = %Cancel
@onready var error_label: Label = %Error

var _previous_paused := false


func _ready() -> void:
	mode_option.add_item("窗口化", Window.MODE_WINDOWED)
	mode_option.add_item("全屏（无边框）", Window.MODE_FULLSCREEN)
	mode_option.select(mode_option.get_item_index(AppSettings.window_mode))
	vsync_toggle.button_pressed = AppSettings.vsync_enabled
	apply_button.pressed.connect(_apply_settings)
	cancel_button.pressed.connect(queue_free)
	close_requested.connect(queue_free)
	# 模态窗口阻止父窗口输入；暂停节点处理，避免地图交互和游戏逻辑继续运行。
	_previous_paused = get_tree().paused
	get_tree().paused = true
	apply_button.grab_focus.call_deferred()


func _exit_tree() -> void:
	get_tree().paused = _previous_paused


func _unhandled_key_input(event: InputEvent) -> void:
	if event.is_action_pressed("ui_cancel"):
		set_input_as_handled()
		queue_free()


func _apply_settings() -> void:
	var mode := mode_option.get_selected_id() as Window.Mode
	var error := AppSettings.save_and_apply(mode, vsync_toggle.button_pressed)
	if error != OK:
		error_label.text = "无法保存设置：" + error_string(error)
		return
	queue_free()
