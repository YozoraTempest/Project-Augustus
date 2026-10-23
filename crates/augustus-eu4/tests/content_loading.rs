// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_eu4::{
    ContentRoot, ContentSet, ContentSource, DefinitionIndex, DefinitionKind, SourceLocation,
    Suppression, TextEncoding,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

// All descriptors and scripts below are synthetic data authored for this project.

fn directory(parent: &Path, name: &str) -> PathBuf {
    let path = parent.join(name);
    fs::create_dir(&path).unwrap();
    path
}

fn write(root: &Path, relative: &str, bytes: impl AsRef<[u8]>) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn base(root: &Path) -> ContentSource {
    ContentSource::base(ContentRoot::open(root).unwrap())
}

fn mod_source(root: &Path) -> ContentSource {
    ContentSource::mod_directory(root, TextEncoding::Utf8).unwrap()
}

#[cfg(windows)]
fn extended_path(path: &Path) -> PathBuf {
    let path = path.to_str().unwrap();
    if path.starts_with(r"\\?\") {
        return PathBuf::from(path);
    }
    if let Some(unc) = path.strip_prefix(r"\\") {
        PathBuf::from(format!(r"\\?\UNC\{unc}"))
    } else {
        PathBuf::from(format!(r"\\?\{path}"))
    }
}

#[test]
fn dependencies_and_source_priority_choose_the_first_matching_file() {
    let fixture = tempfile::tempdir().unwrap();
    let vanilla = directory(fixture.path(), "base");
    let pack = directory(fixture.path(), "pack");
    let patch = directory(fixture.path(), "patch");
    write(&pack, "descriptor.mod", "name=Pack path=ignored/location");
    write(&patch, "descriptor.mod", "name=Patch dependencies={ Pack }");
    write(&vanilla, "common/buildings/example.txt", "old={ cost=1 }");
    write(&pack, "common/buildings/example.txt", "middle={ cost=2 }");
    write(&patch, "common/buildings/example.txt", "new={ cost=3 }");
    let content =
        ContentSet::resolve(vec![base(&vanilla), mod_source(&pack), mod_source(&patch)]).unwrap();
    assert_eq!(
        content
            .sources()
            .iter()
            .map(ContentSource::name)
            .collect::<Vec<_>>(),
        ["Patch", "Pack", "EU4 base"]
    );
    let trace = content
        .trace("common/buildings/example.txt")
        .unwrap()
        .unwrap();
    assert_eq!(trace.candidates.len(), 3);
    assert_eq!(trace.candidates[0].origin.source_index, 0);
    assert_eq!(
        trace.candidates[1].suppressed_by,
        Some(Suppression::FileOverride { by_source: 0 })
    );
    assert_eq!(
        trace.candidates[2].suppressed_by,
        Some(Suppression::FileOverride { by_source: 0 })
    );
    assert_eq!(trace.effective().unwrap().source_index, 0);
    let document = content
        .read_text("common/buildings/example.txt", TextEncoding::Utf8)
        .unwrap();
    assert_eq!(document.parse().unwrap().root_fields()[0].name, "new");
    assert_eq!(document.source(), trace.effective().unwrap().physical_path);
    assert_eq!(
        fs::read(vanilla.join("common/buildings/example.txt")).unwrap(),
        b"old={ cost=1 }"
    );
    fs::remove_file(patch.join("common/buildings/example.txt")).unwrap();
    assert!(
        content
            .read_text("common/buildings/example.txt", TextEncoding::Utf8)
            .is_err()
    );
}

#[test]
fn empty_directory_replacement_hides_lower_files_without_a_fallback() {
    let fixture = tempfile::tempdir().unwrap();
    let vanilla = directory(fixture.path(), "base");
    let pack = directory(fixture.path(), "pack");
    write(
        &pack,
        "descriptor.mod",
        "name=Pack replace_path=events replace_path=events",
    );
    write(&vanilla, "events/old.txt", "event={}");
    write(&vanilla, "events/nested/old.txt", "nested={}");
    write(&vanilla, "events_extra/kept.txt", "kept={}");
    let content = ContentSet::resolve(vec![base(&vanilla), mod_source(&pack)]).unwrap();
    for path in ["events/old.txt", "events/nested/old.txt"] {
        let trace = content.trace(path).unwrap().unwrap();
        assert!(trace.effective().is_none());
        assert_eq!(
            trace.candidates[0].suppressed_by,
            Some(Suppression::DirectoryReplacement {
                by_source: 0,
                replace_path: "events".into(),
            })
        );
        assert!(content.read_text(path, TextEncoding::Utf8).is_err());
    }
    assert!(
        content
            .trace("events_extra/kept.txt")
            .unwrap()
            .unwrap()
            .effective()
            .is_some()
    );
    assert_eq!(
        content.sources()[0]
            .descriptor()
            .unwrap()
            .replace_paths
            .len(),
        2
    );
}

#[test]
fn replacement_keeps_higher_priority_files_and_cuts_lower_priority_sources() {
    let fixture = tempfile::tempdir().unwrap();
    let vanilla = directory(fixture.path(), "base");
    let pack = directory(fixture.path(), "pack");
    let overlay = directory(fixture.path(), "overlay");
    let fallback = directory(fixture.path(), "fallback");
    write(
        &pack,
        "descriptor.mod",
        r#"name="Middle Pack" replace_path="COMMON/BUILDINGS/""#,
    );
    write(&overlay, "descriptor.mod", "name=\"A Overlay\"");
    write(&fallback, "descriptor.mod", "name=\"Z Fallback\"");
    write(&vanilla, "common/buildings/same.txt", "base={}");
    write(&pack, "common/buildings/same.txt", "pack={}");
    write(&overlay, "common/buildings/same.txt", "overlay={}");
    write(&overlay, "common/buildings/extra.txt", "extra={}");
    write(&pack, "common/buildings/pack_only.txt", "pack_only={}");
    write(&fallback, "common/buildings/same.txt", "fallback={}");
    write(
        &fallback,
        "common/buildings/lower_only.txt",
        "lower_only={}",
    );
    let content = ContentSet::resolve(vec![
        base(&vanilla),
        mod_source(&pack),
        mod_source(&fallback),
        mod_source(&overlay),
    ])
    .unwrap();
    let trace = content.trace("common/buildings/same.txt").unwrap().unwrap();
    assert_eq!(trace.candidates[0].origin.source_index, 0);
    assert_eq!(trace.effective().unwrap().source_index, 0);
    assert_eq!(
        trace.candidates[1].suppressed_by,
        Some(Suppression::FileOverride { by_source: 0 })
    );
    assert!(matches!(
        trace.candidates[2].suppressed_by,
        Some(Suppression::DirectoryReplacement { by_source: 1, .. })
    ));
    assert!(matches!(
        trace.candidates[3].suppressed_by,
        Some(Suppression::DirectoryReplacement { by_source: 1, .. })
    ));
    assert_eq!(
        content
            .trace("common/buildings/extra.txt")
            .unwrap()
            .unwrap()
            .effective()
            .unwrap()
            .source_index,
        0
    );
    assert_eq!(
        content
            .trace("common/buildings/pack_only.txt")
            .unwrap()
            .unwrap()
            .effective()
            .unwrap()
            .source_index,
        1
    );
    assert!(
        content
            .trace("common/buildings/lower_only.txt")
            .unwrap()
            .unwrap()
            .effective()
            .is_none()
    );
}

#[test]
fn directory_replacement_does_not_cover_a_sibling_file() {
    let fixture = tempfile::tempdir().unwrap();
    let vanilla = directory(fixture.path(), "base");
    let replacer = directory(fixture.path(), "replacer");
    let lower = directory(fixture.path(), "lower");
    write(&vanilla, "descriptor.mod", "");
    write(&vanilla, "common/defines.lua", "base_defines = true");
    write(&vanilla, "common/defines/base.txt", "base={}");
    write(
        &replacer,
        "descriptor.mod",
        r#"name="A Replacer" replace_path="common/defines""#,
    );
    write(&replacer, "common/defines/replacer.txt", "replacer={}");
    write(&lower, "descriptor.mod", "name=\"B Lower\"");
    write(&lower, "common/defines/lower.txt", "lower={}");

    let content = ContentSet::resolve(vec![
        base(&vanilla),
        mod_source(&replacer),
        mod_source(&lower),
    ])
    .unwrap();

    let sibling = content.trace("common/defines.lua").unwrap().unwrap();
    assert_eq!(sibling.effective().unwrap().source_index, 2);
    assert!(sibling.candidates[0].suppressed_by.is_none());

    let replaced = content.trace("common/defines/lower.txt").unwrap().unwrap();
    assert!(matches!(
        replaced.candidates[0].suppressed_by,
        Some(Suppression::DirectoryReplacement { by_source: 0, .. })
    ));
    assert!(replaced.effective().is_none());
    assert_eq!(
        content
            .trace("common/defines/replacer.txt")
            .unwrap()
            .unwrap()
            .effective()
            .unwrap()
            .source_index,
        0
    );
}

#[test]
fn replacement_normalizes_windows_separators_before_cutting_lower_sources() {
    let fixture = tempfile::tempdir().unwrap();
    let vanilla = directory(fixture.path(), "base");
    let pack = directory(fixture.path(), "pack");
    write(
        &pack,
        "descriptor.mod",
        r#"name=Pack replace_path="common\buildings""#,
    );
    write(&vanilla, "common/buildings/vanilla.txt", "base={}");
    write(&pack, "common/buildings/pack.txt", "pack={}");

    let source = mod_source(&pack);
    assert_eq!(
        source.descriptor().unwrap().replace_paths,
        ["common/buildings"]
    );
    let content = ContentSet::resolve(vec![base(&vanilla), source]).unwrap();

    assert!(
        content
            .trace("common/buildings/vanilla.txt")
            .unwrap()
            .unwrap()
            .effective()
            .is_none()
    );
    assert_eq!(
        content
            .trace("common/buildings/pack.txt")
            .unwrap()
            .unwrap()
            .effective()
            .unwrap()
            .source_index,
        0
    );
}

#[test]
fn directory_replacement_removes_lower_priority_mods_and_base() {
    let fixture = tempfile::tempdir().unwrap();
    let vanilla = directory(fixture.path(), "base");
    let pack = directory(fixture.path(), "pack");
    let patch = directory(fixture.path(), "patch");
    write(&pack, "descriptor.mod", "name=Pack");
    write(
        &patch,
        "descriptor.mod",
        "name=\"A Patch\" replace_path=events",
    );
    write(&vanilla, "events/example.txt", "base={}");
    write(&pack, "events/example.txt", "pack={}");
    let content =
        ContentSet::resolve(vec![base(&vanilla), mod_source(&pack), mod_source(&patch)]).unwrap();
    let trace = content.trace("events/example.txt").unwrap().unwrap();
    assert_eq!(
        trace.candidates[0].suppressed_by,
        Some(Suppression::DirectoryReplacement {
            by_source: 0,
            replace_path: "events".into(),
        })
    );
    assert_eq!(trace.candidates[1].origin.source_index, 2);
    assert_eq!(
        trace.candidates[1].suppressed_by,
        Some(Suppression::DirectoryReplacement {
            by_source: 0,
            replace_path: "events".into(),
        })
    );
    assert!(trace.effective().is_none());
}

#[test]
fn file_lookup_normalizes_separators_and_ascii_case_but_keeps_physical_spelling() {
    let fixture = tempfile::tempdir().unwrap();
    let vanilla = directory(fixture.path(), "base");
    let pack = directory(fixture.path(), "pack");
    write(&pack, "descriptor.mod", "name=Pack");
    write(&vanilla, "common/buildings/example.txt", "old={}");
    write(&pack, "common/buildings/Example.TXT", "new={}");
    let content = ContentSet::resolve(vec![base(&vanilla), mod_source(&pack)]).unwrap();
    let trace = content
        .trace(r"COMMON\BUILDINGS\EXAMPLE.txt")
        .unwrap()
        .unwrap();
    assert_eq!(trace.candidates.len(), 2);
    assert_eq!(
        trace.effective().unwrap().relative_path,
        Path::new("common/buildings/Example.TXT")
    );
    let paths = content
        .effective_files()
        .map(|(path, _)| path)
        .collect::<Vec<_>>();
    assert_eq!(paths, ["common/buildings/example.txt", "descriptor.mod"]);
}

#[test]
fn dependencies_reorder_mods_and_source_identity_is_validated() {
    let fixture = tempfile::tempdir().unwrap();
    let vanilla = directory(fixture.path(), "base");
    let pack = directory(fixture.path(), "pack");
    let patch = directory(fixture.path(), "patch");
    write(&pack, "descriptor.mod", "name=Pack");
    write(&patch, "descriptor.mod", "name=Patch dependencies={ Pack }");
    let unresolved = directory(fixture.path(), "unresolved");
    write(
        &unresolved,
        "descriptor.mod",
        "name=Unresolved dependencies={ Absent }",
    );
    assert!(ContentSet::resolve(vec![base(&vanilla), mod_source(&unresolved)]).is_ok());
    let order =
        ContentSet::resolve(vec![base(&vanilla), mod_source(&patch), mod_source(&pack)]).unwrap();
    assert_eq!(
        order
            .sources()
            .iter()
            .map(ContentSource::name)
            .collect::<Vec<_>>(),
        ["Patch", "Pack", "EU4 base"]
    );
    write(&patch, "descriptor.mod", "name=Patch");
    let name_order =
        ContentSet::resolve(vec![base(&vanilla), mod_source(&patch), mod_source(&pack)]).unwrap();
    assert_eq!(
        name_order
            .sources()
            .iter()
            .map(ContentSource::name)
            .collect::<Vec<_>>(),
        ["Pack", "Patch", "EU4 base"]
    );
    let root_mod = directory(fixture.path(), "root-mod");
    let middle_mod = directory(fixture.path(), "middle-mod");
    let dependency_mod = directory(fixture.path(), "dependency-mod");
    write(
        &root_mod,
        "descriptor.mod",
        "name=\"Z Root\" dependencies={ \"A Middle\" }",
    );
    write(
        &middle_mod,
        "descriptor.mod",
        "name=\"A Middle\" dependencies={ \"M Base\" }",
    );
    write(&dependency_mod, "descriptor.mod", "name=\"M Base\"");
    let dependency_chain = ContentSet::resolve(vec![
        base(&vanilla),
        mod_source(&root_mod),
        mod_source(&middle_mod),
        mod_source(&dependency_mod),
    ])
    .unwrap();
    assert_eq!(
        dependency_chain
            .sources()
            .iter()
            .map(ContentSource::name)
            .collect::<Vec<_>>(),
        ["Z Root", "A Middle", "M Base", "EU4 base"]
    );
    write(&patch, "descriptor.mod", "name=Patch dependencies={ Pack }");
    write(&pack, "descriptor.mod", "name=Pack dependencies={ Patch }");
    assert!(
        ContentSet::resolve(vec![base(&vanilla), mod_source(&pack), mod_source(&patch)])
            .unwrap_err()
            .to_string()
            .contains("cyclic mod dependencies")
    );
    write(&pack, "descriptor.mod", "name=Pack dependencies={ Pack }");
    assert!(ContentSet::resolve(vec![base(&vanilla), mod_source(&pack)]).is_err());
    write(&pack, "descriptor.mod", "name=Pack");
    write(&patch, "descriptor.mod", "name=Pack");
    assert!(
        ContentSet::resolve(vec![base(&vanilla), mod_source(&pack), mod_source(&patch)])
            .unwrap_err()
            .to_string()
            .contains("duplicate mod name")
    );
    assert!(
        ContentSet::resolve(vec![base(&vanilla), mod_source(&pack), mod_source(&pack)])
            .unwrap_err()
            .to_string()
            .contains("mounted more than once")
    );
    assert!(ContentSet::resolve(vec![]).is_err());
    assert!(ContentSet::resolve(vec![mod_source(&pack)]).is_err());
    assert!(ContentSet::resolve(vec![base(&vanilla), base(&pack)]).is_err());
}

#[test]
fn user_directory_precedes_mods_and_base_content() {
    let fixture = tempfile::tempdir().unwrap();
    let user = directory(fixture.path(), "user");
    let vanilla = directory(fixture.path(), "base");
    let pack = directory(fixture.path(), "pack");
    write(&pack, "descriptor.mod", "name=Pack");
    write(&user, "common/defines.lua", "from_user = true");
    write(&pack, "common/defines.lua", "from_mod = true");
    write(&vanilla, "common/defines.lua", "from_base = true");

    let content = ContentSet::resolve(vec![
        base(&vanilla),
        mod_source(&pack),
        ContentSource::user_directory(&user).unwrap(),
    ])
    .unwrap();
    assert_eq!(
        content
            .sources()
            .iter()
            .map(ContentSource::name)
            .collect::<Vec<_>>(),
        ["EU4 user directory", "Pack", "EU4 base"]
    );
    let document = content
        .read_text("common/defines.lua", TextEncoding::Utf8)
        .unwrap();
    assert_eq!(document.parse().unwrap().root_fields()[0].name, "from_user");
}

#[test]
fn launcher_config_filters_mods_before_dependency_ordering() {
    let fixture = tempfile::tempdir().unwrap();
    let vanilla = directory(fixture.path(), "base");
    let dependency = directory(fixture.path(), "dependency");
    let patch = directory(fixture.path(), "patch");
    let inactive_cycle = directory(fixture.path(), "inactive-cycle");
    write(&dependency, "descriptor.mod", "name=Pack");
    write(&patch, "descriptor.mod", "name=Patch dependencies={ Pack }");
    write(
        &inactive_cycle,
        "descriptor.mod",
        "name=Inactive dependencies={ Inactive }",
    );
    write(&vanilla, "common/defines.lua", "from_base = true");
    write(&dependency, "common/defines.lua", "from_pack = true");
    write(&patch, "common/defines.lua", "from_patch = true");
    let config = fixture.path().join("dlc_load.json");
    fs::write(
        &config,
        r#"{"ENABLED_MODS":["Patch","Pack"],"disabled_dlcs":[]}"#,
    )
    .unwrap();

    let content = ContentSet::resolve_with_launcher_config(
        vec![
            base(&vanilla),
            mod_source(&inactive_cycle),
            mod_source(&dependency),
            mod_source(&patch),
        ],
        &config,
    )
    .unwrap();

    assert_eq!(
        content
            .sources()
            .iter()
            .map(ContentSource::name)
            .collect::<Vec<_>>(),
        ["Patch", "Pack", "EU4 base"]
    );
    assert_eq!(
        content
            .read_text("common/defines.lua", TextEncoding::Utf8)
            .unwrap()
            .parse()
            .unwrap()
            .root_fields()[0]
            .name,
        "from_patch"
    );

    fs::write(&config, r#"{"enabled_mods":["Missing"]}"#).unwrap();
    let error = ContentSet::resolve_with_launcher_config(
        vec![base(&vanilla), mod_source(&dependency)],
        &config,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("no supplied directory source: Missing")
    );

    fs::write(&config, r#"{"enabled_mods":["Pack",1]}"#).unwrap();
    let error = ContentSet::resolve_with_launcher_config(
        vec![base(&vanilla), mod_source(&dependency)],
        &config,
    )
    .unwrap_err();
    assert!(error.to_string().contains("invalid type"));
}

#[test]
fn installation_resolver_builds_the_shared_user_mod_and_base_source_stack() {
    let fixture = tempfile::tempdir().unwrap();
    let vanilla = directory(fixture.path(), "base");
    let user = directory(fixture.path(), "user");
    let selected = directory(fixture.path(), "selected");
    let unselected = directory(fixture.path(), "unselected");
    write(&selected, "descriptor.mod", "name=Selected");
    write(&unselected, "descriptor.mod", "name=Unselected");
    write(&vanilla, "common/defines.lua", "from_base = true");
    write(&user, "common/defines.lua", "from_user = true");
    write(&selected, "common/defines.lua", "from_mod = true");
    write(&unselected, "common/defines.lua", "from_unselected = true");
    let config = fixture.path().join("dlc_load.json");
    fs::write(&config, r#"{"enabled_mods":["Selected"]}"#).unwrap();

    let content = ContentSet::from_installation(
        &vanilla,
        &[selected, unselected],
        Some(&user),
        Some(&config),
        TextEncoding::Utf8,
    )
    .unwrap();

    assert_eq!(
        content
            .sources()
            .iter()
            .map(ContentSource::name)
            .collect::<Vec<_>>(),
        ["EU4 user directory", "Selected", "EU4 base"]
    );
    assert_eq!(
        content
            .read_text("common/defines.lua", TextEncoding::Utf8)
            .unwrap()
            .parse()
            .unwrap()
            .root_fields()[0]
            .name,
        "from_user"
    );
}

#[test]
fn installation_resolver_matches_launcher_mod_paths_to_supplied_directories() {
    let fixture = tempfile::tempdir().unwrap();
    let vanilla = directory(fixture.path(), "base");
    let user = directory(fixture.path(), "user");
    let first = directory(fixture.path(), "first-pack");
    let selected = directory(fixture.path(), "selected-pack");
    for root in [&first, &selected] {
        write(root, "descriptor.mod", "name=Pack");
    }
    write(&vanilla, "common/defines.lua", "from_base = true");
    write(&first, "common/defines.lua", "from_first_pack = true");
    write(&selected, "common/defines.lua", "from_selected_pack = true");

    let selected_path = selected.to_string_lossy().replace('\\', "/");
    write(
        &user,
        "mod/pack.mod",
        format!("name=Pack path=\"{selected_path}\""),
    );
    let config = user.join("dlc_load.json");
    fs::write(
        &config,
        r#"{"enabled_mods":["mod/pack.mod"],"disabled_dlcs":[]}"#,
    )
    .unwrap();

    let content = ContentSet::from_installation(
        &vanilla,
        &[first, selected.clone()],
        Some(&user),
        Some(&config),
        TextEncoding::Utf8,
    )
    .unwrap();

    assert_eq!(
        content
            .sources()
            .iter()
            .map(ContentSource::name)
            .collect::<Vec<_>>(),
        ["EU4 user directory", "Pack", "EU4 base"]
    );
    let trace = content.trace("common/defines.lua").unwrap().unwrap();
    assert_eq!(trace.candidates.len(), 2);
    assert_eq!(
        trace.effective().unwrap().physical_path,
        fs::canonicalize(selected.join("common/defines.lua")).unwrap()
    );
    assert_eq!(
        content
            .read_text("common/defines.lua", TextEncoding::Utf8)
            .unwrap()
            .parse()
            .unwrap()
            .root_fields()[0]
            .name,
        "from_selected_pack"
    );
}

#[test]
fn installation_resolver_discovers_launcher_mod_directories_and_keeps_overlay_rules() {
    let fixture = tempfile::tempdir().unwrap();
    let vanilla = directory(fixture.path(), "base");
    let user = directory(fixture.path(), "user");
    let dependency = directory(fixture.path(), "dependency");
    let selected = directory(fixture.path(), "selected-mod");
    write(&dependency, "descriptor.mod", "name=Pack");
    write(
        &selected,
        "descriptor.mod",
        "name=Patch dependencies={ Pack } replace_path=events",
    );

    write(&vanilla, "common/defines/example.txt", "from_base = true");
    write(
        &dependency,
        "common/defines/example.txt",
        "from_pack = true",
    );
    write(&selected, "common/defines/example.txt", "from_patch = true");
    write(&user, "common/defines/example.txt", "from_user = true");
    write(&vanilla, "events/base.txt", "base_event = true");
    write(&dependency, "events/pack.txt", "pack_event = true");
    write(&user, "events/user.txt", "user_event = true");

    let selected_path = selected.to_string_lossy().replace('\\', "/");
    write(
        &user,
        "mod/patch.mod",
        format!("name=Patch path=\"{selected_path}\""),
    );
    let config = user.join("dlc_load.json");
    fs::write(&config, r#"{"enabled_mods":["Pack","mod/patch.mod"]}"#).unwrap();

    let content = ContentSet::from_installation(
        &vanilla,
        &[dependency],
        Some(&user),
        Some(&config),
        TextEncoding::Utf8,
    )
    .unwrap();

    assert_eq!(
        content
            .sources()
            .iter()
            .map(ContentSource::name)
            .collect::<Vec<_>>(),
        ["EU4 user directory", "Patch", "Pack", "EU4 base"]
    );
    assert_eq!(
        content
            .read_text("common/defines/example.txt", TextEncoding::Utf8)
            .unwrap()
            .parse()
            .unwrap()
            .root_fields()[0]
            .name,
        "from_user"
    );

    let replaced = content.trace("events/pack.txt").unwrap().unwrap();
    assert!(replaced.effective().is_none());
    assert_eq!(
        replaced.candidates[0].suppressed_by,
        Some(Suppression::DirectoryReplacement {
            by_source: 1,
            replace_path: "events".into(),
        })
    );
    let user_file = content.trace("events/user.txt").unwrap().unwrap();
    assert_eq!(user_file.effective().unwrap().source_index, 0);
}

#[test]
fn invalid_replace_paths_report_the_descriptor_and_queries_cannot_traverse() {
    let fixture = tempfile::tempdir().unwrap();
    let vanilla = directory(fixture.path(), "base");
    let pack = directory(fixture.path(), "pack");
    for path in [
        "",
        "../events",
        "/events",
        r"C:\events",
        "events/../common",
        ".",
        "events//nested",
    ] {
        write(
            &pack,
            "descriptor.mod",
            format!("name=Pack replace_path=\"{path}\""),
        );
        let error = ContentSource::mod_directory(&pack, TextEncoding::Utf8).unwrap_err();
        assert_eq!(
            error.path(),
            fs::canonicalize(pack.join("descriptor.mod")).unwrap()
        );
    }
    let content = ContentSet::resolve(vec![base(&vanilla)]).unwrap();
    for path in [
        "../outside.txt",
        "/outside.txt",
        r"C:\outside.txt",
        "a/./b",
        "",
    ] {
        assert!(content.trace(path).is_err());
    }
    assert!(content.trace("unknown.txt").unwrap().is_none());
}

#[test]
fn definitions_track_all_duplicates_but_only_from_effective_files() {
    let fixture = tempfile::tempdir().unwrap();
    let vanilla = directory(fixture.path(), "base");
    let pack = directory(fixture.path(), "pack");
    write(&pack, "descriptor.mod", "name=Pack");
    write(&vanilla, "common/buildings/same.txt", "discarded={}");
    write(
        &pack,
        "common/buildings/nested/ignored.txt",
        "nested_only={}",
    );
    write(
        &pack,
        "common/buildings/same.txt",
        "@cap=2\nwarehouse={}\nwarehouse={ nested={} }\nWarehouse={}",
    );
    write(&vanilla, "common/buildings/another.txt", "warehouse={}");
    write(
        &vanilla,
        "common/scripted_effects/other.txt",
        "not_a_building={}",
    );
    write(
        &vanilla,
        "common/buildings/readme.md",
        "ignored non-script input",
    );
    let content = ContentSet::resolve(vec![base(&vanilla), mod_source(&pack)]).unwrap();
    let index =
        DefinitionIndex::build(&content, DefinitionKind::Buildings, TextEncoding::Utf8).unwrap();
    assert!(index.get("discarded").is_none());
    assert!(index.get("nested_only").is_none());
    assert!(index.get("nested").is_none());
    assert!(index.get("@cap").is_none());
    assert!(index.get("not_a_building").is_none());
    let origins = index.get("warehouse").unwrap();
    assert_eq!(origins.len(), 3);
    assert_eq!(
        origins
            .iter()
            .map(|origin| origin.source_index)
            .collect::<Vec<_>>(),
        [1, 0, 0]
    );
    assert_eq!(origins[1].location, SourceLocation { line: 2, column: 1 });
    assert_eq!(origins[1].field_index, 1);
    assert_eq!(origins[2].location.line, 3);
    assert_eq!(index.get("Warehouse").unwrap().len(), 1);
}

#[test]
fn definition_locations_preserve_bom_crlf_unicode_and_quoted_key_positions() {
    let fixture = tempfile::tempdir().unwrap();
    let vanilla = directory(fixture.path(), "base");
    write(
        &vanilla,
        "common/scripted_effects/sample.txt",
        "\u{feff}# source\r\n  \"示例\"={} next={}\r\nthird={}",
    );
    let content = ContentSet::resolve(vec![base(&vanilla)]).unwrap();
    let index = DefinitionIndex::build(
        &content,
        DefinitionKind::ScriptedEffects,
        TextEncoding::Utf8,
    )
    .unwrap();
    assert_eq!(
        index.get("示例").unwrap()[0].location,
        SourceLocation { line: 2, column: 3 }
    );
    assert_eq!(
        index.get("next").unwrap()[0].location,
        SourceLocation {
            line: 2,
            column: 15
        }
    );
    assert_eq!(
        index.get("third").unwrap()[0].location,
        SourceLocation { line: 3, column: 1 }
    );
}

#[test]
fn declaration_shape_errors_include_the_actual_file_and_field_location() {
    let fixture = tempfile::tempdir().unwrap();
    let vanilla = directory(fixture.path(), "base");
    for invalid in ["bad=3", "bad>{}", "bad={ 1 2 }"] {
        write(
            &vanilla,
            "common/scripted_triggers/sample.txt",
            format!("# test\n  {invalid}"),
        );
        let content = ContentSet::resolve(vec![base(&vanilla)]).unwrap();
        let error = DefinitionIndex::build(
            &content,
            DefinitionKind::ScriptedTriggers,
            TextEncoding::Utf8,
        )
        .unwrap_err();
        assert_eq!(
            error.location(),
            Some(SourceLocation { line: 2, column: 3 })
        );
        assert_eq!(
            error.path(),
            fs::canonicalize(vanilla.join("common/scripted_triggers/sample.txt")).unwrap()
        );
    }
}

#[cfg(windows)]
#[test]
fn content_root_reads_a_root_beyond_legacy_max_path() {
    use std::os::windows::ffi::OsStrExt;

    let fixture = tempfile::tempdir().unwrap();
    let mut root = directory(fixture.path(), "base");
    let segment = "segment-12345678901234567890123456789012345678";
    while root.as_os_str().encode_wide().count() <= 270 {
        root.push(segment);
        fs::create_dir(extended_path(&root)).unwrap();
    }
    assert!(root.as_os_str().encode_wide().count() > 260);

    write(&root, "common/buildings/example.txt", "example={ value=1 }");
    let content = ContentRoot::open(&root).unwrap();
    let files = content.files().unwrap();
    assert_eq!(files.len(), 1);
    let document = content
        .read_text(&files[0].relative_path, TextEncoding::Utf8)
        .unwrap();
    assert!(document.parse().is_ok());
}

#[cfg(unix)]
#[test]
fn ambiguous_file_case_within_one_source_is_rejected() {
    let fixture = tempfile::tempdir().unwrap();
    write(fixture.path(), "example.txt", "one={}");
    write(fixture.path(), "Example.txt", "two={}");
    let error = ContentSet::resolve(vec![base(fixture.path())]).unwrap_err();
    assert!(error.to_string().contains("ambiguous content path"));
}
