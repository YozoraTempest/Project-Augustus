# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Project Augustus contributors

extends Node

signal save_selected(path: String)
signal load_selected(path: String)

var save_dialog: FileDialog
var load_dialog: FileDialog
var active_dialog: FileDialog
var pending := false
var _request_generation := 0
var _active_file_selected: Callable
var _active_canceled: Callable

func setup(default_path: String) -> void:
	if save_dialog != null or load_dialog != null:
		return
	var resolved_path := ProjectSettings.globalize_path(default_path)
	var default_directory := resolved_path.get_base_dir()
	var default_file := resolved_path.get_file()
	save_dialog = _create_dialog(
		FileDialog.FILE_MODE_SAVE_FILE, "另存项目存档", default_directory, default_file, true
	)
	load_dialog = _create_dialog(
		FileDialog.FILE_MODE_OPEN_FILE, "打开项目存档", default_directory, default_file, false
	)
	add_child(save_dialog)
	add_child(load_dialog)
	_apply_dialog_bounds(save_dialog)
	_apply_dialog_bounds(load_dialog)
	get_viewport().size_changed.connect(_on_viewport_size_changed)

func _create_dialog(
	file_mode: int,
	title_text: String,
	default_directory: String,
	default_file: String,
	confirm_overwrite: bool
) -> FileDialog:
	var dialog := FileDialog.new()
	dialog.file_mode = file_mode
	dialog.access = FileDialog.ACCESS_FILESYSTEM
	dialog.filters = PackedStringArray(["*.json;Project Augustus 存档;application/json"])
	dialog.mode_overrides_title = false
	dialog.title = title_text
	dialog.current_dir = default_directory
	dialog.current_file = default_file
	dialog.overwrite_warning_enabled = confirm_overwrite
	return dialog

func show_save() -> void:
	_show_dialog(save_dialog)

func show_load() -> void:
	_show_dialog(load_dialog)

func _show_dialog(dialog: FileDialog) -> void:
	if pending or dialog == null or dialog.visible:
		return
	_apply_dialog_bounds(dialog)
	_request_generation += 1
	var generation := _request_generation
	pending = true
	active_dialog = dialog
	_active_file_selected = Callable(self, "_on_file_selected").bind(dialog, generation)
	_active_canceled = Callable(self, "_on_dialog_canceled").bind(dialog, generation)
	dialog.file_selected.connect(_active_file_selected)
	dialog.canceled.connect(_active_canceled)
	dialog.popup_centered_clamped(Vector2i(900, 600), 0.85)

func _on_viewport_size_changed() -> void:
	_apply_dialog_bounds(save_dialog)
	_apply_dialog_bounds(load_dialog)

func _apply_dialog_bounds(dialog: FileDialog) -> void:
	if dialog == null:
		return
	var viewport_size: Vector2 = get_viewport().get_visible_rect().size
	var maximum_size := Vector2i(
		maxi(1, int(viewport_size.x * 0.85)),
		maxi(1, int(viewport_size.y * 0.85))
	)
	dialog.min_size = Vector2i(mini(480, maximum_size.x), mini(320, maximum_size.y))
	dialog.max_size = maximum_size
	_configure_nested_confirmation_dialogs(dialog, maximum_size)

func _configure_nested_confirmation_dialogs(node: Node, maximum_size: Vector2i) -> void:
	for child in node.get_children(true):
		if child is AcceptDialog:
			var confirmation := child as AcceptDialog
			confirmation.dialog_autowrap = true
			confirmation.min_size = Vector2i(mini(440, maximum_size.x), 0)
			confirmation.max_size = maximum_size
		_configure_nested_confirmation_dialogs(child, maximum_size)

func _on_file_selected(path: String, dialog: FileDialog, generation: int) -> void:
	if not _finish_request(dialog, generation):
		return
	if dialog == save_dialog:
		save_selected.emit(path)
	else:
		load_selected.emit(path)

func _on_dialog_canceled(dialog: FileDialog, generation: int) -> void:
	_finish_request(dialog, generation)

func _finish_request(dialog: FileDialog, generation: int) -> bool:
	if not pending or active_dialog != dialog or _request_generation != generation:
		return false
	_request_generation += 1
	_disconnect_active_signals(dialog)
	pending = false
	active_dialog = null
	_active_file_selected = Callable()
	_active_canceled = Callable()
	return true

func _disconnect_active_signals(dialog: FileDialog) -> void:
	if dialog == null:
		return
	if _active_file_selected.is_valid() and dialog.file_selected.is_connected(_active_file_selected):
		dialog.file_selected.disconnect(_active_file_selected)
	if _active_canceled.is_valid() and dialog.canceled.is_connected(_active_canceled):
		dialog.canceled.disconnect(_active_canceled)

func dismiss() -> void:
	_request_generation += 1
	var dialog_to_disconnect := active_dialog
	active_dialog = null
	pending = false
	_disconnect_active_signals(dialog_to_disconnect)
	_active_file_selected = Callable()
	_active_canceled = Callable()
	_hide_window_tree(save_dialog)
	_hide_window_tree(load_dialog)

func _hide_window_tree(node: Node) -> void:
	if node == null:
		return
	for child in node.get_children(true):
		_hide_window_tree(child)
	if node is Window and node.visible:
		(node as Window).hide()
