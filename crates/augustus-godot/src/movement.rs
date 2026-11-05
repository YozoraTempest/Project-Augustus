// SPDX-License-Identifier: GPL-3.0-only

use augustus_core::{ArmyMovement, GameDate};
use augustus_eu4::world::OpeningWorld;
use godot::prelude::*;

pub(crate) fn details(
    world: &OpeningWorld,
    movement: &ArmyMovement,
    date: GameDate,
) -> VarDictionary {
    let mut info = VarDictionary::new();
    info.set("from", i64::from(movement.from));
    info.set("to", i64::from(movement.to));
    info.set("from_name", world.province_name(movement.from).as_str());
    info.set("to_name", world.province_name(movement.to).as_str());
    info.set("started_on", movement.started_on.to_string().as_str());
    info.set("arrives_on", movement.arrives_on.to_string().as_str());
    info.set("days_remaining", i64::from(movement.days_remaining(date)));
    info.set("progress", movement.progress(date) * 100.0);
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
        for (&id, &days) in connections {
            let mut info = VarDictionary::new();
            info.set("province", i64::from(id));
            info.set("name", world.province_name(id).as_str());
            info.set("days", i64::from(days));
            info.set("position", anchor(world, id));
            neighbors.push(&info);
        }
    }
    neighbors
}
