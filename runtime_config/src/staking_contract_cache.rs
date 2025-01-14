use std::{collections::HashMap, sync::Arc};

use anyhow::{anyhow, Error};
use l1x_rpc::rpc_model::NodePoolInfo as RpcNodePoolInfo;
use l1x_sdk::types::U128;
use once_cell::sync::Lazy;
use primitives::{Address, Balance, TimeStamp};
use serde::Deserialize;
use tokio::sync::RwLock;

static CACHED_STAKING_INFO: Lazy<RwLock<Option<Arc<RuntimeStakingInfoCache>>>> = Lazy::new(|| RwLock::new(None));

#[derive(Deserialize)]
pub struct NodePoolInfoOutput {
    node: l1x_sdk::types::Address,
    staked_balance: U128,
    locking_period: U128,
    reward_wallet_address: l1x_sdk::types::Address,
    min_pool_balance: U128,
    min_pool_balance_from: Option<U128>,
    max_pool_balance: U128,
    max_pool_balance_from: Option<U128>,
}

#[derive(Debug)]
pub struct NodePoolInfo {
    pub node: Address,
    pub staked_balance: Balance,
    pub locking_period: TimeStamp,
    pub reward_wallet_address: Address,
    pub min_pool_balance: Balance,
    pub min_pool_balance_from: Option<TimeStamp>,
    pub max_pool_balance: Balance,
    pub max_pool_balance_from: Option<TimeStamp>,
}

impl From<&NodePoolInfoOutput> for NodePoolInfo {
    fn from(v: &NodePoolInfoOutput) -> Self {
        Self {
            node: *v.node.as_bytes(),
            staked_balance: v.staked_balance.into(),
            locking_period: v.locking_period.into(),
            reward_wallet_address: *v.reward_wallet_address.as_bytes(),
            min_pool_balance: v.min_pool_balance.into(),
            min_pool_balance_from: v.min_pool_balance_from.and_then(|v| Some(v.into())),
            max_pool_balance: v.max_pool_balance.into(),
            max_pool_balance_from: v.max_pool_balance_from.and_then(|v| Some(v.into())),
        }
    }
}

impl From<&NodePoolInfo> for RpcNodePoolInfo {
    fn from(v: &NodePoolInfo) -> Self {
        Self {
            node: hex::encode(v.node),
            staked_balance: v.staked_balance.to_string(),
            locking_period: v.locking_period.to_string(),
            reward_wallet_address: hex::encode(v.reward_wallet_address),
            min_pool_balance: v.min_pool_balance.to_string(),
            min_pool_balance_from: v.min_pool_balance_from.and_then(|v| Some(v.to_string())),
            max_pool_balance: v.max_pool_balance.to_string(),
            max_pool_balance_from: v.max_pool_balance_from.and_then(|v| Some(v.to_string())),
        }
    }
}

pub struct RuntimeStakingInfoCache {
    pub nodes: HashMap<Address, NodePoolInfo>,
    pub reward_node_map: HashMap<Address, Address>,
}

impl RuntimeStakingInfoCache {
    pub async fn is_initialized() -> bool {
        CACHED_STAKING_INFO.read().await.is_some()
    }

    pub async fn refresh(data: &Vec<u8>) -> Result<(), Error> {
        let parsed_data: HashMap<l1x_sdk::types::Address, NodePoolInfoOutput> = serde_json::de::from_slice(data)?;

        let mut nodes = HashMap::new();
        let mut reward_node_map = HashMap::new();

        for (address, info) in &parsed_data {
            nodes.insert(*address.as_bytes(), info.into());
            reward_node_map.insert(*info.reward_wallet_address.as_bytes(), *address.as_bytes());
        }

        let new_cache = RuntimeStakingInfoCache { nodes, reward_node_map };

        let mut locked_cache = CACHED_STAKING_INFO.write().await;
        *locked_cache = Some(Arc::new(new_cache));

        Ok(())
    }

    pub async fn get() -> Result<Arc<RuntimeStakingInfoCache>, Error> {
        let config = if let Ok(config) = CACHED_STAKING_INFO.try_read() {
            config
        } else {
            CACHED_STAKING_INFO.read().await
        };
        if let Some(config) = config.as_ref() {
            Ok(config.clone())
        } else {
            Err(anyhow!("Runtime Staking is not initialized"))
        }
    }
}
