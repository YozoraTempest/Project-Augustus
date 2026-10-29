// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use augustus_core::{NativeMigrationCountryState, NativeMigrationStates};
use serde::{Deserialize, Serialize};

/// Native migration state translated from an EU4 save or another provider.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaExternalNativeMigrationState {
    #[serde(default)]
    pub provinces: Option<Vec<u32>>,
    #[serde(default)]
    pub migration_steps: Option<u32>,
}

/// Native migration records read from an EU4 save before binding to a world.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaNativeMigrationImport {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub countries: BTreeMap<String, VanillaExternalNativeMigrationState>,
}

impl VanillaNativeMigrationImport {
    /// Keep known countries and make an entire province list unknown if any
    /// referenced province is absent from the loaded world.
    pub fn states_for_world(
        &self,
        world: &crate::VanillaWorld,
    ) -> BTreeMap<String, VanillaExternalNativeMigrationState> {
        self.countries
            .iter()
            .filter(|(country, _)| world.initial.country(country.as_str()).is_some())
            .map(|(country, state)| {
                let mut state = state.clone();
                if state.provinces.as_ref().is_some_and(|provinces| {
                    provinces
                        .iter()
                        .any(|province| world.initial.province(*province).is_none())
                }) {
                    state.provinces = None;
                }
                (country.clone(), state)
            })
            .collect()
    }
}

impl crate::VanillaWorld {
    /// Rebinds current native migration records while preserving countries
    /// that are absent from this update.
    pub fn set_country_native_migration(
        &mut self,
        states: BTreeMap<String, VanillaExternalNativeMigrationState>,
    ) -> Result<(), augustus_core::Error> {
        let mut countries = self
            .initial
            .world()
            .native_migration_states()
            .countries
            .clone();

        for (tag, state) in states {
            let country = self.initial.country_entity(&tag).ok_or_else(|| {
                augustus_core::Error::Invalid(format!("unknown EU4 native migration country {tag}"))
            })?;
            let provinces = state
                .provinces
                .map(|provinces| {
                    let mut seen = BTreeSet::new();
                    provinces
                        .into_iter()
                        .map(|province| {
                            let entity =
                                self.initial.province_entity(province).ok_or_else(|| {
                                    augustus_core::Error::Invalid(format!(
                                        "unknown EU4 native migration province {province}"
                                    ))
                                })?;
                            if !seen.insert(entity) {
                                return Err(augustus_core::Error::Invalid(format!(
                                    "duplicate EU4 native migration province {province}"
                                )));
                            }
                            Ok(entity)
                        })
                        .collect::<Result<Vec<_>, augustus_core::Error>>()
                })
                .transpose()?;
            countries.insert(
                country,
                NativeMigrationCountryState {
                    provinces,
                    migration_steps: state.migration_steps,
                },
            );
        }

        self.initial
            .set_native_migration_states(NativeMigrationStates { countries })
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, path::Path};

    use augustus_core::NativeMigrationCountryState;

    use super::{VanillaExternalNativeMigrationState, VanillaNativeMigrationImport};
    use crate::{ContentRoot, ContentSet, ContentSource, VanillaWorld};

    fn fixture_world() -> VanillaWorld {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
        let content = ContentSet::resolve(vec![ContentSource::base(
            ContentRoot::open(fixture).unwrap(),
        )])
        .unwrap();
        VanillaWorld::load_from_content(&content, "1444.11.11").unwrap()
    }

    fn state_for(world: &VanillaWorld, country: &str) -> Option<NativeMigrationCountryState> {
        let entity = world.initial.country_entity(country)?;
        world
            .initial
            .world()
            .native_migration_states()
            .countries
            .get(&entity)
            .cloned()
    }

    #[test]
    fn imported_native_state_filters_countries_and_whole_unknown_province_lists() {
        let world = fixture_world();
        let province = world.initial.provinces()[0].id;
        let import = VanillaNativeMigrationImport {
            countries: BTreeMap::from([
                (
                    "AAA".into(),
                    VanillaExternalNativeMigrationState {
                        provinces: Some(vec![province, 0]),
                        migration_steps: Some(4),
                    },
                ),
                (
                    "ZZZ".into(),
                    VanillaExternalNativeMigrationState {
                        provinces: Some(vec![]),
                        migration_steps: None,
                    },
                ),
            ]),
        };

        let filtered = import.states_for_world(&world);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered["AAA"].provinces, None);
        assert_eq!(filtered["AAA"].migration_steps, Some(4));
        assert_eq!(import.countries["AAA"].provinces, Some(vec![province, 0]));
    }

    #[test]
    fn runtime_native_state_updates_are_atomic_partial_and_snapshot_restorable() {
        let mut world = fixture_world();
        let province = world.initial.provinces()[0].id;
        world
            .set_country_native_migration(BTreeMap::from([(
                "AAA".into(),
                VanillaExternalNativeMigrationState {
                    provinces: Some(vec![province]),
                    migration_steps: Some(2),
                },
            )]))
            .unwrap();
        let before_invalid_update = world.initial.world().native_migration_states().clone();

        assert!(
            world
                .set_country_native_migration(BTreeMap::from([
                    (
                        "AAA".into(),
                        VanillaExternalNativeMigrationState {
                            provinces: Some(vec![]),
                            migration_steps: Some(0),
                        },
                    ),
                    ("ZZZ".into(), VanillaExternalNativeMigrationState::default(),),
                ]))
                .is_err()
        );
        assert_eq!(
            world.initial.world().native_migration_states(),
            &before_invalid_update
        );

        assert!(
            world
                .set_country_native_migration(BTreeMap::from([(
                    "AAA".into(),
                    VanillaExternalNativeMigrationState {
                        provinces: Some(vec![0]),
                        migration_steps: Some(9),
                    },
                )]))
                .is_err()
        );
        assert_eq!(
            world.initial.world().native_migration_states(),
            &before_invalid_update
        );

        let other_country = world
            .initial
            .countries()
            .iter()
            .find(|country| country.id != "AAA")
            .expect("fixture has another country")
            .id
            .clone();
        world
            .set_country_native_migration(BTreeMap::from([(
                other_country.clone(),
                VanillaExternalNativeMigrationState {
                    provinces: Some(vec![]),
                    migration_steps: Some(0),
                },
            )]))
            .unwrap();
        world
            .set_country_native_migration(BTreeMap::from([(
                "AAA".into(),
                VanillaExternalNativeMigrationState {
                    provinces: Some(vec![]),
                    migration_steps: Some(0),
                },
            )]))
            .unwrap();
        assert_eq!(
            state_for(&world, "AAA"),
            Some(NativeMigrationCountryState {
                provinces: Some(vec![]),
                migration_steps: Some(0),
            })
        );
        assert_eq!(
            state_for(&world, &other_country),
            Some(NativeMigrationCountryState {
                provinces: Some(vec![]),
                migration_steps: Some(0),
            })
        );

        let snapshot = world.save().unwrap();
        world
            .set_country_native_migration(BTreeMap::from([(
                "AAA".into(),
                VanillaExternalNativeMigrationState::default(),
            )]))
            .unwrap();
        assert_eq!(state_for(&world, "AAA").unwrap().provinces, None);
        world.restore(&snapshot).unwrap();
        assert_eq!(
            state_for(&world, "AAA"),
            Some(NativeMigrationCountryState {
                provinces: Some(vec![]),
                migration_steps: Some(0),
            })
        );
    }
}
