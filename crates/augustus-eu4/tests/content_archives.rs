// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_eu4::{ContentRoot, ContentSet, ContentSource, TextEncoding};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

fn write(root: &Path, relative: &str, bytes: impl AsRef<[u8]>) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn archive(path: &Path, entries: &[(&str, &str)]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let file = fs::File::create(path).unwrap();
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, contents) in entries {
        archive.start_file(name, options).unwrap();
        archive.write_all(contents.as_bytes()).unwrap();
    }
    archive.finish().unwrap();
}

fn base(path: &Path) -> ContentSource {
    ContentSource::base(ContentRoot::open(path).unwrap())
}

fn first_field(content: &ContentSet, path: &str) -> String {
    content
        .read_text(path, TextEncoding::Utf8)
        .unwrap()
        .parse()
        .unwrap()
        .root_fields()[0]
        .name
        .clone()
}

#[test]
fn archive_members_are_read_lazily_and_resolve_between_mods_and_base() {
    let fixture = tempfile::tempdir().unwrap();
    let vanilla = fixture.path().join("base");
    let dlc_zip = fixture.path().join("dlc/content.zip");
    let mod_root = fixture.path().join("mod");
    let user = fixture.path().join("user");
    write(&vanilla, "common/defines.lua", "from_base = true");
    archive(&dlc_zip, &[(r"common\defines.lua", "from_dlc = true")]);
    write(&mod_root, "descriptor.mod", "name=Pack");
    write(&mod_root, "common/defines.lua", "from_mod = true");
    write(&user, "common/defines.lua", "from_user = true");

    let dlc = ContentSource::dlc_archive(&dlc_zip, "DLC: Test Pack").unwrap();
    let content = ContentSet::resolve(vec![
        base(&vanilla),
        dlc,
        ContentSource::mod_directory(&mod_root, TextEncoding::Utf8).unwrap(),
        ContentSource::user_directory(&user).unwrap(),
    ])
    .unwrap();

    assert_eq!(
        content
            .sources()
            .iter()
            .map(ContentSource::name)
            .collect::<Vec<_>>(),
        ["EU4 user directory", "Pack", "DLC: Test Pack", "EU4 base"]
    );
    assert_eq!(first_field(&content, "COMMON/defines.lua"), "from_user");
    assert!(!fixture.path().join("dlc/common/defines.lua").exists());
    let trace = content.trace("common/defines.lua").unwrap().unwrap();
    assert_eq!(trace.candidates.len(), 4);
    assert_eq!(
        trace.candidates[2].origin.physical_path,
        fs::canonicalize(&dlc_zip)
            .unwrap()
            .join("common/defines.lua")
    );
    assert_eq!(
        trace.candidates[2].origin.relative_path,
        PathBuf::from("common/defines.lua")
    );
}

#[test]
fn installed_dlc_discovery_reads_only_present_archive_files() {
    let fixture = tempfile::tempdir().unwrap();
    let game = fixture.path().join("game");
    let dlc_directory = game.join("dlc/dlc100_sample");
    let builtin_directory = game.join("builtin_dlc/dlc101_builtin");
    fs::create_dir_all(&dlc_directory).unwrap();
    fs::create_dir_all(&builtin_directory).unwrap();
    write(
        &dlc_directory,
        "dlc100.dlc",
        "name=Zeta archive=\"dlc/dlc100_sample/dlc100.zip\"",
    );
    write(
        &builtin_directory,
        "dlc101.dlc",
        "name=Alpha archive=\"builtin_dlc/dlc101_builtin/dlc101.zip\"",
    );
    archive(
        &game.join("dlc/dlc100_sample/dlc100.zip"),
        &[("common/defines.lua", "from_sample = true")],
    );
    archive(
        &game.join("builtin_dlc/dlc101_builtin/dlc101.zip"),
        &[
            ("common/defines.lua", "from_alpha = true"),
            ("common/builtin.lua", "from_builtin = true"),
        ],
    );
    write(
        &game.join("dlc/dlc102_missing"),
        "dlc102.dlc",
        "name=Missing archive=\"dlc/dlc102_missing/dlc102.zip\"",
    );
    write(&game, "common/defines.lua", "from_base = true");

    let dlcs = ContentSource::installed_dlc_archives(&game).unwrap();
    assert_eq!(dlcs.len(), 2);
    assert_eq!(
        dlcs.iter().map(ContentSource::name).collect::<Vec<_>>(),
        ["DLC: Zeta", "DLC: Alpha"]
    );
    let content = ContentSet::resolve(std::iter::once(base(&game)).chain(dlcs).collect()).unwrap();
    assert_eq!(first_field(&content, "common/defines.lua"), "from_sample");
    assert_eq!(first_field(&content, "common/builtin.lua"), "from_builtin");
}

#[test]
fn installed_dlc_metadata_cannot_mount_an_archive_outside_the_game_root() {
    let fixture = tempfile::tempdir().unwrap();
    let game = fixture.path().join("game");
    let dlc_directory = game.join("dlc/dlc100_escape");
    fs::create_dir_all(&dlc_directory).unwrap();
    archive(
        &fixture.path().join("outside.zip"),
        &[("file.txt", "outside")],
    );
    write(
        &dlc_directory,
        "dlc100.dlc",
        "name=Escape archive=\"../outside.zip\"",
    );

    let error = ContentSource::installed_dlc_archives(&game).unwrap_err();
    assert!(error.to_string().contains("without traversal"));
}

#[test]
fn archive_member_paths_cannot_escape_the_virtual_root() {
    let fixture = tempfile::tempdir().unwrap();
    let zip_path = fixture.path().join("unsafe.zip");
    archive(&zip_path, &[("../outside.txt", "outside")]);

    let error = ContentSource::dlc_archive(&zip_path, "DLC: Unsafe").unwrap_err();
    assert!(error.to_string().contains("without traversal"));
}
