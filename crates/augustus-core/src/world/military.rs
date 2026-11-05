// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    Army, GameDate, MilitaryState, ProvinceKind, REGIMENT_STRENGTH, Recruitment, RecruitmentError,
    RecruitmentQuote, RecruitmentRules, Regiment, SaveError, UnitKind,
};

use super::World;

impl World {
    pub fn recruitment_rules(&self) -> &RecruitmentRules {
        &self.recruitment_rules
    }

    pub fn military(&self) -> &MilitaryState {
        &self.military
    }

    pub fn armies(&self) -> &BTreeMap<u32, Army> {
        &self.military.armies
    }

    pub fn recruitment(&self, province: u32) -> Option<&Recruitment> {
        self.military.recruitments.get(&province)
    }

    pub fn quote_recruitment(
        &self,
        country: &str,
        province: u32,
        date: GameDate,
    ) -> Result<RecruitmentQuote, RecruitmentError> {
        let country = self
            .country(country)
            .ok_or(RecruitmentError::UnknownCountry)?;
        let state = self
            .province(province)
            .filter(|state| {
                self.active_provinces.contains(&province) && state.kind == ProvinceKind::Land
            })
            .ok_or(RecruitmentError::InvalidProvince)?;
        if state.owner.as_deref() != Some(country.tag())
            || state.controller.as_deref() != Some(country.tag())
        {
            return Err(RecruitmentError::NotOwnedAndControlled);
        }
        if self.recruitment(province).is_some() {
            return Err(RecruitmentError::AlreadyRecruiting);
        }
        if self.military.next_army_id == u32::MAX {
            return Err(RecruitmentError::ArmyLimit);
        }
        let cost = self.recruitment_rules.infantry_cost;
        let available = country.finances.treasury();
        if available < cost {
            return Err(RecruitmentError::InsufficientFunds { cost, available });
        }
        Ok(RecruitmentQuote {
            cost,
            completes_on: date
                .checked_add_days(self.recruitment_rules.infantry_days)
                .ok_or(RecruitmentError::DateOutOfRange)?,
        })
    }

    pub fn start_recruitment(
        &mut self,
        country: &str,
        province: u32,
        date: GameDate,
    ) -> Result<(), RecruitmentError> {
        let quote = self.quote_recruitment(country, province, date)?;
        self.countries
            .get_mut(country)
            .expect("招募国家已注册")
            .finances
            .spend(quote.cost);
        let army_id = self.military.next_army_id;
        self.military.next_army_id += 1;
        self.military.recruitments.insert(
            province,
            Recruitment {
                army_id,
                country: country.to_owned(),
                kind: UnitKind::Infantry,
                started_on: date,
                completes_on: quote.completes_on,
            },
        );
        Ok(())
    }

    pub(crate) fn complete_recruitments(&mut self, date: GameDate) {
        let ready: Vec<_> = self
            .military
            .recruitments
            .iter()
            .filter(|(_, recruitment)| recruitment.completes_on <= date)
            .map(|(&province, _)| province)
            .collect();
        for province in ready {
            let recruitment = self
                .military
                .recruitments
                .remove(&province)
                .expect("招募已到期");
            let regiment = Regiment {
                kind: recruitment.kind,
                strength: REGIMENT_STRENGTH,
            };
            // 同国同省份的后续部队加入驻军，保留原军队编号。
            if let Some(army) = self.military.armies.values_mut().find(|army| {
                army.country == recruitment.country
                    && army.province == province
                    && army.movement.is_none()
            }) {
                army.regiments.push(regiment);
            } else {
                self.military.armies.insert(
                    recruitment.army_id,
                    Army {
                        id: recruitment.army_id,
                        country: recruitment.country,
                        province,
                        regiments: vec![regiment],
                        movement: None,
                    },
                );
            }
        }
    }

    pub(crate) fn restore_military(
        &mut self,
        military: MilitaryState,
        date: GameDate,
    ) -> Result<(), SaveError> {
        let valid_province = |id| {
            self.active_provinces.contains(&id)
                && self
                    .province(id)
                    .is_some_and(|province| province.kind == ProvinceKind::Land)
        };
        let mut ids = BTreeSet::new();
        for (&id, army) in &military.armies {
            if id == 0
                || id != army.id()
                || !ids.insert(id)
                || self.country(&army.country).is_none()
                || !valid_province(army.province)
                || army.regiments.is_empty()
                || army
                    .regiments
                    .iter()
                    .any(|regiment| regiment.strength == 0 || regiment.strength > REGIMENT_STRENGTH)
            {
                return Err(SaveError::InvalidMilitary);
            }
            if let Some(movement) = &army.movement
                && (movement.from != army.province
                    || movement.started_on > date
                    || movement.arrives_on <= date
                    || movement.started_on >= movement.arrives_on
                    || self
                        .land_neighbors(movement.from)
                        .is_none_or(|neighbors| !neighbors.contains_key(&movement.to))
                    || !self.province(movement.to).is_some_and(|province| {
                        province.owner.as_deref() == Some(&army.country)
                            && province.controller.as_deref() == Some(&army.country)
                    }))
            {
                return Err(SaveError::InvalidMilitary);
            }
        }
        for (&province, project) in &military.recruitments {
            if project.army_id == 0
                || !ids.insert(project.army_id)
                || !valid_province(province)
                || self.country(&project.country).is_none()
                || project.started_on > date
                || project.completes_on <= date
            {
                return Err(SaveError::InvalidMilitary);
            }
        }
        if military.next_army_id == 0 || ids.last().is_some_and(|id| *id >= military.next_army_id) {
            return Err(SaveError::InvalidMilitary);
        }
        self.military = military;
        Ok(())
    }
}
