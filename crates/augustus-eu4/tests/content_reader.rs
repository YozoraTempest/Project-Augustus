// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_eu4::{ContentRoot, ModDescriptor, TextDocument, TextEncoding};
use std::{fs, path::Path};

// All input below is synthetic test data authored for this project.

#[test]
fn descriptor_keeps_repeated_replace_paths_and_dependency_lists() {
    let document = TextDocument::from_bytes(
        "example.mod",
        br#"
        name="Example Pack"
        version="1.0"
        supported_version="1.37.*"
        path="mod/example"
        replace_path="common/buildings"
        replace_path="events"
        replace_path="events"
        dependencies={ "Example Base" "Example Library" }
        tags={ "Gameplay" }
        custom_metadata="retained in the input document"
    "#
        .to_vec(),
        TextEncoding::Utf8,
    );
    let parsed = document.parse().unwrap();
    let descriptor = ModDescriptor::parse(&parsed).unwrap();
    assert_eq!(descriptor.name, "Example Pack");
    assert_eq!(
        descriptor.replace_paths,
        ["common/buildings", "events", "events"]
    );
    assert_eq!(descriptor.dependencies, ["Example Base", "Example Library"]);
    assert_eq!(descriptor.other_fields, ["custom_metadata"]);
    assert_eq!(descriptor.path.as_deref(), Some("mod/example"));
}

#[test]
fn script_keeps_repeated_fields_and_comparison_operators() {
    let document = TextDocument::from_bytes(
        "sample.txt",
        b"rule={ value=1 } rule={ value=2 } amount>3".to_vec(),
        TextEncoding::Utf8,
    );
    let parsed = document.parse().unwrap();
    let fields = parsed.root_fields();
    assert_eq!(
        fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        ["rule", "rule", "amount"]
    );
    assert_eq!(fields[0].scalar, None);
    assert_eq!(fields[2].operator, ">");
    assert_eq!(fields[2].scalar.as_deref(), Some("3"));
}

#[test]
fn descriptor_rejects_wrong_field_shapes_and_duplicate_names() {
    for bytes in [
        &b"name={ nested=value }"[..],
        &b"name=first name=second"[..],
        &b"name=example dependencies={ nested=value }"[..],
        &b"name=example tags={ { nested=value } }"[..],
        &b"version=one"[..],
    ] {
        let document = TextDocument::from_bytes("bad.mod", bytes.to_vec(), TextEncoding::Utf8);
        let parsed = document.parse().unwrap();
        assert!(
            ModDescriptor::parse(&parsed).is_err(),
            "accepted invalid descriptor: {bytes:?}"
        );
    }
}

#[test]
fn encoding_is_explicit_and_utf8_does_not_replace_invalid_bytes() {
    let bytes = b"name=\"Caf\xe9\"".to_vec();
    let western = TextDocument::from_bytes("western.mod", bytes.clone(), TextEncoding::Windows1252);
    assert_eq!(
        ModDescriptor::parse(&western.parse().unwrap())
            .unwrap()
            .name,
        "Café"
    );
    let utf8 = TextDocument::from_bytes("utf8.mod", bytes, TextEncoding::Utf8);
    let error = utf8.parse().unwrap_err();
    assert_eq!(error.path(), Path::new("utf8.mod"));
    assert_eq!(error.location().unwrap().line, 1);
}

#[test]
fn utf8_bom_and_non_ascii_names_are_read_without_normalizing_input() {
    let bytes = "\u{feff}name=\"示例内容包\"\r\n".as_bytes().to_vec();
    let document = TextDocument::from_bytes("utf8.mod", bytes.clone(), TextEncoding::Utf8);
    let parsed = document.parse().unwrap();
    assert_eq!(ModDescriptor::parse(&parsed).unwrap().name, "示例内容包");
    assert_eq!(document.byte_len(), bytes.len());
}

#[test]
fn syntax_errors_report_source_and_line_instead_of_panicking() {
    let document = TextDocument::from_bytes(
        "broken.txt",
        b"rule={\n value=1\n}\nname=\"unterminated".to_vec(),
        TextEncoding::Utf8,
    );
    let error = document.parse().unwrap_err();
    assert_eq!(error.path(), Path::new("broken.txt"));
    assert_eq!(error.location().unwrap().line, 4);
    assert!(error.to_string().contains("broken.txt:4:"));
}

#[test]
fn unfinished_blocks_report_end_of_file() {
    let document =
        TextDocument::from_bytes("unfinished.txt", b"rule={\n".to_vec(), TextEncoding::Utf8);
    let error = document.parse().unwrap_err();
    assert_eq!(error.location().unwrap().line, 2);
}

#[test]
fn catalogue_and_read_leave_source_files_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("common")).unwrap();
    fs::write(directory.path().join("common/z.txt"), b"z=2").unwrap();
    fs::write(directory.path().join("common/a.txt"), b"a=1").unwrap();
    let root = ContentRoot::open(directory.path()).unwrap();
    let before = fs::read(directory.path().join("common/a.txt")).unwrap();
    let catalogue = root.files().unwrap();
    assert_eq!(catalogue.len(), 2);
    assert_eq!(catalogue[0].relative_path, Path::new("common/a.txt"));
    let document = root.read_text("common/a.txt", TextEncoding::Utf8).unwrap();
    assert_eq!(
        document.parse().unwrap().root_fields()[0].scalar.as_deref(),
        Some("1")
    );
    assert_eq!(
        fs::read(directory.path().join("common/a.txt")).unwrap(),
        before
    );
    assert_eq!(root.files().unwrap(), catalogue);
}

#[test]
fn relative_reads_reject_traversal_and_absolute_paths() {
    let directory = tempfile::tempdir().unwrap();
    let root = ContentRoot::open(directory.path()).unwrap();
    for path in [Path::new("../outside.txt"), directory.path()] {
        assert!(root.read_text(path, TextEncoding::Utf8).is_err());
    }
}

#[cfg(unix)]
#[test]
fn relative_reads_cannot_escape_through_symlinks() {
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("outside.txt"), b"outside=1").unwrap();
    std::os::unix::fs::symlink(outside.path(), directory.path().join("link")).unwrap();
    let root = ContentRoot::open(directory.path()).unwrap();
    assert!(
        root.read_text("link/outside.txt", TextEncoding::Utf8)
            .is_err()
    );
    assert!(root.files().is_err());
}
