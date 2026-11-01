# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Project Augustus contributors

extends SceneTree

var failures := 0
var temp_root := ""

func check(condition: bool, message: String) -> void:
	if not condition:
		push_error(message)
		failures += 1

func _initialize() -> void:
	root.set_deferred("size", Vector2i(1100, 720))
	run.call_deferred()

func run() -> void:
	var fixture := ProjectSettings.globalize_path("res://../examples/world")
	temp_root = ProjectSettings.globalize_path("user://saved-subject-relations-%d" % OS.get_process_id())
	var game_root := temp_root.path_join("game")
	check(copy_fixture(fixture, game_root), "copy the self-authored base content")
	check(write_fixture_files(game_root), "write self-authored subject mission content")
	var relation_save := write_save("relations.eu4", "diplomacy={dependency={first=BBB second=AAA subject_type=test_union} dependency={first=CCC second=BBB subject_type=test_vassal}}\n")
	var missing_save := write_save("missing-diplomacy.eu4", "")
	var empty_save := write_save("empty-diplomacy.eu4", "diplomacy={}\n")
	check(not relation_save.is_empty() and not missing_save.is_empty() and not empty_save.is_empty(), "write self-authored EU4 saves")
	if failures:
		finish()
		return

	var packed: PackedScene = load("res://world.tscn")
	var ui = packed.instantiate()
	root.add_child(ui)
	await process_frame
	ui.game_path.text = game_root
	ui.eu4_save.text = relation_save
	check(ui.load_world(), "load saved subject relations into world.tscn")
	if failures:
		finish()
		return
	select_aaa_and_expand(ui)
	var initial_query: Dictionary = JSON.parse_string(ui.session.query_country_missions("AAA"))
	check(initial_query.get("ok", false), "query imported subject mission")
	if not initial_query.get("ok", false):
		finish()
		return
	var task := find_mission(initial_query.value, "saved_subject_task")
	check(initial_query.value.selection_known, "saved mission selection is known")
	check(task.get("requirements_met") == true and task.get("claimable") == true, "saved graph satisfies the selected task")
	var before_packet: Dictionary = JSON.parse_string(ui.session.save_snapshot())
	check(before_packet.get("ok", false), "save the loaded subject graph snapshot")
	if not before_packet.get("ok", false):
		finish()
		return
	var before_query: String = before_packet.value
	ui.session.query_country_missions("AAA")
	var after_packet: Dictionary = JSON.parse_string(ui.session.save_snapshot())
	check(after_packet.get("ok", false) and after_packet.value == before_query, "subject mission queries remain read-only")
	check(ui.mission_claim_buttons.has("saved_subject_task"), "expanded AAA mission list renders the task button")
	if ui.mission_claim_buttons.has("saved_subject_task"):
		var claim_button: Button = ui.mission_claim_buttons["saved_subject_task"]
		check(not claim_button.disabled, "saved relation enables the task button")
		claim_button.pressed.emit()
		await process_frame
		check(ui.mission_claim_buttons["saved_subject_task"].disabled, "claiming disables the completed task button")
		var claimed: Dictionary = JSON.parse_string(ui.session.query_country_missions("AAA"))
		check(claimed.get("ok", false), "query the claimed task")
		if not claimed.get("ok", false):
			finish()
			return
		check(find_mission(claimed.value, "saved_subject_task").get("completed") == true, "the task reward is recorded")
		var snapshot: Dictionary = JSON.parse_string(ui.session.restore_snapshot(before_query))
		check(snapshot.get("ok", false), "restore the pre-claim snapshot")
		if not snapshot.get("ok", false):
			finish()
			return
		ui.refresh_country_missions("AAA")
		var restored: Dictionary = JSON.parse_string(ui.session.query_country_missions("AAA"))
		check(restored.get("ok", false), "query the restored task")
		if not restored.get("ok", false):
			finish()
			return
		check(find_mission(restored.value, "saved_subject_task").get("claimable") == true, "snapshot restores the subject task state")
		check(not ui.mission_claim_buttons["saved_subject_task"].disabled, "restored task button is enabled again")

	ui.eu4_save.text = missing_save
	if not ui.load_world():
		check(false, "load a save without root diplomacy")
		finish()
		return
	select_aaa_and_expand(ui)
	var missing: Dictionary = JSON.parse_string(ui.session.query_country_missions("AAA"))
	check(missing.get("ok", false), "query mission with unknown diplomacy")
	if not missing.get("ok", false):
		finish()
		return
	var missing_task := find_mission(missing.value, "saved_subject_task")
	check(missing_task.get("requirements_met") == null, "missing diplomacy keeps the task condition unknown")
	check(ui.mission_claim_buttons.has("saved_subject_task") and ui.mission_claim_buttons["saved_subject_task"].disabled, "unknown relation disables claiming without becoming false")

	ui.eu4_save.text = empty_save
	if not ui.load_world():
		check(false, "load a save with explicitly empty diplomacy")
		finish()
		return
	select_aaa_and_expand(ui)
	var empty: Dictionary = JSON.parse_string(ui.session.query_country_missions("AAA"))
	check(empty.get("ok", false), "query mission with known empty diplomacy")
	if not empty.get("ok", false):
		finish()
		return
	var empty_task := find_mission(empty.value, "saved_subject_task")
	check(empty_task.get("requirements_met") == false, "empty diplomacy evaluates the task condition as false")
	check(ui.mission_claim_buttons.has("saved_subject_task") and ui.mission_claim_buttons["saved_subject_task"].disabled, "known empty graph keeps the task unclaimable")
	finish()

func select_aaa_and_expand(ui) -> void:
	var alpha_index := -1
	for index in ui.country_choice.item_count:
		if ui.country_choice.get_item_metadata(index) == "AAA":
			alpha_index = index
			break
	check(alpha_index >= 0, "AAA exists in the loaded country list")
	if alpha_index < 0:
		return
	ui.select_country(alpha_index, false)
	if not ui.mission_expanded:
		ui.toggle_mission_section()

func find_mission(report: Dictionary, mission_id: String) -> Dictionary:
	for mission in report.get("missions", []):
		if mission.get("id") == mission_id:
			return mission
	return {}

func write_fixture_files(game_root: String) -> bool:
	var missions_dir := game_root.path_join("missions")
	var subject_types_dir := game_root.path_join("common/subject_types")
	var tags_dir := game_root.path_join("common/country_tags")
	var countries_dir := game_root.path_join("common/countries")
	var history_dir := game_root.path_join("history/countries")
	for directory in [missions_dir, subject_types_dir, tags_dir, countries_dir, history_dir]:
		if not DirAccess.dir_exists_absolute(directory) and DirAccess.make_dir_recursive_absolute(directory) != OK:
			return false
	return (
		write_text(missions_dir.path_join("saved_subjects.txt"), "saved_subject_series={slot=1 potential={tag=AAA} saved_subject_task={position=1 trigger={is_subject_of=BBB is_subject_of_type=test_union overlord={tag=BBB} BBB={num_of_subjects=1 any_subject_country={tag=AAA is_subject_of_type=test_union}} NOT={CCC={num_of_subjects=2}}} effect={add_treasury=13}}}\n")
		and write_text(subject_types_dir.path_join("saved_subjects.txt"), "test_union={copy_from=default}\n")
		and write_text(tags_dir.path_join("saved_subjects.txt"), "CCC=\"countries/Gamma.txt\"\n")
		and write_text(countries_dir.path_join("Gamma.txt"), "color={40 120 70}\n")
		and write_text(history_dir.path_join("CCC - Gamma.txt"), "capital=1 primary_culture=alpha religion=test_faith government=monarchy\n")
	)

func write_save(name: String, diplomacy: String) -> String:
	var path := temp_root.path_join(name)
	var text := "EU4txt\ndate=1445.1.1\nplayer=AAA\n%s" % diplomacy
	text += "countries={AAA={human=yes treasury=5.000 completed_missions={} country_missions={mission_slot={saved_subject_series}}} BBB={human=no} CCC={human=no}}\nprovinces={}\n"
	return path if write_text(path, text) else ""

func write_text(path: String, contents: String) -> bool:
	var file := FileAccess.open(path, FileAccess.WRITE)
	if file == null:
		return false
	file.store_string(contents)
	var error := file.get_error()
	file.close()
	return error == OK

func copy_fixture(source: String, destination: String) -> bool:
	if DirAccess.make_dir_recursive_absolute(destination) != OK:
		return false
	for name in DirAccess.get_files_at(source):
		if DirAccess.copy_absolute(source.path_join(name), destination.path_join(name)) != OK:
			return false
	for name in DirAccess.get_directories_at(source):
		if not copy_fixture(source.path_join(name), destination.path_join(name)):
			return false
	return true

func finish() -> void:
	cleanup()
	print("Augustus saved subject relations integration: %d failures" % failures)
	quit(1 if failures else 0)

func cleanup() -> void:
	if temp_root.is_empty():
		return
	var expected := ProjectSettings.globalize_path("user://saved-subject-relations-%d" % OS.get_process_id())
	if temp_root == expected:
		remove_fixture(temp_root)

func remove_fixture(path: String) -> void:
	if path != temp_root and not path.begins_with(temp_root + "/"):
		return
	for name in DirAccess.get_files_at(path):
		DirAccess.remove_absolute(path.path_join(name))
	for name in DirAccess.get_directories_at(path):
		remove_fixture(path.path_join(name))
	DirAccess.remove_absolute(path)
