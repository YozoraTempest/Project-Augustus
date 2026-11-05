// SPDX-License-Identifier: GPL-3.0-only

use crate::{GameDate, RecruitmentError, RecruitmentRefund};

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
        let pool = self.country_manpower(country).expect("招募国家已注册");
        Ok(RecruitmentRefund {
            cost: project.paid_cost.expect("招募实付金币已记录"),
            manpower: project
                .paid_manpower
                .expect("招募实付人力已记录")
                .min(pool.capacity - pool.available),
        })
    }

    pub fn cancel_recruitment(
        &mut self,
        country: &str,
        province: u32,
        date: GameDate,
    ) -> Result<RecruitmentRefund, RecruitmentError> {
        let refund = self.quote_cancel_recruitment(country, province, date)?;
        let capacity = self
            .country_manpower(country)
            .expect("招募国家已注册")
            .capacity;
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
            .refund(refund.manpower, capacity);
        Ok(refund)
    }
}
