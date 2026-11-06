// SPDX-License-Identifier: GPL-3.0-only

use crate::{GameDate, Money, Recruitment, RecruitmentError, RecruitmentRefund, UnitKind};

use super::World;

impl World {
    pub fn quote_cancel_recruitment(
        &self,
        country: &str,
        province: u32,
        order_id: u32,
        date: GameDate,
    ) -> Result<RecruitmentRefund, RecruitmentError> {
        self.country(country)
            .ok_or(RecruitmentError::UnknownCountry)?;
        let queue = self.recruitment_queue(province);
        if queue.is_empty() {
            return Err(RecruitmentError::NotRecruiting);
        }
        let project = queue
            .iter()
            .find(|order| order.id() == order_id)
            .ok_or(RecruitmentError::UnknownOrder)?;
        if project.country != country {
            return Err(RecruitmentError::NotPlayerRecruitment);
        }
        if date >= project.completes_on {
            return Err(RecruitmentError::RecruitmentNotActive);
        }
        let total = Money::from(project.started_on.days_until(project.completes_on));
        let mut cost = project.paid_cost.expect("招募实付金币已记录");
        // 排队订单进度为零，全额退回实付金币。进行中的订单沿用原版严格大于分界的比较。
        if date >= project.started_on {
            let elapsed = Money::from(project.started_on.days_until(date));
            if elapsed > total * self.military_rules.recruitment_full_refund_progress {
                // 原版金币以千分之一为单位，半额退款不足这一单位的部分舍去。
                cost = (cost / Money::TWO).trunc_with_scale(3);
            }
        }
        let manpower = if project.paid_manpower == Some(0) {
            // 人力系统启用前的旧项目没有扣过人力，仍不补发退款。
            0
        } else {
            // 当前只支持普通满编步兵团，返还量按当前适用需求计算，独立于金币比例。
            match project.kind {
                UnitKind::Infantry => self.military_rules.manpower.infantry_cost,
            }
        };
        Ok(RecruitmentRefund { cost, manpower })
    }

    pub fn cancel_recruitment(
        &mut self,
        country: &str,
        province: u32,
        order_id: u32,
        date: GameDate,
    ) -> Result<RecruitmentRefund, RecruitmentError> {
        let refund = self.quote_cancel_recruitment(country, province, order_id, date)?;
        let queue = self
            .military
            .recruitments
            .get_mut(&province)
            .expect("取消队列已检查");
        let index = queue
            .iter()
            .position(|order| order.id() == order_id)
            .expect("取消订单已检查");
        // 移除项目后才能返还资源，同一项目无法重复退款；预留编号不重新使用。
        queue.remove(index);
        reschedule_queue(queue, date);
        if queue.is_empty() {
            self.military.recruitments.remove(&province);
        }
        let country = self.countries.get_mut(country).expect("招募国家已注册");
        country.finances.refund(refund.cost);
        country
            .manpower
            .as_mut()
            .expect("国家人力已初始化")
            .refund(refund.manpower);
        Ok(refund)
    }
}

/// 取消后让剩余订单重新首尾相接。被取消的若是队首，新队首从当天起算，工期不变。
fn reschedule_queue(queue: &mut [Recruitment], date: GameDate) {
    if queue.is_empty() {
        return;
    }
    if queue[0].started_on > date {
        let duration = order_duration(&queue[0]);
        queue[0].started_on = date;
        queue[0].completes_on = date
            .checked_add_days(duration)
            .expect("前移后的完成日仍在历法内");
    }
    for index in 1..queue.len() {
        let duration = order_duration(&queue[index]);
        let started_on = queue[index - 1].completes_on;
        queue[index].started_on = started_on;
        queue[index].completes_on = started_on
            .checked_add_days(duration)
            .expect("前移后的完成日仍在历法内");
    }
}

fn order_duration(order: &Recruitment) -> u32 {
    u32::try_from(order.started_on.days_until(order.completes_on)).expect("招募工期为正")
}
