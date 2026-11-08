# SPDX-License-Identifier: GPL-3.0-only
extends Window

signal confirmed_peace(tag: String, provinces: PackedInt64Array)

var _tag := ""
var _opened := false
var _was_paused := false
var _backend: AugustusMap
var _checks: Array[CheckBox] = []
var _score: Label
var _battle: Label
var _occupation: Label
var _options: VBoxContainer
var _reason: Label
var _white: Button
var _cede: Button


func _ready() -> void:
	exclusive = true
	transient = true
	size = Vector2i(520, 640)
	close_requested.connect(hide)
	visibility_changed.connect(func():
		if not visible:
			_restore_processing())
	var margin := MarginContainer.new()
	margin.set_anchors_preset(Control.PRESET_FULL_RECT)
	margin.add_theme_constant_override("margin_left", 12)
	margin.add_theme_constant_override("margin_top", 12)
	margin.add_theme_constant_override("margin_right", 12)
	margin.add_theme_constant_override("margin_bottom", 12)
	add_child(margin)
	var root := VBoxContainer.new()
	root.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	root.size_flags_vertical = Control.SIZE_EXPAND_FILL
	margin.add_child(root)
	_score = _line(root)
	_battle = _line(root)
	_occupation = _line(root)
	var scroll := ScrollContainer.new()
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	root.add_child(scroll)
	_options = VBoxContainer.new()
	_options.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.add_child(_options)
	_reason = _line(root)
	var actions := HBoxContainer.new()
	root.add_child(actions)
	_white = Button.new()
	_white.text = "白和平"
	_cede = Button.new()
	_cede.text = "割让所选省份"
	var cancel := Button.new()
	cancel.text = "取消"
	actions.add_child(_white)
	actions.add_child(_cede)
	actions.add_child(cancel)
	_white.pressed.connect(_confirm_white)
	_cede.pressed.connect(_confirm_cede)
	cancel.pressed.connect(hide)
	hide()


func open_for_country(backend: AugustusMap, tag: String) -> String:
	var info := backend.get_peace_info(tag)
	if not info.get("ok", false):
		return str(info.get("reason", "双方没有进行中的战争"))
	_backend = backend
	_tag = tag
	title = "与 %s 和谈" % info.target_name
	_score.text = str(info.score_text)
	_battle.text = str(info.battle_text)
	_occupation.text = str(info.occupation_text)
	for child in _options.get_children():
		child.queue_free()
	_checks.clear()
	for province: Dictionary in info.provinces:
		var box := CheckBox.new()
		box.text = "%s（#%d）\n%s" % [province.name, province.id, province.cost_text]
		box.set_meta("province_id", int(province.id))
		box.toggled.connect(func(_on: bool): _refresh_quote())
		_options.add_child(box)
		_checks.append(box)
	_reason.text = str(info.white_reason)
	_white.disabled = false
	_cede.disabled = true
	_was_paused = get_tree().paused
	_opened = true
	get_tree().paused = true
	popup_centered()
	return ""


func _line(parent: Node) -> Label:
	var label := Label.new()
	label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	parent.add_child(label)
	return label


func _selected() -> PackedInt64Array:
	var ids := PackedInt64Array()
	for box in _checks:
		if is_instance_valid(box) and box.button_pressed:
			ids.append(int(box.get_meta("province_id")))
	return ids


func _refresh_quote() -> void:
	if _backend == null or _tag.is_empty():
		return
	var selected := _selected()
	if selected.is_empty():
		var info := _backend.get_peace_info(_tag)
		_reason.text = str(info.get("white_reason", ""))
		_cede.disabled = true
		return
	var quote := _backend.quote_peace(_tag, selected)
	_reason.text = str(quote.get("reason", ""))
	_cede.disabled = not bool(quote.get("ok", false)) or not bool(quote.get("accepted", false))


func _confirm_white() -> void:
	confirmed_peace.emit(_tag, PackedInt64Array())
	hide()


func _confirm_cede() -> void:
	var selected := _selected()
	if selected.is_empty():
		return
	confirmed_peace.emit(_tag, selected)
	hide()


func _restore_processing() -> void:
	if _opened:
		get_tree().paused = _was_paused
		_opened = false
		_tag = ""


func _exit_tree() -> void:
	_restore_processing()
