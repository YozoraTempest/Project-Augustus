// SPDX-License-Identifier: GPL-3.0-only

use augustus_eu4::world::OpeningWorld;
use godot::prelude::*;

pub(crate) fn disband_info(world: &OpeningWorld, id: u32, player: &str) -> VarDictionary {
    let mut info = VarDictionary::new();
    match world.state().quote_disband_army(player, id) {
        Ok(quote) => {
            let army = &world.state().armies()[&id];
            info.set("error", "");
            info.set("id", i64::from(id));
            info.set("name", crate::military::army_name(world, army).as_str());
            info.set("province_name", world.province_name(army.province).as_str());
            info.set("regiment_count", quote.regiment_count as i64);
            info.set("strength", quote.strength as i64);
            crate::info::money(
                &mut info,
                "monthly_maintenance",
                Some(quote.monthly_maintenance),
            );
            info.set("manpower_returned", quote.manpower_returned as i64);
        }
        Err(error) => info.set("error", error.to_string().as_str()),
    }
    info
}

pub(crate) fn info(world: &OpeningWorld, id: u32, player: Option<&str>) -> VarDictionary {
    let mut info = VarDictionary::new();
    let state = world.state();
    let Some(army) = state
        .armies()
        .get(&id)
        .filter(|army| player == Some(army.country.as_str()) && army.movement.is_none())
    else {
        return info;
    };
    info.set("id", i64::from(id));
    info.set("regiment_count", army.regiments.len() as i64);
    let mut candidates: Array<VarDictionary> = Array::new();
    for candidate in state.armies().values().filter(|candidate| {
        candidate.id() != id
            && candidate.country == army.country
            && candidate.province == army.province
            && candidate.movement.is_none()
    }) {
        let mut details = VarDictionary::new();
        details.set("id", i64::from(candidate.id()));
        details.set(
            "name",
            crate::military::army_name(world, candidate).as_str(),
        );
        details.set("regiment_count", candidate.regiments.len() as i64);
        details.set("strength", candidate.strength() as i64);
        details.set("composition", &crate::military::composition(candidate));
        candidates.push(&details);
    }
    info.set("candidates", &candidates);
    let reason = state
        .quote_split_army(&army.country, id, 1)
        .err()
        .map_or_else(String::new, |error| error.to_string());
    info.set("can_split", reason.is_empty());
    info.set("split_reason", reason.as_str());
    info
}

pub(crate) fn split_result(result: Result<u32, String>) -> VarDictionary {
    let mut info = VarDictionary::new();
    let (id, error) = match result {
        Ok(id) => (id, String::new()),
        Err(error) => (0, error),
    };
    info.set("army_id", i64::from(id));
    info.set("error", error.as_str());
    info
}
