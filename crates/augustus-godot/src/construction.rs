// SPDX-License-Identifier: GPL-3.0-only

use augustus_core::{GameDate, Money, ProvinceKind};
use augustus_eu4::world::OpeningWorld;
use godot::prelude::*;

use crate::info::{money, text};

pub(crate) fn construction_info(
    world: &OpeningWorld,
    id: u32,
    player: Option<&str>,
    date: GameDate,
) -> VarDictionary {
    let mut info = VarDictionary::new();
    let state = world.state();
    let Some(province) = state.province(id).filter(|province| {
        province.kind == ProvinceKind::Land && state.province_monthly_tax(id).is_some()
    }) else {
        return info;
    };
    info.set("province_name", world.province_name(id).as_str());
    let names: Vec<_> = province
        .buildings
        .iter()
        .map(|id| GString::from(world.building_name(id).as_str()))
        .collect();
    info.set("buildings", &PackedStringArray::from(names.as_slice()));
    let project = province.construction.as_ref();
    if let Some(project) = project {
        let mut details = VarDictionary::new();
        details.set("name", world.building_name(&project.building).as_str());
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
    let mut options: Array<VarDictionary> = Array::new();
    for definition in state
        .building_definitions()
        .values()
        .filter(|definition| definition.constructible)
    {
        let mut option = VarDictionary::new();
        option.set("id", definition.id.as_str());
        option.set("name", world.building_name(&definition.id).as_str());
        option.set("months", i64::from(definition.months));
        option.set(
            "tax_bonus",
            format!(
                "{}%",
                (definition.tax_modifier * Money::from(100)).normalize()
            )
            .as_str(),
        );
        money(&mut option, "cost", Some(definition.cost));
        let before = state.province_monthly_tax(id);
        let after = state.province_monthly_tax_after_building(id, &definition.id);
        money(&mut option, "tax_before", before);
        money(&mut option, "tax_after", after);
        money(
            &mut option,
            "tax_gain",
            before.zip(after).map(|(before, after)| after - before),
        );
        text(
            &mut option,
            "completes_on",
            date.checked_add_months(definition.months)
                .map(|date| date.to_string())
                .as_deref(),
        );
        let reason = player.map_or_else(
            || "开始游戏后可建造".to_owned(),
            |country| {
                state
                    .quote_construction(country, id, &definition.id, date)
                    .err()
                    .map_or_else(String::new, |error| error.to_string())
            },
        );
        option.set("can_build", reason.is_empty());
        option.set("reason", reason.as_str());
        options.push(&option);
    }
    info.set("options", &options);
    info
}
