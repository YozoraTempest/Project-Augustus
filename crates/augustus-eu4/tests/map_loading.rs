// SPDX-License-Identifier: GPL-3.0-only

use std::{fs, path::PathBuf};

use augustus_eu4::{Error, map::ProvinceMap};
use image::{ImageFormat, Rgb, RgbImage};
use tempfile::TempDir;

const CONFIG: &str = r#"
# 使用自制小地图和非默认文件名。
width = 3
height = 2
max_provinces = 12
definitions = "custom/colors.csv"
provinces = "custom/pixels.data"
sea_starts = { 7 }
irrelevant = { nested = { value = 1 } }
"#;

// 附加名称列包含非 UTF-8 字符和带引号的分号；9 与 11 共用未使用的颜色。
const DEFINITIONS: &[u8] = b"province;red;green;blue;x;x\n\
0;0;0;0;Reserved;x\n\
1;11;22;33;\"Caf\xe9; north\";x\n\
7;44;55;66;South;x\n\
9;77;88;99;Unused;x\n\
11;77;88;99;Unused duplicate;x\n\
12;200;200;200;Outside configured range;x\n";

struct TestMap {
    _directory: TempDir,
    root: PathBuf,
}

impl TestMap {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("自制地图");
        fs::create_dir_all(root.join("map/custom")).unwrap();
        fs::write(root.join("map/default.map"), CONFIG).unwrap();
        fs::write(root.join("map/custom/colors.csv"), DEFINITIONS).unwrap();
        let colors = [
            [11, 22, 33],
            [44, 55, 66],
            [0, 0, 0],
            [44, 55, 66],
            [11, 22, 33],
            [11, 22, 33],
        ];
        // 三像素宽度会产生 BMP 行填充，两行使用不同分布以检查上下方向。
        let bitmap = RgbImage::from_fn(3, 2, |x, y| Rgb(colors[(y * 3 + x) as usize]));
        bitmap
            .save_with_format(root.join("map/custom/pixels.data"), ImageFormat::Bmp)
            .unwrap();
        Self {
            _directory: directory,
            root,
        }
    }

    fn config(&self, config: &str) {
        fs::write(self.root.join("map/default.map"), config).unwrap();
    }

    fn definitions(&self, data: &[u8]) {
        fs::write(self.root.join("map/custom/colors.csv"), data).unwrap();
    }

    fn load(&self) -> Result<ProvinceMap, Error> {
        ProvinceMap::load(&self.root)
    }
}

#[test]
fn loads_configured_files_and_maps_pixels_from_the_top_left() {
    let input = TestMap::new();
    let map = input.load().unwrap();
    let summary = map.summary();
    assert_eq!((summary.width, summary.height), (3, 2));
    assert_eq!(summary.defined_provinces, 4);
    assert_eq!(summary.mapped_provinces, 2);
    assert_eq!(summary.unassigned_pixels, 1);
    for (x, y, expected) in [
        (0, 0, Some(1)),
        (1, 0, Some(7)),
        (2, 0, None),
        (0, 1, Some(7)),
        (1, 1, Some(1)),
        (2, 1, Some(1)),
    ] {
        assert_eq!(map.province_at(x, y).unwrap(), expected);
    }
}

#[test]
fn distinguishes_out_of_bounds_coordinates_from_unassigned_pixels() {
    let map = TestMap::new().load().unwrap();
    for (x, y) in [(3, 0), (0, 2), (u32::MAX, u32::MAX)] {
        assert!(matches!(
            map.province_at(x, y),
            Err(Error::OutOfBounds {
                width: 3,
                height: 2,
                ..
            })
        ));
    }
    assert_eq!(map.province_at(2, 0).unwrap(), None);
}

#[test]
fn rejects_ambiguous_colors_when_the_bitmap_uses_them() {
    let input = TestMap::new();
    input.definitions(b"province;red;green;blue\n1;11;22;33\n7;11;22;33\n");
    let error = input.load().unwrap_err().to_string();
    assert!(error.contains("colors.csv"), "{error}");
    assert!(
        error.contains("multiple provinces use map color"),
        "{error}"
    );
}

#[test]
fn rejects_duplicate_ids_and_invalid_definition_rows() {
    let input = TestMap::new();
    for (data, expected) in [
        (
            "province;red;green;blue\n1;11;22;33\n1;44;55;66\n",
            "duplicate province ID: 1",
        ),
        ("province;red;green;blue\n1;256;22;33\n", "RGB component"),
        ("province;red;green;blue\n1;11\n", "RGB component"),
        ("province;red;green;blue\ninvalid;11;22;33\n", "province ID"),
    ] {
        input.definitions(data.as_bytes());
        let error = input.load().unwrap_err().to_string();
        assert!(error.contains("colors.csv"), "{error}");
        assert!(
            error.contains("line 2") || error.contains("line 3"),
            "{error}"
        );
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn rejects_invalid_or_ambiguous_map_configuration() {
    let input = TestMap::new();
    for (config, expected) in [
        (CONFIG.replace("width = 3", ""), "missing map field: width"),
        (format!("{CONFIG}\nwidth = 3"), "duplicate map field: width"),
        (CONFIG.replace("width = 3", "width = 0"), "positive integer"),
        (CONFIG.replace("width = 3", "width = { 3 }"), "width"),
        (CONFIG.replace("width = 3", "width > 3"), "expected '='"),
        (
            CONFIG.replace("max_provinces = 12", "max_provinces = 1"),
            "max_provinces",
        ),
        (format!("{CONFIG}\nbroken = {{"), ""),
    ] {
        input.config(&config);
        let error = input.load().unwrap_err().to_string();
        assert!(error.contains("default.map"), "{error}");
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn reports_dimension_mismatch_and_invalid_bitmap_with_source_path() {
    let input = TestMap::new();
    input.config(&CONFIG.replace("width = 3", "width = 4"));
    let error = input.load().unwrap_err().to_string();
    assert!(error.contains("pixels.data"), "{error}");
    assert!(error.contains("3 x 2"), "{error}");
    assert!(error.contains("4 x 2"), "{error}");

    input.config(CONFIG);
    fs::write(input.root.join("map/custom/pixels.data"), b"not a bitmap").unwrap();
    let error = input.load().unwrap_err().to_string();
    assert!(error.contains("pixels.data"), "{error}");
}

#[test]
fn reports_missing_content_and_rejects_paths_outside_the_content_root() {
    let input = TestMap::new();
    input.config(&CONFIG.replace("custom/colors.csv", "custom/missing.csv"));
    assert!(matches!(input.load(), Err(Error::Read { .. })));

    input.config(&CONFIG.replace("custom/colors.csv", "../../outside.csv"));
    let error = input.load().unwrap_err().to_string();
    assert!(error.contains("relative content path"), "{error}");
}

#[test]
fn accepts_backslash_separators_in_configured_relative_paths() {
    let input = TestMap::new();
    for separator in ["custom\\", "custom\\\\"] {
        input.config(&CONFIG.replace("custom/", separator));
        assert_eq!(input.load().unwrap().province_at(0, 1).unwrap(), Some(7));
    }
}
