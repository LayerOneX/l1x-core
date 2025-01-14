use crate::config_v1::{
    ConfigVersion1, KinScoreConfigVersion1, StakeScoreConfigVersion1, ValidatorRewardsVersion1, XscoreConfigVersion1,
};

use anyhow::{anyhow, Error};
use once_cell::sync::Lazy;
use primitives::Address;
use serde::Deserialize;
use std::{collections::BTreeSet, sync::Arc};
use tokio::sync::RwLock;

static CACHED_CONFIG: Lazy<RwLock<Option<Arc<RuntimeConfigCache>>>> = Lazy::new(|| RwLock::new(None));

#[derive(Debug, Deserialize)]
struct TomlConfig {
    version: u64,
    version_1: ConfigVersion1,
}

#[derive(Debug)]
pub struct XscoreConfig {
    pub xscore_threshold: f64,
    pub weight_for_stakescore: f64,
    pub weight_for_kinscore: f64,
}

impl From<XscoreConfigVersion1> for XscoreConfig {
    fn from(v: XscoreConfigVersion1) -> Self {
        Self {
            weight_for_kinscore: v.weight_for_kinscore,
            weight_for_stakescore: v.weight_for_stakescore,
            xscore_threshold: v.xscore_threshold,
        }
    }
}
#[derive(Debug)]
pub struct StakeScoreConfig {
    pub min_balance: u128,
    pub max_balance: u128,
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

impl From<StakeScoreConfigVersion1> for StakeScoreConfig {
    fn from(v: StakeScoreConfigVersion1) -> Self {
        Self {
            min_balance: v.min_balance.into(),
            max_balance: v.max_balance.into(),
            min_stake_age: v.min_stake_age,
            max_stake_age: v.max_stake_age,
            min_lock_period: v.min_lock_period,
            max_lock_period: v.max_lock_period,
            weight_for_stake_balance_6: v.weight_for_stake_balance_6,
            weight_for_stake_age_6: v.weight_for_stake_age_6,
            weight_for_locking_period_6: v.weight_for_locking_period_6,
            weight_for_stake_balance_12: v.weight_for_stake_balance_12,
            weight_for_stake_age_12: v.weight_for_stake_age_12,
            weight_for_locking_period_12: v.weight_for_locking_period_12,
            weight_for_stake_balance_18: v.weight_for_stake_balance_18,
            weight_for_stake_age_18: v.weight_for_stake_age_18,
            weight_for_locking_period_18: v.weight_for_locking_period_18,
        }
    }
}

#[derive(Debug)]
pub struct KinScoreConfig {
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

impl From<KinScoreConfigVersion1> for KinScoreConfig {
    fn from(v: KinScoreConfigVersion1) -> Self {
        Self {
            min_uptime: v.min_uptime,
            max_uptime: v.max_uptime,
            min_participation: v.min_participation,
            max_participation: v.max_participation,
            min_response_time: v.min_response_time,
            max_response_time: v.max_response_time,
            min_security_measure: v.min_security_measure,
            max_security_measure: v.max_security_measure,
            xscore_threshold: v.xscore_threshold,
            weight_for_uptime: v.weight_for_uptime,
            weight_for_participation_history: v.weight_for_participation_history,
            weight_for_response_time: v.weight_for_response_time,
            weight_for_security_measure: v.weight_for_security_measure,
        }
    }
}

#[derive(Debug)]
pub struct ValidatorRewards {
    pub validated_block_reward: l1x_sdk::types::U128,
}

impl From<ValidatorRewardsVersion1> for ValidatorRewards {
    fn from(v: ValidatorRewardsVersion1) -> Self {
        Self {
            validated_block_reward: v.validated_block_reward,
        }
    }
}

#[derive(Debug)]
pub struct RuntimeConfigCache {
    pub org_nodes: BTreeSet<Address>,
    pub whitelisted_nodes: Option<BTreeSet<Address>>,
    pub whitelisted_block_proposers: Option<BTreeSet<Address>>,
    pub blacklisted_nodes: Option<BTreeSet<Address>>,
    pub blacklisted_block_proposers: Option<BTreeSet<Address>>,
    pub max_block_height: Option<u64>,
    pub max_validators: u64,
    pub xscore: XscoreConfig,
    pub stake_score: StakeScoreConfig,
    pub kin_score: KinScoreConfig,
    pub rewards: ValidatorRewards,
}

impl RuntimeConfigCache {
    pub async fn is_initialized() -> bool {
        CACHED_CONFIG.read().await.is_some()
    }

    pub async fn refresh(data: &Vec<u8>) -> Result<(), Error> {
        let toml_bytes = crate::decode_base64(data)?;
        let parsed_config = toml::from_str::<TomlConfig>(&String::from_utf8_lossy(&toml_bytes))
            .map_err(|e| anyhow!("Can't decode Runtime Config from toml: {e}"))?;

        if parsed_config.version_1.org_nodes.len() < 2 {
            return Err(anyhow!("Org_nodes length should be greater than or equal to 2"));
        }

        let mut config = CACHED_CONFIG.write().await;
        *config = Some(Arc::new(parsed_config.version_1.into()));

        Ok(())
    }

    pub async fn get() -> Result<Arc<RuntimeConfigCache>, Error> {
        let config = if let Ok(config) = CACHED_CONFIG.try_read() {
            config
        } else {
            CACHED_CONFIG.read().await
        };
        if let Some(config) = config.as_ref() {
            Ok(config.clone())
        } else {
            Err(anyhow!("Runtime Config is not initialized"))
        }
    }
}

impl From<ConfigVersion1> for RuntimeConfigCache {
    fn from(c: ConfigVersion1) -> Self {
        Self {
            org_nodes: BTreeSet::from_iter(c.org_nodes.iter().map(|v| v.0)),
            whitelisted_nodes: c
                .whitelisted_nodes
                .and_then(|v| Some(BTreeSet::from_iter(v.iter().map(|v| v.0)))),
            whitelisted_block_proposers: c
                .whitelisted_block_proposers
                .and_then(|v| Some(BTreeSet::from_iter(v.iter().map(|v| v.0)))),
            blacklisted_nodes: c
                .blacklisted_nodes
                .and_then(|v| Some(BTreeSet::from_iter(v.iter().map(|v| v.0)))),
            blacklisted_block_proposers: c
                .blacklisted_block_proposers
                .and_then(|v| Some(BTreeSet::from_iter(v.iter().map(|v| v.0)))),
            max_block_height: c.max_block_height,
            max_validators: c.max_validators,
            xscore: c.xscore.into(),
            stake_score: c.stake_score.into(),
            kin_score: c.kin_score.into(),
            rewards: c.rewards.into(),
        }
    }
}
