# SPDX-License-Identifier: GPL-3.0-only
extends RefCounted

signal action_requested(eu4_name: String, properties: Dictionary)

var assets
var diagnostics: PackedStringArray = PackedStringArray()
# 主菜单的窗口本身没有底图，不能再铺一层占位色块。
var plain_windows := false
# 主菜单、选国家和对局顶栏按 1920×1080 归位。其它界面文件仍走原来的摆法。
var baseline_layout := false
const BASELINE_SIZE := Vector2(1920, 1080)


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
		"edit_box":
			control = _edit_box(data)
		"unknown":
			if baseline_layout and str(data.get("type_key", "")).to_lower() == "checkboxtype":
				control = _button(data)
			else:
				diagnostics.append("已跳过未知控件：%s %s" % [str(data.get("type_key", "")), str(data.get("name", ""))])
				return null
		_:
			diagnostics.append("已跳过控件：" + kind)
			return null
	_apply_meta(control, data)
	_apply_geometry(control, data, parent)
	if data.get("dont_render") == true:
		control.visible = false
	if data.get("always_transparent") == true:
		control.mouse_filter = Control.MOUSE_FILTER_IGNORE
	parent.add_child(control)
	var child_parent := control
	if kind == "list_box":
		child_parent = control.get_node("Items")
		if control.has_meta("eu4_box"):
			child_parent.set_meta("eu4_box", control.get_meta("eu4_box"))
	for child: Variant in data.get("children", []):
		if child is Dictionary:
			_build_node(child, child_parent)
	return control


func _window() -> Control:
	if plain_windows:
		var box := Control.new()
		box.mouse_filter = Control.MOUSE_FILTER_IGNORE
		return box
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
	var full := _full_texture(sprite)
	var texture: Texture2D = _placed_texture(sprite, _frame_number(data)) if baseline_layout else full
	if texture:
		rect.texture = texture
		if baseline_layout:
			rect.stretch_mode = TextureRect.STRETCH_SCALE
			if full != null and texture != full:
				rect.set_meta("eu4_gold_sheet", full)
	rect.set_meta("eu4_sprite", sprite)
	return rect


func _text(data: Dictionary) -> RichTextLabel:
	var label := RichTextLabel.new()
	label.bbcode_enabled = true
	label.scroll_active = false
	label.fit_content = false
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	label.add_theme_font_size_override("normal_font_size", _font_size(str(data.get("font", ""))))
	_attach_bitmap_font(label, str(data.get("font", "")))
	label.add_theme_color_override("default_color", Color(0.95, 0.91, 0.82))
	label.text = _display_text(data, false)
	if baseline_layout:
		# 行顶对齐字盒顶，字形再按基线落下。写了居中才在字盒宽度里居中。
		label.vertical_alignment = VERTICAL_ALIGNMENT_TOP
	_apply_format(label, str(data.get("format", "")))
	return label


func _button(data: Dictionary) -> Button:
	var button := Button.new()
	button.text = _display_text(data, true)
	button.clip_text = true
	if baseline_layout:
		# buttonText 留在按钮底图里。旁边的文本框仍是单独的控件，不挪到按钮上。
		button.alignment = HORIZONTAL_ALIGNMENT_CENTER
	button.tooltip_text = _localise(_string(data.get("tooltip")))
	var button_font := str(data.get("button_font", data.get("font", "")))
	button.add_theme_font_size_override("font_size", _font_size(button_font))
	_attach_bitmap_font(button, button_font)
	var sprite := _sprite_name(data)
	var texture: Texture2D = _placed_texture(sprite, _frame_number(data)) if baseline_layout else _full_texture(sprite)
	if texture:
		var style := StyleBoxTexture.new()
		style.texture = texture
		for state in ["normal", "hover", "pressed", "disabled", "focus"]:
			button.add_theme_stylebox_override(state, style)
		# flat 会跳过样式盒，按钮自己的底图就画不出来。归位的三块要看见这张图。
		button.flat = not baseline_layout
		if not (data.get("size") is Dictionary):
			button.custom_minimum_size = texture.get_size()
	elif baseline_layout:
		button.flat = false
		var empty := StyleBoxEmpty.new()
		for state in ["normal", "hover", "pressed", "disabled", "focus"]:
			button.add_theme_stylebox_override(state, empty)
	var properties := _property_map(data.get("properties", []))
	var eu4_name := _string(data.get("name"))
	button.set_meta("eu4_sprite", sprite)
	button.pressed.connect(func() -> void: action_requested.emit(eu4_name, properties))
	return button


func _edit_box(data: Dictionary) -> LineEdit:
	var edit := LineEdit.new()
	edit.text = _display_text(data, false)
	edit.focus_mode = Control.FOCUS_ALL
	var edit_font := str(data.get("font", ""))
	edit.add_theme_font_size_override("font_size", _font_size(edit_font))
	_attach_bitmap_font(edit, edit_font)
	edit.add_theme_color_override("font_color", Color(0.96, 0.91, 0.78))
	return edit


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
	if not orientation.is_empty() and orientation.to_upper() not in ["UPPER_LEFT", "CENTER", "CENTER_UP", "CENTER_DOWN", "CENTER_LEFT", "CENTER_RIGHT", "LOWER_LEFT", "UPPER_RIGHT", "LOWER_RIGHT"]:
		control.set_meta("eu4_orientation_unsupported", orientation)
		diagnostics.append("%s 的方向 %s 尚未支持" % [_string(data.get("name")), orientation])


func _apply_geometry(control: Control, data: Dictionary, parent: Node) -> void:
	if baseline_layout:
		_apply_baseline_geometry(control, data, parent)
		return
	# 已经显示的窗口用 show_position。没有这个字段的控件仍用 position。
	var position: Variant = data.get("show_position")
	if not position is Dictionary:
		position = data.get("position")
	var size: Variant = data.get("size")
	var x := 0.0
	var y := 0.0
	if position is Dictionary:
		x = _number(position.get("x"))
		y = _number(position.get("y"))
	var width := _number(size.get("x")) if size is Dictionary else 0.0
	var height := _number(size.get("y")) if size is Dictionary else 0.0
	if data.get("fullscreen") == true and parent is Control:
		var screen: Vector2 = parent.get_viewport().get_visible_rect().size
		if screen.x > 1.0 and screen.y > 1.0:
			width = screen.x
			height = screen.y
	if width <= 0.0:
		width = _texture_width(control)
	if height <= 0.0:
		height = _texture_height(control)
	if width <= 0.0:
		width = _scalar(data.get("max_width"))
	if height <= 0.0:
		height = _scalar(data.get("max_height"))
	if width <= 0.0:
		width = _default_width(str(data.get("kind", "")))
	if height <= 0.0:
		height = _default_height(str(data.get("kind", "")))
	var orientation := _string(data.get("orientation")).to_upper()
	if orientation.is_empty():
		orientation = "UPPER_LEFT"
	# 位置相对父控件的锚点。CENTER_DOWN 把底栏钉在屏幕底边中点。
	match orientation:
		"UPPER_RIGHT":
			control.anchor_left = 1.0
			control.anchor_right = 1.0
			control.anchor_top = 0.0
			control.anchor_bottom = 0.0
			x -= width
		"LOWER_LEFT":
			control.anchor_left = 0.0
			control.anchor_right = 0.0
			control.anchor_top = 1.0
			control.anchor_bottom = 1.0
			y -= height
		"LOWER_RIGHT":
			control.anchor_left = 1.0
			control.anchor_right = 1.0
			control.anchor_top = 1.0
			control.anchor_bottom = 1.0
			x -= width
			y -= height
		"CENTER":
			control.anchor_left = 0.5
			control.anchor_right = 0.5
			control.anchor_top = 0.5
			control.anchor_bottom = 0.5
			x -= width * 0.5
			y -= height * 0.5
		"CENTER_UP":
			control.anchor_left = 0.5
			control.anchor_right = 0.5
			control.anchor_top = 0.0
			control.anchor_bottom = 0.0
		"CENTER_DOWN":
			control.anchor_left = 0.5
			control.anchor_right = 0.5
			control.anchor_top = 1.0
			control.anchor_bottom = 1.0
		"CENTER_LEFT":
			control.anchor_left = 0.0
			control.anchor_right = 0.0
			control.anchor_top = 0.5
			control.anchor_bottom = 0.5
			y -= height * 0.5
		"CENTER_RIGHT":
			control.anchor_left = 1.0
			control.anchor_right = 1.0
			control.anchor_top = 0.5
			control.anchor_bottom = 0.5
			x -= width
			y -= height * 0.5
		_:
			control.set_anchors_preset(Control.PRESET_TOP_LEFT)
	control.offset_left = x
	control.offset_top = y
	control.offset_right = x + width
	control.offset_bottom = y + height


# 1920×1080 上：显示偏移加父参考点，不按控件自己的宽高回退，也不跟窗口缩放。
func _apply_baseline_geometry(control: Control, data: Dictionary, parent: Node) -> void:
	var position: Variant = data.get("show_position")
	if not position is Dictionary:
		position = data.get("position")
	var x := _axis(position, "x")
	var y := _axis(position, "y")
	var declared: Variant = data.get("size")
	var declared_w := _axis(declared, "x")
	var declared_h := _axis(declared, "y")
	var parent_box := Vector2.ZERO
	if parent != null and parent.has_meta("eu4_box"):
		parent_box = parent.get_meta("eu4_box")
	var fullscreen: bool = data.get("fullscreen") == true
	var box := BASELINE_SIZE if fullscreen else Vector2(declared_w, declared_h)
	var visual := box
	if not fullscreen and not (declared is Dictionary):
		visual = _frame_size(control)
		if visual.x <= 0.0:
			visual.x = _scalar(data.get("max_width"))
		if visual.y <= 0.0:
			visual.y = _scalar(data.get("max_height"))
	var picture_scale := _property_number(data, "scale")
	if str(data.get("kind", "")) == "icon" and picture_scale > 0.0 and not is_equal_approx(picture_scale, 1.0):
		visual *= picture_scale
	var orientation := _string(data.get("orientation")).to_upper()
	if orientation.is_empty():
		orientation = "UPPER_LEFT"
	var reference := _baseline_reference(orientation, parent_box)
	var left := reference.x + x
	var top := reference.y + y
	control.set_anchors_preset(Control.PRESET_TOP_LEFT)
	control.offset_left = left
	control.offset_top = top
	control.offset_right = left + visual.x
	control.offset_bottom = top + visual.y
	control.clip_contents = false
	control.set_meta("eu4_box", box)


func _baseline_reference(orientation: String, box: Vector2) -> Vector2:
	match orientation:
		"UPPER_RIGHT":
			return Vector2(box.x, 0.0)
		"LOWER_LEFT":
			return Vector2(0.0, box.y)
		"LOWER_RIGHT":
			return Vector2(box.x, box.y)
		"CENTER":
			return box * 0.5
		"CENTER_UP":
			return Vector2(box.x * 0.5, 0.0)
		"CENTER_DOWN":
			return Vector2(box.x * 0.5, box.y)
		"CENTER_LEFT":
			return Vector2(0.0, box.y * 0.5)
		"CENTER_RIGHT":
			return Vector2(box.x, box.y * 0.5)
		_:
			return Vector2.ZERO


func _axis(value: Variant, axis: String) -> float:
	return _number(value.get(axis)) if value is Dictionary else 0.0


func _frame_number(data: Dictionary) -> int:
	var frame := int(_property_number(data, "frame"))
	return frame if frame >= 1 else 1


func _property_number(data: Dictionary, key: String) -> float:
	var properties: Variant = data.get("properties", [])
	if not properties is Array:
		return 0.0
	for field: Variant in properties:
		if field is Dictionary and str(field.get("key", "")).to_lower() == key:
			var value: Variant = field.get("value", null)
			if value is Dictionary:
				return _number(value.get("value"))
	return 0.0


func _full_texture(sprite: String) -> Texture2D:
	if sprite.is_empty() or assets == null or not assets.has_method("texture_for"):
		return null
	return assets.texture_for(sprite)


func _placed_texture(sprite: String, frame_number: int) -> Texture2D:
	var full := _full_texture(sprite)
	if full == null:
		return null
	var frames := _sprite_frames(sprite)
	if frames <= 1:
		return full
	var index := clampi(frame_number, 1, frames) - 1
	var frame_width := full.get_width() / frames
	var region := AtlasTexture.new()
	region.atlas = full
	region.region = Rect2(index * frame_width, 0, frame_width, full.get_height())
	return region


func _sprite_frames(sprite: String) -> int:
	if assets == null or assets.backend == null or not assets.backend.has_method("resolve_eu4_sprite"):
		return 1
	var info: Variant = assets.backend.resolve_eu4_sprite(sprite)
	if not info is Dictionary:
		return 1
	# 没写 noOfFrames 时绑定给出的是空值，键仍在，不能当成缺省 1 以外的类型去转。
	var raw: Variant = (info as Dictionary).get("frames")
	if raw is int or raw is float:
		return maxi(int(raw), 1)
	return 1


func _frame_size(control: Control) -> Vector2:
	if control is TextureRect and control.texture:
		return control.texture.get_size()
	if control is Button:
		var style := control.get_theme_stylebox("normal")
		if style is StyleBoxTexture and style.texture:
			return style.texture.get_size()
	return Vector2.ZERO


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
	var from_file := _bitmap_line_height(font_name)
	if from_file > 0:
		return from_file
	var digits := ""
	for character: String in font_name:
		if character.is_valid_int():
			digits += character
		elif not digits.is_empty():
			break
	return clampi(int(digits) if not digits.is_empty() else 17, 12, 28)


func _attach_bitmap_font(control: Control, font_name: String) -> void:
	if font_name.is_empty() or assets == null or assets.backend == null:
		return
	if not assets.backend.has_method("resolve_eu4_sprite"):
		return
	var info: Dictionary = assets.backend.resolve_eu4_sprite(font_name)
	var fnt_path := str(info.get("absolute_path", ""))
	var tga_path := str(info.get("absolute_path_1", ""))
	if not fnt_path.to_lower().ends_with(".fnt") or not tga_path.to_lower().ends_with(".tga"):
		return
	var bitmap: Font = preload("res://ui/eu4/eu4_bitmap_font.gd").atlas_font(fnt_path, tga_path)
	if bitmap == null:
		return
	control.set_meta("eu4_bitmap_font", bitmap)
	# 文案已经是英文 yml。这里只套 core.gfx 的点阵字，不另挂系统字体。
	if control is RichTextLabel:
		control.add_theme_font_override("normal_font", bitmap)
	elif control is Button or control is LineEdit:
		control.add_theme_font_override("font", bitmap)


func _bitmap_line_height(font_name: String) -> int:
	if font_name.is_empty() or assets == null or assets.backend == null:
		return 0
	if not assets.backend.has_method("resolve_eu4_sprite"):
		return 0
	var info: Dictionary = assets.backend.resolve_eu4_sprite(font_name)
	var path := str(info.get("absolute_path", ""))
	if path.is_empty() or not path.to_lower().ends_with(".fnt"):
		return 0
	if not FileAccess.file_exists(path):
		return 0
	for raw_line in FileAccess.get_file_as_string(path).split("\n"):
		var line := raw_line.strip_edges()
		if not line.begins_with("common "):
			continue
		var token := "lineHeight="
		var start := line.find(token)
		if start < 0:
			return 0
		start += token.length()
		var end := start
		while end < line.length() and line[end] != " ":
			end += 1
		return int(line.substr(start, end - start))
	return 0


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


func _texture_width(control: Control) -> float:
	if control is TextureRect and control.texture:
		return control.texture.get_width()
	if control is Button:
		var style := control.get_theme_stylebox("normal")
		if style is StyleBoxTexture and style.texture:
			return style.texture.get_width()
	return 0.0


func _texture_height(control: Control) -> float:
	if control is TextureRect and control.texture:
		return control.texture.get_height()
	if control is Button:
		var style := control.get_theme_stylebox("normal")
		if style is StyleBoxTexture and style.texture:
			return style.texture.get_height()
	return 0.0


func _scalar(value: Variant) -> float:
	if value is Dictionary:
		return _number(value.get("value", value.get("x")))
	return _number(value)


func _number(value: Variant) -> float:
	if value == null:
		return 0.0
	if value is String and not str(value).is_valid_float() and not str(value).is_valid_int():
		return 0.0
	return float(value)


func _string(value: Variant) -> String:
	return "" if value == null else str(value)
