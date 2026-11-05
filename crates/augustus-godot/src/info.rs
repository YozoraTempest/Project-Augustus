// SPDX-License-Identifier: GPL-3.0-only

//! 将核心状态投影为界面数据，不在 Godot 中维护另一份国家或省份状态。

use augustus_core::ProvinceKind;
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
    info
}

pub(crate) fn country_info(world: &OpeningWorld, tag: &str) -> VarDictionary {
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

fn text(info: &mut VarDictionary, key: &str, value: Option<&str>) {
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
