// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{Action, Condition, EntityId, FieldId, Plan, RuleId, Target, World};

use crate::{Building, Error};

#[derive(Debug, Clone, Copy)]
pub struct Slots {
    pub treasury: FieldId,
    pub queued: FieldId,
    pub progress: FieldId,
    pub capacity: FieldId,
}

/// The chosen provider owns both starting construction and its future settlement.
/// Selecting a provider occurs before creating or restoring a world.
pub trait ConstructionSystem {
    fn id(&self) -> &'static str;
    fn rules(&self, buildings: &[Building], slots: Slots) -> Vec<Plan>;
    fn start(
        &self,
        world: &World,
        province: EntityId,
        index: usize,
        building: &Building,
        slots: Slots,
    ) -> Result<Plan, Error>;
    fn reorganise(&self, slots: Slots) -> Result<Plan, Error>;
}

pub struct FixedConstruction;
pub struct CapacityConstruction;

fn pay_and_queue(
    world: &World,
    province: EntityId,
    index: usize,
    building: &Building,
    slots: Slots,
) -> Result<Plan, Error> {
    if world.value(province, slots.queued)? != 0 {
        return Err(Error::Invalid(
            "province already has a construction queue".into(),
        ));
    }
    if world.value(province, building.built_field)? != 0 {
        return Err(Error::Invalid("building already exists".into()));
    }
    let owner = world.target(province, Target::Owner)?;
    if world.value(owner, slots.treasury)? < building.cost_milli {
        return Err(Error::Invalid("insufficient treasury".into()));
    }
    let queued = i64::try_from(index)
        .ok()
        .and_then(|index| index.checked_add(1))
        .ok_or(augustus_core::Error::Overflow)?;
    Ok(vec![
        Action::With {
            target: Target::Owner,
            plan: vec![Action::Add {
                target: Target::Current,
                field: slots.treasury,
                value: -building.cost_milli,
            }],
        },
        Action::Set {
            target: Target::Current,
            field: slots.queued,
            value: queued,
        },
        Action::Set {
            target: Target::Current,
            field: slots.progress,
            value: 0,
        },
        Action::Emit(format!("queued:{}", building.id)),
    ])
}

fn finish(building: &Building, slots: Slots) -> Plan {
    let mut plan = vec![
        Action::Set {
            target: Target::Current,
            field: slots.queued,
            value: 0,
        },
        Action::Set {
            target: Target::Current,
            field: building.built_field,
            value: 1,
        },
    ];
    plan.extend(building.completion.clone());
    plan
}

impl ConstructionSystem for FixedConstruction {
    fn id(&self) -> &'static str {
        "fixed-days:1"
    }
    fn rules(&self, _: &[Building], _: Slots) -> Vec<Plan> {
        Vec::new()
    }
    fn reorganise(&self, _: Slots) -> Result<Plan, Error> {
        Err(Error::Invalid(
            "fixed construction does not use daily capacity".into(),
        ))
    }
    fn start(
        &self,
        world: &World,
        province: EntityId,
        index: usize,
        building: &Building,
        slots: Slots,
    ) -> Result<Plan, Error> {
        let mut plan = pay_and_queue(world, province, index, building, slots)?;
        plan.push(Action::Schedule {
            delay: building.standard_days,
            plan: finish(building, slots),
        });
        Ok(plan)
    }
}

impl ConstructionSystem for CapacityConstruction {
    fn id(&self) -> &'static str {
        "capacity-per-day:1"
    }
    fn reorganise(&self, slots: Slots) -> Result<Plan, Error> {
        Ok(vec![Action::RandomBranch {
            then: vec![
                Action::Set {
                    target: Target::Current,
                    field: slots.capacity,
                    value: 1000,
                },
                Action::Emit("capacity:1".into()),
            ],
            otherwise: vec![
                Action::Set {
                    target: Target::Current,
                    field: slots.capacity,
                    value: 2000,
                },
                Action::Emit("capacity:2".into()),
            ],
        }])
    }
    fn rules(&self, buildings: &[Building], slots: Slots) -> Vec<Plan> {
        buildings
            .iter()
            .enumerate()
            .map(|(index, building)| {
                vec![
                    Action::Accumulate {
                        target: Target::Current,
                        field: slots.progress,
                        source: slots.capacity,
                    },
                    Action::If {
                        condition: Condition::AtLeast {
                            target: Target::Current,
                            field: slots.progress,
                            value: building.work_milli,
                        },
                        then: finish(building, slots),
                        otherwise: vec![Action::Schedule {
                            delay: 1,
                            plan: vec![Action::Run(RuleId(index))],
                        }],
                    },
                ]
            })
            .collect()
    }
    fn start(
        &self,
        world: &World,
        province: EntityId,
        index: usize,
        building: &Building,
        slots: Slots,
    ) -> Result<Plan, Error> {
        if world.value(province, slots.capacity)? <= 0 {
            return Err(Error::Invalid(
                "construction capacity must be positive".into(),
            ));
        }
        let mut plan = pay_and_queue(world, province, index, building, slots)?;
        plan.push(Action::Schedule {
            delay: 1,
            plan: vec![Action::Run(RuleId(index))],
        });
        Ok(plan)
    }
}
