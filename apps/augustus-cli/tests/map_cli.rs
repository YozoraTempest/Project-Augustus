// SPDX-License-Identifier: GPL-3.0-only

use std::{fs, path::Path, process::Command};

use image::{ImageFormat, Rgb, RgbImage};

fn game_directory() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let map = directory.path().join("map");
    fs::create_dir(&map).unwrap();
    fs::write(
        map.join("default.map"),
        "width=2 height=1 max_provinces=8 definitions=\"definition.csv\" provinces=\"provinces.bmp\"",
    )
    .unwrap();
    fs::write(
        map.join("definition.csv"),
        "province;red;green;blue\n7;1;2;3\n",
    )
    .unwrap();
    // 左侧像素属于省份 7，右侧像素没有对应定义，用于区分两种查询结果。
    RgbImage::from_fn(2, 1, |x, _| {
        if x == 0 {
            Rgb([1, 2, 3])
        } else {
            Rgb([0, 0, 0])
        }
    })
    .save_with_format(map.join("provinces.bmp"), ImageFormat::Bmp)
    .unwrap();
    directory
}

fn command(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_augustus"));
    command.args(["map", "--game-dir"]).arg(root);
    command
}

#[test]
fn displays_loaded_map_summary_and_coordinate_results() {
    let game = game_directory();
    for (point, expected) in [(["0", "0"], "Province: 7"), (["1", "0"], "Province: none")] {
        let output = command(game.path())
            .arg("--at")
            .args(point)
            .output()
            .unwrap();
        assert!(output.status.success(), "{:?}", output);
        assert!(output.stderr.is_empty());
        let stdout = String::from_utf8(output.stdout).unwrap();
        for expected in [
            "Map size: 2 x 1",
            "Defined provinces: 1",
            "Mapped provinces: 1",
            "Unassigned pixels: 1",
            expected,
        ] {
            assert!(stdout.contains(expected), "{stdout}");
        }
    }
}

#[test]
fn returns_runtime_errors_without_partial_summary_output() {
    let game = game_directory();
    let output = command(game.path())
        .args(["--at", "2", "0"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("outside")
    );

    let output = command(&game.path().join("missing")).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("default.map")
    );
}

#[test]
fn rejects_incomplete_or_invalid_coordinate_arguments() {
    let game = game_directory();
    for arguments in [
        vec!["--at", "0"],
        vec!["--at", "abc", "1"],
        vec!["--at", "-1", "0"],
    ] {
        let output = command(game.path()).args(arguments).output().unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
    }
}
