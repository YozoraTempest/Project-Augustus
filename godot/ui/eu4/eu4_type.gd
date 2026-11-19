# SPDX-License-Identifier: GPL-3.0-only
extends RefCounted

# gfx/fonts/vic_22.fnt 与 vic_18 的 lineHeight。中文用系统黑体，字号跟这两套点阵对齐。
const VIC_22 := 22
const VIC_18 := 18
# gfx/interface/big_button_232.dds，大厅「开始游戏」用这一张。
const BIG_BUTTON := Vector2(232, 52)
# gfx/interface/standard_button_88.dds，对局里的短按钮。
const STANDARD_BUTTON := Vector2(88, 32)

static var _font: Font


static func font() -> Font:
	if _font == null:
		var system := SystemFont.new()
		system.font_names = PackedStringArray([
			"Microsoft YaHei UI",
			"Microsoft YaHei",
			"Noto Sans CJK SC",
			"Source Han Sans SC",
		])
		_font = system
	return _font


static func paint_label(label: Label, size: int) -> void:
	label.add_theme_font_override("font", face_of(label))
	label.add_theme_font_size_override("font_size", size)


static func paint_rich_text(label: RichTextLabel, size: int) -> void:
	label.add_theme_font_override("normal_font", face_of(label))
	label.add_theme_font_size_override("normal_font_size", size)


static func face_of(control: Control) -> Font:
	if control.has_meta("eu4_bitmap_font"):
		var bitmap: Variant = control.get_meta("eu4_bitmap_font")
		if bitmap is Font:
			if bitmap.fallbacks.is_empty():
				bitmap.fallbacks = [font()]
			return bitmap
	return font()


static func paint_button(button: Button, size: int, minimum: Vector2, margins: Vector4 = Vector4(8, 0, 8, 0)) -> void:
	button.add_theme_font_override("font", face_of(button))
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
