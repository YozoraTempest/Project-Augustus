// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{ffi::OsStr, io::Write, path::Path};

pub(crate) fn write_snapshot_to_file(path: &str, snapshot: &str) -> Result<(), String> {
    if path.is_empty() || path.contains('\0') {
        return Err("invalid snapshot file path".into());
    }

    let target = Path::new(path);
    let Some(file_name) = target.file_name() else {
        return Err("snapshot path must name a file".into());
    };
    if file_name.is_empty() || file_name == OsStr::new(".") || file_name == OsStr::new("..") {
        return Err("snapshot path must name a file".into());
    }

    let parent = target
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()
        .map_err(|error| format!("could not resolve snapshot directory: {error}"))?;
    // Keep both rename paths absolute, including Windows extended-length syntax.
    let target = parent.join(file_name);
    let mut temporary = tempfile::NamedTempFile::new_in(&parent)
        .map_err(|error| format!("could not create snapshot temporary file: {error}"))?;
    temporary
        .write_all(snapshot.as_bytes())
        .map_err(|error| format!("could not write snapshot temporary file: {error}"))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| format!("could not sync snapshot temporary file: {error}"))?;
    temporary.persist(target).map_err(|error| {
        format!(
            "could not atomically replace snapshot file: {}",
            error.error
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{fs, fs::File, io::Read};

    use super::write_snapshot_to_file;

    #[test]
    fn writes_and_replaces_exact_opaque_utf8_snapshot_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("世界.snapshot");
        let first = r#"{"counter":18446744073709551615,"text":"世界\\n"}"#;
        let second = r#"{"counter":18446744073709551614,"text":"保存済み"}"#;
        let path_text = path.to_str().unwrap();

        write_snapshot_to_file(path_text, first).unwrap();
        assert_eq!(fs::read(&path).unwrap(), first.as_bytes());

        write_snapshot_to_file(path_text, second).unwrap();
        assert_eq!(fs::read(path).unwrap(), second.as_bytes());
    }

    #[cfg(windows)]
    #[test]
    fn saves_and_replaces_windows_paths_longer_than_260_characters() {
        let directory = tempfile::tempdir().unwrap();
        let parent = directory
            .path()
            .join("campaign-snapshots-".repeat(8))
            .join("province-history-".repeat(8));
        fs::create_dir_all(&parent).unwrap();
        let path = parent.join("世界.snapshot.json");
        let path_text = path.to_str().unwrap().replace('\\', "/");
        assert!(path_text.chars().count() > 260);

        write_snapshot_to_file(&path_text, "first snapshot").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"first snapshot");
        write_snapshot_to_file(&path_text, "replacement snapshot").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"replacement snapshot");
        assert_eq!(fs::read_dir(parent).unwrap().count(), 1);
    }

    #[test]
    fn held_reader_preserves_complete_old_snapshot_when_target_is_replaced() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snapshot.json");
        let original = b"old snapshot bytes";
        fs::write(&path, original).unwrap();
        let mut open_read_handle = File::open(&path).unwrap();

        let result = write_snapshot_to_file(path.to_str().unwrap(), "new snapshot bytes");
        let mut reader_contents = Vec::new();
        open_read_handle.read_to_end(&mut reader_contents).unwrap();
        assert_eq!(reader_contents, original);

        if result.is_err() {
            assert_eq!(fs::read(path).unwrap(), original);
        } else {
            assert_eq!(fs::read(path).unwrap(), b"new snapshot bytes");
        }
    }

    #[test]
    fn invalid_paths_and_directory_targets_leave_no_temporary_files() {
        let directory = tempfile::tempdir().unwrap();
        assert!(write_snapshot_to_file("", "snapshot").is_err());
        assert!(write_snapshot_to_file(".", "snapshot").is_err());
        let invalid_parent = directory.path().join("missing").join("snapshot.json");
        assert!(write_snapshot_to_file(invalid_parent.to_str().unwrap(), "snapshot").is_err());
        assert!(!directory.path().join("missing").exists());

        let nul_path = format!("{}\0snapshot.json", directory.path().display());
        assert!(write_snapshot_to_file(&nul_path, "snapshot").is_err());

        let target_directory = directory.path().join("target-directory");
        fs::create_dir(&target_directory).unwrap();
        let entries_before = fs::read_dir(directory.path()).unwrap().count();
        assert!(write_snapshot_to_file(target_directory.to_str().unwrap(), "snapshot").is_err());
        assert_eq!(
            fs::read_dir(directory.path()).unwrap().count(),
            entries_before
        );
        assert!(target_directory.is_dir());
    }

    #[cfg(windows)]
    #[test]
    fn readonly_destination_is_preserved_when_atomic_replace_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("readonly.snapshot");
        let original = b"original readonly bytes";
        fs::write(&path, original).unwrap();
        let original_permissions = fs::metadata(&path).unwrap().permissions();
        let mut permissions = original_permissions.clone();
        permissions.set_readonly(true);
        fs::set_permissions(&path, permissions).unwrap();

        let result = write_snapshot_to_file(path.to_str().unwrap(), "replacement");
        let contents = fs::read(&path).unwrap();
        fs::set_permissions(&path, original_permissions).unwrap();

        assert!(result.is_err());
        assert_eq!(contents, original);
    }
}
