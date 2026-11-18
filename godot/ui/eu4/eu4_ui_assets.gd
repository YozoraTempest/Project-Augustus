# SPDX-License-Identifier: GPL-3.0-only
extends RefCounted

var backend: AugustusMap
var failures: PackedStringArray = PackedStringArray()
var _cache: Dictionary = {}


func texture_from_file(path: String) -> Texture2D:
	if path.is_empty() or not FileAccess.file_exists(path):
		return null
	var image: Image = _uncompressed_dds(path) if path.to_lower().ends_with(".dds") else Image.load_from_file(path)
	if image == null or image.is_empty():
		failures.append("无法读取纹理：" + path)
		return null
	return ImageTexture.create_from_image(image)


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
		texture = texture_from_file(path)
	_cache[sprite_name] = texture
	return texture


# Godot 读不了 DXT。未压缩的 24 位和 32 位 DDS 在这里解开。
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
	if width == 0 or height == 0 or (format_flags & 0x40) == 0:
		return null
	if bits == 32:
		return _dds_pixels(bytes, int(width), int(height), 4)
	if bits == 24:
		return _dds_pixels(bytes, int(width), int(height), 3)
	return null


func _dds_pixels(bytes: PackedByteArray, width: int, height: int, source_stride: int) -> Image:
	var row_stride := width * source_stride
	var needed := 128 + row_stride * height
	if bytes.size() < needed:
		return null
	var pixels := PackedByteArray()
	pixels.resize(width * height * 4)
	for y: int in height:
		var source := 128 + y * row_stride
		var target := y * width * 4
		for x: int in width:
			var input := source + x * source_stride
			var output := target + x * 4
			pixels[output] = bytes[input + 2]
			pixels[output + 1] = bytes[input + 1]
			pixels[output + 2] = bytes[input]
			pixels[output + 3] = 255 if source_stride == 3 else bytes[input + 3]
	return Image.create_from_data(width, height, false, Image.FORMAT_RGBA8, pixels)
