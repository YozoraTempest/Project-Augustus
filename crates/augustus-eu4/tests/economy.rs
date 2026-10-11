// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::World;
use augustus_eu4::VanillaWorld;
use std::{
    fs,
    path::{Path, PathBuf},
};

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

#[test]
fn buildings_goods_and_routes_reach_the_core_with_history_and_sources() {
    let loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let economy = loaded.initial.world().economy();
    assert_eq!(economy.buildings["test_market"].cost_milli, Some(87125));
    assert_eq!(
        economy.buildings["test_market"].construction_months,
        Some(9)
    );
    assert_eq!(economy.trade_goods["gold"].base_price_milli, Some(0));
    assert_eq!(economy.trade_goods["gold"].gold_type, Some(true));
    let north = loaded.province(1).unwrap();
    assert_eq!(north.buildings.unwrap(), ["test_fort"]);
    assert_eq!(
        north.trade_good_definition.unwrap().base_price_milli,
        Some(2125)
    );
    assert_eq!(north.trade_node, Some("river"));
    assert_eq!(north.trade_node_outgoing.unwrap(), ["harbor"]);
    assert_eq!(
        loaded.province(2).unwrap().buildings.unwrap(),
        ["test_market"]
    );
    assert!(loaded.province(3).unwrap().buildings.is_none());
    assert!(loaded.province(4).unwrap().buildings.unwrap().is_empty());
    assert_eq!(loaded.report.economy.building_instances, 2);
    assert_eq!(loaded.report.economy.trade_routes, 1);
    let origin = &loaded.report.economy.building_history_sources["1:test_market"];
    assert_eq!(origin.line, 20);
    assert!(origin.path.ends_with("history/provinces/1 - North.txt"));
    assert_eq!(
        loaded.report.economy.unapplied_definition_fields["buildings:modifier"],
        1
    );
    let core = loaded.initial.world();
    let restored =
        World::restore(&core.save("fixture").unwrap(), "fixture", core.schema()).unwrap();
    assert_eq!(&restored, core);
    let earlier = VanillaWorld::load(fixture(), "1430.1.1").unwrap();
    assert_eq!(
        earlier.province(1).unwrap().buildings.unwrap(),
        ["test_market"]
    );
    let future = VanillaWorld::load(fixture(), "1450.1.1").unwrap();
    assert!(future.province(1).unwrap().buildings.unwrap().is_empty());
    assert_eq!(
        future.province(1).unwrap().state.trade_good.as_deref(),
        Some("cloth")
    );
}

#[test]
fn duplicate_catalogs_unknown_goods_and_invalid_history_fail_with_locations() {
    for (path, from, to, message) in [
        (
            "common/buildings/test_buildings.txt",
            "test_fort =",
            "test_market =",
            "duplicate economic definition",
        ),
        (
            "common/buildings/test_buildings.txt",
            "cost = 87.125",
            "cost = -1",
            "nonnegative value",
        ),
        (
            "history/provinces/1 - North.txt",
            "test_fort = yes",
            "test_fort = 1",
            "expected yes or no",
        ),
        (
            "history/provinces/1 - North.txt",
            "trade_goods = grain",
            "trade_goods = missing",
            "unknown trade good missing",
        ),
        (
            "common/prices/test_prices.txt",
            "grain =",
            "missing =",
            "unknown trade good missing",
        ),
        (
            "common/tradenodes/test_nodes.txt",
            "members = { 1 }",
            "members = { 1 1 }",
            "duplicate trade node member",
        ),
        (
            "common/tradenodes/test_nodes.txt",
            "members = { 1 }",
            "members = { 999 }",
            "unknown province 999",
        ),
        (
            "common/tradenodes/test_nodes.txt",
            "name = harbor",
            "name = missing",
            "unknown trade route destination",
        ),
    ] {
        let temp = copy_fixture();
        let path = temp.path().join(path);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains(from));
        fs::write(path, text.replace(from, to)).unwrap();
        let error = VanillaWorld::load(temp.path(), "1444.11.11")
            .unwrap_err()
            .to_string();
        assert!(error.contains(message), "{error}");
        if matches!(
            message,
            "duplicate economic definition"
                | "nonnegative value"
                | "expected yes or no"
                | "duplicate trade node member"
        ) {
            assert!(error.contains(".txt:"), "missing input location: {error}");
        }
    }
}
