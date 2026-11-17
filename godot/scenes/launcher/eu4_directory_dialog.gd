# SPDX-License-Identifier: GPL-3.0-only
extends Window

const Install := preload("res://scenes/launcher/eu4_install.gd")

signal directory_changed

@onready var path_input: LineEdit = %Directory
@onready var browse_button: Button = %Browse
@onready var scan_button: Button = %Scan
@onready var candidates: ItemList = %Candidates
@onready var status_label: Label = %Status
@onready var cancel_button: Button = %Cancel
@onready var save_button: Button = %Save
@onready var directory_dialog: FileDialog = $DirectoryDialog

var _ignore_selection := false
var _scanning := false


func _ready() -> void:
	path_input.text = AppSettings.eu4_directory
	browse_button.pressed.connect(func() -> void: directory_dialog.popup_centered_ratio(0.7))
	directory_dialog.dir_selected.connect(_on_browse_selected)
	scan_button.pressed.connect(_scan)
	candidates.item_selected.connect(_on_candidate_selected)
	cancel_button.pressed.connect(queue_free)
	save_button.pressed.connect(_save)
	close_requested.connect(queue_free)
	_set_status("扫描只列出可载入的目录，不会改动已经保存的路径。")
	save_button.grab_focus.call_deferred()


func _unhandled_key_input(event: InputEvent) -> void:
	if event.is_action_pressed("ui_cancel"):
		set_input_as_handled()
		queue_free()


func _on_browse_selected(path: String) -> void:
	path_input.text = Install.normalize_path(path)
	_set_status("已选择该目录，尚未保存。")


func _scan() -> void:
	if _scanning:
		return
	_scanning = true
	scan_button.disabled = true
	_set_status("正在扫描…")
	# 先画出状态，再访问各个固定位置。
	await get_tree().process_frame
	var found: PackedStringArray = Install.scan()
	_scanning = false
	if not is_instance_valid(scan_button):
		return
	scan_button.disabled = false
	_show_candidates(found)


func _show_candidates(found: PackedStringArray) -> void:
	_ignore_selection = true
	candidates.clear()
	var current := Install.normalize_path(path_input.text)
	for path in found:
		var index := candidates.add_item(path)
		candidates.set_item_metadata(index, path)
		candidates.set_item_tooltip(index, path)
		if not current.is_empty() and path.to_lower() == current.to_lower():
			candidates.select(index)
	_ignore_selection = false
	if found.is_empty():
		var kept := "" if current.is_empty() else "当前路径保持不变。"
		_set_status("没有找到可用的欧陆风云 IV 安装目录。" + kept)
		return
	var hint := "点选其一后保存，才会写入设置。"
	if not current.is_empty():
		hint = "当前路径保持不变。点选其一后保存，才会替换。"
	_set_status("找到 %d 个可载入的目录。%s" % [found.size(), hint])


func _on_candidate_selected(index: int) -> void:
	if _ignore_selection:
		return
	path_input.text = str(candidates.get_item_metadata(index))
	_set_status("已选中该目录，尚未保存。")


func _save() -> void:
	var error := AppSettings.set_eu4_directory(path_input.text)
	if error != OK:
		_set_status("无法保存设置：" + error_string(error), true)
		return
	directory_changed.emit()
	queue_free()


func _set_status(message: String, is_error := false) -> void:
	status_label.text = message
	status_label.modulate = Color(1.0, 0.55, 0.48) if is_error else Color.WHITE
