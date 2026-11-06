// SPDX-License-Identifier: GPL-3.0-only

//! 将核心状态投影为界面数据，不在 Godot 中维护另一份国家或省份状态。

use augustus_core::{Money, ProvinceKind};
use augustus_eu4::world::OpeningWorld;
use godot::prelude::*;

pub(crate) fn province_info(world: &OpeningWorld, id: u32) -> VarDictionary {
    let mut info = VarDictionary::new();
    let Some(province) = world.state().province(id) else {
        return info;
    };
    info.set("id", i64::from(id));
    info.set("name", world.province_name(id).as_str());
    info.set("kind", kind_name(province.kind));
    text(&mut info, "owner", province.owner.as_deref());
    text(
        &mut info,
        "owner_name",
        province
            .owner
            .as_deref()
            .map(|tag| world.display_name(tag))
            .as_deref(),
    );
    text(&mut info, "controller", province.controller.as_deref());
    text(
        &mut info,
        "controller_name",
        province
            .controller
            .as_deref()
            .map(|tag| world.display_name(tag))
            .as_deref(),
    );
    for (key, value) in [
        ("culture", &province.culture),
        ("religion", &province.religion),
        ("trade_good", &province.trade_good),
    ] {
        text(
            &mut info,
            key,
            value
                .as_deref()
                .map(|key| world.display_name(key))
                .as_deref(),
        );
    }
    number(
        &mut info,
        "base_tax",
        province.development.tax.map(u64::from),
    );
    number(
        &mut info,
        "base_production",
        province.development.production.map(u64::from),
    );
    number(
        &mut info,
        "base_manpower",
        province.development.manpower.map(u64::from),
    );
    number(&mut info, "development", province.development.total());
    money(
        &mut info,
        "monthly_tax",
        world.state().province_monthly_tax(id),
    );
    money(&mut info, "base_monthly_tax", province.monthly_tax());
    text(
        &mut info,
        "building_tax_bonus",
        world
            .state()
            .province_building_tax_modifier(id)
            .map(|modifier| format!("{}%", (modifier * Money::from(100)).normalize()))
            .as_deref(),
    );
    info
}

pub(crate) fn country_info(world: &OpeningWorld, tag: &str, player: Option<&str>) -> VarDictionary {
    let mut info = VarDictionary::new();
    let Some(country) = world.state().country(tag) else {
        return info;
    };
    info.set("tag", tag);
    info.set("name", world.display_name(tag).as_str());
    info.set(
        "history_conflicts",
        world.country_history_has_conflicts(tag),
    );
    let [r, g, b] = world.country_color(tag).expect("注册国家具有地图颜色");
    info.set(
        "color",
        Color::from_rgb(
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
        ),
    );
    number(&mut info, "capital", country.capital.map(u64::from));
    text(
        &mut info,
        "capital_name",
        country.capital.map(|id| world.province_name(id)).as_deref(),
    );
    for (key, value) in [
        ("primary_culture", &country.primary_culture),
        ("religion", &country.religion),
    ] {
        text(
            &mut info,
            key,
            value
                .as_deref()
                .map(|key| world.display_name(key))
                .as_deref(),
        );
    }
    let summary = world
        .country_summary(tag)
        .expect("注册国家可以查询领土概况");
    info.set("province_count", summary.provinces as i64);
    number(&mut info, "development", summary.development);
    info.extend_dictionary(&country_finances(world, tag), true);
    info.set("manpower", &crate::manpower::info(world, tag));
    info.set("armies", &country_armies(world, tag, player));
    let mut wars: Array<VarDictionary> = Array::new();
    for war in world.state().wars_of(tag) {
        let mut entry = VarDictionary::new();
        entry.set("id", i64::from(war.id()));
        entry.set("attacker", war.attacker());
        entry.set("defender", war.defender());
        entry.set("attacker_name", world.display_name(war.attacker()).as_str());
        entry.set("defender_name", world.display_name(war.defender()).as_str());
        entry.set("started_on", war.started_on().to_string().as_str());
        entry.set("status", war.status().id());
        wars.push(&entry);
    }
    info.set("wars", &wars);
    let (can_declare, reason) = match player {
        Some(player) => match world.state().declaration_error(player, tag) {
            Ok(()) => (true, String::new()),
            Err(error) => (false, error.to_string()),
        },
        None => (false, String::new()),
    };
    info.set("can_declare_war", can_declare);
    info.set("declare_war_reason", reason.as_str());
    info
}

/// 国家页只列出编制。指挥、拆分和解散仍由省份军事面板按 `can_command` 限制。
fn country_armies(world: &OpeningWorld, tag: &str, player: Option<&str>) -> Array<VarDictionary> {
    let mut armies = Array::new();
    for army in world
        .state()
        .armies()
        .values()
        .filter(|army| army.country == tag)
    {
        let mut entry = VarDictionary::new();
        entry.set("id", i64::from(army.id()));
        entry.set("name", crate::military::army_name(world, army).as_str());
        entry.set("province", i64::from(army.province));
        entry.set("province_name", world.province_name(army.province).as_str());
        entry.set("strength", army.strength() as i64);
        entry.set("regiment_count", army.regiments.len() as i64);
        entry.set("composition", &crate::military::composition(army));
        entry.set("can_command", player == Some(tag));
        armies.push(&entry);
    }
    armies
}

pub(crate) fn country_finances(world: &OpeningWorld, tag: &str) -> VarDictionary {
    let mut info = VarDictionary::new();
    let Some(country) = world.state().country(tag) else {
        return info;
    };
    money(&mut info, "treasury", Some(country.finances.treasury()));
    let budget = world
        .state()
        .country_monthly_budget(tag)
        .expect("国家已注册");
    money(&mut info, "monthly_tax", Some(budget.tax));
    money(&mut info, "army_maintenance", Some(budget.army_maintenance));
    money(
        &mut info,
        "base_army_maintenance",
        Some(budget.base_army_maintenance),
    );
    money(
        &mut info,
        "excess_army_maintenance",
        Some(budget.excess_army_maintenance),
    );
    signed_money(&mut info, "monthly_balance", budget.balance());
    info.set("in_deficit", budget.balance() < Money::ZERO);
    info.set(
        "treasury_negative",
        country.finances.treasury() < Money::ZERO,
    );
    let settlement = country.finances.last_settlement();
    text(
        &mut info,
        "last_settlement_month",
        settlement
            .map(|record| format!("{}.{}", record.month.year(), record.month.month()))
            .as_deref(),
    );
    text(
        &mut info,
        "last_settlement_date",
        settlement
            .map(|record| record.settled_on.to_string())
            .as_deref(),
    );
    money(&mut info, "last_tax", settlement.map(|record| record.tax));
    money(
        &mut info,
        "last_maintenance",
        settlement.map(|record| record.army_maintenance),
    );
    money(
        &mut info,
        "last_base_maintenance",
        settlement.map(|record| record.base_maintenance()),
    );
    money(
        &mut info,
        "last_excess_maintenance",
        settlement.map(|record| record.excess_maintenance),
    );
    if let Some(record) = settlement {
        signed_money(&mut info, "last_balance", record.balance());
    } else {
        info.set("last_balance", &Variant::nil());
    }
    info
}

pub(crate) fn kind_name(kind: ProvinceKind) -> &'static str {
    match kind {
        ProvinceKind::Land => "land",
        ProvinceKind::Sea => "sea",
        ProvinceKind::Lake => "lake",
        ProvinceKind::Wasteland => "wasteland",
    }
}

pub(crate) fn text(info: &mut VarDictionary, key: &str, value: Option<&str>) {
    match value {
        Some(value) => info.set(key, value),
        None => info.set(key, &Variant::nil()),
    }
}

fn number(info: &mut VarDictionary, key: &str, value: Option<u64>) {
    match value {
        Some(value) => info.set(key, value as i64),
        None => info.set(key, &Variant::nil()),
    }
}

pub(crate) fn money(info: &mut VarDictionary, key: &str, value: Option<Money>) {
    // 金额由 Rust 格式化后传给界面，Godot 不进行浮点转换或财政计算。
    text(
        info,
        key,
        value
            .map(|amount| format!("{:.3}", amount.round_dp(3)))
            .as_deref(),
    );
}

fn signed_money(info: &mut VarDictionary, key: &str, value: Money) {
    info.set(key, format!("{:+.3}", value.round_dp(3)).as_str());
}
