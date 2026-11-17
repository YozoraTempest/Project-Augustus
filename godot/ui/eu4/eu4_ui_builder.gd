# SPDX-License-Identifier: GPL-3.0-only
extends RefCounted

signal action_requested(eu4_name: String, properties: Dictionary)

var assets
var diagnostics: PackedStringArray = PackedStringArray()


func build_document(document: Dictionary, parent: Control) -> Array:
	var created: Array = []
	for root: Variant in document.get("roots", []):
		if root is Dictionary:
			var node := _build_node(root, parent)
			if node:
				created.append(node)
	return created


func build_named(document: Dictionary, window_name: String, parent: Control) -> Array:
	var matches: Array = []
	for root: Variant in document.get("roots", []):
		if root is Dictionary:
			_collect_named(root, window_name, matches)
	var created: Array = []
	for node: Dictionary in matches:
		var control := _build_node(node, parent)
		if control:
			created.append(control)
	return created


func _collect_named(node: Dictionary, window_name: String, matches: Array) -> void:
	if str(node.get("name", "")) == window_name:
		matches.append(node)
	for child: Variant in node.get("children", []):
		if child is Dictionary:
			_collect_named(child, window_name, matches)


func _build_node(data: Dictionary, parent: Control) -> Control:
	var kind := str(data.get("kind", ""))
	var control: Control = null
	match kind:
		"window":
			control = _window()
		"container_window", "overlapping_elements_box":
			control = Control.new()
		"icon":
			control = _icon(data)
		"instant_text_box", "text_box":
			control = _text(data)
		"button":
			control = _button(data)
		"grid_box":
			var grid := GridContainer.new()
			grid.columns = 1
			control = grid
		"list_box":
			control = _list()
		"unknown":
			diagnostics.append("已跳过未知控件：%s %s" % [str(data.get("type_key", "")), str(data.get("name", ""))])
			return null
		_:
			diagnostics.append("已跳过控件：" + kind)
			return null
	_apply_meta(control, data)
	_apply_geometry(control, data)
	if data.get("dont_render") == true:
		control.visible = false
	if data.get("always_transparent") == true:
		control.mouse_filter = Control.MOUSE_FILTER_IGNORE
	parent.add_child(control)
	var child_parent := control
	if kind == "list_box":
		child_parent = control.get_node("Items")
	for child: Variant in data.get("children", []):
		if child is Dictionary:
			_build_node(child, child_parent)
	return control


func _window() -> Panel:
	var panel := Panel.new()
	var style := StyleBoxFlat.new()
	style.bg_color = Color(0.16, 0.12, 0.09, 0.94)
	style.border_color = Color(0.72, 0.58, 0.32)
	style.set_border_width_all(2)
	style.set_content_margin_all(0)
	panel.add_theme_stylebox_override("panel", style)
	return panel


func _icon(data: Dictionary) -> TextureRect:
	var rect := TextureRect.new()
	rect.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	rect.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	rect.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var sprite := _sprite_name(data)
	if assets != null and assets.has_method("texture_for"):
		var texture: Texture2D = assets.texture_for(sprite)
		if texture:
			rect.texture = texture
	return rect


func _text(data: Dictionary) -> RichTextLabel:
	var label := RichTextLabel.new()
	label.bbcode_enabled = true
	label.scroll_active = false
	label.fit_content = false
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	label.add_theme_font_size_override("normal_font_size", _font_size(str(data.get("font", ""))))
	label.add_theme_color_override("default_color", Color(0.95, 0.91, 0.82))
	label.text = _display_text(data, false)
	_apply_format(label, str(data.get("format", "")))
	return label


func _button(data: Dictionary) -> Button:
	var button := Button.new()
	button.text = _display_text(data, true)
	button.clip_text = true
	button.tooltip_text = _localise(_string(data.get("tooltip")))
	button.add_theme_font_size_override("font_size", _font_size(str(data.get("button_font", data.get("font", "")))))
	var sprite := _sprite_name(data)
	if assets != null and assets.has_method("texture_for") and not sprite.is_empty():
		var texture: Texture2D = assets.texture_for(sprite)
		if texture:
			button.icon = texture
			button.expand_icon = true
	var properties := _property_map(data.get("properties", []))
	var eu4_name := _string(data.get("name"))
	button.pressed.connect(func() -> void: action_requested.emit(eu4_name, properties))
	return button


func _list() -> ScrollContainer:
	var scroll := ScrollContainer.new()
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	var items := VBoxContainer.new()
	items.name = "Items"
	items.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.add_child(items)
	return scroll


func _apply_meta(control: Control, data: Dictionary) -> void:
	control.set_meta("eu4_name", _string(data.get("name")))
	control.set_meta("eu4_kind", str(data.get("kind", "")))
	control.set_meta("eu4_type", str(data.get("type_key", "")))
	var orientation := _string(data.get("orientation"))
	if not orientation.is_empty() and orientation not in ["UPPER_LEFT", "CENTER", "LOWER_LEFT", "UPPER_RIGHT", "LOWER_RIGHT"]:
		control.set_meta("eu4_orientation_unsupported", orientation)
		diagnostics.append("%s 的方向 %s 尚未支持" % [_string(data.get("name")), orientation])


func _apply_geometry(control: Control, data: Dictionary) -> void:
	var position: Variant = data.get("position")
	var size: Variant = data.get("size")
	var x := 0.0
	var y := 0.0
	if position is Dictionary:
		x = _number(position.get("x"))
		y = _number(position.get("y"))
	var width := _number(size.get("x")) if size is Dictionary else 0.0
	var height := _number(size.get("y")) if size is Dictionary else 0.0
	if width <= 0.0:
		width = _default_width(str(data.get("kind", "")))
	if height <= 0.0:
		height = _default_height(str(data.get("kind", "")))
	var orientation := _string(data.get("orientation"))
	if orientation.is_empty():
		orientation = "UPPER_LEFT"
	match orientation:
		"UPPER_RIGHT":
			control.anchor_left = 1.0
			control.anchor_right = 1.0
			control.anchor_top = 0.0
			control.anchor_bottom = 0.0
		"LOWER_LEFT":
			control.anchor_left = 0.0
			control.anchor_right = 0.0
			control.anchor_top = 1.0
			control.anchor_bottom = 1.0
		"LOWER_RIGHT":
			control.anchor_left = 1.0
			control.anchor_right = 1.0
			control.anchor_top = 1.0
			control.anchor_bottom = 1.0
		"CENTER":
			control.anchor_left = 0.5
			control.anchor_right = 0.5
			control.anchor_top = 0.5
			control.anchor_bottom = 0.5
		_:
			control.set_anchors_preset(Control.PRESET_TOP_LEFT)
	control.offset_left = x
	control.offset_top = y
	control.offset_right = x + width
	control.offset_bottom = y + height


func _display_text(data: Dictionary, button: bool) -> String:
	var raw := _string(data.get("button_text")) if button else _string(data.get("text"))
	if raw.is_empty() and button:
		raw = _string(data.get("text"))
	return _localise(raw)


func _localise(key: String) -> String:
	if key.is_empty() or assets == null or assets.backend == null:
		return key
	var resolved := str(assets.backend.localise_eu4(key))
	return key if resolved.is_empty() else resolved


func _sprite_name(data: Dictionary) -> String:
	var sprite := _string(data.get("sprite"))
	if sprite.is_empty():
		sprite = _string(data.get("quad_texture_sprite"))
	return sprite


func _property_map(properties: Variant) -> Dictionary:
	var mapped := {}
	if properties is Array:
		for field: Variant in properties:
			if field is Dictionary:
				mapped[_string(field.get("key"))] = _property_text(field.get("value"))
	return mapped


func _property_text(value: Variant) -> String:
	if value is Dictionary and value.has("value"):
		var inner: Variant = value.value
		if inner is Array or inner is Dictionary:
			return JSON.stringify(inner)
		return str(inner)
	return "" if value == null else str(value)


func _apply_format(label: RichTextLabel, format: String) -> void:
	match format:
		"right":
			label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
		"centre", "center":
			label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
		_:
			label.horizontal_alignment = HORIZONTAL_ALIGNMENT_LEFT


func _font_size(font_name: String) -> int:
	var digits := ""
	for character: String in font_name:
		if character.is_valid_int():
			digits += character
		elif not digits.is_empty():
			break
	return clampi(int(digits) if not digits.is_empty() else 17, 12, 28)


func _default_width(kind: String) -> float:
	match kind:
		"button":
			return 88.0
		"icon":
			return 32.0
		"list_box", "grid_box":
			return 180.0
		_:
			return 140.0


func _default_height(kind: String) -> float:
	match kind:
		"button":
			return 28.0
		"icon":
			return 32.0
		"list_box", "grid_box":
			return 72.0
		"window", "container_window":
			return 64.0
		_:
			return 22.0


func _number(value: Variant) -> float:
	if value == null:
		return 0.0
	if value is String and not str(value).is_valid_float() and not str(value).is_valid_int():
		return 0.0
	return float(value)


func _string(value: Variant) -> String:
	return "" if value == null else str(value)
