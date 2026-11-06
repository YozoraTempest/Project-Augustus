// SPDX-License-Identifier: GPL-3.0-only

use augustus_core::GameDate;
use augustus_eu4::world::OpeningWorld;
use godot::prelude::*;

use crate::{info::money, military};

pub(crate) fn info(world: &OpeningWorld, country: &str, date: GameDate) -> VarDictionary {
    let state = world.state();
    let mut info = VarDictionary::new();
    let mut armies: Array<VarDictionary> = Array::new();
    let mut strength = 0_u64;
    let mut regiment_count = 0_usize;
    // 按稳定编号列出本国军队，地图上查看其他国家不影响全国总览。
    for army in state
        .armies()
        .values()
        .filter(|army| army.country == country)
    {
        strength += army.strength();
        regiment_count += army.regiments.len();
        armies.push(&military::army_info(world, army, date, Some(country)));
    }
    let mut projects = Vec::new();
    for (&province, queue) in state.recruitments() {
        for project in queue.iter().filter(|project| project.country == country) {
            projects.push((province, project));
        }
    }
    // 优先显示即将完成的订单；同一天完成时按省份和订单编号排列。
    projects.sort_by_key(|(province, project)| (project.completes_on, *province, project.id()));
    let mut recruitments: Array<VarDictionary> = Array::new();
    for (province, project) in projects {
        recruitments.push(&military::recruitment_details(
            world,
            province,
            project,
            Some(country),
            date,
        ));
    }
    info.set("country", country);
    info.set("country_name", world.display_name(country).as_str());
    info.set("army_count", armies.len() as i64);
    info.set("regiment_count", regiment_count as i64);
    info.set("strength", strength as i64);
    info.set("recruitment_count", recruitments.len() as i64);
    info.set("manpower", &crate::manpower::info(world, country));
    money(
        &mut info,
        "monthly_maintenance",
        state
            .country_monthly_budget(country)
            .map(|budget| budget.army_maintenance),
    );
    info.set("armies", &armies);
    info.set("recruitments", &recruitments);
    info
}
