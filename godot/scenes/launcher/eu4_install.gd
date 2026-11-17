# SPDX-License-Identifier: GPL-3.0-only
class_name Eu4Install
extends RefCounted

# 扫描只提议 GameSession.load_game 能接受的安装根目录。
# OpeningWorld.load 会读取这里列出的文件；缺任何一项时，载入会在读内容阶段失败。
const EU4_APP_ID := "236850"
const EU4_FOLDER_NAME := "Europa Universalis IV"
const DEFINES_KEYS := [
	"INFANTRY_COST",
	"SUPPLYLIMIT_BASE_MULTIPLIER",
	"STARTING_ARMY_SIZE",
	"STARTING_ARMY_SIZE_AT_WAR",
	"BASE_COMBAT_WIDTH",
]
const REQUIRED_MAP_KEYS := [
	"width",
	"height",
	"max_provinces",
	"definitions",
	"provinces",
	"terrain",
	"rivers",
	"terrain_definition",
	"tree_definition",
]
const OPTIONAL_MAP_FILES := ["adjacencies", "climate"]
const TEXT_DIRECTORIES := [
	"common/country_tags",
	"common/static_modifiers",
	"common/tradegoods",
	"common/units",
	"history/provinces",
]


static func normalize_path(path: String) -> String:
	var cleaned := path.strip_edges().replace("\\", "/")
	while cleaned.ends_with("/"):
		cleaned = cleaned.trim_suffix("/")
	return cleaned


static func is_loadable(directory: String) -> bool:
	var root := normalize_path(directory)
	if root.is_empty() or not DirAccess.dir_exists_absolute(root):
		return false
	if not _map_files_present(root):
		return false
	if not _file_contains(root.path_join("common/defines.lua"), DEFINES_KEYS):
		return false
	if not _file_contains(root.path_join("common/technologies/mil.txt"), ["technology"]):
		return false
	if not _file_contains(root.path_join("common/technology.txt"), ["groups"]):
		return false
	for relative in TEXT_DIRECTORIES:
		if not _has_txt(root.path_join(relative)):
			return false
	return true


# Steam 的 libraryfolders、默认 common 目录，以及其他固定的常见安装位置。
# 已经设置的路径不会在这里被改写；调用方只展示返回的结果。
static func scan() -> PackedStringArray:
	var ordered := PackedStringArray()
	for root in _steam_client_roots():
		ordered.append_array(_candidates_from_steam_root(root))
	ordered.append_array(_usual_steam_common_directories())
	ordered.append_array(_other_known_directories())
	var found := PackedStringArray()
	var seen := {}
	for path in ordered:
		var normalized := normalize_path(path)
		var key := normalized.to_lower()
		if normalized.is_empty() or seen.has(key):
			continue
		seen[key] = true
		if is_loadable(normalized):
			found.append(normalized)
	return found


static func _steam_client_roots() -> PackedStringArray:
	var roots := PackedStringArray()
	if OS.get_name() == "Windows":
		for query in [
			PackedStringArray(["HKCU\\Software\\Valve\\Steam", "SteamPath"]),
			PackedStringArray(["HKLM\\SOFTWARE\\WOW6432Node\\Valve\\Steam", "InstallPath"]),
			PackedStringArray(["HKLM\\SOFTWARE\\Valve\\Steam", "InstallPath"]),
		]:
			var value := _registry_sz(query[0], query[1])
			if not value.is_empty():
				roots.append(value)
	var program_files := _env_dir("ProgramFiles", "C:/Program Files")
	var program_x86 := _env_dir("ProgramFiles(x86)", "C:/Program Files (x86)")
	roots.append(program_x86.path_join("Steam"))
	roots.append(program_files.path_join("Steam"))
	for drive in ["C", "D", "E", "F"]:
		roots.append("%s:/Steam" % drive)
	return roots


static func _candidates_from_steam_root(steam_root: String) -> PackedStringArray:
	var result := PackedStringArray()
	var root := normalize_path(steam_root)
	if root.is_empty() or not DirAccess.dir_exists_absolute(root):
		return result
	var libraries := PackedStringArray([root])
	var vdf_path := root.path_join("steamapps/libraryfolders.vdf")
	if FileAccess.file_exists(vdf_path):
		for library in _library_paths(_read_text(vdf_path)):
			var normalized := normalize_path(library)
			if not normalized.is_empty():
				libraries.append(normalized)
	for library in libraries:
		if not DirAccess.dir_exists_absolute(library):
			continue
		var common := library.path_join("steamapps/common")
		var manifest := library.path_join("steamapps/appmanifest_%s.acf" % EU4_APP_ID)
		if FileAccess.file_exists(manifest):
			var installdir := _vdf_value(_read_text(manifest), "installdir").strip_edges()
			if _is_plain_folder_name(installdir):
				result.append(common.path_join(installdir))
		result.append(common.path_join(EU4_FOLDER_NAME))
	return result


static func _usual_steam_common_directories() -> PackedStringArray:
	var program_files := _env_dir("ProgramFiles", "C:/Program Files")
	var program_x86 := _env_dir("ProgramFiles(x86)", "C:/Program Files (x86)")
	return PackedStringArray([
		program_x86.path_join("Steam/steamapps/common").path_join(EU4_FOLDER_NAME),
		program_files.path_join("Steam/steamapps/common").path_join(EU4_FOLDER_NAME),
	])


static func _other_known_directories() -> PackedStringArray:
	var paths := PackedStringArray()
	var program_files := _env_dir("ProgramFiles", "C:/Program Files")
	var program_x86 := _env_dir("ProgramFiles(x86)", "C:/Program Files (x86)")
	var parents := PackedStringArray([
		program_x86.path_join("GOG Galaxy/Games"),
		program_files.path_join("GOG Galaxy/Games"),
		program_x86.path_join("Paradox Interactive"),
		program_files.path_join("Paradox Interactive"),
		program_files.path_join("Epic Games"),
		program_x86.path_join("Epic Games"),
		program_x86,
		program_files,
		"C:/GOG Games",
		"D:/GOG Games",
	])
	for parent in parents:
		paths.append(parent.path_join(EU4_FOLDER_NAME))
	paths.append(program_files.path_join("Epic Games/EuropaUniversalis4"))
	paths.append(program_x86.path_join("Epic Games/EuropaUniversalis4"))
	for drive in ["C", "D", "E", "F"]:
		paths.append("%s:/SteamLibrary/steamapps/common/%s" % [drive, EU4_FOLDER_NAME])
		paths.append("%s:/Steam/steamapps/common/%s" % [drive, EU4_FOLDER_NAME])
		paths.append("%s:/Games/%s" % [drive, EU4_FOLDER_NAME])
		paths.append("%s:/XboxGames/%s/Content" % [drive, EU4_FOLDER_NAME])
		paths.append("%s:/XboxGames/%s" % [drive, EU4_FOLDER_NAME])
	return paths


static func _registry_sz(key: String, value_name: String) -> String:
	var output: Array = []
	var code := OS.execute("reg", ["query", key, "/v", value_name], output, true)
	if code != 0:
		return ""
	var lines := PackedStringArray()
	for line in output:
		lines.append(str(line))
	for line in "\n".join(lines).split("\n"):
		var parts := line.split("REG_SZ", true, 1)
		if parts.size() == 2 and parts[0].contains(value_name):
			return parts[1].strip_edges()
	return ""


static func _library_paths(text: String) -> PackedStringArray:
	var paths := PackedStringArray()
	for pair in _vdf_pairs(text):
		var key := str(pair[0])
		var value := str(pair[1])
		if key.to_lower() == "path" or (key.is_valid_int() and _looks_like_path(value)):
			paths.append(value)
	return paths


static func _vdf_value(text: String, wanted: String) -> String:
	var name := wanted.to_lower()
	for pair in _vdf_pairs(text):
		if str(pair[0]).to_lower() == name:
			return str(pair[1])
	return ""


static func _vdf_pairs(text: String) -> Array:
	var pairs: Array = []
	var index := 0
	var length := text.length()
	while index < length:
		if text[index] != '"':
			index += 1
			continue
		var key := _read_quoted(text, index)
		index = _skip_space(text, int(key.next))
		if index >= length or text[index] != '"':
			continue
		var value := _read_quoted(text, index)
		index = int(value.next)
		pairs.append([key.text, value.text])
	return pairs


static func _read_quoted(text: String, start: int) -> Dictionary:
	var index := start + 1
	var value := ""
	var length := text.length()
	while index < length:
		var character := text[index]
		if character == "\\":
			if index + 1 < length:
				var escaped := text[index + 1]
				if escaped == "\\" or escaped == '"':
					value += escaped
					index += 2
					continue
			value += character
			index += 1
			continue
		if character == '"':
			return {"text": value, "next": index + 1}
		value += character
		index += 1
	return {"text": value, "next": index}


static func _skip_space(text: String, index: int) -> int:
	while index < text.length() and text[index] in [" ", "\t", "\r", "\n"]:
		index += 1
	return index


static func _looks_like_path(text: String) -> bool:
	var value := text.strip_edges()
	return value.contains(":/") or value.contains(":\\") or value.begins_with("\\\\")


static func _is_plain_folder_name(name: String) -> bool:
	return not name.is_empty() and not name.contains("/") and not name.contains("\\") and not name.contains("..") and not name.contains(":")


static func _read_text(path: String) -> String:
	var file := FileAccess.open(path, FileAccess.READ)
	if file == null:
		return ""
	var bytes := file.get_buffer(file.get_length())
	if bytes.size() >= 2 and bytes[0] == 0xff and bytes[1] == 0xfe:
		return bytes.slice(2).get_string_from_utf16()
	if bytes.size() >= 3 and bytes[0] == 0xef and bytes[1] == 0xbb and bytes[2] == 0xbf:
		return bytes.slice(3).get_string_from_utf8()
	return bytes.get_string_from_utf8()


static func _map_files_present(root: String) -> bool:
	var path := root.path_join("map/default.map")
	if not FileAccess.file_exists(path):
		return false
	var fields := {}
	var seen := {}
	for raw_line in FileAccess.get_file_as_string(path).split("\n"):
		var parsed := _assignment(raw_line)
		if parsed.is_empty():
			continue
		var key := str(parsed.key)
		if seen.has(key) and (key in REQUIRED_MAP_KEYS or key in OPTIONAL_MAP_FILES):
			return false
		seen[key] = true
		fields[key] = str(parsed.value)
	for key in REQUIRED_MAP_KEYS:
		if not fields.has(key):
			return false
	if not _positive_int(str(fields.width), 1) or not _positive_int(str(fields.height), 1):
		return false
	if not _positive_int(str(fields.max_provinces), 2):
		return false
	for key in REQUIRED_MAP_KEYS:
		if key in ["width", "height", "max_provinces"]:
			continue
		if not _map_file_exists(root, str(fields[key])):
			return false
	for key in OPTIONAL_MAP_FILES:
		if fields.has(key) and not _map_file_exists(root, str(fields[key])):
			return false
	return true


static func _assignment(raw_line: String) -> Dictionary:
	var line := raw_line.strip_edges()
	var comment := line.find("#")
	if comment >= 0:
		line = line.substr(0, comment).strip_edges()
	var separator := line.find("=")
	if separator <= 0:
		return {}
	var key := line.substr(0, separator).strip_edges()
	var value := line.substr(separator + 1).strip_edges()
	if key.is_empty() or value.is_empty() or value.begins_with("{"):
		return {}
	if value.begins_with('"'):
		var end := value.rfind('"')
		if end > 0:
			value = value.substr(1, end - 1)
		else:
			value = value.trim_prefix('"')
	if value.is_empty():
		return {}
	return {"key": key, "value": value}


static func _positive_int(value: String, minimum: int) -> bool:
	return value.is_valid_int() and value.to_int() >= minimum


static func _map_file_exists(root: String, relative: String) -> bool:
	var name := relative.replace("\\", "/").strip_edges()
	if name.is_empty() or name.begins_with("/") or name.contains("..") or name.contains(":") or name.is_absolute_path():
		return false
	return FileAccess.file_exists(root.path_join("map").path_join(name))


static func _file_contains(path: String, keys: Array) -> bool:
	if not FileAccess.file_exists(path):
		return false
	var text := FileAccess.get_file_as_string(path)
	if text.is_empty():
		return false
	for key in keys:
		if not text.contains(key):
			return false
	return true


static func _has_txt(directory: String) -> bool:
	var folder := DirAccess.open(directory)
	if folder == null:
		return false
	for file_name in folder.get_files():
		if file_name.get_extension().to_lower() == "txt":
			return true
	return false


static func _env_dir(name: String, fallback: String) -> String:
	var value := OS.get_environment(name).strip_edges()
	return fallback if value.is_empty() else value
