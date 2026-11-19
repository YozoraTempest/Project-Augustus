# SPDX-License-Identifier: GPL-3.0-only
extends RefCounted

# EU4 的点阵字只有拉丁字符。path 同时对应 .fnt、.tga 和图集，.fnt 里没有页文件名。
static var _fonts: Dictionary = {}
var _glyphs: Dictionary = {}
var _line_height := 22
var _page: Image


static func atlas_font(fnt_path: String, tga_path: String) -> Font:
	if fnt_path.is_empty() or tga_path.is_empty():
		return null
	if _fonts.has(fnt_path):
		return _fonts[fnt_path]
	if not FileAccess.file_exists(fnt_path) or not FileAccess.file_exists(tga_path):
		return null
	var page := Image.load_from_file(tga_path)
	if page == null or page.is_empty():
		return null
	return font_from_page(fnt_path, page)


# 页图已经解开时用这条。年份 vic_22s 的页是 dds，不是 tga。
static func font_from_page(fnt_path: String, page: Image) -> Font:
	if fnt_path.is_empty() or page == null or page.is_empty():
		return null
	if _fonts.has(fnt_path):
		return _fonts[fnt_path]
	if page.get_format() != Image.FORMAT_RGBA8:
		page.convert(Image.FORMAT_RGBA8)
	var face_size := 0
	var line_height := 0
	var base_line := 0
	var glyphs: Array[Dictionary] = []
	for raw_line in FileAccess.get_file_as_string(fnt_path).split("\n"):
		var line := raw_line.strip_edges()
		if line.begins_with("info "):
			face_size = maxi(_field(line, "size"), face_size)
		elif line.begins_with("common "):
			line_height = maxi(_field(line, "lineHeight"), line_height)
			base_line = _field(line, "base")
		elif line.begins_with("char "):
			glyphs.append({
				"id": _field(line, "id"),
				"x": _field(line, "x"),
				"y": _field(line, "y"),
				"width": _field(line, "width"),
				"height": _field(line, "height"),
				"xoffset": _field(line, "xoffset"),
				"yoffset": _field(line, "yoffset"),
				"xadvance": _field(line, "xadvance"),
			})
	if glyphs.is_empty():
		return null
	var face := face_size if face_size > 0 else line_height
	var cache_size := Vector2i(face, 0)
	var font := FontFile.new()
	font.set_texture_image(0, cache_size, 0, page)
	# .fnt 的 base 是行顶到基线。yoffset、xoffset 从行顶的笔位算，Godot 的偏移从基线算。
	var baseline := base_line if base_line > 0 else line_height
	if line_height > 0:
		font.set_cache_ascent(0, face, float(baseline))
		font.set_cache_descent(0, face, float(maxi(line_height - baseline, 0)))
	for glyph: Dictionary in glyphs:
		var id := int(glyph.id)
		font.set_glyph_texture_idx(0, cache_size, id, 0)
		font.set_glyph_uv_rect(0, cache_size, id, Rect2(glyph.x, glyph.y, glyph.width, glyph.height))
		font.set_glyph_size(0, cache_size, id, Vector2(glyph.width, glyph.height))
		var y_from_top := int(glyph.yoffset)
		font.set_glyph_offset(0, cache_size, id, Vector2(int(glyph.xoffset), y_from_top - baseline))
		font.set_glyph_advance(0, face, id, Vector2(glyph.xadvance, 0))
	font.set_meta("eu4_face", face)
	font.set_meta("eu4_line_height", line_height if line_height > 0 else face)
	_fonts[fnt_path] = font
	return font


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


static func _field(line: String, key: String) -> int:
	var token := key + "="
	var start := line.find(token)
	if start < 0:
		return 0
	start += token.length()
	var end := start
	while end < line.length() and line[end] != " ":
		end += 1
	return line.substr(start, end - start).to_int()
