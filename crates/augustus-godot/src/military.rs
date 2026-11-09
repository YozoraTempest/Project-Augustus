// SPDX-License-Identifier: GPL-3.0-only

use augustus_core::{
    Army, GameDate, MilitaryRules, Money, ProvinceKind, REGIMENT_STRENGTH, Recruitment, UnitKind,
};
use augustus_eu4::world::OpeningWorld;
use godot::prelude::*;

use crate::info::{money, text};

fn stance(world: &OpeningWorld, player: Option<&str>, country: &str) -> &'static str {
    match player {
        Some(player) if player == country => "own",
        Some(player) if world.state().at_war(player, country) => "enemy",
        _ => "neutral",
    }
}

pub(crate) fn morale_text(current: u32, max: u32) -> String {
    format!(
        "{:.1}/{:.1}",
        f64::from(current) / 1_000.0,
        f64::from(max) / 1_000.0
    )
}

pub(crate) fn unit_label(kind: UnitKind) -> &'static str {
    match kind {
        UnitKind::Infantry => "步兵",
        UnitKind::Cavalry => "骑兵",
    }
}

pub(crate) fn parse_unit_kind(kind: &str) -> Option<UnitKind> {
    UnitKind::ALL.into_iter().find(|unit| unit.id() == kind)
}

pub(crate) fn composition(army: &Army) -> Array<VarDictionary> {
    let mut entries = Array::new();
    for kind in UnitKind::ALL {
        let (count, strength) = army.composition(kind);
        if count == 0 {
            continue;
        }
        let mut entry = VarDictionary::new();
        entry.set("kind", unit_label(kind));
        entry.set("kind_id", kind.id());
        entry.set("regiment_count", count as i64);
        entry.set("strength", strength as i64);
        entries.push(&entry);
    }
    entries
}

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
    info.set("stance", stance(world, player, &army.country));
    info.set("country_name", world.display_name(&army.country).as_str());
    info.set("province", i64::from(army.province));
    info.set("province_name", world.province_name(army.province).as_str());
    info.set("position", crate::movement::anchor(world, army.province));
    info.set("strength", army.strength() as i64);
    info.set("regiment_count", army.regiments.len() as i64);
    let max_morale = world
        .state()
        .country_military(&army.country)
        .and_then(|military| military.morale_milli())
        .or_else(|| world.state().military_rules().combat.morale_milli())
        .expect("陆战规则已检查");
    if let Some(military) = world.state().country_military(&army.country) {
        info.set("military_level", i64::from(military.level));
        info.set("unit_group", military.unit_group.as_str());
    } else {
        info.set("military_level", &Variant::nil());
        info.set("unit_group", &Variant::nil());
    }
    let in_battle = army.battle_id().is_some();
    let retreating = army.is_retreating();
    info.set("in_battle", in_battle);
    info.set("retreating", retreating);
    info.set("exiled", army.is_exiled());
    info.set(
        "morale",
        morale_text(army.average_morale(max_morale), max_morale).as_str(),
    );
    let combat_status = world.state().army_combat_status(army.id());
    info.set("combat_status", combat_status.as_str());
    let order_block = if in_battle {
        "正在交战，不能下达命令"
    } else if retreating {
        "正在撤退，到达前不能接受命令"
    } else {
        ""
    };
    info.set("order_block", order_block);
    info.set(
        "can_move",
        order_block.is_empty() && player == Some(army.country.as_str()),
    );
    if let Some(forecast) = world.state().supply_forecast(army.id(), date) {
        let mut supply = VarDictionary::new();
        supply.set("date", forecast.on.to_string().as_str());
        supply.set("province", i64::from(forecast.province));
        supply.set(
            "province_name",
            world.province_name(forecast.province).as_str(),
        );
        supply.set("supply_limit", i64::from(forecast.supply_limit));
        supply.set("regiment_count", i64::from(forecast.regiment_count));
        supply.set("loss", i64::from(forecast.loss));
        info.set("supply", &supply);
    } else {
        info.set("supply", &Variant::nil());
    }
    info.set("composition", &composition(army));
    let mut regiments: Array<VarDictionary> = Array::new();
    for regiment in &army.regiments {
        let mut details = VarDictionary::new();
        details.set("kind", unit_label(regiment.kind));
        if regiment.unit.is_empty() {
            details.set("unit", &Variant::nil());
        } else {
            details.set("unit", regiment.unit.as_str());
        }
        details.set("strength", i64::from(regiment.strength));
        regiments.push(&details);
    }
    info.set("regiments", &regiments);
    if let Some(reinforcement) = world.state().army_reinforcement_text(army.id(), date) {
        info.set("reinforcement", reinforcement.as_str());
    }
    if let Some(movement) = &army.movement {
        info.set("movement", &crate::movement::details(world, movement, date));
        info.set(
            "destination_position",
            crate::movement::anchor(world, movement.current_leg().to),
        );
    } else {
        info.set("movement", &Variant::nil());
    }
    let targets: Vec<_> = world
        .state()
        .land_neighbors(army.province)
        .into_iter()
        .flatten()
        .filter(|(id, _)| world.state().can_enter(&army.country, **id))
        .map(|(&id, &distance)| {
            let days = MilitaryRules::travel_days(
                distance,
                army.march_speed(world.state().military_rules()),
            );
            GString::from(format!("{}（{} 天）", world.province_name(id), days).as_str())
        })
        .collect();
    info.set("move_targets", &PackedStringArray::from(targets.as_slice()));
    info
}

fn unit_offer(
    world: &OpeningWorld,
    id: u32,
    player: Option<&str>,
    date: GameDate,
    kind: UnitKind,
) -> VarDictionary {
    let state = world.state();
    let manpower_cost = state.military_rules().manpower_cost(kind);
    let (cost, days) = state.recruitment_terms(id, kind);
    let mut offer = VarDictionary::new();
    offer.set("kind_id", kind.id());
    offer.set("kind", unit_label(kind));
    money(&mut offer, "cost", Some(cost));
    offer.set("manpower_cost", manpower_cost as i64);
    offer.set("days", i64::from(days));
    offer.set("strength", i64::from(REGIMENT_STRENGTH));
    let quoted = player.map(|country| state.quote_recruitment(country, id, kind, date));
    // 报价成功时，完成日包含本省已有队列的等待。尚未开局时从当天起算本单工期。
    let completes_on = match &quoted {
        Some(Ok(quote)) => Some(quote.completes_on.to_string()),
        None => date.checked_add_days(days).map(|date| date.to_string()),
        Some(Err(_)) => None,
    };
    text(&mut offer, "completes_on", completes_on.as_deref());
    let reason = match quoted {
        None => "开始游戏后可招募".to_owned(),
        Some(Ok(_)) => String::new(),
        Some(Err(error)) => error.to_string(),
    };
    offer.set("can_recruit", reason.is_empty());
    offer.set("reason", reason.as_str());
    offer
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
    info.set("province_name", world.province_name(id).as_str());
    if let Some(player) = player {
        info.set("manpower", &crate::manpower::info(world, player));
    } else {
        info.set("manpower", &Variant::nil());
    }
    let mut offers: Array<VarDictionary> = Array::new();
    for kind in UnitKind::ALL {
        offers.push(&unit_offer(world, id, player, date, kind));
    }
    info.set("offers", &offers);
    let mut orders: Array<VarDictionary> = Array::new();
    for project in state.recruitment_queue(id) {
        orders.push(&recruitment_details(world, id, project, player, date));
    }
    info.set("orders", &orders);
    let combat_status = state.province_combat_status(id);
    if !combat_status.is_empty() {
        info.set("combat_status", combat_status.as_str());
    }
    let mut armies: Array<VarDictionary> = Array::new();
    for army in state.armies().values().filter(|army| army.province == id) {
        armies.push(&army_info(world, army, date, player));
    }
    info.set("armies", &armies);
    info
}

// 省份面板和全国总览共用同一份进度、日期与退款信息。
pub(crate) fn recruitment_details(
    world: &OpeningWorld,
    province: u32,
    project: &Recruitment,
    player: Option<&str>,
    date: GameDate,
) -> VarDictionary {
    let mut details = VarDictionary::new();
    details.set("id", i64::from(project.id()));
    details.set("province", i64::from(province));
    details.set("province_name", world.province_name(province).as_str());
    details.set("queued", date < project.started_on);
    details.set("country", project.country.as_str());
    details.set("stance", stance(world, player, &project.country));
    details.set("kind", unit_label(project.kind));
    details.set("kind_id", project.kind.id());
    details.set("started_on", project.started_on.to_string().as_str());
    details.set("completes_on", project.completes_on.to_string().as_str());
    let refund = player.and_then(|country| {
        world
            .state()
            .quote_cancel_recruitment(country, province, project.id(), date)
            .ok()
    });
    details.set("can_cancel", refund.is_some());
    details.set(
        "refund_rules",
        format!(
            "金币：进度不超过 {}% 时全退，超过后退还 50%。\n人力独立返还，允许超过储备上限。",
            (world
                .state()
                .military_rules()
                .recruitment_full_refund_progress
                * Money::from(100))
            .normalize()
        )
        .as_str(),
    );
    money(
        &mut details,
        "refund_cost",
        refund.as_ref().map(|refund| refund.cost),
    );
    details.set(
        "refund_manpower",
        refund.as_ref().map_or(0, |refund| refund.manpower) as i64,
    );
    details.set(
        "days_remaining",
        i64::from(date.days_until(project.completes_on).max(0)),
    );
    details.set("progress", project.progress(date));
    details
}

pub(crate) fn markers(
    world: &OpeningWorld,
    date: GameDate,
    player: Option<&str>,
) -> Array<VarDictionary> {
    let mut markers = Array::new();
    for army in world.state().armies().values() {
        let mut marker = army_info(world, army, date, player);
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
