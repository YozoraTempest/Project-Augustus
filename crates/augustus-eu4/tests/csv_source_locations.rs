// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{fs, path::Path, path::PathBuf};

use augustus_eu4::{SourceLocation, VanillaWorld};

const DEFINITION_RECORDS: [&str; 7] = [
    "province;red;green;blue;x;x",
    "1;240;10;10;North;x",
    "2;10;240;10;East;x",
    "3;10;10;240;Sea;x",
    "4;120;90;10;Highland;x",
    "5;240;10;240;Unused One;x",
    "6;240;10;240;Unused Two;x",
];

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

fn copy_fixture() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    for entry in walkdir::WalkDir::new(fixture()) {
        let entry = entry.unwrap();
        let target = temp
            .path()
            .join(entry.path().strip_prefix(fixture()).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(target).unwrap();
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
    temp
}

fn write_definition(root: &Path, endings: &[&str]) {
    assert!(
        endings.len() == DEFINITION_RECORDS.len() || endings.len() + 1 == DEFINITION_RECORDS.len()
    );
    let mut contents = String::new();
    for (index, record) in DEFINITION_RECORDS.iter().enumerate() {
        contents.push_str(record);
        if let Some(ending) = endings.get(index) {
            contents.push_str(ending);
        }
    }
    fs::write(root.join("map/definition.csv"), contents).unwrap();
}

fn province_source_line(world: &VanillaWorld, id: u32) -> usize {
    world.province_sources[&id][0].line
}

#[test]
fn definition_source_lines_follow_lf_crlf_bare_cr_and_mixed_terminators() {
    let mixed = vec!["\r\n", "\n", "\r", "\r\n", "\n", "\r", "\r\n"];
    let cases = vec![
        ("LF", vec!["\n"; DEFINITION_RECORDS.len()]),
        (
            "LF without final newline",
            vec!["\n"; DEFINITION_RECORDS.len() - 1],
        ),
        ("CRLF", vec!["\r\n"; DEFINITION_RECORDS.len()]),
        ("bare CR", vec!["\r"; DEFINITION_RECORDS.len()]),
        ("mixed", mixed),
    ];

    for (name, endings) in cases {
        let root = copy_fixture();
        write_definition(root.path(), &endings);
        let loaded = VanillaWorld::load(root.path(), "1444.11.11").unwrap();
        for (index, id) in (1..=6).enumerate() {
            assert_eq!(
                province_source_line(&loaded, id),
                index + 2,
                "{name} province {id}"
            );
        }
    }
}

#[test]
fn definition_source_lines_skip_blank_lines_and_preserve_quoted_crlf() {
    let root = copy_fixture();
    fs::write(
        root.path().join("map/definition.csv"),
        b"province;red;green;blue;x;x\r\n\
          1;240;10;10;North;x\r\n\
          \r\n\
          2;10;240;10;\"East\r\n\
          Coast\";x\r\n\
          \r\n\
          3;10;10;240;Sea;x\r\n\
          4;120;90;10;Highland;x\r\n\
          5;240;10;240;Unused One;x\r\n\
          6;240;10;240;Unused Two;x\r\n",
    )
    .unwrap();

    let loaded = VanillaWorld::load(root.path(), "1444.11.11").unwrap();
    assert_eq!(province_source_line(&loaded, 2), 4);
    assert_eq!(loaded.initial.province(2).unwrap().name, "East\r\nCoast");
    assert_eq!(province_source_line(&loaded, 3), 7);
}

#[test]
fn invalid_definition_numbers_report_the_physical_record_line_and_column() {
    for (bad_record, message) in [
        ("3;not-a-number;10;240;Sea;x", "invalid definition integer"),
        ("3;256;10;240;Sea;x", "map color out of range"),
    ] {
        let root = copy_fixture();
        let prefix = b"province;red;green;blue;x;x\r\n\
            1;240;10;10;North;x\r\n\
            \r\n\
            2;10;240;10;\"East\r\n\
            Coast\";x\r\n\
            \r\n";
        let mut contents = prefix.to_vec();
        contents.extend_from_slice(bad_record.as_bytes());
        contents.extend_from_slice(b"\r\n");
        fs::write(root.path().join("map/definition.csv"), contents).unwrap();

        let error = VanillaWorld::load(root.path(), "1444.11.11").unwrap_err();
        assert_eq!(
            error.location(),
            Some(SourceLocation { line: 7, column: 1 }),
            "{error}"
        );
        assert!(error.to_string().contains(message), "{error}");
    }
}

#[test]
fn adjacency_errors_report_physical_rows_after_multiline_records_and_blank_lines() {
    for (last_row, message) in [
        (
            "1;99;sea",
            "adjacency references an undefined province on adjacency row 5",
        ),
        (
            "0;1;sea",
            "province zero is not a valid adjacency endpoint on adjacency row 5",
        ),
    ] {
        let root = copy_fixture();
        fs::write(
            root.path().join("map/adjacencies.csv"),
            format!("From;To;Type;Note\r\n1;2;sea;\"connected\r\npassage\"\r\n\r\n{last_row}\r\n"),
        )
        .unwrap();

        let error = VanillaWorld::load(root.path(), "1444.11.11").unwrap_err();
        assert!(error.to_string().contains(message), "{error}");
    }
}

#[test]
fn definition_line_ending_changes_keep_snapshot_bytes_and_cross_restore_compatible() {
    let root = copy_fixture();
    write_definition(root.path(), &["\n"; DEFINITION_RECORDS.len()]);
    let lf = VanillaWorld::load(root.path(), "1444.11.11").unwrap();
    let lf_snapshot = lf.save().unwrap();

    write_definition(root.path(), &["\r\n"; DEFINITION_RECORDS.len()]);
    let mut crlf = VanillaWorld::load(root.path(), "1444.11.11").unwrap();
    for id in 1..=6 {
        assert_eq!(
            serde_json::to_value(lf.initial.province(id)).unwrap(),
            serde_json::to_value(crlf.initial.province(id)).unwrap()
        );
    }
    assert_eq!(lf_snapshot.as_bytes(), crlf.save().unwrap().as_bytes());
    crlf.restore(&lf_snapshot).unwrap();
}
