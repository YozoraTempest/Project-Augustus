// SPDX-License-Identifier: GPL-3.0-only

use augustus_core::{ArmyMovement, GameDate, MilitaryRules, UnitKind};
use augustus_eu4::world::OpeningWorld;
use godot::prelude::*;

pub(crate) fn details(
    world: &OpeningWorld,
    movement: &ArmyMovement,
    date: GameDate,
) -> VarDictionary {
    let mut info = VarDictionary::new();
    let current = movement.current_leg();
    info.set("from", i64::from(current.from));
    info.set("to", i64::from(current.to));
    info.set("from_name", world.province_name(current.from).as_str());
    info.set("to_name", world.province_name(current.to).as_str());
    info.set("destination", i64::from(movement.destination()));
    info.set(
        "destination_name",
        world.province_name(movement.destination()).as_str(),
    );
    info.set("started_on", movement.started_on().to_string().as_str());
    info.set("arrives_on", movement.arrives_on().to_string().as_str());
    info.set("days_remaining", i64::from(movement.days_remaining(date)));
    info.set("progress", movement.progress(date) * 100.0);
    info.set("leg_index", movement.leg_index() as i64);
    info.set("leg_progress", current.progress(date) * 100.0);
    info.set("leg_arrives_on", current.arrives_on.to_string().as_str());
    let mut route: Array<VarDictionary> = Array::new();
    for leg in movement.legs() {
        let mut details = VarDictionary::new();
        details.set("from_name", world.province_name(leg.from).as_str());
        details.set("to_name", world.province_name(leg.to).as_str());
        details.set("from_position", anchor(world, leg.from));
        details.set("to_position", anchor(world, leg.to));
        route.push(&details);
    }
    info.set("route", &route);
    info
}

pub(crate) fn anchor(world: &OpeningWorld, province: u32) -> Vector2 {
    let [x, y] = world
        .map()
        .province_anchor(province)
        .expect("军队省份存在于地图");
    Vector2::new(x as f32 + 0.5, y as f32 + 0.5)
}

pub(crate) fn neighbors(world: &OpeningWorld, province: u32) -> Array<VarDictionary> {
    let mut neighbors = Array::new();
    if let Some(connections) = world.state().land_neighbors(province) {
        // 邻省列表没有选定军队，天数按步兵速度估算。具体命令使用该军队的最慢兵种。
        let speed = world.state().military_rules().speed(UnitKind::Infantry);
        for (&id, &distance) in connections {
            let mut info = VarDictionary::new();
            info.set("province", i64::from(id));
            info.set("name", world.province_name(id).as_str());
            info.set(
                "days",
                i64::from(MilitaryRules::travel_days(distance, speed)),
            );
            info.set("position", anchor(world, id));
            neighbors.push(&info);
        }
    }
    neighbors
}
