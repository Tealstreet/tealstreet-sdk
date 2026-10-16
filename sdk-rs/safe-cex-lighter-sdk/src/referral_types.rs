//! Referral value declarations with shared nested records and ordered token arrays.
use std::{cell::RefCell, rc::Rc};

pub struct ReferralRewardToken {
    pub token: Vec<u16>,
    pub amount: Vec<u16>,
}
pub struct ReferralRewards {
    pub total_usd: Option<Vec<u16>>,
    pub by_token: Option<Rc<RefCell<Vec<ReferralRewardToken>>>>,
}
pub struct ReferralPoints {
    pub account_index: Vec<u16>,
    pub total_points: f64,
    pub available_points: Option<f64>,
    pub pending_points: Option<f64>,
    pub referral_count: Option<f64>,
    pub tier: Option<Vec<u16>>,
    pub rewards: Option<Rc<RefCell<ReferralRewards>>>,
}
impl ReferralPoints {
    pub fn new(account_index: Vec<u16>, total_points: f64) -> Self {
        Self {
            account_index,
            total_points,
            available_points: None,
            pending_points: None,
            referral_count: None,
            tier: None,
            rewards: None,
        }
    }
}
pub struct ReferralPointsResponse {
    pub referral_points: Rc<RefCell<ReferralPoints>>,
}
impl ReferralPointsResponse {
    pub fn new(referral_points: Rc<RefCell<ReferralPoints>>) -> Self {
        Self { referral_points }
    }
}
