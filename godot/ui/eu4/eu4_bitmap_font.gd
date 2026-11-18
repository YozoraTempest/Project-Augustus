# SPDX-License-Identifier: GPL-3.0-only
extends RefCounted

# EU4 的 vic_22 是 Garamond 点阵字，只有拉丁字符。缺字时调用方继续用界面主题字体。
var _glyphs: Dictionary = {}
var _line_height := 22
var _page: Image


func load_vic22(directory: String) -> bool:
	var root := directory.strip_edges()
	var fnt_path := root.path_join("gfx/fonts/vic_22.fnt")
	var page_path := root.path_join("gfx/fonts/vic_22.tga")
	if not FileAccess.file_exists(fnt_path) or not FileAccess.file_exists(page_path):
		return false
	_page = Image.load_from_file(page_path)
	if _page == null or _page.is_empty():
		return false
	if _page.get_format() != Image.FORMAT_RGBA8:
		_page.convert(Image.FORMAT_RGBA8)
	_glyphs.clear()
	for raw_line in FileAccess.get_file_as_string(fnt_path).split("\n"):
		var line := raw_line.strip_edges()
		if line.begins_with("common "):
			_line_height = maxi(_field(line, "lineHeight"), 1)
		elif line.begins_with("char "):
			var id := _field(line, "id")
			_glyphs[id] = {
				"x": _field(line, "x"),
				"y": _field(line, "y"),
				"width": _field(line, "width"),
				"height": _field(line, "height"),
				"xoffset": _field(line, "xoffset"),
				"yoffset": _field(line, "yoffset"),
				"xadvance": _field(line, "xadvance"),
			}
	return not _glyphs.is_empty()


func covers(text: String) -> bool:
	if _page == null or text.is_empty():
		return false
	for character in text:
		if not _glyphs.has(character.unicode_at(0)):
			return false
	return true


func render(text: String) -> Texture2D:
	if not covers(text):
		return null
	var width := 0
	for character in text:
		width += int(_glyphs[character.unicode_at(0)].xadvance)
	width = maxi(width, 1)
	var image := Image.create(width, _line_height, false, Image.FORMAT_RGBA8)
	image.fill(Color(0, 0, 0, 0))
	var pen := 0
	for character in text:
		var glyph: Dictionary = _glyphs[character.unicode_at(0)]
		var glyph_width := int(glyph.width)
		var glyph_height := int(glyph.height)
		if glyph_width > 0 and glyph_height > 0:
			var source := Rect2i(int(glyph.x), int(glyph.y), glyph_width, glyph_height)
			var dest := Vector2i(pen + int(glyph.xoffset), int(glyph.yoffset))
			if dest.y < 0:
				dest.y = 0
			image.blit_rect(_page, source, dest)
		pen += int(glyph.xadvance)
	return ImageTexture.create_from_image(image)


func _field(line: String, key: String) -> int:
	var token := key + "="
	var start := line.find(token)
	if start < 0:
		return 0
	start += token.length()
	var end := start
	while end < line.length() and line[end] != " ":
		end += 1
	return line.substr(start, end - start).to_int()
