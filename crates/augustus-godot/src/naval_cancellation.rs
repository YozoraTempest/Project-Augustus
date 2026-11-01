// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::NavalConstructionId;
use augustus_eu4::VanillaWorld;

pub(crate) fn cancel_naval_construction(
    world: &mut VanillaWorld,
    country: &str,
    order_id: &str,
) -> Result<String, String> {
    if world.initial.country_entity(country).is_none() {
        return Err("unknown country".into());
    }
    let id = parse_order_id(order_id)?;
    let order = world
        .naval_construction_orders()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|order| order.id == id)
        .ok_or_else(|| format!("naval construction {order_id} is not pending"))?;
    if order.country != country {
        return Err(format!(
            "naval construction {order_id} does not belong to country {country}"
        ));
    }
    world
        .cancel_naval_constructions(&[id])
        .map_err(|error| error.to_string())?;
    Ok(id.0.to_string())
}

fn parse_order_id(value: &str) -> Result<NavalConstructionId, String> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("naval construction ID must be an unsigned decimal string".into());
    }
    let id = value
        .parse::<u64>()
        .map_err(|_| "naval construction ID is outside the u64 range".to_owned())?;
    Ok(NavalConstructionId(id))
}

#[cfg(test)]
mod tests {
    use super::{cancel_naval_construction, parse_order_id};
    use augustus_core::NavalConstructionId;
    use augustus_eu4::{
        ContentSet, TextEncoding, VanillaNavalConstructionOrderView, VanillaWorld,
        read_vanilla_naval_constructions_for_queue_import,
    };
    use std::path::Path;

    fn imported_world() -> VanillaWorld {
        let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
        let game_root = examples.join("world");
        let save_path = examples.join("naval-queues/save.eu4");
        let content =
            ContentSet::from_installation(&game_root, &[], None, None, TextEncoding::Utf8).unwrap();
        let mut world = VanillaWorld::load_from_content_with_save(&content, &save_path).unwrap();
        let queue_report = read_vanilla_naval_constructions_for_queue_import(&save_path).unwrap();
        world
            .import_naval_construction_queues(&queue_report)
            .unwrap();
        world
    }

    fn first_aaa_order(world: &VanillaWorld) -> VanillaNavalConstructionOrderView {
        world
            .naval_construction_orders()
            .unwrap()
            .into_iter()
            .find(|order| order.country == "AAA")
            .unwrap()
    }

    fn treasury(world: &VanillaWorld, country: &str) -> Option<i64> {
        let entity = world.initial.country_entity(country)?;
        world
            .initial
            .world()
            .country_resources()
            .get(&entity)
            .and_then(|resources| resources.treasury_milli)
    }

    #[test]
    fn cancellation_restores_the_remaining_fifo_order_and_does_not_refund() {
        let mut world = imported_world();

        world.record_country_expense("AAA", 27, 750).unwrap();
        world.record_delayed_country_expense("AAA", 27, 30).unwrap();
        let initial_orders = world.naval_construction_orders().unwrap();
        let cancelled = initial_orders
            .iter()
            .find(|order| order.country == "AAA")
            .unwrap()
            .clone();
        let remaining = initial_orders
            .iter()
            .find(|order| order.country == "AAA" && order.id != cancelled.id)
            .unwrap()
            .clone();
        let units_before = world.initial.naval_units().len();
        let treasury_before = treasury(&world, "AAA");
        let ledger_before = world.initial.world().country_ledgers().clone();
        assert!(!ledger_before.is_empty());

        let cancelled_id = cancelled.id.0.to_string();
        assert_eq!(
            cancel_naval_construction(&mut world, "AAA", &cancelled_id).unwrap(),
            cancelled_id
        );
        let current = world.naval_construction_orders().unwrap();
        assert_eq!(current.len(), 1);
        assert_eq!(current[0].id, remaining.id);
        assert_eq!(current[0].queue_position, Some(1));
        assert_eq!(current[0].completion_day, remaining.completion_day);
        assert_eq!(treasury(&world, "AAA"), treasury_before);
        assert_eq!(world.initial.world().country_ledgers(), &ledger_before);

        let cancelled_snapshot = world.save().unwrap();
        let mut restored = imported_world();
        restored.restore(&cancelled_snapshot).unwrap();
        assert_eq!(
            restored.save().unwrap().as_bytes(),
            cancelled_snapshot.as_bytes()
        );
        assert_eq!(
            restored
                .naval_construction_orders()
                .unwrap()
                .iter()
                .map(|order| order.id)
                .collect::<Vec<_>>(),
            [remaining.id]
        );

        let due_day = remaining.completion_day;
        let current_day = restored.initial.world().day();
        assert!(due_day > current_day);
        restored.advance(due_day - current_day).unwrap();
        assert!(restored.naval_construction_orders().unwrap().is_empty());
        assert_eq!(restored.initial.naval_units().len(), units_before + 1);
        assert_eq!(
            restored.initial.naval_units().last().unwrap().unit_type,
            remaining.unit_type
        );

        let after_second_cancel = world.save().unwrap();
        assert!(cancel_naval_construction(&mut world, "AAA", &cancelled_id).is_err());
        assert_eq!(
            world.save().unwrap().as_bytes(),
            after_second_cancel.as_bytes()
        );
    }

    #[test]
    fn invalid_country_and_order_identifiers_leave_snapshot_unchanged() {
        let mut world = imported_world();
        let first = first_aaa_order(&world);
        let order_id = first.id.0.to_string();
        let before = world.save().unwrap();

        assert!(cancel_naval_construction(&mut world, "ZZZ", &order_id).is_err());
        assert_eq!(world.save().unwrap().as_bytes(), before.as_bytes());
        assert!(cancel_naval_construction(&mut world, "BBB", &order_id).is_err());
        assert_eq!(world.save().unwrap().as_bytes(), before.as_bytes());

        for invalid in [
            "",
            "-1",
            "+1",
            "1.0",
            "1x",
            " 1",
            "1 ",
            "18446744073709551616",
        ] {
            assert!(
                cancel_naval_construction(&mut world, "AAA", invalid).is_err(),
                "{invalid:?}"
            );
            assert_eq!(
                world.save().unwrap().as_bytes(),
                before.as_bytes(),
                "{invalid:?}"
            );
        }

        let after_completion = {
            let current_day = world.initial.world().day();
            assert!(first.completion_day > current_day);
            world.advance(first.completion_day - current_day).unwrap();
            world.save().unwrap()
        };
        assert!(cancel_naval_construction(&mut world, "AAA", &order_id).is_err());
        assert_eq!(
            world.save().unwrap().as_bytes(),
            after_completion.as_bytes()
        );
    }

    #[test]
    fn order_identifier_parser_preserves_the_full_u64_range() {
        assert_eq!(
            parse_order_id("18446744073709551615").unwrap(),
            NavalConstructionId(u64::MAX)
        );
        assert!(parse_order_id("18446744073709551616").is_err());
        assert!(parse_order_id("-1").is_err());
        assert!(parse_order_id("1.0").is_err());
    }
}
