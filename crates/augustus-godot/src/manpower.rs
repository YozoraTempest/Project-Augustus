// SPDX-License-Identifier: GPL-3.0-only

use augustus_eu4::world::OpeningWorld;
use godot::prelude::*;

pub(crate) fn info(world: &OpeningWorld, tag: &str) -> VarDictionary {
    let mut info = VarDictionary::new();
    if let Some(summary) = world.state().country_manpower(tag) {
        info.set("available", summary.available as i64);
        info.set("capacity", summary.capacity as i64);
        info.set("monthly_recovery", summary.monthly_recovery as i64);
    }
    info
}
