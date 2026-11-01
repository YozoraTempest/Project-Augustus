// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use augustus_core::{
    Action, AttributeEvaluation, EntityKind, EntityRef, MONTHLY_AUTONOMY_DELTA_ATTRIBUTE,
    ScopeSelector, Target,
};
use augustus_eu4::{VanillaExternalState, VanillaWorld};
use tempfile::TempDir;
use walkdir::WalkDir;

const OCCUPIED_LINE: &str =
    "occupied = { local_tax_modifier = -0.5 local_manpower_modifier = -0.5 }";

// Synthetic test values exercise runtime wiring; they do not claim to reproduce EU4 values.
const SYNTHETIC_OCCUPIED: &str = "occupied = { local_tax_modifier = -0.5 local_manpower_modifier = -0.5 trade_goods_size_modifier = -0.25 local_fort_maintenance_modifier = -0.2 local_autonomy = 0.05 }";

const OCCUPATION_FORMULAS: [(&str, &str); 5] = [
    ("tax_basis", "local_tax_modifier"),
    ("manpower_contribution", "local_manpower_modifier"),
    ("trade_goods_size_modifier", "trade_goods_size_modifier"),
    (
        "local_fort_maintenance_modifier",
        "local_fort_maintenance_modifier",
    ),
    (MONTHLY_AUTONOMY_DELTA_ATTRIBUTE, "local_autonomy"),
];

#[derive(Clone, Copy)]
enum OccupiedFixture<'a> {
    Defined(&'a str),
    Missing,
    Duplicate(&'a str),
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

fn copy_fixture() -> TempDir {
    let temp = tempfile::tempdir().unwrap();
    for entry in WalkDir::new(fixture()) {
        let entry = entry.unwrap();
        let target = temp
            .path()
            .join(entry.path().strip_prefix(fixture()).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(target).unwrap();
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
    temp
}

fn load_world(
    occupied_fixture: OccupiedFixture<'_>,
    initial_controller_known: bool,
) -> (TempDir, VanillaWorld) {
    let temp = copy_fixture();
    let static_path = temp.path().join("common/static_modifiers/test_static.txt");
    let static_source = fs::read_to_string(&static_path).unwrap();
    let revised_static = match occupied_fixture {
        OccupiedFixture::Defined(occupied_definition) => {
            static_source.replacen(OCCUPIED_LINE, occupied_definition, 1)
        }
        OccupiedFixture::Missing => static_source.replacen(OCCUPIED_LINE, "", 1),
        OccupiedFixture::Duplicate(duplicate_definition) => {
            format!("{static_source}\n{duplicate_definition}\n")
        }
    };
    assert_ne!(static_source, revised_static);
    fs::write(static_path, revised_static).unwrap();

    if !initial_controller_known {
        let province_path = temp.path().join("history/provinces/1 - North.txt");
        let province_source = fs::read_to_string(&province_path).unwrap();
        let revised_province = province_source
            .replace("controller = AAA", "")
            .replace("controller = BBB", "");
        assert_ne!(province_source, revised_province);
        fs::write(province_path, revised_province).unwrap();
    }

    let world = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    (temp, world)
}

fn controller_action(country: &str) -> Action {
    Action::SetProvinceController {
        target: Target::Current,
        controller: Some(EntityRef::Selected(ScopeSelector::Named {
            kind: EntityKind::Country,
            name: country.into(),
        })),
    }
}

fn absent_controller_action() -> Action {
    Action::SetProvinceController {
        target: Target::Current,
        controller: None,
    }
}

fn evaluate(world: &VanillaWorld, attribute: &str) -> AttributeEvaluation {
    world
        .province(1)
        .unwrap()
        .attributes
        .get(attribute)
        .unwrap()
        .clone()
}

fn has_occupied_contribution(evaluation: &AttributeEvaluation, property: &str) -> bool {
    evaluation
        .steps
        .iter()
        .flat_map(|step| &step.contributions)
        .any(|contribution| {
            contribution.source == "static:occupied"
                && contribution.property == property
                && contribution.micros != 0
        })
}

fn has_controller_unknown(evaluation: &AttributeEvaluation) -> bool {
    evaluation
        .unresolved
        .iter()
        .any(|reason| reason.contains("province occupation controller is unknown"))
}

#[test]
fn occupied_modifiers_follow_controller_changes_across_all_supported_formulas() {
    let (_temp, mut world) = load_world(OccupiedFixture::Defined(SYNTHETIC_OCCUPIED), true);
    let province = world.initial.province_entity(1).unwrap();
    let owner = world.initial.country_entity("BBB").unwrap();
    let initial_controller = world.initial.country_entity("BBB").unwrap();
    let foreign_controller = world.initial.country_entity("AAA").unwrap();
    let baseline = OCCUPATION_FORMULAS
        .iter()
        .map(|(attribute, _)| (*attribute, evaluate(&world, attribute)))
        .collect::<std::collections::BTreeMap<_, _>>();

    assert_eq!(
        world
            .initial
            .world()
            .province_controller_state(province)
            .unwrap(),
        Some(Some(initial_controller))
    );
    for (attribute, property) in OCCUPATION_FORMULAS {
        let before = &baseline[attribute];
        if attribute == MONTHLY_AUTONOMY_DELTA_ATTRIBUTE {
            assert!(before.known_subtotal_micros.is_none());
            assert!(
                before
                    .unresolved
                    .iter()
                    .any(|reason| reason.contains("unknown political status at_war"))
            );
        } else {
            assert!(before.known_subtotal_micros.is_some(), "{attribute}");
        }
        assert!(
            !has_occupied_contribution(before, property),
            "{attribute} should not include occupied effects while owner and controller match"
        );
    }

    world
        .initial
        .execute(province, &[controller_action("AAA")])
        .unwrap();
    assert_eq!(
        world.initial.world().entity(province).unwrap().owner,
        Some(owner)
    );
    assert_eq!(
        world
            .initial
            .world()
            .province_controller_state(province)
            .unwrap(),
        Some(Some(foreign_controller))
    );
    for (attribute, property) in OCCUPATION_FORMULAS {
        let occupied = evaluate(&world, attribute);
        assert!(
            has_occupied_contribution(&occupied, property),
            "{attribute}"
        );
        if attribute == MONTHLY_AUTONOMY_DELTA_ATTRIBUTE {
            assert!(occupied.known_subtotal_micros.is_none());
            assert_eq!(occupied.unresolved, baseline[attribute].unresolved);
            assert_eq!(
                occupied.steps[0].flat_micros - baseline[attribute].steps[0].flat_micros,
                500
            );
        } else {
            assert_ne!(
                occupied.known_subtotal_micros, baseline[attribute].known_subtotal_micros,
                "{attribute} should recalculate after the controller changes"
            );
            assert!(occupied.known_subtotal_micros.is_some(), "{attribute}");
        }
    }
    assert!(
        evaluate(&world, "tax_basis").value_micros.is_none(),
        "the tax formula remains incomplete beyond the occupied modifier"
    );

    world
        .initial
        .execute(province, &[controller_action("BBB")])
        .unwrap();
    for (attribute, property) in OCCUPATION_FORMULAS {
        let returned = evaluate(&world, attribute);
        assert_eq!(
            returned.known_subtotal_micros, baseline[attribute].known_subtotal_micros,
            "{attribute} should return to its same-owner subtotal"
        );
        assert_eq!(returned.steps, baseline[attribute].steps, "{attribute}");
        assert!(
            !has_occupied_contribution(&returned, property),
            "{attribute}"
        );
    }
}

#[test]
fn scheduled_occupation_survives_snapshot_restore_and_continues() {
    let (temp, mut continuous) = load_world(OccupiedFixture::Defined(SYNTHETIC_OCCUPIED), true);
    let province = continuous.initial.province_entity(1).unwrap();
    continuous
        .initial
        .execute(
            province,
            &[Action::Schedule {
                delay: 1,
                plan: vec![controller_action("AAA")],
            }],
        )
        .unwrap();
    let scheduled_snapshot = continuous.save().unwrap();

    let mut resumed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    resumed.restore(&scheduled_snapshot).unwrap();
    assert!(!has_occupied_contribution(
        &evaluate(&resumed, "tax_basis"),
        "local_tax_modifier"
    ));

    continuous.advance(1).unwrap();
    resumed.advance(1).unwrap();
    assert!(has_occupied_contribution(
        &evaluate(&resumed, "tax_basis"),
        "local_tax_modifier"
    ));
    assert_eq!(continuous.save().unwrap(), resumed.save().unwrap());
}

#[test]
fn unknown_controller_is_unresolved_until_action_establishes_known_controller() {
    let (_temp, mut world) = load_world(OccupiedFixture::Defined(SYNTHETIC_OCCUPIED), false);
    let province = world.initial.province_entity(1).unwrap();
    let before = evaluate(&world, "tax_basis");
    assert!(has_controller_unknown(&before));
    let unrelated_unknowns = before
        .unresolved
        .iter()
        .filter(|reason| !reason.contains("province occupation controller is unknown"))
        .cloned()
        .collect::<BTreeSet<_>>();
    assert!(
        !unrelated_unknowns.is_empty(),
        "the fixture should retain unrelated incomplete tax sources"
    );

    world
        .initial
        .execute(province, &[controller_action("BBB")])
        .unwrap();
    let after = evaluate(&world, "tax_basis");
    assert!(!has_controller_unknown(&after));
    assert_eq!(after.unresolved, unrelated_unknowns);
    assert!(!has_occupied_contribution(&after, "local_tax_modifier"));
}

#[test]
fn known_absent_controller_is_false_and_distinct_from_unknown() {
    let (temp, historical) = load_world(OccupiedFixture::Defined(SYNTHETIC_OCCUPIED), true);
    let province = historical.initial.province_entity(1).unwrap();
    assert_eq!(
        historical
            .initial
            .world()
            .province_controller_state(province)
            .unwrap(),
        Some(Some(historical.initial.country_entity("BBB").unwrap()))
    );
    let state = VanillaExternalState {
        province_controllers: [(1, None)].into(),
        ..Default::default()
    };
    let mut world =
        VanillaWorld::load_with_external_state(temp.path(), "1444.11.11", &state).unwrap();
    let province = world.initial.province_entity(1).unwrap();
    assert_eq!(
        world
            .initial
            .world()
            .province_controller_state(province)
            .unwrap(),
        None
    );
    let initially_unknown = evaluate(&world, "tax_basis");
    assert!(has_controller_unknown(&initially_unknown));

    world
        .initial
        .execute(province, &[absent_controller_action()])
        .unwrap();
    assert_eq!(
        world
            .initial
            .world()
            .province_controller_state(province)
            .unwrap(),
        Some(None)
    );
    let known_absent = evaluate(&world, "tax_basis");
    assert!(!has_controller_unknown(&known_absent));
    assert!(!has_occupied_contribution(
        &known_absent,
        "local_tax_modifier"
    ));
}

#[test]
fn ambiguous_occupied_property_is_unresolved_only_while_occupied() {
    let ambiguous = "occupied = { local_tax_modifier = -0.25 local_tax_modifier = -0.5 }";
    let (_temp, mut world) = load_world(OccupiedFixture::Defined(ambiguous), true);
    let province = world.initial.province_entity(1).unwrap();
    let inactive = evaluate(&world, "tax_basis");
    assert!(
        !inactive
            .unresolved
            .iter()
            .any(|reason| reason.contains("ambiguous:static_modifiers:occupied:local_tax_modifier")),
        "an inactive occupied modifier must not invalidate the tax formula"
    );

    world
        .initial
        .execute(province, &[controller_action("AAA")])
        .unwrap();
    let active = evaluate(&world, "tax_basis");
    assert!(active.unresolved.iter().any(|reason| {
        reason.contains("ambiguous:static_modifiers:occupied:local_tax_modifier")
    }));

    world
        .initial
        .execute(province, &[controller_action("BBB")])
        .unwrap();
    let inactive_again = evaluate(&world, "tax_basis");
    assert!(!inactive_again.unresolved.iter().any(|reason| {
        reason.contains("ambiguous:static_modifiers:occupied:local_tax_modifier")
    }));
}

#[test]
fn occupied_definition_diagnostics_are_unresolved_only_while_occupied() {
    let cases = [
        (
            "missing definition",
            OccupiedFixture::Missing,
            "missing_static_modifier:occupied",
        ),
        (
            "ambiguous definition",
            OccupiedFixture::Duplicate("occupied = { local_tax_modifier = -0.25 }"),
            "ambiguous_static_modifier:occupied",
        ),
        (
            "unsupported scalar",
            OccupiedFixture::Defined("occupied = { local_tax_modifier = unsupported }"),
            "unresolved:static_modifiers:occupied:local_tax_modifier",
        ),
        (
            "non-numeric flag",
            OccupiedFixture::Defined("occupied = { local_tax_modifier = yes }"),
            "non_numeric:static_modifiers:occupied:local_tax_modifier",
        ),
    ];

    for (case, fixture, expected_reason) in cases {
        let (_temp, mut world) = load_world(fixture, true);
        let province = world.initial.province_entity(1).unwrap();
        let inactive = evaluate(&world, "tax_basis");
        assert!(
            !inactive
                .unresolved
                .iter()
                .any(|reason| reason.contains(expected_reason)),
            "inactive {case} should not invalidate the tax formula"
        );

        world
            .initial
            .execute(province, &[controller_action("AAA")])
            .unwrap();
        let active = evaluate(&world, "tax_basis");
        assert!(
            active
                .unresolved
                .iter()
                .any(|reason| reason.contains(expected_reason)),
            "active {case} should report its unresolved source"
        );

        world
            .initial
            .execute(province, &[controller_action("BBB")])
            .unwrap();
        let inactive_again = evaluate(&world, "tax_basis");
        assert!(
            !inactive_again
                .unresolved
                .iter()
                .any(|reason| reason.contains(expected_reason)),
            "inactive {case} should clear its unresolved source"
        );
    }
}
