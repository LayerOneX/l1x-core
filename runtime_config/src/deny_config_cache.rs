use anyhow::{anyhow, Error};
use borsh::BorshDeserialize;
use once_cell::sync::Lazy;
use primitives::Address;
use std::{collections::HashSet, sync::Arc};
use tokio::sync::RwLock;

static CACHED_CONFIG: Lazy<RwLock<Option<Arc<RuntimeDenyConfigCache>>>> = Lazy::new(|| RwLock::new(None));

#[derive(Debug, BorshDeserialize)]
pub struct RoleBasedLists {
    pub receivers: HashSet<l1x_sdk::types::Address>,
    pub senders: HashSet<l1x_sdk::types::Address>,
}

#[derive(Debug, BorshDeserialize)]
pub struct RoleBasedListsOutput {
    deny_list: RoleBasedLists,
    allow_list: RoleBasedLists,
}

#[derive(Debug)]
pub struct WBAddresses {
    pub sender_addresses: HashSet<Address>,
    pub receiver_addresses: HashSet<Address>,
}

impl From<RoleBasedLists> for WBAddresses {
    fn from(value: RoleBasedLists) -> Self {
        Self {
            sender_addresses: HashSet::from_iter(value.senders.iter().map(|v| *v.as_bytes())),
            receiver_addresses: HashSet::from_iter(value.receivers.iter().map(|v| *v.as_bytes())),
        }
    }
}

#[derive(Debug)]
pub struct RuntimeDenyConfigCache {
    pub whitelisted_addresses: Option<WBAddresses>,
    pub blacklisted_addresses: Option<WBAddresses>,
}

impl RuntimeDenyConfigCache {
    pub async fn is_initialized() -> bool {
        CACHED_CONFIG.read().await.is_some()
    }

    pub async fn refresh(data: &Vec<u8>) -> Result<(), Error> {
        let borsh_bytes = crate::decode_base64(data)?;
        let sender_receiver_list: RoleBasedListsOutput = borsh::de::BorshDeserialize::try_from_slice(&borsh_bytes)?;

        let mut config = CACHED_CONFIG.write().await;
        *config = Some(Arc::new(sender_receiver_list.into()));

        Ok(())
    }

    pub async fn get() -> Result<Arc<RuntimeDenyConfigCache>, Error> {
        let config = if let Ok(config) = CACHED_CONFIG.try_read() {
            config
        } else {
            CACHED_CONFIG.read().await
        };
        if let Some(config) = config.as_ref() {
            Ok(config.clone())
        } else {
            Err(anyhow!("Runtime Deny Config is not initialized"))
        }
    }
}

impl From<RoleBasedListsOutput> for RuntimeDenyConfigCache {
    fn from(c: RoleBasedListsOutput) -> Self {
        let whitelisted_addresses = if c.allow_list.receivers.is_empty() && c.allow_list.receivers.is_empty() {
            None
        } else {
            Some(c.allow_list.into())
        };
        let blacklisted_addresses = if c.deny_list.receivers.is_empty() && c.deny_list.senders.is_empty() {
            None
        } else {
            Some(c.deny_list.into())
        };
        Self {
            whitelisted_addresses,
            blacklisted_addresses,
        }
    }
}
