// SPDX-License-Identifier: GPL-3.0-only

use augustus_core::{Army, GameDate, ProvinceKind, REGIMENT_STRENGTH, UnitKind};
use augustus_eu4::world::OpeningWorld;
use godot::prelude::*;

use crate::info::{money, text};

pub(crate) fn army_name(world: &OpeningWorld, army: &Army) -> String {
    format!(
        "{} 第 {} 军队",
        world.display_name(&army.country),
        army.id()
    )
}

pub(crate) fn army_info(
    world: &OpeningWorld,
    army: &Army,
    date: GameDate,
    player: Option<&str>,
) -> VarDictionary {
    let mut info = VarDictionary::new();
    info.set("id", i64::from(army.id()));
    info.set("name", army_name(world, army).as_str());
    info.set("country", army.country.as_str());
    info.set("can_command", player == Some(army.country.as_str()));
    info.set("country_name", world.display_name(&army.country).as_str());
    info.set("province", i64::from(army.province));
    info.set("province_name", world.province_name(army.province).as_str());
    info.set("strength", army.strength() as i64);
    info.set("regiment_count", army.regiments.len() as i64);
    let mut regiments: Array<VarDictionary> = Array::new();
    for regiment in &army.regiments {
        let mut details = VarDictionary::new();
        details.set(
            "kind",
            match regiment.kind {
                UnitKind::Infantry => "步兵",
            },
        );
        details.set("strength", i64::from(regiment.strength));
        regiments.push(&details);
    }
    info.set("regiments", &regiments);
    if let Some(movement) = &army.movement {
        info.set("movement", &crate::movement::details(world, movement, date));
    } else {
        info.set("movement", &Variant::nil());
    }
    let targets: Vec<_> = world
        .state()
        .land_neighbors(army.province)
        .into_iter()
        .flatten()
        .filter(|(id, _)| {
            world.state().province(**id).is_some_and(|province| {
                province.owner.as_deref() == Some(&army.country)
                    && province.controller.as_deref() == Some(&army.country)
            })
        })
        .map(|(&id, &days)| {
            GString::from(format!("{}（{} 天）", world.province_name(id), days).as_str())
        })
        .collect();
    info.set("move_targets", &PackedStringArray::from(targets.as_slice()));
    info
}

pub(crate) fn recruitment_info(
    world: &OpeningWorld,
    id: u32,
    player: Option<&str>,
    date: GameDate,
) -> VarDictionary {
    let mut info = VarDictionary::new();
    let state = world.state();
    if !state
        .province(id)
        .is_some_and(|province| province.kind == ProvinceKind::Land)
        || state.province_monthly_tax(id).is_none()
    {
        return info;
    }
    let rules = state.military_rules();
    info.set("province_name", world.province_name(id).as_str());
    money(&mut info, "cost", Some(rules.infantry_cost));
    info.set("manpower_cost", rules.manpower.infantry_cost as i64);
    if let Some(player) = player {
        info.set("manpower", &crate::manpower::info(world, player));
    } else {
        info.set("manpower", &Variant::nil());
    }
    info.set("days", i64::from(rules.infantry_days));
    info.set("strength", i64::from(REGIMENT_STRENGTH));
    text(
        &mut info,
        "completes_on",
        date.checked_add_days(rules.infantry_days)
            .map(|date| date.to_string())
            .as_deref(),
    );
    let reason = player.map_or_else(
        || "开始游戏后可招募".to_owned(),
        |country| {
            state
                .quote_recruitment(country, id, date)
                .err()
                .map_or_else(String::new, |error| error.to_string())
        },
    );
    info.set("can_recruit", reason.is_empty());
    info.set("reason", reason.as_str());
    if let Some(project) = state.recruitment(id) {
        let mut details = VarDictionary::new();
        details.set("country", project.country.as_str());
        details.set("started_on", project.started_on.to_string().as_str());
        details.set("completes_on", project.completes_on.to_string().as_str());
        let total = project.started_on.days_until(project.completes_on);
        let remaining = date.days_until(project.completes_on).max(0);
        details.set("days_remaining", i64::from(remaining));
        details.set(
            "progress",
            f64::from(total - remaining) / f64::from(total) * 100.0,
        );
        info.set("project", &details);
    } else {
        info.set("project", &Variant::nil());
    }
    let mut armies: Array<VarDictionary> = Array::new();
    for army in state.armies().values().filter(|army| army.province == id) {
        armies.push(&army_info(world, army, date, player));
    }
    info.set("armies", &armies);
    info
}

pub(crate) fn markers(
    world: &OpeningWorld,
    date: GameDate,
    player: Option<&str>,
) -> Array<VarDictionary> {
    let mut markers = Array::new();
    for army in world.state().armies().values() {
        let mut marker = army_info(world, army, date, player);
        marker.set("position", crate::movement::anchor(world, army.province));
        if let Some(movement) = &army.movement {
            marker.set(
                "destination_position",
                crate::movement::anchor(world, movement.current_leg().to),
            );
        }
        let [r, g, b] = world.country_color(&army.country).expect("军队国家已注册");
        marker.set(
            "color",
            Color::from_rgb(
                f32::from(r) / 255.0,
                f32::from(g) / 255.0,
                f32::from(b) / 255.0,
            ),
        );
        markers.push(&marker);
    }
    markers
}
