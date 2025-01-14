use std::collections::HashSet;

use serde::Deserialize;

use crate::AddressString;

#[derive(Debug, Deserialize)]
pub(crate) struct ConfigVersion1 {
    pub org_nodes: HashSet<AddressString>,
    pub whitelisted_nodes: Option<HashSet<AddressString>>,
    pub whitelisted_block_proposers: Option<HashSet<AddressString>>,
    pub blacklisted_nodes: Option<HashSet<AddressString>>,
    pub blacklisted_block_proposers: Option<HashSet<AddressString>>,
    pub max_block_height: Option<u64>,
    pub max_validators: u64,
    pub xscore: XscoreConfigVersion1,
    pub stake_score: StakeScoreConfigVersion1,
    pub kin_score: KinScoreConfigVersion1,
    pub rewards: ValidatorRewardsVersion1,
}

#[derive(Debug, Deserialize)]
pub struct XscoreConfigVersion1 {
    pub xscore_threshold: f64,
    pub weight_for_stakescore: f64,
    pub weight_for_kinscore: f64,
}
#[derive(Debug, Deserialize)]
pub struct StakeScoreConfigVersion1 {
    pub min_balance: l1x_sdk::types::U128,
    pub max_balance: l1x_sdk::types::U128,
    pub min_stake_age: u64,
    pub max_stake_age: u64,
    pub min_lock_period: u64,
    pub max_lock_period: u64,
    pub weight_for_stake_balance_6: f64,
    pub weight_for_stake_age_6: f64,
    pub weight_for_locking_period_6: f64,
    pub weight_for_stake_balance_12: f64,
    pub weight_for_stake_age_12: f64,
    pub weight_for_locking_period_12: f64,
    pub weight_for_stake_balance_18: f64,
    pub weight_for_stake_age_18: f64,
    pub weight_for_locking_period_18: f64,
}

#[derive(Debug, Deserialize)]
pub struct KinScoreConfigVersion1 {
    pub min_uptime: f64,
    pub max_uptime: f64,
    pub min_participation: u32,
    pub max_participation: u32,
    pub min_response_time: u64,
    pub max_response_time: u64,
    pub min_security_measure: f64,
    pub max_security_measure: f64,
    pub xscore_threshold: f64,
    pub weight_for_uptime: f64,
    pub weight_for_participation_history: f64,
    pub weight_for_response_time: f64,
    pub weight_for_security_measure: f64,
}

#[derive(Debug, Deserialize)]
pub struct ValidatorRewardsVersion1 {
    pub validated_block_reward: l1x_sdk::types::U128,
}
