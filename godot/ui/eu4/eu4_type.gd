# SPDX-License-Identifier: GPL-3.0-only
extends RefCounted

# 字号来自 core.gfx 点阵字的 lineHeight。界面不加载系统中文，也不把黑体补进缺字。
const VIC_22 := 22
const VIC_18 := 18
const BIG_BUTTON := Vector2(232, 52)
const STANDARD_BUTTON := Vector2(88, 32)


static func paint_label(label: Label, size: int) -> void:
	var face := face_of(label)
	if face == null:
		return
	label.add_theme_font_override("font", face)
	label.add_theme_font_size_override("font_size", size)


static func paint_rich_text(label: RichTextLabel, size: int) -> void:
	var face := face_of(label)
	if face == null:
		return
	label.add_theme_font_override("normal_font", face)
	label.add_theme_font_size_override("normal_font_size", size)


static func face_of(control: Control) -> Font:
	if control != null and control.has_meta("eu4_bitmap_font"):
		var bitmap: Variant = control.get_meta("eu4_bitmap_font")
		if bitmap is Font:
			return bitmap
	return null


static func paint_button(button: Button, size: int, minimum: Vector2, margins: Vector4 = Vector4(8, 0, 8, 0)) -> void:
	var face := face_of(button)
	if face != null:
		button.add_theme_font_override("font", face)
	button.add_theme_font_size_override("font_size", size)
	button.custom_minimum_size = minimum
	button.alignment = HORIZONTAL_ALIGNMENT_CENTER
	for state in ["normal", "hover", "pressed", "disabled", "focus"]:
		var style := button.get_theme_stylebox(state)
		if style == null:
			continue
		var copy := style.duplicate()
		copy.content_margin_left = margins.x
		copy.content_margin_top = margins.y
		copy.content_margin_right = margins.z
		copy.content_margin_bottom = margins.w
		button.add_theme_stylebox_override(state, copy)
