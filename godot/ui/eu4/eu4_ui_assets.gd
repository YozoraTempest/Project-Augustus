# SPDX-License-Identifier: GPL-3.0-only
extends RefCounted

var backend: AugustusMap
var failures: PackedStringArray = PackedStringArray()
var _cache: Dictionary = {}


func texture_for(sprite_name: String) -> Texture2D:
	if sprite_name.is_empty() or backend == null:
		return null
	if _cache.has(sprite_name):
		return _cache[sprite_name]
	var info: Dictionary = backend.resolve_eu4_sprite(sprite_name)
	var texture: Texture2D = null
	var path := str(info.get("absolute_path", ""))
	if path.is_empty():
		if not sprite_name.is_empty():
			failures.append("没有精灵：" + sprite_name)
	else:
		var image := Image.load_from_file(path)
		if image == null or image.is_empty():
			image = _uncompressed_dds(path)
		if image == null or image.is_empty():
			failures.append("无法读取纹理：" + path)
		else:
			texture = ImageTexture.create_from_image(image)
	_cache[sprite_name] = texture
	return texture


# Godot 读不了 EU4 的未压缩 DDS。这里只解 32 位 RGB，DXT 仍记为失败。
func _uncompressed_dds(path: String) -> Image:
	if not path.to_lower().ends_with(".dds"):
		return null
	var file := FileAccess.open(path, FileAccess.READ)
	if file == null:
		return null
	var bytes := file.get_buffer(file.get_length())
	if bytes.size() < 128 or bytes[0] != 0x44 or bytes[1] != 0x44 or bytes[2] != 0x53 or bytes[3] != 0x20:
		return null
	var height := bytes.decode_u32(12)
	var width := bytes.decode_u32(16)
	var format_flags := bytes.decode_u32(80)
	var bits := bytes.decode_u32(88)
	if width == 0 or height == 0 or bits != 32 or (format_flags & 0x40) == 0:
		return null
	var row_stride := int(width) * 4
	var needed := 128 + row_stride * int(height)
	if bytes.size() < needed:
		return null
	var pixels := PackedByteArray()
	pixels.resize(row_stride * int(height))
	for y: int in int(height):
		var source := 128 + y * row_stride
		var target := y * row_stride
		for x: int in int(width):
			var input := source + x * 4
			var output := target + x * 4
			pixels[output] = bytes[input + 2]
			pixels[output + 1] = bytes[input + 1]
			pixels[output + 2] = bytes[input]
			pixels[output + 3] = bytes[input + 3]
	return Image.create_from_data(int(width), int(height), false, Image.FORMAT_RGBA8, pixels)
