// SPDX-License-Identifier: GPL-3.0-only

use std::{fs, path::PathBuf};

use augustus_core::ProvinceKind;
use augustus_eu4::{Error, world::OpeningWorld};
use image::{ImageFormat, Rgb, RgbImage};
use tempfile::TempDir;

struct TestWorld {
    _directory: TempDir,
    root: PathBuf,
}

impl TestWorld {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("开局");
        for path in [
            "map",
            "common/country_tags",
            "common/countries",
            "history/provinces",
        ] {
            fs::create_dir_all(root.join(path)).unwrap();
        }
        fs::write(
            root.join("map/default.map"),
            r#"
width = 6 height = 1 max_provinces = 8
definitions = "definition.csv" provinces = "provinces.bmp"
sea_starts = { 3 } lakes = { 4 } climate = "custom-climate.txt"
"#,
        )
        .unwrap();
        fs::write(root.join("map/custom-climate.txt"), "impassable = { 5 }").unwrap();
        let mut definitions = String::from("province;red;green;blue\n");
        for id in 1..=7 {
            definitions.push_str(&format!("{id};{id};0;0\n"));
        }
        fs::write(root.join("map/definition.csv"), definitions).unwrap();
        // 1、2 是国家领土，3、4、5 是不同地理类型，6 是未殖民地，7 未绘制。
        RgbImage::from_fn(6, 1, |x, _| Rgb([x as u8 + 1, 0, 0]))
            .save_with_format(root.join("map/provinces.bmp"), ImageFormat::Bmp)
            .unwrap();
        fs::write(root.join("common/country_tags/00.txt"), "AAA = \"countries/First.txt\"\nBBB = \"countries/Second.txt\"\nCCC = \"countries/Unused.txt\"").unwrap();
        for (name, color) in [
            ("First", "10 30 200"),
            ("Second", "200 40 20"),
            ("Unused", "0 0 0"),
        ] {
            fs::write(
                root.join(format!("common/countries/{name}.txt")),
                format!("color = {{ {color} }}\ngraphical_culture = western_gfx"),
            )
            .unwrap();
        }
        fs::write(
            root.join("history/provinces/1-First.txt"),
            r#"
owner = BBB
1444.11.11 = { owner = AAA controller = BBB }
1300.1.1 = { owner = BBB }
1444.11.12 = { owner = CCC }
"#,
        )
        .unwrap();
        fs::write(
            root.join("history/provinces/2 - Second.txt"),
            "owner = BBB\ncontroller = AAA",
        )
        .unwrap();
        fs::write(
            root.join("history/provinces/6-Uncolonized.txt"),
            "owner = AAA\n1444.11.11 = { owner = --- }",
        )
        .unwrap();
        fs::write(root.join("history/provinces/7-Unused.txt"), "owner = CCC").unwrap();
        Self {
            _directory: directory,
            root,
        }
    }

    fn write(&self, path: &str, contents: &str) {
        fs::write(self.root.join(path), contents).unwrap();
    }

    fn load(&self) -> Result<OpeningWorld, Error> {
        OpeningWorld::load(&self.root)
    }
}

#[test]
fn loads_opening_ownership_and_political_colors() {
    let input = TestWorld::new();
    let world = input.load().unwrap();
    assert_eq!(world.state().countries().len(), 3);
    let country = world.state().country("AAA").unwrap();
    assert_eq!(country.tag(), "AAA");
    assert!(std::ptr::eq(
        country,
        world.state().province_owner(1).unwrap()
    ));
    assert_eq!(world.state().province_owner(2).unwrap().tag(), "BBB");
    // 已注册的国家即使没有地图上的领土，仍保留可查询的实体。
    assert_eq!(world.state().country("CCC").unwrap().tag(), "CCC");
    assert!(world.state().country("ZZZ").is_none());
    assert!(world.state().province_owner(6).is_none());
    assert_eq!(world.landed_country_count(), 2);
    assert_eq!(
        world.state().province(1).unwrap().owner.as_deref(),
        Some("AAA")
    );
    assert_eq!(
        world.state().province(2).unwrap().owner.as_deref(),
        Some("BBB")
    );
    assert_eq!(world.state().province(6).unwrap().owner, None);
    assert_eq!(world.province_color(1), [10, 30, 200]);
    assert_eq!(world.province_color(2), [200, 40, 20]);
    for (id, kind) in [
        (3, ProvinceKind::Sea),
        (4, ProvinceKind::Lake),
        (5, ProvinceKind::Wasteland),
        (6, ProvinceKind::Land),
    ] {
        let province = world.state().province(id).unwrap();
        assert_eq!(province.kind, kind);
        assert!(province.owner.is_none());
        assert_ne!(world.province_color(id), world.province_color(1));
    }
    assert_ne!(world.province_color(3), world.province_color(6));
    assert_ne!(world.province_color(5), world.province_color(6));
    assert_ne!(world.province_color(0), world.province_color(6));
}

#[test]
fn applies_initial_values_then_dated_blocks_in_stable_chronological_order() {
    let input = TestWorld::new();
    input.write(
        "history/provinces/1-First.txt",
        r#"
1444.11.11 = { owner = BBB }
1200.1.1 = { owner = CCC }
1444.11.11 = { owner = AAA }
owner = CCC
1444.11.12 = { owner = ZZZ }
"#,
    );
    let world = input.load().unwrap();
    assert_eq!(
        world.state().province(1).unwrap().owner.as_deref(),
        Some("AAA")
    );
}

#[test]
fn rejects_unknown_owners_duplicate_histories_and_missing_land_histories() {
    let input = TestWorld::new();
    input.write("history/provinces/1-First.txt", "owner = ZZZ");
    let error = input.load().unwrap_err().to_string();
    assert!(
        error.contains("1-First.txt") && error.contains("unknown province owner: ZZZ"),
        "{error}"
    );
    input.write("history/provinces/1-First.txt", "owner = AAA");
    input.write("history/provinces/1-Duplicate.txt", "owner = BBB");
    let error = input.load().unwrap_err().to_string();
    assert!(error.contains("duplicate province history: 1"), "{error}");
    fs::remove_file(input.root.join("history/provinces/1-Duplicate.txt")).unwrap();
    fs::remove_file(input.root.join("history/provinces/1-First.txt")).unwrap();
    let error = input.load().unwrap_err().to_string();
    assert!(
        error.contains("mapped land province 1 has no history"),
        "{error}"
    );
}

#[test]
fn rejects_invalid_dates_and_unknown_history_provinces() {
    let input = TestWorld::new();
    input.write(
        "history/provinces/1-First.txt",
        "1444.13.1 = { owner = AAA }",
    );
    let error = input.load().unwrap_err().to_string();
    assert!(
        error.contains("1-First.txt") && error.contains("invalid history date"),
        "{error}"
    );
    input.write("history/provinces/1-First.txt", "owner = AAA");
    input.write("history/provinces/8-Unknown.txt", "owner = AAA");
    let error = input.load().unwrap_err().to_string();
    assert!(
        error.contains("8-Unknown.txt") && error.contains("unknown province 8"),
        "{error}"
    );
}

#[test]
fn rejects_invalid_country_definitions_and_content_paths() {
    let input = TestWorld::new();
    for (contents, expected) in [
        ("color = { 1 2 }", "three components"),
        ("color = { 1 2 256 }", "0..255"),
        ("other = yes", "missing country color"),
    ] {
        input.write("common/countries/First.txt", contents);
        let error = input.load().unwrap_err().to_string();
        assert!(
            error.contains("First.txt") && error.contains(expected),
            "{error}"
        );
    }
    input.write("common/countries/First.txt", "color = { 1 2 3 }");
    input.write(
        "common/country_tags/01.txt",
        "AAA = \"countries/First.txt\"",
    );
    assert!(
        input
            .load()
            .unwrap_err()
            .to_string()
            .contains("duplicate country tag: AAA")
    );
    fs::remove_file(input.root.join("common/country_tags/01.txt")).unwrap();
    input.write(
        "common/country_tags/00.txt",
        "AAA = \"countries/../../outside.txt\"",
    );
    assert!(
        input
            .load()
            .unwrap_err()
            .to_string()
            .contains("relative content path")
    );
}

#[test]
fn water_classification_takes_precedence_over_impassable_and_cannot_be_owned() {
    let input = TestWorld::new();
    input.write("map/custom-climate.txt", "impassable = { 3 4 5 }");
    let world = input.load().unwrap();
    assert_eq!(world.state().province(3).unwrap().kind, ProvinceKind::Sea);
    assert_eq!(world.state().province(4).unwrap().kind, ProvinceKind::Lake);
    input.write("map/custom-climate.txt", "impassable = { 5 }");
    input.write("history/provinces/3-Sea.txt", "owner = AAA");
    assert!(
        input
            .load()
            .unwrap_err()
            .to_string()
            .contains("non-land province 3")
    );
}
