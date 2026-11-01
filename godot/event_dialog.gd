# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Project Augustus contributors

extends ColorRect

signal option_chosen(pending_id: int, option_id: String)

var panel: PanelContainer
var scroll: ScrollContainer
var title_label: Label
var body_label: Label
var options: VBoxContainer
var option_buttons: Array[Button] = []

var _choice: Dictionary = {}
var _pending_count := 0
var _previous_focus: WeakRef

func _ready() -> void:
	name = "EventChoiceOverlay"
	color = Color(0.025, 0.035, 0.045, 0.78)
	mouse_filter = Control.MOUSE_FILTER_STOP
	set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	visible = false
	panel = PanelContainer.new()
	panel.name = "EventChoicePanel"
	var surface := StyleBoxFlat.new()
	surface.bg_color = Color(0.06, 0.085, 0.11)
	surface.border_color = Color(0.23, 0.3, 0.36)
	surface.set_border_width_all(1)
	surface.set_corner_radius_all(6)
	panel.add_theme_stylebox_override("panel", surface)
	add_child(panel)
	var margin := MarginContainer.new()
	for side in ["left", "right", "top", "bottom"]:
		margin.add_theme_constant_override("margin_" + side, 24)
	panel.add_child(margin)
	scroll = ScrollContainer.new()
	scroll.name = "EventContentScroll"
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	scroll.follow_focus = true
	scroll.focus_mode = Control.FOCUS_ALL
	margin.add_child(scroll)
	var content := VBoxContainer.new()
	content.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	content.add_theme_constant_override("separation", 14)
	scroll.add_child(content)
	title_label = Label.new()
	title_label.add_theme_font_size_override("font_size", 23)
	title_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	content.add_child(title_label)
	body_label = Label.new()
	body_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	content.add_child(body_label)
	options = VBoxContainer.new()
	options.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	options.add_theme_constant_override("separation", 8)
	content.add_child(options)
	resized.connect(_resize_panel)
	_resize_panel()

func _resize_panel() -> void:
	panel.size = Vector2(minf(760, maxf(0, size.x - 48)), minf(640, maxf(0, size.y - 48)))
	panel.position = (size - panel.size) * 0.5

func present(choice: Dictionary, pending_count: int) -> void:
	if visible and choice == _choice and pending_count == _pending_count:
		return
	var same_event: bool = visible and _choice.get("id") == choice.id
	var previous_scroll := scroll.scroll_vertical
	var focused_option := ""
	for index in option_buttons.size():
		if option_buttons[index].has_focus():
			focused_option = str(_choice.options[index])
	if not visible:
		var focused := get_viewport().gui_get_focus_owner()
		_previous_focus = weakref(focused) if focused != null else null
	_choice = choice.duplicate(true)
	_pending_count = pending_count
	title_label.text = str(choice.get("title", choice.event))
	var details := ""
	if choice.get("description") != null:
		details = str(choice.description) + "\n"
	details += "%s · 第 %d 天" % [choice.current_name, int(choice.day)]
	if choice.get("picture") != null:
		details += "\n" + str(choice.picture)
	if choice.get("location") != null:
		details += "\n" + str(choice.location)
	if pending_count > 1:
		details += "\n另有 %d 个待处理事件" % (pending_count - 1)
	body_label.text = details
	_clear_options()
	var option_labels: Dictionary = choice.get("option_labels", {})
	var option_locations: Dictionary = choice.get("option_locations", {})
	for option_id in choice.options:
		var option_key := str(option_id)
		var option := Button.new()
		option.text = str(option_labels.get(option_key, option_key))
		option.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		option.custom_minimum_size.y = 50
		option.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		option.pressed.connect(func(): option_chosen.emit(int(choice.id), option_key))
		if option_locations.get(option_key) != null:
			option.tooltip_text = "定位：%s" % option_locations[option_key]
		option_buttons.append(option)
		options.add_child(option)
	if option_buttons.is_empty():
		var unavailable := Label.new()
		unavailable.text = "当前没有可选项"
		unavailable.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		options.add_child(unavailable)
	_link_focus()
	visible = true
	if same_event:
		if not focused_option.is_empty():
			var option_index: int = choice.options.find(focused_option)
			if option_index >= 0:
				option_buttons[option_index].grab_focus()
				get_tree().process_frame.connect(_reveal_focused_option.bind(option_buttons[option_index]), CONNECT_ONE_SHOT)
			else:
				scroll.grab_focus()
		scroll.scroll_vertical = previous_scroll
	else:
		scroll.scroll_vertical = 0
		scroll.grab_focus()

func dismiss() -> void:
	var was_visible := visible
	visible = false
	_choice.clear()
	_pending_count = 0
	_clear_options()
	if was_visible and _previous_focus != null:
		var focused = _previous_focus.get_ref()
		if focused is Control and focused.is_visible_in_tree():
			focused.grab_focus()
	_previous_focus = null

func _reveal_focused_option(option: Button) -> void:
	if is_instance_valid(option) and option.has_focus() and visible:
		scroll.ensure_control_visible(option)

func _clear_options() -> void:
	for child in options.get_children():
		options.remove_child(child)
		child.queue_free()
	option_buttons.clear()

func _link_focus() -> void:
	var controls: Array[Control] = [scroll]
	for option in option_buttons:
		controls.append(option)
	for index in controls.size():
		controls[index].focus_next = controls[index].get_path_to(controls[(index + 1) % controls.size()])
		controls[index].focus_previous = controls[index].get_path_to(controls[(index - 1 + controls.size()) % controls.size()])
		controls[index].focus_neighbor_top = controls[index].focus_previous
		controls[index].focus_neighbor_bottom = controls[index].focus_next
		controls[index].focus_neighbor_left = NodePath(".")
		controls[index].focus_neighbor_right = NodePath(".")
