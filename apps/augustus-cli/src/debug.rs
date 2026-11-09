// SPDX-License-Identifier: GPL-3.0-only

//! 把核心世界整理成调试服务使用的 JSON。命令仍调用玩家使用的接口。

use augustus_core::{
    Army, DayAdvance, GameDate, Money, MonthlyBudget, PeaceSheet, Province, Simulation, UnitKind,
    War, World, format_warscore_milli,
};
use serde_json::{Value, json};

pub fn clock_report(simulation: &Simulation) -> Value {
    let clock = simulation.clock();
    json!({
        "date": clock.date().to_string(),
        "paused": clock.is_paused(),
        "speed": clock.speed().multiplier(),
    })
}

pub fn country_report(world: &World, tag: &str, name: &str) -> Result<Value, String> {
    let country = world.country(tag).ok_or("请选择有效的国家")?;
    let budget = world.country_monthly_budget(tag).map(budget_value);
    let wars: Vec<_> = world.wars_of(tag).map(war_value).collect();
    let truces: Vec<_> = world
        .truces_of(tag)
        .map(|truce| {
            json!({
                "countries": truce.countries(),
                "until": truce.until().to_string(),
            })
        })
        .collect();
    Ok(json!({
        "tag": country.tag(),
        "name": name,
        "capital": country.capital,
        "treasury": money(country.finances.treasury()),
        "manpower": world.country_manpower(tag).map(|summary| json!({
            "available": summary.available,
            "capacity": summary.capacity,
            "monthly_recovery": summary.monthly_recovery,
        })),
        "land_force_limit": world.country_land_force_limit(tag),
        "budget": budget,
        "wars": wars,
        "truces": truces,
    }))
}

pub fn province_report(
    world: &World,
    id: u32,
    name: &str,
    date: GameDate,
) -> Result<Value, String> {
    let province = world.province(id).ok_or("请选择有效的省份")?;
    Ok(json!({
        "id": id,
        "name": name,
        "kind": province_kind(province),
        "owner": province.owner,
        "controller": province.controller,
        "development": {
            "tax": province.development.tax,
            "production": province.development.production,
            "manpower": province.development.manpower,
        },
        "buildings": province.buildings,
        "construction": province.construction.as_ref().map(|project| json!({
            "building": project.building,
            "country": project.country,
            "completes_on": project.completes_on.to_string(),
        })),
        "occupation": province.occupation.as_ref().map(|progress| json!({
            "occupier": progress.occupier,
            "days": progress.days,
        })),
        "combat": world.province_combat_status(id),
        "recruitment": world.recruitment_queue(id).iter().map(|order| json!({
            "id": order.id(),
            "country": order.country,
            "kind": order.kind.id(),
            "started_on": order.started_on.to_string(),
            "completes_on": order.completes_on.to_string(),
            "progress": order.progress(date),
        })).collect::<Vec<_>>(),
    }))
}

pub fn army_report(world: &World, id: u32, date: GameDate) -> Result<Value, String> {
    let army = world.armies().get(&id).ok_or("请选择有效的军队")?;
    Ok(army_value(world, army, date))
}

pub fn armies_report(world: &World, tag: Option<&str>, date: GameDate) -> Result<Value, String> {
    if let Some(tag) = tag {
        world.country(tag).ok_or("请选择有效的国家")?;
    }
    let armies: Vec<_> = world
        .armies()
        .values()
        .filter(|army| tag.is_none_or(|tag| army.country == tag))
        .map(|army| army_value(world, army, date))
        .collect();
    Ok(json!({ "armies": armies }))
}

pub fn battle_report(world: &World, id: u32) -> Result<Value, String> {
    let battle = world.battle(id).ok_or("没有这场战斗")?;
    Ok(battle_value(battle))
}

pub fn battles_report(world: &World) -> Value {
    json!({
        "battles": world.battles().values().map(battle_value).collect::<Vec<_>>(),
    })
}

pub fn peace_report(
    world: &World,
    country: &str,
    enemy: &str,
    date: GameDate,
) -> Result<Value, String> {
    world
        .peace_sheet(country, enemy, date)
        .map(|sheet| peace_value(&sheet))
        .map_err(|error| error.to_string())
}

pub fn declare_war(
    world: &mut World,
    player: &str,
    defender: &str,
    date: GameDate,
) -> Result<Value, String> {
    let id = world
        .declare_war(player, defender, date)
        .map_err(|error| error.to_string())?;
    Ok(json!({ "ok": true, "war_id": id }))
}

pub fn move_army(
    world: &mut World,
    player: &str,
    id: u32,
    destination: u32,
    date: GameDate,
) -> Result<Value, String> {
    world
        .move_army(player, id, destination, date)
        .map_err(|error| error.to_string())?;
    Ok(json!({ "ok": true }))
}

pub fn stop_army(world: &mut World, player: &str, id: u32) -> Result<Value, String> {
    world
        .stop_army(player, id)
        .map_err(|error| error.to_string())?;
    Ok(json!({ "ok": true }))
}

pub fn retreat_army(
    world: &mut World,
    player: &str,
    id: u32,
    date: GameDate,
) -> Result<Value, String> {
    world
        .retreat_from_battle(player, id, date)
        .map_err(|error| error.to_string())?;
    Ok(json!({ "ok": true }))
}

pub fn recruit(
    world: &mut World,
    player: &str,
    province: u32,
    kind: &str,
    date: GameDate,
) -> Result<Value, String> {
    let kind = match kind {
        "infantry" => UnitKind::Infantry,
        "cavalry" => UnitKind::Cavalry,
        _ => return Err("请选择步兵或骑兵".to_owned()),
    };
    world
        .start_recruitment(player, province, kind, date)
        .map_err(|error| error.to_string())?;
    Ok(json!({ "ok": true }))
}

pub fn cancel_recruitment(
    world: &mut World,
    player: &str,
    province: u32,
    order_id: u32,
    date: GameDate,
) -> Result<Value, String> {
    world
        .cancel_recruitment(player, province, order_id, date)
        .map_err(|error| error.to_string())?;
    Ok(json!({ "ok": true }))
}

pub fn make_peace(
    world: &mut World,
    player: &str,
    enemy: &str,
    provinces: &[u32],
    date: GameDate,
) -> Result<Value, String> {
    let until = world
        .make_peace(player, enemy, provinces, date)
        .map_err(|error| error.to_string())?;
    Ok(json!({ "ok": true, "truce_until": until.to_string() }))
}

pub fn advance(
    simulation: &mut Simulation,
    world: &mut World,
    days: u32,
    player: &str,
) -> DayAdvance {
    simulation.advance_days(world, days, Some(player))
}

fn army_value(world: &World, army: &Army, date: GameDate) -> Value {
    let max_morale = world.military_rules().combat.morale_milli();
    let (infantry, infantry_strength) = army.composition(UnitKind::Infantry);
    let (cavalry, cavalry_strength) = army.composition(UnitKind::Cavalry);
    json!({
        "id": army.id(),
        "country": army.country,
        "province": army.province,
        "strength": army.strength(),
        "morale": max_morale.map(|max| army.average_morale(max)),
        "infantry": { "regiments": infantry, "strength": infantry_strength },
        "cavalry": { "regiments": cavalry, "strength": cavalry_strength },
        "retreating": army.is_retreating(),
        "exiled": army.is_exiled(),
        "battle_id": army.battle_id(),
        "combat": world.army_combat_status(army.id()),
        "movement": army.movement.as_ref().map(|movement| json!({
            "destination": movement.destination(),
            "arrives_on": movement.arrives_on().to_string(),
            "days_remaining": movement.days_remaining(date),
        })),
    })
}

fn battle_value(battle: &augustus_core::Battle) -> Value {
    json!({
        "id": battle.id(),
        "province": battle.province(),
        "started_on": battle.started_on().to_string(),
        "elapsed_days": battle.elapsed_days(),
        "attacker_armies": battle.side_armies(0),
        "defender_armies": battle.side_armies(1),
        "attacker_losses": battle.strength_lost(0),
        "defender_losses": battle.strength_lost(1),
    })
}

fn war_value(war: &War) -> Value {
    json!({
        "id": war.id(),
        "attacker": war.attacker(),
        "defender": war.defender(),
        "started_on": war.started_on().to_string(),
    })
}

fn peace_value(sheet: &PeaceSheet) -> Value {
    json!({
        "score": format_warscore_milli(sheet.score.total_milli),
        "score_milli": sheet.score.total_milli,
        "battle_score": format_warscore_milli(sheet.score.battle_milli),
        "occupation_score": format_warscore_milli(sheet.score.occupation_milli),
        "white_peace": {
            "accepted": sheet.white_peace.accepted,
            "reason": sheet.white_peace.reason,
            "truce_until": sheet.white_peace.truce_until.to_string(),
        },
        "provinces": sheet.provinces.iter().map(|province| json!({
            "id": province.province,
            "development": province.development,
            "cost": format_warscore_milli(province.cost_milli),
            "capital": province.capital,
            "controlled_by_offerer": province.controlled_by_offerer,
        })).collect::<Vec<_>>(),
    })
}

fn budget_value(budget: MonthlyBudget) -> Value {
    json!({
        "tax": money(budget.tax),
        "army_maintenance": money(budget.army_maintenance),
        "balance": money(budget.balance()),
    })
}

fn province_kind(province: &Province) -> &'static str {
    use augustus_core::ProvinceKind;
    match province.kind {
        ProvinceKind::Land => "land",
        ProvinceKind::Sea => "sea",
        ProvinceKind::Lake => "lake",
        ProvinceKind::Wasteland => "wasteland",
    }
}

fn money(value: Money) -> String {
    value.to_string()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use augustus_core::{
        BuildingDefinition, CombatRules, Country, Development, GameDate, LandConnection,
        ManpowerRules, MilitaryRules, Money, Province, ProvinceKind, Simulation, World,
    };

    use super::{country_report, declare_war, move_army};

    fn rules() -> MilitaryRules {
        MilitaryRules {
            infantry_cost: Money::from(10),
            cavalry_cost: Money::from(25),
            infantry_days: 60,
            cavalry_days: 90,
            infantry_speed: Money::from(1),
            cavalry_speed: Money::from(1),
            recruitment_full_refund_progress: Money::new(1, 1),
            land_maintenance_factor: Money::ZERO,
            monthly_reinforce: Money::ZERO,
            extra_land_reinforce_cost: Money::ZERO,
            supply_base: 6,
            base_land_forcelimit: Money::ZERO,
            development_land_forcelimit: Money::ZERO,
            land_forcelimit_extra_cost_factor: Money::ZERO,
            goods_land_forcelimit: BTreeMap::new(),
            tax_recruitment_time: Money::ZERO,
            starting_army_size: Money::ZERO,
            starting_army_size_at_war: Money::ZERO,
            manpower: ManpowerRules {
                base_capacity: 1_000,
                per_development: 0,
                recovery_months: 10,
                minimum_monthly: 1,
                infantry_cost: 1_000,
                cavalry_cost: 1_000,
            },
            combat: CombatRules::baseline(),
        }
    }

    fn owned(tag: &str) -> Province {
        let mut province = Province::new(ProvinceKind::Land);
        province.owner = Some(tag.to_owned());
        province.controller = Some(tag.to_owned());
        province.development = Development {
            tax: Some(1),
            production: Some(1),
            manpower: Some(1),
        };
        province
    }

    fn sample() -> (World, Simulation) {
        let world = World::new(
            [Country::new("AAA"), Country::new("BBB")],
            BTreeMap::from([(1, owned("AAA")), (2, owned("BBB"))]),
            [1, 2],
            Vec::<BuildingDefinition>::new(),
            rules(),
            Vec::<LandConnection>::new(),
        )
        .unwrap();
        let simulation = Simulation::new(GameDate::new(1444, 11, 11).unwrap());
        (world, simulation)
    }

    #[test]
    fn country_query_and_illegal_commands_do_not_need_eu4_files() {
        let (mut world, simulation) = sample();
        let date = simulation.clock().date();
        let country = country_report(&world, "AAA", "Aaa").unwrap();
        assert_eq!(country["tag"], "AAA");
        assert_eq!(country["name"], "Aaa");
        assert_eq!(country["treasury"], "0");
        assert!(country_report(&world, "ZZZ", "缺失").is_err());

        let error = move_army(&mut world, "AAA", 9, 2, date).unwrap_err();
        assert_eq!(error, "请选择有效的军队");
        let error = declare_war(&mut world, "AAA", "AAA", date).unwrap_err();
        assert_eq!(error, "不能向自己宣战");
        assert!(world.wars_of("AAA").next().is_none());
    }
}
