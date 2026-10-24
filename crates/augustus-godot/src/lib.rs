// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_demo::{Demo, Error, Input, System, View};
use godot::prelude::*;
use serde::Serialize;
use std::{collections::BTreeSet, path::PathBuf};

mod frame_clock;

struct AugustusExtension;

#[gdextension]
// SAFETY: Registration and engine object lifetimes are delegated to godot-rust.
// The simulation has no unsafe code and holds no Godot objects or callbacks.
unsafe impl ExtensionLibrary for AugustusExtension {}

#[derive(GodotClass)]
#[class(init, base=RefCounted)]
struct AugustusSession {
    demo: Option<Demo>,
}

fn reply<T: Serialize>(result: Result<T, Error>) -> GString {
    let packet = match result {
        Ok(value) => serde_json::json!({"ok": true, "value": value}),
        Err(error) => serde_json::json!({"ok": false, "error": error.to_string()}),
    };
    packet.to_string().as_str().into()
}

impl AugustusSession {
    fn demo(&self) -> Result<&Demo, Error> {
        self.demo
            .as_ref()
            .ok_or_else(|| Error::Invalid("start a session first".into()))
    }
    fn demo_mut(&mut self) -> Result<&mut Demo, Error> {
        self.demo
            .as_mut()
            .ok_or_else(|| Error::Invalid("start a session first".into()))
    }
}

#[derive(GodotClass)]
#[class(init, base=RefCounted)]
struct AugustusWorldSession {
    loaded: Option<augustus_eu4::VanillaWorld>,
    saved_imperial_baseline: Option<augustus_eu4::SavedImperialBaselineReport>,
    saved_office_preferences: Option<augustus_eu4::SavedOfficePreferenceReport>,
    frame_clock: frame_clock::FrameTimeSmoother,
}

enum SavedStateImport {
    None,
    OfficePreferences(PathBuf),
    ImperialBaseline(PathBuf),
}

struct WorldLoadOptions {
    saved_state_import: SavedStateImport,
    external_state: Option<PathBuf>,
    players: Option<BTreeSet<String>>,
}

fn world_reply<T: Serialize>(result: Result<T, String>) -> GString {
    match result {
        Ok(value) => serde_json::json!({"ok": true, "value": value}),
        Err(error) => serde_json::json!({"ok": false, "error": error}),
    }
    .to_string()
    .as_str()
    .into()
}

fn load_world_from_content(
    path: String,
    date: String,
    mod_directories: Vec<PathBuf>,
    user_directory: Option<PathBuf>,
    launcher_config: Option<PathBuf>,
    options: WorldLoadOptions,
) -> Result<
    (
        augustus_eu4::VanillaWorld,
        Option<augustus_eu4::SavedImperialBaselineReport>,
        Option<augustus_eu4::SavedOfficePreferenceReport>,
    ),
    String,
> {
    if !matches!(&options.saved_state_import, SavedStateImport::None)
        && (!mod_directories.is_empty() || user_directory.is_some())
    {
        return Err(
            "baseline save import requires vanilla content without Mods or user-directory overlays"
                .into(),
        );
    }
    let content = augustus_eu4::ContentSet::from_installation(
        path,
        &mod_directories,
        user_directory.as_deref(),
        launcher_config.as_deref(),
        augustus_eu4::TextEncoding::Windows1252,
    )
    .map_err(|error| error.to_string())?;
    let external_state = options
        .external_state
        .map(|path| {
            std::fs::read(&path)
                .map_err(|error| format!("{}: {error}", path.display()))
                .and_then(|bytes| {
                    serde_json::from_slice::<augustus_eu4::VanillaExternalState>(&bytes)
                        .map_err(|error| format!("{}: {error}", path.display()))
                })
        })
        .transpose()?;
    let mut loaded = match (options.players.as_ref(), external_state.as_ref()) {
        (Some(players), Some(state)) => {
            augustus_eu4::VanillaWorld::load_from_content_with_players_and_external_state(
                &content, &date, players, state,
            )
        }
        (Some(players), None) => {
            augustus_eu4::VanillaWorld::load_from_content_with_players(&content, &date, players)
        }
        (None, Some(state)) => augustus_eu4::VanillaWorld::load_from_content_with_external_state(
            &content, &date, state,
        ),
        (None, None) => augustus_eu4::VanillaWorld::load_from_content(&content, &date),
    }
    .map_err(|error| error.to_string())?;
    let (saved_imperial_baseline, saved_office_preferences) = match options.saved_state_import {
        SavedStateImport::None => (None, None),
        SavedStateImport::OfficePreferences(path) => (
            None,
            Some(
                augustus_eu4::apply_saved_office_preferences(&mut loaded, path)
                    .map_err(|error| error.to_string())?,
            ),
        ),
        SavedStateImport::ImperialBaseline(path) => (
            Some(
                augustus_eu4::apply_saved_imperial_baseline(&mut loaded, path)
                    .map_err(|error| error.to_string())?,
            ),
            None,
        ),
    };
    Ok((loaded, saved_imperial_baseline, saved_office_preferences))
}

fn load_world_from_eu4_save(
    path: String,
    save_path: PathBuf,
    mod_directories: Vec<PathBuf>,
    user_directory: Option<PathBuf>,
    launcher_config: Option<PathBuf>,
) -> Result<
    (
        augustus_eu4::VanillaWorld,
        Option<augustus_eu4::SavedImperialBaselineReport>,
        Option<augustus_eu4::SavedOfficePreferenceReport>,
    ),
    String,
> {
    if !mod_directories.is_empty() || user_directory.is_some() {
        return Err(
            "EU4 save import requires vanilla content without Mods or user-directory overlays"
                .into(),
        );
    }
    let content = augustus_eu4::ContentSet::from_installation(
        path,
        &mod_directories,
        user_directory.as_deref(),
        launcher_config.as_deref(),
        augustus_eu4::TextEncoding::Windows1252,
    )
    .map_err(|error| error.to_string())?;
    let loaded = augustus_eu4::VanillaWorld::load_from_content_with_save(&content, save_path)
        .map_err(|error| error.to_string())?;
    Ok((loaded, None, None))
}

impl AugustusWorldSession {
    fn loaded(&self) -> Result<&augustus_eu4::VanillaWorld, String> {
        self.loaded
            .as_ref()
            .ok_or_else(|| "load a world first".into())
    }
    fn loaded_mut(&mut self) -> Result<&mut augustus_eu4::VanillaWorld, String> {
        self.loaded
            .as_mut()
            .ok_or_else(|| "load a world first".into())
    }

    fn load_world_with_content_internal(
        &mut self,
        path: GString,
        date: GString,
        mod_directories: Array<GString>,
        user_directory: GString,
        launcher_config: GString,
        options: WorldLoadOptions,
    ) -> GString {
        let mod_directories = mod_directories
            .iter_shared()
            .map(|path| PathBuf::from(path.to_string()))
            .collect::<Vec<_>>();
        let user_directory = PathBuf::from(user_directory.to_string());
        let user_directory = (!user_directory.as_os_str().is_empty()).then_some(user_directory);
        let launcher_config = PathBuf::from(launcher_config.to_string());
        let launcher_config = (!launcher_config.as_os_str().is_empty()).then_some(launcher_config);
        match load_world_from_content(
            path.to_string(),
            date.to_string(),
            mod_directories,
            user_directory,
            launcher_config,
            options,
        ) {
            Ok((loaded, saved_imperial_baseline, saved_office_preferences)) => {
                self.loaded = Some(loaded);
                self.saved_imperial_baseline = saved_imperial_baseline;
                self.saved_office_preferences = saved_office_preferences;
                self.frame_clock.reset();
                self.world_overview()
            }
            Err(error) => world_reply::<serde_json::Value>(Err(error)),
        }
    }

    fn load_world_from_eu4_save_internal(
        &mut self,
        path: GString,
        save_path: GString,
        mod_directories: Array<GString>,
        user_directory: GString,
        launcher_config: GString,
    ) -> GString {
        let mod_directories = mod_directories
            .iter_shared()
            .map(|path| PathBuf::from(path.to_string()))
            .collect::<Vec<_>>();
        let user_directory = PathBuf::from(user_directory.to_string());
        let user_directory = (!user_directory.as_os_str().is_empty()).then_some(user_directory);
        let launcher_config = PathBuf::from(launcher_config.to_string());
        let launcher_config = (!launcher_config.as_os_str().is_empty()).then_some(launcher_config);
        let save_path = PathBuf::from(save_path.to_string());
        match load_world_from_eu4_save(
            path.to_string(),
            save_path,
            mod_directories,
            user_directory,
            launcher_config,
        ) {
            Ok((loaded, saved_imperial_baseline, saved_office_preferences)) => {
                self.loaded = Some(loaded);
                self.saved_imperial_baseline = saved_imperial_baseline;
                self.saved_office_preferences = saved_office_preferences;
                self.frame_clock.reset();
                self.world_overview()
            }
            Err(error) => world_reply::<serde_json::Value>(Err(error)),
        }
    }

    fn players(values: Array<GString>) -> Option<BTreeSet<String>> {
        let players = values
            .iter_shared()
            .map(|value| value.to_string().trim().to_owned())
            .filter(|value| !value.is_empty())
            .collect::<BTreeSet<_>>();
        (!players.is_empty()).then_some(players)
    }
}

#[godot_api]
impl AugustusWorldSession {
    #[func]
    fn update_frame_timing(&mut self, clock_value: f64) -> GString {
        let timing = self.frame_clock.update(clock_value);
        world_reply::<serde_json::Value>(Ok(serde_json::json!({
            "raw_delta": timing.raw_delta.is_finite().then_some(f64::from(timing.raw_delta)),
            "smoothed_delta": timing.smoothed_delta.is_finite().then_some(f64::from(timing.smoothed_delta)),
        })))
    }

    #[func]
    fn advance(&mut self, days: i64) -> GString {
        let result = (|| {
            let days = u64::try_from(days).map_err(|_| "days must be nonnegative")?;
            self.loaded_mut()?
                .advance(days)
                .map_err(|e| e.to_string())?;
            Ok(())
        })();
        match result {
            Ok(()) => self.simulation_view(),
            Err(error) => world_reply::<()>(Err(error)),
        }
    }

    #[func]
    fn simulation_view(&self) -> GString {
        world_reply(self.loaded().and_then(|loaded| {
            let world = loaded.initial.world();
            Ok(serde_json::json!({"day": world.day(), "date": world.date().map_err(|e| e.to_string())?,
                "tax_settlements": world.taxation().map(|t| t.settlement_count),
                "days_until_next_month": world.calendar().map(|c| c.next_month(world.day()).map(|next| next - world.day())).transpose().map_err(|e| e.to_string())?,
                "last_tax_day": world.taxation().and_then(|t| t.last_settlement.as_ref().map(|s| s.day))}))
        }))
    }

    #[func]
    fn save_snapshot(&self) -> GString {
        world_reply(
            self.loaded()
                .and_then(|loaded| loaded.save().map_err(|e| e.to_string())),
        )
    }

    #[func]
    fn restore_snapshot(&mut self, snapshot: GString) -> GString {
        match self.loaded_mut().and_then(|loaded| {
            loaded
                .restore(&snapshot.to_string())
                .map_err(|e| e.to_string())
        }) {
            Ok(()) => {
                self.frame_clock.reset();
                self.simulation_view()
            }
            Err(error) => world_reply::<()>(Err(error)),
        }
    }
    #[func]
    fn load_world(&mut self, path: GString, date: GString) -> GString {
        match augustus_eu4::VanillaWorld::load(path.to_string(), &date.to_string()) {
            Ok(loaded) => {
                self.loaded = Some(loaded);
                self.saved_imperial_baseline = None;
                self.saved_office_preferences = None;
                self.frame_clock.reset();
                self.world_overview()
            }
            Err(e) => world_reply::<serde_json::Value>(Err(e.to_string())),
        }
    }

    #[func]
    fn load_world_with_content(
        &mut self,
        path: GString,
        date: GString,
        mod_directories: Array<GString>,
        user_directory: GString,
        launcher_config: GString,
    ) -> GString {
        self.load_world_with_content_internal(
            path,
            date,
            mod_directories,
            user_directory,
            launcher_config,
            WorldLoadOptions {
                saved_state_import: SavedStateImport::None,
                external_state: None,
                players: None,
            },
        )
    }

    #[func]
    fn load_world_with_content_and_players(
        &mut self,
        path: GString,
        date: GString,
        mod_directories: Array<GString>,
        user_directory: GString,
        launcher_config: GString,
        players: Array<GString>,
    ) -> GString {
        self.load_world_with_content_internal(
            path,
            date,
            mod_directories,
            user_directory,
            launcher_config,
            WorldLoadOptions {
                saved_state_import: SavedStateImport::None,
                external_state: None,
                players: Self::players(players),
            },
        )
    }

    #[func]
    fn load_world_with_eu4_save(
        &mut self,
        path: GString,
        save_path: GString,
        mod_directories: Array<GString>,
        user_directory: GString,
        launcher_config: GString,
    ) -> GString {
        self.load_world_from_eu4_save_internal(
            path,
            save_path,
            mod_directories,
            user_directory,
            launcher_config,
        )
    }

    #[func]
    fn load_world_with_content_and_external_state(
        &mut self,
        path: GString,
        date: GString,
        mod_directories: Array<GString>,
        user_directory: GString,
        launcher_config: GString,
        external_state: GString,
    ) -> GString {
        let external_state = PathBuf::from(external_state.to_string());
        let external_state = (!external_state.as_os_str().is_empty()).then_some(external_state);
        self.load_world_with_content_internal(
            path,
            date,
            mod_directories,
            user_directory,
            launcher_config,
            WorldLoadOptions {
                saved_state_import: SavedStateImport::None,
                external_state,
                players: None,
            },
        )
    }

    #[func]
    #[allow(clippy::too_many_arguments)]
    fn load_world_with_content_and_players_and_external_state(
        &mut self,
        path: GString,
        date: GString,
        mod_directories: Array<GString>,
        user_directory: GString,
        launcher_config: GString,
        players: Array<GString>,
        external_state: GString,
    ) -> GString {
        let external_state = PathBuf::from(external_state.to_string());
        let external_state = (!external_state.as_os_str().is_empty()).then_some(external_state);
        self.load_world_with_content_internal(
            path,
            date,
            mod_directories,
            user_directory,
            launcher_config,
            WorldLoadOptions {
                saved_state_import: SavedStateImport::None,
                external_state,
                players: Self::players(players),
            },
        )
    }

    #[func]
    #[allow(clippy::too_many_arguments)]
    fn load_world_with_players_and_external_state_and_baseline_save(
        &mut self,
        path: GString,
        date: GString,
        mod_directories: Array<GString>,
        user_directory: GString,
        launcher_config: GString,
        players: Array<GString>,
        external_state: GString,
        baseline_save: GString,
    ) -> GString {
        let external_state = PathBuf::from(external_state.to_string());
        let external_state = (!external_state.as_os_str().is_empty()).then_some(external_state);
        let baseline_save = PathBuf::from(baseline_save.to_string());
        let saved_state_import = if baseline_save.as_os_str().is_empty() {
            SavedStateImport::None
        } else {
            SavedStateImport::ImperialBaseline(baseline_save)
        };
        self.load_world_with_content_internal(
            path,
            date,
            mod_directories,
            user_directory,
            launcher_config,
            WorldLoadOptions {
                saved_state_import,
                external_state,
                players: Self::players(players),
            },
        )
    }

    #[func]
    fn load_world_with_office_preferences(
        &mut self,
        path: GString,
        date: GString,
        mod_directories: Array<GString>,
        user_directory: GString,
        launcher_config: GString,
        office_preference_save: GString,
    ) -> GString {
        let office_preference_save = PathBuf::from(office_preference_save.to_string());
        let office_preference_save =
            (!office_preference_save.as_os_str().is_empty()).then_some(office_preference_save);
        self.load_world_with_content_internal(
            path,
            date,
            mod_directories,
            user_directory,
            launcher_config,
            WorldLoadOptions {
                saved_state_import: office_preference_save
                    .map(SavedStateImport::OfficePreferences)
                    .unwrap_or(SavedStateImport::None),
                external_state: None,
                players: None,
            },
        )
    }

    #[func]
    fn load_world_with_baseline_save(
        &mut self,
        path: GString,
        date: GString,
        mod_directories: Array<GString>,
        user_directory: GString,
        launcher_config: GString,
        baseline_save: GString,
    ) -> GString {
        let baseline_save = PathBuf::from(baseline_save.to_string());
        let saved_state_import = if baseline_save.as_os_str().is_empty() {
            SavedStateImport::None
        } else {
            SavedStateImport::ImperialBaseline(baseline_save)
        };
        self.load_world_with_content_internal(
            path,
            date,
            mod_directories,
            user_directory,
            launcher_config,
            WorldLoadOptions {
                saved_state_import,
                external_state: None,
                players: None,
            },
        )
    }

    #[func]
    #[allow(clippy::too_many_arguments)]
    fn load_world_with_players_and_baseline_save(
        &mut self,
        path: GString,
        date: GString,
        mod_directories: Array<GString>,
        user_directory: GString,
        launcher_config: GString,
        players: Array<GString>,
        baseline_save: GString,
    ) -> GString {
        let baseline_save = PathBuf::from(baseline_save.to_string());
        let saved_state_import = if baseline_save.as_os_str().is_empty() {
            SavedStateImport::None
        } else {
            SavedStateImport::ImperialBaseline(baseline_save)
        };
        self.load_world_with_content_internal(
            path,
            date,
            mod_directories,
            user_directory,
            launcher_config,
            WorldLoadOptions {
                saved_state_import,
                external_state: None,
                players: Self::players(players),
            },
        )
    }

    #[func]
    fn world_overview(&self) -> GString {
        world_reply(self.loaded().map(|loaded| {
            let countries: Vec<_> = loaded
                .initial
                .countries()
                .iter()
                .map(|c| {
                    serde_json::json!({"id": c.id, "name": c.name,
                    "province_count": loaded.initial.owned_provinces(&c.id).len()})
                })
                .collect();
            serde_json::json!({"report": loaded.report, "countries": countries,
                "palette": loaded.palette(), "province_bitmap": loaded.province_bitmap,
                "saved_imperial_baseline": self.saved_imperial_baseline.as_ref(),
                "saved_office_preferences": self.saved_office_preferences.as_ref()})
        }))
    }

    #[func]
    fn query_country(&self, id: GString) -> GString {
        world_reply(self.loaded().and_then(|loaded| {
            let id = id.to_string();
            let country = loaded.country(&id).ok_or("unknown country")?;
            let mut value = serde_json::json!(country);
            value["attributes"] = serde_json::to_value(
                loaded
                    .country_attributes(&id)
                    .map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            if let Some(accounts) = loaded
                .initial
                .world()
                .monthly_accruals()
                .and_then(|state| state.last_settlement.as_ref())
                .and_then(|last| {
                    last.accounts
                        .get(&loaded.initial.country_entity(&id).expect("known country"))
                })
            {
                value["last_monthly_accruals"] = serde_json::json!(accounts);
            }
            Ok(value)
        }))
    }

    #[func]
    fn query_unit_templates(&self, id: GString) -> GString {
        world_reply(self.loaded().and_then(|loaded| {
            let id = id.to_string();
            let country = loaded
                .initial
                .country_entity(&id)
                .ok_or("unknown country")?;
            let world = loaded.initial.world();
            let available = world
                .available_unit_templates(country)
                .map_err(|error| error.to_string())?;
            let selected = ["infantry", "cavalry", "artillery"]
                .into_iter()
                .map(|category| {
                    (
                        category,
                        world.selected_unit(country, category).map(str::to_owned),
                    )
                })
                .collect::<std::collections::BTreeMap<_, _>>();
            Ok(serde_json::json!({"country": id, "available": available, "selected": selected}))
        }))
    }

    #[func]
    fn select_unit_template(
        &mut self,
        id: GString,
        category: GString,
        unit_type: GString,
    ) -> GString {
        let id = id.to_string();
        let result = self.loaded_mut().and_then(|loaded| {
            let country = loaded
                .initial
                .country_entity(&id)
                .ok_or("unknown country")?;
            loaded
                .initial
                .set_selected_unit(country, &category.to_string(), &unit_type.to_string())
                .map_err(|error| error.to_string())
        });
        match result {
            Ok(()) => self.query_unit_templates(id.as_str().into()),
            Err(error) => world_reply::<serde_json::Value>(Err(error)),
        }
    }

    #[func]
    fn clear_unit_template(&mut self, id: GString, category: GString) -> GString {
        let id = id.to_string();
        let result = self.loaded_mut().and_then(|loaded| {
            let country = loaded
                .initial
                .country_entity(&id)
                .ok_or("unknown country")?;
            loaded
                .initial
                .clear_selected_unit(country, &category.to_string())
                .map_err(|error| error.to_string())
        });
        match result {
            Ok(()) => self.query_unit_templates(id.as_str().into()),
            Err(error) => world_reply::<serde_json::Value>(Err(error)),
        }
    }

    #[func]
    fn pending_event_choices(&self) -> GString {
        world_reply(self.loaded().and_then(|loaded| {
            let world = loaded.initial.world();
            let pending_events = world
                .pending_events()
                .iter()
                .map(|pending| {
                    let current = world
                        .entity(pending.current)
                        .map_err(|error| error.to_string())?;
                    let options = world
                        .available_event_options(pending.id)
                        .map_err(|error| error.to_string())?;
                    let option_locations = options
                        .iter()
                        .map(|option| {
                            let location = world
                                .pending_event_option_location(pending.id, &option.id)
                                .map_err(|error| error.to_string())?
                                .map(|entity| {
                                    world
                                        .entity(entity)
                                        .map(|state| state.name.clone())
                                        .map_err(|error| error.to_string())
                                })
                                .transpose()?;
                            Ok((option.id.clone(), location))
                        })
                        .collect::<Result<std::collections::BTreeMap<_, _>, String>>()?;
                    let location = world
                        .pending_event_location(pending.id)
                        .map_err(|error| error.to_string())?
                        .map(|entity| {
                            world
                                .entity(entity)
                                .map(|state| state.name.clone())
                                .map_err(|error| error.to_string())
                        })
                        .transpose()?;
                    let picture = world
                        .pending_event_picture(pending.id)
                        .map_err(|error| error.to_string())?;
                    let event_text = loaded.event_text(&pending.event);
                    let option_labels = options
                        .iter()
                        .map(|option| {
                            let label = event_text
                                .and_then(|text| text.option_labels.get(&option.id))
                                .cloned()
                                .unwrap_or_else(|| option.id.clone());
                            (option.id.clone(), label)
                        })
                        .collect::<std::collections::BTreeMap<_, _>>();
                    Ok(serde_json::json!({
                        "id": pending.id.0,
                        "event": pending.event,
                        "title": event_text.map_or_else(|| pending.event.clone(), |text| text.title.clone()),
                        "description": event_text.and_then(|text| text.description.clone()),
                        "current": pending.current,
                        "current_name": current.name,
                        "day": pending.day,
                        "picture": picture,
                        "location": location,
                        "options": options.iter().map(|option| option.id.clone()).collect::<Vec<_>>(),
                        "option_labels": option_labels,
                        "option_locations": option_locations,
                    }))
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(serde_json::json!({"pending_events": pending_events}))
        }))
    }

    #[func]
    fn choose_event_option(&mut self, pending_id: i64, option: GString) -> GString {
        let result = (|| {
            let pending = u64::try_from(pending_id).map_err(|_| "invalid pending event id")?;
            self.loaded_mut()?
                .initial
                .choose_event(&augustus_core::EventChoice {
                    pending: augustus_core::PendingEventId(pending),
                    option: option.to_string(),
                })
                .map_err(|error| error.to_string())
        })();
        match result {
            Ok(()) => self.pending_event_choices(),
            Err(error) => world_reply::<serde_json::Value>(Err(error)),
        }
    }

    #[func]
    fn query_province(&self, id: i64) -> GString {
        world_reply(self.loaded().and_then(|loaded| {
            let id = u32::try_from(id).map_err(|_| "invalid province identifier")?;
            loaded
                .province(id)
                .map(|p| serde_json::json!(p))
                .ok_or_else(|| "unknown province".into())
        }))
    }
}

#[godot_api]
impl AugustusSession {
    #[func]
    fn start_session(&mut self, capacity: bool, native: bool) -> GString {
        let result = Demo::embedded(
            if native { Input::Native } else { Input::Eu4 },
            if capacity {
                System::Capacity
            } else {
                System::Fixed
            },
        );
        match result {
            Ok(demo) => {
                self.demo = Some(demo);
                reply(self.demo().and_then(Demo::view))
            }
            Err(error) => reply::<View>(Err(error)),
        }
    }

    #[func]
    fn query(&self) -> GString {
        reply(self.demo().and_then(Demo::view))
    }

    #[func]
    fn build(&mut self, province: i64, building: i64) -> GString {
        let result = (|| {
            let province = usize::try_from(province)
                .map_err(|_| Error::Invalid("invalid province index".into()))?;
            let building = usize::try_from(building)
                .map_err(|_| Error::Invalid("invalid building index".into()))?;
            let demo = self.demo_mut()?;
            demo.build(province, building)?;
            demo.view()
        })();
        reply(result)
    }

    #[func]
    fn advance(&mut self, days: i64) -> GString {
        reply((|| {
            let days = u64::try_from(days)
                .map_err(|_| Error::Invalid("days must be nonnegative".into()))?;
            let demo = self.demo_mut()?;
            demo.advance(days)?;
            demo.view()
        })())
    }

    #[func]
    fn save_snapshot(&self) -> GString {
        reply(self.demo().and_then(Demo::save))
    }

    #[func]
    fn reorganise(&mut self, province: i64) -> GString {
        reply((|| {
            let province = usize::try_from(province)
                .map_err(|_| Error::Invalid("invalid province index".into()))?;
            let demo = self.demo_mut()?;
            demo.reorganise(province)?;
            demo.view()
        })())
    }

    #[func]
    fn restore_snapshot(&mut self, snapshot: GString) -> GString {
        reply((|| {
            let demo = self.demo_mut()?;
            demo.restore(&snapshot.to_string())?;
            demo.view()
        })())
    }
}
