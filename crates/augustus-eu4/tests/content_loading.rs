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

#[test]
fn explicit_order_preserves_override_chain_and_reads_only_the_selected_file() {
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
    let trace = content
        .trace("common/buildings/example.txt")
        .unwrap()
        .unwrap();
    assert_eq!(trace.candidates.len(), 3);
    assert_eq!(
        trace.candidates[0].suppressed_by,
        Some(Suppression::FileOverride { by_source: 1 })
    );
    assert_eq!(
        trace.candidates[1].suppressed_by,
        Some(Suppression::FileOverride { by_source: 2 })
    );
    assert_eq!(trace.effective().unwrap().source_index, 2);
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
                by_source: 1,
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
        content.sources()[1]
            .descriptor()
            .unwrap()
            .replace_paths
            .len(),
        2
    );
}

#[test]
fn replacement_keeps_own_files_and_allows_later_mods_to_add_or_override() {
    let fixture = tempfile::tempdir().unwrap();
    let vanilla = directory(fixture.path(), "base");
    let pack = directory(fixture.path(), "pack");
    let patch = directory(fixture.path(), "patch");
    write(
        &pack,
        "descriptor.mod",
        r#"name=Pack replace_path="COMMON/BUILDINGS/""#,
    );
    write(&patch, "descriptor.mod", "name=Patch dependencies={ Pack }");
    write(&vanilla, "common/buildings/same.txt", "base={}");
    write(&pack, "common/buildings/same.txt", "pack={}");
    write(&patch, "common/buildings/same.txt", "patch={}");
    write(&patch, "common/buildings/extra.txt", "extra={}");
    let content =
        ContentSet::resolve(vec![base(&vanilla), mod_source(&pack), mod_source(&patch)]).unwrap();
    let trace = content.trace("common/buildings/same.txt").unwrap().unwrap();
    assert!(matches!(
        trace.candidates[0].suppressed_by,
        Some(Suppression::DirectoryReplacement { by_source: 1, .. })
    ));
    assert_eq!(
        trace.candidates[1].suppressed_by,
        Some(Suppression::FileOverride { by_source: 2 })
    );
    assert_eq!(trace.effective().unwrap().source_index, 2);
    assert_eq!(
        content
            .trace("common/buildings/extra.txt")
            .unwrap()
            .unwrap()
            .effective()
            .unwrap()
            .source_index,
        2
    );
}

#[test]
fn later_directory_replacement_removes_an_earlier_mod_as_well_as_the_base() {
    let fixture = tempfile::tempdir().unwrap();
    let vanilla = directory(fixture.path(), "base");
    let pack = directory(fixture.path(), "pack");
    let patch = directory(fixture.path(), "patch");
    write(&pack, "descriptor.mod", "name=Pack");
    write(
        &patch,
        "descriptor.mod",
        "name=Patch dependencies={ Pack } replace_path=events",
    );
    write(&vanilla, "events/example.txt", "base={}");
    write(&pack, "events/example.txt", "pack={}");
    let content =
        ContentSet::resolve(vec![base(&vanilla), mod_source(&pack), mod_source(&patch)]).unwrap();
    let trace = content.trace("events/example.txt").unwrap().unwrap();
    assert_eq!(
        trace.candidates[0].suppressed_by,
        Some(Suppression::FileOverride { by_source: 1 })
    );
    assert!(matches!(
        trace.candidates[1].suppressed_by,
        Some(Suppression::DirectoryReplacement { by_source: 2, .. })
    ));
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
fn dependencies_and_source_identity_are_validated_without_reordering() {
    let fixture = tempfile::tempdir().unwrap();
    let vanilla = directory(fixture.path(), "base");
    let pack = directory(fixture.path(), "pack");
    let patch = directory(fixture.path(), "patch");
    write(&pack, "descriptor.mod", "name=Pack");
    write(&patch, "descriptor.mod", "name=Patch dependencies={ Pack }");
    let missing = ContentSet::resolve(vec![base(&vanilla), mod_source(&patch)]).unwrap_err();
    assert!(missing.to_string().contains("missing mod dependency: Pack"));
    assert_eq!(
        missing.path(),
        fs::canonicalize(patch.join("descriptor.mod")).unwrap()
    );
    let order = ContentSet::resolve(vec![base(&vanilla), mod_source(&patch), mod_source(&pack)])
        .unwrap_err();
    assert!(order.to_string().contains("must precede Patch: Pack"));
    write(&pack, "descriptor.mod", "name=Pack dependencies={ Patch }");
    assert!(
        ContentSet::resolve(vec![base(&vanilla), mod_source(&pack), mod_source(&patch)]).is_err()
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
        r"common\buildings",
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
        [0, 1, 1]
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

#[cfg(unix)]
#[test]
fn ambiguous_file_case_within_one_source_is_rejected() {
    let fixture = tempfile::tempdir().unwrap();
    write(fixture.path(), "example.txt", "one={}");
    write(fixture.path(), "Example.txt", "two={}");
    let error = ContentSet::resolve(vec![base(fixture.path())]).unwrap_err();
    assert!(error.to_string().contains("ambiguous content path"));
}
