// SPDX-License-Identifier: GPL-3.0-only

use crate::{GameDate, Money, RecruitmentError, RecruitmentRefund, UnitKind};

use super::World;

impl World {
    pub fn quote_cancel_recruitment(
        &self,
        country: &str,
        province: u32,
        date: GameDate,
    ) -> Result<RecruitmentRefund, RecruitmentError> {
        self.country(country)
            .ok_or(RecruitmentError::UnknownCountry)?;
        let project = self
            .recruitment(province)
            .ok_or(RecruitmentError::NotRecruiting)?;
        if project.country != country {
            return Err(RecruitmentError::NotPlayerRecruitment);
        }
        if date < project.started_on || date >= project.completes_on {
            return Err(RecruitmentError::RecruitmentNotActive);
        }
        let elapsed = Money::from(project.started_on.days_until(date));
        let total = Money::from(project.started_on.days_until(project.completes_on));
        let mut cost = project.paid_cost.expect("招募实付金币已记录");
        // 原版使用严格大于分界的比较；不使用界面四舍五入后的百分比。
        if elapsed > total * self.military_rules.recruitment_full_refund_progress {
            // 原版金币以千分之一为单位，半额退款不足这一单位的部分舍去。
            cost = (cost / Money::TWO).trunc_with_scale(3);
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
        date: GameDate,
    ) -> Result<RecruitmentRefund, RecruitmentError> {
        let refund = self.quote_cancel_recruitment(country, province, date)?;
        // 移除项目后才能返还资源，同一项目无法重复退款；预留编号不重新使用。
        self.military
            .recruitments
            .remove(&province)
            .expect("取消项目已检查");
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
