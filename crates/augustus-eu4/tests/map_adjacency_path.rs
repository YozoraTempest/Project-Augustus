// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::EntityId;
use augustus_eu4::{ContentRoot, ContentSet, ContentSource, TextEncoding, VanillaWorld};
use image::RgbImage;
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

const DATE: &str = "1444.11.11";
const SPECIAL_EDGES: &str = concat!(
    "From;To;Type;Through;start_x;start_y;stop_x;stop_y;comment\n",
    "1;2;canal;-1;-1;-1;-1;-1;self-authored land link\n",
);
const INVALID_DEFAULT: &str = concat!("From;To;Type;Through\n", "1;999;sea;-1\n",);

fn examples_world() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

fn fixture() -> tempfile::TempDir {
    let source = examples_world();
    let temp = tempfile::tempdir().unwrap();
    for entry in walkdir::WalkDir::new(&source) {
        let entry = entry.unwrap();
        let target = temp
            .path()
            .join(entry.path().strip_prefix(&source).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(target).unwrap();
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
    write_isolated_map(temp.path());
    append(
        temp.path(),
        "history/provinces/1 - North.txt",
        "1444.11.11 = { owner = AAA controller = AAA }",
    );
    write(
        temp.path(),
        "history/provinces/4 - Highland.txt",
        "owner = BBB\ncontroller = BBB\n",
    );
    temp
}

fn write_isolated_map(root: &Path) {
    let colors = [
        [0, 0, 0],
        [240, 10, 10],
        [10, 240, 10],
        [10, 10, 240],
        [120, 90, 10],
    ];
    let layout = [1_usize, 4, 2, 4, 4, 4, 4, 3];
    let pixels = layout
        .into_iter()
        .flat_map(|province| colors[province])
        .collect::<Vec<_>>();
    let bitmap = RgbImage::from_raw(4, 2, pixels).unwrap();
    bitmap.save(root.join("map/provinces.bmp")).unwrap();
}

fn write(root: &Path, relative_path: &str, contents: impl AsRef<[u8]>) {
    let path = root.join(relative_path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn append(root: &Path, relative_path: &str, text: &str) {
    let path = root.join(relative_path);
    let previous = fs::read_to_string(&path).unwrap();
    fs::write(path, format!("{previous}\n{text}\n")).unwrap();
}

fn write_map_file(root: &Path, relative_path: &str, text: &str) {
    write(root, &format!("map/{relative_path}"), text);
}

fn set_adjacency_path(root: &Path, relative_path: &str) {
    let path = root.join("map/default.map");
    let previous = fs::read_to_string(&path).unwrap();
    fs::write(
        path,
        format!("{previous}\nadjacencies = \"{relative_path}\"\n"),
    )
    .unwrap();
}

fn load(root: &Path) -> VanillaWorld {
    VanillaWorld::load(root, DATE).unwrap()
}

fn owner_area(world: &VanillaWorld, province: u32) -> BTreeSet<EntityId> {
    let start = world.initial.province_entity(province).unwrap();
    world
        .initial
        .world()
        .province_owner_area(start)
        .unwrap()
        .expect("fixture provinces have known owners and complete adjacency")
        .into_iter()
        .collect()
}

fn assert_owner_area(world: &VanillaWorld, province: u32, expected: &[u32]) {
    let expected = expected
        .iter()
        .map(|province| world.initial.province_entity(*province).unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(owner_area(world, province), expected);
}

fn error_path(error: &augustus_eu4::Error) -> String {
    error.path().to_string_lossy().replace('\\', "/")
}

fn write_archive(path: &Path, member: &str, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let file = fs::File::create(path).unwrap();
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    archive.start_file(member, options).unwrap();
    archive.write_all(contents.as_bytes()).unwrap();
    archive.finish().unwrap();
}

#[test]
fn nested_configured_path_is_used_without_reading_the_legacy_file() {
    let root = fixture();
    let world_without_special_edges = load(root.path());
    assert_owner_area(&world_without_special_edges, 1, &[1]);

    set_adjacency_path(root.path(), "routes/custom/links.csv");
    write_map_file(root.path(), "routes/custom/links.csv", SPECIAL_EDGES);
    write_map_file(root.path(), "adjacencies.csv", INVALID_DEFAULT);

    let configured = load(root.path());
    assert_owner_area(&configured, 1, &[1, 2]);
}

#[test]
fn omitted_and_explicit_default_paths_have_identical_snapshots_and_cross_restore() {
    let omitted_root = fixture();
    write_map_file(omitted_root.path(), "adjacencies.csv", SPECIAL_EDGES);
    let mut omitted = load(omitted_root.path());
    let omitted_snapshot = omitted.save().unwrap();

    let explicit_root = fixture();
    write_map_file(explicit_root.path(), "adjacencies.csv", SPECIAL_EDGES);
    set_adjacency_path(explicit_root.path(), "adjacencies.csv");
    let mut explicit = load(explicit_root.path());
    let explicit_snapshot = explicit.save().unwrap();

    assert_eq!(omitted_snapshot.as_bytes(), explicit_snapshot.as_bytes());
    assert_owner_area(&omitted, 1, &[1, 2]);
    assert_owner_area(&explicit, 1, &[1, 2]);

    explicit.restore(&omitted_snapshot).unwrap();
    omitted.restore(&explicit_snapshot).unwrap();
    assert_eq!(
        explicit.save().unwrap().as_bytes(),
        omitted_snapshot.as_bytes()
    );
    assert_eq!(
        omitted.save().unwrap().as_bytes(),
        explicit_snapshot.as_bytes()
    );
}

#[test]
fn missing_configured_path_errors_instead_of_falling_back_to_default() {
    let root = fixture();
    write_map_file(root.path(), "adjacencies.csv", SPECIAL_EDGES);
    set_adjacency_path(root.path(), "routes/missing.csv");

    let error = VanillaWorld::load(root.path(), DATE).unwrap_err();
    assert!(error_path(&error) == "map/routes/missing.csv", "{error}");
}

#[test]
fn unsafe_relative_path_and_malformed_csv_keep_their_configured_source() {
    let traversal = fixture();
    write_map_file(traversal.path(), "adjacencies.csv", SPECIAL_EDGES);
    set_adjacency_path(traversal.path(), "../adjacencies.csv");
    let error = VanillaWorld::load(traversal.path(), DATE).unwrap_err();
    assert!(error_path(&error) == "map/../adjacencies.csv", "{error}");
    assert!(error.to_string().contains("without traversal"), "{error}");

    let malformed = fixture();
    write_map_file(malformed.path(), "adjacencies.csv", SPECIAL_EDGES);
    set_adjacency_path(malformed.path(), "routes/broken.csv");
    write_map_file(malformed.path(), "routes/broken.csv", "From;Type\n1;sea\n");
    let error = VanillaWorld::load(malformed.path(), DATE).unwrap_err();
    assert!(
        error_path(&error).ends_with("/map/routes/broken.csv"),
        "{error}"
    );
    assert!(
        error.to_string().contains("missing the To column"),
        "{error}"
    );
}

#[test]
fn configured_path_resolves_from_mod_directory_and_archive_content_views() {
    let directory_base = fixture();
    set_adjacency_path(directory_base.path(), "routes/links.csv");
    let mod_root = tempfile::tempdir().unwrap();
    write(
        mod_root.path(),
        "descriptor.mod",
        "name = \"Synthetic Map Overlay\"\n",
    );
    write(mod_root.path(), "map/routes/links.csv", SPECIAL_EDGES);
    let directory_content = ContentSet::resolve(vec![
        ContentSource::base(ContentRoot::open(directory_base.path()).unwrap()),
        ContentSource::mod_directory(mod_root.path(), TextEncoding::Utf8).unwrap(),
    ])
    .unwrap();
    let directory_world = VanillaWorld::load_from_content(&directory_content, DATE).unwrap();
    assert_owner_area(&directory_world, 1, &[1, 2]);

    let archive_base = fixture();
    set_adjacency_path(archive_base.path(), "routes/links.csv");
    let archive_dir = tempfile::tempdir().unwrap();
    let archive_path = archive_dir.path().join("synthetic-map.zip");
    write_archive(&archive_path, "map/routes/links.csv", SPECIAL_EDGES);
    let archive_content = ContentSet::resolve(vec![
        ContentSource::base(ContentRoot::open(archive_base.path()).unwrap()),
        ContentSource::dlc_archive(&archive_path, "Synthetic Map Archive").unwrap(),
    ])
    .unwrap();
    let archive_world = VanillaWorld::load_from_content(&archive_content, DATE).unwrap();
    assert_owner_area(&archive_world, 1, &[1, 2]);
}
