// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_demo::{Demo, Error, Input, System, View};
use godot::prelude::*;
use serde::Serialize;

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
}

#[godot_api]
impl AugustusWorldSession {
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
            Ok(()) => self.simulation_view(),
            Err(error) => world_reply::<()>(Err(error)),
        }
    }
    #[func]
    fn load_world(&mut self, path: GString, date: GString) -> GString {
        match augustus_eu4::VanillaWorld::load(path.to_string(), &date.to_string()) {
            Ok(loaded) => {
                self.loaded = Some(loaded);
                self.world_overview()
            }
            Err(e) => world_reply::<serde_json::Value>(Err(e.to_string())),
        }
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
                "palette": loaded.palette(), "province_bitmap": loaded.province_bitmap})
        }))
    }

    #[func]
    fn query_country(&self, id: GString) -> GString {
        world_reply(self.loaded().and_then(|loaded| {
            loaded
                .country(&id.to_string())
                .map(|c| serde_json::json!(c))
                .ok_or_else(|| "unknown country".into())
        }))
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
