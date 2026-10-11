// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    fs,
    path::{Path, PathBuf},
};

use augustus_core::{EntityKind, ProvinceClass, Schema, World};
use augustus_eu4::VanillaWorld;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

fn copy_fixture() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    for entry in walkdir::WalkDir::new(fixture()) {
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

fn append_fixture(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    let previous = fs::read_to_string(&path).unwrap();
    fs::write(path, format!("{previous}\n{text}")).unwrap();
}

fn selected_idea(loaded: &VanillaWorld, tag: &str) -> Option<String> {
    loaded
        .country(tag)
        .unwrap()
        .ideas
        .unwrap()
        .groups
        .as_ref()
        .and_then(|groups| groups.keys().next().cloned())
}

#[test]
fn historical_federation_membership_is_not_a_generated_nation_and_later_origin_is_unknown() {
    let fixture = copy_fixture();
    append_fixture(
        fixture.path(),
        "history/countries/AAA - Alpha.txt",
        "federation = { name = test_union members = { AAA BBB } }",
    );
    fs::write(fixture.path().join("common/ideas/00_conditions.txt"),
        "historical = { free = yes trigger = { is_client_nation = no is_federation_nation = no } one = {} }").unwrap();
    let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert_eq!(selected_idea(&loaded, "AAA").as_deref(), Some("historical"));
    let later = VanillaWorld::load(fixture.path(), "1450.1.1").unwrap();
    assert_eq!(selected_idea(&later, "AAA"), None);
    assert!(later.report.ideas.unknown_statuses["AAA"].contains("federation_nation"));
    append_fixture(
        fixture.path(),
        "history/countries/AAA - Alpha.txt",
        "if = { limit = { always = yes } create_client_state = {} }",
    );
    let pending = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert_eq!(selected_idea(&pending, "BBB"), None);
    assert!(pending.report.ideas.unknown_statuses["BBB"].contains("client_nation"));
}

#[test]
fn colonial_identity_uses_the_current_subject_definition_and_ordered_inheritance() {
    let fixture = copy_fixture();
    fs::write(
        fixture.path().join("common/ideas/00_conditions.txt"),
        "colonial = { free = yes trigger = { is_colonial_nation = yes } one = {} }",
    )
    .unwrap();
    append_fixture(
        fixture.path(),
        "history/countries/AAA - Alpha.txt",
        "colonial_parent = BBB",
    );
    let subjects = fixture.path().join("common/subject_types/test.txt");
    fs::write(&subjects, "default = { has_colonial_parent = no } colony = {} colony = { copy_from = default has_colonial_parent = yes } test_vassal = { copy_from = colony }").unwrap();
    let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert_eq!(selected_idea(&loaded, "BBB").as_deref(), Some("colonial"));
    assert_eq!(selected_idea(&loaded, "AAA").as_deref(), Some("test_alpha"));
    fs::write(&subjects, "default = { has_colonial_parent = no } colony = { copy_from = default has_colonial_parent = yes } test_vassal = { has_colonial_parent = no copy_from = colony }").unwrap();
    let inherited = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert_eq!(
        selected_idea(&inherited, "BBB").as_deref(),
        Some("colonial")
    );
    fs::write(&subjects, "default = { has_colonial_parent = no } colony = { copy_from = default has_colonial_parent = yes } test_vassal = { copy_from = colony has_colonial_parent = no }").unwrap();
    let overridden = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert_eq!(
        selected_idea(&overridden, "BBB").as_deref(),
        Some("test_common")
    );
    let released = VanillaWorld::load(fixture.path(), "1450.1.1").unwrap();
    let beta = released.initial.country_entity("BBB").unwrap();
    assert_eq!(
        released
            .initial
            .world()
            .politics()
            .status(beta, "colonial_nation"),
        Some(false)
    );
}

#[test]
fn cyclic_or_duplicate_subject_definitions_keep_colonial_selection_unknown() {
    let fixture = copy_fixture();
    fs::write(
        fixture.path().join("common/ideas/00_conditions.txt"),
        "colonial = { free = yes trigger = { is_colonial_nation = yes } one = {} }",
    )
    .unwrap();
    for definitions in [
        "default = { has_colonial_parent = no } test_vassal = { copy_from = other } other = { copy_from = test_vassal }",
        "default = { has_colonial_parent = no } test_vassal = { has_colonial_parent = yes } test_vassal = { has_colonial_parent = no }",
    ] {
        fs::write(
            fixture.path().join("common/subject_types/test.txt"),
            definitions,
        )
        .unwrap();
        let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
        assert_eq!(selected_idea(&loaded, "BBB"), None);
        assert!(loaded.report.ideas.unknown_statuses["BBB"].contains("colonial_nation"));
    }
}

#[test]
fn revolutionary_reforms_and_revolution_target_history_are_distinct_conditions() {
    let fixture = copy_fixture();
    append_fixture(
        fixture.path(),
        "common/government_reforms/test_governments.txt",
        "test_revolution = { revolutionary = yes custom_attributes = { test_charter = yes } }",
    );
    append_fixture(
        fixture.path(),
        "history/countries/AAA - Alpha.txt",
        "1443.1.1 = { revolution_target = yes add_government_reform = test_revolution }\n1445.1.1 = { revolution_target = no }",
    );
    fs::write(fixture.path().join("common/ideas/00_conditions.txt"),
        "revolution = { free = yes trigger = { is_revolution_target = yes is_revolutionary = yes has_government_attribute = test_charter } one = {} }").unwrap();
    let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert_eq!(selected_idea(&loaded, "AAA").as_deref(), Some("revolution"));
    let later = VanillaWorld::load(fixture.path(), "1450.1.1").unwrap();
    assert_eq!(selected_idea(&later, "AAA").as_deref(), Some("test_alpha"));
    append_fixture(
        fixture.path(),
        "history/countries/BBB - Beta.txt",
        "revolution_target = yes",
    );
    let error = VanillaWorld::load(fixture.path(), "1444.11.11")
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("multiple revolution targets"));
}

#[test]
fn uncertain_reform_features_and_scoped_target_changes_do_not_become_false() {
    let fixture = copy_fixture();
    let reforms = fixture
        .path()
        .join("common/government_reforms/test_governments.txt");
    let original = fs::read_to_string(&reforms).unwrap();
    append_fixture(
        fixture.path(),
        "history/countries/AAA - Alpha.txt",
        "add_government_reform = conditional_charter",
    );
    fs::write(
        fixture.path().join("common/ideas/00_conditions.txt"),
        "charter = { free = yes trigger = { has_government_attribute = test_charter } one = {} }",
    )
    .unwrap();
    for body in [
        "custom_attributes = { test_charter = yes test_charter = no }",
        "custom_attributes = { test_charter = yes } conditional = { allow = { unsupported = yes } custom_attributes = { test_charter = no } }",
    ] {
        fs::write(
            &reforms,
            format!("{original}\nconditional_charter = {{ {body} }}"),
        )
        .unwrap();
        let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
        assert_eq!(selected_idea(&loaded, "AAA"), None);
        assert_eq!(
            selected_idea(&loaded, "BBB").as_deref(),
            Some("test_common")
        );
    }
    fs::write(&reforms, format!("{original}\nconditional_charter = {{ conditional = {{ allow = {{ always = yes }} custom_attributes = {{ test_charter = yes }} }} }}")).unwrap();
    let known = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert_eq!(selected_idea(&known, "AAA").as_deref(), Some("charter"));
    append_fixture(
        fixture.path(),
        "history/countries/AAA - Alpha.txt",
        "BBB = { revolution_target = yes }",
    );
    fs::write(
        fixture.path().join("common/ideas/00_conditions.txt"),
        "target = { free = yes trigger = { is_revolution_target = no } one = {} }",
    )
    .unwrap();
    let unknown = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert_eq!(selected_idea(&unknown, "BBB"), None);
}

#[test]
fn capital_geography_uses_shared_groups_and_the_bound_capital_with_scope_checks() {
    let fixture = copy_fixture();
    append_fixture(
        fixture.path(),
        "map/default.map",
        "region = \"region.txt\"\nsuperregion = \"superregion.txt\"",
    );
    fs::write(fixture.path().join("map/region.txt"),
        "home = { areas = { east_area } monsoon = { start = 6 end = 9 } monsoon = { start = 7 end = 8 } } away = { areas = { north_area sea_area } } empty = { }").unwrap();
    fs::write(
        fixture.path().join("map/superregion.txt"),
        "continent = { restrict_charter home } other = { away } vacant = {}",
    )
    .unwrap();
    let conditions = fixture.path().join("common/ideas/00_conditions.txt");
    fs::write(&conditions, "geographic = { free = yes trigger = { capital_scope = { region = home superregion = continent } } one = {} }").unwrap();
    let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert_eq!(selected_idea(&loaded, "AAA").as_deref(), Some("geographic"));
    assert_eq!(
        selected_idea(&loaded, "BBB").as_deref(),
        Some("test_common")
    );
    assert_eq!(loaded.report.administration.regions, 3);
    assert_eq!(loaded.report.administration.superregions, 3);
    assert_eq!(
        loaded
            .report
            .administration
            .unapplied_geography_fields
            .len(),
        3
    );
    let later = VanillaWorld::load(fixture.path(), "1450.1.1").unwrap();
    assert_eq!(selected_idea(&later, "AAA").as_deref(), Some("test_alpha"));
    for trigger in [
        "region = home",
        "capital_scope = { tag = AAA }",
        "capital_scope = { superregion = missing }",
    ] {
        fs::write(
            &conditions,
            format!("geographic = {{ free = yes trigger = {{ {trigger} }} one = {{}} }}"),
        )
        .unwrap();
        let unknown = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
        assert_eq!(selected_idea(&unknown, "AAA"), None);
    }
    fs::write(
        &conditions,
        "geographic = { free = yes trigger = { capital_scope = { region = home } } one = {} }",
    )
    .unwrap();
    append_fixture(
        fixture.path(),
        "history/countries/AAA - Alpha.txt",
        "1444.11.11 = { capital = 1 }",
    );
    let invalid_capital = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert_eq!(selected_idea(&invalid_capital, "AAA"), None);
    assert!(
        invalid_capital.report.ideas.unresolved_countries["AAA"]
            .contains("candidate:geographic:unknown_capital")
    );
}

#[test]
fn ambiguous_or_dangling_geographic_memberships_fail_with_source() {
    let fixture = copy_fixture();
    append_fixture(fixture.path(), "map/default.map", "region = \"region.txt\"");
    let path = fixture.path().join("map/region.txt");
    for groups in [
        "home = { areas = { east_area } areas = { north_area } }",
        "home = { areas = { east_area } } home = { areas = { north_area } }",
        "home = { areas = { missing } }",
        "home = { areas = { east_area } } away = { areas = { east_area } }",
    ] {
        fs::write(&path, groups).unwrap();
        assert!(VanillaWorld::load(fixture.path(), "1444.11.11").is_err());
    }
    fs::write(
        &path,
        "home = { areas = { east_area } areas = { north_area } }",
    )
    .unwrap();
    let error = VanillaWorld::load(fixture.path(), "1444.11.11")
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("region.txt") && error.contains("one area list"));
}

#[test]
fn national_traditions_select_from_flags_and_ruler_and_progress_stays_explicit() {
    let loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let alpha = loaded.country("AAA").unwrap();
    assert_eq!(
        alpha.ideas.unwrap().groups.as_ref().unwrap()["test_alpha"],
        Some(0)
    );
    assert_eq!(
        alpha.declared_idea_effects.known_totals_micros["test_tradition"],
        125_000
    );
    assert!(
        !alpha
            .declared_idea_effects
            .known_totals_micros
            .contains_key("global_tax_modifier")
    );
    let later = VanillaWorld::load(fixture(), "1450.1.1").unwrap();
    assert_eq!(
        later
            .country("AAA")
            .unwrap()
            .ideas
            .unwrap()
            .groups
            .as_ref()
            .unwrap()["test_alpha"],
        None
    );
    assert!(later.report.ideas.unresolved_countries["AAA"].contains("unknown_historical_progress"));
}

#[test]
fn earlier_unknown_candidate_blocks_fallback_but_false_and_true_short_circuit() {
    let fixture = copy_fixture();
    let file = fixture.path().join("common/ideas/00_conditions.txt");
    fs::write(&file, "first = { free = yes trigger = { OR = { unsupported = yes tag = AAA } } start = { global_tax_modifier = 0.125 } one = {} }\nsecond = { free = yes trigger = { AND = { unsupported = yes tag = AAA } } one = {} }").unwrap();
    let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .ideas
            .unwrap()
            .groups
            .as_ref()
            .unwrap()
            .contains_key("first")
    );
    assert!(
        loaded
            .country("BBB")
            .unwrap()
            .ideas
            .unwrap()
            .groups
            .is_none()
    );
    assert!(
        loaded.report.ideas.unresolved_countries["BBB"]
            .contains("candidate:first:unsupported_trigger:unsupported")
    );
    fs::write(
        &file,
        "first = { free = yes trigger = { AND = { unsupported = yes tag = AAA } } one = {} }",
    )
    .unwrap();
    let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .ideas
            .unwrap()
            .groups
            .is_none()
    );
    assert!(
        loaded
            .country("BBB")
            .unwrap()
            .ideas
            .unwrap()
            .groups
            .as_ref()
            .unwrap()
            .contains_key("test_common")
    );
}

#[test]
fn flag_history_is_ordered_and_unknown_scoped_mutations_are_not_assumed_false() {
    let fixture = copy_fixture();
    let path = fixture.path().join("history/countries/AAA - Alpha.txt");
    let history = fs::read_to_string(&path).unwrap();
    fs::write(&path, format!("{history}\n1443.1.1 = {{ clr_country_flag = not_implemented }}\n1445.1.1 = {{ set_country_flag = not_implemented }}")).unwrap();
    let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .ideas
            .unwrap()
            .groups
            .as_ref()
            .unwrap()
            .contains_key("test_common")
    );
    fs::write(
        &path,
        format!(
            "{history}\nif = {{ limit = {{ always = yes }} clr_country_flag = not_implemented }}"
        ),
    )
    .unwrap();
    let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .ideas
            .unwrap()
            .groups
            .is_none()
    );
    fs::write(&path, format!("{history}\nadd_idea_group = test_common")).unwrap();
    let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert!(
        loaded.report.ideas.unresolved_countries["AAA"].contains("unapplied_history_selection")
    );
}

#[test]
fn duplicate_properties_preserve_missing_effects_and_duplicate_groups_fail_with_source() {
    let fixture = copy_fixture();
    let path = fixture.path().join("common/ideas/00_conditions.txt");
    fs::write(&path, "first = { free = yes trigger = { NOT = { tag = BBB tag = CCC } } start = { global_tax_modifier = 0.25 global_tax_modifier = 0.5 } one = {} }").unwrap();
    let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    let effects = loaded.country("AAA").unwrap().declared_idea_effects;
    assert!(
        effects
            .unresolved
            .contains("ideas:traditions:first:unresolved:global_tax_modifier")
    );
    assert!(
        !effects
            .known_totals_micros
            .contains_key("global_tax_modifier")
    );
    assert!(
        loaded
            .country("BBB")
            .unwrap()
            .ideas
            .unwrap()
            .groups
            .as_ref()
            .unwrap()
            .contains_key("test_common")
    );
    fs::write(&path, "test_alpha = { free = yes one = {} }").unwrap();
    let error = VanillaWorld::load(fixture.path(), "1444.11.11")
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("ambiguous idea group definition") && error.contains("test_ideas.txt"));
}

#[test]
fn translated_and_native_progress_have_the_same_live_attribute_effects() {
    let mut loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let province = loaded.initial.province_entity(2).unwrap();
    let alpha = loaded.initial.country_entity("AAA").unwrap();
    let before = loaded
        .initial
        .world()
        .attributes()
        .evaluate(loaded.initial.world(), province, "tax_basis")
        .unwrap();
    let mut ideas = loaded.initial.world().ideas().clone();
    ideas
        .countries
        .get_mut(&alpha)
        .unwrap()
        .groups
        .as_mut()
        .unwrap()
        .insert("test_alpha".into(), Some(2));
    loaded.initial.set_ideas(ideas).unwrap();
    let after = loaded
        .initial
        .world()
        .attributes()
        .evaluate(loaded.initial.world(), province, "tax_basis")
        .unwrap();
    assert!(after.known_subtotal_micros > before.known_subtotal_micros);
    assert!(
        after.steps[0]
            .contributions
            .iter()
            .any(|c| c.source == "ideas:completion:test_alpha" && c.micros == 500_000)
    );
    let saved = loaded.save().unwrap();
    loaded.restore(&saved).unwrap();
    assert_eq!(
        after,
        loaded
            .initial
            .world()
            .attributes()
            .evaluate(loaded.initial.world(), province, "tax_basis")
            .unwrap()
    );
}

#[test]
fn uppercase_tag_paid_groups_and_unlock_scripts_keep_their_actual_scope() {
    let fixture = copy_fixture();
    fs::write(fixture.path().join("common/ideas/00_conditions.txt"), "paid = { category = ADM trigger = { always = yes } one = { global_tax_modifier = 5 } }\nfirst = { free = yes trigger = { TAG = AAA } start = { global_tax_modifier = 0.125 } one = { effect = { add_country_modifier = { name = deferred } } } bonus = {} }").unwrap();
    let mut loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert_eq!(loaded.report.ideas.skipped_paid_groups, 1);
    assert!(!loaded.initial.world().ideas().groups.contains_key("paid"));
    assert_eq!(
        loaded
            .country("AAA")
            .unwrap()
            .declared_idea_effects
            .known_totals_micros["global_tax_modifier"],
        125_000
    );
    let entity = loaded.initial.country_entity("AAA").unwrap();
    assert!(
        loaded
            .initial
            .world()
            .ideas()
            .property_effects(entity, "global_tax_modifier")
            .unwrap()
            .unresolved
            .is_empty()
    );
    let mut ideas = loaded.initial.world().ideas().clone();
    ideas
        .countries
        .get_mut(&entity)
        .unwrap()
        .groups
        .as_mut()
        .unwrap()
        .insert("first".into(), Some(1));
    loaded.initial.set_ideas(ideas).unwrap();
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .declared_idea_effects
            .unresolved
            .contains("ideas:idea:first:one:unexecuted:unlock_script")
    );
}

#[test]
fn dated_history_reaches_shared_native_state_and_preserves_provenance() {
    let loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let province = loaded.initial.province(1).unwrap();
    assert_eq!(province.owner.as_deref(), Some("BBB"));
    assert_eq!(province.controller.as_deref(), Some("BBB"));
    assert_eq!(province.cores, ["BBB"]);
    assert_eq!(province.tax_milli, Some(2125));
    assert_eq!(province.manpower_milli, Some(3500));
    assert_eq!(loaded.initial.bound_capital("AAA"), Some(2));
    assert_eq!(loaded.initial.bound_capital("BBB"), Some(1));
    assert_eq!(loaded.report.ruleset, "eu4-final-all-dlc");
    assert_eq!(
        loaded.report.unapplied_country_fields["set_country_flag"],
        1
    );
    assert_eq!(loaded.report.unapplied_province_fields["discovered_by"], 1);
    let origin = &loaded.report.unapplied_examples["province:discovered_by"];
    assert_eq!(origin.line, 16);
    assert!(origin.path.ends_with("history/provinces/1 - North.txt"));
    assert_eq!(loaded.province_sources[&1][0].line, 2);
    assert_eq!(loaded.province_sources[&1].len(), 2);

    // The same independent native input produces the same simulation entities,
    // relations and fixed-point values, without retaining an EU4 runtime.
    let schema = Schema::new([
        "province.tax_base",
        "province.production_base",
        "province.manpower_base",
    ])
    .unwrap();
    let mut native = World::new(schema);
    let alpha = native.add_entity("AAA", EntityKind::Country);
    let beta = native.add_entity("BBB", EntityKind::Country);
    let north = native.add_entity("1", EntityKind::Province);
    let east = native.add_entity("2", EntityKind::Province);
    for id in 3..=6 {
        native.add_entity(id.to_string(), EntityKind::Province);
    }
    native.set_owner(north, beta).unwrap();
    native.set_owner(east, alpha).unwrap();
    native.set_capital(alpha, east).unwrap();
    native.set_capital(beta, north).unwrap();
    for (entity, values) in [(north, [2125, 2000, 3500]), (east, [4000, 5000, 6000])] {
        for (name, value) in [
            "province.tax_base",
            "province.production_base",
            "province.manpower_base",
        ]
        .into_iter()
        .zip(values)
        {
            native
                .set(entity, native.schema().field(name).unwrap(), value)
                .unwrap();
        }
    }
    let politics = serde_json::from_str(r#"{
        "countries": {
            "0": {"statuses":{"client_nation":false,"colonial_nation":false,"federation_nation":false,"revolution_target":false},"ruler":{"name":"Iris","dynasty":"Sun","birth_date":{"year":1410,"month":1,"day":1},"administration":2,"diplomacy":3,"military":4},
                "heir":{"name":"Rowan","regnal_name":"Rowan I","dynasty":"Sun","birth_date":{"year":1440,"month":1,"day":1},"historical_death_date":{"year":1444,"month":11,"day":12},"administration":1,"diplomacy":2,"military":3,"claim_milli":87125,"block_disinherit":true},
                "consort":{"name":"Vale","country_of_origin":1,"female":true,"birth_date":{"year":1415,"month":1,"day":1},"historical_death_date":{"year":1450,"month":1,"day":1}}},
            "1": {"statuses":{"client_nation":false,"colonial_nation":false,"federation_nation":false,"revolution_target":false},"ruler":{"name":"Council","regent":true,"administration":0,"diplomacy":0,"military":0}}
        },
        "relations": [
            {"kind":"alliance","first":0,"second":1},
            {"kind":"guarantee","first":1,"second":0},
            {"kind":"warning","first":0,"second":1},
            {"kind":"trade_league","first":0,"second":1},
            {"kind":"subject","first":0,"second":1,"subject_type":"test_vassal"}
        ],
        "offices":{"imperial_emperor":0,"celestial_emperor":1}
    }"#).unwrap();
    native.set_politics(politics).unwrap();
    native
        .set_governments(
            serde_json::from_str(include_str!(
                "../../../examples/world/native_governments.json"
            ))
            .unwrap(),
        )
        .unwrap();
    native
        .set_ideas(
            serde_json::from_str(include_str!("../../../examples/world/native_ideas.json"))
                .unwrap(),
        )
        .unwrap();
    native
        .set_affiliations(
            serde_json::from_str(include_str!(
                "../../../examples/world/native_affiliations.json"
            ))
            .unwrap(),
        )
        .unwrap();
    native
        .set_economy(
            serde_json::from_str(
                r#"{
        "buildings": {
            "test_market":{"cost_milli":87125,"construction_months":9},
            "test_fort":{"cost_milli":200000,"construction_months":18}
        },
        "trade_goods": {
            "grain":{"base_price_milli":2125},
            "cloth":{"base_price_milli":3000},
            "gold":{"base_price_milli":0,"gold_type":true}
        },
        "trade_nodes": {
            "river":{"location":2,"members":[2],"outgoing":["harbor"],"inland":true},
            "harbor":{"location":3,"members":[3,4],"outgoing":[],"end":true}
        },
        "provinces": {
            "2":{"buildings":["test_fort"],"trade_good":"grain"},
            "3":{"buildings":["test_market"],"trade_good":"cloth"},
            "5":{"buildings":[]}
        }
    }"#,
            )
            .unwrap(),
        )
        .unwrap();
    let mut technology = augustus_core::Technology::default();
    for branch in ["administrative", "diplomatic", "military"] {
        let mut levels: Vec<_> = [1300, 1340, 1390, 1430]
            .into_iter()
            .map(|year| augustus_core::TechnologyLevel {
                year,
                values_micros: Default::default(),
                flags: Default::default(),
            })
            .collect();
        match branch {
            "administrative" => {
                levels[0].values_micros.insert("test_yield".into(), 12500);
                levels[3].flags.insert("test_capability".into(), true);
            }
            "diplomatic" => {
                levels[3].values_micros.insert("test_range".into(), 7125000);
            }
            _ => {
                levels[0]
                    .values_micros
                    .insert("test_strength".into(), 1250000);
            }
        }
        technology.tables.insert(branch.into(), levels);
    }
    technology.countries.insert(
        alpha,
        ["administrative", "diplomatic", "military"]
            .into_iter()
            .map(|b| (b.into(), 3))
            .collect(),
    );
    native.set_technology(technology).unwrap();
    native
        .set_country_resources(
            [
                (
                    alpha,
                    augustus_core::CountryResources {
                        mercantilism_milli: Some(18500),
                        ..Default::default()
                    },
                ),
                (beta, augustus_core::CountryResources::default()),
            ]
            .into_iter()
            .collect(),
        )
        .unwrap();
    native.set_modifiers(serde_json::from_str(r#"{
        "definitions": {
            "test_river":{"values_micros":{"test_trade_value":7125000,"test_growth":-2500},"flags":{}},
            "test_harbor":{"values_micros":{"test_trade_value":1125000},"flags":{"test_capability":true}},
            "test_state":{"values_micros":{"test_tax_modifier":12500},"flags":{}},
            "test_empty":{"values_micros":{},"flags":{}}
        },
        "active":{"0":{"test_state":{"hidden":true}},"2":{"test_harbor":{},"test_river":{}}}
    }"#).unwrap()).unwrap();
    native
        .set_attributes(
            serde_json::from_str(include_str!(
                "../../../examples/world/native_attributes.json"
            ))
            .unwrap(),
        )
        .unwrap();
    native
        .set_administration(
            serde_json::from_str(include_str!(
                "../../../examples/world/native_administration.json"
            ))
            .unwrap(),
        )
        .unwrap();
    native
        .set_calendar(augustus_core::Calendar {
            start: augustus_core::WorldDate {
                year: 1444,
                month: 11,
                day: 11,
            },
            month_lengths: [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
        })
        .unwrap();
    native
        .set_taxation(augustus_core::Taxation::new(
            "tax_after_autonomy",
            [alpha, beta].into_iter().collect(),
        ))
        .unwrap();
    assert_eq!(loaded.initial.world(), &native);
    let earlier = VanillaWorld::load(fixture(), "1435.1.1").unwrap();
    assert_eq!(
        earlier.initial.province(1).unwrap().owner.as_deref(),
        Some("AAA")
    );
    assert_eq!(earlier.initial.bound_capital("BBB"), None);
    assert_eq!(earlier.report.unbound_active_capitals, Vec::<String>::new());
}

#[test]
fn map_categories_and_absent_history_are_explicit() {
    let loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    assert_eq!(
        loaded.initial.province(3).unwrap().class,
        ProvinceClass::Sea
    );
    assert_eq!(
        loaded.initial.province(4).unwrap().class,
        ProvinceClass::Wasteland
    );
    assert_eq!(
        loaded.initial.province(5).unwrap().class,
        ProvinceClass::Unused
    );
    assert_eq!(loaded.report.unmapped_definition_ids, [5, 6]);
    assert_eq!(loaded.report.missing_province_history, [3, 5, 6]);
    assert_eq!(loaded.initial.province(3).unwrap().tax_milli, None);
    assert_eq!(loaded.palette().len(), 4);
    assert_eq!(loaded.report.unknown_map_colors, Vec::<[u8; 3]>::new());
    let temp = copy_fixture();
    let path = temp.path().join("map/definition.csv");
    let text = fs::read_to_string(&path)
        .unwrap()
        .replace("1;240;10;10", "1;241;10;10");
    fs::write(path, text).unwrap();
    let changed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(changed.report.unknown_map_colors, [[240, 10, 10]]);
}

#[test]
fn ambiguous_country_history_is_reported_without_selecting_a_winner() {
    let temp = copy_fixture();
    fs::write(
        temp.path().join("history/countries/AAA - Other.txt"),
        "capital = 1\n",
    )
    .unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(loaded.report.ambiguous_country_histories["AAA"].len(), 2);
    assert_eq!(loaded.report.country_history_files, 1);
    assert_eq!(
        loaded.initial.country("AAA").unwrap().declared_capital,
        None
    );
    assert_eq!(loaded.initial.country("AAA").unwrap().religion, None);
    assert_eq!(loaded.report.unbound_active_capitals, ["AAA"]);
}

#[test]
fn invalid_relationships_and_used_colors_fail_world_construction() {
    for (from, to, expected) in [
        ("owner = BBB", "owner = ZZZ", "unknown country ZZZ"),
        (
            "base_tax = 2.125",
            "base_tax = -1",
            "nonnegative base value",
        ),
        ("1440.1.1", "1440.13.1", "invalid history date"),
    ] {
        let temp = copy_fixture();
        let path = temp.path().join("history/provinces/1 - North.txt");
        let text = fs::read_to_string(&path).unwrap().replace(from, to);
        fs::write(&path, text).unwrap();
        let error = VanillaWorld::load(temp.path(), "1444.11.11")
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{error}");
    }
    let temp = copy_fixture();
    let path = temp.path().join("map/definition.csv");
    fs::write(
        &path,
        fs::read_to_string(&path)
            .unwrap()
            .replace("2;10;240;10", "2;240;10;10"),
    )
    .unwrap();
    assert!(
        VanillaWorld::load(temp.path(), "1444.11.11")
            .unwrap_err()
            .to_string()
            .contains("duplicate map color")
    );
}

#[test]
fn wrong_version_dimensions_and_dates_fail_before_publishing_a_world() {
    assert!(VanillaWorld::load(fixture(), "1444.2.30").is_err());
    let temp = copy_fixture();
    fs::write(
        temp.path().join("launcher-settings.json"),
        r#"{"rawVersion":"v1.36.0.0"}"#,
    )
    .unwrap();
    assert!(
        VanillaWorld::load(temp.path(), "1444.11.11")
            .unwrap_err()
            .to_string()
            .contains("1.37.5")
    );
    fs::copy(
        fixture().join("launcher-settings.json"),
        temp.path().join("launcher-settings.json"),
    )
    .unwrap();
    let path = temp.path().join("map/default.map");
    fs::write(
        &path,
        fs::read_to_string(&path)
            .unwrap()
            .replace("width = 4", "width = 5"),
    )
    .unwrap();
    assert!(
        VanillaWorld::load(temp.path(), "1444.11.11")
            .unwrap_err()
            .to_string()
            .contains("dimensions")
    );
}
