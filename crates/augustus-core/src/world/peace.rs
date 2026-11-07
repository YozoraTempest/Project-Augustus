// SPDX-License-Identifier: GPL-3.0-only

use std::collections::BTreeSet;

use crate::peace::{
    PeaceError, PeaceQuote, PeaceSheet, ProvinceCost, ViewerBattle, WarScoreView, accepts_terms,
    clamp_battle_milli, clamp_warscore_milli, format_warscore_milli, land_battle_milli,
    occupation_share_milli, province_cost_milli, truce_expiry,
};
use crate::{BattleScore, GameDate, ProvinceKind, ReportedSide, Truce};

use super::World;

impl World {
    /// 从该国视角读取一场进行中的战争。分数来自已结束战斗和当前占领，不在界面重算规则。
    pub fn war_score(&self, viewer: &str, war_id: u32) -> Option<WarScoreView> {
        let war = self.war_state.active.get(&war_id)?;
        if !war.involves(viewer) {
            return None;
        }
        Some(self.score_view(viewer, war.attacker(), war.defender(), war.battles()))
    }

    pub fn truces_of<'a>(&'a self, country: &'a str) -> impl Iterator<Item = &'a Truce> + 'a {
        self.war_state
            .truces
            .iter()
            .filter(move |truce| truce.countries().iter().any(|tag| tag == country))
    }

    pub fn truce_between(&self, left: &str, right: &str) -> Option<&Truce> {
        self.war_state
            .truces
            .iter()
            .find(|truce| truce.binds(left, right))
    }

    /// 预览白和平或一组割让。不修改战争、省份或停战。
    pub fn quote_peace(
        &self,
        offerer: &str,
        target: &str,
        provinces: &[u32],
        date: GameDate,
    ) -> Result<PeaceQuote, PeaceError> {
        let war = self
            .war_between(offerer, target)
            .ok_or(PeaceError::NotAtWar)?;
        if self.country(offerer).is_none() || self.country(target).is_none() {
            return Err(PeaceError::UnknownCountry);
        }
        let score = self.score_view(offerer, war.attacker(), war.defender(), war.battles());
        let demands = self.demand_costs(offerer, target, provinces)?;
        let cost_milli = demands.iter().map(|demand| demand.cost_milli).sum();
        let white_peace = provinces.is_empty();
        let accepted = accepts_terms(score.total_milli, cost_milli, white_peace);
        let truce_until = truce_expiry(date, cost_milli / 1_000).ok_or(PeaceError::DateOutOfRange)?;
        let reason = if white_peace {
            format!("白和平可以缔结，停战至 {truce_until} 前")
        } else if accepted {
            format!(
                "可以接受：战争分数 {}，条款 {}，停战至 {truce_until} 前",
                format_warscore_milli(score.total_milli),
                format_warscore_milli(cost_milli)
            )
        } else {
            format!(
                "战争分数不足：现有 {}，条款需要 {}",
                format_warscore_milli(score.total_milli),
                format_warscore_milli(cost_milli)
            )
        };
        Ok(PeaceQuote {
            score_milli: score.total_milli,
            cost_milli,
            accepted,
            reason,
            demands,
            truce_until,
        })
    }

    /// 和谈打开时一次取回分数来源、对方全部陆地和白和平。
    pub fn peace_sheet(
        &self,
        offerer: &str,
        target: &str,
        date: GameDate,
    ) -> Result<PeaceSheet, PeaceError> {
        let war = self
            .war_between(offerer, target)
            .ok_or(PeaceError::NotAtWar)?;
        let score = self.score_view(offerer, war.attacker(), war.defender(), war.battles());
        let mut provinces = Vec::new();
        for id in &self.active_provinces {
            if self.province(*id).is_some_and(|province| {
                province.kind == ProvinceKind::Land && province.owner.as_deref() == Some(target)
            }) {
                provinces.extend(self.demand_costs(offerer, target, &[*id])?);
            }
        }
        let white_peace = self.quote_peace(offerer, target, &[], date)?;
        Ok(PeaceSheet {
            score,
            provinces,
            white_peace,
        })
    }

    /// 缔结白和平或割让。非法或分数不足的条款不会改变战争和省份。
    pub fn make_peace(
        &mut self,
        offerer: &str,
        target: &str,
        provinces: &[u32],
        date: GameDate,
    ) -> Result<GameDate, PeaceError> {
        let quote = self.quote_peace(offerer, target, provinces, date)?;
        if !quote.accepted {
            return Err(PeaceError::InsufficientScore {
                score: format_warscore_milli(quote.score_milli),
                cost: format_warscore_milli(quote.cost_milli),
            });
        }
        self.apply_peace(offerer, target, provinces, date, quote.truce_until);
        Ok(quote.truce_until)
    }

    /// 只记录正常结束的战斗。和约中止的战斗没有胜负，不进入账本。
    pub(crate) fn record_finished_battle(
        &mut self,
        province: u32,
        on: GameDate,
        sides: &[ReportedSide; 2],
        loser: Option<u8>,
    ) {
        let Some(loser) = loser else {
            return;
        };
        let loser_index = usize::from(loser);
        if loser_index > 1 {
            return;
        }
        let winner_index = 1 - loser_index;
        let [loser_country] = sides[loser_index].countries.as_slice() else {
            return;
        };
        let [winner_country] = sides[winner_index].countries.as_slice() else {
            return;
        };
        let Some(war) = self.war_between(loser_country, winner_country) else {
            return;
        };
        let attacker = war.attacker().to_owned();
        let war_id = war.id();
        let loser_limit = self.country_land_force_limit(loser_country).unwrap_or(0);
        let attacker_limit = self.country_land_force_limit(&attacker).unwrap_or(0);
        let milli = land_battle_milli(sides[loser_index].lost, loser_limit.min(attacker_limit));
        let attacker_milli = if loser_country == &attacker { -milli } else { milli };
        self.war_state
            .active
            .get_mut(&war_id)
            .expect("战斗所属战争存在")
            .record_battle(BattleScore::new(
                province,
                on,
                attacker_milli,
                loser_country.clone(),
                sides[loser_index].lost,
            ));
    }

    fn score_view(
        &self,
        viewer: &str,
        attacker: &str,
        defender: &str,
        battles: &[BattleScore],
    ) -> WarScoreView {
        let (attacker_occupied, defender_total) = self.occupied_development(defender, attacker);
        let (defender_occupied, attacker_total) = self.occupied_development(attacker, defender);
        let occupation = occupation_share_milli(attacker_occupied, defender_total)
            - occupation_share_milli(defender_occupied, attacker_total);
        let battle_unclamped = battles
            .iter()
            .fold(0_i64, |sum, battle| {
                sum.saturating_add(battle.attacker_milli())
            });
        let battle = clamp_battle_milli(battle_unclamped);
        let total = clamp_warscore_milli(occupation.saturating_add(battle));
        let mut view_battles = Vec::with_capacity(battles.len());
        for battle in battles {
            view_battles.push(ViewerBattle {
                province: battle.province(),
                on: battle.on(),
                loser: battle.loser().to_owned(),
                losses: battle.losses(),
                milli: battle.attacker_milli(),
            });
        }
        let mut view = WarScoreView {
            total_milli: total,
            battle_milli: battle,
            battle_unclamped_milli: battle_unclamped,
            occupation_milli: occupation,
            battles: view_battles,
            occupied_enemy_development: attacker_occupied,
            enemy_development: defender_total,
            enemy_occupied_development: defender_occupied,
            own_development: attacker_total,
        };
        if viewer == defender {
            view.total_milli = -view.total_milli;
            view.battle_milli = -view.battle_milli;
            view.battle_unclamped_milli = -view.battle_unclamped_milli;
            view.occupation_milli = -view.occupation_milli;
            for battle in &mut view.battles {
                battle.milli = -battle.milli;
            }
            std::mem::swap(
                &mut view.occupied_enemy_development,
                &mut view.enemy_occupied_development,
            );
            std::mem::swap(&mut view.enemy_development, &mut view.own_development);
        }
        view
    }

    /// 对方拥有、且控制者是占领方的发展度，以及对方的总发展度。
    fn occupied_development(&self, owner: &str, occupier: &str) -> (u64, u64) {
        let mut occupied = 0;
        let mut total = 0;
        for id in &self.active_provinces {
            let Some(province) = self.province(*id) else {
                continue;
            };
            if province.kind != ProvinceKind::Land || province.owner.as_deref() != Some(owner) {
                continue;
            }
            let development = province.development.total().unwrap_or(0);
            total += development;
            if province.controller.as_deref() == Some(occupier) {
                occupied += development;
            }
        }
        (occupied, total)
    }

    fn demand_costs(
        &self,
        offerer: &str,
        target: &str,
        provinces: &[u32],
    ) -> Result<Vec<ProvinceCost>, PeaceError> {
        let mut seen = BTreeSet::new();
        let mut demands = Vec::with_capacity(provinces.len());
        let capital = self.country(target).and_then(|country| country.capital);
        for &id in provinces {
            let province = self
                .province(id)
                .filter(|province| {
                    self.active_provinces.contains(&id) && province.kind == ProvinceKind::Land
                })
                .ok_or(PeaceError::NotCession)?;
            if province.owner.as_deref() != Some(target) || !seen.insert(id) {
                return Err(PeaceError::NotCession);
            }
            let development = province.development.total().unwrap_or(0);
            let controlled_by_offerer = province.controller.as_deref() == Some(offerer);
            demands.push(ProvinceCost {
                province: id,
                development,
                cost_milli: province_cost_milli(
                    development,
                    capital == Some(id),
                    controlled_by_offerer,
                ),
                capital: capital == Some(id),
                controlled_by_offerer,
            });
        }
        Ok(demands)
    }

    fn apply_peace(
        &mut self,
        offerer: &str,
        target: &str,
        provinces: &[u32],
        date: GameDate,
        until: GameDate,
    ) {
        let offerer = offerer.to_owned();
        let target = target.to_owned();
        self.abort_pair_battles(&offerer, &target);
        let ceded: BTreeSet<u32> = provinces.iter().copied().collect();
        let mut controllers = Vec::new();
        let ids: Vec<u32> = self.active_provinces.iter().copied().collect();
        for id in ids {
            let Some(province) = self.provinces.get(&id) else {
                continue;
            };
            if province.kind != ProvinceKind::Land {
                continue;
            }
            let owner = province.owner.clone();
            let controller = province.controller.clone();
            let involved = ceded.contains(&id)
                || owner.as_deref() == Some(offerer.as_str())
                || owner.as_deref() == Some(target.as_str());
            if !involved {
                continue;
            }
            // 已付款的建设继续按完工日建成，不因和约退款。
            if ceded.contains(&id) {
                let province = self.provinces.get_mut(&id).expect("割让省份存在");
                province.owner = Some(offerer.clone());
                province.controller = Some(offerer.clone());
                province.occupation = None;
                if controller.as_deref() != Some(offerer.as_str())
                    || owner.as_deref() != Some(offerer.as_str())
                {
                    self.control_changed = true;
                }
                controllers.push((id, offerer.clone()));
            } else if let Some(owner) = owner.clone() {
                let held_by_belligerent = controller.as_deref() != Some(owner.as_str())
                    && (controller.as_deref() == Some(offerer.as_str())
                        || controller.as_deref() == Some(target.as_str()));
                if held_by_belligerent {
                    self.provinces.get_mut(&id).expect("归还省份存在").controller =
                        Some(owner.clone());
                    self.control_changed = true;
                    controllers.push((id, owner));
                }
                self.provinces.get_mut(&id).expect("交战省份存在").occupation = None;
            }
        }
        for (id, controller) in controllers {
            self.cancel_uncontrolled_recruitment(id, &controller, date);
        }
        self.ensure_owned_capital(&offerer);
        self.ensure_owned_capital(&target);
        // 先结束战争，境外土地才不再可进入，返回路线才能按停战后的边界计算。
        self.war_state
            .active
            .retain(|_, war| !war.binds(&offerer, &target));
        self.settle_armies_after_peace(&offerer, &target, date);
        self.upsert_truce(&offerer, &target, until);
    }

    fn abort_pair_battles(&mut self, left: &str, right: &str) {
        let ids: Vec<u32> = self.military.battles.keys().copied().collect();
        for id in ids {
            let mut countries = BTreeSet::new();
            for side in 0..2 {
                for army_id in self.military.battles[&id].side_armies(side) {
                    if let Some(country) = self.armies().get(army_id).map(|army| army.country.clone())
                    {
                        countries.insert(country);
                    }
                }
            }
            let only_pair = countries.len() == 2
                && countries.contains(left)
                && countries.contains(right);
            if only_pair {
                self.disengage(id);
            }
        }
    }

    fn ensure_owned_capital(&mut self, tag: &str) {
        let still_owned = self.countries.get(tag).and_then(|country| country.capital).is_some_and(
            |id| self.provinces.get(&id).and_then(|province| province.owner.as_deref()) == Some(tag),
        );
        if still_owned {
            return;
        }
        let replacement = self.provinces.iter().find_map(|(id, province)| {
            (self.active_provinces.contains(id)
                && province.kind == ProvinceKind::Land
                && province.owner.as_deref() == Some(tag))
            .then_some(*id)
        });
        if let Some(country) = self.countries.get_mut(tag) {
            country.capital = replacement;
        }
    }

    fn settle_armies_after_peace(&mut self, left: &str, right: &str, date: GameDate) {
        let ids: Vec<u32> = self
            .military
            .armies
            .iter()
            .filter(|(_, army)| army.country == left || army.country == right)
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            let Some(army) = self.military.armies.get(&id) else {
                continue;
            };
            let country = army.country.clone();
            let province = army.province;
            if self.can_enter(&country, province) {
                let at_home = self
                    .province(province)
                    .and_then(|province| province.owner.as_deref())
                    == Some(country.as_str());
                if at_home && let Some(army) = self.military.armies.get_mut(&id) {
                    army.exiled = false;
                }
                self.trim_closed_route(id);
            } else {
                self.send_army_home(id, date);
            }
        }
    }

    /// 当前省份仍可进入时，删掉停战后不能进入的后续路段。
    fn trim_closed_route(&mut self, id: u32) {
        let Some(army) = self.military.armies.get(&id) else {
            return;
        };
        if army.exiled {
            return;
        }
        let Some(movement) = army.movement.clone() else {
            return;
        };
        let country = army.country.clone();
        let mut end = movement.legs.len();
        for (index, leg) in movement.legs.iter().enumerate().skip(movement.leg_index) {
            if !self.can_enter(&country, leg.to) {
                end = index;
                break;
            }
        }
        let army = self.military.armies.get_mut(&id).expect("调整行军的军队存在");
        if end <= movement.leg_index {
            army.movement = None;
            if army.retreating {
                army.retreating = false;
                army.retreat_shattered = Some(false);
            }
        } else if end < movement.legs.len() {
            army.movement
                .as_mut()
                .expect("被截断的行军存在")
                .legs
                .truncate(end);
        }
    }

    /// 无法进入当前土地时返回最近的本国省份。没有陆地或没有陆路时移除军队，避免永远停在境外。
    fn send_army_home(&mut self, id: u32, date: GameDate) {
        let Some(army) = self.military.armies.get(&id) else {
            return;
        };
        let country = army.country.clone();
        let from = army.province;
        let speed = army.march_speed(&self.military_rules);
        let Some(home) = self.nearest_owned_land(&country, from) else {
            self.military.armies.remove(&id);
            return;
        };
        match self.land_route(&country, from, home, speed, date, None, true) {
            Ok(movement) => {
                let army = self
                    .military
                    .armies
                    .get_mut(&id)
                    .expect("返回本国的军队存在");
                army.movement = Some(movement);
                army.exiled = true;
                army.retreating = false;
                army.retreat_shattered = Some(false);
                army.battle_id = None;
            }
            Err(_) => {
                self.military.armies.remove(&id);
            }
        }
    }

    fn upsert_truce(&mut self, left: &str, right: &str, until: GameDate) {
        let mut countries = [left.to_owned(), right.to_owned()];
        countries.sort();
        self.war_state
            .truces
            .retain(|truce| !truce.binds(left, right));
        self.war_state.truces.push(Truce::new(countries, until));
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use serde_json::json;

    use crate::{
        Army, Battle, Construction, Country, Development, GameDate, LandConnection, ManpowerRules,
        MilitaryRules, Money, PeaceError, Province, ProvinceKind, Regiment, ReportedSide,
        SaveError, SavedGame, Simulation, UnitKind, WarError, CombatRules,
    };

    use super::World;

    fn date() -> GameDate {
        GameDate::new(1444, 11, 11).unwrap()
    }

    fn province(owner: &str) -> Province {
        let mut province = Province::new(ProvinceKind::Land);
        province.owner = Some(owner.to_owned());
        province.controller = Some(owner.to_owned());
        province.development = Development {
            tax: Some(3),
            production: Some(3),
            manpower: Some(3),
        };
        province
    }

    fn rules() -> MilitaryRules {
        MilitaryRules {
            infantry_cost: Money::from(10),
            cavalry_cost: Money::from(25),
            infantry_days: 60,
            cavalry_days: 90,
            infantry_speed: Money::from(1),
            cavalry_speed: Money::from(1),
            recruitment_full_refund_progress: Money::ONE,
            land_maintenance_factor: Money::ZERO,
            monthly_reinforce: Money::ZERO,
            extra_land_reinforce_cost: Money::ZERO,
            supply_base: 6,
            base_land_forcelimit: Money::from(11),
            development_land_forcelimit: Money::ZERO,
            land_forcelimit_extra_cost_factor: Money::from(2),
            goods_land_forcelimit: BTreeMap::new(),
            tax_recruitment_time: Money::ZERO,
            starting_army_size: Money::ZERO,
            starting_army_size_at_war: Money::ZERO,
            manpower: ManpowerRules {
                base_capacity: 20_000,
                per_development: 0,
                recovery_months: 10,
                minimum_monthly: 1,
                infantry_cost: 1_000,
                cavalry_cost: 1_000,
            },
            combat: CombatRules::baseline(),
        }
    }

    fn sample() -> World {
        World::new(
            [Country::new("AAA"), Country::new("BBB"), Country::new("CCC")],
            BTreeMap::from([
                (1, province("AAA")),
                (5, province("AAA")),
                (2, province("BBB")),
                (4, province("BBB")),
                (3, province("CCC")),
            ]),
            [1, 2, 3, 4, 5],
            Vec::<crate::BuildingDefinition>::new(),
            rules(),
            [
                LandConnection { from: 1, to: 5, distance: 1 },
                LandConnection { from: 1, to: 2, distance: 1 },
                LandConnection { from: 2, to: 4, distance: 1 },
                LandConnection { from: 1, to: 3, distance: 1 },
                LandConnection { from: 3, to: 4, distance: 1 },
            ],
        )
        .unwrap()
    }

    fn station(world: &mut World, id: u32, country: &str, province: u32) {
        world.military.armies.insert(
            id,
            Army {
                id,
                country: country.to_owned(),
                province,
                regiments: vec![Regiment::fresh(UnitKind::Infantry, 1_000)],
                movement: None,
                battle_id: None,
                retreating: false,
                retreat_shattered: Some(false),
                exiled: false,
                retreat_previous: None,
                retreat_return: None,
            },
        );
        world.military.next_army_id = world.military.next_army_id.max(id + 1);
    }

    fn side(country: &str, lost: u64) -> ReportedSide {
        ReportedSide {
            countries: vec![country.to_owned()],
            army_ids: Vec::new(),
            strength: 0,
            morale: 0,
            lost,
        }
    }

    fn record_loss(world: &mut World, losses: u64) {
        world.record_finished_battle(
            2,
            date(),
            &[side("BBB", losses), side("AAA", 0)],
            Some(0),
        );
    }

    #[test]
    fn battle_and_occupation_explain_warscore_and_cession_is_limited() {
        let mut world = sample();
        let date = date();
        world.declare_war("AAA", "BBB", date).unwrap();
        world.declare_war("AAA", "CCC", date).unwrap();
        record_loss(&mut world, 10_000);
        let score = world.war_score("AAA", 1).unwrap();
        assert_eq!(score.total_milli, 9_080);
        assert_eq!(score.battle_milli, 9_080);
        assert!(score.battle_text().contains("10000"));
        assert_eq!(world.war_score("BBB", 1).unwrap().total_milli, -9_080);

        world.provinces.get_mut(&4).unwrap().controller = Some("AAA".to_owned());
        let occupied = world.war_score("AAA", 1).unwrap();
        assert_eq!(occupied.occupation_milli, 50_000);
        assert_eq!(occupied.total_milli, 59_080);
        assert_eq!(occupied.occupied_enemy_development, 9);
        assert_eq!(occupied.enemy_development, 18);

        world.countries.get_mut("BBB").unwrap().capital = Some(2);
        world
            .countries
            .get_mut("BBB")
            .unwrap()
            .finances
            .refund(Money::from(100));
        world
            .start_recruitment("BBB", 2, UnitKind::Infantry, date)
            .unwrap();
        world.provinces.get_mut(&2).unwrap().construction = Some(Construction {
            building: "temple".to_owned(),
            country: "BBB".to_owned(),
            started_on: date,
            completes_on: date.checked_add_months(12).unwrap(),
        });

        let rejected = world.make_peace("AAA", "BBB", &[2, 3], date);
        assert_eq!(rejected, Err(PeaceError::NotCession));
        assert_eq!(world.province(3).unwrap().owner.as_deref(), Some("CCC"));
        assert!(world.at_war("AAA", "BBB"));

        let until = world.make_peace("AAA", "BBB", &[2], date).unwrap();
        assert_eq!(until, GameDate::new(1450, 10, 2).unwrap());
        assert!(!world.at_war("AAA", "BBB"));
        assert!(world.at_war("AAA", "CCC"));
        assert_eq!(world.province(2).unwrap().owner.as_deref(), Some("AAA"));
        assert_eq!(world.province(2).unwrap().controller.as_deref(), Some("AAA"));
        assert_eq!(world.province(4).unwrap().owner.as_deref(), Some("BBB"));
        assert_eq!(world.province(4).unwrap().controller.as_deref(), Some("BBB"));
        assert!(world.province(4).unwrap().occupation.is_none());
        assert!(world.recruitment_queue(2).is_empty());
        assert_eq!(
            world.province(2).unwrap().construction.as_ref().unwrap().building,
            "temple"
        );
        assert_eq!(world.country("BBB").unwrap().capital, Some(4));
        assert!(world.take_control_changed());
        assert_eq!(
            world.declare_war("AAA", "BBB", date),
            Err(WarError::Truce { until })
        );
        assert!(world.declare_war("AAA", "BBB", until).is_ok());
    }

    #[test]
    fn battle_subtotal_is_clamped_and_over_cost_terms_do_not_apply() {
        let mut world = sample();
        let date = date();
        world.declare_war("AAA", "BBB", date).unwrap();
        for _ in 0..5 {
            record_loss(&mut world, 20_000);
        }
        let score = world.war_score("AAA", 1).unwrap();
        assert_eq!(score.battle_unclamped_milli, 50_000);
        assert_eq!(score.battle_milli, 40_000);
        assert_eq!(score.total_milli, 40_000);

        let mut poor = sample();
        poor.declare_war("AAA", "BBB", date).unwrap();
        let quote = poor.quote_peace("AAA", "BBB", &[2], date).unwrap();
        assert!(!quote.accepted);
        assert_eq!(
            poor.make_peace("AAA", "BBB", &[2], date),
            Err(PeaceError::InsufficientScore {
                score: "0.000".to_owned(),
                cost: "7.920".to_owned(),
            })
        );
        assert!(poor.at_war("AAA", "BBB"));
        assert_eq!(poor.province(2).unwrap().owner.as_deref(), Some("BBB"));
        assert!(poor.make_peace("AAA", "BBB", &[], date).is_ok());
        assert!(!poor.at_war("AAA", "BBB"));
    }

    #[test]
    fn white_peace_restores_control_and_stops_the_war() {
        let mut world = sample();
        let date = date();
        world.declare_war("AAA", "BBB", date).unwrap();
        world.provinces.get_mut(&1).unwrap().controller = Some("BBB".to_owned());
        world.provinces.get_mut(&1).unwrap().occupation = Some(crate::OccupationProgress {
            occupier: "BBB".to_owned(),
            days: 10,
        });
        let mut battle = Battle::new(1, 2, date);
        battle.side_mut(0).armies = vec![1];
        battle.side_mut(1).armies = vec![2];
        station(&mut world, 1, "AAA", 4);
        station(&mut world, 2, "BBB", 2);
        world.military.armies.get_mut(&1).unwrap().battle_id = Some(1);
        world.military.armies.get_mut(&2).unwrap().battle_id = Some(1);
        world.military.battles.insert(1, battle);
        world.military.next_battle_id = 2;
        let strength = world.armies()[&1].strength();

        let until = world.make_peace("BBB", "AAA", &[], date).unwrap();
        assert_eq!(until, GameDate::new(1449, 12, 2).unwrap());
        assert_eq!(world.province(1).unwrap().controller.as_deref(), Some("AAA"));
        assert_eq!(world.province(1).unwrap().owner.as_deref(), Some("AAA"));
        assert!(world.province(1).unwrap().occupation.is_none());
        assert!(world.battles().is_empty());
        assert!(world.armies()[&1].battle_id().is_none());
        assert!(world.armies()[&1].is_exiled());
        assert_eq!(world.armies()[&1].movement.as_ref().unwrap().destination(), 1);
        assert!(!world.armies()[&2].is_exiled());

        let saved = SavedGame::capture(&world, &Simulation::new(date), "AAA");
        let restored = saved.restore(&world).unwrap();
        assert_eq!(
            restored.world.armies()[&1].movement,
            world.armies()[&1].movement
        );
        assert!(restored.world.truce_between("AAA", "BBB").unwrap().active_on(date));

        let mut simulation = Simulation::new(date);
        simulation.set_paused(false);
        assert_eq!(simulation.advance(&mut world, Duration::from_secs(2)), 2);
        assert_eq!(world.armies()[&1].province, 1);
        assert!(!world.armies()[&1].is_exiled());
        assert_eq!(world.armies()[&1].strength(), strength);
        assert!(world.battles().is_empty());
        assert_eq!(world.province(1).unwrap().controller.as_deref(), Some("AAA"));
        assert!(!world.at_war("AAA", "BBB"));
    }

    #[test]
    fn closed_march_is_trimmed_and_old_war_fields_still_load() {
        let mut world = sample();
        let date = date();
        world.declare_war("AAA", "BBB", date).unwrap();
        station(&mut world, 1, "AAA", 1);
        world.move_army("AAA", 1, 4, date).unwrap();
        world.make_peace("AAA", "BBB", &[], date).unwrap();
        let army = &world.armies()[&1];
        assert_eq!(army.province, 1);
        assert!(army.movement.is_none());
        assert!(!army.is_exiled());

        let later = GameDate::new(1460, 1, 1).unwrap();
        world.declare_war("AAA", "BBB", later).unwrap();
        let saved = SavedGame::capture(&world, &Simulation::new(later), "AAA");
        let mut value = serde_json::to_value(&saved).unwrap();
        value["wars"]["active"]["2"]
            .as_object_mut()
            .unwrap()
            .remove("battles");
        value["wars"].as_object_mut().unwrap().remove("truces");
        let restored = serde_json::from_value::<SavedGame>(value)
            .unwrap()
            .restore(&world)
            .unwrap();
        assert!(restored.world.at_war("AAA", "BBB"));
        assert!(restored.world.truce_between("AAA", "BBB").is_none());

        let mut duplicated = serde_json::to_value(&saved).unwrap();
        duplicated["wars"]["truces"] = json!([
            {
                "countries": ["AAA", "BBB"],
                "until": "1450.1.1"
            },
            {
                "countries": ["BBB", "AAA"],
                "until": "1451.1.1"
            }
        ]);
        match serde_json::from_value::<SavedGame>(duplicated)
            .unwrap()
            .restore(&world)
        {
            Err(SaveError::InvalidWar) => {}
            Ok(_) => panic!("重复停战不应读入"),
            Err(error) => panic!("应为无效战争，实际是 {error}"),
        }
    }
}
